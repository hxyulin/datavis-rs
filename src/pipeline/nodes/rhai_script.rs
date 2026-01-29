//! RhaiScript node — executes user-provided Rhai scripts on entire data streams.
//!
//! Unlike ScriptTransformNode which applies per-variable converters, RhaiScriptNode
//! runs a single script that can process all samples in a packet, allowing for
//! cross-variable operations, filtering, and custom transformations.
//!
//! ## Script Interface
//!
//! The script receives:
//! - `samples` - Array of sample objects with `.var_id`, `.raw`, `.converted`
//! - `time` - Current timestamp in seconds
//! - `dt` - Delta time since last tick
//! - `tick` - Tick counter
//!
//! ## Example Scripts
//!
//! Passthrough (no modification):
//! ```rhai
//! samples
//! ```
//!
//! Scale all values (use index-based iteration to modify in place):
//! ```rhai
//! let len = samples.len();
//! for i in 0..len {
//!     samples[i].converted = samples[i].converted * 2.0;
//! }
//! samples
//! ```
//!
//! Apply lowpass filter to all samples:
//! ```rhai
//! let len = samples.len();
//! for i in 0..len {
//!     samples[i].converted = lowpass(samples[i].converted, 10.0);
//! }
//! samples
//! ```

use crate::pipeline::node::NodeContext;
use crate::pipeline::packet::ConfigValue;
use crate::pipeline::port::{PortDescriptor, PortDirection, PortKind};
use crate::scripting::ScriptEngine;
use rhai::{Array, Dynamic, Scope, AST};

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

/// A pipeline node that executes user-provided Rhai scripts on entire data packets.
pub struct RhaiScriptNode {
    /// Human-readable name for this node instance.
    name: String,
    /// The user's Rhai script source code.
    script_source: String,
    /// Compiled AST (cached after successful compilation).
    compiled: Option<AST>,
    /// Rhai engine instance with registered functions.
    engine: ScriptEngine,
    /// Last error message (if script failed to compile/run).
    last_error: Option<String>,
}

impl RhaiScriptNode {
    /// Create a new RhaiScriptNode with default passthrough behavior.
    pub fn new() -> Self {
        Self {
            name: "Rhai Script".to_string(),
            script_source: String::new(),
            compiled: None,
            engine: ScriptEngine::new(),
            last_error: None,
        }
    }

