//! Thread boundary between the pipeline (backend) and the UI (frontend).
//!
//! `PipelineBridge` provides the same API surface as the old `FrontendReceiver`,
//! allowing the UI code to transition with minimal changes.

use crate::config::{MemoryAccessMode, ProbeConfig};
use crate::pipeline::id::{NodeId, VarId};
use crate::pipeline::packet::ConfigValue;
use crate::session::types::{SessionRecording, SessionState};
use crate::types::{CollectionStats, ConnectionStatus, Variable, VariableType};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::time::Duration;

/// Messages sent from pipeline sinks / executor to the UI thread.
#[derive(Debug, Clone)]
pub enum SinkMessage {
    /// Batch of data samples: (VarId, timestamp, raw, converted).
    DataBatch(Vec<(VarId, Duration, f64, f64)>),

    /// Batch of data samples for a specific graph pane.
    GraphDataBatch {
        /// Which pane this data is for. None means broadcast to all.
        pane_id: Option<u64>,
        /// Data samples: (VarId, timestamp, raw, converted).
        data: Vec<(VarId, Duration, f64, f64)>,
    },

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
/// Now wraps the new backend FrontendReceiver and converts message types.
pub struct PipelineBridge {
    frontend_receiver: Option<crate::backend::FrontendReceiver>,
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
        (Self {
            frontend_receiver: None,
            cmd_tx,
            msg_rx
        }, cmd_rx, msg_tx)
    }

    /// Create a PipelineBridge from a FrontendReceiver (new backend)
    ///
    /// This wraps the new backend's FrontendReceiver and converts between
    /// old message types (SinkMessage/PipelineCommand) and new types
    /// (BackendMessage/BackendCommand) for backwards compatibility.
    pub fn from_frontend_receiver(receiver: crate::backend::FrontendReceiver) -> Self {
        let (cmd_tx, _cmd_rx) = bounded(CMD_CHANNEL_CAPACITY);
        let (_msg_tx, msg_rx) = bounded(MSG_CHANNEL_CAPACITY);
        Self {
            frontend_receiver: Some(receiver),
            cmd_tx,
            msg_rx,
        }
    }

    // --- Drain messages (same as FrontendReceiver) ---

    /// Drain all pending messages.
    pub fn drain(&self) -> Vec<SinkMessage> {
        // If using new backend, drain from frontend_receiver and convert
        if let Some(ref receiver) = self.frontend_receiver {
            return receiver.drain().into_iter()
                .filter_map(|msg| Self::convert_backend_message(msg))
                .collect();
        }

        // Otherwise use old channel
        let mut msgs = Vec::new();
        while let Ok(msg) = self.msg_rx.try_recv() {
            msgs.push(msg);
        }
        msgs
    }

    /// Try to receive a single message without blocking.
    pub fn try_recv(&self) -> Option<SinkMessage> {
        // If using new backend, try_recv from frontend_receiver and convert
        if let Some(ref receiver) = self.frontend_receiver {
            return receiver.try_recv().and_then(|msg| Self::convert_backend_message(msg));
        }

        // Otherwise use old channel
        self.msg_rx.try_recv().ok()
    }

    /// Convert BackendMessage to SinkMessage
    fn convert_backend_message(msg: crate::backend::BackendMessage) -> Option<SinkMessage> {
        use crate::backend::BackendMessage;
        use crate::pipeline::id::VarId;

        match msg {
            BackendMessage::DataBatch(data) => {
                // Convert u32 IDs to VarId
                let converted: Vec<_> = data.into_iter()
                    .map(|(id, ts, raw, conv)| (VarId(id), ts, raw, conv))
                    .collect();
                Some(SinkMessage::DataBatch(converted))
            }
            BackendMessage::DataUpdate(update) => {
                // Convert DataUpdate to DataBatch for now
                let converted: Vec<_> = update.global.into_iter()
                    .map(|(id, ts, raw, conv)| (VarId(id), ts, raw, conv))
                    .collect();
                Some(SinkMessage::DataBatch(converted))
            }
            BackendMessage::Stats(stats) => {
                Some(SinkMessage::Stats(stats))
            }
            BackendMessage::ConnectionStatus(status) => {
                Some(SinkMessage::ConnectionStatus(status))
            }
            BackendMessage::ConnectionError(error) => {
                Some(SinkMessage::ConnectionError(error))
            }
            BackendMessage::DataPoint { variable_id, timestamp, raw_value, converted_value } => {
                Some(SinkMessage::DataBatch(vec![(VarId(variable_id), timestamp, raw_value, converted_value)]))
            }
            BackendMessage::ReadError { variable_id, error } => {
                Some(SinkMessage::ReadError { variable_id, error })
            }
            BackendMessage::WriteSuccess { variable_id } => {
                Some(SinkMessage::WriteSuccess { variable_id })
            }
            BackendMessage::WriteError { variable_id, error } => {
                Some(SinkMessage::WriteError { variable_id, error })
            }
            BackendMessage::VariableList(vars) => {
                Some(SinkMessage::VariableList(vars))
            }
            BackendMessage::ProbeList(probes) => {
                Some(SinkMessage::ProbeList(probes))
            }
            BackendMessage::Shutdown => None,
        }
    }

