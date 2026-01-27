//! Backend module for SWD polling with probe-rs
//!
//! This module handles all SWD communication in a separate thread to keep
//! the UI responsive. It uses crossbeam channels for thread-safe communication
//! with the frontend.
//!
//! # Architecture
//!
//! The backend runs in a separate thread from the UI, communicating via channels:
//!
//! - [`BackendCommand`] - Messages sent from UI to backend (connect, read, write, etc.)
//! - [`BackendMessage`] - Messages sent from backend to UI (data, status, errors)
//! - [`FrontendReceiver`] - UI-side handle for sending commands and receiving messages
//! - [`SwdBackend`] - Main backend entry point that spawns the worker thread
//!
//! # Components
//!
//! - [`ProbeBackend`] - Low-level probe-rs interface for real hardware
//! - [`MockProbeBackend`] - Mock probe for testing without hardware (feature-gated)
//! - [`BackendWorker`] - Main worker loop that processes commands and polls variables
//! - [`ElfParser`] / [`DwarfParser`] - Parse ELF/DWARF debug info for symbol discovery
//! - [`TypeTable`] - Manages type information from debug symbols
//!
//! # Example
//!
//! ```ignore
//! use datavis_rs::backend::SwdBackend;
//! use datavis_rs::config::AppConfig;
//!
//! let config = AppConfig::default();
//! let (backend, frontend) = SwdBackend::new(config);
//!
//! // Spawn backend thread
//! std::thread::spawn(move || backend.run());
//!
//! // Send commands from UI
//! frontend.connect(None, "STM32F407VGTx".to_string(), config.probe);
//! frontend.start_collection();
//!
//! // Receive messages
//! for msg in frontend.drain() {
//!     match msg {
//!         BackendMessage::DataPoint { variable_id, timestamp, raw_value, converted_value } => {
//!             // Handle new data
//!         }
//!         _ => {}
//!     }
//! }
//! ```

pub mod dwarf_parser;
pub mod elf_parser;
#[cfg(feature = "mock-probe")]
pub mod mock_probe;
pub mod probe;
pub mod probe_trait;
pub mod read_manager;
pub mod type_table;
pub mod worker;

use crate::config::ProbeConfig;

pub use dwarf_parser::{DwarfDiagnostics, DwarfParseResult, DwarfParser, ParsedSymbol, VariableStatus};
pub use elf_parser::{demangle_symbol, ElfInfo, ElfParser, SymbolInfo, SymbolType};
pub use type_table::{
    BaseClassDef, DwarfTypeKey, EnumDef, EnumVariant as TypeTableEnumVariant, ForwardDeclKind,
    GlobalTypeKey, MemberDef, PrimitiveDef, SharedTypeTable, StructDef, TemplateParam, TypeDef,
    TypeHandle, TypeId, TypeTable, TypeTableStats,
};

#[cfg(feature = "mock-probe")]
pub use mock_probe::{MockDataPattern, MockProbeBackend, MockProbeInfo, MockVariableConfig};
pub use probe::{ProbeBackend, ProbeInfo};
pub use probe_trait::{DebugProbe, DetectedProbeInfo, ProbeStats};
pub use read_manager::{ReadManager, ReadRegion, DEFAULT_GAP_THRESHOLD};
pub use worker::{BackendWorker, SwdCommand, SwdResponse};

use crate::config::AppConfig;
use crate::types::{CollectionStats, ConnectionStatus, Variable};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

/// Message sent from the UI to the backend
#[derive(Debug, Clone)]
pub enum BackendCommand {
    /// Connect to a probe
    Connect {
        /// Optional probe selector
        selector: Option<String>,
        /// Target chip name
        target: String,
        /// Full probe configuration (includes connect_under_reset, halt_on_connect, etc.)
        probe_config: ProbeConfig,
    },
    /// Disconnect from the current probe
    Disconnect,
    /// Start data collection
    StartCollection,
    /// Stop data collection
    StopCollection,
    /// Add a variable to observe
    AddVariable(Variable),
    /// Remove a variable by ID
    RemoveVariable(u32),
    /// Update a variable's configuration
    UpdateVariable(Variable),
    /// Write a value to a variable
    WriteVariable {
        /// Variable ID
        id: u32,
        /// Value to write (will be converted to the variable's type)
        value: f64,
    },
    /// Clear all collected data
    ClearData,
    /// Set the polling rate in Hz
    SetPollRate(u32),
    /// Set memory access mode (Background, Halted, HaltedPersistent)
    SetMemoryAccessMode(crate::config::MemoryAccessMode),
    /// Request current statistics
    RequestStats,
    /// Shutdown the backend
    Shutdown,
    /// Use mock probe instead of real hardware (only available with mock-probe feature)
    #[cfg(feature = "mock-probe")]
    UseMockProbe(bool),
    /// Request probe list refresh (async)
    RefreshProbes,
}

