//! Thread boundary between the pipeline (backend) and the UI (frontend).
//!
//! `PipelineBridge` provides the same API surface as the old `FrontendReceiver`,
//! allowing the UI code to transition with minimal changes.

use crate::config::{MemoryAccessMode, ProbeConfig};
use crate::pipeline::id::{EdgeId, NodeId, VarId};
use crate::pipeline::packet::ConfigValue;
use crate::pipeline::port::PortDescriptor;
use crate::session::types::{SessionRecording, SessionState};
use crate::types::{CollectionStats, ConnectionStatus, Variable, VariableType};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::time::Duration;

/// Messages sent from pipeline sinks / executor to the UI thread.
#[derive(Debug, Clone)]
pub enum SinkMessage {
    /// Batch of data samples: (VarId, timestamp, raw, converted).
    DataBatch(Vec<(VarId, Duration, f64, f64)>),

    /// Periodic collection statistics.
    Stats(CollectionStats),

    /// Connection status changed.
    ConnectionStatus(ConnectionStatus),

    /// Connection error.
    ConnectionError(String),

    /// A node encountered an error.
    NodeError {
        node_id: NodeId,
        message: String,
    },

    /// A recording has been completed.
    RecordingComplete(SessionRecording),

    /// Variable list update (when variable tree changes).
    VariableList(Vec<Variable>),

    /// Probe list (response to RefreshProbes).
    ProbeList(Vec<crate::backend::DetectedProbe>),

    /// Write succeeded.
    WriteSuccess { variable_id: u32 },

    /// Write failed.
    WriteError { variable_id: u32, error: String },

    /// Read error on a variable.
    ReadError { variable_id: u32, error: String },

    /// Recorder status update (state, frame count).
    RecorderStatus {
        state: SessionState,
        frame_count: usize,
    },

    /// Exporter status update (active, rows written).
    ExporterStatus {
        active: bool,
        rows_written: u64,
    },

    /// Snapshot of the variable tree for UI display.
    VariableTreeSnapshot(Vec<VariableNodeSnapshot>),

    /// Pipeline topology snapshot for the pipeline editor.
    Topology(TopologySnapshot),

    /// Pipeline is shutting down.
    Shutdown,
}

/// Serializable snapshot of a single variable tree node.
#[derive(Debug, Clone)]
pub struct VariableNodeSnapshot {
    pub id: VarId,
    pub name: String,
    pub short_name: String,
    pub address: u64,
    pub var_type: VariableType,
    pub parent: VarId,
    pub first_child: VarId,
    pub next_sibling: VarId,
    pub depth: u16,
    pub is_leaf: bool,
    pub enabled: bool,
}

/// Snapshot of a single pipeline node.
#[derive(Debug, Clone)]
pub struct NodeSnapshot {
    pub id: NodeId,
    pub name: String,
    pub ports: Vec<PortDescriptor>,
}

/// Snapshot of a single pipeline edge.
#[derive(Debug, Clone)]
pub struct EdgeSnapshot {
    pub id: EdgeId,
    pub from_node: NodeId,
    pub to_node: NodeId,
}

/// Complete topology snapshot of the pipeline graph.
#[derive(Debug, Clone)]
pub struct TopologySnapshot {
    pub nodes: Vec<NodeSnapshot>,
    pub edges: Vec<EdgeSnapshot>,
}

/// Commands sent from the UI thread to the pipeline.
#[derive(Debug, Clone)]
pub enum PipelineCommand {
    /// Start the data collection pipeline.
    Start,
    /// Stop the data collection pipeline.
    Stop,
    /// Connect to a debug probe.
    Connect {
        selector: Option<String>,
        target: String,
        probe_config: ProbeConfig,
    },
    /// Disconnect from the probe.
    Disconnect,
    /// Add a variable to the pipeline's variable tree.
    AddVariable(Variable),
    /// Remove a variable by its legacy u32 ID.
    RemoveVariable(u32),
    /// Update a variable's configuration.
    UpdateVariable(Variable),
    /// Write a value to a target variable.
    WriteVariable { id: u32, value: f64 },
    /// Set the global poll rate.
    SetPollRate(u32),
    /// Set memory access mode.
    SetMemoryAccessMode(MemoryAccessMode),
    /// Clear all collected data.
    ClearData,
    /// Request current statistics.
    RequestStats,
    /// Send a config change to a specific node.
    NodeConfig {
        node_id: NodeId,
        key: String,
        value: ConfigValue,
    },
    /// Use mock probe (feature-gated).
    #[cfg(feature = "mock-probe")]
    UseMockProbe(bool),
    /// Refresh list of available probes.
    RefreshProbes,
    /// Request a variable tree snapshot to be sent back.
    RequestVariableTree,
    /// Request a pipeline topology snapshot.
    RequestTopology,
    /// Shut down the pipeline thread.
    Shutdown,
}

