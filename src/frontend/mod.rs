//! Frontend module for egui UI
//!
//! This module provides the main UI components using eframe/egui.
//! It receives data from the backend through crossbeam channels and
//! renders it in real-time.
//!
//! # Architecture
//!
//! The frontend uses an egui_dock workspace where every UI element is a pane:
//! variable browser, variable list, settings, time series, watcher, FFT view.
//! Panes can be rearranged via drag-and-drop docking.
//!
//! # Main Types
//!
//! - [`DataVisApp`] - Main application state implementing [`eframe::App`]
//! - [`Workspace`] - Dock state and pane management
//! - [`PlotView`] - Plot configuration and rendering
//!
//! # Submodules
//!
//! - `workspace` - Dock workspace, tab viewer, default layout
//! - `panes` - Individual pane render functions
//! - `panels` - Reusable panel components (connection, stats, etc.)
//! - `plot` - Plot rendering with egui_plot
//! - [`script_editor`] - Rhai script editor with syntax highlighting
//! - `widgets` - Custom UI widgets (status indicators, sparklines, etc.)

pub mod dialogs;
pub mod markers;
pub mod panes;
mod panels;
mod plot;
pub mod script_editor;
pub mod state;
pub mod widgets;
pub mod workspace;

pub use panels::*;
pub use plot::PlotView;
pub use script_editor::{ScriptEditor, ScriptEditorState};
pub use state::{AppAction, DialogId, SharedState};
pub use widgets::*;

use dialogs::{
    show_dialog, DuplicateConfirmState, ElfSymbolsAction, ElfSymbolsContext, ElfSymbolsDialog,
    ElfSymbolsState, VariableChangeAction, VariableChangeContext, VariableChangeDialog,
    VariableChangeState,
};
use workspace::tab_viewer::WorkspaceTabViewer;
use workspace::{PaneId, PaneKind, PaneState, Workspace};

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
#[allow(dead_code)]
#[derive(Debug)]
enum VariableAction {
    SetEnabled(bool),
    Edit,
    Remove,
}

/// Type of change detected for a variable when reloading ELF
#[derive(Debug, Clone)]
pub enum VariableChangeType {
    AddressChanged { old_address: u64, new_address: u64 },
    TypeChanged {
        old_type: VariableType,
        new_type: VariableType,
        new_type_name: String,
    },
    NotFound,
}

/// A detected change for a single variable
#[derive(Debug, Clone)]
pub struct VariableChange {
    pub variable_id: u32,
    pub variable_name: String,
    pub current_address: u64,
    pub current_type: VariableType,
    pub change_type: VariableChangeType,
    pub selected: bool,
}

/// Main application state for the data visualizer
#[allow(dead_code)]
pub struct DataVisApp {
    // === Communication ===
    frontend: FrontendReceiver,

    // === Shared State ===
    config: AppConfig,
    app_state: AppState,
    settings: RuntimeSettings,
    connection_status: ConnectionStatus,
    variable_data: HashMap<u32, VariableData>,
    stats: CollectionStats,
    start_time: Instant,
    last_error: Option<String>,
    persistence_config: crate::config::DataPersistenceConfig,

    // === ELF Data ===
    elf_file_path: Option<PathBuf>,
    elf_info: Option<ElfInfo>,
    elf_symbols: Vec<ElfSymbol>,

    // === Workspace (replaces page navigation) ===
    workspace: Workspace,

    // === Global Dialogs ===
    variable_change_open: bool,
    variable_change_state: VariableChangeState,
    elf_symbols_open: bool,
    elf_symbols_state: ElfSymbolsState,
    duplicate_confirm_open: bool,
    duplicate_confirm_state: DuplicateConfirmState,
}

/// State for variable autocomplete/selector (kept for compatibility)
#[allow(dead_code)]
#[derive(Default)]
pub struct VariableSelectorState {
    pub query: String,
    pub filtered_symbols: Vec<ElfSymbol>,
    pub selected_index: Option<usize>,
    pub dropdown_open: bool,
    pub cursor_position: usize,
    pub expanded_paths: std::collections::HashSet<String>,
}

