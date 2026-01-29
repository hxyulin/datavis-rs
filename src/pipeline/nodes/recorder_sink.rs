//! RecorderSink node — records data to a session recording.
//!
//! Built from the existing `SessionRecorder`, but integrated as a pipeline node.

use crate::pipeline::node::NodeContext;
use crate::pipeline::packet::ConfigValue;
use crate::pipeline::port::{PortDescriptor, PortDirection, PortKind};
use crate::session::types::{
    RecordedFrame, RecordedValue, SessionMetadata, SessionRecording, SessionState,
};
use std::collections::HashMap;
use std::time::Duration;

static PORTS: &[PortDescriptor] = &[PortDescriptor {
    name: "in",
    direction: PortDirection::Input,
    kind: PortKind::DataStream,
}];

/// RecorderSink: records data frames to a session recording buffer.
pub struct RecorderSinkNode {
    state: SessionState,
    recording: SessionRecording,
    max_frames: usize,
    sample_interval: Duration,
    last_recorded: HashMap<u32, Duration>,
    pane_id: Option<u64>,
}

impl RecorderSinkNode {
    pub fn new() -> Self {
        Self {
            state: SessionState::Idle,
            recording: SessionRecording::new(),
            max_frames: 0,
            sample_interval: Duration::from_millis(10),
            last_recorded: HashMap::new(),
            pane_id: None,
        }
    }

    pub fn name(&self) -> &str {
        "RecorderSink"
    }

    pub fn ports(&self) -> &[PortDescriptor] {
        PORTS
    }

    pub fn pane_id(&self) -> Option<u64> {
        self.pane_id
    }

    pub fn on_activate(&mut self, _ctx: &mut NodeContext) {
        // Recording starts when explicitly armed via config
    }

    pub fn on_data(&mut self, ctx: &mut NodeContext) {
        if self.state != SessionState::Recording || ctx.input.is_empty() {
            return;
        }

        // Check max frames
        if self.max_frames > 0 && self.recording.frames.len() >= self.max_frames {
            return;
        }

        let current_time = ctx.timestamp;
        let mut frame_values = HashMap::new();
        let mut any_new = false;

        for sample in ctx.input.iter() {
            let var_id = sample.var_id.0;

            let should_record = self
                .last_recorded
                .get(&var_id)
                .map(|last| current_time.saturating_sub(*last) >= self.sample_interval)
                .unwrap_or(true);

            if !should_record {
                continue;
            }

            frame_values.insert(
                var_id,
                RecordedValue {
                    raw_value: sample.raw,
                    converted_value: sample.converted,
                },
            );
            self.last_recorded.insert(var_id, current_time);
            any_new = true;
        }

        if any_new {
            self.recording.frames.push(RecordedFrame {
                timestamp: current_time,
                values: frame_values,
            });
        }
    }

    pub fn on_deactivate(&mut self, _ctx: &mut NodeContext) {
        if self.state == SessionState::Recording {
            self.stop_recording();
        }
    }

    pub fn on_config_change(&mut self, key: &str, value: &ConfigValue, _ctx: &mut NodeContext) {
        match key {
            "arm" => {
                if value.as_bool() == Some(true) {
                    self.start_recording(SessionMetadata::new("Pipeline Recording"));
                }
            }
            "disarm" | "stop" => {
                self.stop_recording();
            }
            "sample_interval_ms" => {
                if let Some(ms) = value.as_int() {
                    self.sample_interval = Duration::from_millis(ms.max(1) as u64);
                }
            }
            "max_frames" => {
                if let Some(n) = value.as_int() {
                    self.max_frames = n.max(0) as usize;
                }
            }
            "pane_id" => {
                if let Some(id) = value.as_int() {
                    self.pane_id = Some(id as u64);
                }
            }
            _ => {}
        }
    }

    // ── Public API ──

    pub fn start_recording(&mut self, metadata: SessionMetadata) {
        self.recording = SessionRecording::with_metadata(metadata);
        self.last_recorded.clear();
        self.state = SessionState::Recording;
    }

    pub fn stop_recording(&mut self) {
        if self.state == SessionState::Recording {
            self.recording.finalize();
            self.state = SessionState::Stopped;
        }
    }

    pub fn take_recording(&mut self) -> SessionRecording {
        self.state = SessionState::Idle;
        self.last_recorded.clear();
        std::mem::take(&mut self.recording)
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn is_recording(&self) -> bool {
        self.state == SessionState::Recording
    }

    pub fn frame_count(&self) -> usize {
        self.recording.frames.len()
    }
}