    /// Create a new RhaiScriptNode with a specific name.
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::new()
        }
    }

    /// Set the script source and compile it.
    pub fn set_script(&mut self, source: &str) {
        self.script_source = source.to_string();
        self.compile_script();
    }

    /// Get the current script source.
    pub fn script_source(&self) -> &str {
        &self.script_source
    }

    /// Get the last error message, if any.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Check if the script is valid and compiled.
    pub fn is_valid(&self) -> bool {
        self.compiled.is_some() || self.script_source.is_empty()
    }

    fn compile_script(&mut self) {
        if self.script_source.is_empty() {
            self.compiled = None;
            self.last_error = None;
            return;
        }

        match self.engine.engine().compile(&self.script_source) {
            Ok(ast) => {
                self.compiled = Some(ast);
                self.last_error = None;
                tracing::debug!("RhaiScriptNode: Script compiled successfully");
            }
            Err(e) => {
                self.compiled = None;
                self.last_error = Some(format!("Compile error: {}", e));
                tracing::warn!("RhaiScriptNode: {}", self.last_error.as_ref().unwrap());
            }
        }
    }

    fn execute_script(
        &self,
        samples_array: Array,
        time_secs: f64,
        dt_secs: f64,
        tick: u64,
    ) -> Result<Array, String> {
        let ast = self
            .compiled
            .as_ref()
            .ok_or_else(|| "No compiled script".to_string())?;

        let mut scope = Scope::new();
        scope.push("samples", samples_array);
        scope.push("time", time_secs);
        scope.push("dt", dt_secs);
        scope.push("tick", tick as i64);

        self.engine
            .engine()
            .eval_ast_with_scope::<Dynamic>(&mut scope, ast)
            .map_err(|e| format!("Execution error: {}", e))
            .and_then(|result| {
                result
                    .try_cast::<Array>()
                    .ok_or_else(|| "Script must return an array of samples".to_string())
            })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ports(&self) -> &[PortDescriptor] {
        PORTS
    }

    pub fn on_activate(&mut self, _ctx: &mut NodeContext) {
        // Re-compile on activation in case something changed
        if !self.script_source.is_empty() {
            self.compile_script();
        }
    }

    pub fn on_data(&mut self, ctx: &mut NodeContext) {
        // If no script or compilation error, pass through unchanged
        if self.script_source.is_empty() || self.compiled.is_none() {
            ctx.output.copy_from(ctx.input);
            return;
        }

        let time_secs = ctx.timestamp.as_secs_f64();
        let dt_secs = ctx.dt.as_secs_f64();

        // Convert input samples to Rhai array
        let samples_array = self.input_to_rhai_array(ctx);

        // Execute script
        match self.execute_script(samples_array, time_secs, dt_secs, ctx.tick) {
            Ok(result_array) => {
                self.rhai_array_to_output(result_array, ctx);
                self.last_error = None;
            }
            Err(e) => {
                // On error, pass through and log
                ctx.output.copy_from(ctx.input);
                self.last_error = Some(e.clone());
                tracing::trace!("RhaiScriptNode execution error: {}", e);
            }
        }

        // Always propagate timestamp
        ctx.output.timestamp = ctx.input.timestamp;

        // Pass through events
        for event in ctx.input_events {
            ctx.output_events.push(event.clone());
        }
    }

    pub fn on_deactivate(&mut self, _ctx: &mut NodeContext) {}

    pub fn on_config_change(&mut self, key: &str, value: &ConfigValue, _ctx: &mut NodeContext) {
        match key {
            "script" => {
                if let Some(source) = value.as_str() {
                    self.set_script(source);
                }
            }
            "name" => {
                if let Some(name) = value.as_str() {
                    self.name = name.to_string();
                }
            }
            _ => {}
        }
    }

    /// Convert input packet samples to a Rhai array of dynamic maps.
    fn input_to_rhai_array(&self, ctx: &NodeContext) -> Array {
        ctx.input
            .iter()
            .map(|sample| {
                let mut map = rhai::Map::new();
                map.insert("var_id".into(), Dynamic::from(sample.var_id.0 as i64));
                map.insert("raw".into(), Dynamic::from(sample.raw));
                map.insert("converted".into(), Dynamic::from(sample.converted));
                Dynamic::from(map)
            })
            .collect()
    }

    /// Convert Rhai result array back to output packet.
    fn rhai_array_to_output(&self, result_array: Array, ctx: &mut NodeContext) {
        use crate::pipeline::id::VarId;

        for item in result_array {
            if let Some(map) = item.try_cast::<rhai::Map>() {
                let var_id = map
                    .get("var_id")
                    .and_then(|v| v.as_int().ok())
                    .unwrap_or(0) as u32;
                let raw = map.get("raw").and_then(|v| v.as_float().ok()).unwrap_or(0.0);
                let converted = map
                    .get("converted")
                    .and_then(|v| v.as_float().ok())
                    .unwrap_or(raw);

                ctx.output.push_value(VarId(var_id), raw, converted);
            }
        }
    }
}

impl Default for RhaiScriptNode {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for RhaiScriptNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RhaiScriptNode")
            .field("name", &self.name)
            .field("script_len", &self.script_source.len())
            .field("compiled", &self.compiled.is_some())
            .field("last_error", &self.last_error)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::id::VarId;
    use crate::pipeline::packet::DataPacket;
    use crate::pipeline::variable_tree::VariableTree;
    use std::time::Duration;