impl VariableSelectorState {
    pub fn update_filter(&mut self, elf_info: Option<&ElfInfo>) {
        self.filtered_symbols.clear();
        if let Some(info) = elf_info {
            let results = info.search_variables(&self.query);
            self.filtered_symbols = results.into_iter().cloned().collect();
        }
        if self.filtered_symbols.is_empty() {
            self.selected_index = None;
        } else if let Some(idx) = self.selected_index {
            if idx >= self.filtered_symbols.len() {
                self.selected_index = Some(self.filtered_symbols.len() - 1);
            }
        }
    }

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

    pub fn selected_symbol(&self) -> Option<&ElfSymbol> {
        self.selected_index
            .and_then(|idx| self.filtered_symbols.get(idx))
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.filtered_symbols.clear();
        self.selected_index = None;
        self.dropdown_open = false;
        self.expanded_paths.clear();
    }

    pub fn toggle_expanded(&mut self, path: &str) {
        if self.expanded_paths.contains(path) {
            self.expanded_paths.retain(|p| !p.starts_with(path));
        } else {
            self.expanded_paths.insert(path.to_string());
        }
    }

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
    pub type_name: Option<String>,
    pub size: Option<u64>,
    pub script_editor_state: ScriptEditorState,
}

impl VariableEditorState {
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
            color: [100, 149, 237, 255],
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

        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.iter_mut().for_each(|(_, font_id)| {
            font_id.size *= app_state.ui_preferences.font_scale;
        });
        cc.egui_ctx.set_style(style);

        let project_name = if let Some(ref path) = project_path {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled Project")
                .to_string()
        } else {
            "Untitled Project".to_string()
        };

        crate::types::Variable::sync_next_id(&config.variables);

        let mut variable_data = HashMap::new();
        for var in &config.variables {
            variable_data.insert(var.id, VariableData::new(var.clone()));
        }

        for var in &config.variables {
            frontend.add_variable(var.clone());
        }

        let target_chip_input = config.probe.target_chip.clone();

        frontend.send_command(BackendCommand::RefreshProbes);

        // Build workspace with default layout
        let mut workspace = Workspace {
            dock_state: egui_dock::DockState::new(vec![]),
            pane_states: HashMap::new(),
            pane_entries: HashMap::new(),
        };
        let dock_state = workspace::default_layout::build_default_layout(&mut workspace);
        workspace.dock_state = dock_state;

