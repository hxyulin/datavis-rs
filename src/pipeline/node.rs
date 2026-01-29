//! Node abstraction for the pipeline.
//!
//! Two-layer design:
//! - **`NodePlugin` trait** — for extensibility and future visual editor plugins.
//! - **`BuiltinNode` enum** — for all built-in nodes. The compiler can inline
//!   match arms, eliminating dynamic dispatch overhead on the hot path.
//!
//! `AnyNode` wraps either variant so the pipeline can handle both uniformly.

use crate::pipeline::packet::{ConfigValue, DataPacket, PipelineEvent};
use crate::pipeline::port::PortDescriptor;
use crate::pipeline::variable_tree::VariableTree;
use std::time::Duration;

/// Context passed to node lifecycle hooks each tick.
pub struct NodeContext<'a> {
    /// Input data from upstream nodes.
    pub input: &'a DataPacket,
    /// Output buffer — node writes its results here.
    pub output: &'a mut DataPacket,
    /// Events received from upstream or the pipeline.
    pub input_events: &'a [PipelineEvent],
    /// Events to emit downstream.
    pub output_events: &'a mut Vec<PipelineEvent>,
    /// Read-only view of the variable tree.
    pub var_tree: &'a VariableTree,
    /// Current tick timestamp (relative to pipeline start).
    pub timestamp: Duration,
    /// Time since last tick.
    pub dt: Duration,
    /// Monotonic tick counter.
    pub tick: u64,
}

/// Trait for pluggable/user-defined nodes.
pub trait NodePlugin: Send {
    /// Human-readable name of this node.
    fn name(&self) -> &str;

    /// Port descriptors for this node.
    fn ports(&self) -> &[PortDescriptor];

    /// Called when the pipeline activates (collection starts).
    fn on_activate(&mut self, _ctx: &mut NodeContext) {}

    /// Called every tick to process data.
    fn on_data(&mut self, ctx: &mut NodeContext);

    /// Called when the pipeline deactivates (collection stops).
    fn on_deactivate(&mut self, _ctx: &mut NodeContext) {}

    /// Called when a config value changes.
    fn on_config_change(&mut self, _key: &str, _value: &ConfigValue, _ctx: &mut NodeContext) {}
}

// Forward-declare built-in node types (defined in nodes/ submodule).
use crate::pipeline::nodes::{
    ExporterSinkNode, FilterNode, GraphSinkNode, ProbeSourceNode, RecorderSinkNode, RhaiScriptNode,
    ScriptTransformNode, UIBroadcastSinkNode,
};

/// Enum dispatch for built-in nodes — zero dynamic dispatch overhead.
pub enum BuiltinNode {
    ProbeSource(ProbeSourceNode),
    ScriptTransform(ScriptTransformNode),
    Filter(FilterNode),
    UIBroadcastSink(UIBroadcastSinkNode),
    RecorderSink(RecorderSinkNode),
    ExporterSink(ExporterSinkNode),
    RhaiScript(RhaiScriptNode),
    GraphSink(GraphSinkNode),
}

impl BuiltinNode {
    pub fn name(&self) -> &str {
        match self {
            BuiltinNode::ProbeSource(n) => n.name(),
            BuiltinNode::ScriptTransform(n) => n.name(),
            BuiltinNode::Filter(n) => n.name(),
            BuiltinNode::UIBroadcastSink(n) => n.name(),
            BuiltinNode::RecorderSink(n) => n.name(),
            BuiltinNode::ExporterSink(n) => n.name(),
            BuiltinNode::RhaiScript(n) => n.name(),
            BuiltinNode::GraphSink(n) => n.name(),
        }
    }

    pub fn ports(&self) -> &[PortDescriptor] {
        match self {
            BuiltinNode::ProbeSource(n) => n.ports(),
            BuiltinNode::ScriptTransform(n) => n.ports(),
            BuiltinNode::Filter(n) => n.ports(),
            BuiltinNode::UIBroadcastSink(n) => n.ports(),
            BuiltinNode::RecorderSink(n) => n.ports(),
            BuiltinNode::ExporterSink(n) => n.ports(),
            BuiltinNode::RhaiScript(n) => n.ports(),
            BuiltinNode::GraphSink(n) => n.ports(),
        }
    }

