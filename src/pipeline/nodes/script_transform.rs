//! ScriptTransform node — applies Rhai converter scripts to raw values.
//!
//! Replaces the script execution logic in `BackendWorker::poll_variables()`.

use crate::pipeline::id::VarId;
use crate::pipeline::node::NodeContext;
use crate::pipeline::port::{PortDescriptor, PortDirection, PortKind};
use crate::scripting::{CompiledConverter, ExecutionContext, ScriptEngine};
use crate::types::Variable;
use std::collections::HashMap;

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

/// ScriptTransform: applies converter scripts to each sample.
pub struct ScriptTransformNode {
    engine: ScriptEngine,
    /// variable legacy id → compiled converter
    converters: HashMap<u32, Option<CompiledConverter>>,
    /// variable legacy id → (prev_raw, prev_converted, prev_time_secs)
    prev_values: HashMap<u32, (f64, f64, f64)>,
    /// VarId → legacy variable id (for looking up converters)
    var_id_to_legacy: HashMap<VarId, u32>,
}

impl ScriptTransformNode {
    pub fn new() -> Self {
        Self {
            engine: ScriptEngine::new(),
            converters: HashMap::new(),
            prev_values: HashMap::new(),
            var_id_to_legacy: HashMap::new(),
        }
    }

    pub fn name(&self) -> &str {
        "ScriptTransform"
    }

    pub fn ports(&self) -> &[PortDescriptor] {
        PORTS
    }

    pub fn on_activate(&mut self, _ctx: &mut NodeContext) {
        self.prev_values.clear();
    }

    pub fn on_data(&mut self, ctx: &mut NodeContext) {
        let time_secs = ctx.timestamp.as_secs_f64();
        ctx.output.timestamp = ctx.input.timestamp;

        for sample in ctx.input.iter() {
            let legacy_id = self.var_id_to_legacy.get(&sample.var_id);

            let converted = if let Some(&legacy_id) = legacy_id {
                if let Some(Some(converter)) = self.converters.get(&legacy_id) {
                    // Build execution context
                    let exec_ctx = if let Some(&(prev_raw, prev_converted, prev_time)) =
                        self.prev_values.get(&legacy_id)
                    {
                        let dt = time_secs - prev_time;
                        ExecutionContext::new(time_secs, dt, prev_raw, prev_converted)
                    } else {
                        ExecutionContext::first_sample(time_secs)
                    };

                    match self.engine.execute(converter, sample.raw, exec_ctx) {
                        Ok(v) => {
                            self.prev_values
                                .insert(legacy_id, (sample.raw, v, time_secs));
                            v
                        }
                        Err(e) => {
                            tracing::trace!("Converter error for var {}: {}", legacy_id, e);
                            // Store prev even on error
                            self.prev_values.insert(
                                legacy_id,
                                (sample.raw, sample.raw, time_secs),
                            );
                            sample.raw
                        }
                    }
                } else {
                    // No converter — pass through and track prev
                    self.prev_values
                        .insert(legacy_id, (sample.raw, sample.raw, time_secs));
                    sample.raw
                }
            } else {
                sample.raw
            };

            ctx.output.push_value(sample.var_id, sample.raw, converted);
        }

        // Pass through events
        for event in ctx.input_events {
            ctx.output_events.push(event.clone());
        }
    }

    pub fn on_deactivate(&mut self, _ctx: &mut NodeContext) {}

    pub fn on_config_change(
        &mut self,
        _key: &str,
        _value: &crate::pipeline::packet::ConfigValue,
        _ctx: &mut NodeContext,
    ) {
    }

    // ── Pipeline API ──

    pub fn add_variable(&mut self, var: &Variable) {
        let var_id = VarId(var.id);
        self.var_id_to_legacy.insert(var_id, var.id);

        let converter = if let Some(ref script) = var.converter_script {
            match self.engine.compile(&var.name, script) {
                Ok(c) => Some(c),
                Err(e) => {
                    tracing::warn!(
                        "Failed to compile converter for '{}': {}",
                        var.name,
                        e
                    );
                    None
                }
            }
        } else {
            None
        };

        self.converters.insert(var.id, converter);
    }

    pub fn remove_variable(&mut self, id: u32) {
        self.converters.remove(&id);
        self.prev_values.remove(&id);
        // Remove from var_id_to_legacy
        self.var_id_to_legacy.retain(|_, &mut v| v != id);
    }

    pub fn update_variable(&mut self, var: &Variable) {
        // Re-add: recompiles converter if changed
        self.add_variable(var);
    }
}
