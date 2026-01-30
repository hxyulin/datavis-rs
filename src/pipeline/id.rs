//! Identity types for the pipeline system.
//!
//! All IDs are newtypes over `u32` that serve as direct array indices
//! into their respective storage vectors, providing O(1) lookup.

use std::fmt;

/// Index into `Pipeline::nodes`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const INVALID: NodeId = NodeId(u32::MAX);

    #[inline]
    pub fn is_valid(self) -> bool {
        self != Self::INVALID
    }

    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::INVALID {
            write!(f, "NodeId(INVALID)")
        } else {
            write!(f, "NodeId({})", self.0)
        }
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Compact port identifier. High 20 bits = node index, low 12 bits = port index.
/// Supports up to ~1M nodes with 4096 ports each.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PortId(pub u32);

impl PortId {
    const PORT_BITS: u32 = 12;
    const PORT_MASK: u32 = (1 << Self::PORT_BITS) - 1;

    pub fn new(node: NodeId, port_index: u16) -> Self {
        debug_assert!(port_index < (1 << Self::PORT_BITS) as u16);
        Self((node.0 << Self::PORT_BITS) | (port_index as u32 & Self::PORT_MASK))
    }

    #[inline]
    pub fn node(self) -> NodeId {
        NodeId(self.0 >> Self::PORT_BITS)
    }

    #[inline]
    pub fn port_index(self) -> u16 {
        (self.0 & Self::PORT_MASK) as u16
    }
}

impl fmt::Debug for PortId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PortId(node={}, port={})",
            self.node().0,
            self.port_index()
        )
    }
}

/// Index into `VariableTree::nodes`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct VarId(pub u32);

impl VarId {
    pub const INVALID: VarId = VarId(u32::MAX);

    #[inline]
    pub fn is_valid(self) -> bool {
        self != Self::INVALID
    }

    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Debug for VarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::INVALID {
            write!(f, "VarId(INVALID)")
        } else {
            write!(f, "VarId({})", self.0)
        }
    }
}

impl fmt::Display for VarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Index into `Pipeline::edges`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct EdgeId(pub u32);

impl EdgeId {
    pub const INVALID: EdgeId = EdgeId(u32::MAX);

    #[inline]
    pub fn is_valid(self) -> bool {
        self != Self::INVALID
    }

    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Debug for EdgeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::INVALID {
            write!(f, "EdgeId(INVALID)")
        } else {
            write!(f, "EdgeId({})", self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id() {
        let id = NodeId(42);
        assert!(id.is_valid());
        assert_eq!(id.index(), 42);
        assert!(!NodeId::INVALID.is_valid());
    }

    #[test]
    fn test_port_id_round_trip() {
        let node = NodeId(100);
        let port = PortId::new(node, 7);
        assert_eq!(port.node(), node);
        assert_eq!(port.port_index(), 7);
    }

    #[test]
    fn test_port_id_limits() {
        let node = NodeId((1 << 20) - 1); // Max node
        let port = PortId::new(node, 4095); // Max port
        assert_eq!(port.node(), node);
        assert_eq!(port.port_index(), 4095);
    }

    #[test]
    fn test_var_id() {
        let id = VarId(0);
        assert!(id.is_valid());
        assert_eq!(id.index(), 0);
        assert!(!VarId::INVALID.is_valid());
    }

    #[test]
    fn test_edge_id() {
        let id = EdgeId(5);
        assert!(id.is_valid());
        assert!(!EdgeId::INVALID.is_valid());
    }
}