    // --- Commands (same API surface as FrontendReceiver) ---

    /// Convert PipelineCommand to BackendCommand
    fn convert_pipeline_command(cmd: PipelineCommand) -> crate::backend::BackendCommand {
        use crate::backend::BackendCommand;

        match cmd {
            PipelineCommand::Connect { selector, target, probe_config } => {
                BackendCommand::Connect { selector, target, probe_config }
            }
            PipelineCommand::Disconnect => BackendCommand::Disconnect,
            PipelineCommand::Start => BackendCommand::StartCollection,
            PipelineCommand::Stop => BackendCommand::StopCollection,
            PipelineCommand::AddVariable(var) => BackendCommand::AddVariable(var),
            PipelineCommand::RemoveVariable(id) => BackendCommand::RemoveVariable(id),
            PipelineCommand::UpdateVariable(var) => BackendCommand::UpdateVariable(var),
            PipelineCommand::WriteVariable { id, value } => {
                BackendCommand::WriteVariable { id, value }
            }
            PipelineCommand::ClearData => BackendCommand::ClearData,
            PipelineCommand::SetPollRate(rate) => BackendCommand::SetPollRate(rate),
            PipelineCommand::SetMemoryAccessMode(mode) => BackendCommand::SetMemoryAccessMode(mode),
            PipelineCommand::Shutdown => BackendCommand::Shutdown,
            #[cfg(feature = "mock-probe")]
            PipelineCommand::UseMockProbe(use_mock) => BackendCommand::UseMockProbe(use_mock),
            PipelineCommand::RefreshProbes => BackendCommand::RefreshProbes,
            _ => BackendCommand::Shutdown, // Fallback for unhandled commands
        }
    }

    pub fn send_command(&self, cmd: PipelineCommand) -> bool {
        if let Some(ref receiver) = self.frontend_receiver {
            return receiver.send_command(Self::convert_pipeline_command(cmd));
        }
        self.cmd_tx.send(cmd).is_ok()
    }

    pub fn connect(&self, selector: Option<String>, target: String, probe_config: ProbeConfig) {
        if let Some(ref receiver) = self.frontend_receiver {
            receiver.connect(selector, target, probe_config);
            return;
        }
        let _ = self.cmd_tx.send(PipelineCommand::Connect {
            selector,
            target,
            probe_config,
        });
    }

    pub fn disconnect(&self) {
        if let Some(ref receiver) = self.frontend_receiver {
            receiver.disconnect();
            return;
        }
        let _ = self.cmd_tx.send(PipelineCommand::Disconnect);
    }

    pub fn start_collection(&self) {
        if let Some(ref receiver) = self.frontend_receiver {
            receiver.start_collection();
            return;
        }
        let _ = self.cmd_tx.send(PipelineCommand::Start);
    }

    pub fn stop_collection(&self) {
        if let Some(ref receiver) = self.frontend_receiver {
            receiver.stop_collection();
            return;
        }
        let _ = self.cmd_tx.send(PipelineCommand::Stop);
    }

    pub fn add_variable(&self, variable: Variable) {
        if let Some(ref receiver) = self.frontend_receiver {
            receiver.add_variable(variable);
            return;
        }
        let _ = self.cmd_tx.send(PipelineCommand::AddVariable(variable));
    }

    pub fn remove_variable(&self, id: u32) {
        if let Some(ref receiver) = self.frontend_receiver {
            receiver.remove_variable(id);
            return;
        }
        let _ = self.cmd_tx.send(PipelineCommand::RemoveVariable(id));
    }

    pub fn update_variable(&self, variable: Variable) {
        if let Some(ref receiver) = self.frontend_receiver {
            receiver.update_variable(variable);
            return;
        }
        let _ = self.cmd_tx.send(PipelineCommand::UpdateVariable(variable));
    }

    pub fn write_variable(&self, id: u32, value: f64) {
        if let Some(ref receiver) = self.frontend_receiver {
            receiver.write_variable(id, value);
            return;
        }
        let _ = self
            .cmd_tx
            .send(PipelineCommand::WriteVariable { id, value });
    }

    pub fn clear_data(&self) {
        if let Some(ref receiver) = self.frontend_receiver {
            receiver.clear_data();
            return;
        }
        let _ = self.cmd_tx.send(PipelineCommand::ClearData);
    }

    pub fn shutdown(&self) {
        if let Some(ref receiver) = self.frontend_receiver {
            receiver.shutdown();
            return;
        }
        let _ = self.cmd_tx.send(PipelineCommand::Shutdown);
    }

    #[cfg(feature = "mock-probe")]
    pub fn use_mock_probe(&self, use_mock: bool) {
        if let Some(ref receiver) = self.frontend_receiver {
            receiver.use_mock_probe(use_mock);
            return;
        }
        let _ = self.cmd_tx.send(PipelineCommand::UseMockProbe(use_mock));
    }
}
