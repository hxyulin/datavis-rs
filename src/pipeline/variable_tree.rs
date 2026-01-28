//! Hierarchical variable tree for the pipeline.
//!
//! Replaces the flat `Vec<Variable>` + `HashMap<u32, VariableData>` with a tree
//! that supports struct/array decomposition. Variables are stored in a flat `Vec`
//! indexed by `VarId`, with parent/child/sibling links forming an intrusive tree.
//!
//! ## Auto-decomposition
//!
//! When a user adds a struct variable (e.g., `pid_state` of type `PidController`),
//! `decompose_type` walks the DWARF type info and creates child nodes:
//!
//! ```text
//! pid_state            (branch, addr=0x2000_0100)
//! +-- pid_state.kp     (leaf f32, addr=0x2000_0100)
//! +-- pid_state.ki     (leaf f32, addr=0x2000_0104)
//! +-- pid_state.kd     (leaf f32, addr=0x2000_0108)
//! +-- pid_state.output (leaf f32, addr=0x2000_010C)
//! ```

use crate::pipeline::id::VarId;
use crate::types::VariableType;
use std::collections::HashMap;

use crate::backend::type_table::{TypeDef, TypeId, TypeTable};

/// A single node in the variable tree.
#[derive(Debug, Clone)]
pub struct VariableNode {
    pub id: VarId,
    /// Full dotted path, e.g. `"pid_state.gains.kp"`.
    pub name: String,
    /// Leaf segment only, e.g. `"kp"`.
    pub short_name: String,
    /// Memory address on the target.
    pub address: u64,
    /// Data type for reading/parsing.
    pub var_type: VariableType,
    /// Parent node (VarId::INVALID for roots).
    pub parent: VarId,
    /// First child (intrusive linked list).
    pub first_child: VarId,
    /// Next sibling (intrusive linked list).
    pub next_sibling: VarId,
    /// Depth in the tree (0 for roots).
    pub depth: u16,
    /// True if this node is directly readable from the probe (no children, or primitive).
    pub is_leaf: bool,
    /// Whether this variable is currently enabled for polling.
    pub enabled: bool,
    /// Optional converter script source.
    pub converter_script: Option<String>,
    /// Display color (RGBA).
    pub color: [u8; 4],
    /// Unit label.
    pub unit: String,
}

impl VariableNode {
    fn new_root(id: VarId, name: String, address: u64, var_type: VariableType) -> Self {
        Self {
            id,
            short_name: name.clone(),
            name,
            address,
            var_type,
            parent: VarId::INVALID,
            first_child: VarId::INVALID,
            next_sibling: VarId::INVALID,
            depth: 0,
            is_leaf: true,
            enabled: false,
            converter_script: None,
            color: [128, 128, 128, 255],
            unit: String::new(),
        }
    }
}

/// Flat-storage hierarchical variable tree.
///
/// - `VarId` is a direct index into `nodes`.
/// - Name and address lookups are O(1) via HashMap.
/// - Tree traversal uses intrusive linked lists (first_child / next_sibling).
#[derive(Debug)]
pub struct VariableTree {
    nodes: Vec<VariableNode>,
    name_index: HashMap<String, VarId>,
    address_index: HashMap<u64, VarId>,
}

impl Default for VariableTree {
    fn default() -> Self {
        Self::new()
    }
}

