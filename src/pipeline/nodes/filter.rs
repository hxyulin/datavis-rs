//! FilterNode — lowpass, threshold, rate-limit filters.
//!
//! Currently a passthrough; filter parameters can be configured at runtime.

use crate::pipeline::node::NodeContext;
use crate::pipeline::packet::ConfigValue;
use crate::pipeline::port::{PortDescriptor, PortDirection, PortKind};

static PORTS: &[PortDescriptor] = &[
    PortDescriptor {
        name: "in",
        direction: PortDirection::Input,
        kind: PortKind::DataStream,
    },
    PortDescriptor {
        name: "out",
        direction: PortDirection::Output,
        kind: PortKind::DataStream,
    },
];

/// Filter node — passthrough by default, configurable at runtime.
pub struct FilterNode {
    // Future: filter type, cutoff, state per variable
}

impl FilterNode {
    pub fn new() -> Self {
        Self {}
    }

    pub fn name(&self) -> &str {
        "Filter"
    }

    pub fn ports(&self) -> &[PortDescriptor] {
        PORTS
    }

    pub fn on_activate(&mut self, _ctx: &mut NodeContext) {}

    pub fn on_data(&mut self, ctx: &mut NodeContext) {
        // Passthrough: copy all input samples to output
        ctx.output.copy_from(ctx.input);

        for event in ctx.input_events {
            ctx.output_events.push(event.clone());
        }
    }

    pub fn on_deactivate(&mut self, _ctx: &mut NodeContext) {}

    pub fn on_config_change(&mut self, _key: &str, _value: &ConfigValue, _ctx: &mut NodeContext) {}
}
