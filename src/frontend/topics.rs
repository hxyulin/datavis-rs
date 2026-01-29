//! Shared topic data published by the backend and consumed by panes.
//!
//! The `Topics` struct is a plain data bus — direct field access, zero overhead.
//! The app writes to it from `process_backend_messages()`.
//! Panes read from it via `shared.topics`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::backend::DetectedProbe;
use crate::pipeline::bridge::VariableNodeSnapshot;
use crate::session::types::{SessionRecording, SessionState};
use crate::types::{CollectionStats, ConnectionStatus, VariableData};

/// All shared data published by the backend and consumed by panes.
///
/// This is a plain struct — direct field access, zero overhead.
/// No `HashMap` lookup, no `TypeId` hashing, no `Box<dyn Any>` downcasting.
pub struct Topics {
    // --- Live data (high frequency) ---
    /// Collected variable time-series data, keyed by variable ID.
    /// This is the primary data sink for all visualizer panes.
    pub variable_data: HashMap<u32, VariableData>,

    /// Per-graph-pane data storage. Keyed by pane ID, then variable ID.
    /// Used when GraphSink nodes route data to specific panes.
    pub graph_pane_data: HashMap<u64, HashMap<u32, VariableData>>,

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

    // --- Staleness tracking (for warning when sinks disconnect) ---
    /// Track when each pane last received data (keyed by pane ID)
    pub pane_data_freshness: HashMap<u64, Instant>,

    /// Track when global data was last updated
    pub global_data_freshness: Option<Instant>,

    /// Staleness threshold (default 3 seconds)
    pub staleness_threshold: Duration,
}

impl Default for Topics {
    fn default() -> Self {
        Self {
            variable_data: HashMap::new(),
            graph_pane_data: HashMap::new(),
            stats: CollectionStats::default(),
            connection_status: ConnectionStatus::Disconnected,
            recorder_state: SessionState::Idle,
            recorder_frame_count: 0,
            exporter_active: false,
            exporter_rows_written: 0,
            available_probes: Vec::new(),
            completed_recordings: Vec::new(),
            variable_tree: Vec::new(),
            project_name: String::new(),
            project_file_path: None,
            elf_generation: 0,
            pane_data_freshness: HashMap::new(),
            global_data_freshness: None,
            staleness_threshold: Duration::from_secs(3),
        }
    }
}
