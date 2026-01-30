//! Shared state types for the frontend
//!
//! This module defines the shared state container and action types used by
//! the workspace-based architecture. Panes receive `SharedState` via borrowing
//! and return `AppAction`s instead of mutating state directly.

use std::path::PathBuf;
use std::time::Instant;

use crate::backend::{ElfInfo, ElfSymbol};
use crate::config::settings::RuntimeSettings;
use crate::config::{AppConfig, AppState, DataPersistenceConfig};
use crate::frontend::topics::Topics;
use crate::pipeline::bridge::PipelineBridge;
use crate::pipeline::id::NodeId;
use crate::pipeline::packet::ConfigValue;
use crate::types::{Variable, VariableType};

use super::workspace::{PaneId, PaneKind};

/// Specification for a child variable when adding a struct
#[derive(Debug, Clone)]
pub struct ChildVariableSpec {
    pub name: String,
    pub address: u64,
    pub var_type: VariableType,
}

/// Specification for a pointer-dependent child variable (Phase 2 feature)
#[derive(Debug, Clone)]
pub struct PointerChildSpec {
    pub name: String,
    pub offset_from_pointer: u64,
    pub var_type: VariableType,
}

/// Shared state accessible by all panes (borrowed, not owned).
///
/// This struct provides panes with access to shared application state
/// through borrowed references. Published data (variables, stats, status,
/// snapshots) is accessed via `topics`.
pub struct SharedState<'a> {
    // Communication
    pub frontend: &'a PipelineBridge,

    // Configuration (read-write by panes)
    pub config: &'a mut AppConfig,
    pub settings: &'a mut RuntimeSettings,
    pub app_state: &'a mut AppState,

    // ELF (read-only)
    pub elf_info: Option<&'a ElfInfo>,
    pub elf_symbols: &'a [ElfSymbol],
    pub elf_file_path: Option<&'a PathBuf>,

    // Persistence
    pub persistence_config: &'a mut DataPersistenceConfig,

    // Error display
    pub last_error: &'a mut Option<String>,

    // Timing — current time in seconds (frozen when not collecting)
    pub display_time: f64,

    // All published data (variables, stats, status, snapshots)
    pub topics: &'a mut Topics,

    // Current pane being rendered (for per-pane data access)
    pub current_pane_id: Option<PaneId>,
}

/// Actions that any page can emit
///
/// Pages return `Vec<AppAction>` instead of mutating state directly.
/// This enables:
/// - Testable page logic
/// - Clear separation between UI and business logic
/// - Centralized action handling
#[derive(Debug, Clone)]
pub enum AppAction {
    // Backend commands
    /// Connect to a debug probe
    Connect {
        probe_selector: Option<String>,
        target: String,
    },
    /// Disconnect from the current probe
    Disconnect,
    /// Start data collection
    StartCollection,
    /// Stop data collection
    StopCollection,
    /// Refresh available probes list
    RefreshProbes,
    /// Set memory access mode
    SetMemoryAccessMode(crate::config::MemoryAccessMode),
    /// Set poll rate
    SetPollRate(u32),
    /// Use mock probe (feature-gated)
    #[cfg(feature = "mock-probe")]
    UseMockProbe(bool),

    // Variable management
    /// Add a new variable
    AddVariable(Variable),
    /// Add a struct variable with auto-decomposed children
    AddStructVariable {
        parent: Variable,
        children: Vec<ChildVariableSpec>,
    },
    /// Add a pointer variable with auto-dereferencing children (Phase 2 feature)
    AddPointerVariable {
        pointer: Variable,
        children: Vec<PointerChildSpec>,
        pointer_poll_rate_hz: u32,
    },
    /// Remove a variable by ID
    RemoveVariable(u32),
    /// Update an existing variable
    UpdateVariable(Variable),
    /// Write a value to a variable
    WriteVariable { id: u32, value: f64 },

    // ELF management
    /// Load an ELF file
    LoadElf(PathBuf),
    /// Detect variable changes after ELF reload
    DetectVariableChanges,

    // Project management
    /// Save the current project
    SaveProject(PathBuf),
    /// Load a project
    LoadProject(PathBuf),

    // Dialogs
    /// Open a dialog
    OpenDialog(DialogId),

    // Data
    /// Clear all collected data
    ClearData,
    /// Clear data for a specific variable
    ClearVariableData(u32),

    // Pipeline node configuration
    /// Send a config key/value to a specific pipeline node
    NodeConfig {
        node_id: NodeId,
        key: String,
        value: ConfigValue,
    },
    /// Request pipeline topology snapshot
    RequestTopology,

    // Pipeline graph mutations (removed in Phase 3)
    // The pipeline editor has been removed in favor of direct converter configuration

    // Workspace actions
    /// Open/focus a singleton pane, or create if not exists
    OpenPane(PaneKind),
    /// Create a new visualizer instance
    NewVisualizer(PaneKind),
    /// Close a pane (remove from dock and clean up state)
    ClosePane(PaneId),
    /// Rename a variable group (parent + propagate prefix to children)
    RenameVariable { id: u32, new_name: String },

    // Project management
    /// Create a new project (reset config to defaults)
    NewProject,
    /// Reset the workspace layout to defaults
    ResetLayout,

    // Toolbar
    /// Toggle collection pause state
    TogglePause,
}

/// Dialog identifiers
///
/// Used with `AppAction::OpenDialog` to specify which dialog to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogId {
    /// Add a new variable
    AddVariable,
    /// Edit an existing variable
    EditVariable(u32),
    /// Edit converter script for a variable
    ConverterEditor(u32),
    /// Edit value for a variable
    ValueEditor(u32),
    /// View variable details
    VariableDetail(u32),
    /// Browse ELF symbols
    ElfSymbols,
    /// Variable change detection results
    VariableChange,
    /// Duplicate variable confirmation
    DuplicateConfirm,
}

impl<'a> SharedState<'a> {
    /// Check if pane data is stale (no updates for staleness_threshold duration).
    ///
    /// Returns `false` if collection is stopped, otherwise checks if data has not
    /// been received for longer than the configured staleness threshold.
    ///
    /// # Arguments
    /// * `pane_id` - Optional pane ID. If provided, checks pane-specific data freshness.
    ///   If None, checks global data freshness.
    ///
    /// # Returns
    /// * `true` if data is stale (no updates within threshold while collecting)
    /// * `false` if data is fresh, collection stopped, or no data received yet
    pub fn is_pane_data_stale(&self, pane_id: Option<u64>) -> bool {
        // Don't warn if collection stopped
        if !self.settings.collecting {
            return false;
        }

        let threshold = self.topics.staleness_threshold;
        let now = Instant::now();

        if let Some(pid) = pane_id {
            // Check pane-specific data first
            if let Some(last_update) = self.topics.pane_data_freshness.get(&pid) {
                return now.duration_since(*last_update) > threshold;
            }
        }

        // Fall back to global data check
        if let Some(global_update) = self.topics.global_data_freshness {
            return now.duration_since(global_update) > threshold;
        }

        // No data received yet - not stale, just empty
        false
    }
}
