//! FilterNode — variable filtering node.
//!
//! Filters data samples based on variable ID. When `allowed_vars` is empty,
//! all data passes through (passthrough mode). Otherwise, only samples
//! with var_id in `allowed_vars` pass through (or not in, if inverted).

use crate::pipeline::node::NodeContext;
use crate::pipeline::packet::ConfigValue;
use crate::pipeline::port::{PortDescriptor, PortDirection, PortKind};
use std::collections::HashSet;

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

/// Filter node — filters variables by ID.
///
/// By default passes through all data (passthrough mode).
/// Configure with allowed_vars to filter specific variables.
pub struct FilterNode {
    /// Variables allowed to pass through. Empty = passthrough all.
    allowed_vars: HashSet<u32>,
    /// Invert mode: if true, block listed vars instead of allowing.
    invert_mode: bool,
}

impl FilterNode {
    pub fn new() -> Self {
        Self {
            allowed_vars: HashSet::new(),
            invert_mode: false,
        }
    }

    pub fn name(&self) -> &str {
        "Filter"
    }

    pub fn ports(&self) -> &[PortDescriptor] {
        PORTS
    }

    pub fn on_activate(&mut self, _ctx: &mut NodeContext) {}

    pub fn on_data(&mut self, ctx: &mut NodeContext) {
        if self.allowed_vars.is_empty() {
            // Passthrough mode: copy all input samples to output
            ctx.output.copy_from(ctx.input);
        } else {
            // Filter mode: only pass samples matching the filter
            ctx.output.timestamp = ctx.input.timestamp;
            for sample in ctx.input.iter() {
                let var_id = sample.var_id.0;
                let is_in_set = self.allowed_vars.contains(&var_id);

                // Pass if: (in_set AND !invert) OR (!in_set AND invert)
                let should_pass = is_in_set != self.invert_mode;

                if should_pass {
                    ctx.output.push(sample.clone());
                }
            }
        }

        // Forward events
        for event in ctx.input_events {
            ctx.output_events.push(event.clone());
        }
    }

    pub fn on_deactivate(&mut self, _ctx: &mut NodeContext) {}

    pub fn on_config_change(&mut self, key: &str, value: &ConfigValue, _ctx: &mut NodeContext) {
        match key {
            "allowed_vars" => {
                // Format: comma-separated var IDs, e.g., "0,1,5,12"
                if let Some(s) = value.as_str() {
                    self.allowed_vars.clear();
                    if !s.is_empty() {
                        for part in s.split(',') {
                            if let Ok(id) = part.trim().parse::<u32>() {
                                self.allowed_vars.insert(id);
                            }
                        }
                    }
                }
            }
            "invert_mode" => {
                if let Some(b) = value.as_bool() {
                    self.invert_mode = b;
                }
            }
            "clear" => {
                // Clear the filter (return to passthrough mode)
                self.allowed_vars.clear();
                self.invert_mode = false;
            }
            _ => {}
        }
    }

    /// Get the current allowed variables set.
    pub fn allowed_vars(&self) -> &HashSet<u32> {
        &self.allowed_vars
    }

    /// Check if invert mode is enabled.
    pub fn invert_mode(&self) -> bool {
        self.invert_mode
    }

    /// Check if in passthrough mode (no filtering).
    pub fn is_passthrough(&self) -> bool {
        self.allowed_vars.is_empty()
    }
}

impl Default for FilterNode {
    fn default() -> Self {
        Self::new()
    }
}
