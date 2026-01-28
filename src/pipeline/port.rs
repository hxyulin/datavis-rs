//! Port descriptors for the node system.
//!
//! Each node declares its ports (inputs/outputs) via static `PortDescriptor` arrays.
//! The pipeline uses these to validate edge connections.

/// The kind of data flowing through a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortKind {
    /// Continuous data stream (samples per tick).
    DataStream,
    /// Discrete events (triggers, state changes).
    Event,
    /// Configuration values (changed infrequently).
    Config,
}

/// Whether a port is an input or output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDirection {
    Input,
    Output,
}

/// Static descriptor for a node's port.
#[derive(Debug, Clone)]
pub struct PortDescriptor {
    pub name: &'static str,
    pub direction: PortDirection,
    pub kind: PortKind,
}

impl PortDescriptor {
    pub const fn input(name: &'static str, kind: PortKind) -> Self {
        Self {
            name,
            direction: PortDirection::Input,
            kind,
        }
    }

    pub const fn output(name: &'static str, kind: PortKind) -> Self {
        Self {
            name,
            direction: PortDirection::Output,
            kind,
        }
    }
}
