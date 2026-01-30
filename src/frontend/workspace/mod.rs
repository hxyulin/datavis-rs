//! Workspace module for dockable pane management
//!
//! Provides the core workspace types: PaneId, PaneKind, Workspace.
//! Uses egui_dock for drag-and-drop docking, tabs, and splits.

pub mod default_layout;
pub mod tab_viewer;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::frontend::pane_registry::{self, PaneKindInfo};
use crate::frontend::pane_trait::Pane;

/// Unique identifier for a pane instance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaneId(pub u64);

static NEXT_PANE_ID: AtomicU64 = AtomicU64::new(1);

impl PaneId {
    pub fn next() -> Self {
        Self(NEXT_PANE_ID.fetch_add(1, Ordering::SeqCst))
    }
}

/// Kind of pane (used for dispatch and menu display)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaneKind {
    // Utility (singletons)
    VariableBrowser,
    VariableList,
    Recorder,
    // Visualizers (multiple instances allowed)
    TimeSeries,
    Watcher,
    FftView,
}

/// Metadata entry for a pane
pub struct PaneEntry {
    pub id: PaneId,
    pub kind: PaneKind,
    pub title: String,
}

/// The workspace holds all dock state, pane data, and the pane registry.
pub struct Workspace {
    pub dock_state: egui_dock::DockState<PaneId>,
    pub pane_states: HashMap<PaneId, Box<dyn Pane>>,
    pub pane_entries: HashMap<PaneId, PaneEntry>,
    registry: HashMap<PaneKind, PaneKindInfo>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    /// Create a new workspace with the pane registry.
    pub fn new() -> Self {
        let registry: HashMap<PaneKind, PaneKindInfo> = pane_registry::build_registry()
            .into_iter()
            .map(|info| (info.kind, info))
            .collect();

        Self {
            dock_state: egui_dock::DockState::new(vec![]),
            pane_states: HashMap::new(),
            pane_entries: HashMap::new(),
            registry,
        }
    }

    /// Register a new pane and return its ID.
    pub fn register_pane(&mut self, kind: PaneKind, title: impl Into<String>) -> PaneId {
        let id = PaneId::next();
        let title = title.into();

        let state = self
            .registry
            .get(&kind)
            .map(|info| (info.factory)())
            .expect("PaneKind not found in registry");

        self.pane_states.insert(id, state);
        self.pane_entries.insert(id, PaneEntry { id, kind, title });

        id
    }

    /// Look up the display name for a pane kind from the registry.
    pub fn display_name(&self, kind: PaneKind) -> &'static str {
        self.registry
            .get(&kind)
            .map(|info| info.display_name)
            .unwrap_or("Unknown")
    }

    /// Check whether a pane kind is a singleton.
    pub fn is_singleton(&self, kind: PaneKind) -> bool {
        self.registry
            .get(&kind)
            .map(|info| info.is_singleton)
            .unwrap_or(false)
    }

    /// Iterate all singleton pane kinds in the registry.
    pub fn registry_singletons(&self) -> impl Iterator<Item = &PaneKindInfo> {
        self.registry.values().filter(|info| info.is_singleton)
    }

    /// Iterate all multi-instance pane kinds in the registry.
    pub fn registry_multi(&self) -> impl Iterator<Item = &PaneKindInfo> {
        self.registry.values().filter(|info| !info.is_singleton)
    }

    /// Check if a singleton pane of the given kind already exists.
    pub fn has_singleton_pane(&self, kind: PaneKind) -> bool {
        self.pane_entries.values().any(|e| e.kind == kind)
    }

    /// Find an existing singleton pane ID.
    pub fn find_singleton(&self, kind: PaneKind) -> Option<PaneId> {
        self.pane_entries
            .values()
            .find(|e| e.kind == kind)
            .map(|e| e.id)
    }

    /// Remove a pane by ID.
    pub fn remove_pane(&mut self, id: PaneId) {
        self.pane_states.remove(&id);
        self.pane_entries.remove(&id);
    }

    /// Serialize the current workspace layout for session persistence.
    ///
    /// Returns None if serialization fails.
    pub fn serialize_layout(&self) -> Option<crate::config::SerializedWorkspaceLayout> {
        let dock_json = serde_json::to_string(&self.dock_state).ok()?;

        let panes = self
            .pane_entries
            .values()
            .map(|entry| crate::config::SerializedPane {
                id: entry.id.0,
                kind: format!("{:?}", entry.kind),
                title: entry.title.clone(),
            })
            .collect();

        Some(crate::config::SerializedWorkspaceLayout { dock_json, panes })
    }

    /// Restore workspace layout from serialized state.
    ///
    /// This reconstructs panes and dock state from the saved layout.
    /// Returns true if restoration succeeded.
    pub fn restore_layout(&mut self, layout: &crate::config::SerializedWorkspaceLayout) -> bool {
        // First, parse the dock state
        let dock_state: egui_dock::DockState<PaneId> = match serde_json::from_str(&layout.dock_json)
        {
            Ok(state) => state,
            Err(e) => {
                tracing::warn!("Failed to deserialize dock state: {}", e);
                return false;
            }
        };

        // Clear existing panes
        self.pane_states.clear();
        self.pane_entries.clear();

        // Track the highest ID so we can update NEXT_PANE_ID
        let mut max_id = 0u64;

        // Reconstruct panes from serialized metadata
        for pane_info in &layout.panes {
            let pane_id = PaneId(pane_info.id);
            max_id = max_id.max(pane_info.id);

            // Parse the pane kind from string
            let kind = match pane_info.kind.as_str() {
                "VariableBrowser" => PaneKind::VariableBrowser,
                "VariableList" => PaneKind::VariableList,
                "Recorder" => PaneKind::Recorder,
                "TimeSeries" => PaneKind::TimeSeries,
                "Watcher" => PaneKind::Watcher,
                "FftView" => PaneKind::FftView,
                // Legacy: skip PipelineEditor from old configs
                "PipelineEditor" => {
                    tracing::info!("Skipping legacy PipelineEditor pane from saved layout");
                    continue;
                }
                _ => {
                    tracing::warn!("Unknown pane kind: {}, skipping", pane_info.kind);
                    continue;
                }
            };

            // Create the pane state using the factory
            let state = self
                .registry
                .get(&kind)
                .map(|info| (info.factory)())
                .expect("PaneKind not found in registry");

            self.pane_states.insert(pane_id, state);
            self.pane_entries.insert(
                pane_id,
                PaneEntry {
                    id: pane_id,
                    kind,
                    title: pane_info.title.clone(),
                },
            );
        }

        // Update NEXT_PANE_ID to avoid collisions
        use std::sync::atomic::Ordering;
        NEXT_PANE_ID.fetch_max(max_id + 1, Ordering::SeqCst);

        // Set the dock state
        self.dock_state = dock_state;

        tracing::info!(
            "Restored workspace layout with {} panes",
            layout.panes.len()
        );
        true
    }
}
