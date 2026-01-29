/// Compiled execution plan for a pipeline graph.
/// Contains only active nodes (nodes participating in data flow from sources to sinks).
#[derive(Debug, Clone)]
pub struct CompiledPlan {
    /// Active node indices in topological order
    pub active_nodes: Vec<usize>,

    /// Pre-computed edge routing (from_idx, to_idx)
    pub active_edges: Vec<(usize, usize)>,

    /// Cache invalidation generation number
    pub generation: u64,

    /// Compilation statistics
    pub stats: PlanStats,

    /// Sink nodes that are not in active_nodes (disconnected from sources)
    pub inactive_sink_nodes: Vec<usize>,
}

/// Statistics about the compiled plan
#[derive(Debug, Clone, Default)]
pub struct PlanStats {
    /// Total number of nodes in the graph (including disconnected)
    pub total_nodes: usize,

    /// Number of active nodes in the execution plan
    pub active_nodes: usize,

    /// Number of disconnected nodes (not in execution plan)
    pub disconnected_nodes: usize,

    /// Number of source nodes (no inputs)
    pub source_nodes: usize,

    /// Number of sink nodes (no outputs)
    pub sink_nodes: usize,

    /// Compilation time in microseconds
    pub compile_time_us: u64,
}

impl CompiledPlan {
    /// Create a new empty compiled plan
    pub fn new() -> Self {
        Self {
            active_nodes: Vec::new(),
            active_edges: Vec::new(),
            generation: 0,
            stats: PlanStats::default(),
            inactive_sink_nodes: Vec::new(),
        }
    }

    /// Check if the plan has any active nodes
    pub fn is_empty(&self) -> bool {
        self.active_nodes.is_empty()
    }
}

impl Default for CompiledPlan {
    fn default() -> Self {
        Self::new()
    }
}