impl VariableTree {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            name_index: HashMap::new(),
            address_index: HashMap::new(),
        }
    }

    /// Total number of nodes in the tree.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Add a root variable (no parent).
    pub fn add_root(
        &mut self,
        name: String,
        address: u64,
        var_type: VariableType,
    ) -> VarId {
        let id = VarId(self.nodes.len() as u32);
        let node = VariableNode::new_root(id, name.clone(), address, var_type);
        self.name_index.insert(name, id);
        if node.is_leaf {
            self.address_index.insert(address, id);
        }
        self.nodes.push(node);
        id
    }

    /// Add a child node under `parent`. Returns the child's VarId.
    pub fn add_child(
        &mut self,
        parent: VarId,
        short_name: String,
        address: u64,
        var_type: VariableType,
    ) -> VarId {
        let id = VarId(self.nodes.len() as u32);
        let parent_name = self.nodes[parent.index()].name.clone();
        let parent_depth = self.nodes[parent.index()].depth;
        let full_name = format!("{}.{}", parent_name, short_name);

        let node = VariableNode {
            id,
            name: full_name.clone(),
            short_name,
            address,
            var_type,
            parent,
            first_child: VarId::INVALID,
            next_sibling: VarId::INVALID,
            depth: parent_depth + 1,
            is_leaf: true,
            enabled: false,
            converter_script: None,
            color: [128, 128, 128, 255],
            unit: String::new(),
        };

        self.name_index.insert(full_name, id);
        self.address_index.insert(address, id);
        self.nodes.push(node);

        // Link into parent's child list
        let first = self.nodes[parent.index()].first_child;
        if !first.is_valid() {
            self.nodes[parent.index()].first_child = id;
        } else {
            // Walk to end of sibling chain
            let mut cur = first;
            loop {
                let next = self.nodes[cur.index()].next_sibling;
                if !next.is_valid() {
                    self.nodes[cur.index()].next_sibling = id;
                    break;
                }
                cur = next;
            }
        }

        // Parent is no longer a leaf
        self.nodes[parent.index()].is_leaf = false;

        id
    }

    /// Get a node by VarId (O(1) array index).
    #[inline]
    pub fn get(&self, id: VarId) -> Option<&VariableNode> {
        if id.is_valid() {
            self.nodes.get(id.index())
        } else {
            None
        }
    }

    /// Get a mutable reference to a node.
    #[inline]
    pub fn get_mut(&mut self, id: VarId) -> Option<&mut VariableNode> {
        if id.is_valid() {
            self.nodes.get_mut(id.index())
        } else {
            None
        }
    }

    /// Look up by full dotted path (O(1) HashMap).
    pub fn find_by_name(&self, name: &str) -> Option<VarId> {
        self.name_index.get(name).copied()
    }

    /// Look up a leaf by memory address (O(1) HashMap).
    pub fn find_by_address(&self, address: u64) -> Option<VarId> {
        self.address_index.get(&address).copied()
    }

    /// Iterate over all nodes.
    pub fn iter(&self) -> impl Iterator<Item = &VariableNode> {
        self.nodes.iter()
    }

    /// Iterate over children of a given node.
    pub fn children(&self, parent: VarId) -> ChildIter<'_> {
        let first = self
            .get(parent)
            .map(|n| n.first_child)
            .unwrap_or(VarId::INVALID);
        ChildIter {
            tree: self,
            current: first,
        }
    }

    /// Iterate over root nodes (depth == 0).
    pub fn roots(&self) -> impl Iterator<Item = &VariableNode> {
        self.nodes.iter().filter(|n| !n.parent.is_valid())
    }

    /// Iterate over enabled leaf nodes (the ones we actually poll).
    pub fn enabled_leaves(&self) -> impl Iterator<Item = &VariableNode> {
        self.nodes.iter().filter(|n| n.is_leaf && n.enabled)
    }

    /// Iterate over all leaf nodes.
    pub fn leaves(&self) -> impl Iterator<Item = &VariableNode> {
        self.nodes.iter().filter(|n| n.is_leaf)
    }

    /// Remove a variable and all its descendants.
    /// Returns the list of removed VarIds.
    pub fn remove(&mut self, id: VarId) -> Vec<VarId> {
        let mut removed = Vec::new();
        self.collect_subtree(id, &mut removed);

        // Unlink from parent
        if let Some(node) = self.get(id) {
            let parent = node.parent;
            if parent.is_valid() {
                let first = self.nodes[parent.index()].first_child;
                if first == id {
                    // Removing first child — update parent's first_child
                    let next = self.nodes[id.index()].next_sibling;
                    self.nodes[parent.index()].first_child = next;
                } else {
                    // Walk sibling chain to find predecessor
                    let mut cur = first;
                    while cur.is_valid() {
                        let next = self.nodes[cur.index()].next_sibling;
                        if next == id {
                            self.nodes[cur.index()].next_sibling =
                                self.nodes[id.index()].next_sibling;
                            break;
                        }
                        cur = next;
                    }
                }
                // Check if parent becomes a leaf
                if !self.nodes[parent.index()].first_child.is_valid() {
                    self.nodes[parent.index()].is_leaf = true;
                }
            }
        }

        // Remove from indexes (don't remove from nodes vec — IDs are indices)
        for &rid in &removed {
            if let Some(node) = self.nodes.get(rid.index()) {
                self.name_index.remove(&node.name);
                if node.is_leaf {
                    self.address_index.remove(&node.address);
                }
            }
        }

        removed
    }

    fn collect_subtree(&self, id: VarId, out: &mut Vec<VarId>) {
        if !id.is_valid() {
            return;
        }
        out.push(id);
        let mut child = self.nodes.get(id.index()).map(|n| n.first_child).unwrap_or(VarId::INVALID);
        while child.is_valid() {
            self.collect_subtree(child, out);
            child = self.nodes[child.index()].next_sibling;
        }
    }

    /// Auto-decompose a struct/array type from the TypeTable into child nodes.
    ///
    /// Walks the DWARF type definition recursively, creating tree nodes for each
    /// member/element. Only leaves (primitives, enums, pointers) are readable.
    pub fn decompose_type(
        &mut self,
        parent: VarId,
        type_table: &TypeTable,
        type_id: TypeId,
        base_address: u64,
    ) {
        self.decompose_type_inner(parent, type_table, type_id, base_address, 0);
    }

    fn decompose_type_inner(
        &mut self,
        parent: VarId,
        type_table: &TypeTable,
        type_id: TypeId,
        base_address: u64,
        depth: usize,
    ) {
        // Guard against infinite recursion
        if depth > 32 {
            return;
        }

        let resolved_id = type_table.resolve(type_id);
        let Some(type_def) = type_table.get(resolved_id) else {
            return;
        };

        match type_def {
            TypeDef::Struct(s) | TypeDef::Union(s) => {
                let members = s.members.clone();
                for member in &members {
                    let member_addr = base_address + member.offset;
                    let member_type = type_table.to_variable_type(member.type_id);
                    let child_id =
                        self.add_child(parent, member.name.clone(), member_addr, member_type);

                    // Recursively decompose if the member is itself a struct/union/array
                    if type_table.is_expandable(member.type_id) {
                        self.decompose_type_inner(
                            child_id,
                            type_table,
                            member.type_id,
                            member_addr,
                            depth + 1,
                        );
                    }
                }
            }
            TypeDef::Array { element, count } => {
                let count = count.unwrap_or(0);
                let elem_size = type_table.type_size(*element).unwrap_or(0);
                let element = *element;
                for i in 0..count.min(256) {
                    // Cap at 256 elements
                    let elem_addr = base_address + i * elem_size;
                    let elem_type = type_table.to_variable_type(element);
                    let child_id =
                        self.add_child(parent, format!("[{}]", i), elem_addr, elem_type);

                    if type_table.is_expandable(element) {
                        self.decompose_type_inner(
                            child_id,
                            type_table,
                            element,
                            elem_addr,
                            depth + 1,
                        );
                    }
                }
            }
            TypeDef::Typedef { underlying, .. }
            | TypeDef::Const(underlying)
            | TypeDef::Volatile(underlying)
            | TypeDef::Restrict(underlying) => {
                let underlying = *underlying;
                self.decompose_type_inner(parent, type_table, underlying, base_address, depth + 1);
            }
            TypeDef::ForwardDecl {
                target: Some(target),
                ..
            } => {
                let target = *target;
                self.decompose_type_inner(parent, type_table, target, base_address, depth + 1);
            }
            // Primitives, enums, pointers — these are leaves, nothing to decompose.
            _ => {}
        }
    }
}