/// Channel capacity for commands (UI → pipeline).
const CMD_CHANNEL_CAPACITY: usize = 256;
/// Channel capacity for messages (pipeline → UI).
/// 10,000 messages ≈ 10s at 1kHz with batching.
const MSG_CHANNEL_CAPACITY: usize = 10_000;

/// UI-side handle for communicating with the pipeline thread.
///
/// Drop-in replacement for `FrontendReceiver` — same method names.
pub struct PipelineBridge {
    pub cmd_tx: Sender<PipelineCommand>,
    pub msg_rx: Receiver<SinkMessage>,
}

impl PipelineBridge {
    /// Create a new bridge pair: `(bridge_for_ui, cmd_rx, msg_tx)`.
    ///
    /// The pipeline thread owns `cmd_rx` and `msg_tx`.
    pub fn new() -> (Self, Receiver<PipelineCommand>, Sender<SinkMessage>) {
        let (cmd_tx, cmd_rx) = bounded(CMD_CHANNEL_CAPACITY);
        let (msg_tx, msg_rx) = bounded(MSG_CHANNEL_CAPACITY);
        (Self { cmd_tx, msg_rx }, cmd_rx, msg_tx)
    }

    // --- Drain messages (same as FrontendReceiver) ---

    /// Drain all pending messages.
    pub fn drain(&self) -> Vec<SinkMessage> {
        let mut msgs = Vec::new();
        while let Ok(msg) = self.msg_rx.try_recv() {
            msgs.push(msg);
        }
        msgs
    }

    /// Try to receive a single message without blocking.
    pub fn try_recv(&self) -> Option<SinkMessage> {
        self.msg_rx.try_recv().ok()
    }

    // --- Commands (same API surface as FrontendReceiver) ---

    pub fn send_command(&self, cmd: PipelineCommand) -> bool {
        self.cmd_tx.send(cmd).is_ok()
    }

    pub fn connect(&self, selector: Option<String>, target: String, probe_config: ProbeConfig) {
        let _ = self.cmd_tx.send(PipelineCommand::Connect {
            selector,
            target,
            probe_config,
        });
    }

    pub fn disconnect(&self) {
        let _ = self.cmd_tx.send(PipelineCommand::Disconnect);
    }

    pub fn start_collection(&self) {
        let _ = self.cmd_tx.send(PipelineCommand::Start);
    }

    pub fn stop_collection(&self) {
        let _ = self.cmd_tx.send(PipelineCommand::Stop);
    }

    pub fn add_variable(&self, variable: Variable) {
        let _ = self.cmd_tx.send(PipelineCommand::AddVariable(variable));
    }

    pub fn remove_variable(&self, id: u32) {
        let _ = self.cmd_tx.send(PipelineCommand::RemoveVariable(id));
    }

    pub fn update_variable(&self, variable: Variable) {
        let _ = self.cmd_tx.send(PipelineCommand::UpdateVariable(variable));
    }

    pub fn write_variable(&self, id: u32, value: f64) {
        let _ = self
            .cmd_tx
            .send(PipelineCommand::WriteVariable { id, value });
    }

    pub fn clear_data(&self) {
        let _ = self.cmd_tx.send(PipelineCommand::ClearData);
    }

    pub fn shutdown(&self) {
        let _ = self.cmd_tx.send(PipelineCommand::Shutdown);
    }

    #[cfg(feature = "mock-probe")]
    pub fn use_mock_probe(&self, use_mock: bool) {
        let _ = self.cmd_tx.send(PipelineCommand::UseMockProbe(use_mock));
    }
}
