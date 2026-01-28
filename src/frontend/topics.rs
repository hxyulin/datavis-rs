//! Shared topic data published by the backend and consumed by panes.
//!
//! The `Topics` struct is a plain data bus — direct field access, zero overhead.
//! The app writes to it from `process_backend_messages()`.
//! Panes read from it via `shared.topics`.

use std::collections::HashMap;

use crate::backend::DetectedProbe;
use crate::pipeline::bridge::{TopologySnapshot, VariableNodeSnapshot};
use crate::pipeline::id::NodeId;
use crate::session::types::{SessionRecording, SessionState};
use crate::types::{CollectionStats, ConnectionStatus, VariableData};

/// All shared data published by the backend and consumed by panes.
///
/// This is a plain struct — direct field access, zero overhead.
/// No `HashMap` lookup, no `TypeId` hashing, no `Box<dyn Any>` downcasting.
#[derive(Default)]
pub struct Topics {
    // --- Pipeline infrastructure ---
    /// Node ID for recorder sink (set once at startup)
    pub recorder_node_id: NodeId,
    /// Node ID for exporter sink (set once at startup)
    pub exporter_node_id: NodeId,

    // --- Live data (high frequency) ---
    /// Collected variable time-series data, keyed by variable ID.
    /// This is the primary data sink for all visualizer panes.
    pub variable_data: HashMap<u32, VariableData>,

    /// Collection statistics (updated ~2Hz from pipeline)
    pub stats: CollectionStats,

    /// Current probe connection status
    pub connection_status: ConnectionStatus,

    // --- Status (low frequency) ---
    /// Recorder state
    pub recorder_state: SessionState,
    /// Recorder frame count
    pub recorder_frame_count: usize,

    /// Whether exporter is active
    pub exporter_active: bool,
    /// Rows written by exporter
    pub exporter_rows_written: u64,

    // --- Snapshots (on-demand / event-driven) ---
    /// Pipeline topology for the editor pane
    pub topology: Option<TopologySnapshot>,

    /// Available debug probes (from RefreshProbes)
    pub available_probes: Vec<DetectedProbe>,

    /// Completed session recordings
    pub completed_recordings: Vec<SessionRecording>,

    /// Variable tree snapshot (hierarchical structure from pipeline)
    pub variable_tree: Vec<VariableNodeSnapshot>,

    // --- Project metadata (shared between Settings pane and app save/load) ---
    /// Project name
    pub project_name: String,
    /// Path to the current project file
    pub project_file_path: Option<std::path::PathBuf>,

    /// ELF reload generation — incremented when ELF is loaded.
    /// Panes compare against their last-seen value to react.
    pub elf_generation: u64,
}
