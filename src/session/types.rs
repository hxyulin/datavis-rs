//! Session data types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

use crate::types::{DataPoint, Variable};

/// State of session recording/playback
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionState {
    /// No active session
    #[default]
    Idle,
    /// Currently recording a session
    Recording,
    /// Session recorded, ready for playback
    Stopped,
    /// Playing back a recorded session
    Playing,
    /// Playback paused
    Paused,
}

impl SessionState {
    /// Check if currently recording
    pub fn is_recording(&self) -> bool {
        matches!(self, SessionState::Recording)
    }

    /// Check if currently playing
    pub fn is_playing(&self) -> bool {
        matches!(self, SessionState::Playing)
    }

    /// Check if paused
    pub fn is_paused(&self) -> bool {
        matches!(self, SessionState::Paused)
    }

    /// Check if has recorded data
    pub fn has_recording(&self) -> bool {
        !matches!(self, SessionState::Idle)
    }

    /// Display name for the state
    pub fn display_name(&self) -> &'static str {
        match self {
            SessionState::Idle => "Idle",
            SessionState::Recording => "Recording",
            SessionState::Stopped => "Stopped",
            SessionState::Playing => "Playing",
            SessionState::Paused => "Paused",
        }
    }
}

/// Metadata for a recorded session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Name/title of the session
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// When the session was recorded
    pub recorded_at: chrono::DateTime<chrono::Utc>,
    /// Total duration of the session
    pub duration: Duration,
    /// Poll rate used during recording (Hz)
    pub poll_rate_hz: u32,
    /// Target chip/device name
    pub target_name: Option<String>,
    /// ELF file path (if loaded)
    pub elf_path: Option<String>,
    /// Number of data points recorded
    pub total_data_points: usize,
    /// Variables that were recorded
    pub variables: Vec<Variable>,
}

impl Default for SessionMetadata {
    fn default() -> Self {
        Self {
            name: String::from("Untitled Session"),
            description: None,
            recorded_at: chrono::Utc::now(),
            duration: Duration::ZERO,
            poll_rate_hz: 100,
            target_name: None,
            elf_path: None,
            total_data_points: 0,
            variables: Vec::new(),
        }
    }
}

impl SessionMetadata {
    /// Create new metadata with a name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// A single recorded data frame containing all variable values at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedFrame {
    /// Time offset from start of recording
    pub timestamp: Duration,
    /// Variable ID to data point mapping
    pub values: HashMap<u32, RecordedValue>,
}

/// A recorded value for a single variable at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedValue {
    /// Raw value read from memory
    pub raw_value: f64,
    /// Converted value (after script processing)
    pub converted_value: f64,
}

impl From<&DataPoint> for RecordedValue {
    fn from(dp: &DataPoint) -> Self {
        Self {
            raw_value: dp.raw_value,
            converted_value: dp.converted_value,
        }
    }
}

/// A complete recorded session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecording {
    /// Session metadata
    pub metadata: SessionMetadata,
    /// Recorded data frames (sorted by timestamp)
    pub frames: Vec<RecordedFrame>,
}

impl Default for SessionRecording {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionRecording {
    /// Create a new empty recording
    pub fn new() -> Self {
        Self {
            metadata: SessionMetadata::default(),
            frames: Vec::new(),
        }
    }

    /// Create with metadata
    pub fn with_metadata(metadata: SessionMetadata) -> Self {
        Self {
            metadata,
            frames: Vec::new(),
        }
    }

    /// Get the total duration of the recording
    pub fn duration(&self) -> Duration {
        self.frames
            .last()
            .map(|f| f.timestamp)
            .unwrap_or(Duration::ZERO)
    }

    /// Get the number of frames
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Check if the recording is empty
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Get data points for a variable within a time range
    pub fn get_variable_data(
        &self,
        var_id: u32,
        start: Duration,
        end: Duration,
    ) -> Vec<DataPoint> {
        self.frames
            .iter()
            .filter(|f| f.timestamp >= start && f.timestamp <= end)
            .filter_map(|f| {
                f.values.get(&var_id).map(|v| DataPoint {
                    timestamp: f.timestamp,
                    raw_value: v.raw_value,
                    converted_value: v.converted_value,
                })
            })
            .collect()
    }

    /// Find the frame index at or before a given time
    pub fn find_frame_at(&self, time: Duration) -> Option<usize> {
        if self.frames.is_empty() {
            return None;
        }

        // Binary search for the frame
        let idx = self.frames.partition_point(|f| f.timestamp <= time);
        if idx == 0 {
            Some(0)
        } else {
            Some(idx - 1)
        }
    }

    /// Save recording to a file (JSON format)
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Load recording from a file
    pub fn load_from_file(path: &std::path::Path) -> Result<Self, std::io::Error> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Finalize the recording by updating metadata
    pub fn finalize(&mut self) {
        self.metadata.duration = self.duration();
        self.metadata.total_data_points = self.frames.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_state() {
        assert!(SessionState::Recording.is_recording());
        assert!(SessionState::Playing.is_playing());
        assert!(SessionState::Paused.is_paused());
        assert!(SessionState::Stopped.has_recording());
        assert!(!SessionState::Idle.has_recording());
    }

    #[test]
    fn test_session_recording() {
        let mut recording = SessionRecording::new();
        assert!(recording.is_empty());

        recording.frames.push(RecordedFrame {
            timestamp: Duration::from_millis(100),
            values: HashMap::new(),
        });

        assert!(!recording.is_empty());
        assert_eq!(recording.frame_count(), 1);
        assert_eq!(recording.duration(), Duration::from_millis(100));
    }

    #[test]
    fn test_find_frame_at() {
        let mut recording = SessionRecording::new();
        for i in 0..10 {
            recording.frames.push(RecordedFrame {
                timestamp: Duration::from_millis(i * 100),
                values: HashMap::new(),
            });
        }

        assert_eq!(recording.find_frame_at(Duration::from_millis(0)), Some(0));
        assert_eq!(recording.find_frame_at(Duration::from_millis(150)), Some(1));
        assert_eq!(recording.find_frame_at(Duration::from_millis(500)), Some(5));
        assert_eq!(recording.find_frame_at(Duration::from_millis(1000)), Some(9));
    }
}
