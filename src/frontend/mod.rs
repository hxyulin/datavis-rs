//! Frontend module for egui UI
//!
//! This module provides the main UI components using eframe/egui.
//! It receives data from the backend through crossbeam channels and
//! renders it in real-time.
//!
//! # Architecture
//!
//! The frontend runs on the main thread and communicates with the backend
//! worker thread via channels. It never blocks on SWD operations, ensuring
//! a responsive UI even when the probe is slow or unresponsive.
//!
//! # Main Types
//!
//! - [`DataVisApp`] - Main application state implementing [`eframe::App`]
//! - [`AppPage`] - Enum of available pages (Variables, Visualizer, Settings)
//! - [`PlotView`] - Plot configuration and rendering
//! - [`VariableEditorState`] - State for the variable add/edit dialog
//!
//! # Pages
//!
//! The application has three main pages:
//!
//! 1. **Variables** - Add, edit, and manage observed variables
//! 2. **Visualizer** - Real-time plotting with axis controls
//! 3. **Settings** - Configure probe, collection, and UI options
//!
//! # Submodules
//!
//! - `panels` - Reusable panel components (connection, stats, etc.)
//! - `plot` - Plot rendering with egui_plot
//! - [`script_editor`] - Rhai script editor with syntax highlighting
//! - `widgets` - Custom UI widgets (status indicators, sparklines, etc.)

pub mod dialogs;
pub mod markers;
pub mod pages;
mod panels;
mod plot;
pub mod script_editor;
pub mod state;
mod widgets;

pub use pages::{Page, SettingsPage, SettingsPageState, VariablesPage, VariablesPageState, VisualizerPage, VisualizerPageState};
pub use panels::*;
pub use plot::PlotView;
pub use script_editor::{ScriptEditor, ScriptEditorState};
pub use state::{AppAction, AppPage, DialogId, SharedState};
pub use widgets::*;

use dialogs::{
    show_dialog, DuplicateConfirmState, ElfSymbolsAction, ElfSymbolsContext, ElfSymbolsDialog,
    ElfSymbolsState, VariableChangeAction, VariableChangeContext, VariableChangeDialog,
    VariableChangeState,
};

use crate::backend::{
    parse_elf, BackendCommand, BackendMessage, ElfInfo, ElfSymbol, FrontendReceiver,
};
use crate::config::{settings::RuntimeSettings, AppConfig, AppState};
use crate::types::{CollectionStats, ConnectionStatus, DataPoint, VariableData, VariableType};
use egui::Color32;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

/// Actions that can be performed on variables from the UI
///
/// These actions are used internally to handle variable management
/// operations from the UI.
#[allow(dead_code)]
#[derive(Debug)]
enum VariableAction {
    SetEnabled(bool),
    Edit,
    Remove,
}

// AppPage is now defined in state.rs

/// Type of change detected for a variable when reloading ELF
#[derive(Debug, Clone)]
pub enum VariableChangeType {
    /// Variable found but at a different address
    AddressChanged { old_address: u64, new_address: u64 },
    /// Variable found but DWARF type differs from configured VariableType
    TypeChanged {
        old_type: VariableType,
        new_type: VariableType,
        new_type_name: String,
    },
    /// Variable was in config but no longer found in ELF
    NotFound,
}

/// A detected change for a single variable
#[derive(Debug, Clone)]
pub struct VariableChange {
    /// The variable ID from config
    pub variable_id: u32,
    /// The variable name
    pub variable_name: String,
    /// Current configured address
    pub current_address: u64,
    /// Current configured type
    pub current_type: VariableType,
    /// The type of change detected
    pub change_type: VariableChangeType,
    /// Whether this change is selected for update
    pub selected: bool,
}

/// Main application state for the data visualizer
///
/// This struct holds all UI state for the application including
/// connection status, variable data, configuration, and dialog states.
#[allow(dead_code)]
pub struct DataVisApp {
    // === Communication ===
    /// Frontend receiver for backend communication
    frontend: FrontendReceiver,

    // === Shared State ===
    /// Application configuration (from current project)
    config: AppConfig,
    /// Application state (persisted across sessions)
    app_state: AppState,
    /// Runtime settings
    settings: RuntimeSettings,
    /// Current connection status
    connection_status: ConnectionStatus,
    /// Variable data storage
    variable_data: HashMap<u32, VariableData>,
    /// Collection statistics
    stats: CollectionStats,
    /// Start time for the session
    start_time: Instant,
    /// Last error message
    last_error: Option<String>,
    /// Data persistence configuration
    persistence_config: crate::config::DataPersistenceConfig,

