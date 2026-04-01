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

/// How a child variable's address is determined.
#[derive(Debug, Clone, PartialEq)]
pub enum ChildAddressMode {
    /// Absolute address known at compile time (from DWARF debug info).
    Absolute(u64),
    /// Address is base_pointer_value + offset, resolved at runtime.
    RelativeToPointer { offset: u64 },
}

impl ChildAddressMode {
    /// Resolve this address mode to a concrete address.
    /// For `Absolute`, always returns the static address.
    /// For `RelativeToPointer`, requires a pointer value to resolve.
    pub fn resolve(&self, pointer_value: Option<u64>) -> Option<u64> {
        match self {
            ChildAddressMode::Absolute(addr) => Some(*addr),
            ChildAddressMode::RelativeToPointer { offset } => {
                pointer_value.map(|base| base.wrapping_add(*offset))
            }
        }
    }

    /// Whether this address requires runtime resolution.
    pub fn is_dynamic(&self) -> bool {
        matches!(self, ChildAddressMode::RelativeToPointer { .. })
    }
}

/// Specification for a child variable when adding a struct or pointer.
/// Supports arbitrary nesting — intermediate struct nodes carry their own children.
#[derive(Debug, Clone)]
pub struct ChildVariableSpec {
    pub name: String,
    pub address_mode: ChildAddressMode,
    pub var_type: VariableType,
    /// Nested children (non-empty for intermediate struct/array nodes)
    pub children: Vec<ChildVariableSpec>,
}

impl ChildVariableSpec {
    /// Count total leaf nodes (variables without children) in this tree.
    pub fn leaf_count(&self) -> usize {
        if self.children.is_empty() {
            1
        } else {
            self.children.iter().map(|c| c.leaf_count()).sum()
        }
    }
}

/// Immutable context available to all panes.
pub struct SharedContext<'a> {
    pub frontend: &'a PipelineBridge,
    pub elf_info: Option<&'a ElfInfo>,
    pub elf_symbols: &'a [ElfSymbol],
    pub elf_file_path: Option<&'a PathBuf>,
    pub display_time: f64,
    pub current_pane_id: Option<PaneId>,
}

/// Mutable state that panes can modify.
pub struct SharedMut<'a> {
    pub config: &'a mut AppConfig,
    pub settings: &'a mut RuntimeSettings,
    pub app_state: &'a mut AppState,
    pub persistence_config: &'a mut DataPersistenceConfig,
    pub last_error: &'a mut Option<String>,
    pub topics: &'a mut Topics,
}

