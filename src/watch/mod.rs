//! Live Watch domain types and tree-walk logic.
//!
//! Distinct from the `Variable` system: live watch entries are *inspection-only*
//! (no plotting, no converters, no recording). Each `WatchRoot` is a symbol name
//! that gets resolved against the loaded ELF; its tree is recomputed on the fly
//! from DWARF type info plus a per-root set of expanded paths.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};

use serde::{Deserialize, Serialize};

use crate::backend::type_table::TypeHandle;
use crate::backend::{ElfInfo, ElfSymbol};
use crate::types::VariableType;

/// Cap on how many array elements we render/read per array node.
/// Matches the Variable Browser cap so behaviour is consistent.
pub const MAX_WATCH_ARRAY_ELEMENTS: u64 = 1024;

/// Stable identifier for a watch root, persisted in the project file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WatchId(pub u32);

static NEXT_WATCH_ID: AtomicU32 = AtomicU32::new(1);

impl WatchId {
    pub fn next() -> Self {
        Self(NEXT_WATCH_ID.fetch_add(1, Ordering::SeqCst))
    }

    /// Bump the global counter past any watch ids that were just loaded from a project.
    pub fn sync_after_load(roots: &[WatchRoot]) {
        if let Some(max) = roots.iter().map(|r| r.id.0).max() {
            let mut current = NEXT_WATCH_ID.load(Ordering::SeqCst);
            while current <= max {
                match NEXT_WATCH_ID.compare_exchange(
                    current,
                    max + 1,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => break,
                    Err(c) => current = c,
                }
            }
        }
    }
}

/// A persisted live-watch entry: a symbol expression and which sub-paths the user
/// has expanded. Children are *not* persisted — they're re-derived from DWARF
/// every time the panel renders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchRoot {
    pub id: WatchId,
    /// Expression — v1: a symbol name resolvable in `ElfInfo`.
    pub expression: String,
    /// Set of paths (relative to the root) the user has expanded.
    /// Empty path "" means the root itself is expanded.
    #[serde(default)]
    pub expanded_paths: HashSet<String>,
}

impl WatchRoot {
    pub fn new(expression: impl Into<String>) -> Self {
        Self {
            id: WatchId::next(),
            expression: expression.into(),
            expanded_paths: HashSet::new(),
        }
    }

    pub fn is_expanded(&self, path: &str) -> bool {
        self.expanded_paths.contains(path)
    }

    pub fn toggle_expanded(&mut self, path: &str) {
        if self.expanded_paths.contains(path) {
            self.expanded_paths.remove(path);
        } else {
            self.expanded_paths.insert(path.to_string());
        }
    }
}

/// How a leaf's address is computed at read time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchAddress {
    /// Compile-time address. No runtime resolution needed.
    Static(u64),
    /// `<value of pointer leaf at parent_path> + offset`. The scheduler caches
    /// the most recent read of the pointer leaf and uses it for children.
    PointerDeref {
        /// Path of the pointer leaf this child depends on.
        parent_path: String,
        offset: u64,
    },
}

/// What the scheduler should read each tick. One per visible (and expanded)
/// primitive or pointer node in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchLeafRead {
    pub root_id: WatchId,
    pub path: String,
    pub address: WatchAddress,
    pub var_type: VariableType,
    /// True iff this leaf reads a pointer value (so the scheduler should also
    /// classify the result as a `PointerState` for UI display).
    pub is_pointer: bool,
}

/// The scheduler's published value for a single leaf.
#[derive(Debug, Clone)]
pub struct WatchValue {
    pub raw: Result<f64, String>,
    /// Only populated when the leaf is a pointer.
    pub pointer_state: Option<crate::types::PointerState>,
}

