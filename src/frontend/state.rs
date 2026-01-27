//! Shared state types for the frontend
//!
//! This module defines the shared state container and action types used by
//! the page-based architecture. Pages receive `SharedState` via borrowing
//! and return `AppAction`s instead of mutating state directly.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::backend::{ElfInfo, ElfSymbol, FrontendReceiver};
use crate::config::{AppConfig, AppState, DataPersistenceConfig};
use crate::config::settings::RuntimeSettings;
use crate::types::{CollectionStats, ConnectionStatus, Variable, VariableData};

/// Shared state accessible by all pages (borrowed, not owned)
///
/// This struct provides pages with access to shared application state
/// through borrowed references. This design:
/// - Maintains single ownership in `DataVisApp`
/// - Avoids Arc/Mutex overhead for UI state
/// - Works well with egui's immediate mode paradigm
/// - Enables clear dependency injection
pub struct SharedState<'a> {
    // Communication
    /// Backend communication channel
    pub frontend: &'a FrontendReceiver,
    /// Current probe connection status
    pub connection_status: ConnectionStatus,

    // Configuration
    /// Application configuration (variables, probe settings, UI settings)
    pub config: &'a mut AppConfig,
    /// Runtime settings (collection state, plot settings)
    pub settings: &'a mut RuntimeSettings,
    /// Persistent application state (recent projects, preferences)
    pub app_state: &'a mut AppState,

    // Data
    /// Collected variable data (time series)
    pub variable_data: &'a mut HashMap<u32, VariableData>,
    /// Collection statistics
    pub stats: &'a CollectionStats,

    // ELF (shared read-only across pages)
    /// Parsed ELF information
    pub elf_info: Option<&'a ElfInfo>,
    /// Symbols from ELF file
    pub elf_symbols: &'a [ElfSymbol],
    /// Path to loaded ELF file
    pub elf_file_path: Option<&'a PathBuf>,

    // Persistence
    /// Data persistence configuration
    pub persistence_config: &'a mut DataPersistenceConfig,

    // Error display
    /// Last error message to display
    pub last_error: &'a mut Option<String>,

    // Start time for timing
    /// Application start time
    pub start_time: std::time::Instant,
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
    // Navigation
    /// Navigate to a different page
    NavigateTo(AppPage),

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

/// Application pages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppPage {
    /// Variables page - ELF browser, variable list
    #[default]
    Variables,
    /// Visualizer page - Plot, time controls
    Visualizer,
    /// Settings page - Probe, collection, UI settings
    Settings,
}

impl AppPage {
    /// Get the display name for this page
    pub fn name(&self) -> &'static str {
        match self {
            AppPage::Variables => "Variables",
            AppPage::Visualizer => "Visualizer",
            AppPage::Settings => "Settings",
        }
    }

    /// Get the icon for this page
    pub fn icon(&self) -> &'static str {
        match self {
            AppPage::Variables => ".",
            AppPage::Visualizer => ".",
            AppPage::Settings => ".",
        }
    }

    /// Get all available pages
    pub fn all() -> &'static [AppPage] {
        &[AppPage::Variables, AppPage::Visualizer, AppPage::Settings]
    }
}

impl std::fmt::Display for AppPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}
