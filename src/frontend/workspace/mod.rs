//! Workspace module for dockable pane management
//!
//! Provides the core workspace types: PaneId, PaneKind, PaneState, Workspace.
//! Uses egui_dock for drag-and-drop docking, tabs, and splits.

pub mod default_layout;
pub mod tab_viewer;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::frontend::panes::{
    FftViewState, SettingsPaneState, TimeSeriesState, VariableBrowserState, VariableListState,
    WatcherState,
};

/// Unique identifier for a pane instance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(pub u64);

static NEXT_PANE_ID: AtomicU64 = AtomicU64::new(1);

impl PaneId {
    pub fn next() -> Self {
        Self(NEXT_PANE_ID.fetch_add(1, Ordering::SeqCst))
    }
}

/// Kind of pane (used for dispatch and menu display)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneKind {
    // Utility (singletons)
    VariableBrowser,
    VariableList,
    Settings,
    // Visualizers (multiple instances allowed)
    TimeSeries,
    Watcher,
    FftView,
}

impl PaneKind {
    /// Display name for menus
    pub fn display_name(&self) -> &'static str {
        match self {
            PaneKind::VariableBrowser => "Variable Browser",
            PaneKind::VariableList => "Variables",
            PaneKind::Settings => "Settings",
            PaneKind::TimeSeries => "Time Series",
            PaneKind::Watcher => "Watcher",
            PaneKind::FftView => "FFT View",
        }
    }

    /// Whether only one instance of this pane kind is allowed
    pub fn is_singleton(&self) -> bool {
        matches!(
            self,
            PaneKind::VariableBrowser | PaneKind::VariableList | PaneKind::Settings
        )
    }
}

/// Per-pane state (enum dispatch, not trait objects)
pub enum PaneState {
    VariableBrowser(VariableBrowserState),
    VariableList(VariableListState),
    Settings(SettingsPaneState),
    TimeSeries(TimeSeriesState),
    Watcher(WatcherState),
    FftView(FftViewState),
}

/// Metadata entry for a pane
pub struct PaneEntry {
    pub id: PaneId,
    pub kind: PaneKind,
    pub title: String,
}

/// The workspace holds all dock state and pane data
pub struct Workspace {
    pub dock_state: egui_dock::DockState<PaneId>,
    pub pane_states: HashMap<PaneId, PaneState>,
    pub pane_entries: HashMap<PaneId, PaneEntry>,
}

impl Workspace {
    /// Register a new pane and return its ID
    pub fn register_pane(&mut self, kind: PaneKind, title: impl Into<String>) -> PaneId {
        let id = PaneId::next();
        let title = title.into();

        let state = match kind {
            PaneKind::VariableBrowser => PaneState::VariableBrowser(VariableBrowserState::default()),
            PaneKind::VariableList => PaneState::VariableList(VariableListState::default()),
            PaneKind::Settings => PaneState::Settings(SettingsPaneState::default()),
            PaneKind::TimeSeries => PaneState::TimeSeries(TimeSeriesState::default()),
            PaneKind::Watcher => PaneState::Watcher(WatcherState::default()),
            PaneKind::FftView => PaneState::FftView(FftViewState::default()),
        };

        self.pane_states.insert(id, state);
        self.pane_entries.insert(
            id,
            PaneEntry {
                id,
                kind,
                title,
            },
        );

        id
    }

    /// Check if a singleton pane of the given kind already exists
    pub fn has_singleton(&self, kind: PaneKind) -> bool {
        self.pane_entries.values().any(|e| e.kind == kind)
    }

    /// Find an existing singleton pane ID
    pub fn find_singleton(&self, kind: PaneKind) -> Option<PaneId> {
        self.pane_entries
            .values()
            .find(|e| e.kind == kind)
            .map(|e| e.id)
    }

    /// Remove a pane by ID
    pub fn remove_pane(&mut self, id: PaneId) {
        self.pane_states.remove(&id);
        self.pane_entries.remove(&id);
    }
}
