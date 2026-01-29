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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_topics_default() {
        let topics = Topics::default();

        assert!(topics.variable_data.is_empty());
        assert!(topics.graph_pane_data.is_empty());
        assert_eq!(topics.connection_status, ConnectionStatus::Disconnected);
        assert!(topics.available_probes.is_empty());
        assert!(topics.completed_recordings.is_empty());
        assert_eq!(topics.project_name, "");
        assert_eq!(topics.project_file_path, None);
        assert_eq!(topics.elf_generation, 0);
        assert_eq!(topics.staleness_threshold, Duration::from_secs(3));
    }

    #[test]
    fn test_variable_data_insertion() {
        let mut topics = Topics::default();

        // Just test the HashMap operations, not VariableData construction
        assert_eq!(topics.variable_data.len(), 0);
        assert!(!topics.variable_data.contains_key(&1));

        // We can't easily create a VariableData without a Variable,
        // so we just test the HashMap structure
    }

    #[test]
    fn test_graph_pane_data_insertion() {
        let mut topics = Topics::default();

        let pane_id = 100u64;

        // Test nested HashMap structure
        let pane_data = topics.graph_pane_data.entry(pane_id).or_default();
        assert_eq!(pane_data.len(), 0);

        assert!(topics.graph_pane_data.contains_key(&pane_id));
    }

    #[test]
    fn test_pane_data_freshness_tracking() {
        let mut topics = Topics::default();

        let pane_id = 100u64;
        let now = Instant::now();

        topics.pane_data_freshness.insert(pane_id, now);

        assert!(topics.pane_data_freshness.contains_key(&pane_id));
        assert_eq!(topics.pane_data_freshness[&pane_id], now);
    }

    #[test]
    fn test_global_data_freshness() {
        let mut topics = Topics::default();

        assert!(topics.global_data_freshness.is_none());

        let now = Instant::now();
        topics.global_data_freshness = Some(now);

        assert!(topics.global_data_freshness.is_some());
        assert_eq!(topics.global_data_freshness.unwrap(), now);
    }

    #[test]
    fn test_elf_generation_increment() {
        let mut topics = Topics::default();

        assert_eq!(topics.elf_generation, 0);

        topics.elf_generation += 1;
        assert_eq!(topics.elf_generation, 1);

        topics.elf_generation += 1;
        assert_eq!(topics.elf_generation, 2);
    }

    #[test]
    fn test_connection_status_transition() {
        let mut topics = Topics::default();

        assert_eq!(topics.connection_status, ConnectionStatus::Disconnected);

        topics.connection_status = ConnectionStatus::Connecting;
        assert_eq!(topics.connection_status, ConnectionStatus::Connecting);

        topics.connection_status = ConnectionStatus::Connected;
        assert_eq!(topics.connection_status, ConnectionStatus::Connected);
    }

    #[test]
    fn test_available_probes_management() {
        let topics = Topics::default();

        assert!(topics.available_probes.is_empty());

        // Would need actual DetectedProbe instances to test fully
        // but we can test the vector operations
        assert_eq!(topics.available_probes.len(), 0);
    }

    #[test]
    fn test_recorder_state_tracking() {
        let mut topics = Topics::default();

        assert_eq!(topics.recorder_state, SessionState::Idle);
        assert_eq!(topics.recorder_frame_count, 0);

        topics.recorder_state = SessionState::Recording;
        topics.recorder_frame_count = 100;

        assert_eq!(topics.recorder_state, SessionState::Recording);
        assert_eq!(topics.recorder_frame_count, 100);
    }

    #[test]
    fn test_exporter_tracking() {
        let mut topics = Topics::default();

        assert!(!topics.exporter_active);
        assert_eq!(topics.exporter_rows_written, 0);

        topics.exporter_active = true;
        topics.exporter_rows_written = 1000;

        assert!(topics.exporter_active);
        assert_eq!(topics.exporter_rows_written, 1000);
    }

    #[test]
    fn test_project_metadata() {
        let mut topics = Topics::default();

        topics.project_name = "TestProject".to_string();
        topics.project_file_path = Some(PathBuf::from("/path/to/project.json"));

        assert_eq!(topics.project_name, "TestProject");
        assert_eq!(
            topics.project_file_path,
            Some(PathBuf::from("/path/to/project.json"))
        );
    }

    #[test]
    fn test_staleness_threshold_customization() {
        let mut topics = Topics::default();

        assert_eq!(topics.staleness_threshold, Duration::from_secs(3));

        topics.staleness_threshold = Duration::from_secs(5);
        assert_eq!(topics.staleness_threshold, Duration::from_secs(5));
    }
}
