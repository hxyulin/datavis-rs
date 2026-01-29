//! Pipeline executor — the main tick loop and graph scheduler.
//!
//! The pipeline runs on a dedicated thread. Each tick:
//! 1. Drain commands from the UI.
//! 2. Clear all output buffers.
//! 3. Execute nodes in topological order.
//! 4. Propagate outputs to connected inputs.
//! 5. Rate-limit to the configured Hz.

use crate::config::{AppConfig, MemoryAccessMode, ProbeConfig};
use crate::pipeline::bridge::{PipelineCommand, SinkMessage};
use crate::pipeline::compiled_plan::CompiledPlan;
use crate::pipeline::compiler::PipelineCompiler;
use crate::pipeline::id::{EdgeId, NodeId};
use crate::pipeline::node::{AnyNode, BuiltinNode, NodeContext};
use crate::pipeline::node_type::NodeType;
use crate::pipeline::nodes::{
    ExporterSinkNode, FilterNode, GraphSinkNode, ProbeSourceNode, RecorderSinkNode,
    RhaiScriptNode, ScriptTransformNode, UIBroadcastSinkNode,
};
use crate::pipeline::packet::{ConfigValue, DataPacket, PipelineEvent};
use crate::pipeline::variable_tree::VariableTree;
use crate::types::{ConnectionStatus, Variable};
use crossbeam_channel::{Receiver, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// An edge connecting an output port of one node to an input port of another.
#[derive(Debug, Clone)]
pub struct Edge {
    pub id: EdgeId,
    pub from_node: NodeId,
    pub to_node: NodeId,
}

/// A slot holding a node and its per-tick I/O buffers.
pub struct NodeSlot {
    pub node: AnyNode,
    pub input_buf: DataPacket,
    pub output_buf: DataPacket,
    pub input_events: Vec<PipelineEvent>,
    pub output_events: Vec<PipelineEvent>,
    /// Whether this node has been deleted (slot is empty).
    pub deleted: bool,
}

impl NodeSlot {
    pub fn new(node: AnyNode) -> Self {
        Self {
            node,
            input_buf: DataPacket::new(),
            output_buf: DataPacket::new(),
            input_events: Vec::new(),
            output_events: Vec::new(),
            deleted: false,
        }
    }
}

/// The main pipeline graph and executor.
pub struct Pipeline {
    nodes: Vec<NodeSlot>,
    edges: Vec<Edge>,
    /// Topological execution order (indices into `nodes`). Recomputed on graph change.
    execution_order: Vec<usize>,
    /// True when execution_order needs recomputing (deferred topo sort).
    execution_order_dirty: bool,
    /// Cached compiled execution plan
    compiled_plan: CompiledPlan,
    /// Generation counter for cache invalidation
    graph_generation: u64,
    /// Whether plan needs recompilation
    compiled_plan_dirty: bool,
    var_tree: VariableTree,
    /// True when the variable tree snapshot cache is stale.
    var_tree_dirty: bool,
    /// Cached variable tree snapshot.
    cached_var_tree_snapshot: Option<Vec<crate::pipeline::bridge::VariableNodeSnapshot>>,
    tick: u64,
    tick_rate_hz: u32,
    active: bool,
    running: Arc<AtomicBool>,
    cmd_rx: Receiver<PipelineCommand>,
    msg_tx: Sender<SinkMessage>,
    start_time: Option<Instant>,
    last_tick_time: Option<Instant>,
    last_stats_time: Instant,
    connection_status: ConnectionStatus,
    #[allow(dead_code)]
    config: AppConfig,
}

impl Pipeline {
    pub fn new(
        config: AppConfig,
        cmd_rx: Receiver<PipelineCommand>,
        msg_tx: Sender<SinkMessage>,
        running: Arc<AtomicBool>,
    ) -> Self {
        let tick_rate_hz = config.collection.poll_rate_hz;
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            execution_order: Vec::new(),
            execution_order_dirty: false,
            compiled_plan: CompiledPlan::new(),
            graph_generation: 0,
            compiled_plan_dirty: true,
            var_tree: VariableTree::new(),
            var_tree_dirty: false,
            cached_var_tree_snapshot: None,
            tick: 0,
            tick_rate_hz,
            active: false,
            running,
            cmd_rx,
            msg_tx,
            start_time: None,
            last_tick_time: None,
            last_stats_time: Instant::now(),
            connection_status: ConnectionStatus::Disconnected,
            config,
        }
    }

    // ── Graph building ──

    /// Add a node to the pipeline. Returns its NodeId.
    pub fn add_node(&mut self, node: AnyNode) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(NodeSlot::new(node));
        id
    }

    /// Connect the output of `from` to the input of `to`.
    pub fn add_edge(&mut self, from: NodeId, to: NodeId) -> EdgeId {
        let id = EdgeId(self.edges.len() as u32);
        self.edges.push(Edge {
            id,
            from_node: from,
            to_node: to,
        });
        self.invalidate_compiled_plan();
        id
    }

    /// Get a reference to the variable tree.
    pub fn var_tree(&self) -> &VariableTree {
        &self.var_tree
    }

    /// Get a mutable reference to the variable tree.
    pub fn var_tree_mut(&mut self) -> &mut VariableTree {
        &mut self.var_tree
    }

    /// Flush deferred execution order recompute (for tests / external callers).
    pub fn flush_execution_order(&mut self) {
        if self.execution_order_dirty {
            self.recompute_execution_order();
            self.execution_order_dirty = false;
        }
    }

    /// Invalidate the compiled execution plan (called when graph topology changes).
    fn invalidate_compiled_plan(&mut self) {
        self.compiled_plan_dirty = true;
        self.execution_order_dirty = true;
        self.graph_generation += 1;
    }

    /// Recompile the execution plan if needed (lazy recompilation).
    fn recompile_if_needed(&mut self) {
        if self.compiled_plan_dirty {
            self.compiled_plan = PipelineCompiler::compile(
                &self.nodes,
                &self.edges,
                self.graph_generation,
            );
            self.compiled_plan_dirty = false;

            tracing::info!(
                "Pipeline recompiled: {} active / {} total (gen {})",
                self.compiled_plan.stats.active_nodes,
                self.compiled_plan.stats.total_nodes,
                self.compiled_plan.generation,
            );

            // Log warnings for inactive sink nodes
            if !self.compiled_plan.inactive_sink_nodes.is_empty() {
                for &sink_idx in &self.compiled_plan.inactive_sink_nodes {
                    let node_name = self.nodes[sink_idx].node.name();
                    tracing::warn!(
                        "Sink node '{}' (idx {}) is disconnected from data sources",
                        node_name,
                        sink_idx
                    );
                }
            }
        }
    }

    // ── Topological sort (Kahn's algorithm) ──

    fn recompute_execution_order(&mut self) {
        let n = self.nodes.len();
        let mut in_degree = vec![0u32; n];
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];

        for edge in &self.edges {
            let from = edge.from_node.index();
            let to = edge.to_node.index();
            if from < n && to < n {
                adj[from].push(to);
                in_degree[to] += 1;
            }
        }

        // Kahn's algorithm
        let mut queue: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut order = Vec::with_capacity(n);

        while let Some(node) = queue.pop() {
            order.push(node);
            for &next in &adj[node] {
                in_degree[next] -= 1;
                if in_degree[next] == 0 {
                    queue.push(next);
                }
            }
        }

        if order.len() != n {
            tracing::warn!(
                "Pipeline graph has a cycle! Only {} of {} nodes scheduled.",
                order.len(),
                n
            );
        }

        self.execution_order = order;
    }

    // ── Main run loop ──

    /// Run the pipeline until `running` is set to false or Shutdown is received.
    pub fn run(&mut self) {
        tracing::info!("Pipeline thread started");

        while self.running.load(Ordering::Relaxed) {
            self.process_commands();
            self.check_recorder_completion();

            if self.active {
                self.tick();
            }

            // Send stats periodically
            if self.last_stats_time.elapsed() >= Duration::from_millis(500) {
                self.send_stats();
                self.last_stats_time = Instant::now();
            }

            self.rate_limit();
        }

        // Deactivate all nodes on shutdown
        if self.active {
            self.deactivate_all();
            self.check_recorder_completion();
        }

        let _ = self.msg_tx.send(SinkMessage::Shutdown);
        tracing::info!("Pipeline thread exiting");
    }

    fn process_commands(&mut self) {
        while let Ok(cmd) = self.cmd_rx.try_recv() {
            match cmd {
                PipelineCommand::Start => {
                    if !self.active {
                        self.activate_all();
                    }
                }
                PipelineCommand::Stop => {
                    if self.active {
                        self.deactivate_all();
                    }
                }
                PipelineCommand::Connect {
                    selector,
                    target,
                    probe_config,
                } => {
                    self.handle_connect(selector, target, probe_config);
                }
                PipelineCommand::Disconnect => {
                    self.handle_disconnect();
                }
                PipelineCommand::AddVariable(var) => {
                    self.handle_add_variable(var);
                }
                PipelineCommand::RemoveVariable(id) => {
                    self.handle_remove_variable(id);
                }
                PipelineCommand::UpdateVariable(var) => {
                    self.handle_update_variable(var);
                }
                PipelineCommand::WriteVariable { id, value } => {
                    self.handle_write_variable(id, value);
                }
                PipelineCommand::SetPollRate(hz) => {
                    self.tick_rate_hz = hz;
                }
                PipelineCommand::SetMemoryAccessMode(mode) => {
                    self.handle_set_memory_access_mode(mode);
                }
                PipelineCommand::ClearData => {
                    // Clear is handled by sending a message to the UI
                    // The pipeline itself doesn't store historical data
                }
                PipelineCommand::RequestStats => {
                    self.send_stats();
                }
                PipelineCommand::NodeConfig {
                    node_id,
                    key,
                    value,
                } => {
                    self.handle_node_config(node_id, &key, &value);
                }
                #[cfg(feature = "mock-probe")]
                PipelineCommand::UseMockProbe(use_mock) => {
                    self.handle_use_mock_probe(use_mock);
                }
                PipelineCommand::RefreshProbes => {
                    self.handle_refresh_probes();
                }
                PipelineCommand::RequestVariableTree => {
                    self.handle_request_variable_tree();
                }
                PipelineCommand::RequestTopology => {
                    self.handle_request_topology();
                }
                PipelineCommand::AddNode { node_type, config } => {
                    self.handle_add_node(node_type, config);
                }
                PipelineCommand::RemoveNode(node_id) => {
                    self.handle_remove_node(node_id);
                }
                PipelineCommand::AddEdge { from_node, to_node } => {
                    self.handle_add_edge(from_node, to_node);
                }
                PipelineCommand::RemoveEdge(edge_id) => {
                    self.handle_remove_edge(edge_id);
                }
                PipelineCommand::Shutdown => {
                    self.running.store(false, Ordering::Relaxed);
                }
            }
        }
    }

    // ── Tick execution ──

    fn tick(&mut self) {
        // Lazy recompilation (only when graph topology changes)
        self.recompile_if_needed();

        let now = Instant::now();
        let timestamp = self
            .start_time
            .map(|s| now.duration_since(s))
            .unwrap_or(Duration::ZERO);
        let dt = self
            .last_tick_time
            .map(|t| now.duration_since(t))
            .unwrap_or(Duration::from_millis(1));
        self.last_tick_time = Some(now);

        // 1. Clear ACTIVE node outputs only (pre-filtered, no deleted check needed)
        for &idx in &self.compiled_plan.active_nodes {
            let slot = &mut self.nodes[idx];
            slot.output_buf.clear();
            slot.output_events.clear();
        }

        // 2. Execute ACTIVE nodes only in topological order (pre-sorted, no checks needed)
        for &idx in &self.compiled_plan.active_nodes {
            let slot = &mut self.nodes[idx];

            let mut ctx = NodeContext {
                input: &slot.input_buf,
                output: &mut slot.output_buf,
                input_events: &slot.input_events,
                output_events: &mut slot.output_events,
                var_tree: &self.var_tree,
                timestamp,
                dt,
                tick: self.tick,
            };

            slot.node.on_data(&mut ctx);
        }

        // 3. Propagate ACTIVE edges only (pre-computed, pre-validated indices)
        let ptr = self.nodes.as_mut_ptr();
        for &(from, to) in &self.compiled_plan.active_edges {
            unsafe {
                let src = &(*ptr.add(from)).output_buf;
                let dst = &mut (*ptr.add(to)).input_buf;
                dst.copy_from(src);
                dst.timestamp = timestamp;
            }

            // Also propagate events
            let events: Vec<PipelineEvent> = self.nodes[from].output_events.clone();
            self.nodes[to].input_events = events;
        }

        self.tick += 1;
    }

    // ── Lifecycle ──

    fn activate_all(&mut self) {
        if self.execution_order_dirty {
            self.recompute_execution_order();
            self.execution_order_dirty = false;
        }
        self.active = true;
        self.start_time = Some(Instant::now());
        self.last_tick_time = None;
        self.tick = 0;

        let timestamp = Duration::ZERO;
        let dt = Duration::from_millis(1);

        for idx in 0..self.nodes.len() {
            let slot = &mut self.nodes[idx];
            if slot.deleted {
                continue;
            }
            let mut ctx = NodeContext {
                input: &slot.input_buf,
                output: &mut slot.output_buf,
                input_events: &slot.input_events,
                output_events: &mut slot.output_events,
                var_tree: &self.var_tree,
                timestamp,
                dt,
                tick: 0,
            };
            slot.node.on_activate(&mut ctx);
        }
    }

    fn deactivate_all(&mut self) {
        let timestamp = self
            .start_time
            .map(|s| s.elapsed())
            .unwrap_or(Duration::ZERO);
        let dt = Duration::from_millis(1);

        for idx in 0..self.nodes.len() {
            let slot = &mut self.nodes[idx];
            if slot.deleted {
                continue;
            }
            let mut ctx = NodeContext {
                input: &slot.input_buf,
                output: &mut slot.output_buf,
                input_events: &slot.input_events,
                output_events: &mut slot.output_events,
                var_tree: &self.var_tree,
                timestamp,
                dt,
                tick: self.tick,
            };
            slot.node.on_deactivate(&mut ctx);
        }
        self.active = false;
    }

    // ── Rate limiting ──

    fn rate_limit(&self) {
        if self.tick_rate_hz == 0 {
            std::thread::sleep(Duration::from_millis(10));
            return;
        }

        let target_interval = Duration::from_nanos(1_000_000_000 / self.tick_rate_hz as u64);

        if let Some(last) = self.last_tick_time {
            let elapsed = last.elapsed();
            if elapsed < target_interval {
                let remaining = target_interval - elapsed;
                // Spin for sub-millisecond accuracy, sleep for larger waits
                if remaining > Duration::from_millis(2) {
                    std::thread::sleep(remaining - Duration::from_millis(1));
                }
                while last.elapsed() < target_interval {
                    std::hint::spin_loop();
                }
            }
        } else if !self.active {
            // Not active — idle wait
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    // ── Command handlers ──

    fn handle_connect(
        &mut self,
        selector: Option<String>,
        target: String,
        probe_config: ProbeConfig,
    ) {
        // Forward connect to ProbeSource node if present
        self.update_connection_status(ConnectionStatus::Connecting);

        for slot in &mut self.nodes {
            if let AnyNode::Builtin(BuiltinNode::ProbeSource(ref mut probe_node)) = slot.node {
                match probe_node.connect(selector.as_deref(), &target, &probe_config) {
                    Ok(()) => {
                        self.connection_status = ConnectionStatus::Connected;
                        let _ = self
                            .msg_tx
                            .send(SinkMessage::ConnectionStatus(ConnectionStatus::Connected));
                    }
                    Err(e) => {
                        self.connection_status = ConnectionStatus::Error;
                        let _ = self.msg_tx.send(SinkMessage::ConnectionError(e));
                        let _ = self
                            .msg_tx
                            .send(SinkMessage::ConnectionStatus(ConnectionStatus::Error));
                    }
                }
                return;
            }
        }
    }

    fn handle_disconnect(&mut self) {
        for slot in &mut self.nodes {
            if let AnyNode::Builtin(BuiltinNode::ProbeSource(ref mut probe_node)) = slot.node {
                probe_node.disconnect();
            }
        }
        self.update_connection_status(ConnectionStatus::Disconnected);
    }

    fn handle_add_variable(&mut self, var: Variable) {
        let var_id = self.var_tree.add_root(
            var.name.clone(),
            var.address,
            var.var_type,
        );

        // Copy variable properties
        if let Some(node) = self.var_tree.get_mut(var_id) {
            node.enabled = var.enabled;
            node.converter_script = var.converter_script.clone();
            node.color = var.color;
            node.unit = var.unit.clone();
        }
        self.var_tree_dirty = true;

        // Notify ProbeSource about the new variable
        for slot in &mut self.nodes {
            if let AnyNode::Builtin(BuiltinNode::ProbeSource(ref mut probe_node)) = slot.node {
                probe_node.add_variable(&var);
            }
            if let AnyNode::Builtin(BuiltinNode::ScriptTransform(ref mut script_node)) = slot.node
            {
                script_node.add_variable(&var);
            }
        }
    }

    fn handle_remove_variable(&mut self, id: u32) {
        self.var_tree_dirty = true;
        // Find the variable by its legacy ID
        // Legacy IDs map to the variable's original u32 ID stored in the tree
        for slot in &mut self.nodes {
            if let AnyNode::Builtin(BuiltinNode::ProbeSource(ref mut probe_node)) = slot.node {
                probe_node.remove_variable(id);
            }
            if let AnyNode::Builtin(BuiltinNode::ScriptTransform(ref mut script_node)) = slot.node
            {
                script_node.remove_variable(id);
            }
        }
    }

    fn handle_update_variable(&mut self, var: Variable) {
        self.var_tree_dirty = true;
        for slot in &mut self.nodes {
            if let AnyNode::Builtin(BuiltinNode::ProbeSource(ref mut probe_node)) = slot.node {
                probe_node.update_variable(&var);
            }
            if let AnyNode::Builtin(BuiltinNode::ScriptTransform(ref mut script_node)) = slot.node
            {
                script_node.update_variable(&var);
            }
        }
    }

    fn handle_write_variable(&mut self, id: u32, value: f64) {
        for slot in &mut self.nodes {
            if let AnyNode::Builtin(BuiltinNode::ProbeSource(ref mut probe_node)) = slot.node {
                match probe_node.write_variable(id, value) {
                    Ok(()) => {
                        let _ = self
                            .msg_tx
                            .send(SinkMessage::WriteSuccess { variable_id: id });
                    }
                    Err(e) => {
                        let _ = self.msg_tx.send(SinkMessage::WriteError {
                            variable_id: id,
                            error: e,
                        });
                    }
                }
            }
        }
    }

    fn handle_set_memory_access_mode(&mut self, mode: MemoryAccessMode) {
        for slot in &mut self.nodes {
            if let AnyNode::Builtin(BuiltinNode::ProbeSource(ref mut probe_node)) = slot.node {
                probe_node.set_memory_access_mode(mode);
            }
        }
    }

    fn handle_node_config(&mut self, node_id: NodeId, key: &str, value: &ConfigValue) {
        if let Some(slot) = self.nodes.get_mut(node_id.index()) {
            let timestamp = self
                .start_time
                .map(|s| s.elapsed())
                .unwrap_or(Duration::ZERO);
            let mut ctx = NodeContext {
                input: &slot.input_buf,
                output: &mut slot.output_buf,
                input_events: &slot.input_events,
                output_events: &mut slot.output_events,
                var_tree: &self.var_tree,
                timestamp,
                dt: Duration::from_millis(1),
                tick: self.tick,
            };
            slot.node.on_config_change(key, value, &mut ctx);
        }
    }

    #[cfg(feature = "mock-probe")]
    fn handle_use_mock_probe(&mut self, use_mock: bool) {
        for slot in &mut self.nodes {
            if let AnyNode::Builtin(BuiltinNode::ProbeSource(ref mut probe_node)) = slot.node {
                probe_node.set_use_mock(use_mock);
            }
        }
    }

    fn handle_refresh_probes(&self) {
        let tx = self.msg_tx.clone();
        std::thread::spawn(move || {
            let probes = crate::backend::list_all_probes();
            let _ = tx.send(SinkMessage::ProbeList(probes));
        });
    }

    fn update_connection_status(&mut self, status: ConnectionStatus) {
        self.connection_status = status;
        let _ = self.msg_tx.send(SinkMessage::ConnectionStatus(status));
    }

    fn send_stats(&self) {
        // Collect stats from probe source node
        for slot in &self.nodes {
            if let AnyNode::Builtin(BuiltinNode::ProbeSource(ref probe_node)) = slot.node {
                let stats = probe_node.collection_stats();
                let _ = self.msg_tx.send(SinkMessage::Stats(stats));
                break;
            }
        }

        // Send recorder status
        for slot in &self.nodes {
            if let AnyNode::Builtin(BuiltinNode::RecorderSink(ref recorder)) = slot.node {
                let _ = self.msg_tx.send(SinkMessage::RecorderStatus {
                    state: recorder.state(),
                    frame_count: recorder.frame_count(),
                });
                break;
            }
        }

        // Send exporter status
        for slot in &self.nodes {
            if let AnyNode::Builtin(BuiltinNode::ExporterSink(ref exporter)) = slot.node {
                let _ = self.msg_tx.send(SinkMessage::ExporterStatus {
                    active: exporter.is_active(),
                    rows_written: exporter.rows_written(),
                });
                break;
            }
        }
    }

    /// Check if the recorder has completed a recording (transitioned to Stopped).
    /// If so, take the recording and send it to the UI.
    fn check_recorder_completion(&mut self) {
        for slot in &mut self.nodes {
            if let AnyNode::Builtin(BuiltinNode::RecorderSink(ref mut recorder)) = slot.node {
                if recorder.state() == crate::session::SessionState::Stopped {
                    let recording = recorder.take_recording();
                    if !recording.is_empty() {
                        let _ = self.msg_tx.send(SinkMessage::RecordingComplete(recording));
                    }
                }
                break;
            }
        }
    }

    fn handle_request_variable_tree(&mut self) {
        use crate::pipeline::bridge::VariableNodeSnapshot;
        if self.var_tree_dirty || self.cached_var_tree_snapshot.is_none() {
            let snapshots: Vec<VariableNodeSnapshot> = self
                .var_tree
                .iter()
                .map(|node| VariableNodeSnapshot {
                    id: node.id,
                    name: node.name.clone(),
                    short_name: node.short_name.clone(),
                    address: node.address,
                    var_type: node.var_type,
                    parent: node.parent,
                    first_child: node.first_child,
                    next_sibling: node.next_sibling,
                    depth: node.depth,
                    is_leaf: node.is_leaf,
                    enabled: node.enabled,
                })
                .collect();
            self.cached_var_tree_snapshot = Some(snapshots);
            self.var_tree_dirty = false;
        }
        if let Some(ref snapshots) = self.cached_var_tree_snapshot {
            let _ = self.msg_tx.send(SinkMessage::VariableTreeSnapshot(snapshots.clone()));
        }
    }

    fn handle_request_topology(&self) {
        use crate::pipeline::bridge::{EdgeSnapshot, NodeSnapshot, TopologySnapshot};
        let nodes: Vec<NodeSnapshot> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, slot)| !slot.deleted)
            .map(|(i, slot)| {
                // Determine node type from the node itself
                let node_type = match &slot.node {
                    AnyNode::Builtin(builtin) => match builtin {
                        BuiltinNode::Filter(_) => Some(NodeType::Filter),
                        BuiltinNode::RhaiScript(_) => Some(NodeType::RhaiScript),
                        BuiltinNode::UIBroadcastSink(_) => Some(NodeType::UIBroadcastSink),
                        BuiltinNode::RecorderSink(_) => Some(NodeType::RecorderSink),
                        BuiltinNode::ExporterSink(_) => Some(NodeType::ExporterSink),
                        BuiltinNode::GraphSink(_) => Some(NodeType::GraphSink),
                        _ => None,
                    },
                    AnyNode::Plugin(_) => None,
                };

                NodeSnapshot {
                    id: NodeId(i as u32),
                    name: slot.node.name().to_string(),
                    ports: slot.node.ports().to_vec(),
                    node_type,
                }
            })
            .collect();

        let edges: Vec<EdgeSnapshot> = self
            .edges
            .iter()
            .map(|e| EdgeSnapshot {
                id: e.id,
                from_node: e.from_node,
                to_node: e.to_node,
            })
            .collect();

        let _ = self
            .msg_tx
            .send(SinkMessage::Topology(TopologySnapshot { nodes, edges }));
    }

    // ── Dynamic graph mutation handlers ──

    fn handle_add_node(&mut self, node_type: NodeType, config: Option<ConfigValue>) {
        let factory = NodeFactory::new(self.msg_tx.clone());
        let node = factory.create(node_type, config);
        let node_id = self.add_node(node);
        self.invalidate_compiled_plan();

        let _ = self.msg_tx.send(SinkMessage::NodeAdded(node_id));
        let _ = self.msg_tx.send(SinkMessage::TopologyChanged);

        // Auto-create panes for nodes with UI representations
        use crate::frontend::workspace::PaneKind;
        let pane_kind = match node_type {
            NodeType::GraphSink => Some(PaneKind::TimeSeries),
            NodeType::RecorderSink => Some(PaneKind::Recorder),
            _ => None,
        };

        if let Some(kind) = pane_kind {
            let _ = self.msg_tx.send(SinkMessage::CreatePaneForNode {
                node_id,
                pane_kind: kind,
            });
        }

        tracing::info!("Added node {:?} of type {:?}", node_id, node_type);
    }

    fn handle_remove_node(&mut self, node_id: NodeId) {
        let idx = node_id.index();
        if idx >= self.nodes.len() {
            let _ = self.msg_tx.send(SinkMessage::GraphError(format!(
                "Invalid node ID: {:?}",
                node_id
            )));
            return;
        }

        // Check if this is a protected node (ProbeSource only - UIBroadcastSink can be removed)
        let node_name = self.nodes[idx].node.name().to_string();
        if node_name == "ProbeSource" {
            let _ = self.msg_tx.send(SinkMessage::GraphError(format!(
                "Cannot remove protected node: {}",
                node_name
            )));
            return;
        }

        // Check if this node has a linked pane that should be closed
        let pane_id = match &self.nodes[idx].node {
            AnyNode::Builtin(BuiltinNode::GraphSink(sink)) => {
                // Try to get pane_id from GraphSink
                sink.pane_id()
            }
            AnyNode::Builtin(BuiltinNode::RecorderSink(sink)) => {
                // Try to get pane_id from RecorderSink
                sink.pane_id()
            }
            _ => None,
        };

        // Remove all edges connected to this node
        self.edges
            .retain(|e| e.from_node != node_id && e.to_node != node_id);

        // Mark the node as deleted
        self.nodes[idx].deleted = true;
        self.invalidate_compiled_plan();

        let _ = self.msg_tx.send(SinkMessage::NodeRemoved(node_id));
        let _ = self.msg_tx.send(SinkMessage::TopologyChanged);

        // If this node had a linked pane, tell frontend to close it
        if let Some(pane_id) = pane_id {
            let _ = self.msg_tx.send(SinkMessage::ClosePaneForNode { pane_id });
        }

        tracing::info!("Removed node {:?}", node_id);
    }

    fn handle_add_edge(&mut self, from_node: NodeId, to_node: NodeId) {
        // Validate nodes exist and are not deleted
        let from_idx = from_node.index();
        let to_idx = to_node.index();

        if from_idx >= self.nodes.len() || self.nodes[from_idx].deleted {
            let _ = self.msg_tx.send(SinkMessage::GraphError(format!(
                "Invalid source node: {:?}",
                from_node
            )));
            return;
        }
        if to_idx >= self.nodes.len() || self.nodes[to_idx].deleted {
            let _ = self.msg_tx.send(SinkMessage::GraphError(format!(
                "Invalid target node: {:?}",
                to_node
            )));
            return;
        }
        if from_node == to_node {
            let _ = self
                .msg_tx
                .send(SinkMessage::GraphError("Cannot connect node to itself".to_string()));
            return;
        }

        // Check for cycles (simple check: if adding this edge would create a back-edge)
        if self.would_create_cycle(from_node, to_node) {
            let _ = self.msg_tx.send(SinkMessage::GraphError(
                "Adding this edge would create a cycle".to_string(),
            ));
            return;
        }

        let edge_id = self.add_edge(from_node, to_node);
        let _ = self.msg_tx.send(SinkMessage::EdgeAdded(edge_id));
        let _ = self.msg_tx.send(SinkMessage::TopologyChanged);
        tracing::info!(
            "Added edge {:?}: {:?} -> {:?}",
            edge_id,
            from_node,
            to_node
        );
    }

    fn handle_remove_edge(&mut self, edge_id: EdgeId) {
        let idx = edge_id.index();
        if idx >= self.edges.len() {
            let _ = self.msg_tx.send(SinkMessage::GraphError(format!(
                "Invalid edge ID: {:?}",
                edge_id
            )));
            return;
        }

        self.edges.remove(idx);
        self.invalidate_compiled_plan();

        let _ = self.msg_tx.send(SinkMessage::EdgeRemoved(edge_id));
        let _ = self.msg_tx.send(SinkMessage::TopologyChanged);
        tracing::info!("Removed edge {:?}", edge_id);
    }

    /// Check if adding an edge from `from` to `to` would create a cycle.
    fn would_create_cycle(&self, from: NodeId, to: NodeId) -> bool {
        // If `to` can reach `from` through existing edges, adding from->to creates a cycle.
        let mut visited = vec![false; self.nodes.len()];
        let mut stack = vec![to];

        while let Some(current) = stack.pop() {
            if current == from {
                return true;
            }
            let idx = current.index();
            if idx >= self.nodes.len() || visited[idx] {
                continue;
            }
            visited[idx] = true;

            // Find all nodes reachable from current
            for edge in &self.edges {
                if edge.from_node == current && !self.nodes[edge.to_node.index()].deleted {
                    stack.push(edge.to_node);
                }
            }
        }
        false
    }
}

