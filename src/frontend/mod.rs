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
pub mod pane_registry;
pub mod pane_trait;
mod panels;
pub mod panes;
mod plot;
pub mod script_editor;
pub mod state;
pub mod status_bar;
pub mod toolbar;
pub mod topics;
pub mod widgets;
pub mod workspace;

pub use panels::*;
pub use plot::PlotView;
pub use script_editor::{ScriptEditor, ScriptEditorState};
pub use state::{AppAction, DialogId, SharedState};
pub use topics::Topics;
pub use widgets::*;

use dialogs::{
    show_dialog, CollectionSettingsAction, CollectionSettingsContext, CollectionSettingsDialog,
    CollectionSettingsState, ConnectionSettingsAction, ConnectionSettingsContext,
    ConnectionSettingsDialog, ConnectionSettingsState, DuplicateConfirmState, ElfSymbolsAction,
    ElfSymbolsContext, ElfSymbolsDialog, ElfSymbolsState, PersistenceSettingsAction,
    PersistenceSettingsContext, PersistenceSettingsDialog, PersistenceSettingsState,
    PreferencesAction, PreferencesContext, PreferencesDialog, PreferencesState,
    VariableChangeAction, VariableChangeContext, VariableChangeDialog, VariableChangeState,
};
use workspace::tab_viewer::WorkspaceTabViewer;
use workspace::{PaneId, PaneKind, Workspace};

