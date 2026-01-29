//! Communication bridge and shared types for backend/frontend communication.
//!
//! This module has been simplified after the architecture redesign (Phase 3).
//! The node-based pipeline has been replaced with a direct data flow using
//! ConverterEngine in the backend worker.
//!
//! # Remaining Components
//!
//! - **PipelineBridge** - Communication channel between backend and frontend
//! - **SinkMessage** - Messages sent from backend to frontend
//! - **Basic types** - IDs, packets, and variable tree for compatibility

pub mod bridge;
pub mod error;
pub mod id;
pub mod packet;
pub mod variable_tree;

pub use bridge::{
    PipelineBridge, PipelineCommand, SinkMessage,
    VariableNodeSnapshot,
};
pub use error::{PipelineError, PipelineResult};
pub use id::{NodeId, VarId};
pub use packet::{ConfigValue, DataPacket, PipelineEvent, Sample, MAX_PACKET_VARS};
pub use variable_tree::{VariableNode, VariableTree};