    // === ELF Data (shared across pages) ===
    /// Loaded ELF/AXF file path
    elf_file_path: Option<PathBuf>,
    /// Parsed ELF info (contains symbols and type info)
    elf_info: Option<ElfInfo>,
    /// Symbols parsed from ELF file (for display)
    elf_symbols: Vec<ElfSymbol>,

    // === Navigation ===
    /// Current page/view being displayed
    current_page: AppPage,

    // === Page State ===
    /// Variables page state
    variables_page: VariablesPageState,
    /// Visualizer page state
    visualizer_page: VisualizerPageState,
    /// Settings page state
    settings_page: SettingsPageState,

    // === Global Dialogs (can appear on any page) ===
    /// Variable change detection dialog state
    variable_change_open: bool,
    variable_change_state: VariableChangeState,
    /// ELF symbols dialog state
    elf_symbols_open: bool,
    elf_symbols_state: ElfSymbolsState,
    /// Duplicate variable confirmation dialog state
    duplicate_confirm_open: bool,
    duplicate_confirm_state: DuplicateConfirmState,
}

/// State for variable autocomplete/selector
///
/// Manages the autocomplete dropdown for selecting variables from
/// ELF files, including filtering and keyboard navigation.
#[allow(dead_code)]
#[derive(Default)]
pub struct VariableSelectorState {
    /// Current search query
    pub query: String,
    /// Filtered symbols matching query
    pub filtered_symbols: Vec<ElfSymbol>,
    /// Currently selected index in filtered list
    pub selected_index: Option<usize>,
    /// Whether the dropdown is open
    pub dropdown_open: bool,
    /// Cursor position for text input
    pub cursor_position: usize,
    /// Set of expanded paths (supports nested expansion like "0", "0.member1", "0.member1.nested")
    pub expanded_paths: std::collections::HashSet<String>,
}

impl VariableSelectorState {
    /// Update filtered symbols based on query
    pub fn update_filter(&mut self, elf_info: Option<&ElfInfo>) {
        self.filtered_symbols.clear();
        if let Some(info) = elf_info {
            let results = info.search_variables(&self.query);
            // Show all results - ScrollArea handles virtualization for performance
            self.filtered_symbols = results.into_iter().cloned().collect();
        }
        // Reset selection if list changed
        if self.filtered_symbols.is_empty() {
            self.selected_index = None;
        } else if let Some(idx) = self.selected_index {
            if idx >= self.filtered_symbols.len() {
                self.selected_index = Some(self.filtered_symbols.len() - 1);
            }
        }
    }

    /// Move selection up
    pub fn select_previous(&mut self) {
        if self.filtered_symbols.is_empty() {
            return;
        }
        self.selected_index = Some(match self.selected_index {
            Some(0) => self.filtered_symbols.len() - 1,
            Some(idx) => idx - 1,
            None => 0,
        });
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if self.filtered_symbols.is_empty() {
            return;
        }
        self.selected_index = Some(match self.selected_index {
            Some(idx) if idx + 1 >= self.filtered_symbols.len() => 0,
            Some(idx) => idx + 1,
            None => 0,
        });
    }

    /// Get currently selected symbol
    pub fn selected_symbol(&self) -> Option<&ElfSymbol> {
        self.selected_index
            .and_then(|idx| self.filtered_symbols.get(idx))
    }

    /// Clear state
    pub fn clear(&mut self) {
        self.query.clear();
        self.filtered_symbols.clear();
        self.selected_index = None;
        self.dropdown_open = false;
        self.expanded_paths.clear();
    }

    /// Toggle expansion state for a path
    pub fn toggle_expanded(&mut self, path: &str) {
        if self.expanded_paths.contains(path) {
            // Remove this path and all child paths
            self.expanded_paths.retain(|p| !p.starts_with(path));
        } else {
            self.expanded_paths.insert(path.to_string());
        }
    }

    /// Check if a path is expanded
    pub fn is_expanded(&self, path: &str) -> bool {
        self.expanded_paths.contains(path)
    }
}

/// State for the variable editor dialog
#[derive(Default)]
pub struct VariableEditorState {
    pub name: String,
    pub address: String,
    pub var_type: crate::types::VariableType,
    pub unit: String,
    pub converter_script: String,
    pub color: [u8; 4],
    pub editing_id: Option<u32>,
    /// Type name from DWARF info (for display)
    pub type_name: Option<String>,
    /// Size from symbol info
    pub size: Option<u64>,
    /// Script editor state for autocomplete
    pub script_editor_state: ScriptEditorState,
}