use crate::backend::{parse_elf, ElfInfo, ElfSymbol};
use crate::config::{settings::RuntimeSettings, AppConfig, AppState};
use crate::pipeline::bridge::{PipelineBridge, PipelineCommand, SinkMessage};
use crate::types::{CollectionStats, ConnectionStatus, DataPoint, VariableData, VariableType};
use egui::Color32;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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
    AddressChanged {
        old_address: u64,
        new_address: u64,
    },
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
    frontend: PipelineBridge,

    // === Shared State ===
    config: AppConfig,
    app_state: AppState,
    settings: RuntimeSettings,
    start_time: Instant,
    /// Base time accumulated from previous start/stop cycles
    accumulated_time: Duration,
    /// Instant when the current collection session started (None if stopped)
    collection_start: Option<Instant>,
    last_error: Option<String>,
    persistence_config: crate::config::DataPersistenceConfig,

    // === All published data (variables, stats, status, snapshots) ===
    topics: Topics,

    // === ELF Data ===
    elf_file_path: Option<PathBuf>,
    elf_info: Option<ElfInfo>,
    elf_symbols: Vec<ElfSymbol>,

    // === Workspace (replaces page navigation) ===
    workspace: Workspace,

    // === Node-Pane Linkage ===
    /// Maps NodeId → PaneId for auto-created panes
    node_to_pane: std::collections::HashMap<crate::pipeline::id::NodeId, workspace::PaneId>,
    /// Maps PaneId → NodeId for auto-created panes
    pane_to_node: std::collections::HashMap<workspace::PaneId, crate::pipeline::id::NodeId>,

    // === Native menu bar (None on platforms without native menu support) ===
    #[allow(dead_code)]
    native_menu: Option<muda::Menu>,

    // === UI chrome visibility ===
    show_toolbar: bool,
    show_status_bar: bool,

    // === Connection state (shared between toolbar and quick-connect) ===
    selected_probe_index: Option<usize>,
    target_chip_input: String,

    // === Global Dialogs ===
    variable_change_open: bool,
    variable_change_state: VariableChangeState,
    elf_symbols_open: bool,
    elf_symbols_state: ElfSymbolsState,
    duplicate_confirm_open: bool,
    duplicate_confirm_state: DuplicateConfirmState,

    // === Connection dialog ===
    connection_dialog_open: bool,

    // === Settings dialogs (Phase 3) ===
    connection_settings_open: bool,
    connection_settings_state: ConnectionSettingsState,
    collection_settings_open: bool,
    collection_settings_state: CollectionSettingsState,
    persistence_settings_open: bool,
    persistence_settings_state: PersistenceSettingsState,
    preferences_open: bool,
    preferences_state: PreferencesState,

    // === Help dialog ===
    help_open: bool,

    // === UI Session State (for automatic persistence) ===
    ui_session: crate::config::UiSessionState,
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
        frontend: PipelineBridge,
        mut config: AppConfig,
        app_state: AppState,
        project_path: Option<PathBuf>,
        // Pipeline removed in Phase 3 - node_ids no longer needed
        native_menu: Option<muda::Menu>,
        ui_session: crate::config::UiSessionState,
    ) -> Self {
        // Configure fonts and styles
        let fonts = egui::FontDefinitions::default();
        cc.egui_ctx.set_fonts(fonts);

        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.iter_mut().for_each(|(_, font_id)| {
            font_id.size *= app_state.ui_preferences.font_scale;
        });
        cc.egui_ctx.set_style(style);

        // If no project is loaded but session has variables, restore them
        // This allows one-off debugging sessions to persist across app restarts
        if project_path.is_none() && !ui_session.variables.is_empty() {
            tracing::info!(
                "Restoring {} variables from UI session",
                ui_session.variables.len()
            );
            config.variables = ui_session.variables.clone();
        }

        // Restore ELF path from session and load if it exists
        let (elf_file_path, elf_info, elf_symbols) = if let Some(ref path) =
            ui_session.elf_file_path
        {
            if path.exists() {
                match crate::backend::parse_elf(path) {
                    Ok(info) => {
                        tracing::info!(
                            "Restored ELF from session: {} variables, {} functions",
                            info.variable_count(),
                            info.function_count()
                        );
                        let symbols: Vec<_> = info.get_variables().into_iter().cloned().collect();
                        (Some(path.clone()), Some(info), symbols)
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load ELF from session path {:?}: {}", path, e);
                        (None, None, Vec::new())
                    }
                }
            } else {
                tracing::warn!("ELF path from session no longer exists: {:?}", path);
                (None, None, Vec::new())
            }
        } else {
            (None, None, Vec::new())
        };

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
        for var in config.variables.values() {
            variable_data.insert(var.id, VariableData::new(var.clone()));
        }

        for var in config.variables.values() {
            frontend.add_variable(var.clone());
        }

        // Use target chip from UI session if available, otherwise from config
        let target_chip_input = if !ui_session.target_chip_input.is_empty() {
            ui_session.target_chip_input.clone()
        } else {
            config.probe.target_chip.clone()
        };

        frontend.send_command(PipelineCommand::RefreshProbes);

        // Build workspace - restore from session if available, otherwise use default layout
        let mut workspace = Workspace::new();
        let layout_restored = if let Some(ref layout) = ui_session.workspace_layout {
            workspace.restore_layout(layout)
        } else {
            false
        };

        if !layout_restored {
            tracing::info!("Using default workspace layout");
            let dock_state = workspace::default_layout::build_default_layout(&mut workspace);
            workspace.dock_state = dock_state;
        }

        let topics = Topics {
            variable_data,
            project_name,
            project_file_path: project_path.clone(),
            ..Topics::default()
        };

        // Use UI session values for chrome visibility
        let show_toolbar = ui_session.show_toolbar;
        let show_status_bar = ui_session.show_status_bar;
        let selected_probe_index = ui_session.selected_probe_index;

        Self {
            frontend,
            config,
            app_state,
            settings: RuntimeSettings::default(),
            start_time: Instant::now(),
            accumulated_time: Duration::ZERO,
            collection_start: None,
            last_error: None,
            persistence_config: crate::config::DataPersistenceConfig::default(),
            topics,
            elf_file_path,
            elf_info,
            elf_symbols,
            workspace,
            node_to_pane: std::collections::HashMap::new(),
            pane_to_node: std::collections::HashMap::new(),
            native_menu,
            show_toolbar,
            show_status_bar,
            selected_probe_index,
            target_chip_input,
            variable_change_open: false,
            variable_change_state: VariableChangeState::default(),
            elf_symbols_open: false,
            elf_symbols_state: ElfSymbolsState::default(),
            duplicate_confirm_open: false,
            duplicate_confirm_state: DuplicateConfirmState::default(),
            connection_dialog_open: false,
            connection_settings_open: false,
            connection_settings_state: ConnectionSettingsState::default(),
            collection_settings_open: false,
            collection_settings_state: CollectionSettingsState::default(),
            persistence_settings_open: false,
            persistence_settings_state: PersistenceSettingsState::default(),
            preferences_open: false,
            preferences_state: PreferencesState::default(),
            help_open: false,
            ui_session,
        }
    }

    fn process_backend_messages(&mut self) -> bool {
        let messages = self.frontend.drain();
        let had_messages = !messages.is_empty();

        for msg in messages {
            match msg {
                SinkMessage::ConnectionStatus(status) => {
                    self.topics.connection_status = status;
                    if status == ConnectionStatus::Connected {
                        self.last_error = None;
                    }
                }
                SinkMessage::ConnectionError(err) => {
                    self.last_error = Some(err);
                    self.topics.connection_status = ConnectionStatus::Error;
                }
                SinkMessage::DataBatch(batch) => {
                    // Skip adding data when paused - this effectively freezes the graph
                    if !self.settings.paused {
                        for (var_id, timestamp, raw_value, converted_value) in batch {
                            let variable_id = var_id.0;
                            if let Some(data) = self.topics.variable_data.get_mut(&variable_id) {
                                data.push(DataPoint::with_conversion(
                                    timestamp,
                                    raw_value,
                                    converted_value,
                                ));
                            }
                        }
                        // Record timestamp for global data freshness
                        self.topics.global_data_freshness = Some(std::time::Instant::now());
                    }
                }
                SinkMessage::GraphDataBatch { pane_id, data } => {
                    // Skip adding data when paused
                    if !self.settings.paused {
                        match pane_id {
                            Some(id) => {
                                // Route to specific pane's data store
                                let pane_data = self.topics.graph_pane_data.entry(id).or_default();
                                for (var_id, timestamp, raw_value, converted_value) in data {
                                    let variable_id = var_id.0;
                                    let var_data =
                                        pane_data.entry(variable_id).or_insert_with(|| {
                                            // Create a new VariableData for this variable in this pane
                                            if let Some(original) =
                                                self.topics.variable_data.get(&variable_id)
                                            {
                                                VariableData::new(original.variable.clone())
                                            } else {
                                                // Fallback: create a basic variable
                                                let var = crate::types::Variable::new(
                                                    format!("var_{}", variable_id),
                                                    0,
                                                    crate::types::VariableType::F32,
                                                );
                                                VariableData::new(var)
                                            }
                                        });
                                    var_data.push(DataPoint::with_conversion(
                                        timestamp,
                                        raw_value,
                                        converted_value,
                                    ));
                                }
                                // Record timestamp for pane-specific data freshness
                                self.topics
                                    .pane_data_freshness
                                    .insert(id, std::time::Instant::now());
                            }
                            None => {
                                // Broadcast to all panes - add to global variable_data
                                for (var_id, timestamp, raw_value, converted_value) in data {
                                    let variable_id = var_id.0;
                                    if let Some(data) =
                                        self.topics.variable_data.get_mut(&variable_id)
                                    {
                                        data.push(DataPoint::with_conversion(
                                            timestamp,
                                            raw_value,
                                            converted_value,
                                        ));
                                    }
                                }
                                // Record timestamp for global data freshness
                                self.topics.global_data_freshness = Some(std::time::Instant::now());
                            }
                        }
                    }
                }
                SinkMessage::ReadError { variable_id, error } => {
                    if let Some(data) = self.topics.variable_data.get_mut(&variable_id) {
                        data.record_error(error);
                    }
                }
                SinkMessage::Stats(stats) => {
                    self.topics.stats = stats;
                }
                SinkMessage::VariableList(vars) => {
                    for var in vars {
                        self.topics
                            .variable_data
                            .entry(var.id)
                            .or_insert_with(|| VariableData::new(var));
                    }
                }
                SinkMessage::ProbeList(probes) => {
                    tracing::info!("Received {} probes", probes.len());
                    // Reset selected index if it's now out of bounds
                    if let Some(idx) = self.selected_probe_index {
                        if idx >= probes.len() {
                            self.selected_probe_index = None;
                        }
                    }
                    // Auto-select the only probe if exactly one is available
                    if probes.len() == 1 && self.selected_probe_index.is_none() {
                        self.selected_probe_index = Some(0);
                    }
                    self.topics.available_probes = probes;
                }
                SinkMessage::Shutdown => {
                    tracing::info!("Backend shutdown received");
                }
                SinkMessage::WriteSuccess { variable_id } => {
                    tracing::info!("Successfully wrote to variable {}", variable_id);
                }
                SinkMessage::WriteError { variable_id, error } => {
                    tracing::error!("Failed to write to variable {}: {}", variable_id, error);
                    self.last_error = Some(format!("Write failed: {}", error));
                }
                SinkMessage::NodeError { node_id, message } => {
                    tracing::error!("Node {:?} error: {}", node_id, message);
                }
                SinkMessage::RecorderStatus { state, frame_count } => {
                    self.topics.recorder_state = state;
                    self.topics.recorder_frame_count = frame_count;
                }
                SinkMessage::ExporterStatus {
                    active,
                    rows_written,
                } => {
                    self.topics.exporter_active = active;
                    self.topics.exporter_rows_written = rows_written;
                }
                SinkMessage::VariableTreeSnapshot(snapshots) => {
                    self.topics.variable_tree = snapshots;
                }
                SinkMessage::RecordingComplete(recording) => {
                    tracing::info!("Recording complete: {} frames", recording.frames.len());
                    self.topics.completed_recordings.push(recording);
                }
            }
        }

        had_messages
    }

    /// Compute current display time (frozen when not collecting)
    fn display_time(&self) -> Duration {
        self.accumulated_time
            + self
                .collection_start
                .map(|s| s.elapsed())
                .unwrap_or(Duration::ZERO)
    }

    fn handle_action(&mut self, action: AppAction) {
        match action {
            AppAction::Connect {
                probe_selector,
                target,
            } => {
                self.frontend.send_command(PipelineCommand::Connect {
                    selector: probe_selector,
                    target,
                    probe_config: self.config.probe.clone(),
                });
            }
            AppAction::Disconnect => {
                self.frontend.send_command(PipelineCommand::Disconnect);
            }
            AppAction::StartCollection => {
                // Clear data on start to avoid timestamp discontinuity
                // (backend resets timestamps to 0 on each start)
                for data in self.topics.variable_data.values_mut() {
                    data.clear();
                }
                // Reset accumulated time since we're clearing the data
                self.accumulated_time = Duration::ZERO;
                self.settings.collecting = true;
                self.settings.paused = false; // Ensure not paused when starting
                self.collection_start = Some(Instant::now());
                self.frontend.send_command(PipelineCommand::Start);
            }
            AppAction::StopCollection => {
                self.settings.collecting = false;
                // Freeze: accumulate elapsed time from this session
                if let Some(start) = self.collection_start.take() {
                    self.accumulated_time += start.elapsed();
                }
                self.frontend.send_command(PipelineCommand::Stop);
            }
            AppAction::RefreshProbes => {
                tracing::debug!("Refreshing probe list...");
                self.frontend.send_command(PipelineCommand::RefreshProbes);
            }
            AppAction::SetMemoryAccessMode(mode) => {
                self.frontend
                    .send_command(PipelineCommand::SetMemoryAccessMode(mode));
            }
            AppAction::SetPollRate(rate) => {
                self.frontend
                    .send_command(PipelineCommand::SetPollRate(rate));
            }
            #[cfg(feature = "mock-probe")]
            AppAction::UseMockProbe(use_mock) => {
                self.frontend.use_mock_probe(use_mock);
            }
            AppAction::AddVariable(var) => {
                self.add_variable(var);
            }
            AppAction::AddStructVariable { parent, children } => {
                let parent_id = parent.id;
                let parent_color = parent.color;
                let child_count = children.len();
                self.add_variable_confirmed(parent);
                for (i, child_spec) in children.into_iter().enumerate() {
                    let mut child = crate::types::Variable::new(
                        &child_spec.name,
                        child_spec.address,
                        child_spec.var_type,
                    );
                    child.parent_id = Some(parent_id);
                    child.enabled = false;
                    child.show_in_graph = false;
                    child.color =
                        crate::types::Variable::generate_child_color(parent_color, i, child_count);
                    self.add_variable_confirmed(child);
                }
            }
            AppAction::AddPointerVariable {
                mut pointer,
                children,
                pointer_poll_rate_hz,
            } => {
                use crate::types::{PointerMetadata, PointerState};

                let pointer_id = pointer.id;
                let pointer_color = pointer.color;
                let child_count = children.len();

                // Set up pointer metadata on parent
                pointer.pointer_metadata = Some(PointerMetadata {
                    cached_address: None,
                    last_pointer_read: None,
                    pointer_poll_rate_hz,
                    pointer_parent_id: None,
                    offset_from_pointer: 0,
                    pointer_state: PointerState::Unread,
                });

                self.add_variable_confirmed(pointer);

                // Create dependent child variables
                for (i, child_spec) in children.into_iter().enumerate() {
                    let mut child = crate::types::Variable::new(
                        &child_spec.name,
                        0,
                        child_spec.var_type, // Address will be resolved at runtime
                    );
                    child.parent_id = Some(pointer_id);
                    child.enabled = false;
                    child.show_in_graph = false;
                    child.color =
                        crate::types::Variable::generate_child_color(pointer_color, i, child_count);

                    // Set up pointer metadata for dependent child
                    child.pointer_metadata = Some(PointerMetadata {
                        cached_address: None,
                        last_pointer_read: None,
                        pointer_poll_rate_hz: 0, // Children don't poll independently
                        pointer_parent_id: Some(pointer_id),
                        offset_from_pointer: child_spec.offset_from_pointer,
                        pointer_state: PointerState::Unread,
                    });

                    self.add_variable_confirmed(child);
                }
            }
            AppAction::RemoveVariable(id) => {
                // Remove children first
                let child_ids: Vec<u32> = self
                    .config
                    .variables
                    .values()
                    .filter(|v| v.parent_id == Some(id))
                    .map(|v| v.id)
                    .collect();
                for child_id in child_ids {
                    self.remove_variable_internal(child_id);
                }
                self.remove_variable_internal(id);
            }
            AppAction::UpdateVariable(var) => {
                self.frontend
                    .send_command(PipelineCommand::UpdateVariable(var));
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
                for data in self.topics.variable_data.values_mut() {
                    data.clear();
                }
            }
            AppAction::ClearVariableData(id) => {
                if let Some(data) = self.topics.variable_data.get_mut(&id) {
                    data.clear();
                }
            }
            AppAction::OpenPane(kind) => {
                if self.workspace.is_singleton(kind) {
                    if let Some(id) = self.workspace.find_singleton(kind) {
                        // Focus existing singleton
                        if let Some(tab_location) = self.workspace.dock_state.find_tab(&id) {
                            self.workspace.dock_state.set_active_tab(tab_location);
                        }
                    } else {
                        let name = self.workspace.display_name(kind);
                        let id = self.workspace.register_pane(kind, name);
                        self.workspace.dock_state.push_to_first_leaf(id);
                    }
                } else {
                    let name = self.workspace.display_name(kind);
                    let id = self.workspace.register_pane(kind, name);
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
                let display = self.workspace.display_name(kind);
                let title = format!("{} {}", display, count + 1);
                let id = self.workspace.register_pane(kind, title);
                self.workspace.dock_state.push_to_first_leaf(id);

                // Pipeline removed in Phase 3 - pane routing now handled by DataRouter
                // For TimeSeries panes, data routing is automatic via DataRouter
                if kind == PaneKind::TimeSeries {
                    // TODO: Send SubscribePane command to backend with pane variables
                    // For now, panes receive global data
                }
            }
            AppAction::NodeConfig {
                node_id,
                key,
                value,
            } => {
                self.frontend.send_command(PipelineCommand::NodeConfig {
                    node_id,
                    key,
                    value,
                });
            }
            AppAction::RequestTopology => {
                // Pipeline topology removed - no action needed
            }
            AppAction::ClosePane(id) => {
                // Pipeline node linkage removed - just close the pane
                self.workspace.remove_pane(id);
            }
            AppAction::NewProject => {
                // Reset to a fresh project
                self.config = crate::config::AppConfig::default();
                self.topics.project_name = "Untitled Project".to_string();
                self.topics.project_file_path = None;
                self.target_chip_input = self.config.probe.target_chip.clone();
                self.elf_file_path = None;
                self.elf_info = None;
                self.elf_symbols.clear();
                self.topics.variable_data.clear();
                self.topics.stats = CollectionStats::default();
                self.last_error = None;
                self.persistence_config = crate::config::DataPersistenceConfig::default();
            }
            AppAction::ResetLayout => {
                // Rebuild workspace with default layout
                let mut workspace = Workspace::new();
                let dock_state = workspace::default_layout::build_default_layout(&mut workspace);
                workspace.dock_state = dock_state;
                self.workspace = workspace;
            }
            AppAction::TogglePause => {
                let was_paused = self.settings.paused;
                self.settings.toggle_pause();

                // When resuming from pause, insert a NaN point to break the line
                // This prevents drawing a line across the time gap
                if was_paused && !self.settings.paused {
                    // Get the current display time for the gap marker
                    let gap_time = self.display_time();
                    for data in self.topics.variable_data.values_mut() {
                        data.push(crate::types::DataPoint::gap_marker(gap_time));
                    }
                }
            }
            AppAction::RenameVariable { id, new_name } => {
                let old_name = self.config.find_variable(id).map(|v| v.name.clone());
                if let Some(old_name) = old_name {
                    // Update parent name in config
                    if let Some(var) = self.config.find_variable_mut(id) {
                        var.name = new_name.clone();
                    }
                    // Update parent name in topics
                    if let Some(data) = self.topics.variable_data.get_mut(&id) {
                        data.variable.name = new_name.clone();
                    }
                    // Propagate prefix change to children
                    let child_ids: Vec<u32> = self
                        .config
                        .variables
                        .values()
                        .filter(|v| v.parent_id == Some(id))
                        .map(|v| v.id)
                        .collect();
                    for child_id in child_ids {
                        if let Some(child) = self.config.find_variable_mut(child_id) {
                            if let Some(suffix) = child.name.strip_prefix(&old_name) {
                                child.name = format!("{}{}", new_name, suffix);
                            }
                        }
                        if let Some(child_data) = self.topics.variable_data.get_mut(&child_id) {
                            if let Some(suffix) = child_data.variable.name.strip_prefix(&old_name) {
                                child_data.variable.name = format!("{}{}", new_name, suffix);
                            }
                        }
                    }
                }
            } // Pipeline actions removed in Phase 3
              // AppAction::AddPipelineNode, AddPipelineNodeWithConfig, etc. no longer exist
        }
    }

    fn open_dialog(&mut self, dialog_id: DialogId) {
        match dialog_id {
            DialogId::AddVariable | DialogId::EditVariable(_) => {}
            DialogId::ConverterEditor(_)
            | DialogId::ValueEditor(_)
            | DialogId::VariableDetail(_) => {}
            DialogId::ElfSymbols => {
                self.elf_symbols_open = true;
            }
            DialogId::VariableChange => {
                self.variable_change_open = true;
            }
            DialogId::DuplicateConfirm => {}
        }
    }

    fn load_elf(&mut self, path: &Path) {
        self.elf_file_path = Some(path.to_path_buf());
        match parse_elf(path) {
            Ok(info) => {
                tracing::info!(
                    "Parsed ELF: {} variables, {} functions",
                    info.variable_count(),
                    info.function_count()
                );
                self.elf_symbols = info.get_variables().into_iter().cloned().collect();
                self.elf_info = Some(info);
                // Signal ELF reload — VariableBrowser auto-refreshes via elf_generation
                self.topics.elf_generation += 1;
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
            .values()
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
        self.topics
            .variable_data
            .insert(var.id, VariableData::new(var.clone()));
        self.frontend.add_variable(var);
    }

    fn remove_variable_internal(&mut self, id: u32) {
        self.config.remove_variable(id);
        self.topics.variable_data.remove(&id);
        self.frontend
            .send_command(PipelineCommand::RemoveVariable(id));
    }

    fn clear_all_data(&mut self) {
        for data in self.topics.variable_data.values_mut() {
            data.clear();
        }
        self.topics.stats = CollectionStats::default();
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
                    let var =
                        crate::types::Variable::new(&symbol.display_name, symbol.address, var_type);
                    self.add_variable(var);
                }
            }
        }
    }

    fn save_project_to_path(&mut self, path: PathBuf) {
        let project_name = self.topics.project_name.clone();

        let project = crate::config::ProjectFile {
            version: 1,
            name: project_name.clone(),
            config: self.config.clone(),
            binary_path: self.elf_file_path.clone(),
            persistence: self.persistence_config.clone(),
        };

        match project.save(&path) {
            Ok(()) => {
                self.topics.project_file_path = Some(path.clone());

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

                // Update project metadata in Topics
                self.topics.project_name = project.name.clone();
                self.topics.project_file_path = Some(path.clone());

                // Update target chip input field (now on DataVisApp directly)
                self.target_chip_input = self.config.probe.target_chip.clone();

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
                            // Signal ELF reload — VariableBrowser auto-refreshes via elf_generation
                            self.topics.elf_generation += 1;
                            tracing::info!("Loaded ELF from project: {:?}", binary_path);
                            self.detect_variable_changes();
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse ELF from project: {}", e);
                        }
                    }
                }

                self.topics.variable_data.clear();
                for var in self.config.variables.values() {
                    self.topics
                        .variable_data
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

        for var in self.config.variables.values() {
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
            if let Some(var) = self.config.variables.get_mut(&var_id) {
                tracing::info!(
                    "Updating variable '{}' address: 0x{:08X} -> 0x{:08X}",
                    var.name,
                    var.address,
                    new_address
                );
                var.address = new_address;
            }
            if let Some(data) = self.topics.variable_data.get_mut(&var_id) {
                data.variable.address = new_address;
            }
            if let Some(var) = self.config.variables.get(&var_id) {
                self.frontend.update_variable(var.clone());
            }
        }

        for (var_id, new_type) in type_updates {
            if let Some(var) = self.config.variables.get_mut(&var_id) {
                tracing::info!(
                    "Updating variable '{}' type: {} -> {}",
                    var.name,
                    var.var_type,
                    new_type
                );
                var.var_type = new_type;
            }
            if let Some(data) = self.topics.variable_data.get_mut(&var_id) {
                data.variable.var_type = new_type;
            }
            if let Some(var) = self.config.variables.get(&var_id) {
                self.frontend.update_variable(var.clone());
            }
        }

        for id in ids_to_remove {
            if let Some(var) = self.config.variables.get(&id) {
                tracing::info!("Removing missing variable '{}' (id: {})", var.name, id);
            }
            self.config.remove_variable(id);
            self.topics.variable_data.remove(&id);
            self.frontend
                .send_command(PipelineCommand::RemoveVariable(id));
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
                self.frontend.send_command(PipelineCommand::Stop);
            } else {
                self.settings.collecting = true;
                self.frontend.send_command(PipelineCommand::Start);
            }
        }

        if save_project {
            let project_file_path = self.topics.project_file_path.clone();

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
            for data in self.topics.variable_data.values_mut() {
                data.clear();
            }
        }

        if toggle_pause {
            self.settings.paused = !self.settings.paused;
        }
    }

    /// Process native menu events from muda
    fn process_native_menu_events(&mut self) {
        use crate::menu::{MenuEvent, MenuId};

        // Poll for menu events
        while let Ok(event) = muda::MenuEvent::receiver().try_recv() {
            if let Some(menu_id) = MenuId::from_muda_id(&event.id) {
                if let Some(menu_event) = MenuEvent::from_menu_id(&menu_id) {
                    self.handle_menu_event(menu_event);
                }
            }
        }
    }

    /// Handle a menu event
    fn handle_menu_event(&mut self, event: crate::menu::MenuEvent) {
        use crate::menu::MenuEvent;

        match event {
            MenuEvent::Action(action) => {
                self.handle_action(*action);
            }
            MenuEvent::ToggleToolbar => {
                self.show_toolbar = !self.show_toolbar;
            }
            MenuEvent::ToggleStatusBar => {
                self.show_status_bar = !self.show_status_bar;
            }
            MenuEvent::OpenConnectionSettings => {
                self.connection_settings_state =
                    ConnectionSettingsState::from_config(&self.config.probe);
                self.connection_settings_open = true;
            }
            MenuEvent::OpenCollectionSettings => {
                self.collection_settings_state =
                    CollectionSettingsState::from_config(&self.config.collection);
                self.collection_settings_open = true;
            }
            MenuEvent::OpenPersistenceSettings => {
                self.persistence_settings_state =
                    PersistenceSettingsState::from_config(&self.persistence_config);
                self.persistence_settings_open = true;
            }
            MenuEvent::OpenPreferences => {
                self.preferences_state =
                    PreferencesState::from_config(&self.config.ui, &self.app_state.ui_preferences);
                self.preferences_open = true;
            }
            MenuEvent::OpenElfSymbols => {
                self.elf_symbols_state = ElfSymbolsState::default();
                self.elf_symbols_open = true;
            }
            MenuEvent::OpenHelp => {
                self.help_open = true;
            }
            MenuEvent::OpenShortcuts => {
                // TODO: Implement shortcuts dialog
                self.help_open = true;
            }
            MenuEvent::OpenAbout => {
                // TODO: Implement about dialog
                self.help_open = true;
            }
            MenuEvent::LoadElf => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Load ELF Binary")
                    .add_filter("ELF files", &["elf", "axf", "out", "bin"])
                    .pick_file()
                {
                    self.handle_action(AppAction::LoadElf(path));
                }
            }
            MenuEvent::OpenProject => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Open Project")
                    .add_filter("DataVis Project", &["dvproj", "json"])
                    .pick_file()
                {
                    self.handle_action(AppAction::LoadProject(path));
                }
            }
            MenuEvent::SaveProject => {
                if let Some(ref path) = self.topics.project_file_path {
                    self.save_project_to_path(path.clone());
                } else {
                    // Save As if no path
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title("Save Project")
                        .add_filter("DataVis Project", &["dvproj", "json"])
                        .save_file()
                    {
                        self.save_project_to_path(path);
                    }
                }
            }
            MenuEvent::SaveProjectAs => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Save Project As")
                    .add_filter("DataVis Project", &["dvproj", "json"])
                    .save_file()
                {
                    self.save_project_to_path(path);
                }
            }
            MenuEvent::LoadRecentProject(idx) => {
                if let Some(recent) = self.app_state.recent_projects.get(idx) {
                    let path = recent.path.clone();
                    self.handle_action(AppAction::LoadProject(path));
                }
            }
            MenuEvent::Quit => {
                std::process::exit(0);
            }
        }
    }

    /// Render the menu bar (Phase 2: restructured)
    fn render_menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                // === Project button ===
                let project_name = self.topics.project_name.clone();
                ui.menu_button(&project_name, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.topics.project_name);
                    });
                    ui.separator();
                    if ui.button("Save (Ctrl+S)").clicked() {
                        if let Some(ref path) = self.topics.project_file_path {
                            let p = path.clone();
                            self.handle_action(AppAction::SaveProject(p));
                        } else if let Some(path) = rfd::FileDialog::new()
                            .add_filter("DataVis Project", &[crate::config::PROJECT_FILE_EXTENSION])
                            .set_file_name("project.datavisproj")
                            .save_file()
                        {
                            self.handle_action(AppAction::SaveProject(path));
                        }
                        ui.close();
                    }
                    if ui.button("Save As...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("DataVis Project", &[crate::config::PROJECT_FILE_EXTENSION])
                            .set_file_name("project.datavisproj")
                            .save_file()
                        {
                            self.handle_action(AppAction::SaveProject(path));
                        }
                        ui.close();
                    }
                    ui.separator();
                    // Recent projects submenu
                    let recents: Vec<_> = self
                        .app_state
                        .recent_projects
                        .iter()
                        .map(|r| (r.path.clone(), r.name.clone()))
                        .collect();
                    if !recents.is_empty() {
                        ui.menu_button("Recent Projects", |ui| {
                            for (path, name) in &recents {
                                if ui.button(name).clicked() {
                                    self.handle_action(AppAction::LoadProject(path.clone()));
                                    ui.close();
                                }
                            }
                        });
                    }
                });

                // === File menu ===
                ui.menu_button("File", |ui| {
                    if ui.button("New Project").clicked() {
                        self.handle_action(AppAction::NewProject);
                        ui.close();
                    }
                    if ui.button("Open Project...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("DataVis Project", &[crate::config::PROJECT_FILE_EXTENSION])
                            .pick_file()
                        {
                            self.handle_action(AppAction::LoadProject(path));
                        }
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Save Project (Ctrl+S)").clicked() {
                        if let Some(ref path) = self.topics.project_file_path {
                            let p = path.clone();
                            self.handle_action(AppAction::SaveProject(p));
                        } else if let Some(path) = rfd::FileDialog::new()
                            .add_filter("DataVis Project", &[crate::config::PROJECT_FILE_EXTENSION])
                            .set_file_name("project.datavisproj")
                            .save_file()
                        {
                            self.handle_action(AppAction::SaveProject(path));
                        }
                        ui.close();
                    }
                    if ui.button("Save As...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("DataVis Project", &[crate::config::PROJECT_FILE_EXTENSION])
                            .set_file_name("project.datavisproj")
                            .save_file()
                        {
                            self.handle_action(AppAction::SaveProject(path));
                        }
                        ui.close();
                    }
                });

                // === View menu ===
                ui.menu_button("View", |ui| {
                    // Chrome toggles
                    if ui.checkbox(&mut self.show_toolbar, "Toolbar").changed() {
                        // Just toggling the bool
                    }
                    if ui
                        .checkbox(&mut self.show_status_bar, "Status Bar")
                        .changed()
                    {
                        // Just toggling the bool
                    }

                    ui.separator();

                    // Singleton panes (open/focus)
                    let singletons: Vec<_> = self
                        .workspace
                        .registry_singletons()
                        .map(|info| (info.kind, info.display_name))
                        .collect();
                    for (kind, name) in singletons {
                        if ui.button(name).clicked() {
                            self.handle_action(AppAction::OpenPane(kind));
                            ui.close();
                        }
                    }

                    ui.separator();

                    // Multi-instance visualizers
                    let multi: Vec<_> = self
                        .workspace
                        .registry_multi()
                        .map(|info| (info.kind, info.display_name))
                        .collect();
                    for (kind, name) in multi {
                        if ui.button(format!("New {}", name)).clicked() {
                            self.handle_action(AppAction::NewVisualizer(kind));
                            ui.close();
                        }
                    }

                    ui.separator();

                    if ui.button("Reset Layout").clicked() {
                        self.handle_action(AppAction::ResetLayout);
                        ui.close();
                    }
                });

                // === Tools menu ===
                ui.menu_button("Tools", |ui| {
                    if ui.button("Connection Settings...").clicked() {
                        self.connection_settings_state =
                            ConnectionSettingsState::from_config(&self.config.probe);
                        self.connection_settings_open = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Load ELF...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Load ELF Binary")
                            .add_filter("ELF files", &["elf", "axf", "out", "bin"])
                            .pick_file()
                        {
                            self.handle_action(AppAction::LoadElf(path));
                        }
                        ui.close();
                    }
                    if ui.button("Browse ELF Symbols...").clicked() {
                        self.elf_symbols_open = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Collection Settings...").clicked() {
                        self.collection_settings_state =
                            CollectionSettingsState::from_config(&self.config.collection);
                        self.collection_settings_open = true;
                        ui.close();
                    }
                    if ui.button("Data Persistence...").clicked() {
                        self.persistence_settings_state =
                            PersistenceSettingsState::from_config(&self.persistence_config);
                        self.persistence_settings_open = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Preferences...").clicked() {
                        self.preferences_state = PreferencesState::from_config(
                            &self.config.ui,
                            &self.app_state.ui_preferences,
                        );
                        self.preferences_open = true;
                        ui.close();
                    }
                });

                // === Help menu ===
                ui.menu_button("Help", |ui| {
                    if ui.button("Getting Started").clicked() {
                        self.help_open = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Keyboard Shortcuts").clicked() {
                        // TODO: Show keyboard shortcuts dialog
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("About DataVis").clicked() {
                        // TODO: Show about dialog
                        ui.close();
                    }
                });

                // === Right-aligned: Quick Connect dropdown ===
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.render_quick_connect(ui);
                });
            });
        });
    }

    /// Render the quick-connect area in the menu bar
    fn render_quick_connect(&mut self, ui: &mut egui::Ui) {
        let status = self.topics.connection_status;

        // When disconnected, show a prominent Connect button
        match status {
            ConnectionStatus::Disconnected | ConnectionStatus::Error => {
                // Show a real Button for connecting
                let btn = egui::Button::new(egui::RichText::new("Connect").strong())
                    .fill(egui::Color32::from_rgb(0, 100, 180));

                if ui
                    .add(btn)
                    .on_hover_text("Click to connect to a debug probe")
                    .clicked()
                {
                    self.connection_dialog_open = true;
                }
                return;
            }
            _ => {}
        }

        // When connected/connecting, show status dropdown
        let (btn_text, status_color) = match status {
            ConnectionStatus::Connected => ("Connected", Color32::GREEN),
            ConnectionStatus::Connecting => ("Connecting...", Color32::YELLOW),
            _ => ("Disconnected", Color32::GRAY),
        };

        let response = ui.menu_button(egui::RichText::new(btn_text).color(status_color), |ui| {
            ui.set_min_width(250.0);

            // Target chip
            ui.horizontal(|ui| {
                ui.label("Target:");
                ui.text_edit_singleline(&mut self.target_chip_input);
                if ui.button("Apply").clicked() {
                    self.config.probe.target_chip = self.target_chip_input.clone();
                }
            });

            // Probe selector (use submenu instead of ComboBox to avoid nested popup issues)
            ui.horizontal(|ui| {
                ui.label("Probe:");
                let probes = &self.topics.available_probes;
                let selected_text = self
                    .selected_probe_index
                    .and_then(|i| probes.get(i))
                    .map(|p| p.display_name())
                    .unwrap_or_else(|| "Select probe...".to_string());

                ui.menu_button(selected_text, |ui| {
                    if probes.is_empty() {
                        ui.label("No probes detected");
                    } else {
                        for (i, probe) in probes.iter().enumerate() {
                            let is_selected = self.selected_probe_index == Some(i);
                            if ui
                                .selectable_label(is_selected, probe.display_name())
                                .clicked()
                            {
                                self.selected_probe_index = Some(i);
                                ui.close();
                            }
                        }
                    }
                });

                if ui.button("Refresh").clicked() {
                    self.handle_action(AppAction::RefreshProbes);
                }
            });

            ui.separator();

            // Connect / Disconnect
            match status {
                ConnectionStatus::Connected => {
                    if ui.button("Disconnect").clicked() {
                        self.handle_action(AppAction::Disconnect);
                        ui.close();
                    }
                }
                ConnectionStatus::Connecting => {
                    ui.add_enabled(false, egui::Button::new("Connecting..."));
                }
                _ => {
                    let can_connect = self.selected_probe_index.is_some();
                    if ui
                        .add_enabled(can_connect, egui::Button::new("Connect"))
                        .clicked()
                    {
                        self.config.probe.target_chip = self.target_chip_input.clone();

                        #[cfg(feature = "mock-probe")]
                        if let Some(idx) = self.selected_probe_index {
                            if let Some(crate::backend::DetectedProbe::Mock(_)) =
                                self.topics.available_probes.get(idx)
                            {
                                self.handle_action(AppAction::UseMockProbe(true));
                            }
                        }

                        if let Some(idx) = self.selected_probe_index {
                            let selector = match self.topics.available_probes.get(idx) {
                                Some(crate::backend::DetectedProbe::Real(info)) => {
                                    Some(format!("{:04x}:{:04x}", info.vendor_id, info.product_id))
                                }
                                #[cfg(feature = "mock-probe")]
                                Some(crate::backend::DetectedProbe::Mock(_)) => None,
                                _ => None,
                            };
                            self.handle_action(AppAction::Connect {
                                probe_selector: selector,
                                target: self.target_chip_input.clone(),
                            });
                        }
                        ui.close();
                    }
                }
            }

            ui.separator();
            if ui.small_button("Connection Settings...").clicked() {
                self.connection_settings_state =
                    ConnectionSettingsState::from_config(&self.config.probe);
                self.connection_settings_open = true;
                ui.close();
            }
        });
        let _ = response;
    }

    /// Render the settings dialogs (connection, collection, persistence, preferences)
    fn render_settings_dialogs(&mut self, ctx: &egui::Context) {
        // Connection dialog (main connect UI)
        if self.connection_dialog_open {
            let mut should_close = false;
            egui::Window::new("Connect to Probe")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_min_width(300.0);

                    ui.heading("Debug Probe Connection");
                    ui.add_space(8.0);

                    // Target chip
                    ui.horizontal(|ui| {
                        ui.label("Target Chip:");
                        ui.text_edit_singleline(&mut self.target_chip_input);
                    });

                    ui.add_space(4.0);

                    // Probe selector
                    ui.horizontal(|ui| {
                        ui.label("Probe:");
                        let probes = &self.topics.available_probes;
                        let selected_text = self
                            .selected_probe_index
                            .and_then(|i| probes.get(i))
                            .map(|p| p.display_name())
                            .unwrap_or_else(|| "-- Select --".to_string());

                        egui::ComboBox::from_id_salt("connection_dialog_probe")
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                if probes.is_empty() {
                                    ui.label("No probes detected");
                                } else {
                                    for (i, probe) in probes.iter().enumerate() {
                                        let is_selected = self.selected_probe_index == Some(i);
                                        if ui
                                            .selectable_label(is_selected, probe.display_name())
                                            .clicked()
                                        {
                                            self.selected_probe_index = Some(i);
                                        }
                                    }
                                }
                            });

                        if ui.button("Refresh").clicked() {
                            self.handle_action(AppAction::RefreshProbes);
                        }
                    });

                    if self.topics.available_probes.is_empty() {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(
                                "No probes found. Click Refresh or connect a probe.",
                            )
                            .color(Color32::GRAY)
                            .italics(),
                        );
                    }

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Buttons
                    ui.horizontal(|ui| {
                        let can_connect = self.selected_probe_index.is_some();

                        if ui
                            .add_enabled(can_connect, egui::Button::new("Connect"))
                            .clicked()
                        {
                            self.config.probe.target_chip = self.target_chip_input.clone();

                            #[cfg(feature = "mock-probe")]
                            if let Some(idx) = self.selected_probe_index {
                                if let Some(crate::backend::DetectedProbe::Mock(_)) =
                                    self.topics.available_probes.get(idx)
                                {
                                    self.handle_action(AppAction::UseMockProbe(true));
                                }
                            }

                            if let Some(idx) = self.selected_probe_index {
                                let selector = match self.topics.available_probes.get(idx) {
                                    Some(crate::backend::DetectedProbe::Real(info)) => Some(
                                        format!("{:04x}:{:04x}", info.vendor_id, info.product_id),
                                    ),
                                    #[cfg(feature = "mock-probe")]
                                    Some(crate::backend::DetectedProbe::Mock(_)) => None,
                                    _ => None,
                                };
                                self.handle_action(AppAction::Connect {
                                    probe_selector: selector,
                                    target: self.target_chip_input.clone(),
                                });
                            }
                            should_close = true;
                        }

                        if ui.button("Cancel").clicked() {
                            should_close = true;
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Settings...").clicked() {
                                self.connection_settings_state =
                                    ConnectionSettingsState::from_config(&self.config.probe);
                                self.connection_settings_open = true;
                            }
                        });
                    });
                });

            if should_close {
                self.connection_dialog_open = false;
            }
        }

        // Connection settings dialog
        if self.connection_settings_open {
            // Sync state from config when opening
            let dialog_ctx = ConnectionSettingsContext;
            if let Some(action) = show_dialog::<ConnectionSettingsDialog>(
                ctx,
                &mut self.connection_settings_open,
                &mut self.connection_settings_state,
                dialog_ctx,
            ) {
                match action {
                    ConnectionSettingsAction::Apply(state) => {
                        self.config.probe.speed_khz = state.speed_khz;
                        self.config.probe.connect_under_reset = state.connect_under_reset;
                        self.config.probe.halt_on_connect = state.halt_on_connect;
                        let old_mode = self.config.probe.memory_access_mode;
                        self.config.probe.memory_access_mode = state.memory_access_mode;
                        self.config.probe.usb_timeout_ms = state.usb_timeout_ms;
                        self.config.probe.bulk_read_gap_threshold = state.bulk_read_gap_threshold;
                        if self.config.probe.memory_access_mode != old_mode {
                            self.handle_action(AppAction::SetMemoryAccessMode(
                                self.config.probe.memory_access_mode,
                            ));
                        }
                    }
                }
            }
        }

        // Collection settings dialog
        if self.collection_settings_open {
            let dialog_ctx = CollectionSettingsContext;
            if let Some(action) = show_dialog::<CollectionSettingsDialog>(
                ctx,
                &mut self.collection_settings_open,
                &mut self.collection_settings_state,
                dialog_ctx,
            ) {
                match action {
                    CollectionSettingsAction::Apply(state) => {
                        let old_rate = self.config.collection.poll_rate_hz;
                        self.config.collection.poll_rate_hz = state.poll_rate_hz;
                        self.config.collection.max_data_points = state.max_data_points;
                        self.config.collection.timeout_ms = state.timeout_ms;
                        if self.config.collection.poll_rate_hz != old_rate {
                            self.handle_action(AppAction::SetPollRate(
                                self.config.collection.poll_rate_hz,
                            ));
                        }
                    }
                }
            }
        }

        // Persistence settings dialog
        if self.persistence_settings_open {
            let dialog_ctx = PersistenceSettingsContext;
            if let Some(action) = show_dialog::<PersistenceSettingsDialog>(
                ctx,
                &mut self.persistence_settings_open,
                &mut self.persistence_settings_state,
                dialog_ctx,
            ) {
                match action {
                    PersistenceSettingsAction::Apply(state) => {
                        self.persistence_config = state.to_config();
                    }
                }
            }
        }

        // Preferences dialog
        if self.preferences_open {
            let dialog_ctx = PreferencesContext;
            if let Some(action) = show_dialog::<PreferencesDialog>(
                ctx,
                &mut self.preferences_open,
                &mut self.preferences_state,
                dialog_ctx,
            ) {
                match action {
                    PreferencesAction::Apply(state) => {
                        self.app_state.ui_preferences.dark_mode = state.dark_mode;
                        self.app_state.ui_preferences.font_scale = state.font_scale;
                        self.app_state.ui_preferences.language = state.language;
                        self.config.ui.show_grid = state.show_grid;
                        self.config.ui.show_legend = state.show_legend;
                        self.config.ui.auto_scale_y = state.auto_scale_y;
                        self.config.ui.line_width = state.line_width;
                        self.config.ui.time_window_seconds = state.time_window_seconds;
                        self.config.ui.show_raw_values = state.show_raw_values;
                        // Sync runtime settings with config
                        self.settings.display_time_window = state.time_window_seconds;
                        // Apply language change
                        crate::i18n::set_language(state.language);
                    }
                }
            }
        }

        // Help dialog
        if self.help_open {
            egui::Window::new("Getting Started")
                .collapsible(true)
                .resizable(true)
                .default_width(450.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.heading("DataVis - SWD Data Visualizer");
                        ui.add_space(8.0);

                        ui.label("A real-time variable visualization tool for embedded systems debugging.");
                        ui.add_space(12.0);

                        ui.heading("Quick Start");
                        ui.add_space(4.0);
                        ui.label("1. Click the 'Connect' button in the menu bar");
                        ui.label("2. Select your debug probe and target chip");
                        ui.label("3. Load an ELF file (Tools > Load ELF...)");
                        ui.label("4. Browse and add variables from the Variable Browser");
                        ui.label("5. Press Space to start/stop data collection");
                        ui.add_space(12.0);

                        ui.heading("Keyboard Shortcuts");
                        ui.add_space(4.0);
                        egui::Grid::new("shortcuts_grid").show(ui, |ui| {
                            ui.label("Space");
                            ui.label("Start/Stop collection");
                            ui.end_row();

                            ui.label("Ctrl+S");
                            ui.label("Save project");
                            ui.end_row();

                            ui.label("Ctrl+L");
                            ui.label("Clear data");
                            ui.end_row();

                            ui.label("P");
                            ui.label("Pause/Resume (while collecting)");
                            ui.end_row();
                        });
                        ui.add_space(12.0);

                        ui.heading("Panes");
                        ui.add_space(4.0);
                        ui.label("- Variable Browser: Browse and add variables from ELF");
                        ui.label("- Variable List: Manage added variables");
                        ui.label("- Time Series: Real-time data plot");
                        ui.label("- FFT View: Frequency analysis");
                        ui.label("- Watcher: Monitor variable values");
                        ui.add_space(12.0);

                        ui.separator();
                        ui.add_space(4.0);
                        if ui.button("Close").clicked() {
                            self.help_open = false;
                        }
                    });
                });
        }
    }

    /// Render pane dialogs that need &Context (called after dock area renders)
    fn render_pane_dialogs(&mut self, ctx: &egui::Context) {
        let mut actions = Vec::new();
        let display_time = self.display_time().as_secs_f64();

        // Iterate all panes and call render_dialogs via trait dispatch.
        let pane_ids: Vec<PaneId> = self.workspace.pane_states.keys().copied().collect();

        for pane_id in pane_ids {
            if let Some(pane) = self.workspace.pane_states.get_mut(&pane_id) {
                let mut shared = SharedState {
                    frontend: &self.frontend,
                    config: &mut self.config,
                    settings: &mut self.settings,
                    app_state: &mut self.app_state,
                    elf_info: self.elf_info.as_ref(),
                    elf_symbols: &self.elf_symbols,
                    elf_file_path: self.elf_file_path.as_ref(),
                    persistence_config: &mut self.persistence_config,
                    last_error: &mut self.last_error,
                    display_time,
                    topics: &mut self.topics,
                    current_pane_id: Some(pane_id),
                };

                let pane_actions = pane.render_dialogs(&mut shared, ctx);
                actions.extend(pane_actions);
            }
        }

        for action in actions {
            self.handle_action(action);
        }
    }
}

