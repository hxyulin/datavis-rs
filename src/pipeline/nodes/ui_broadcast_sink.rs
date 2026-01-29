//! UIBroadcastSink node — broadcasts data batches to all UI panes via crossbeam channel.
//!
//! Publishes data samples to the frontend for visualization across all graph panes and UI components.

use crate::pipeline::bridge::SinkMessage;
use crate::pipeline::node::NodeContext;
use crate::pipeline::packet::PipelineEvent;
use crate::pipeline::port::{PortDescriptor, PortDirection, PortKind};
use crossbeam_channel::Sender;

static PORTS: &[PortDescriptor] = &[PortDescriptor {
    name: "in",
    direction: PortDirection::Input,
    kind: PortKind::DataStream,
}];

/// UIBroadcastSink: converts data packets to SinkMessage::DataBatch and broadcasts to all UI.
pub struct UIBroadcastSinkNode {
    tx: Sender<SinkMessage>,
    dropped: u64,
}

impl UIBroadcastSinkNode {
    pub fn new(tx: Sender<SinkMessage>) -> Self {
        Self { tx, dropped: 0 }
    }

    pub fn name(&self) -> &str {
        "UIBroadcastSink"
    }

    pub fn ports(&self) -> &[PortDescriptor] {
        PORTS
    }

    pub fn on_activate(&mut self, _ctx: &mut NodeContext) {
        self.dropped = 0;
    }

    pub fn on_data(&mut self, ctx: &mut NodeContext) {
        if ctx.input.is_empty() {
            return;
        }

        let batch: Vec<_> = ctx
            .input
            .iter()
            .map(|s| (s.var_id, ctx.input.timestamp, s.raw, s.converted))
            .collect();

        if self.tx.try_send(SinkMessage::DataBatch(batch)).is_err() {
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
            tracing::warn!("UIBroadcastSink dropped {} messages due to backpressure", self.dropped);
        }
    }

    pub fn on_config_change(
        &mut self,
        _key: &str,
        _value: &crate::pipeline::packet::ConfigValue,
        _ctx: &mut NodeContext,
    ) {
    }
}