impl VariableEditorState {
    /// Populate from an ELF symbol
    pub fn from_symbol(symbol: &ElfSymbol, elf_info: Option<&ElfInfo>) -> Self {
        let (var_type, type_name) = if let Some(info) = elf_info {
            let vt = info.infer_variable_type_for_symbol(symbol);
            let tn = Some(info.get_symbol_type_name(symbol));
            (vt, tn)
        } else {
            (symbol.infer_variable_type(), None)
        };

        Self {
            name: symbol.display_name.clone(),
            address: format!("0x{:08X}", symbol.address),
            var_type,
            unit: String::new(),
            converter_script: String::new(),
            color: [100, 149, 237, 255], // Cornflower blue default
            editing_id: None,
            type_name,
            size: Some(symbol.size),
            script_editor_state: ScriptEditorState::default(),
        }
    }
}

#[allow(dead_code)]
impl DataVisApp {
    /// Create a new application instance
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        frontend: FrontendReceiver,
        config: AppConfig,
        app_state: AppState,
        project_path: Option<PathBuf>,
    ) -> Self {
        // Configure fonts and styles
        let fonts = egui::FontDefinitions::default();
        cc.egui_ctx.set_fonts(fonts);

        // Apply font scale from preferences
        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.iter_mut().for_each(|(_, font_id)| {
            font_id.size *= app_state.ui_preferences.font_scale;
        });
        cc.egui_ctx.set_style(style);

        // Determine project name
        let project_name = if let Some(ref path) = project_path {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled Project")
                .to_string()
        } else {
            "Untitled Project".to_string()
        };

        // Sync the variable ID counter to avoid ID collisions with loaded variables
        crate::types::Variable::sync_next_id(&config.variables);

        // Initialize variable data from config
        let mut variable_data = HashMap::new();
        for var in &config.variables {
            variable_data.insert(var.id, VariableData::new(var.clone()));
        }

        // Send initial variables to backend
        for var in &config.variables {
            frontend.add_variable(var.clone());
        }

        let target_chip_input = config.probe.target_chip.clone();

        // Request probe list asynchronously from backend
        frontend.send_command(BackendCommand::RefreshProbes);

        // Initialize settings page state with target chip
        let mut settings_page = SettingsPageState::default();
        settings_page.target_chip_input = target_chip_input;
        settings_page.project_name = project_name;
        settings_page.project_file_path = project_path;

        Self {
            frontend,
            config,
            app_state,
            settings: RuntimeSettings::default(),
            connection_status: ConnectionStatus::Disconnected,
            variable_data,
            stats: CollectionStats::default(),
            start_time: Instant::now(),
            last_error: None,
            persistence_config: crate::config::DataPersistenceConfig::default(),
            elf_file_path: None,
            elf_info: None,
            elf_symbols: Vec::new(),
            current_page: AppPage::default(),
            variables_page: VariablesPageState::default(),
            visualizer_page: VisualizerPageState::default(),
            settings_page,
            variable_change_open: false,
            variable_change_state: VariableChangeState::default(),
            elf_symbols_open: false,
            elf_symbols_state: ElfSymbolsState::default(),
            duplicate_confirm_open: false,
            duplicate_confirm_state: DuplicateConfirmState::default(),
        }
    }

    /// Process messages from the backend
    /// Process messages from the backend, returns true if any messages were processed
    fn process_backend_messages(&mut self) -> bool {
        let messages = self.frontend.drain();
        let had_messages = !messages.is_empty();

        for msg in messages {
            match msg {
                BackendMessage::ConnectionStatus(status) => {
                    self.connection_status = status;
                    if status == ConnectionStatus::Connected {
                        self.last_error = None;
                    }
                }
                BackendMessage::ConnectionError(err) => {
                    self.last_error = Some(err);
                    self.connection_status = ConnectionStatus::Error;
                }
                BackendMessage::DataPoint {
                    variable_id,
                    timestamp,
                    raw_value,
                    converted_value,
                } => {
                    if let Some(data) = self.variable_data.get_mut(&variable_id) {
                        data.push(DataPoint::with_conversion(
                            timestamp,
                            raw_value,
                            converted_value,
                        ));
                    }
                }
                BackendMessage::DataBatch(batch) => {
                    for (variable_id, timestamp, raw_value, converted_value) in batch {
                        if let Some(data) = self.variable_data.get_mut(&variable_id) {
                            data.push(DataPoint::with_conversion(
                                timestamp,
                                raw_value,
                                converted_value,
                            ));
                        }
                    }
                }
                BackendMessage::ReadError { variable_id, error } => {
                    if let Some(data) = self.variable_data.get_mut(&variable_id) {
                        data.record_error(error);
                    }
                }
                BackendMessage::Stats(stats) => {
                    self.stats = stats;
                }
                BackendMessage::VariableList(vars) => {
                    // Update variable data with new list
                    for var in vars {
                        if !self.variable_data.contains_key(&var.id) {
                            self.variable_data.insert(var.id, VariableData::new(var));
                        }
                    }
                }
                BackendMessage::ProbeList(probes) => {
                    self.settings_page.available_probes = probes;
                    // Reset selection if it's now out of bounds
                    if let Some(idx) = self.settings_page.selected_probe_index {
                        if idx >= self.settings_page.available_probes.len() {
                            self.settings_page.selected_probe_index = None;
                        }
                    }
                    tracing::info!("Received {} probes", self.settings_page.available_probes.len());
                }
                BackendMessage::Shutdown => {
                    tracing::info!("Backend shutdown received");
                }
                BackendMessage::WriteSuccess { variable_id } => {
                    tracing::info!("Successfully wrote to variable {}", variable_id);
                    // Could show a toast notification here
                }
                BackendMessage::WriteError { variable_id, error } => {
                    tracing::error!("Failed to write to variable {}: {}", variable_id, error);
                    self.last_error = Some(format!("Write failed: {}", error));
                }
            }
        }

        had_messages
    }

    /// Handle an action emitted by a page
    fn handle_action(&mut self, action: AppAction) {
        match action {
            AppAction::NavigateTo(page) => {
                self.current_page = page;
            }
            AppAction::Connect { probe_selector, target } => {
                self.frontend.send_command(BackendCommand::Connect {
                    selector: probe_selector,
                    target,
                    probe_config: self.config.probe.clone(),
                });
            }
            AppAction::Disconnect => {
                self.frontend.send_command(BackendCommand::Disconnect);
            }
            AppAction::StartCollection => {
                self.settings.collecting = true;
                self.frontend.send_command(BackendCommand::StartCollection);
            }
            AppAction::StopCollection => {
                self.settings.collecting = false;
                self.frontend.send_command(BackendCommand::StopCollection);
            }
            AppAction::RefreshProbes => {
                self.frontend.send_command(BackendCommand::RefreshProbes);
            }
            AppAction::SetMemoryAccessMode(mode) => {
                self.frontend.send_command(BackendCommand::SetMemoryAccessMode(mode));
            }
            AppAction::SetPollRate(rate) => {
                self.frontend.send_command(BackendCommand::SetPollRate(rate));
            }
            #[cfg(feature = "mock-probe")]
            AppAction::UseMockProbe(use_mock) => {
                self.frontend.use_mock_probe(use_mock);
            }
            AppAction::AddVariable(var) => {
                self.add_variable(var);
            }
            AppAction::RemoveVariable(id) => {
                self.config.remove_variable(id);
                self.variable_data.remove(&id);
                self.frontend.send_command(BackendCommand::RemoveVariable(id));
            }
            AppAction::UpdateVariable(var) => {
                self.frontend.send_command(BackendCommand::UpdateVariable(var));
            }
            AppAction::WriteVariable { id, value } => {
                self.frontend.write_variable(id, value);
            }
            AppAction::LoadElf(path) => {
                self.load_elf(&path);
            }
            AppAction::DetectVariableChanges => {
                self.detect_variable_changes();
            }
            AppAction::SaveProject(path) => {
                self.save_project_to_path(path);
            }
            AppAction::LoadProject(path) => {
                self.load_project_from_path(path);
            }
            AppAction::OpenDialog(dialog_id) => {
                self.open_dialog(dialog_id);
            }
            AppAction::ClearData => {
                for data in self.variable_data.values_mut() {
                    data.clear();
                }
            }
            AppAction::ClearVariableData(id) => {
                if let Some(data) = self.variable_data.get_mut(&id) {
                    data.clear();
                }
            }
        }
    }

    /// Open a dialog by ID
    fn open_dialog(&mut self, dialog_id: DialogId) {
        match dialog_id {
            DialogId::AddVariable => {
                // Not implemented in new page system yet
            }
            DialogId::EditVariable(_id) => {
                // Not implemented in new page system yet
            }
            DialogId::ConverterEditor(_id) => {
                // Handled by page state
            }
            DialogId::ValueEditor(_id) => {
                // Handled by page state
            }
            DialogId::VariableDetail(_id) => {
                // Handled by page state
            }
            DialogId::ElfSymbols => {
                self.elf_symbols_open = true;
            }
            DialogId::VariableChange => {
                self.variable_change_open = true;
            }
            DialogId::DuplicateConfirm => {
                // Handled by page state
            }
        }
    }

    /// Load an ELF file
    fn load_elf(&mut self, path: &PathBuf) {
        self.elf_file_path = Some(path.clone());
        match parse_elf(path) {
            Ok(info) => {
                tracing::info!(
                    "Parsed ELF: {} variables, {} functions",
                    info.variable_count(),
                    info.function_count()
                );
                self.elf_symbols = info.get_variables().into_iter().cloned().collect();
                self.elf_info = Some(info);
                self.variables_page.selector.update_filter(self.elf_info.as_ref());
                self.detect_variable_changes();
            }
            Err(e) => {
                self.last_error = Some(format!("Failed to parse ELF: {}", e));
                self.elf_info = None;
                self.elf_symbols.clear();
            }
        }
    }


    /// Add a variable directly, checking for duplicates
    fn add_variable(&mut self, var: crate::types::Variable) {
        // Check for duplicate by address
        let is_duplicate = self
            .config
            .variables
            .iter()
            .any(|v| v.address == var.address);

        if is_duplicate {
            // Store pending variable and show confirmation dialog
            self.duplicate_confirm_state = DuplicateConfirmState::with_variable(var);
            self.duplicate_confirm_open = true;
        } else {
            self.add_variable_confirmed(var);
        }
    }

    /// Add a variable without duplicate check (after confirmation)
    fn add_variable_confirmed(&mut self, var: crate::types::Variable) {
        // Add to config
        self.config.add_variable(var.clone());
        // Add to variable data for display
        self.variable_data
            .insert(var.id, VariableData::new(var.clone()));
        // Send to backend
        self.frontend.add_variable(var);
    }

    /// Clear all collected data
    fn clear_all_data(&mut self) {
        for data in self.variable_data.values_mut() {
            data.clear();
        }
        self.stats = CollectionStats::default();
    }

    /// Render the ELF symbols dialog using the Dialog trait
    fn render_elf_symbols_with_context(&mut self, ctx: &egui::Context) {
        let dialog_ctx = ElfSymbolsContext {
            elf_file_path: self.elf_file_path.as_deref(),
            symbols: &self.elf_symbols,
            elf_info: self.elf_info.as_ref(),
        };

        if let Some(action) = show_dialog::<ElfSymbolsDialog>(
            ctx,
            &mut self.elf_symbols_open,
            &mut self.elf_symbols_state,
            dialog_ctx,
        ) {
            match action {
                ElfSymbolsAction::Select(symbol) => {
                    // Add the variable directly from the symbol
                    let var_type = self
                        .elf_info
                        .as_ref()
                        .map(|info| info.infer_variable_type_for_symbol(&symbol))
                        .unwrap_or(crate::types::VariableType::U32);
                    let var = crate::types::Variable::new(
                        &symbol.display_name,
                        symbol.address,
                        var_type,
                    );
                    self.add_variable(var);
                }
            }
        }
    }

    fn save_project_to_path(&mut self, path: PathBuf) {
        let project = crate::config::ProjectFile {
            version: 1,
            name: self.settings_page.project_name.clone(),
            config: self.config.clone(),
            binary_path: self.elf_file_path.clone(),
            persistence: self.persistence_config.clone(),
        };

        match project.save(&path) {
            Ok(()) => {
                self.settings_page.project_file_path = Some(path.clone());

                // Update app state with recent project
                self.app_state.add_recent_project(
                    &path,
                    &self.settings_page.project_name,
                    Some(&self.config.probe.target_chip),
                );

                // Save app state
                if let Err(e) = self.app_state.save() {
                    tracing::warn!("Failed to save app state: {}", e);
                }

                tracing::info!("Project saved successfully");
            }
            Err(e) => {
                self.last_error = Some(format!("Failed to save project: {}", e));
            }
        }
    }

    /// Load a project from a file path
    fn load_project_from_path(&mut self, path: PathBuf) {
        match crate::config::ProjectFile::load(&path) {
            Ok(project) => {
                self.config = project.config;
                self.persistence_config = project.persistence;

                // Sync the variable ID counter to avoid ID collisions with loaded variables
                crate::types::Variable::sync_next_id(&self.config.variables);
                self.settings_page.project_name = project.name.clone();
                self.settings_page.project_file_path = Some(path.clone());

                // Update app state with recent project
                self.app_state.add_recent_project(
                    &path,
                    &project.name,
                    Some(&self.config.probe.target_chip),
                );

                // Save app state
                if let Err(e) = self.app_state.save() {
                    tracing::warn!("Failed to save app state: {}", e);
                }

                // Update ELF path and try to load it
                if let Some(binary_path) = project.binary_path {
                    self.elf_file_path = Some(binary_path.clone());
                    // Try to parse the ELF file
                    match crate::backend::parse_elf(&binary_path) {
                        Ok(info) => {
                            self.elf_symbols = info.symbols.clone();
                            self.elf_info = Some(info);
                            self.variables_page.selector.update_filter(self.elf_info.as_ref());
                            tracing::info!("Loaded ELF from project: {:?}", binary_path);
                            // Detect variable changes after ELF reload
                            self.detect_variable_changes();
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse ELF from project: {}", e);
                        }
                    }
                }

                // Update target chip input
                self.settings_page.target_chip_input = self.config.probe.target_chip.clone();

                // Reload variables
                self.variable_data.clear();
                for var in &self.config.variables {
                    self.variable_data
                        .insert(var.id, crate::types::VariableData::new(var.clone()));
                    self.frontend.add_variable(var.clone());
                }

                tracing::info!("Project loaded successfully");
            }
            Err(e) => {
                self.last_error = Some(format!("Failed to load project: {}", e));
            }
        }
    }

    /// Detect changes between watched variables and newly loaded ELF symbols
    fn detect_variable_changes(&mut self) {
        // Skip if no ELF loaded or no variables to check
        let elf_info = match &self.elf_info {
            Some(info) => info,
            None => return,
        };

        if self.config.variables.is_empty() {
            return;
        }

        let mut changes: Vec<VariableChange> = Vec::new();

        for var in &self.config.variables {
            // Try to find the symbol by name in the new ELF
            let symbol = elf_info.find_symbol(&var.name);

            match symbol {
                Some(sym) => {
                    // Symbol found - check for address change
                    if sym.address != var.address {
                        changes.push(VariableChange {
                            variable_id: var.id,
                            variable_name: var.name.clone(),
                            current_address: var.address,
                            current_type: var.var_type,
                            change_type: VariableChangeType::AddressChanged {
                                old_address: var.address,
                                new_address: sym.address,
                            },
                            selected: true, // Pre-select for convenience
                        });
                    }

                    // Check for type change
                    let new_var_type = elf_info.infer_variable_type_for_symbol(sym);
                    if var.var_type != new_var_type {
                        let type_name = elf_info.get_symbol_type_name(sym);
                        changes.push(VariableChange {
                            variable_id: var.id,
                            variable_name: var.name.clone(),
                            current_address: var.address,
                            current_type: var.var_type,
                            change_type: VariableChangeType::TypeChanged {
                                old_type: var.var_type,
                                new_type: new_var_type,
                                new_type_name: type_name,
                            },
                            selected: true,
                        });
                    }
                }
                None => {
                    // Symbol not found in new ELF
                    changes.push(VariableChange {
                        variable_id: var.id,
                        variable_name: var.name.clone(),
                        current_address: var.address,
                        current_type: var.var_type,
                        change_type: VariableChangeType::NotFound,
                        selected: false, // Don't pre-select removal
                    });
                }
            }
        }

        // Only show dialog if there are changes
        if !changes.is_empty() {
            tracing::info!(
                "Detected {} variable changes after ELF reload",
                changes.len()
            );
            self.variable_change_state = VariableChangeState::with_changes(changes);
            self.variable_change_open = true;
        }
    }

    /// Render the variable change detection dialog using the Dialog trait
    fn render_variable_change_with_context(&mut self, ctx: &egui::Context) {
        if let Some(action) = show_dialog::<VariableChangeDialog>(
            ctx,
            &mut self.variable_change_open,
            &mut self.variable_change_state,
            VariableChangeContext,
        ) {
            match action {
                VariableChangeAction::UpdateSelected => {
                    self.apply_selected_variable_changes();
                }
                VariableChangeAction::UpdateAll => {
                    self.variable_change_state.select_all();
                    self.apply_selected_variable_changes();
                }
            }
        }
    }

    /// Apply selected variable changes from the dialog
    fn apply_selected_variable_changes(&mut self) {
        // Collect IDs to remove (NotFound with selected = true)
        let mut ids_to_remove: Vec<u32> = Vec::new();

        // Collect updates to apply (to avoid borrowing issues)
        let mut address_updates: Vec<(u32, u64)> = Vec::new();
        let mut type_updates: Vec<(u32, VariableType)> = Vec::new();

        for change in &self.variable_change_state.changes {
            if !change.selected {
                continue;
            }

            match &change.change_type {
                VariableChangeType::AddressChanged { new_address, .. } => {
                    address_updates.push((change.variable_id, *new_address));
                }
                VariableChangeType::TypeChanged { new_type, .. } => {
                    type_updates.push((change.variable_id, *new_type));
                }
                VariableChangeType::NotFound => {
                    ids_to_remove.push(change.variable_id);
                }
            }
        }

        // Apply address updates
        for (var_id, new_address) in address_updates {
            if let Some(var) = self.config.variables.iter_mut().find(|v| v.id == var_id) {
                tracing::info!(
                    "Updating variable '{}' address: 0x{:08X} -> 0x{:08X}",
                    var.name,
                    var.address,
                    new_address
                );
                var.address = new_address;
            }

            // Update in variable_data
            if let Some(data) = self.variable_data.get_mut(&var_id) {
                data.variable.address = new_address;
            }

            // Notify backend
            if let Some(var) = self.config.variables.iter().find(|v| v.id == var_id) {
                self.frontend.update_variable(var.clone());
            }
        }

        // Apply type updates
        for (var_id, new_type) in type_updates {
            if let Some(var) = self.config.variables.iter_mut().find(|v| v.id == var_id) {
                tracing::info!(
                    "Updating variable '{}' type: {} -> {}",
                    var.name,
                    var.var_type,
                    new_type
                );
                var.var_type = new_type;
            }

            // Update in variable_data
            if let Some(data) = self.variable_data.get_mut(&var_id) {
                data.variable.var_type = new_type;
            }

            // Notify backend
            if let Some(var) = self.config.variables.iter().find(|v| v.id == var_id) {
                self.frontend.update_variable(var.clone());
            }
        }

        // Remove not-found variables
        for id in ids_to_remove {
            if let Some(var) = self.config.variables.iter().find(|v| v.id == id) {
                tracing::info!("Removing missing variable '{}' (id: {})", var.name, id);
            }
            self.config.remove_variable(id);
            self.variable_data.remove(&id);
            self.frontend
                .send_command(BackendCommand::RemoveVariable(id));
        }
    }

    /// Handle global keyboard shortcuts
    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::Key;

        // Collect which shortcuts were triggered (to avoid borrow issues)
        let mut toggle_collection = false;
        let mut save_project = false;
        let mut nav_to: Option<AppPage> = None;
        let mut clear_data = false;
        let mut toggle_pause = false;

        ctx.input(|i| {
            // Space: Start/Stop collection (only on Visualizer page)
            if i.key_pressed(Key::Space)
                && !i.modifiers.any()
                && self.current_page == AppPage::Visualizer
            {
                toggle_collection = true;
            }

            // Ctrl+S / Cmd+S: Save project
            if i.key_pressed(Key::S) && i.modifiers.command_only() {
                save_project = true;
            }

            // Ctrl+1/2/3: Navigate to page
            if i.modifiers.command_only() {
                if i.key_pressed(Key::Num1) {
                    nav_to = Some(AppPage::Variables);
                } else if i.key_pressed(Key::Num2) {
                    nav_to = Some(AppPage::Visualizer);
                } else if i.key_pressed(Key::Num3) {
                    nav_to = Some(AppPage::Settings);
                }
            }

            // Ctrl+L: Clear all data
            if i.key_pressed(Key::L) && i.modifiers.command_only() {
                clear_data = true;
            }

            // P: Toggle pause (only when collecting)
            if i.key_pressed(Key::P)
                && !i.modifiers.any()
                && self.settings.collecting
                && self.current_page == AppPage::Visualizer
            {
                toggle_pause = true;
            }
        });

        // Apply collected actions
        if toggle_collection {
            if self.settings.collecting {
                self.settings.collecting = false;
                self.frontend.send_command(BackendCommand::StopCollection);
            } else {
                self.settings.collecting = true;
                self.frontend.send_command(BackendCommand::StartCollection);
            }
        }

        if save_project {
            if let Some(ref path) = self.settings_page.project_file_path {
                self.save_project_to_path(path.clone());
            } else {
                // Open save dialog
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Save Project")
                    .add_filter("DataVis Project", &["dvproj", "json"])
                    .save_file()
                {
                    self.save_project_to_path(path);
                }
            }
        }

        if let Some(page) = nav_to {
            self.current_page = page;
        }

        if clear_data {
            for data in self.variable_data.values_mut() {
                data.clear();
            }
        }

        if toggle_pause {
            self.settings.paused = !self.settings.paused;
        }
    }
}

