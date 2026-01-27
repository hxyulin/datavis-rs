//! Session player for playing back recorded sessions

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::types::DataPoint;

use super::types::{SessionRecording, SessionState};

/// Session player for playing back recorded sessions
#[derive(Debug)]
pub struct SessionPlayer {
    /// Current playback state
    state: SessionState,
    /// The recording being played
    recording: Option<SessionRecording>,
    /// Current playback position (frame index)
    current_frame: usize,
    /// Current playback time
    current_time: Duration,
    /// Playback speed multiplier (1.0 = real-time, 2.0 = 2x speed, etc.)
    playback_speed: f64,
    /// When playback started (real time)
    playback_start: Option<Instant>,
    /// Playback position when started (to handle pause/resume)
    playback_offset: Duration,
    /// Whether to loop playback
    loop_playback: bool,
}

impl Default for SessionPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionPlayer {
    /// Create a new session player
    pub fn new() -> Self {
        Self {
            state: SessionState::Idle,
            recording: None,
            current_frame: 0,
            current_time: Duration::ZERO,
            playback_speed: 1.0,
            playback_start: None,
            playback_offset: Duration::ZERO,
            loop_playback: false,
        }
    }

    /// Get current state
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Check if playing
    pub fn is_playing(&self) -> bool {
        self.state.is_playing()
    }

    /// Check if paused
    pub fn is_paused(&self) -> bool {
        self.state.is_paused()
    }

    /// Check if a recording is loaded
    pub fn has_recording(&self) -> bool {
        self.recording.is_some()
    }

    /// Get the loaded recording
    pub fn recording(&self) -> Option<&SessionRecording> {
        self.recording.as_ref()
    }

    /// Get current playback time
    pub fn current_time(&self) -> Duration {
        self.current_time
    }

    /// Get current frame index
    pub fn current_frame(&self) -> usize {
        self.current_frame
    }

    /// Get playback speed
    pub fn playback_speed(&self) -> f64 {
        self.playback_speed
    }

    /// Set playback speed
    pub fn set_playback_speed(&mut self, speed: f64) {
        // Store current position before changing speed
        if self.is_playing() {
            self.update_playback_time();
            self.playback_offset = self.current_time;
            self.playback_start = Some(Instant::now());
        }
        self.playback_speed = speed.clamp(0.1, 10.0);
    }

    /// Get whether loop playback is enabled
    pub fn loop_playback(&self) -> bool {
        self.loop_playback
    }

    /// Set loop playback
    pub fn set_loop_playback(&mut self, loop_enabled: bool) {
        self.loop_playback = loop_enabled;
    }

    /// Get total duration
    pub fn total_duration(&self) -> Duration {
        self.recording
            .as_ref()
            .map(|r| r.duration())
            .unwrap_or(Duration::ZERO)
    }

