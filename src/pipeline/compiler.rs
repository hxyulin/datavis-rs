use super::compiled_plan::{CompiledPlan, PlanStats};
use super::executor::{Edge, NodeSlot};
use super::node::{AnyNode, BuiltinNode};
use std::collections::VecDeque;

/// Compiles a pipeline graph into an optimized execution plan
pub struct PipelineCompiler;

impl PipelineCompiler {
    /// Compile a pipeline graph into an optimized execution plan.
    ///
    /// This performs bidirectional reachability analysis to identify which nodes
    /// participate in active data flow (have both upstream sources AND downstream sinks).
    ///
    /// # Arguments
    /// * `nodes` - All nodes in the graph (including deleted/disconnected)
    /// * `edges` - All edges in the graph
    /// * `generation` - Generation counter for cache invalidation
    ///
    /// # Returns
    /// A `CompiledPlan` containing only active nodes in topological order
    pub fn compile(nodes: &[NodeSlot], edges: &[Edge], generation: u64) -> CompiledPlan {
        let start_time = std::time::Instant::now();

        let n = nodes.len();
        if n == 0 {
            return CompiledPlan {
                active_nodes: Vec::new(),
                active_edges: Vec::new(),
                generation,
                stats: PlanStats::default(),
                inactive_sink_nodes: Vec::new(),
            };
        }

        // Build adjacency lists (forward and backward)
        let (fwd_adj, bwd_adj) = Self::build_adjacency(nodes, edges);

        // Identify sources (nodes that produce data)
        let sources = Self::identify_sources(nodes, &bwd_adj);

        // Identify sinks (nodes that consume data)
        let sinks = Self::identify_sinks(nodes, &fwd_adj);

        // Forward reachability from sources
        let fwd_reachable = Self::forward_reachability(&sources, &fwd_adj, n);

        // Backward reachability from sinks
        let bwd_reachable = Self::backward_reachability(&sinks, &bwd_adj, n);

        // Compute active set: nodes participating in data flow from sources to sinks
        let active_set = Self::compute_active_set(
            nodes,
            &sources,
            &sinks,
            &fwd_reachable,
            &bwd_reachable,
        );

        // Topological sort of active nodes
        let active_nodes = Self::topological_sort_active(nodes, edges, &active_set);

        // Filter edges to only include active → active connections
        let active_edges = Self::filter_active_edges(edges, &active_set);

        // Identify inactive sink nodes (disconnected from sources)
        let inactive_sink_nodes: Vec<usize> = sinks
            .iter()
            .filter(|&&sink_idx| !active_set[sink_idx])
            .copied()
            .collect();

        let compile_time_us = start_time.elapsed().as_micros() as u64;

        let total_nodes = nodes.iter().filter(|slot| !slot.deleted).count();
        let active_count = active_nodes.len();

        let stats = PlanStats {
            total_nodes,
            active_nodes: active_count,
            disconnected_nodes: total_nodes.saturating_sub(active_count),
            source_nodes: sources.len(),
            sink_nodes: sinks.len(),
            compile_time_us,
        };

        CompiledPlan {
            active_nodes,
            active_edges,
            generation,
            stats,
            inactive_sink_nodes,
        }
    }

    /// Build forward and backward adjacency lists
    fn build_adjacency(
        nodes: &[NodeSlot],
        edges: &[Edge],
    ) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
        let n = nodes.len();
        let mut fwd_adj = vec![Vec::new(); n];
        let mut bwd_adj = vec![Vec::new(); n];

        for edge in edges {
            let from = edge.from_node.index();
            let to = edge.to_node.index();

            // Skip edges involving deleted nodes
            if from >= n || to >= n || nodes[from].deleted || nodes[to].deleted {
                continue;
            }

            fwd_adj[from].push(to);
            bwd_adj[to].push(from);
        }