impl eframe::App for DataVisApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Capture window state for session persistence
        self.capture_window_state(ctx);

        let had_messages = self.process_backend_messages();
        self.handle_keyboard_shortcuts(ctx);

        // Process native menu events (if using native menus)
        self.process_native_menu_events();

        if (self.settings.collecting && !self.settings.paused)
            || self.topics.connection_status == ConnectionStatus::Connected
            || had_messages
        {
            ctx.request_repaint();
        }

        // 1. Menu bar (only render egui menu on Linux where native menus aren't supported)
        #[cfg(target_os = "linux")]
        self.render_menu_bar(ctx);

        // 2. Toolbar (if visible)
        if self.show_toolbar {
            let toolbar_result = egui::TopBottomPanel::top("toolbar")
                .show(ctx, |ui| {
                    let toolbar_ctx = toolbar::ToolbarContext {
                        topics: &self.topics,
                        config: &self.config,
                        settings: &self.settings,
                        elf_file_path: self.elf_file_path.as_ref(),
                        selected_probe_index: self.selected_probe_index,
                        target_chip_input: self.target_chip_input.clone(),
                    };
                    toolbar::render_toolbar(ui, &toolbar_ctx)
                })
                .inner;

            // Apply state changes from toolbar
            if let Some(idx) = toolbar_result.state_changes.selected_probe_index {
                self.selected_probe_index = idx;
            }
            if let Some(target) = toolbar_result.state_changes.target_chip_input {
                self.target_chip_input = target;
            }

            // Handle actions
            for action in toolbar_result.actions {
                self.handle_action(action);
            }
        }

        // 3. Status bar (if visible)
        if self.show_status_bar {
            egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
                let status_ctx = status_bar::StatusBarContext {
                    topics: &self.topics,
                    target_chip: &self.config.probe.target_chip,
                    last_error: self.last_error.as_deref(),
                };
                status_bar::render_status_bar(ui, &status_ctx);
            });
        }

        // 4. Dock workspace
        {
            let display_time = self.display_time().as_secs_f64();
            let singleton_pane_kinds: Vec<_> = self
                .workspace
                .registry_singletons()
                .map(|info| (info.kind, info.display_name))
                .collect();
            let multi_pane_kinds: Vec<_> = self
                .workspace
                .registry_multi()
                .map(|info| (info.kind, info.display_name))
                .collect();

            let mut viewer = WorkspaceTabViewer {
                frontend: &self.frontend,
                config: &mut self.config,
                settings: &mut self.settings,
                app_state: &mut self.app_state,
                elf_info: self.elf_info.as_ref(),
                elf_symbols: &self.elf_symbols,
                elf_file_path: self.elf_file_path.as_ref(),
                persistence_config: &mut self.persistence_config,
                last_error: &mut self.last_error,
                display_time,
                topics: &mut self.topics,
                pane_states: &mut self.workspace.pane_states,
                pane_entries: &self.workspace.pane_entries,
                actions: Vec::new(),
                singleton_pane_kinds,
                multi_pane_kinds,
            };

            egui_dock::DockArea::new(&mut self.workspace.dock_state)
                .style(egui_dock::Style::from_egui(ctx.style().as_ref()))
                .show_add_buttons(true)
                .show_add_popup(true)
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

        // Settings dialogs
        self.render_settings_dialogs(ctx);

        // Duplicate confirm dialog
        if self.duplicate_confirm_open {
            use dialogs::{
                show_dialog, DuplicateConfirmAction, DuplicateConfirmContext,
                DuplicateConfirmDialog,
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

        // Auto-save from Topics
        let project_info = self
            .topics
            .project_file_path
            .as_ref()
            .map(|p| (p.clone(), self.topics.project_name.clone()));

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

        // Save UI session state
        self.save_ui_session();
    }
}

impl DataVisApp {
    /// Capture current window state from egui context
    fn capture_window_state(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            if let Some(rect) = i.viewport().outer_rect {
                self.ui_session.window.position = Some((rect.min.x as i32, rect.min.y as i32));
                self.ui_session.window.size = (rect.width() as u32, rect.height() as u32);
            }
            if let Some(maximized) = i.viewport().maximized {
                self.ui_session.window.maximized = maximized;
            }
        });
    }

    /// Save the current UI session state for restoration on next launch
    fn save_ui_session(&mut self) {
        // Update session state with current values
        self.ui_session.show_toolbar = self.show_toolbar;
        self.ui_session.show_status_bar = self.show_status_bar;
        self.ui_session.selected_probe_index = self.selected_probe_index;
        self.ui_session.target_chip_input = self.target_chip_input.clone();
        self.ui_session.last_project_path = self.topics.project_file_path.clone();

        // Save ELF file path
        self.ui_session.elf_file_path = self.elf_file_path.clone();

        // Save variables (for one-off sessions without explicit project save)
        self.ui_session.variables = self.config.variables.clone();

        // Serialize workspace layout
        self.ui_session.workspace_layout = self.workspace.serialize_layout();

        // Save to disk
        if let Err(e) = self.ui_session.save() {
            tracing::warn!("Failed to save UI session state: {}", e);
        }
    }
}