    pub fn on_activate(&mut self, ctx: &mut NodeContext) {
        match self {
            BuiltinNode::ProbeSource(n) => n.on_activate(ctx),
            BuiltinNode::ScriptTransform(n) => n.on_activate(ctx),
            BuiltinNode::Filter(n) => n.on_activate(ctx),
            BuiltinNode::UIBroadcastSink(n) => n.on_activate(ctx),
            BuiltinNode::RecorderSink(n) => n.on_activate(ctx),
            BuiltinNode::ExporterSink(n) => n.on_activate(ctx),
            BuiltinNode::RhaiScript(n) => n.on_activate(ctx),
            BuiltinNode::GraphSink(n) => n.on_activate(ctx),
        }
    }

    pub fn on_data(&mut self, ctx: &mut NodeContext) {
        match self {
            BuiltinNode::ProbeSource(n) => n.on_data(ctx),
            BuiltinNode::ScriptTransform(n) => n.on_data(ctx),
            BuiltinNode::Filter(n) => n.on_data(ctx),
            BuiltinNode::UIBroadcastSink(n) => n.on_data(ctx),
            BuiltinNode::RecorderSink(n) => n.on_data(ctx),
            BuiltinNode::ExporterSink(n) => n.on_data(ctx),
            BuiltinNode::RhaiScript(n) => n.on_data(ctx),
            BuiltinNode::GraphSink(n) => n.on_data(ctx),
        }
    }

    pub fn on_deactivate(&mut self, ctx: &mut NodeContext) {
        match self {
            BuiltinNode::ProbeSource(n) => n.on_deactivate(ctx),
            BuiltinNode::ScriptTransform(n) => n.on_deactivate(ctx),
            BuiltinNode::Filter(n) => n.on_deactivate(ctx),
            BuiltinNode::UIBroadcastSink(n) => n.on_deactivate(ctx),
            BuiltinNode::RecorderSink(n) => n.on_deactivate(ctx),
            BuiltinNode::ExporterSink(n) => n.on_deactivate(ctx),
            BuiltinNode::RhaiScript(n) => n.on_deactivate(ctx),
            BuiltinNode::GraphSink(n) => n.on_deactivate(ctx),
        }
    }

    pub fn on_config_change(&mut self, key: &str, value: &ConfigValue, ctx: &mut NodeContext) {
        match self {
            BuiltinNode::ProbeSource(n) => n.on_config_change(key, value, ctx),
            BuiltinNode::ScriptTransform(n) => n.on_config_change(key, value, ctx),
            BuiltinNode::Filter(n) => n.on_config_change(key, value, ctx),
            BuiltinNode::UIBroadcastSink(n) => n.on_config_change(key, value, ctx),
            BuiltinNode::RecorderSink(n) => n.on_config_change(key, value, ctx),
            BuiltinNode::ExporterSink(n) => n.on_config_change(key, value, ctx),
            BuiltinNode::RhaiScript(n) => n.on_config_change(key, value, ctx),
            BuiltinNode::GraphSink(n) => n.on_config_change(key, value, ctx),
        }
    }
}

/// Wrapper that holds either a built-in node (enum dispatch) or a plugin (trait object).
pub enum AnyNode {
    Builtin(BuiltinNode),
    Plugin(Box<dyn NodePlugin>),
}

impl AnyNode {
    pub fn name(&self) -> &str {
        match self {
            AnyNode::Builtin(n) => n.name(),
            AnyNode::Plugin(n) => n.name(),
        }
    }

    pub fn ports(&self) -> &[PortDescriptor] {
        match self {
            AnyNode::Builtin(n) => n.ports(),
            AnyNode::Plugin(n) => n.ports(),
        }
    }

    pub fn on_activate(&mut self, ctx: &mut NodeContext) {
        match self {
            AnyNode::Builtin(n) => n.on_activate(ctx),
            AnyNode::Plugin(n) => n.on_activate(ctx),
        }
    }

    pub fn on_data(&mut self, ctx: &mut NodeContext) {
        match self {
            AnyNode::Builtin(n) => n.on_data(ctx),
            AnyNode::Plugin(n) => n.on_data(ctx),
        }
    }

    pub fn on_deactivate(&mut self, ctx: &mut NodeContext) {
        match self {
            AnyNode::Builtin(n) => n.on_deactivate(ctx),
            AnyNode::Plugin(n) => n.on_deactivate(ctx),
        }
    }

    pub fn on_config_change(&mut self, key: &str, value: &ConfigValue, ctx: &mut NodeContext) {
        match self {
            AnyNode::Builtin(n) => n.on_config_change(key, value, ctx),
            AnyNode::Plugin(n) => n.on_config_change(key, value, ctx),
        }
    }
}