    /// Get playback progress (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        let total = self.total_duration();
        if total.is_zero() {
            return 0.0;
        }
        self.current_time.as_secs_f64() / total.as_secs_f64()
    }

    /// Load a recording for playback
    pub fn load(&mut self, recording: SessionRecording) {
        self.recording = Some(recording);
        self.current_frame = 0;
        self.current_time = Duration::ZERO;
        self.playback_start = None;
        self.playback_offset = Duration::ZERO;
        self.state = SessionState::Stopped;
    }

    /// Unload the current recording
    pub fn unload(&mut self) {
        self.recording = None;
        self.current_frame = 0;
        self.current_time = Duration::ZERO;
        self.playback_start = None;
        self.playback_offset = Duration::ZERO;
        self.state = SessionState::Idle;
    }

    /// Start or resume playback
    pub fn play(&mut self) {
        if self.recording.is_none() {
            return;
        }

        match self.state {
            SessionState::Stopped | SessionState::Paused => {
                self.playback_start = Some(Instant::now());
                self.playback_offset = self.current_time;
                self.state = SessionState::Playing;
            }
            _ => {}
        }
    }

    /// Pause playback
    pub fn pause(&mut self) {
        if self.state == SessionState::Playing {
            self.update_playback_time();
            self.playback_offset = self.current_time;
            self.playback_start = None;
            self.state = SessionState::Paused;
        }
    }

    /// Stop playback and reset to beginning
    pub fn stop(&mut self) {
        self.current_frame = 0;
        self.current_time = Duration::ZERO;
        self.playback_start = None;
        self.playback_offset = Duration::ZERO;
        self.state = SessionState::Stopped;
    }

    /// Seek to a specific time
    pub fn seek(&mut self, time: Duration) {
        let Some(ref recording) = self.recording else {
            return;
        };

        // Clamp to recording duration
        let duration = recording.duration();
        let time = if time > duration { duration } else { time };

        self.current_time = time;
        self.playback_offset = time;

        // Find the frame at this time
        if let Some(idx) = recording.find_frame_at(time) {
            self.current_frame = idx;
        }

        // Reset playback start if playing
        if self.is_playing() {
            self.playback_start = Some(Instant::now());
        }
    }

    /// Seek by progress (0.0 to 1.0)
    pub fn seek_progress(&mut self, progress: f64) {
        let progress = progress.clamp(0.0, 1.0);
        let time = Duration::from_secs_f64(self.total_duration().as_secs_f64() * progress);
        self.seek(time);
    }

    /// Step forward by one frame
    pub fn step_forward(&mut self) {
        let Some(ref recording) = self.recording else {
            return;
        };

        if self.current_frame < recording.frames.len().saturating_sub(1) {
            self.current_frame += 1;
            self.current_time = recording.frames[self.current_frame].timestamp;
            self.playback_offset = self.current_time;
        }
    }

    /// Step backward by one frame
    pub fn step_backward(&mut self) {
        let Some(ref recording) = self.recording else {
            return;
        };

        if self.current_frame > 0 {
            self.current_frame -= 1;
            self.current_time = recording.frames[self.current_frame].timestamp;
            self.playback_offset = self.current_time;
        }
    }

    /// Update playback time based on real elapsed time
    fn update_playback_time(&mut self) {
        if let Some(start) = self.playback_start {
            let real_elapsed = start.elapsed();
            let playback_elapsed =
                Duration::from_secs_f64(real_elapsed.as_secs_f64() * self.playback_speed);
            self.current_time = self.playback_offset + playback_elapsed;
        }
    }

    /// Update playback state (call this each frame)
    /// Returns data points that should be displayed up to current time
    pub fn update(&mut self) -> HashMap<u32, Vec<DataPoint>> {
        let mut result = HashMap::new();

        if !self.is_playing() {
            return result;
        }

        if self.recording.is_none() {
            return result;
        }

        // Update current time
        self.update_playback_time();

        // Check if we've reached the end
        let duration = self.recording.as_ref().unwrap().duration();
        if self.current_time >= duration {
            if self.loop_playback {
                // Loop back to beginning
                self.current_time = Duration::ZERO;
                self.playback_offset = Duration::ZERO;
                self.playback_start = Some(Instant::now());
                self.current_frame = 0;
            } else {
                // Stop at end
                self.current_time = duration;
                self.state = SessionState::Stopped;
            }
        }

        // Find frames to return (from last frame to current time)
        let recording = self.recording.as_ref().unwrap();
        if let Some(end_idx) = recording.find_frame_at(self.current_time) {
            // Collect all frames from current to end_idx
            for idx in self.current_frame..=end_idx {
                if idx >= recording.frames.len() {
                    break;
                }

                let frame = &recording.frames[idx];
                for (var_id, value) in &frame.values {
                    let point = DataPoint {
                        timestamp: frame.timestamp,
                        raw_value: value.raw_value,
                        converted_value: value.converted_value,
                    };

                    result.entry(*var_id).or_insert_with(Vec::new).push(point);
                }
            }

            self.current_frame = end_idx;
        }

        result
    }

    /// Get all data points up to the current time for display
    pub fn get_data_until_now(&self) -> HashMap<u32, Vec<DataPoint>> {
        let mut result = HashMap::new();

        let Some(ref recording) = self.recording else {
            return result;
        };

        // Get all frames up to current time
        for frame in &recording.frames {
            if frame.timestamp > self.current_time {
                break;
            }

            for (var_id, value) in &frame.values {
                let point = DataPoint {
                    timestamp: frame.timestamp,
                    raw_value: value.raw_value,
                    converted_value: value.converted_value,
                };

                result.entry(*var_id).or_insert_with(Vec::new).push(point);
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::{RecordedFrame, SessionMetadata};

    fn create_test_recording() -> SessionRecording {
        let mut recording = SessionRecording::with_metadata(SessionMetadata::new("Test"));
        for i in 0..10 {
            recording.frames.push(RecordedFrame {
                timestamp: Duration::from_millis(i * 100),
                values: HashMap::new(),
            });
        }
        recording
    }

    #[test]
    fn test_player_lifecycle() {
        let mut player = SessionPlayer::new();
        assert_eq!(player.state(), SessionState::Idle);

        player.load(create_test_recording());
        assert_eq!(player.state(), SessionState::Stopped);
        assert!(player.has_recording());

        player.play();
        assert_eq!(player.state(), SessionState::Playing);

        player.pause();
        assert_eq!(player.state(), SessionState::Paused);

        player.stop();
        assert_eq!(player.state(), SessionState::Stopped);

        player.unload();
        assert_eq!(player.state(), SessionState::Idle);
        assert!(!player.has_recording());
    }

    #[test]
    fn test_seek() {
        let mut player = SessionPlayer::new();
        player.load(create_test_recording());

        player.seek(Duration::from_millis(500));
        assert_eq!(player.current_time(), Duration::from_millis(500));
        assert_eq!(player.current_frame(), 5);

        player.seek_progress(0.5);
        assert!(player.current_frame() <= 5);
    }

    #[test]
    fn test_step() {
        let mut player = SessionPlayer::new();
        player.load(create_test_recording());

        player.step_forward();
        assert_eq!(player.current_frame(), 1);

        player.step_forward();
        assert_eq!(player.current_frame(), 2);

        player.step_backward();
        assert_eq!(player.current_frame(), 1);
    }

    #[test]
    fn test_playback_speed() {
        let mut player = SessionPlayer::new();
        player.set_playback_speed(2.0);
        assert_eq!(player.playback_speed(), 2.0);

        // Test clamping
        player.set_playback_speed(100.0);
        assert_eq!(player.playback_speed(), 10.0);

        player.set_playback_speed(0.01);
        assert_eq!(player.playback_speed(), 0.1);
    }
}