    fn make_context<'a>(
        input: &'a DataPacket,
        output: &'a mut DataPacket,
        input_events: &'a [crate::pipeline::packet::PipelineEvent],
        output_events: &'a mut Vec<crate::pipeline::packet::PipelineEvent>,
        var_tree: &'a VariableTree,
    ) -> NodeContext<'a> {
        NodeContext {
            input,
            output,
            input_events,
            output_events,
            var_tree,
            timestamp: Duration::from_secs(1),
            dt: Duration::from_millis(10),
            tick: 100,
        }
    }

    #[test]
    fn test_passthrough_empty_script() {
        let mut node = RhaiScriptNode::new();
        let mut input = DataPacket::new();
        input.push_value(VarId(0), 1.0, 1.0);
        input.push_value(VarId(1), 2.0, 4.0);

        let mut output = DataPacket::new();
        let var_tree = VariableTree::new();
        let input_events = vec![];
        let mut output_events = vec![];

        let mut ctx = make_context(
            &input,
            &mut output,
            &input_events,
            &mut output_events,
            &var_tree,
        );
        node.on_data(&mut ctx);

        assert_eq!(output.len(), 2);
        assert_eq!(output.get(0).unwrap().raw, 1.0);
        assert_eq!(output.get(1).unwrap().raw, 2.0);
    }

    #[test]
    fn test_passthrough_script() {
        let mut node = RhaiScriptNode::new();
        node.set_script("samples");

        let mut input = DataPacket::new();
        input.push_value(VarId(0), 1.0, 1.0);

        let mut output = DataPacket::new();
        let var_tree = VariableTree::new();
        let input_events = vec![];
        let mut output_events = vec![];

        let mut ctx = make_context(
            &input,
            &mut output,
            &input_events,
            &mut output_events,
            &var_tree,
        );
        node.on_data(&mut ctx);

        assert_eq!(output.len(), 1);
        assert_eq!(output.get(0).unwrap().raw, 1.0);
        assert_eq!(output.get(0).unwrap().converted, 1.0);
    }

    #[test]
    fn test_transform_script() {
        let mut node = RhaiScriptNode::new();
        // Use index-based iteration since Rhai's for loop gives copies
        node.set_script(
            r#"
            let len = samples.len();
            for i in 0..len {
                samples[i].converted = samples[i].converted * 2.0;
            }
            samples
        "#,
        );

        assert!(node.is_valid());

        let mut input = DataPacket::new();
        input.push_value(VarId(0), 5.0, 5.0);
        input.push_value(VarId(1), 10.0, 10.0);

        let mut output = DataPacket::new();
        let var_tree = VariableTree::new();
        let input_events = vec![];
        let mut output_events = vec![];

        let mut ctx = make_context(
            &input,
            &mut output,
            &input_events,
            &mut output_events,
            &var_tree,
        );
        node.on_data(&mut ctx);

        assert_eq!(output.len(), 2);
        assert_eq!(output.get(0).unwrap().converted, 10.0);
        assert_eq!(output.get(1).unwrap().converted, 20.0);
    }

    #[test]
    fn test_invalid_script() {
        let mut node = RhaiScriptNode::new();
        node.set_script("this is not valid rhai syntax !!!@#$");

        assert!(!node.is_valid());
        assert!(node.last_error().is_some());
    }

    #[test]
    fn test_config_change() {
        let mut node = RhaiScriptNode::new();

        let input = DataPacket::new();
        let mut output = DataPacket::new();
        let var_tree = VariableTree::new();
        let input_events = vec![];
        let mut output_events = vec![];

        let mut ctx = make_context(
            &input,
            &mut output,
            &input_events,
            &mut output_events,
            &var_tree,
        );

        node.on_config_change(
            "script",
            &ConfigValue::String("samples".to_string()),
            &mut ctx,
        );
        assert!(node.is_valid());
        assert_eq!(node.script_source(), "samples");

        node.on_config_change(
            "name",
            &ConfigValue::String("My Script".to_string()),
            &mut ctx,
        );
        assert_eq!(node.name(), "My Script");
    }
}