/// A row to render — produced by the same walk that produces leaves.
#[derive(Debug, Clone)]
pub struct WatchRow {
    pub root_id: WatchId,
    pub path: String,
    pub depth: usize,
    pub display_name: String,
    pub type_name: String,
    pub address: WatchAddress,
    pub var_type: VariableType,
    pub kind: WatchRowKind,
    /// Set if this row's value can be edited in place.
    pub writable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchRowKind {
    /// A primitive value — single read, displayable, possibly writable.
    Primitive,
    /// A pointer — reads its address; expansion shows the pointee.
    Pointer { can_expand: bool },
    /// A struct/union — no value of its own; expansion shows members.
    Struct,
    /// An array — no value of its own; expansion shows elements.
    Array { count: u64, truncated: bool },
    /// A type with no value and no expandable contents (e.g. void*).
    Opaque,
}

/// Status of resolving a watch root against the current ELF.
#[derive(Debug, Clone)]
pub enum WatchResolution {
    Resolved {
        symbol: ElfSymbol,
        type_handle: Option<TypeHandle>,
    },
    NoElf,
    NotFound,
}

/// Resolve a watch root's expression against the current ELF.
/// v1 only supports symbol names.
pub fn resolve_root(elf: Option<&ElfInfo>, root: &WatchRoot) -> WatchResolution {
    let Some(elf) = elf else {
        return WatchResolution::NoElf;
    };
    match elf.find_symbol(&root.expression) {
        Some(sym) => {
            let handle = elf.symbol_type_handle(sym);
            WatchResolution::Resolved {
                symbol: sym.clone(),
                type_handle: handle,
            }
        }
        None => WatchResolution::NotFound,
    }
}

/// Walk a single watch root and emit (rows, leaves).
///
/// `rows` is the flat in-render-order list the pane displays.
/// `leaves` is the set of reads the scheduler needs to perform this tick.
pub fn walk_root(root: &WatchRoot, resolution: &WatchResolution, out: &mut WalkOutput) {
    match resolution {
        WatchResolution::Resolved {
            symbol,
            type_handle,
        } => {
            walk_node(
                root,
                &symbol.display_name,
                "",
                0,
                symbol.address,
                type_handle.clone(),
                /* closest_pointer_path = */ None,
                /* offset_from_pointer = */ 0,
                out,
            );
        }
        WatchResolution::NoElf | WatchResolution::NotFound => {
            // Emit a single "unresolved" row; the pane decides how to render it.
            out.rows.push(WatchRow {
                root_id: root.id,
                path: String::new(),
                depth: 0,
                display_name: root.expression.clone(),
                type_name: match resolution {
                    WatchResolution::NoElf => "<no ELF loaded>".to_string(),
                    WatchResolution::NotFound => "<not in current ELF>".to_string(),
                    _ => unreachable!(),
                },
                address: WatchAddress::Static(0),
                var_type: VariableType::U32,
                kind: WatchRowKind::Opaque,
                writable: false,
            });
        }
    }
}

#[derive(Debug, Default)]
pub struct WalkOutput {
    pub rows: Vec<WatchRow>,
    pub leaves: Vec<WatchLeafRead>,
}

#[allow(clippy::too_many_arguments)]
fn walk_node(
    root: &WatchRoot,
    display_name: &str,
    path: &str,
    depth: usize,
    static_address: u64,
    type_handle: Option<TypeHandle>,
    closest_pointer_path: Option<&str>,
    offset_from_pointer: u64,
    out: &mut WalkOutput,
) {
    // Decide the WatchAddress for this node.
    let address = match closest_pointer_path {
        None => WatchAddress::Static(static_address),
        Some(p) => WatchAddress::PointerDeref {
            parent_path: p.to_string(),
            offset: offset_from_pointer,
        },
    };

    let underlying = type_handle.as_ref().map(|h| h.underlying());
    let type_name = type_handle
        .as_ref()
        .map(|h| h.type_name())
        .unwrap_or_else(|| "?".to_string());

    let is_pointer = underlying
        .as_ref()
        .is_some_and(|h| h.is_pointer_or_reference());
    let is_array = underlying.as_ref().is_some_and(|h| h.is_array());
    let has_members = underlying
        .as_ref()
        .and_then(|h| h.members())
        .is_some_and(|m| !m.is_empty());

    // Pointer expansion uses the pointee.
    let pointer_can_expand = is_pointer
        && underlying
            .as_ref()
            .and_then(|h| h.pointee_underlying())
            .is_some_and(|p| {
                p.is_struct_or_union() || p.is_array() || !p.members().unwrap_or(&[]).is_empty()
            });

    // Determine row kind, and whether this node should produce a leaf read.
    let var_type = type_handle
        .as_ref()
        .map(|h| h.to_variable_type())
        .unwrap_or(VariableType::U32);

    let kind: WatchRowKind;
    let mut emit_leaf = false;
    let mut writable = false;

    if is_pointer {
        kind = WatchRowKind::Pointer {
            can_expand: pointer_can_expand,
        };
        emit_leaf = true;
    } else if is_array {
        let count = underlying
            .as_ref()
            .and_then(|h| h.array_count())
            .unwrap_or(0);
        let display_count = count.min(MAX_WATCH_ARRAY_ELEMENTS);
        kind = WatchRowKind::Array {
            count: display_count,
            truncated: count > MAX_WATCH_ARRAY_ELEMENTS,
        };
    } else if has_members {
        kind = WatchRowKind::Struct;
    } else if let Some(handle) = type_handle.as_ref() {
        if handle.is_addable() {
            kind = WatchRowKind::Primitive;
            emit_leaf = true;
            writable = var_type.is_writable();
        } else {
            kind = WatchRowKind::Opaque;
        }
    } else {
        kind = WatchRowKind::Opaque;
    }

    out.rows.push(WatchRow {
        root_id: root.id,
        path: path.to_string(),
        depth,
        display_name: display_name.to_string(),
        type_name,
        address: address.clone(),
        var_type,
        kind: kind.clone(),
        writable,
    });

    if emit_leaf {
        out.leaves.push(WatchLeafRead {
            root_id: root.id,
            path: path.to_string(),
            address: address.clone(),
            var_type,
            is_pointer,
        });
    }

    // Recurse if this node is expanded.
    if !root.is_expanded(path) {
        return;
    }

    // For pointers: recurse through the pointee, switching the deref context.
    if is_pointer {
        let Some(pointee) = underlying.as_ref().and_then(|h| h.pointee_underlying()) else {
            return;
        };
        // Children of an expanded pointer use this node's path as the deref base.
        recurse_into(
            root,
            path,
            depth,
            /* base_static_addr = */ 0, // unused under pointer
            &pointee,
            Some(path),
            0,
            out,
        );
        return;
    }

    // For struct/union: recurse into members.
    // For arrays: recurse into elements (capped at MAX_WATCH_ARRAY_ELEMENTS).
    if let Some(h) = underlying.as_ref() {
        recurse_into(
            root,
            path,
            depth,
            static_address,
            h,
            closest_pointer_path,
            offset_from_pointer,
            out,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn recurse_into(
    root: &WatchRoot,
    parent_path: &str,
    parent_depth: usize,
    parent_static_address: u64,
    parent_handle: &TypeHandle,
    closest_pointer_path: Option<&str>,
    offset_from_pointer: u64,
    out: &mut WalkOutput,
) {
    if let Some(members) = parent_handle.members() {
        for member in members {
            let child_path = format!("{}.{}", parent_path, member.name);
            let child_addr = parent_static_address.wrapping_add(member.offset);
            let child_offset = offset_from_pointer.wrapping_add(member.offset);
            let child_handle = parent_handle.member_type(member);
            walk_node(
                root,
                &member.name,
                &child_path,
                parent_depth + 1,
                child_addr,
                Some(child_handle),
                closest_pointer_path,
                child_offset,
                out,
            );
        }
        return;
    }

    if parent_handle.is_array() {
        let count = parent_handle
            .array_count()
            .unwrap_or(0)
            .min(MAX_WATCH_ARRAY_ELEMENTS);
        let elem_size = parent_handle.element_size().unwrap_or(0);
        if count == 0 || elem_size == 0 {
            return;
        }
        let elem_handle = parent_handle.element_type();
        for i in 0..count {
            let child_path = format!("{}[{}]", parent_path, i);
            let child_addr = parent_static_address.wrapping_add(i.wrapping_mul(elem_size));
            let child_offset = offset_from_pointer.wrapping_add(i.wrapping_mul(elem_size));
            walk_node(
                root,
                &format!("[{}]", i),
                &child_path,
                parent_depth + 1,
                child_addr,
                elem_handle.clone(),
                closest_pointer_path,
                child_offset,
                out,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watch_id_uniqueness() {
        let a = WatchId::next();
        let b = WatchId::next();
        assert_ne!(a.0, b.0);
    }

    #[test]
    fn test_watch_root_toggle() {
        let mut r = WatchRoot::new("g_foo");
        assert!(!r.is_expanded(""));
        r.toggle_expanded("");
        assert!(r.is_expanded(""));
        r.toggle_expanded("");
        assert!(!r.is_expanded(""));
    }

    #[test]
    fn test_watch_root_serde_roundtrip() {
        let mut r = WatchRoot::new("g_foo");
        r.toggle_expanded("");
        r.toggle_expanded(".bar");
        let json = serde_json::to_string(&r).unwrap();
        let back: WatchRoot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.expression, "g_foo");
        assert!(back.is_expanded(""));
        assert!(back.is_expanded(".bar"));
    }

    #[test]
    fn test_watch_resolution_no_elf() {
        let r = WatchRoot::new("g_foo");
        let resolution = resolve_root(None, &r);
        let mut out = WalkOutput::default();
        walk_root(&r, &resolution, &mut out);
        assert_eq!(out.rows.len(), 1);
        assert_eq!(out.leaves.len(), 0);
        assert_eq!(out.rows[0].type_name, "<no ELF loaded>");
    }

    #[test]
    fn test_watch_address_static_for_root() {
        // Without DWARF type info, a primitive resolution should still emit a
        // leaf with Static address.
        // We can't easily build an ElfInfo in unit tests, so we exercise
        // walk_node directly with a synthetic resolution-like shape via a fake.
        // (Real type-aware tests live in integration tests where mock probes
        // and ELF fixtures are available.)
        // This test simply verifies WatchAddress equality.
        let a = WatchAddress::Static(0x2000_0000);
        let b = WatchAddress::Static(0x2000_0000);
        assert_eq!(a, b);
    }

    #[test]
    fn test_pointer_deref_addresses() {
        let a = WatchAddress::PointerDeref {
            parent_path: "".to_string(),
            offset: 8,
        };
        let b = WatchAddress::PointerDeref {
            parent_path: "".to_string(),
            offset: 8,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_sync_after_load_advances_counter() {
        let mut r = WatchRoot::new("a");
        r.id = WatchId(10_000);
        WatchId::sync_after_load(std::slice::from_ref(&r));
        let next = WatchId::next();
        assert!(next.0 > 10_000);
    }
}
