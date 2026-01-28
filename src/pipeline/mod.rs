//! Node-based data pipeline architecture.
//!
//! Data flows through typed nodes: Source (probes) → Transform (scripts, filters)
//! → Sink (graphs, recorder, exporter). The pipeline runs on a dedicated thread
//! and communicates with the UI via crossbeam channels.
//!
//! # Architecture
//!
//! ```text
//! [ProbeSource] ──► [ScriptTransform] ──► [UiSink]
//!                                    ├──► [RecorderSink]
//!                                    └──► [ExporterSink]
//! ```
//!
//! # Design
//!
//! - **Enum dispatch on hot path** — `BuiltinNode` enum for all built-in nodes.
//! - **Zero allocation on hot path** — `DataPacket` is a fixed-size inline buffer.
//! - **Variable tree** — flat `Vec<VariableNode>` with `VarId` as array index.
//! - **Dedicated thread** — pipeline runs independently, sinks push via channel.
//! - **Backward compatible** — `PipelineBridge` has same API as `FrontendReceiver`.

pub mod bridge;
pub mod error;
pub mod executor;
pub mod id;
pub mod node;
pub mod nodes;
pub mod packet;
pub mod port;
pub mod variable_tree;

pub use bridge::{
    EdgeSnapshot, NodeSnapshot, PipelineBridge, PipelineCommand, SinkMessage, TopologySnapshot,
    VariableNodeSnapshot,
};
pub use error::{PipelineError, PipelineResult};
pub use executor::{Pipeline, PipelineBuilder, PipelineNodeIds};
pub use id::{EdgeId, NodeId, PortId, VarId};
pub use node::{AnyNode, BuiltinNode, NodeContext, NodePlugin};
pub use packet::{ConfigValue, DataPacket, PipelineEvent, Sample, MAX_PACKET_VARS};
pub use port::{PortDescriptor, PortDirection, PortKind};
pub use variable_tree::{VariableNode, VariableTree};
