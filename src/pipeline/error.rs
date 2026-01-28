//! Pipeline-specific error types.

use crate::pipeline::id::NodeId;
use thiserror::Error;

/// Errors that can occur within the pipeline system.
#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("Node {node_id:?} error: {message}")]
    Node { node_id: NodeId, message: String },

    #[error("Invalid edge: {0}")]
    InvalidEdge(String),

    #[error("Cycle detected in pipeline graph")]
    CycleDetected,

    #[error("Port mismatch: {0}")]
    PortMismatch(String),

    #[error("Variable tree error: {0}")]
    VariableTree(String),

    #[error("Probe error: {0}")]
    Probe(String),

    #[error("Script error: {0}")]
    Script(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Channel send error")]
    ChannelSend,

    #[error("Channel receive error")]
    ChannelRecv,

    #[error("Pipeline not active")]
    NotActive,
}

pub type PipelineResult<T> = std::result::Result<T, PipelineError>;