        // Initialize settings pane state with target chip and project info
        if let Some(settings_pane_id) = workspace.find_singleton(PaneKind::Settings) {
            if let Some(PaneState::Settings(ref mut settings_state)) =
                workspace.pane_states.get_mut(&settings_pane_id)
            {
                settings_state.target_chip_input = target_chip_input;
                settings_state.project_name = project_name;
                settings_state.project_file_path = project_path.clone();
            }
        }

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
            workspace,
            variable_change_open: false,
            variable_change_state: VariableChangeState::default(),
            elf_symbols_open: false,
            elf_symbols_state: ElfSymbolsState::default(),
            duplicate_confirm_open: false,
            duplicate_confirm_state: DuplicateConfirmState::default(),
        }
    }

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
                    for var in vars {
                        if !self.variable_data.contains_key(&var.id) {
                            self.variable_data.insert(var.id, VariableData::new(var));
                        }
                    }
                }
                BackendMessage::ProbeList(probes) => {
                    // Update probes in settings pane
                    if let Some(settings_id) = self.workspace.find_singleton(PaneKind::Settings) {
                        if let Some(PaneState::Settings(ref mut s)) =
                            self.workspace.pane_states.get_mut(&settings_id)
                        {
                            // Reset selection if out of bounds
                            if let Some(idx) = s.selected_probe_index {
                                if idx >= probes.len() {
                                    s.selected_probe_index = None;
                                }
                            }
                            s.available_probes = probes.clone();
                        }
                    }
                    tracing::info!("Received {} probes", probes.len());
                }
                BackendMessage::Shutdown => {
                    tracing::info!("Backend shutdown received");
                }
                BackendMessage::WriteSuccess { variable_id } => {
                    tracing::info!("Successfully wrote to variable {}", variable_id);
                }
                BackendMessage::WriteError { variable_id, error } => {
                    tracing::error!("Failed to write to variable {}: {}", variable_id, error);
                    self.last_error = Some(format!("Write failed: {}", error));
                }
            }
        }

        had_messages
    }

    fn handle_action(&mut self, action: AppAction) {
        match action {
            AppAction::Connect {
                probe_selector,
                target,
            } => {
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
                self.frontend
                    .send_command(BackendCommand::SetMemoryAccessMode(mode));
            }
            AppAction::SetPollRate(rate) => {
                self.frontend
                    .send_command(BackendCommand::SetPollRate(rate));
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
                self.frontend
                    .send_command(BackendCommand::RemoveVariable(id));
            }
            AppAction::UpdateVariable(var) => {
                self.frontend
                    .send_command(BackendCommand::UpdateVariable(var));
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
            AppAction::OpenPane(kind) => {
                if kind.is_singleton() {
                    if let Some(id) = self.workspace.find_singleton(kind) {
                        // Focus existing singleton
                        if let Some(tab_location) = self.workspace.dock_state.find_tab(&id) {
                            self.workspace.dock_state.set_active_tab(tab_location);
                        }
                    } else {
                        let id = self.workspace.register_pane(kind, kind.display_name());
                        self.workspace.dock_state.push_to_first_leaf(id);
                    }
                } else {
                    let id = self.workspace.register_pane(kind, kind.display_name());
                    self.workspace.dock_state.push_to_first_leaf(id);
                }
            }
            AppAction::NewVisualizer(kind) => {
                let count = self
                    .workspace
                    .pane_entries
                    .values()
                    .filter(|e| e.kind == kind)
                    .count();
                let title = format!("{} {}", kind.display_name(), count + 1);
                let id = self.workspace.register_pane(kind, title);
                self.workspace.dock_state.push_to_first_leaf(id);
            }
            AppAction::ClosePane(id) => {
                self.workspace.remove_pane(id);
            }
        }
    }

    fn open_dialog(&mut self, dialog_id: DialogId) {
        match dialog_id {
            DialogId::AddVariable | DialogId::EditVariable(_) => {}
            DialogId::ConverterEditor(_) | DialogId::ValueEditor(_) | DialogId::VariableDetail(_) => {
            }
            DialogId::ElfSymbols => {
                self.elf_symbols_open = true;
            }
            DialogId::VariableChange => {
                self.variable_change_open = true;
            }
            DialogId::DuplicateConfirm => {}
        }
    }

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
                // Update variable browser filter
                if let Some(browser_id) = self.workspace.find_singleton(PaneKind::VariableBrowser) {
                    if let Some(PaneState::VariableBrowser(ref mut s)) =
                        self.workspace.pane_states.get_mut(&browser_id)
                    {
                        s.update_filter(self.elf_info.as_ref());
                    }
                }
                self.detect_variable_changes();
            }
            Err(e) => {
                self.last_error = Some(format!("Failed to parse ELF: {}", e));
                self.elf_info = None;
                self.elf_symbols.clear();
            }
        }
    }

    fn add_variable(&mut self, var: crate::types::Variable) {
        let is_duplicate = self
            .config
            .variables
            .iter()
            .any(|v| v.address == var.address);

        if is_duplicate {
            self.duplicate_confirm_state = DuplicateConfirmState::with_variable(var);
            self.duplicate_confirm_open = true;
        } else {
            self.add_variable_confirmed(var);
        }
    }

    fn add_variable_confirmed(&mut self, var: crate::types::Variable) {
        self.config.add_variable(var.clone());
        self.variable_data
            .insert(var.id, VariableData::new(var.clone()));
        self.frontend.add_variable(var);
    }

    fn clear_all_data(&mut self) {
        for data in self.variable_data.values_mut() {
            data.clear();
        }
        self.stats = CollectionStats::default();
    }

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
        // Get project name from settings pane
        let project_name = self
            .workspace
            .find_singleton(PaneKind::Settings)
            .and_then(|id| self.workspace.pane_states.get(&id))
            .and_then(|s| {
                if let PaneState::Settings(s) = s {
                    Some(s.project_name.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "Untitled Project".to_string());

        let project = crate::config::ProjectFile {
            version: 1,
            name: project_name.clone(),
            config: self.config.clone(),
            binary_path: self.elf_file_path.clone(),
            persistence: self.persistence_config.clone(),
        };

        match project.save(&path) {
            Ok(()) => {
                // Update project file path in settings pane
                if let Some(settings_id) = self.workspace.find_singleton(PaneKind::Settings) {
                    if let Some(PaneState::Settings(ref mut s)) =
                        self.workspace.pane_states.get_mut(&settings_id)
                    {
                        s.project_file_path = Some(path.clone());
                    }
                }

                self.app_state.add_recent_project(
                    &path,
                    &project_name,
                    Some(&self.config.probe.target_chip),
                );

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

    fn load_project_from_path(&mut self, path: PathBuf) {
        match crate::config::ProjectFile::load(&path) {
            Ok(project) => {
                self.config = project.config;
                self.persistence_config = project.persistence;

                crate::types::Variable::sync_next_id(&self.config.variables);

                // Update settings pane
                if let Some(settings_id) = self.workspace.find_singleton(PaneKind::Settings) {
                    if let Some(PaneState::Settings(ref mut s)) =
                        self.workspace.pane_states.get_mut(&settings_id)
                    {
                        s.project_name = project.name.clone();
                        s.project_file_path = Some(path.clone());
                        s.target_chip_input = self.config.probe.target_chip.clone();
                    }
                }

                self.app_state.add_recent_project(
                    &path,
                    &project.name,
                    Some(&self.config.probe.target_chip),
                );

                if let Err(e) = self.app_state.save() {
                    tracing::warn!("Failed to save app state: {}", e);
                }

                if let Some(binary_path) = project.binary_path {
                    self.elf_file_path = Some(binary_path.clone());
                    match crate::backend::parse_elf(&binary_path) {
                        Ok(info) => {
                            self.elf_symbols = info.symbols.clone();
                            self.elf_info = Some(info);
                            // Update variable browser filter
                            if let Some(browser_id) =
                                self.workspace.find_singleton(PaneKind::VariableBrowser)
                            {
                                if let Some(PaneState::VariableBrowser(ref mut s)) =
                                    self.workspace.pane_states.get_mut(&browser_id)
                                {
                                    s.update_filter(self.elf_info.as_ref());
                                }
                            }
                            tracing::info!("Loaded ELF from project: {:?}", binary_path);
                            self.detect_variable_changes();
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse ELF from project: {}", e);
                        }
                    }
                }

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

    fn detect_variable_changes(&mut self) {
        let elf_info = match &self.elf_info {
            Some(info) => info,
            None => return,
        };

        if self.config.variables.is_empty() {
            return;
        }

        let mut changes: Vec<VariableChange> = Vec::new();

        for var in &self.config.variables {
            let symbol = elf_info.find_symbol(&var.name);

            match symbol {
                Some(sym) => {
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
                            selected: true,
                        });
                    }

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
                    changes.push(VariableChange {
                        variable_id: var.id,
                        variable_name: var.name.clone(),
                        current_address: var.address,
                        current_type: var.var_type,
                        change_type: VariableChangeType::NotFound,
                        selected: false,
                    });
                }
            }
        }

        if !changes.is_empty() {
            tracing::info!(
                "Detected {} variable changes after ELF reload",
                changes.len()
            );
            self.variable_change_state = VariableChangeState::with_changes(changes);
            self.variable_change_open = true;
        }
    }

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

    fn apply_selected_variable_changes(&mut self) {
        let mut ids_to_remove: Vec<u32> = Vec::new();
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
            if let Some(data) = self.variable_data.get_mut(&var_id) {
                data.variable.address = new_address;
            }
            if let Some(var) = self.config.variables.iter().find(|v| v.id == var_id) {
                self.frontend.update_variable(var.clone());
            }
        }

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
            if let Some(data) = self.variable_data.get_mut(&var_id) {
                data.variable.var_type = new_type;
            }
            if let Some(var) = self.config.variables.iter().find(|v| v.id == var_id) {
                self.frontend.update_variable(var.clone());
            }
        }

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

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::Key;

        let mut toggle_collection = false;
        let mut save_project = false;
        let mut clear_data = false;
        let mut toggle_pause = false;

        ctx.input(|i| {
            if i.key_pressed(Key::Space) && !i.modifiers.any() {
                toggle_collection = true;
            }

            if i.key_pressed(Key::S) && i.modifiers.command_only() {
                save_project = true;
            }

            if i.key_pressed(Key::L) && i.modifiers.command_only() {
                clear_data = true;
            }

            if i.key_pressed(Key::P) && !i.modifiers.any() && self.settings.collecting {
                toggle_pause = true;
            }
        });

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
            // Get project file path from settings pane
            let project_file_path = self
                .workspace
                .find_singleton(PaneKind::Settings)
                .and_then(|id| self.workspace.pane_states.get(&id))
                .and_then(|s| {
                    if let PaneState::Settings(s) = s {
                        s.project_file_path.clone()
                    } else {
                        None
                    }
                });

            if let Some(path) = project_file_path {
                self.save_project_to_path(path);
            } else if let Some(path) = rfd::FileDialog::new()
                .set_title("Save Project")
                .add_filter("DataVis Project", &["dvproj", "json"])
                .save_file()
            {
                self.save_project_to_path(path);
            }
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

    /// Render pane dialogs that need &Context (called after dock area renders)
    fn render_pane_dialogs(&mut self, ctx: &egui::Context) {
        let mut actions = Vec::new();

        // We need to iterate pane states and call dialog rendering.
        // To avoid borrow issues with SharedState, we construct it per-pane.
        let pane_ids: Vec<PaneId> = self.workspace.pane_states.keys().copied().collect();

        for pane_id in pane_ids {
            let kind = self
                .workspace
                .pane_entries
                .get(&pane_id)
                .map(|e| e.kind);

            if let Some(kind) = kind {
                // For each pane that has dialogs, construct SharedState and call
                let state = self.workspace.pane_states.get_mut(&pane_id);
                if let Some(state) = state {
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

                    match (kind, state) {
                        (PaneKind::TimeSeries, PaneState::TimeSeries(s)) => {
                            panes::time_series::render_dialogs(s, &mut shared, ctx, &mut actions);
                        }
                        (PaneKind::VariableList, PaneState::VariableList(s)) => {
                            panes::variable_list::render_dialogs(s, &mut shared, ctx, &mut actions);
                        }
                        (PaneKind::Settings, PaneState::Settings(s)) => {
                            panes::settings::render_dialogs(s, &mut shared, ctx, &mut actions);
                        }
                        _ => {}
                    }
                }
            }
        }

        for action in actions {
            self.handle_action(action);
        }
    }
}

impl eframe::App for DataVisApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let had_messages = self.process_backend_messages();
        self.handle_keyboard_shortcuts(ctx);

        if (self.settings.collecting && !self.settings.paused)
            || self.connection_status == ConnectionStatus::Connected
            || had_messages
        {
            ctx.request_repaint();
        }

        // Menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Save Project").clicked() {
                        self.handle_action(AppAction::SaveProject(PathBuf::new()));
                        ui.close();
                    }
                    if ui.button("Load Project...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter(
                                "DataVis Project",
                                &[crate::config::PROJECT_FILE_EXTENSION],
                            )
                            .pick_file()
                        {
                            self.handle_action(AppAction::LoadProject(path));
                        }
                        ui.close();
                    }
                });

                ui.menu_button("View", |ui| {
                    // Singleton panes (open/focus)
                    if ui.button("Variable Browser").clicked() {
                        self.handle_action(AppAction::OpenPane(PaneKind::VariableBrowser));
                        ui.close();
                    }
                    if ui.button("Variables").clicked() {
                        self.handle_action(AppAction::OpenPane(PaneKind::VariableList));
                        ui.close();
                    }
                    if ui.button("Settings").clicked() {
                        self.handle_action(AppAction::OpenPane(PaneKind::Settings));
                        ui.close();
                    }

                    ui.separator();

                    // New visualizer instances
                    if ui.button("New Time Series").clicked() {
                        self.handle_action(AppAction::NewVisualizer(PaneKind::TimeSeries));
                        ui.close();
                    }
                    if ui.button("New Watcher").clicked() {
                        self.handle_action(AppAction::NewVisualizer(PaneKind::Watcher));
                        ui.close();
                    }
                    if ui.button("New FFT View").clicked() {
                        self.handle_action(AppAction::NewVisualizer(PaneKind::FftView));
                        ui.close();
                    }
                });

                // Right-aligned: connection status, collection controls
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (status_color, status_text) = match self.connection_status {
                        ConnectionStatus::Connected => (Color32::GREEN, "Connected"),
                        ConnectionStatus::Connecting => (Color32::YELLOW, "Connecting..."),
                        ConnectionStatus::Disconnected => (Color32::GRAY, "Disconnected"),
                        ConnectionStatus::Error => (Color32::RED, "Error"),
                    };
                    ui.colored_label(status_color, status_text);

                    if self.settings.collecting {
                        if self.settings.paused {
                            ui.colored_label(Color32::YELLOW, "Paused");
                        } else {
                            ui.colored_label(Color32::GREEN, "Recording");
                        }
                    }
                });
            });
        });

        // Dock workspace
        {
            let mut viewer = WorkspaceTabViewer {
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
                pane_states: &mut self.workspace.pane_states,
                pane_entries: &self.workspace.pane_entries,
                actions: Vec::new(),
            };

            egui_dock::DockArea::new(&mut self.workspace.dock_state)
                .style(egui_dock::Style::from_egui(ctx.style().as_ref()))
                .show(ctx, &mut viewer);

            let actions = viewer.actions;
            for action in actions {
                self.handle_action(action);
            }
        }

        // Render pane dialogs (require &Context)
        self.render_pane_dialogs(ctx);

        // Global dialogs
        self.render_variable_change_with_context(ctx);
        self.render_elf_symbols_with_context(ctx);

        // Duplicate confirm dialog
        if self.duplicate_confirm_open {
            use dialogs::{
                show_dialog, DuplicateConfirmAction, DuplicateConfirmContext, DuplicateConfirmDialog,
            };

            let dialog_ctx = DuplicateConfirmContext;
            if let Some(action) = show_dialog::<DuplicateConfirmDialog>(
                ctx,
                &mut self.duplicate_confirm_open,
                &mut self.duplicate_confirm_state,
                dialog_ctx,
            ) {
                match action {
                    DuplicateConfirmAction::Confirm(var) => {
                        self.add_variable_confirmed(var);
                    }
                }
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.frontend.shutdown();

        self.app_state.update_last_connection(
            &self.config.probe.target_chip,
            self.config.probe.probe_selector.as_deref(),
        );

        if let Err(e) = self.app_state.save() {
            tracing::warn!("Failed to save app state: {}", e);
        }

        // Auto-save from settings pane state
        let project_info = self
            .workspace
            .find_singleton(PaneKind::Settings)
            .and_then(|id| self.workspace.pane_states.get(&id))
            .and_then(|s| {
                if let PaneState::Settings(s) = s {
                    s.project_file_path.as_ref().map(|p| (p.clone(), s.project_name.clone()))
                } else {
                    None
                }
            });

        if let Some((path, name)) = project_info {
            let project = crate::config::ProjectFile {
                version: 1,
                name,
                config: self.config.clone(),
                binary_path: self.elf_file_path.clone(),
                persistence: self.persistence_config.clone(),
            };

            if let Err(e) = project.save(&path) {
                tracing::warn!("Failed to auto-save project on exit: {}", e);
            }
        }
    }
}