/// Iterator over the children of a node.
pub struct ChildIter<'a> {
    tree: &'a VariableTree,
    current: VarId,
}

impl<'a> Iterator for ChildIter<'a> {
    type Item = &'a VariableNode;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.current.is_valid() {
            return None;
        }
        let node = &self.tree.nodes[self.current.index()];
        self.current = node.next_sibling;
        Some(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::type_table::{MemberDef, PrimitiveDef, StructDef};

    #[test]
    fn test_add_root() {
        let mut tree = VariableTree::new();
        let id = tree.add_root("counter".into(), 0x2000_0000, VariableType::U32);

        assert_eq!(tree.len(), 1);
        let node = tree.get(id).unwrap();
        assert_eq!(node.name, "counter");
        assert_eq!(node.address, 0x2000_0000);
        assert!(node.is_leaf);
        assert!(!node.parent.is_valid());
    }

    #[test]
    fn test_add_child() {
        let mut tree = VariableTree::new();
        let root = tree.add_root("pid".into(), 0x2000_0100, VariableType::Raw(16));
        let kp = tree.add_child(root, "kp".into(), 0x2000_0100, VariableType::F32);
        let ki = tree.add_child(root, "ki".into(), 0x2000_0104, VariableType::F32);

        assert_eq!(tree.len(), 3);
        assert!(!tree.get(root).unwrap().is_leaf);
        assert!(tree.get(kp).unwrap().is_leaf);

        let children: Vec<_> = tree.children(root).collect();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].short_name, "kp");
        assert_eq!(children[1].short_name, "ki");

        // Full path
        assert_eq!(tree.get(kp).unwrap().name, "pid.kp");
        assert_eq!(tree.get(ki).unwrap().name, "pid.ki");
    }

    #[test]
    fn test_find_by_name() {
        let mut tree = VariableTree::new();
        let root = tree.add_root("state".into(), 0x100, VariableType::Raw(8));
        tree.add_child(root, "x".into(), 0x100, VariableType::F32);
        tree.add_child(root, "y".into(), 0x104, VariableType::F32);

        assert!(tree.find_by_name("state.x").is_some());
        assert!(tree.find_by_name("state.y").is_some());
        assert!(tree.find_by_name("state.z").is_none());
    }

    #[test]
    fn test_find_by_address() {
        let mut tree = VariableTree::new();
        tree.add_root("counter".into(), 0x2000_0000, VariableType::U32);

        assert!(tree.find_by_address(0x2000_0000).is_some());
        assert!(tree.find_by_address(0xDEAD).is_none());
    }

    #[test]
    fn test_enabled_leaves() {
        let mut tree = VariableTree::new();
        let a = tree.add_root("a".into(), 0x100, VariableType::U32);
        let b = tree.add_root("b".into(), 0x104, VariableType::U32);

        tree.get_mut(a).unwrap().enabled = true;
        // b stays disabled

        let enabled: Vec<_> = tree.enabled_leaves().collect();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "a");

        let _ = b; // suppress unused
    }

    #[test]
    fn test_decompose_struct() {
        let mut type_table = TypeTable::new();

        // Create primitive types
        let f32_id = type_table.insert(TypeDef::Primitive(PrimitiveDef::Float));

        // Create a struct: PidController { kp: f32, ki: f32, kd: f32, output: f32 }
        let mut pid_def = StructDef::new(Some("PidController".into()), 16, false);
        pid_def
            .members
            .push(MemberDef::new("kp".into(), 0, f32_id));
        pid_def
            .members
            .push(MemberDef::new("ki".into(), 4, f32_id));
        pid_def
            .members
            .push(MemberDef::new("kd".into(), 8, f32_id));
        pid_def
            .members
            .push(MemberDef::new("output".into(), 12, f32_id));
        let pid_type_id = type_table.insert(TypeDef::Struct(pid_def));

        // Add root and decompose
        let mut tree = VariableTree::new();
        let root = tree.add_root("pid_state".into(), 0x2000_0100, VariableType::Raw(16));
        tree.decompose_type(root, &type_table, pid_type_id, 0x2000_0100);

        // Should have 5 nodes: root + 4 members
        assert_eq!(tree.len(), 5);

        let children: Vec<_> = tree.children(root).collect();
        assert_eq!(children.len(), 4);
        assert_eq!(children[0].name, "pid_state.kp");
        assert_eq!(children[0].address, 0x2000_0100);
        assert_eq!(children[1].name, "pid_state.ki");
        assert_eq!(children[1].address, 0x2000_0104);
        assert_eq!(children[2].name, "pid_state.kd");
        assert_eq!(children[2].address, 0x2000_0108);
        assert_eq!(children[3].name, "pid_state.output");
        assert_eq!(children[3].address, 0x2000_010C);

        // Root should no longer be a leaf
        assert!(!tree.get(root).unwrap().is_leaf);
        // Children should be leaves
        assert!(children.iter().all(|c| c.is_leaf));
    }

    #[test]
    fn test_remove_subtree() {
        let mut tree = VariableTree::new();
        let root = tree.add_root("s".into(), 0x100, VariableType::Raw(8));
        tree.add_child(root, "x".into(), 0x100, VariableType::F32);
        tree.add_child(root, "y".into(), 0x104, VariableType::F32);

        let removed = tree.remove(root);
        assert_eq!(removed.len(), 3); // root + 2 children

        // Name index should be cleared
        assert!(tree.find_by_name("s").is_none());
        assert!(tree.find_by_name("s.x").is_none());
    }
}