        (fwd_adj, bwd_adj)
    }

    /// Identify source nodes (nodes that can produce data without upstream inputs)
    ///
    /// A node is a source if it has NO input ports structurally (e.g., ProbeSource).
    /// We don't use edge connectivity here - a source is defined by its port structure.
    fn identify_sources(nodes: &[NodeSlot], _bwd_adj: &[Vec<usize>]) -> Vec<usize> {
        let mut sources = Vec::new();

        for (idx, slot) in nodes.iter().enumerate() {
            if slot.deleted {
                continue;
            }

            // Source if it has no input port (structurally defined, not edge-based)
            if !Self::has_input_port(slot) {
                sources.push(idx);
            }
        }

        sources
    }

    /// Identify sink nodes (nodes that consume data without downstream outputs)
    ///
    /// A node is a sink if it has NO output ports structurally (e.g., GraphSink, RecorderSink).
    /// We don't use edge connectivity here - a sink is defined by its port structure.
    fn identify_sinks(nodes: &[NodeSlot], _fwd_adj: &[Vec<usize>]) -> Vec<usize> {
        let mut sinks = Vec::new();

        for (idx, slot) in nodes.iter().enumerate() {
            if slot.deleted {
                continue;
            }

            // Sink if it has no output port (structurally defined, not edge-based)
            if !Self::has_output_port(slot) {
                sinks.push(idx);
            }
        }

        sinks
    }

    /// Check if a node has an input port
    fn has_input_port(slot: &NodeSlot) -> bool {
        match &slot.node {
            AnyNode::Builtin(builtin) => match builtin {
                BuiltinNode::ProbeSource(_) => false,
                _ => true, // All other built-in nodes have input ports
            },
            AnyNode::Plugin(_) => true, // Assume plugins have inputs
        }
    }

    /// Check if a node has an output port
    fn has_output_port(slot: &NodeSlot) -> bool {
        match &slot.node {
            AnyNode::Builtin(builtin) => match builtin {
                BuiltinNode::UIBroadcastSink(_) => false,
                BuiltinNode::GraphSink(_) => false,
                BuiltinNode::RecorderSink(_) => false,
                BuiltinNode::ExporterSink(_) => false,
                _ => true, // All other built-in nodes have output ports
            },
            AnyNode::Plugin(_) => true, // Assume plugins have outputs
        }
    }

    /// Perform forward reachability analysis from sources using DFS
    fn forward_reachability(
        sources: &[usize],
        fwd_adj: &[Vec<usize>],
        n: usize,
    ) -> Vec<bool> {
        let mut reachable = vec![false; n];
        let mut stack = Vec::new();

        // Mark all sources as reachable
        for &src in sources {
            reachable[src] = true;
            stack.push(src);
        }

        // DFS from sources
        while let Some(node) = stack.pop() {
            for &neighbor in &fwd_adj[node] {
                if !reachable[neighbor] {
                    reachable[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }

        reachable
    }

    /// Perform backward reachability analysis from sinks using DFS
    fn backward_reachability(sinks: &[usize], bwd_adj: &[Vec<usize>], n: usize) -> Vec<bool> {
        let mut reachable = vec![false; n];
        let mut stack = Vec::new();

        // Mark all sinks as reachable
        for &sink in sinks {
            reachable[sink] = true;
            stack.push(sink);
        }

        // DFS backward from sinks
        while let Some(node) = stack.pop() {
            for &neighbor in &bwd_adj[node] {
                if !reachable[neighbor] {
                    reachable[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }

        reachable
    }

    /// Compute active set: nodes participating in data flow from sources to sinks
    ///
    /// A node is active if:
    /// - It's a source AND forward-reachable to at least one sink
    /// - It's a sink AND backward-reachable from at least one source
    /// - It's reachable from sources AND reachable to sinks (on a path between them)
    fn compute_active_set(
        nodes: &[NodeSlot],
        sources: &[usize],
        sinks: &[usize],
        fwd_reachable: &[bool],
        bwd_reachable: &[bool],
    ) -> Vec<bool> {
        let n = nodes.len();
        let mut active = vec![false; n];

        // Add sources that can reach sinks
        for &src in sources {
            if fwd_reachable[src] && bwd_reachable[src] {
                active[src] = true;
            }
        }

        // Add sinks that are reachable from sources
        for &sink in sinks {
            if fwd_reachable[sink] && bwd_reachable[sink] {
                active[sink] = true;
            }
        }

        // Add transform nodes on paths between sources and sinks
        for i in 0..n {
            if !nodes[i].deleted && fwd_reachable[i] && bwd_reachable[i] {
                active[i] = true;
            }
        }

        active
    }

    /// Topological sort of active nodes using Kahn's algorithm
    fn topological_sort_active(
        nodes: &[NodeSlot],
        edges: &[Edge],
        active_set: &[bool],
    ) -> Vec<usize> {
        let n = nodes.len();

        // Build adjacency list for active nodes only
        let mut adj = vec![Vec::new(); n];
        let mut in_degree = vec![0; n];

        for edge in edges {
            let from = edge.from_node.index();
            let to = edge.to_node.index();

            // Skip edges not in active set or involving deleted nodes
            if from >= n || to >= n || nodes[from].deleted || nodes[to].deleted {
                continue;
            }

            if active_set[from] && active_set[to] {
                adj[from].push(to);
                in_degree[to] += 1;
            }
        }

        // Kahn's algorithm
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        // Add all active nodes with in-degree 0
        for i in 0..n {
            if active_set[i] && !nodes[i].deleted && in_degree[i] == 0 {
                queue.push_back(i);
            }
        }

        while let Some(node) = queue.pop_front() {
            result.push(node);

            for &neighbor in &adj[node] {
                in_degree[neighbor] -= 1;
                if in_degree[neighbor] == 0 {
                    queue.push_back(neighbor);
                }
            }
        }

        result
    }

    /// Filter edges to only include active → active connections
    fn filter_active_edges(edges: &[Edge], active_set: &[bool]) -> Vec<(usize, usize)> {
        edges
            .iter()
            .filter_map(|edge| {
                let from = edge.from_node.index();
                let to = edge.to_node.index();

                if from < active_set.len()
                    && to < active_set.len()
                    && active_set[from]
                    && active_set[to]
                {
                    Some((from, to))
                } else {
                    None
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::node::{AnyNode, BuiltinNode};
    use crate::pipeline::nodes::{FilterNode, GraphSinkNode, RecorderSinkNode, RhaiScriptNode};
    use crossbeam_channel::bounded;

    fn create_test_slot(node: AnyNode) -> NodeSlot {
        NodeSlot {
            node,
            deleted: false,
            input_buf: Default::default(),
            output_buf: Default::default(),
            input_events: Vec::new(),
            output_events: Vec::new(),
        }
    }

    #[test]
    fn test_compile_disconnected_sink() {
        // Graph: [GraphSink1] (connected to nothing), [GraphSink2] (connected to nothing)
        // Without sources or connections, no sinks should be active
        let (msg_tx, _msg_rx) = bounded(16);
        let nodes = vec![
            create_test_slot(AnyNode::Builtin(BuiltinNode::GraphSink(
                GraphSinkNode::new(msg_tx.clone(), None),
            ))),
            create_test_slot(AnyNode::Builtin(BuiltinNode::RecorderSink(
                RecorderSinkNode::new(),
            ))),
        ];

        let edges = vec![];

        let plan = PipelineCompiler::compile(&nodes, &edges, 1);

        // No sources exist, so no sinks can receive data - all should be inactive
        assert_eq!(plan.stats.total_nodes, 2);
        assert_eq!(plan.stats.active_nodes, 0);
        assert_eq!(plan.stats.disconnected_nodes, 2);
        assert_eq!(plan.stats.source_nodes, 0);
        assert_eq!(plan.stats.sink_nodes, 2);
    }

    #[test]
    fn test_compile_transform_no_sink() {
        // Graph: [Filter] → [Script] → (nothing)
        let nodes = vec![
            create_test_slot(AnyNode::Builtin(BuiltinNode::Filter(FilterNode::new()))),
            create_test_slot(AnyNode::Builtin(BuiltinNode::RhaiScript(
                RhaiScriptNode::new(),
            ))),
        ];

        let edges = vec![Edge {
            id: crate::pipeline::id::EdgeId(0),
            from_node: crate::pipeline::id::NodeId(0),
            to_node: crate::pipeline::id::NodeId(1),
        }];

        let plan = PipelineCompiler::compile(&nodes, &edges, 1);

        // No sources (Filter has input port) and no sinks - nothing active
        assert_eq!(plan.stats.total_nodes, 2);
        assert_eq!(plan.stats.active_nodes, 0);
        assert_eq!(plan.stats.source_nodes, 0);
        assert_eq!(plan.stats.sink_nodes, 0);
    }

    #[test]
    fn test_compile_only_sinks() {
        // Graph: [FilterA] → [ScriptA] → [SinkA], [FilterB] → [ScriptB] → [SinkB]
        // Filter nodes are not sources (have input ports), so nothing should be active
        let (msg_tx, _msg_rx) = bounded(16);
        let nodes = vec![
            create_test_slot(AnyNode::Builtin(BuiltinNode::Filter(FilterNode::new()))), // 0
            create_test_slot(AnyNode::Builtin(BuiltinNode::RhaiScript(
                RhaiScriptNode::new(),
            ))), // 1
            create_test_slot(AnyNode::Builtin(BuiltinNode::GraphSink(
                GraphSinkNode::new(msg_tx.clone(), None),
            ))), // 2
            create_test_slot(AnyNode::Builtin(BuiltinNode::Filter(FilterNode::new()))), // 3
            create_test_slot(AnyNode::Builtin(BuiltinNode::RhaiScript(
                RhaiScriptNode::new(),
            ))), // 4
            create_test_slot(AnyNode::Builtin(BuiltinNode::GraphSink(
                GraphSinkNode::new(msg_tx.clone(), None),
            ))), // 5
        ];

        let edges = vec![
            Edge {
                id: crate::pipeline::id::EdgeId(0),
                from_node: crate::pipeline::id::NodeId(0),
                to_node: crate::pipeline::id::NodeId(1),
            },
            Edge {
                id: crate::pipeline::id::EdgeId(1),
                from_node: crate::pipeline::id::NodeId(1),
                to_node: crate::pipeline::id::NodeId(2),
            },
            Edge {
                id: crate::pipeline::id::EdgeId(2),
                from_node: crate::pipeline::id::NodeId(3),
                to_node: crate::pipeline::id::NodeId(4),
            },
            Edge {
                id: crate::pipeline::id::EdgeId(3),
                from_node: crate::pipeline::id::NodeId(4),
                to_node: crate::pipeline::id::NodeId(5),
            },
        ];

        let plan = PipelineCompiler::compile(&nodes, &edges, 1);

        // Filter nodes are not sources (have input ports), so without real sources
        // no nodes should be active even though sinks are present
        assert_eq!(plan.stats.total_nodes, 6);
        assert_eq!(plan.stats.active_nodes, 0);
        assert_eq!(plan.stats.disconnected_nodes, 6);
        assert_eq!(plan.stats.source_nodes, 0);
        assert_eq!(plan.stats.sink_nodes, 2); // Two GraphSinks
    }

    #[test]
    fn test_cache_invalidation() {
        let nodes = vec![create_test_slot(AnyNode::Builtin(BuiltinNode::Filter(
            FilterNode::new(),
        )))];

        let edges = vec![];

        let plan1 = PipelineCompiler::compile(&nodes, &edges, 1);
        assert_eq!(plan1.generation, 1);

        let plan2 = PipelineCompiler::compile(&nodes, &edges, 2);
        assert_eq!(plan2.generation, 2);
    }
}