/// Shared state accessible by all panes (composed of immutable context + mutable state).
///
/// This struct provides panes with access to shared application state
/// through borrowed references. Published data (variables, stats, status,
/// snapshots) is accessed via `state.topics`.
pub struct SharedState<'a> {
    pub ctx: SharedContext<'a>,
    pub state: SharedMut<'a>,
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
    /// Set poll rate
    SetPollRate(u32),
    /// Use mock probe (feature-gated)
    #[cfg(feature = "mock-probe")]
    UseMockProbe(bool),

    // Variable management
    /// Add a new variable
    AddVariable(Variable),
    /// Add a struct or pointer variable with auto-decomposed children.
    /// When `pointer_poll_rate_hz` is `Some`, the parent is treated as a pointer
    /// and children with `RelativeToPointer` addresses get pointer metadata.
    AddStructVariable {
        parent: Variable,
        children: Vec<ChildVariableSpec>,
        pointer_poll_rate_hz: Option<u32>,
    },
    /// Remove a variable by ID
    RemoveVariable(u32),
    /// Update an existing variable
    UpdateVariable(Variable),
    /// Set a variable's poll rate with tree propagation (down: clamp children, up: raise ancestors)
    SetVariablePollRate { id: u32, rate_hz: u32 },
    /// Toggle enabled state for an entire variable tree
    ToggleTreeEnabled { root_id: u32, enabled: bool },
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
        if !self.state.settings.collecting {
            return false;
        }

        let threshold = self.state.topics.staleness_threshold;
        let now = Instant::now();

        if let Some(pid) = pane_id {
            // Check pane-specific data first
            if let Some(last_update) = self.state.topics.pane_data_freshness.get(&pid) {
                return now.duration_since(*last_update) > threshold;
            }
        }

        // Fall back to global data check
        if let Some(global_update) = self.state.topics.global_data_freshness {
            return now.duration_since(global_update) > threshold;
        }

        // No data received yet - not stale, just empty
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::VariableType;

    #[test]
    fn test_absolute_resolve() {
        let mode = ChildAddressMode::Absolute(0x2000);
        assert_eq!(mode.resolve(None), Some(0x2000));
        assert_eq!(mode.resolve(Some(0x5000)), Some(0x2000));
    }

    #[test]
    fn test_relative_resolve() {
        let mode = ChildAddressMode::RelativeToPointer { offset: 8 };
        assert_eq!(mode.resolve(Some(0x3000)), Some(0x3008));
    }

    #[test]
    fn test_relative_resolve_none() {
        let mode = ChildAddressMode::RelativeToPointer { offset: 8 };
        assert_eq!(mode.resolve(None), None);
    }

    #[test]
    fn test_is_dynamic() {
        assert!(!ChildAddressMode::Absolute(0).is_dynamic());
        assert!(ChildAddressMode::RelativeToPointer { offset: 0 }.is_dynamic());
    }

    #[test]
    fn test_leaf_count_flat() {
        let spec = ChildVariableSpec {
            name: "parent".into(),
            address_mode: ChildAddressMode::Absolute(0),
            var_type: VariableType::U32,
            children: vec![
                ChildVariableSpec { name: "a".into(), address_mode: ChildAddressMode::Absolute(0), var_type: VariableType::U32, children: vec![] },
                ChildVariableSpec { name: "b".into(), address_mode: ChildAddressMode::Absolute(4), var_type: VariableType::U32, children: vec![] },
                ChildVariableSpec { name: "c".into(), address_mode: ChildAddressMode::Absolute(8), var_type: VariableType::F32, children: vec![] },
            ],
        };
        assert_eq!(spec.leaf_count(), 3);
    }

    #[test]
    fn test_leaf_count_nested() {
        let spec = ChildVariableSpec {
            name: "root".into(),
            address_mode: ChildAddressMode::Absolute(0),
            var_type: VariableType::U32,
            children: vec![
                ChildVariableSpec {
                    name: "inner".into(),
                    address_mode: ChildAddressMode::Absolute(0),
                    var_type: VariableType::U32,
                    children: vec![
                        ChildVariableSpec { name: "a".into(), address_mode: ChildAddressMode::Absolute(0), var_type: VariableType::U32, children: vec![] },
                        ChildVariableSpec { name: "b".into(), address_mode: ChildAddressMode::Absolute(4), var_type: VariableType::U32, children: vec![] },
                    ],
                },
                ChildVariableSpec { name: "c".into(), address_mode: ChildAddressMode::Absolute(8), var_type: VariableType::F32, children: vec![] },
            ],
        };
        assert_eq!(spec.leaf_count(), 3);
    }

    #[test]
    fn test_shared_mut_construction() {
        use crate::config::{AppConfig, AppState, DataPersistenceConfig};
        use crate::config::settings::RuntimeSettings;
        use crate::frontend::topics::Topics;

        let mut config = AppConfig::default();
        let mut settings = RuntimeSettings::default();
        let mut app_state = AppState::default();
        let mut persistence = DataPersistenceConfig::default();
        let mut last_error: Option<String> = None;
        let mut topics = Topics::default();

        let state = SharedMut {
            config: &mut config,
            settings: &mut settings,
            app_state: &mut app_state,
            persistence_config: &mut persistence,
            last_error: &mut last_error,
            topics: &mut topics,
        };

        // Verify fields are accessible
        assert!(state.config.variables.is_empty());
        assert!(!state.settings.paused);
    }

    #[test]
    fn test_shared_mut_add_variable() {
        use crate::config::{AppConfig, AppState, DataPersistenceConfig};
        use crate::config::settings::RuntimeSettings;
        use crate::frontend::topics::Topics;

        let mut config = AppConfig::default();
        let mut settings = RuntimeSettings::default();
        let mut app_state = AppState::default();
        let mut persistence = DataPersistenceConfig::default();
        let mut last_error: Option<String> = None;
        let mut topics = Topics::default();

        let state = SharedMut {
            config: &mut config,
            settings: &mut settings,
            app_state: &mut app_state,
            persistence_config: &mut persistence,
            last_error: &mut last_error,
            topics: &mut topics,
        };

        let var = crate::types::Variable::new("test", 0x1000, VariableType::U32);
        let id = var.id;
        state.config.add_variable(var);
        assert!(state.config.variables.contains_key(&id));
    }

    #[test]
    fn test_shared_mut_independent_of_context() {
        use crate::config::{AppConfig, AppState, DataPersistenceConfig};
        use crate::config::settings::RuntimeSettings;
        use crate::frontend::topics::Topics;

        let mut config = AppConfig::default();
        let mut settings = RuntimeSettings::default();
        let mut app_state = AppState::default();
        let mut persistence = DataPersistenceConfig::default();
        let mut last_error: Option<String> = None;
        let mut topics = Topics::default();

        // SharedMut can be constructed without SharedContext or PipelineBridge
        let state = SharedMut {
            config: &mut config,
            settings: &mut settings,
            app_state: &mut app_state,
            persistence_config: &mut persistence,
            last_error: &mut last_error,
            topics: &mut topics,
        };

        state.settings.paused = true;
        assert!(state.settings.paused);
    }
}
