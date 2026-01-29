//! GraphSink node — sends data batches to a specific graph pane.
//!
//! Each GraphSink can be associated with a specific pane via its pane_id.
//! This allows multiple graph panes to have independent data streams.

use crate::pipeline::bridge::SinkMessage;
use crate::pipeline::node::NodeContext;
use crate::pipeline::packet::{ConfigValue, PipelineEvent};
use crate::pipeline::port::{PortDescriptor, PortDirection, PortKind};
use crossbeam_channel::Sender;

static PORTS: &[PortDescriptor] = &[PortDescriptor {
    name: "in",
    direction: PortDirection::Input,
    kind: PortKind::DataStream,
}];

/// GraphSink: sends data to a specific graph pane for visualization.
pub struct GraphSinkNode {
    tx: Sender<SinkMessage>,
    /// Which pane this sink belongs to. None means broadcast to all.
    pane_id: Option<u64>,
    dropped: u64,
    active: bool,
}

impl GraphSinkNode {
    pub fn new(tx: Sender<SinkMessage>, pane_id: Option<u64>) -> Self {
        Self {
            tx,
            pane_id,
            dropped: 0,
            active: true,
        }
    }

    pub fn name(&self) -> &str {
        "GraphSink"
    }

    pub fn ports(&self) -> &[PortDescriptor] {
        PORTS
    }

    pub fn on_activate(&mut self, _ctx: &mut NodeContext) {
        self.dropped = 0;
        self.active = true;
    }

    pub fn on_data(&mut self, ctx: &mut NodeContext) {
        if !self.active || ctx.input.is_empty() {
            return;
        }

        let batch: Vec<_> = ctx
            .input
            .iter()
            .map(|s| (s.var_id, ctx.input.timestamp, s.raw, s.converted))
            .collect();

        let msg = SinkMessage::GraphDataBatch {
            pane_id: self.pane_id,
            data: batch,
        };

        if self.tx.try_send(msg).is_err() {
            self.dropped += 1;
        }

        // Forward error events
        for event in ctx.input_events {
            match event {
                PipelineEvent::VariableError { var_id, message } => {
                    let _ = self.tx.try_send(SinkMessage::ReadError {
                        variable_id: var_id.0,
                        error: message.clone(),
                    });
                }
                _ => {}
            }
        }
    }

    pub fn on_deactivate(&mut self, _ctx: &mut NodeContext) {
        if self.dropped > 0 {
            tracing::warn!(
                "GraphSink (pane {:?}) dropped {} messages due to backpressure",
                self.pane_id,
                self.dropped
            );
        }
    }

    pub fn on_config_change(&mut self, key: &str, value: &ConfigValue, _ctx: &mut NodeContext) {
        match key {
            "pane_id" => {
                if let Some(id) = value.as_int() {
                    self.pane_id = Some(id as u64);
                } else if value.as_str() == Some("none") {
                    self.pane_id = None;
                }
            }
            "active" => {
                if let Some(active) = value.as_bool() {
                    self.active = active;
                }
            }
            _ => {}
        }
    }

    /// Get the pane ID this sink is associated with.
    pub fn pane_id(&self) -> Option<u64> {
        self.pane_id
    }
}