impl eframe::App for DataVisApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process backend messages
        let had_messages = self.process_backend_messages();

        // Handle global keyboard shortcuts
        self.handle_keyboard_shortcuts(ctx);

        // Request continuous repaint when:
        // 1. Actively collecting and not paused
        // 2. Connected (to show status updates)
        // 3. Just received messages (to ensure UI stays responsive)
        if (self.settings.collecting && !self.settings.paused)
            || self.connection_status == ConnectionStatus::Connected
            || had_messages
        {
            ctx.request_repaint();
        }

        // Top navigation bar with page tabs
        egui::TopBottomPanel::top("nav_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("DataVis-RS");
                ui.separator();

                // Page tabs
                for page in [AppPage::Variables, AppPage::Visualizer, AppPage::Settings] {
                    let selected = self.current_page == page;
                    if ui.selectable_label(selected, page.name()).clicked() {
                        self.current_page = page;
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Connection status indicator
                    let (status_color, status_text) = match self.connection_status {
                        ConnectionStatus::Connected => (Color32::GREEN, "Connected"),
                        ConnectionStatus::Connecting => (Color32::YELLOW, "Connecting..."),
                        ConnectionStatus::Disconnected => (Color32::GRAY, "Disconnected"),
                        ConnectionStatus::Error => (Color32::RED, "Error"),
                    };
                    ui.colored_label(status_color, status_text);

                    // Show collection status on visualizer
                    if self.current_page == AppPage::Visualizer && self.settings.collecting {
                        if self.settings.paused {
                            ui.colored_label(Color32::YELLOW, "Paused");
                        } else {
                            ui.colored_label(Color32::GREEN, "Recording");
                        }
                    }
                });
            });
        });

        // Build shared state for pages
        let mut shared = SharedState {
            frontend: &self.frontend,
            connection_status: self.connection_status,
            config: &mut self.config,
            settings: &mut self.settings,
            app_state: &mut self.app_state,
            variable_data: &mut self.variable_data,
            stats: &self.stats,
            elf_info: self.elf_info.as_ref(),
            elf_symbols: &self.elf_symbols,
            elf_file_path: self.elf_file_path.as_ref(),
            persistence_config: &mut self.persistence_config,
            last_error: &mut self.last_error,
            start_time: self.start_time,
        };

        // Render current page and collect actions
        let actions = match self.current_page {
            AppPage::Variables => {
                VariablesPage::render(&mut self.variables_page, &mut shared, ctx)
            }
            AppPage::Visualizer => {
                VisualizerPage::render(&mut self.visualizer_page, &mut shared, ctx)
            }
            AppPage::Settings => {
                SettingsPage::render(&mut self.settings_page, &mut shared, ctx)
            }
        };

        // Drop shared state before handling actions (avoids borrow conflicts)
        drop(shared);

        // Handle actions from pages
        for action in actions {
            self.handle_action(action);
        }

        // Global dialogs (can appear on any page)
        self.render_variable_change_with_context(ctx);
        self.render_elf_symbols_with_context(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Signal backend to shutdown
        self.frontend.shutdown();

        // Update last connection info in app state
        self.app_state.update_last_connection(
            &self.config.probe.target_chip,
            self.config.probe.probe_selector.as_deref(),
        );

        // Save app state
        if let Err(e) = self.app_state.save() {
            tracing::warn!("Failed to save app state: {}", e);
        }

        // Auto-save current project if we have a path
        if let Some(ref path) = self.settings_page.project_file_path {
            let project = crate::config::ProjectFile {
                version: 1,
                name: self.settings_page.project_name.clone(),
                config: self.config.clone(),
                binary_path: self.elf_file_path.clone(),
                persistence: self.persistence_config.clone(),
            };

            if let Err(e) = project.save(path) {
                tracing::warn!("Failed to auto-save project on exit: {}", e);
            }
        }
    }
}