/// Represents a detected probe (real or mock)
#[derive(Debug, Clone)]
pub enum DetectedProbe {
    /// Real hardware probe
    Real(ProbeInfo),
    /// Mock probe for testing (only available with mock-probe feature)
    #[cfg(feature = "mock-probe")]
    Mock(MockProbeInfo),
}

impl DetectedProbe {
    /// Get the display name of the probe
    pub fn display_name(&self) -> String {
        match self {
            DetectedProbe::Real(info) => info.to_string(),
            #[cfg(feature = "mock-probe")]
            DetectedProbe::Mock(info) => format!("{} (Mock)", info.name),
        }
    }

    /// Check if this is a mock probe
    pub fn is_mock(&self) -> bool {
        #[cfg(feature = "mock-probe")]
        {
            matches!(self, DetectedProbe::Mock(_))
        }
        #[cfg(not(feature = "mock-probe"))]
        {
            false
        }
    }
}

/// Parsed symbol from an ELF/AXF file (re-export for backwards compatibility)
pub type ElfSymbol = SymbolInfo;

/// Parse ELF symbols from a file
pub fn parse_elf_symbols(path: &std::path::Path) -> Result<Vec<ElfSymbol>, String> {
    match ElfParser::parse(path) {
        Ok(info) => Ok(info.get_variables().into_iter().cloned().collect()),
        Err(e) => {
            tracing::warn!("ELF parsing failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Parse ELF file and return full info
pub fn parse_elf(path: &std::path::Path) -> Result<ElfInfo, String> {
    ElfParser::parse(path).map_err(|e| e.to_string())
}

/// List all available probes (real + mock if feature enabled)
/// This should be called from a background thread, not the UI thread.
pub fn list_all_probes() -> Vec<DetectedProbe> {
    let mut probes = Vec::new();

    // Add real probes
    for info in ProbeBackend::list_probes() {
        probes.push(DetectedProbe::Real(info));
    }

    // Add mock probes (only when feature is enabled)
    #[cfg(feature = "mock-probe")]
    {
        for info in mock_probe::list_mock_probes() {
            probes.push(DetectedProbe::Mock(info));
        }
    }

    probes
}

/// List probes asynchronously by sending result through a channel
/// This is safe to call from any thread
pub fn list_probes_async(sender: Sender<BackendMessage>) {
    std::thread::spawn(move || {
        let probes = list_all_probes();
        let _ = sender.send(BackendMessage::ProbeList(probes));
    });
}

/// Message sent from the backend to the UI
#[derive(Debug, Clone)]
pub enum BackendMessage {
    /// Connection status changed
    ConnectionStatus(ConnectionStatus),
    /// Connection error occurred
    ConnectionError(String),
    /// New data point received
    DataPoint {
        variable_id: u32,
        timestamp: Duration,
        raw_value: f64,
        converted_value: f64,
    },
    /// Batch of data points (more efficient for high-frequency updates)
    DataBatch(Vec<(u32, Duration, f64, f64)>),
    /// Variable read error
    ReadError { variable_id: u32, error: String },
    /// Variable write succeeded
    WriteSuccess { variable_id: u32 },
    /// Variable write error
    WriteError { variable_id: u32, error: String },
    /// Statistics update
    Stats(CollectionStats),
    /// Variable list update
    VariableList(Vec<Variable>),
    /// Probe list update (response to RefreshProbes)
    ProbeList(Vec<DetectedProbe>),
    /// Backend is shutting down
    Shutdown,
}

/// Frontend receiver for backend messages
pub struct FrontendReceiver {
    /// Receiver for backend messages
    pub receiver: Receiver<BackendMessage>,
    /// Sender for commands to the backend
    pub command_sender: Sender<BackendCommand>,
}

impl FrontendReceiver {
    /// Try to receive a message without blocking
    pub fn try_recv(&self) -> Option<BackendMessage> {
        self.receiver.try_recv().ok()
    }

    /// Receive all pending messages
    pub fn drain(&self) -> Vec<BackendMessage> {
        let mut messages = Vec::new();
        while let Ok(msg) = self.receiver.try_recv() {
            messages.push(msg);
        }
        messages
    }

    /// Send a command to the backend
    pub fn send_command(&self, cmd: BackendCommand) -> bool {
        self.command_sender.send(cmd).is_ok()
    }

    /// Request connection to a probe
    pub fn connect(&self, selector: Option<String>, target: String, probe_config: ProbeConfig) {
        let _ = self.command_sender.send(BackendCommand::Connect {
            selector,
            target,
            probe_config,
        });
    }

    /// Request disconnection
    pub fn disconnect(&self) {
        let _ = self.command_sender.send(BackendCommand::Disconnect);
    }

    /// Start data collection
    pub fn start_collection(&self) {
        let _ = self.command_sender.send(BackendCommand::StartCollection);
    }

    /// Stop data collection
    pub fn stop_collection(&self) {
        let _ = self.command_sender.send(BackendCommand::StopCollection);
    }

    /// Add a variable to observe
    pub fn add_variable(&self, variable: Variable) {
        let _ = self
            .command_sender
            .send(BackendCommand::AddVariable(variable));
    }

    /// Remove a variable
    pub fn remove_variable(&self, id: u32) {
        let _ = self.command_sender.send(BackendCommand::RemoveVariable(id));
    }

    /// Update a variable
    pub fn update_variable(&self, variable: Variable) {
        let _ = self
            .command_sender
            .send(BackendCommand::UpdateVariable(variable));
    }

    /// Write a value to a variable
    pub fn write_variable(&self, id: u32, value: f64) {
        let _ = self
            .command_sender
            .send(BackendCommand::WriteVariable { id, value });
    }

    /// Clear collected data
    pub fn clear_data(&self) {
        let _ = self.command_sender.send(BackendCommand::ClearData);
    }

    /// Set whether to use mock probe (only available with mock-probe feature)
    #[cfg(feature = "mock-probe")]
    pub fn use_mock_probe(&self, use_mock: bool) {
        let _ = self
            .command_sender
            .send(BackendCommand::UseMockProbe(use_mock));
    }

    /// Request shutdown
    pub fn shutdown(&self) {
        let _ = self.command_sender.send(BackendCommand::Shutdown);
    }
}

/// The SWD backend that runs in a separate thread
pub struct SwdBackend {
    /// Configuration
    config: AppConfig,
    /// Receiver for commands from the UI
    command_receiver: Receiver<BackendCommand>,
    /// Sender for messages to the UI
    message_sender: Sender<BackendMessage>,
    /// Running flag
    running: Arc<AtomicBool>,
}

impl SwdBackend {
    /// Create a new SWD backend with communication channels
    pub fn new(config: AppConfig) -> (Self, FrontendReceiver) {
        let (cmd_tx, cmd_rx) = bounded(256);
        // Use bounded channel for backpressure - prevents memory spikes if UI can't keep up
        // 10,000 messages is enough for ~10 seconds at 1000 Hz with batching
        let (msg_tx, msg_rx) = bounded(10_000);

        let backend = Self {
            config,
            command_receiver: cmd_rx,
            message_sender: msg_tx,
            running: Arc::new(AtomicBool::new(true)),
        };

        let frontend = FrontendReceiver {
            receiver: msg_rx,
            command_sender: cmd_tx,
        };

        (backend, frontend)
    }

    /// Run the backend loop
    pub fn run(self) {
        let mut worker = BackendWorker::new(
            self.config,
            self.command_receiver,
            self.message_sender,
            self.running,
        );
        worker.run();
    }

    /// Get a handle to stop the backend
    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        self.running.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::VariableType;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_backend_creation() {
        let config = AppConfig::default();
        let (backend, frontend) = SwdBackend::new(config);

        // Backend should be running
        assert!(backend.running.load(Ordering::SeqCst));

        // Should be able to send commands
        assert!(frontend.send_command(BackendCommand::Shutdown));
    }

    #[test]
    fn test_frontend_receiver_commands() {
        let config = AppConfig::default();
        let (_backend, frontend) = SwdBackend::new(config.clone());

        // Test various commands
        frontend.connect(None, "STM32F407VGTx".to_string(), config.probe.clone());
        frontend.start_collection();

        let var = Variable::new("test", 0x2000_0000, VariableType::U32);
        frontend.add_variable(var);

        frontend.stop_collection();
        frontend.disconnect();
        frontend.shutdown();
    }

    #[test]
    #[ignore = "USB enumeration can hang on some systems (especially macOS)"]
    fn test_list_all_probes_does_not_panic() {
        // This should not panic even if probe-rs fails
        let probes = list_all_probes();
        // Just verify it returns a vector (may be empty if no probes or errors)
        let _ = probes.len();
    }
}