/// Factory for creating nodes dynamically.
///
/// Sink nodes require access to the msg_tx channel, so this factory
/// encapsulates the creation logic with access to the channel.
pub struct NodeFactory {
    msg_tx: Sender<SinkMessage>,
}

impl NodeFactory {
    pub fn new(msg_tx: Sender<SinkMessage>) -> Self {
        Self { msg_tx }
    }

    /// Create a node based on the NodeType and optional config.
    pub fn create(&self, node_type: NodeType, config: Option<ConfigValue>) -> AnyNode {
        match node_type {
            NodeType::Filter => AnyNode::Builtin(BuiltinNode::Filter(FilterNode::new())),
            NodeType::RhaiScript => {
                let mut node = RhaiScriptNode::new();
                if let Some(ConfigValue::String(script)) = config {
                    node.set_script(&script);
                }
                AnyNode::Builtin(BuiltinNode::RhaiScript(node))
            }
            NodeType::UIBroadcastSink => {
                AnyNode::Builtin(BuiltinNode::UIBroadcastSink(UIBroadcastSinkNode::new(
                    self.msg_tx.clone(),
                )))
            }
            NodeType::RecorderSink => {
                AnyNode::Builtin(BuiltinNode::RecorderSink(RecorderSinkNode::new()))
            }
            NodeType::ExporterSink => {
                AnyNode::Builtin(BuiltinNode::ExporterSink(ExporterSinkNode::new()))
            }
            NodeType::GraphSink => {
                let pane_id = config.and_then(|c| c.as_int()).map(|id| id as u64);
                AnyNode::Builtin(BuiltinNode::GraphSink(GraphSinkNode::new(
                    self.msg_tx.clone(),
                    pane_id,
                )))
            }
        }
    }
}

