//! Node type enumeration for dynamic node creation.
//!
//! This module defines the types of nodes that can be dynamically created
//! at runtime through the pipeline editor.

use serde::{Deserialize, Serialize};

/// Types of nodes that can be instantiated dynamically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    // Transform nodes
    /// A filter node that can filter variables by ID.
    Filter,
    /// A Rhai script node that executes user-provided scripts on data streams.
    RhaiScript,

    // Sink nodes (dynamically addable)
    /// UI Broadcast sink that broadcasts data to all UI panes for visualization.
    #[serde(alias = "VariableSink")] // For backward compatibility
    UIBroadcastSink,
    /// Recorder sink that records data to a session recording.
    RecorderSink,
    /// Exporter sink that exports data to files (CSV, JSON, etc.).
    ExporterSink,
    /// Graph sink that sends data to a specific graph pane.
    GraphSink,
}

impl NodeType {
    /// Get the display name for this node type.
    pub fn display_name(&self) -> &'static str {
        match self {
            NodeType::Filter => "Filter",
            NodeType::RhaiScript => "Rhai Script",
            NodeType::UIBroadcastSink => "UI Broadcast Sink",
            NodeType::RecorderSink => "Recorder Sink",
            NodeType::ExporterSink => "Exporter Sink",
            NodeType::GraphSink => "Graph Sink",
        }
    }

    /// Get all available node types.
    pub fn all() -> &'static [NodeType] {
        &[
            NodeType::Filter,
            NodeType::RhaiScript,
            NodeType::UIBroadcastSink,
            NodeType::RecorderSink,
            NodeType::ExporterSink,
            NodeType::GraphSink,
        ]
    }

    /// Check if this node type is a sink node.
    pub fn is_sink(&self) -> bool {
        matches!(
            self,
            NodeType::UIBroadcastSink
                | NodeType::RecorderSink
                | NodeType::ExporterSink
                | NodeType::GraphSink
        )
    }

    /// Check if this node type is a transform node.
    pub fn is_transform(&self) -> bool {
        matches!(self, NodeType::Filter | NodeType::RhaiScript)
    }

    /// Get a detailed description of what this node does.
    pub fn description(&self) -> &'static str {
        match self {
            NodeType::Filter =>
                "Filters data by variable ID.\n\
                 Allows only specified variables to pass through.\n\
                 Supports invert mode to block instead of allow.",

            NodeType::RhaiScript =>
                "Executes custom Rhai scripts on data.\n\
                 Transform samples using user code.\n\
                 Has built-in lowpass/highpass filters.",

            NodeType::UIBroadcastSink =>
                "Broadcasts data to all UI panes.\n\
                 Updates variable browser and live views.\n\
                 Use GraphSink for specific panes.",

            NodeType::GraphSink =>
                "Routes data to a specific graph pane.\n\
                 Configure pane_id to link to TimeSeries.\n\
                 Supports multiple independent graphs.",

            NodeType::RecorderSink =>
                "Records data to session files.\n\
                 Captures timestamped samples for playback.\n\
                 Configure sample rate and max frames.",

            NodeType::ExporterSink =>
                "Exports data to CSV/JSON files.\n\
                 Continuous file writing during collection.\n\
                 Choose wide or long format layout.",
        }
    }
}

impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}
