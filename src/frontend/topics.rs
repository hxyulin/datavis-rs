//! Shared topic data published by the backend and consumed by panes.
//!
//! The `Topics` struct is a plain data bus — direct field access, zero overhead.
//! The app writes to it from `process_backend_messages()`.
//! Panes read from it via `shared.topics`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::backend::DetectedProbe;
use crate::session::types::{SessionRecording, SessionState};
use crate::types::{CollectionStats, ConnectionStatus, PointerState, VariableData};
use crate::watch::{WatchId, WatchValue};

/// All shared data published by the backend and consumed by panes.
///
/// This is a plain struct — direct field access, zero overhead.
/// No `HashMap` lookup, no `TypeId` hashing, no `Box<dyn Any>` downcasting.
pub struct Topics {
    // --- Live data (high frequency) ---
    /// Collected variable time-series data, keyed by variable ID.
    /// This is the primary data sink for all visualizer panes.
    pub variable_data: HashMap<u32, VariableData>,

    /// Collection statistics (updated ~2Hz from backend)
    pub stats: CollectionStats,

    /// Current probe connection status
    pub connection_status: ConnectionStatus,

    // --- Status (low frequency) ---
    /// Recorder state
    pub recorder_state: SessionState,
    /// Recorder frame count
    pub recorder_frame_count: usize,

    // --- Snapshots (on-demand / event-driven) ---
    /// Available debug probes (from RefreshProbes)
    pub available_probes: Vec<DetectedProbe>,

    /// Completed session recordings
    pub completed_recordings: Vec<SessionRecording>,

    // --- Project metadata (shared between Settings pane and app save/load) ---
    /// Project name
    pub project_name: String,
    /// Path to the current project file
    pub project_file_path: Option<std::path::PathBuf>,

    /// ELF reload generation — incremented when ELF is loaded.
    /// Panes compare against their last-seen value to react.
    pub elf_generation: u64,

    /// Pointer states for UI display (populated by backend worker)
    pub pointer_states: HashMap<u32, PointerState>,

    /// Live Watch values, keyed by (watch root id, leaf path).
    /// Published by the watch scheduler on each tick; consumed by the LiveWatch pane.
    pub watch_values: HashMap<(WatchId, String), WatchValue>,

    /// Track when the last stats update was received from the backend.
    /// Used to detect poll-thread stalls (e.g., probe read hanging).
    pub last_stats_update: Option<Instant>,

    /// Staleness threshold (default 3 seconds)
    pub staleness_threshold: Duration,
}

impl Default for Topics {
    fn default() -> Self {
        Self {
            variable_data: HashMap::new(),
            stats: CollectionStats::default(),
            connection_status: ConnectionStatus::Disconnected,
            recorder_state: SessionState::Idle,
            recorder_frame_count: 0,
            available_probes: Vec::new(),
            completed_recordings: Vec::new(),
            project_name: String::new(),
            project_file_path: None,
            elf_generation: 0,
            pointer_states: HashMap::new(),
            watch_values: HashMap::new(),
            last_stats_update: None,
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
        let topics = Topics::default();

        // Just test the HashMap operations, not VariableData construction
        assert_eq!(topics.variable_data.len(), 0);
        assert!(!topics.variable_data.contains_key(&1));

        // We can't easily create a VariableData without a Variable,
        // so we just test the HashMap structure
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
    #[allow(clippy::field_reassign_with_default)]
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