/// Node IDs for the default pipeline graph, so the UI can address specific nodes.
#[derive(Debug, Clone, Copy)]
pub struct PipelineNodeIds {
    pub probe_source: NodeId,
    pub script_transform: NodeId,
    pub variable_sink: NodeId,
    pub recorder_sink: NodeId,
    pub exporter_sink: NodeId,
}

/// Builder for constructing the default pipeline graph.
pub struct PipelineBuilder {
    config: AppConfig,
}

impl PipelineBuilder {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    /// Build the default pipeline matching current app behavior:
    /// ```text
    /// ProbeSource → ScriptTransform → VariableSink
    /// ```
    /// RecorderSink and ExporterSink are added dynamically via UI.
    pub fn build_default(
        self,
        cmd_rx: Receiver<PipelineCommand>,
        msg_tx: Sender<SinkMessage>,
        running: Arc<AtomicBool>,
    ) -> (Pipeline, PipelineNodeIds) {
        let mut pipeline = Pipeline::new(self.config.clone(), cmd_rx, msg_tx.clone(), running);

        // Create nodes
        let probe_source = ProbeSourceNode::new(self.config.clone());
        let script_transform = ScriptTransformNode::new();
        let ui_sink = UIBroadcastSinkNode::new(msg_tx.clone());
        let recorder_sink = RecorderSinkNode::new();
        let exporter_sink = ExporterSinkNode::new();

        let source_id = pipeline.add_node(AnyNode::Builtin(BuiltinNode::ProbeSource(probe_source)));
        let transform_id =
            pipeline.add_node(AnyNode::Builtin(BuiltinNode::ScriptTransform(script_transform)));
        let ui_id = pipeline.add_node(AnyNode::Builtin(BuiltinNode::UIBroadcastSink(ui_sink)));
        let recorder_id =
            pipeline.add_node(AnyNode::Builtin(BuiltinNode::RecorderSink(recorder_sink)));
        let exporter_id =
            pipeline.add_node(AnyNode::Builtin(BuiltinNode::ExporterSink(exporter_sink)));

        // Wire: ProbeSource → ScriptTransform → VariableSink
        // This is the minimal default pipeline. RecorderSink and ExporterSink
        // exist but are not connected - they can be wired dynamically via UI.
        pipeline.add_edge(source_id, transform_id);
        pipeline.add_edge(transform_id, ui_id);

        // Add configured variables
        for var in &self.config.variables {
            pipeline.handle_add_variable(var.clone());
        }

        let node_ids = PipelineNodeIds {
            probe_source: source_id,
            script_transform: transform_id,
            variable_sink: ui_id,
            recorder_sink: recorder_id,
            exporter_sink: exporter_id,
        };

        (pipeline, node_ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::nodes::FilterNode;
    use crossbeam_channel::bounded;

    #[test]
    fn test_topological_sort_linear() {
        let (cmd_tx, cmd_rx) = bounded(16);
        let (msg_tx, msg_rx) = bounded(16);
        let running = Arc::new(AtomicBool::new(true));
        let config = AppConfig::default();

        let mut pipeline = Pipeline::new(config, cmd_rx, msg_tx, running);

        // Add 3 nodes: A → B → C
        let a = pipeline.add_node(AnyNode::Builtin(BuiltinNode::Filter(FilterNode::new())));
        let b = pipeline.add_node(AnyNode::Builtin(BuiltinNode::Filter(FilterNode::new())));
        let c = pipeline.add_node(AnyNode::Builtin(BuiltinNode::Filter(FilterNode::new())));

        pipeline.add_edge(a, b);
        pipeline.add_edge(b, c);
        pipeline.flush_execution_order();

        // Execution order should be [0, 1, 2]
        assert_eq!(pipeline.execution_order.len(), 3);
        // A must come before B, B before C
        let pos_a = pipeline
            .execution_order
            .iter()
            .position(|&x| x == a.index())
            .unwrap();
        let pos_b = pipeline
            .execution_order
            .iter()
            .position(|&x| x == b.index())
            .unwrap();
        let pos_c = pipeline
            .execution_order
            .iter()
            .position(|&x| x == c.index())
            .unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);

        drop(cmd_tx);
        drop(msg_rx);
    }

    #[test]
    fn test_topological_sort_diamond() {
        let (_cmd_tx, cmd_rx) = bounded(16);
        let (msg_tx, _msg_rx) = bounded(16);
        let running = Arc::new(AtomicBool::new(true));
        let config = AppConfig::default();

        let mut pipeline = Pipeline::new(config, cmd_rx, msg_tx, running);

        // Diamond: A → B, A → C, B → D, C → D
        let a = pipeline.add_node(AnyNode::Builtin(BuiltinNode::Filter(FilterNode::new())));
        let b = pipeline.add_node(AnyNode::Builtin(BuiltinNode::Filter(FilterNode::new())));
        let c = pipeline.add_node(AnyNode::Builtin(BuiltinNode::Filter(FilterNode::new())));
        let d = pipeline.add_node(AnyNode::Builtin(BuiltinNode::Filter(FilterNode::new())));

        pipeline.add_edge(a, b);
        pipeline.add_edge(a, c);
        pipeline.add_edge(b, d);
        pipeline.add_edge(c, d);
        pipeline.flush_execution_order();

        assert_eq!(pipeline.execution_order.len(), 4);

        let pos = |nid: NodeId| {
            pipeline
                .execution_order
                .iter()
                .position(|&x| x == nid.index())
                .unwrap()
        };

        assert!(pos(a) < pos(b));
        assert!(pos(a) < pos(c));
        assert!(pos(b) < pos(d));
        assert!(pos(c) < pos(d));
    }
}
