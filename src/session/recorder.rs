//! Session recorder for capturing data collection sessions

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::types::{DataPoint, Variable, VariableData};

use super::types::{RecordedFrame, RecordedValue, SessionMetadata, SessionRecording, SessionState};

/// Session recorder for capturing data collection sessions
#[derive(Debug)]
pub struct SessionRecorder {
    /// Current recording state
    state: SessionState,
    /// Start time of recording
    start_time: Option<Instant>,
    /// Current recording
    recording: SessionRecording,
    /// Maximum number of frames to record (0 = unlimited)
    max_frames: usize,
    /// Sample interval for recording (to avoid recording every single point)
    sample_interval: Duration,
    /// Last recorded time for each variable
    last_recorded: HashMap<u32, Duration>,
}

impl Default for SessionRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionRecorder {
    /// Create a new session recorder
    pub fn new() -> Self {
        Self {
            state: SessionState::Idle,
            start_time: None,
            recording: SessionRecording::new(),
            max_frames: 0,                              // Unlimited by default
            sample_interval: Duration::from_millis(10), // 100 Hz max recording rate
            last_recorded: HashMap::new(),
        }
    }

    /// Create with a specific sample interval
    pub fn with_sample_interval(sample_interval: Duration) -> Self {
        Self {
            sample_interval,
            ..Self::new()
        }
    }

    /// Set maximum number of frames to record
    pub fn set_max_frames(&mut self, max: usize) {
        self.max_frames = max;
    }

    /// Set sample interval
    pub fn set_sample_interval(&mut self, interval: Duration) {
        self.sample_interval = interval;
    }

    /// Get current state
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Check if recording
    pub fn is_recording(&self) -> bool {
        self.state.is_recording()
    }

    /// Get the current recording
    pub fn recording(&self) -> &SessionRecording {
        &self.recording
    }

    /// Take the recording (consumes it)
    pub fn take_recording(&mut self) -> SessionRecording {
        self.state = SessionState::Idle;
        self.start_time = None;
        self.last_recorded.clear();
        std::mem::take(&mut self.recording)
    }

    /// Start a new recording
    pub fn start_recording(&mut self, metadata: SessionMetadata) {
        self.recording = SessionRecording::with_metadata(metadata);
        self.start_time = Some(Instant::now());
        self.last_recorded.clear();
        self.state = SessionState::Recording;
    }

    /// Stop recording
    pub fn stop_recording(&mut self) {
        if self.state == SessionState::Recording {
            self.recording.finalize();
            self.state = SessionState::Stopped;
        }
    }

    /// Cancel recording (discard data)
    pub fn cancel_recording(&mut self) {
        self.recording = SessionRecording::new();
        self.start_time = None;
        self.last_recorded.clear();
        self.state = SessionState::Idle;
    }

    /// Record a frame of data from all variables
    pub fn record_frame(&mut self, variable_data: &HashMap<u32, VariableData>) {
        if !self.is_recording() {
            return;
        }

        let Some(start_time) = self.start_time else {
            return;
        };

        let current_time = start_time.elapsed();

        // Check max frames limit
        if self.max_frames > 0 && self.recording.frames.len() >= self.max_frames {
            return;
        }

        // Collect values that have been updated since last recording
        let mut frame_values = HashMap::new();
        let mut any_new_data = false;

        for (var_id, data) in variable_data {
            // Check sample interval
            let should_record = self
                .last_recorded
                .get(var_id)
                .map(|last| current_time - *last >= self.sample_interval)
                .unwrap_or(true);

            if !should_record {
                continue;
            }

            // Get the latest data point
            if let Some(last_point) = data.last() {
                frame_values.insert(
                    *var_id,
                    RecordedValue {
                        raw_value: last_point.raw_value,
                        converted_value: last_point.converted_value,
                    },
                );
                self.last_recorded.insert(*var_id, current_time);
                any_new_data = true;
            }
        }

        // Only add frame if we have new data
        if any_new_data {
            self.recording.frames.push(RecordedFrame {
                timestamp: current_time,
                values: frame_values,
            });
        }
    }

    /// Record data from a specific data point
    pub fn record_point(&mut self, var_id: u32, point: &DataPoint, elapsed: Duration) {
        if !self.is_recording() {
            return;
        }

        // Check max frames limit
        if self.max_frames > 0 && self.recording.frames.len() >= self.max_frames {
            return;
        }

        // Check sample interval
        let should_record = self
            .last_recorded
            .get(&var_id)
            .map(|last| elapsed - *last >= self.sample_interval)
            .unwrap_or(true);

        if !should_record {
            return;
        }

        // Find or create frame at this timestamp
        // For simplicity, we'll create a new frame for each point
        // In a more optimized version, we'd batch points at the same timestamp
        let mut values = HashMap::new();
        values.insert(var_id, RecordedValue::from(point));

        self.recording.frames.push(RecordedFrame {
            timestamp: elapsed,
            values,
        });

        self.last_recorded.insert(var_id, elapsed);
    }

    /// Get recording duration
    pub fn recording_duration(&self) -> Duration {
        self.start_time
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO)
    }

    /// Get number of recorded frames
    pub fn frame_count(&self) -> usize {
        self.recording.frames.len()
    }

    /// Initialize recording variables from config
    pub fn set_variables(&mut self, variables: &[Variable]) {
        self.recording.metadata.variables = variables.to_vec();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recorder_lifecycle() {
        let mut recorder = SessionRecorder::new();
        assert_eq!(recorder.state(), SessionState::Idle);

        recorder.start_recording(SessionMetadata::new("Test"));
        assert_eq!(recorder.state(), SessionState::Recording);
        assert!(recorder.is_recording());

        recorder.stop_recording();
        assert_eq!(recorder.state(), SessionState::Stopped);
        assert!(!recorder.is_recording());
    }

    #[test]
    fn test_take_recording() {
        let mut recorder = SessionRecorder::new();
        recorder.start_recording(SessionMetadata::new("Test"));
        recorder.stop_recording();

        let recording = recorder.take_recording();
        assert_eq!(recording.metadata.name, "Test");
        assert_eq!(recorder.state(), SessionState::Idle);
    }

    #[test]
    fn test_cancel_recording() {
        let mut recorder = SessionRecorder::new();
        recorder.start_recording(SessionMetadata::new("Test"));
        recorder.cancel_recording();

        assert_eq!(recorder.state(), SessionState::Idle);
        assert!(recorder.recording().is_empty());
    }
}
