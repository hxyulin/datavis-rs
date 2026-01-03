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

/// Maximum number of array elements to display when expanding an array.
/// This prevents UI slowdown for very large arrays.
const MAX_ARRAY_ELEMENTS: u64 = 1024;

mod panels;
mod plot;
pub mod script_editor;
mod widgets;

pub use panels::*;
pub use plot::PlotView;
pub use script_editor::{ScriptEditor, ScriptEditorState};
pub use widgets::*;

use crate::backend::{
    parse_elf, BackendCommand, BackendMessage, DetectedProbe, ElfInfo, ElfSymbol, FrontendReceiver,
    TypeHandle,
};
use crate::config::{settings::RuntimeSettings, AppConfig, AppState};
use crate::types::{CollectionStats, ConnectionStatus, DataPoint, VariableData};
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

/// The different pages/views in the application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppPage {
    /// Variable selection and management page
    #[default]
    Variables,
    /// Data visualization page with plots
    Visualizer,
    /// Application settings page
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
            AppPage::Variables => "📋",
            AppPage::Visualizer => "📈",
            AppPage::Settings => "⚙",
        }
    }
}

/// Main application state for the data visualizer
///
/// This struct holds all UI state for the application including
/// connection status, variable data, configuration, and dialog states.
#[allow(dead_code)]
pub struct DataVisApp {
    /// Frontend receiver for backend communication
    frontend: FrontendReceiver,
    /// Application configuration (from current project)
    config: AppConfig,
    /// Application state (persisted across sessions)
    app_state: AppState,
    /// Current project name
    project_name: String,
    /// Runtime settings
    settings: RuntimeSettings,
    /// Pending variable to add (for duplicate confirmation)
    pending_variable: Option<crate::types::Variable>,
    /// Show duplicate variable confirmation dialog
    show_duplicate_confirm: bool,
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
    /// Whether the side panel is open
    side_panel_open: bool,
    /// Whether the bottom panel is open
    bottom_panel_open: bool,
    /// Selected variable ID for details view
    selected_variable_id: Option<u32>,
    /// Target chip input
    target_chip_input: String,
    /// Variable editor state
    variable_editor: VariableEditorState,
    /// Show add variable dialog
    show_add_variable_dialog: bool,
    /// Show converter editor dialog (for editing just the converter script)
    show_converter_editor: bool,
    /// Variable ID being edited in converter editor
    converter_editor_var_id: Option<u32>,
    /// Converter script being edited
    converter_editor_script: String,
    /// Script editor state for converter editor
    converter_editor_state: ScriptEditorState,
    /// Show settings dialog
    show_settings_dialog: bool,
    /// Available probes (real + mock)
    available_probes: Vec<DetectedProbe>,
    /// Selected probe index
    selected_probe_index: Option<usize>,
    /// Loaded ELF/AXF file path
    elf_file_path: Option<PathBuf>,
    /// Parsed ELF info (contains symbols and type info)
    elf_info: Option<ElfInfo>,
    /// Symbols parsed from ELF file (for display)
    elf_symbols: Vec<ElfSymbol>,
    /// Show ELF symbols dialog
    show_elf_symbols_dialog: bool,
    /// Search filter for ELF symbols
    elf_symbol_filter: String,
    /// Whether using mock probe (only with mock-probe feature)
    #[cfg(feature = "mock-probe")]
    use_mock_probe: bool,
    /// Variable selector state for autocomplete
    variable_selector: VariableSelectorState,
    /// Show variable selector popup
    show_variable_selector: bool,
    /// Current page/view being displayed
    current_page: AppPage,
    /// Path to the current project file (.datavisproj)
    project_file_path: Option<PathBuf>,
    /// Data persistence configuration
    persistence_config: crate::config::DataPersistenceConfig,
    /// Show value editor dialog
    show_value_editor: bool,
    /// Variable ID being edited in value editor
    value_editor_var_id: Option<u32>,
    /// Value being edited (as string for text input)
    value_editor_input: String,
    /// Error message for value editor (e.g., parse error)
    value_editor_error: Option<String>,
    /// Show variable detail dialog (comprehensive variable editor)
    show_variable_detail: bool,
    /// Variable ID being viewed/edited in detail dialog
    variable_detail_id: Option<u32>,
    /// Temporary color for the variable detail editor
    variable_detail_color: [u8; 4],
    /// Temporary name for the variable detail editor
    variable_detail_name: String,
    /// Temporary unit for the variable detail editor
    variable_detail_unit: String,
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

        Self {
            frontend,
            config,
            app_state,
            project_name,
            settings: RuntimeSettings::default(),
            connection_status: ConnectionStatus::Disconnected,
            variable_data,
            stats: CollectionStats::default(),
            start_time: Instant::now(),
            last_error: None,
            side_panel_open: true,
            bottom_panel_open: true,
            selected_variable_id: None,
            target_chip_input,
            variable_editor: VariableEditorState::default(),
            show_add_variable_dialog: false,
            show_converter_editor: false,
            converter_editor_var_id: None,
            converter_editor_script: String::new(),
            converter_editor_state: ScriptEditorState::default(),
            show_settings_dialog: false,
            available_probes: Vec::new(), // Will be populated by async response
            selected_probe_index: None,
            elf_file_path: None,
            elf_info: None,
            elf_symbols: Vec::new(),
            show_elf_symbols_dialog: false,
            elf_symbol_filter: String::new(),
            #[cfg(feature = "mock-probe")]
            use_mock_probe: false,
            variable_selector: VariableSelectorState::default(),
            show_variable_selector: false,
            current_page: AppPage::default(),
            pending_variable: None,
            show_duplicate_confirm: false,
            project_file_path: project_path,
            persistence_config: crate::config::DataPersistenceConfig::default(),
            show_value_editor: false,
            value_editor_var_id: None,
            value_editor_input: String::new(),
            value_editor_error: None,
            show_variable_detail: false,
            variable_detail_id: None,
            variable_detail_color: [0, 0, 0, 255],
            variable_detail_name: String::new(),
            variable_detail_unit: String::new(),
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
                    self.available_probes = probes;
                    // Reset selection if it's now out of bounds
                    if let Some(idx) = self.selected_probe_index {
                        if idx >= self.available_probes.len() {
                            self.selected_probe_index = None;
                        }
                    }
                    tracing::info!("Received {} probes", self.available_probes.len());
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

    /// Render the top toolbar
    fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Probe selection dropdown
            ui.add_space(8.0);

            ui.label("Probe:");
            let probe_text = if let Some(idx) = self.selected_probe_index {
                if idx < self.available_probes.len() {
                    self.available_probes[idx].display_name()
                } else {
                    "Select probe...".to_string()
                }
            } else {
                "Select probe...".to_string()
            };

            egui::ComboBox::from_id_salt("probe_selector")
                .selected_text(probe_text)
                .width(200.0)
                .show_ui(ui, |ui| {
                    for (idx, probe) in self.available_probes.iter().enumerate() {
                        let is_selected = self.selected_probe_index == Some(idx);
                        if ui
                            .selectable_label(is_selected, probe.display_name())
                            .clicked()
                        {
                            self.selected_probe_index = Some(idx);
                            #[cfg(feature = "mock-probe")]
                            {
                                self.use_mock_probe = probe.is_mock();
                            }
                        }
                    }
                });

            // Refresh probes button
            if ui
                .button("🔄")
                .on_hover_text("Refresh probe list")
                .clicked()
            {
                // Send async request to backend thread
                self.frontend.send_command(BackendCommand::RefreshProbes);
            }

            ui.separator();

            // Connection controls
            match self.connection_status {
                ConnectionStatus::Disconnected | ConnectionStatus::Error => {
                    let can_connect = self.selected_probe_index.is_some();
                    if ui
                        .add_enabled(can_connect, egui::Button::new("🔌 Connect"))
                        .clicked()
                    {
                        // Tell backend whether to use mock probe (only with feature)
                        #[cfg(feature = "mock-probe")]
                        self.frontend.use_mock_probe(self.use_mock_probe);

                        let selector = self.selected_probe_index.and_then(|idx| {
                            match self.available_probes.get(idx) {
                                Some(DetectedProbe::Real(info)) => {
                                    Some(format!("{:04x}:{:04x}", info.vendor_id, info.product_id))
                                }
                                #[cfg(feature = "mock-probe")]
                                Some(DetectedProbe::Mock(_)) => None,
                                _ => None,
                            }
                        });
                        self.frontend.connect(
                            selector,
                            self.target_chip_input.clone(),
                            self.config.probe.clone(),
                        );
                    }
                }
                ConnectionStatus::Connecting => {
                    ui.add_enabled(false, egui::Button::new("⏳ Connecting..."));
                }
                ConnectionStatus::Connected => {
                    if ui.button("🔌 Disconnect").clicked() {
                        self.frontend.disconnect();
                    }
                }
            }

            ui.separator();

            // Target chip selector
            ui.label("Target:");
            ui.add(
                egui::TextEdit::singleline(&mut self.target_chip_input)
                    .desired_width(120.0)
                    .hint_text("e.g., STM32F407VGTx"),
            );

            ui.separator();

            // ELF/AXF file selector
            if ui
                .button("📂 Load ELF")
                .on_hover_text("Load symbols from ELF/AXF file")
                .clicked()
            {
                self.open_elf_file_dialog();
            }

            if let Some(ref path) = self.elf_file_path {
                if let Some(name) = path.file_name() {
                    ui.label(format!("📄 {}", name.to_string_lossy()));
                    if ui.button("👁").on_hover_text("View symbols").clicked() {
                        self.show_elf_symbols_dialog = true;
                    }
                }
            }

            ui.separator();

            // Collection controls
            let connected = self.connection_status == ConnectionStatus::Connected;

            if self.settings.collecting {
                if ui
                    .add_enabled(connected, egui::Button::new("⏹ Stop"))
                    .clicked()
                {
                    self.settings.stop_collection();
                    self.frontend.stop_collection();
                }

                if self.settings.paused {
                    if ui
                        .add_enabled(connected, egui::Button::new("▶ Resume"))
                        .clicked()
                    {
                        self.settings.toggle_pause();
                    }
                } else {
                    if ui
                        .add_enabled(connected, egui::Button::new("⏸ Pause"))
                        .clicked()
                    {
                        self.settings.toggle_pause();
                    }
                }
            } else {
                if ui
                    .add_enabled(connected, egui::Button::new("▶ Start"))
                    .clicked()
                {
                    self.settings.start_collection();
                    self.frontend.start_collection();
                }
            }

            if ui.button("🗑 Clear").clicked() {
                self.clear_all_data();
                self.frontend.clear_data();
            }

            ui.separator();

            // View controls
            ui.toggle_value(&mut self.side_panel_open, "📋 Variables");
            ui.toggle_value(&mut self.bottom_panel_open, "📊 Stats");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Settings button
                if ui.button("⚙").clicked() {
                    self.show_settings_dialog = true;
                }

                ui.separator();

                // Status indicator
                let (status_text, status_color) = match self.connection_status {
                    ConnectionStatus::Disconnected => ("Disconnected", Color32::GRAY),
                    ConnectionStatus::Connecting => ("Connecting...", Color32::YELLOW),
                    ConnectionStatus::Connected => ("Connected", Color32::GREEN),
                    ConnectionStatus::Error => ("Error", Color32::RED),
                };

                ui.colored_label(status_color, format!("● {}", status_text));

                if self.settings.collecting && !self.settings.paused {
                    ui.colored_label(Color32::GREEN, "● Recording");
                }
            });
        });
    }

    /// Render the side panel with variable list
    fn render_side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("variables_panel")
            .default_width(self.config.ui.side_panel_width)
            .resizable(true)
            .show_animated(ctx, self.side_panel_open, |ui| {
                ui.heading("Variables");

                ui.horizontal(|ui| {
                    // Show "Select from ELF" button if ELF is loaded
                    if self.elf_info.is_some() {
                        if ui
                            .button("📋 Select")
                            .on_hover_text("Select variable from ELF symbols")
                            .clicked()
                        {
                            self.variable_selector.update_filter(self.elf_info.as_ref());
                            self.show_variable_selector = true;
                        }
                    }

                    if ui
                        .button("➕ Add")
                        .on_hover_text("Add variable manually")
                        .clicked()
                    {
                        self.variable_editor = VariableEditorState::default();
                        self.variable_editor.color = [255, 100, 100, 255];
                        self.show_add_variable_dialog = true;
                    }

                    if ui.button("🔄 Refresh").clicked() {
                        // Request variable list from backend
                        self.frontend.send_command(BackendCommand::RequestStats);
                    }
                });

                ui.separator();

                // Variable list header
                ui.heading("Watched Variables");

                // Show status messages to help diagnose issues
                if self.connection_status != ConnectionStatus::Connected {
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::YELLOW, "⚠");
                        ui.label("Not connected - connect to a probe to read values");
                    });
                } else if !self.settings.collecting {
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::YELLOW, "⚠");
                        ui.label("Collection stopped - click Start to begin reading");
                    });
                } else if self.settings.paused {
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::YELLOW, "⏸");
                        ui.label("Collection paused");
                    });
                }

                if self.variable_data.is_empty() {
                    ui.label("No variables added yet.");
                    ui.label("Use 'Add Variable' or load an ELF file.");
                } else {
                    // Collect display info first to avoid borrow issues
                    let variable_display_info: Vec<_> = self
                        .variable_data
                        .iter()
                        .map(|(&id, data)| {
                            (
                                id,
                                data.variable.name.clone(),
                                data.variable.address,
                                data.variable.var_type,
                                data.variable.unit.clone(),
                                data.variable.enabled,
                                data.variable.color,
                                data.last_value,
                                data.last_converted_value,
                                data.error_count,
                                data.data_points.len(),
                            )
                        })
                        .collect();

                    let mut actions: Vec<(u32, VariableAction)> = Vec::new();

                    // Table header
                    egui::Grid::new("variable_header_grid")
                        .num_columns(4)
                        .striped(false)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            ui.strong("On");
                            ui.strong("Variable");
                            ui.strong("Value");
                            ui.strong("Actions");
                            ui.end_row();
                        });

                    ui.separator();

                    // Variable table
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        egui::Grid::new("variable_table_grid")
                            .num_columns(4)
                            .striped(true)
                            .spacing([8.0, 6.0])
                            .show(ui, |ui| {
                                for (
                                    id,
                                    name,
                                    address,
                                    var_type,
                                    unit,
                                    enabled,
                                    color,
                                    last_value,
                                    last_converted_value,
                                    error_count,
                                    point_count,
                                ) in &variable_display_info
                                {
                                    let color = Color32::from_rgba_unmultiplied(
                                        color[0], color[1], color[2], color[3],
                                    );

                                    // Enable checkbox with color indicator
                                    ui.horizontal(|ui| {
                                        let mut new_enabled = *enabled;
                                        if ui.checkbox(&mut new_enabled, "").changed() {
                                            actions.push((
                                                *id,
                                                VariableAction::SetEnabled(new_enabled),
                                            ));
                                        }
                                        ui.colored_label(color, "●");
                                    });

                                    // Variable name and address
                                    ui.vertical(|ui| {
                                        let is_selected = self.selected_variable_id == Some(*id);
                                        let response = ui.selectable_label(is_selected, name);
                                        if response.clicked() {
                                            self.selected_variable_id =
                                                if is_selected { None } else { Some(*id) };
                                        }
                                        ui.small(format!("0x{:08X}", address));
                                    });

                                    // Current value display
                                    ui.vertical(|ui| {
                                        if let Some(value) = last_converted_value {
                                            // Format value based on magnitude
                                            let formatted = if value.abs() < 0.001 && *value != 0.0
                                            {
                                                format!("{:.2e}", value)
                                            } else if value.abs() >= 10000.0 {
                                                format!("{:.1}", value)
                                            } else {
                                                format!("{:.3}", value)
                                            };
                                            ui.strong(format!("{} {}", formatted, unit));

                                            // Show raw value if different
                                            if let Some(raw) = last_value {
                                                if (*raw - *value).abs() > 0.0001 {
                                                    ui.small(format!("raw: {:.2}", raw));
                                                }
                                            }
                                        } else {
                                            ui.colored_label(Color32::GRAY, "---");
                                        }

                                        // Show point count and error info
                                        let info = if *error_count > 0 {
                                            format!("{} pts, {} err", point_count, error_count)
                                        } else {
                                            format!("{} pts", point_count)
                                        };
                                        ui.small(info);
                                    });

                                    // Action buttons
                                    ui.horizontal(|ui| {
                                        if ui.small_button("✏").on_hover_text("Edit").clicked() {
                                            actions.push((*id, VariableAction::Edit));
                                        }
                                        if ui.small_button("🗑").on_hover_text("Remove").clicked()
                                        {
                                            actions.push((*id, VariableAction::Remove));
                                        }
                                    });

                                    ui.end_row();

                                    // Show expanded details if selected
                                    if self.selected_variable_id == Some(*id) {
                                        ui.label(""); // Empty cell
                                        ui.horizontal(|ui| {
                                            ui.small(format!("Type: {}", var_type));
                                        });
                                        ui.label(""); // Empty cell
                                        ui.label(""); // Empty cell
                                        ui.end_row();
                                    }
                                }
                            });
                    });

                    // Process actions after the loop
                    for (id, action) in actions {
                        match action {
                            VariableAction::SetEnabled(enabled) => {
                                if let Some(data) = self.variable_data.get_mut(&id) {
                                    data.variable.enabled = enabled;
                                    self.frontend.update_variable(data.variable.clone());
                                }
                            }
                            VariableAction::Edit => {
                                self.open_variable_editor(id);
                            }
                            VariableAction::Remove => {
                                self.variable_data.remove(&id);
                                self.frontend.remove_variable(id);
                            }
                        }
                    }
                }
            });
    }

    /// Render the bottom panel with statistics
    fn render_bottom_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("stats_panel")
            .default_height(self.config.ui.bottom_panel_height)
            .resizable(true)
            .show_animated(ctx, self.bottom_panel_open, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Statistics");

                    ui.separator();

                    // Display stats
                    ui.label(format!(
                        "Samples: {} | Errors: {} | Success Rate: {:.1}%",
                        self.stats.successful_reads,
                        self.stats.failed_reads,
                        self.stats.success_rate()
                    ));

                    ui.separator();

                    // Rate display with throttle indicator
                    let target_rate = self.config.collection.poll_rate_hz as f64;
                    let actual_rate = self.stats.effective_sample_rate;
                    let is_throttled = actual_rate > 0.0 && actual_rate < target_rate * 0.9;
                    let rate_color = if is_throttled {
                        Color32::from_rgb(255, 100, 100) // Red when throttled
                    } else if actual_rate > 0.0 {
                        Color32::from_rgb(100, 255, 100) // Green when at target
                    } else {
                        Color32::GRAY
                    };

                    ui.label(format!(
                        "Avg Read Time: {:.1} μs |",
                        self.stats.avg_read_time_us
                    ));
                    ui.colored_label(rate_color, format!("{:.1} Hz", actual_rate));
                    if is_throttled {
                        ui.colored_label(
                            Color32::from_rgb(255, 200, 100),
                            format!("(target: {})", self.config.collection.poll_rate_hz),
                        );
                    }

                    ui.separator();

                    ui.label(format!(
                        "Data: {:.2} KB",
                        self.stats.total_bytes_read as f64 / 1024.0
                    ));
                });

                // Error display
                if let Some(ref error) = self.last_error {
                    ui.colored_label(Color32::RED, format!("Error: {}", error));
                }

                // Time window control
                ui.horizontal(|ui| {
                    ui.label("Time Window:");
                    ui.add(
                        egui::Slider::new(&mut self.settings.display_time_window, 1.0..=60.0)
                            .suffix(" s")
                            .logarithmic(true),
                    );

                    ui.checkbox(&mut self.settings.follow_latest, "Follow Latest");

                    if !self.settings.is_manual_y_scale() {
                        if ui.button("Manual Y Scale").clicked() {
                            self.settings.set_y_range(-10.0, 10.0);
                        }
                    } else {
                        if ui.button("Auto Y Scale").clicked() {
                            self.settings.clear_y_range();
                        }
                    }
                });
            });
    }

    /// Render the main plot area
    fn render_plot(&mut self, ui: &mut egui::Ui) {
        use egui_plot::{Line, Plot, PlotBounds, PlotPoints, VLine};

        // Calculate the current time based on the latest data
        let time_window = self.settings.display_time_window;
        let max_time_window = self.settings.max_time_window;
        let mut max_time = 0.0f64;

        // Find the latest timestamp across all variables
        for data in self.variable_data.values() {
            if let Some((_, end)) = data.time_range() {
                max_time = max_time.max(end);
            }
        }

        // Determine zoom/drag permissions based on lock and autoscale settings
        let allow_x_zoom = !self.settings.lock_x;
        let allow_y_zoom = !self.settings.lock_y;
        let allow_x_drag = !self.settings.lock_x && !self.settings.autoscale_x;
        let allow_y_drag = !self.settings.lock_y && !self.settings.autoscale_y;

        let mut plot = Plot::new("main_plot")
            .legend(egui_plot::Legend::default())
            .show_axes(true)
            .show_grid(self.config.ui.show_grid)
            .allow_zoom(egui::Vec2b::new(allow_x_zoom, allow_y_zoom))
            .allow_drag(egui::Vec2b::new(allow_x_drag, allow_y_drag))
            .allow_scroll(egui::Vec2b::new(
                !self.settings.lock_x,
                !self.settings.lock_y,
            ))
            .allow_boxed_zoom(allow_x_zoom || allow_y_zoom)
            .x_axis_label("Time (s)")
            .y_axis_label("Value");

        // Set auto bounds behavior based on autoscale settings
        plot = plot.auto_bounds(egui::Vec2b::new(false, self.settings.autoscale_y));

        // Calculate bounds
        let autoscale_x = self.settings.autoscale_x;
        let autoscale_y = self.settings.autoscale_y;
        let x_bounds_manual = if self.settings.is_manual_x_scale() {
            Some((self.settings.x_min.unwrap(), self.settings.x_max.unwrap()))
        } else {
            None
        };
        let y_bounds_manual = if self.settings.is_manual_y_scale() {
            Some((self.settings.y_min.unwrap(), self.settings.y_max.unwrap()))
        } else {
            None
        };

        let response = plot.show(ui, |plot_ui| {
            // Calculate and set bounds based on autoscale settings
            let (x_min, x_max) = if autoscale_x {
                // Auto-scale X: follow latest data with time window
                let x_max = max_time.max(time_window);
                let x_min = (x_max - time_window).max(0.0);
                (x_min, x_max)
            } else if let Some((xmin, xmax)) = x_bounds_manual {
                (xmin, xmax)
            } else {
                // Default: show last time_window seconds
                let x_max = max_time.max(time_window);
                let x_min = (x_max - time_window).max(0.0);
                (x_min, x_max)
            };

            // Calculate Y bounds from visible data if autoscaling
            let (y_min, y_max) = if autoscale_y {
                let mut y_min = f64::MAX;
                let mut y_max = f64::MIN;

                for data in self.variable_data.values() {
                    if !data.variable.enabled || !data.variable.show_in_graph {
                        continue;
                    }
                    for dp in &data.data_points {
                        let t = dp.timestamp.as_secs_f64();
                        if t >= x_min && t <= x_max {
                            y_min = y_min.min(dp.converted_value);
                            y_max = y_max.max(dp.converted_value);
                        }
                    }
                }

                // Add some padding to Y bounds
                if y_min < f64::MAX && y_max > f64::MIN {
                    let y_range = y_max - y_min;
                    let padding = if y_range > 0.0 { y_range * 0.1 } else { 1.0 };
                    (y_min - padding, y_max + padding)
                } else {
                    (-1.0, 1.0)
                }
            } else if let Some((ymin, ymax)) = y_bounds_manual {
                (ymin, ymax)
            } else {
                (-1.0, 1.0)
            };

            plot_ui.set_plot_bounds(PlotBounds::from_min_max([x_min, y_min], [x_max, y_max]));

            // Draw data lines
            for (_id, data) in &self.variable_data {
                if !data.variable.enabled || !data.variable.show_in_graph {
                    continue;
                }

                let points: PlotPoints = data.as_plot_points().into();

                let color = Color32::from_rgba_unmultiplied(
                    data.variable.color[0],
                    data.variable.color[1],
                    data.variable.color[2],
                    data.variable.color[3],
                );

                let line = Line::new(points)
                    .name(&data.variable.name)
                    .color(color)
                    .width(self.config.ui.line_width);

                plot_ui.line(line);
            }

            // Render current time indicator if autoscaling X
            if autoscale_x && max_time > 0.0 {
                let vline = VLine::new(max_time)
                    .color(Color32::from_rgba_unmultiplied(255, 255, 255, 64))
                    .width(1.0);
                plot_ui.vline(vline);
            }
        });

        // Handle plot interactions - update settings from user zoom/pan
        if response.response.dragged() && !self.settings.lock_x {
            // User is dragging - disable autoscale X
            if self.settings.autoscale_x {
                self.settings.autoscale_x = false;
                self.settings.follow_latest = false;
            }
        }

        // Check for zoom changes and update time window
        if response.response.hovered() {
            let scroll_delta = ui.input(|i| i.raw_scroll_delta);
            if scroll_delta.y.abs() > 0.0 && !self.settings.lock_x {
                // User is zooming - disable autoscale X
                if self.settings.autoscale_x {
                    self.settings.autoscale_x = false;
                    self.settings.follow_latest = false;
                }
            }
        }

        // After any interaction, capture the new bounds and update time window
        if !self.settings.autoscale_x && !self.settings.lock_x {
            let bounds = response.transform.bounds();
            let new_x_min = bounds.min()[0];
            let new_x_max = bounds.max()[0];
            let x_range = new_x_max - new_x_min;

            if x_range > 0.0 {
                // Clamp time window to max
                let clamped_range = x_range.clamp(0.1, max_time_window);

                // Update settings with new bounds
                self.settings.display_time_window = clamped_range;
                self.settings.x_min = Some(new_x_min);
                self.settings.x_max = Some(new_x_max);
            }
        }

        // Similarly for Y axis
        if !self.settings.autoscale_y && !self.settings.lock_y {
            let bounds = response.transform.bounds();
            self.settings.y_min = Some(bounds.min()[1]);
            self.settings.y_max = Some(bounds.max()[1]);
        }
    }

    /// Render the add variable dialog
    fn render_add_variable_dialog(&mut self, ctx: &egui::Context) {
        let mut should_close = false;
        let mut should_add = false;

        if self.show_add_variable_dialog {
            let title = if self.variable_editor.editing_id.is_some() {
                "Edit Variable"
            } else {
                "Add Variable"
            };

            egui::Window::new(title)
                .resizable(true)
                .collapsible(false)
                .default_width(500.0)
                .show(ctx, |ui| {
                    // Show DWARF type info if available
                    if let Some(ref type_name) = self.variable_editor.type_name {
                        ui.horizontal(|ui| {
                            ui.label("DWARF Type:");
                            ui.colored_label(Color32::LIGHT_BLUE, type_name);
                        });
                        if let Some(size) = self.variable_editor.size {
                            ui.horizontal(|ui| {
                                ui.label("Size:");
                                ui.label(format!("{} bytes", size));
                            });
                        }
                        ui.separator();
                    }

                    egui::Grid::new("add_var_grid")
                        .num_columns(2)
                        .spacing([10.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Name:");
                            ui.text_edit_singleline(&mut self.variable_editor.name);
                            ui.end_row();

                            ui.label("Address:");
                            ui.text_edit_singleline(&mut self.variable_editor.address);
                            ui.end_row();

                            ui.label("Type:");
                            egui::ComboBox::from_id_salt("var_type")
                                .selected_text(format!("{}", self.variable_editor.var_type))
                                .show_ui(ui, |ui| {
                                    use crate::types::VariableType::*;
                                    ui.selectable_value(
                                        &mut self.variable_editor.var_type,
                                        U8,
                                        "u8",
                                    );
                                    ui.selectable_value(
                                        &mut self.variable_editor.var_type,
                                        U16,
                                        "u16",
                                    );
                                    ui.selectable_value(
                                        &mut self.variable_editor.var_type,
                                        U32,
                                        "u32",
                                    );
                                    ui.selectable_value(
                                        &mut self.variable_editor.var_type,
                                        I8,
                                        "i8",
                                    );
                                    ui.selectable_value(
                                        &mut self.variable_editor.var_type,
                                        I16,
                                        "i16",
                                    );
                                    ui.selectable_value(
                                        &mut self.variable_editor.var_type,
                                        I32,
                                        "i32",
                                    );
                                    ui.selectable_value(
                                        &mut self.variable_editor.var_type,
                                        F32,
                                        "f32",
                                    );
                                    ui.selectable_value(
                                        &mut self.variable_editor.var_type,
                                        F64,
                                        "f64",
                                    );
                                });
                            ui.end_row();

                            ui.label("Unit:");
                            ui.text_edit_singleline(&mut self.variable_editor.unit);
                            ui.end_row();

                            ui.label("Color:");
                            let mut color = egui::Color32::from_rgba_unmultiplied(
                                self.variable_editor.color[0],
                                self.variable_editor.color[1],
                                self.variable_editor.color[2],
                                self.variable_editor.color[3],
                            );
                            if ui.color_edit_button_srgba(&mut color).changed() {
                                self.variable_editor.color = color.to_array();
                            }
                            ui.end_row();
                        });

                    ui.separator();

                    // Converter script editor with autocomplete
                    ui.label("Converter Script:");
                    ui.add_space(4.0);

                    ScriptEditor::new(
                        &mut self.variable_editor.converter_script,
                        &mut self.variable_editor.script_editor_state,
                        "variable_converter_editor",
                    )
                    .show(ui);

                    ui.separator();

                    ui.horizontal(|ui| {
                        let button_text = if self.variable_editor.editing_id.is_some() {
                            "Save"
                        } else {
                            "Add"
                        };

                        if ui.button(button_text).clicked() {
                            should_add = true;
                            should_close = true;
                        }

                        if ui.button("Cancel").clicked() {
                            should_close = true;
                        }
                    });
                });
        }

        // Process actions after the window is rendered
        if should_add {
            self.add_variable_from_editor();
        }
        if should_close {
            self.show_add_variable_dialog = false;
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
            self.pending_variable = Some(var);
            self.show_duplicate_confirm = true;
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

    /// Add a variable from the editor state
    fn add_variable_from_editor(&mut self) {
        let address =
            u64::from_str_radix(self.variable_editor.address.trim_start_matches("0x"), 16)
                .unwrap_or(0);

        let mut var = crate::types::Variable::new(
            self.variable_editor.name.clone(),
            address,
            self.variable_editor.var_type,
        )
        .with_color(self.variable_editor.color)
        .with_unit(self.variable_editor.unit.clone());

        if !self.variable_editor.converter_script.is_empty() {
            var = var.with_converter(self.variable_editor.converter_script.clone());
        }

        self.add_variable(var);
    }

    /// Open the variable editor for an existing variable
    fn open_variable_editor(&mut self, id: u32) {
        if let Some(data) = self.variable_data.get(&id) {
            let var = &data.variable;
            self.variable_editor = VariableEditorState {
                name: var.name.clone(),
                address: format!("0x{:08X}", var.address),
                var_type: var.var_type,
                unit: var.unit.clone(),
                converter_script: var.converter_script.clone().unwrap_or_default(),
                color: var.color,
                editing_id: Some(id),
                type_name: None,
                size: None,
                script_editor_state: ScriptEditorState::default(),
            };
            self.show_add_variable_dialog = true;
        }
    }

    /// Clear all collected data
    fn clear_all_data(&mut self) {
        for data in self.variable_data.values_mut() {
            data.clear();
        }
        self.stats = CollectionStats::default();
    }

    /// Open the ELF/AXF file dialog
    fn open_elf_file_dialog(&mut self) {
        // Use rfd for native file dialog
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("ELF/AXF Files", &["elf", "axf", "out"])
            .add_filter("All Files", &["*"])
            .set_title("Select ELF/AXF Binary")
            .pick_file()
        {
            tracing::info!("Selected ELF file: {:?}", path);
            self.elf_file_path = Some(path.clone());

            // Parse ELF with full info (symbols, types, etc.)
            match parse_elf(&path) {
                Ok(info) => {
                    tracing::info!(
                        "Parsed ELF: {} variables, {} functions",
                        info.variable_count(),
                        info.function_count()
                    );
                    // Get variables for the symbol list
                    self.elf_symbols = info.get_variables().into_iter().cloned().collect();
                    let var_count = self.elf_symbols.len();
                    let type_count = info.type_table().len();

                    self.elf_info = Some(info);

                    // Update the variable selector filter
                    self.variable_selector.update_filter(self.elf_info.as_ref());

                    // Show a success message with stats instead of opening a popup
                    tracing::info!(
                        "ELF loaded: {} variables, {} types with DWARF info",
                        var_count,
                        type_count
                    );
                    // Don't automatically open the symbols dialog - user can use variable selector
                }
                Err(e) => {
                    self.last_error = Some(format!("Failed to parse ELF: {}", e));
                    self.elf_info = None;
                    self.elf_symbols.clear();
                }
            }
        }
    }

    /// Open variable selector with a symbol pre-filled
    fn open_variable_editor_from_symbol(&mut self, symbol: &ElfSymbol) {
        self.variable_editor = VariableEditorState::from_symbol(symbol, self.elf_info.as_ref());
        self.show_add_variable_dialog = true;
        self.show_variable_selector = false;
        self.variable_selector.clear();
    }

    /// Render the variable selector with autocomplete
    fn render_variable_selector(&mut self, ctx: &egui::Context) {
        let mut should_close = false;
        let mut symbol_to_use: Option<ElfSymbol> = None;
        let mut toggle_expand_path: Option<String> = None;
        let mut variables_to_add: Vec<crate::types::Variable> = Vec::new();

        if self.show_variable_selector {
            egui::Window::new("Select Variable")
                .default_size([600.0, 500.0])
                .resizable(true)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Search:");
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.variable_selector.query)
                                .desired_width(400.0)
                                .hint_text("Type to search variables..."),
                        );

                        // Update filter when query changes
                        if response.changed() {
                            self.variable_selector.update_filter(self.elf_info.as_ref());
                            self.variable_selector.dropdown_open = true;
                        }

                        // Handle keyboard navigation
                        if response.has_focus() {
                            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                                self.variable_selector.select_previous();
                            }
                            if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                                self.variable_selector.select_next();
                            }
                            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                if let Some(sym) = self.variable_selector.selected_symbol() {
                                    symbol_to_use = Some(sym.clone());
                                }
                            }
                            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                should_close = true;
                            }
                        }

                        if ui.button("❌").clicked() {
                            self.variable_selector.query.clear();
                            self.variable_selector.update_filter(self.elf_info.as_ref());
                        }
                    });

                    ui.separator();

                    // Show filtered results count
                    let num_results = self.variable_selector.filtered_symbols.len();
                    ui.label(format!("{} variables found", num_results));

                    ui.separator();

                    egui::ScrollArea::vertical()
                        .max_height(380.0)
                        .show(ui, |ui| {
                            for idx in 0..self.variable_selector.filtered_symbols.len() {
                                let symbol = &self.variable_selector.filtered_symbols[idx];
                                let root_path = idx.to_string();

                                // Render this symbol and its nested members recursively
                                Self::render_type_tree(
                                    ui,
                                    &symbol.display_name,
                                    symbol.address,
                                    self.elf_info
                                        .as_ref()
                                        .and_then(|info| info.symbol_type_handle(symbol)),
                                    &root_path,
                                    0,
                                    &self.variable_selector.expanded_paths,
                                    self.variable_selector.selected_index == Some(idx),
                                    &mut toggle_expand_path,
                                    &mut variables_to_add,
                                    &mut symbol_to_use,
                                    Some(symbol),
                                );
                            }
                        });

                    ui.separator();

                    ui.horizontal(|ui| {
                        let has_selection = self.variable_selector.selected_index.is_some();
                        if ui
                            .add_enabled(has_selection, egui::Button::new("Select"))
                            .clicked()
                        {
                            if let Some(sym) = self.variable_selector.selected_symbol() {
                                symbol_to_use = Some(sym.clone());
                            }
                        }

                        if ui.button("Cancel").clicked() {
                            should_close = true;
                        }
                    });
                });
        }

        // Handle toggle expand after the UI loop
        if let Some(path) = toggle_expand_path {
            self.variable_selector.toggle_expanded(&path);
        }

        if should_close {
            self.show_variable_selector = false;
            self.variable_selector.clear();
        }

        if let Some(symbol) = symbol_to_use {
            self.open_variable_editor_from_symbol(&symbol);
        }

        // Handle adding variables
        for var in variables_to_add {
            self.add_variable(var);
        }
    }

    /// Recursively render a type tree with expansion support
    fn render_type_tree(
        ui: &mut egui::Ui,
        name: &str,
        address: u64,
        type_handle: Option<TypeHandle>,
        path: &str,
        indent_level: usize,
        expanded_paths: &std::collections::HashSet<String>,
        is_selected: bool,
        toggle_expand_path: &mut Option<String>,
        variables_to_add: &mut Vec<crate::types::Variable>,
        symbol_to_use: &mut Option<ElfSymbol>,
        root_symbol: Option<&ElfSymbol>,
    ) {
        let is_expanded = expanded_paths.contains(path);
        let type_name = type_handle
            .as_ref()
            .map(|h| h.type_name())
            .unwrap_or_else(|| "unknown".to_string());
        let size = type_handle.as_ref().and_then(|h| h.size()).unwrap_or(0);

        // Check if this type is expandable (struct/union with members, or pointer/ref to such)
        let can_expand = type_handle
            .as_ref()
            .map(|h| h.is_expandable())
            .unwrap_or(false);

        // Check if this type can be added as a variable (primitives, enums, pointers, typedefs to these)
        let is_addable = type_handle.as_ref().map(|h| h.is_addable()).unwrap_or(true);

        // Get the underlying type handle for member access
        let underlying = type_handle.as_ref().map(|h| h.underlying());

        // Get members if this is a struct/union (or pointer/reference to one)
        let members = underlying.as_ref().and_then(|h| {
            // First check if this is directly a struct/union
            if let Some(members) = h.members() {
                return Some((members.to_vec(), h.clone()));
            }
            // Check if it's a pointer/reference to a struct
            if h.is_pointer_or_reference() {
                if let Some(pointee) = h.pointee() {
                    let pointee_underlying = pointee.underlying();
                    if let Some(members) = pointee_underlying.members() {
                        return Some((members.to_vec(), pointee_underlying));
                    }
                }
            }
            None
        });

        // Check if this is an array type
        let array_info = underlying.as_ref().and_then(|h| {
            if h.is_array() {
                let count = h.array_count().unwrap_or(0);
                let elem_size = h.element_size().unwrap_or(0);
                let elem_type = h.element_type();
                if count > 0 && elem_size > 0 {
                    Some((count.min(MAX_ARRAY_ELEMENTS), elem_size, elem_type))
                } else {
                    None
                }
            } else {
                None
            }
        });

        ui.horizontal(|ui| {
            // Indentation
            ui.add_space((indent_level * 20) as f32);

            // Expand/collapse button
            if can_expand {
                let expand_icon = if is_expanded { "▼" } else { "▶" };
                if ui
                    .small_button(expand_icon)
                    .on_hover_text(if is_expanded {
                        "Collapse"
                    } else {
                        "Expand members"
                    })
                    .clicked()
                {
                    *toggle_expand_path = Some(path.to_string());
                }
            } else {
                ui.add_space(18.0);
            }

            // Format the display based on indent level
            let display_text = if indent_level == 0 {
                format!(
                    "{} @ 0x{:08X} ({} bytes) - {}",
                    name, address, size, type_name
                )
            } else {
                // For nested members, show just the member name without the full path
                let short_name = name.rsplit('.').next().unwrap_or(name);
                format!(".{}: {} @ 0x{:08X}", short_name, type_name, address)
            };

            let response = ui.selectable_label(is_selected && indent_level == 0, &display_text);

            if response.double_clicked() {
                if let Some(sym) = root_symbol {
                    *symbol_to_use = Some(sym.clone());
                }
            }

            // Add button for all addable types (including top-level)
            if is_addable {
                if ui
                    .small_button("➕")
                    .on_hover_text("Add as variable")
                    .clicked()
                {
                    let var_type = type_handle
                        .as_ref()
                        .map(|h| h.to_variable_type())
                        .unwrap_or(crate::types::VariableType::U32);
                    let var = crate::types::Variable::new(name, address, var_type);
                    variables_to_add.push(var);
                }
            }
        });

        // Render expanded members or array elements
        if is_expanded {
            // First check if this is an array
            if let Some((count, elem_size, elem_type)) = array_info.clone() {
                let display_count = count.min(MAX_ARRAY_ELEMENTS);
                let truncated = count > MAX_ARRAY_ELEMENTS;

                // Show array info
                ui.horizontal(|ui| {
                    ui.add_space(((indent_level + 1) * 20) as f32);
                    if truncated {
                        ui.colored_label(
                            Color32::YELLOW,
                            format!(
                                "Showing {} of {} elements (max {})",
                                display_count, count, MAX_ARRAY_ELEMENTS
                            ),
                        );
                    } else {
                        ui.colored_label(Color32::GRAY, format!("{} elements", count));
                    }
                });

                // Add all elements button
                ui.horizontal(|ui| {
                    ui.add_space(((indent_level + 1) * 20) as f32);
                    if ui
                        .small_button(format!("➕ Add all {} elements", display_count))
                        .clicked()
                    {
                        for i in 0..display_count {
                            let elem_addr = address + (i * elem_size);
                            let elem_name = format!("{}[{}]", name, i);
                            let var_type = elem_type
                                .as_ref()
                                .map(|h| h.to_variable_type())
                                .unwrap_or(crate::types::VariableType::U32);
                            let var = crate::types::Variable::new(&elem_name, elem_addr, var_type);
                            variables_to_add.push(var);
                        }
                    }
                });

                // Render each array element
                for i in 0..display_count {
                    let elem_addr = address + (i * elem_size);
                    let elem_name = format!("{}[{}]", name, i);
                    let elem_path = format!("{}[{}]", path, i);

                    Self::render_type_tree(
                        ui,
                        &elem_name,
                        elem_addr,
                        elem_type.clone(),
                        &elem_path,
                        indent_level + 1,
                        expanded_paths,
                        false,
                        toggle_expand_path,
                        variables_to_add,
                        symbol_to_use,
                        root_symbol,
                    );
                }
            } else if let Some((member_list, parent_handle)) = members {
                // Handle struct/union members
                if member_list.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(((indent_level + 1) * 20) as f32);
                        ui.colored_label(Color32::GRAY, "(No member info available)");
                    });
                } else {
                    // Add all members button
                    ui.horizontal(|ui| {
                        ui.add_space(((indent_level + 1) * 20) as f32);
                        if ui.small_button("➕ Add all members").clicked() {
                            Self::collect_all_members(
                                name,
                                address,
                                &member_list,
                                &parent_handle,
                                variables_to_add,
                            );
                        }
                    });

                    // Render each member recursively
                    for member in &member_list {
                        let member_path = format!("{}.{}", path, member.name);
                        let member_addr = address + member.offset;
                        let full_name = format!("{}.{}", name, member.name);
                        let member_type_handle = parent_handle.member_type(member);

                        Self::render_type_tree(
                            ui,
                            &full_name,
                            member_addr,
                            Some(member_type_handle),
                            &member_path,
                            indent_level + 1,
                            expanded_paths,
                            false,
                            toggle_expand_path,
                            variables_to_add,
                            symbol_to_use,
                            root_symbol,
                        );
                    }
                }
            } else {
                // Type is expandable but we couldn't get struct/array info - show message
                ui.horizontal(|ui| {
                    ui.add_space(((indent_level + 1) * 20) as f32);
                    ui.colored_label(Color32::GRAY, "(Type info not fully resolved)");
                });
            }
        }
    }

    /// Collect all members of a struct recursively as variables
    fn collect_all_members(
        base_name: &str,
        base_address: u64,
        members: &[crate::backend::MemberDef],
        parent_handle: &TypeHandle,
        variables_to_add: &mut Vec<crate::types::Variable>,
    ) {
        for member in members {
            let member_addr = base_address + member.offset;
            let full_name = format!("{}.{}", base_name, member.name);
            let member_type = parent_handle.member_type(member);
            let var_type = member_type.to_variable_type();
            let var = crate::types::Variable::new(&full_name, member_addr, var_type);
            variables_to_add.push(var);
        }
    }

    /// Render the ELF symbols dialog
    fn render_elf_symbols_dialog(&mut self, ctx: &egui::Context) {
        let mut should_close = false;
        let mut symbol_to_add: Option<crate::backend::ElfSymbol> = None;

        if self.show_elf_symbols_dialog {
            egui::Window::new("ELF Symbols")
                .default_size([400.0, 500.0])
                .resizable(true)
                .collapsible(false)
                .show(ctx, |ui| {
                    // File path display
                    if let Some(ref path) = self.elf_file_path {
                        ui.label(format!("File: {}", path.display()));
                    }

                    ui.separator();

                    // Search filter
                    ui.horizontal(|ui| {
                        ui.label("Filter:");
                        ui.text_edit_singleline(&mut self.elf_symbol_filter);
                        if ui.button("Clear").clicked() {
                            self.elf_symbol_filter.clear();
                        }
                    });

                    ui.separator();

                    // Show summary
                    ui.label(format!("{} variables found", self.elf_symbols.len()));

                    if self.elf_symbols.is_empty() {
                        ui.colored_label(Color32::YELLOW, "No variables found in ELF file.");
                        ui.label("You can still add variables manually.");
                    } else {
                        // Symbol list
                        egui::ScrollArea::vertical()
                            .max_height(350.0)
                            .show(ui, |ui| {
                                let filter_lower = self.elf_symbol_filter.to_lowercase();
                                for symbol in &self.elf_symbols {
                                    // Match against display name, demangled name, or mangled name
                                    if !filter_lower.is_empty() && !symbol.matches(&filter_lower) {
                                        continue;
                                    }

                                    ui.horizontal(|ui| {
                                        // Display name (demangled short form)
                                        ui.label(&symbol.display_name);
                                        ui.label(format!("0x{:08X}", symbol.address));
                                        ui.label(format!("{} bytes", symbol.size));

                                        // Show type info if available
                                        let type_str = self
                                            .elf_info
                                            .as_ref()
                                            .map(|info| info.get_symbol_type_name(symbol))
                                            .unwrap_or_else(|| {
                                                symbol.infer_variable_type().to_string()
                                            });
                                        ui.label(type_str);

                                        if ui.button("Add").clicked() {
                                            symbol_to_add = Some(symbol.clone());
                                        }
                                    })
                                    .response
                                    .on_hover_ui(|ui| {
                                        // Tooltip with full info
                                        ui.label(format!("Mangled: {}", symbol.mangled_name));
                                        ui.label(format!("Demangled: {}", symbol.demangled_name));
                                        ui.label(format!("Section: {}", symbol.section));
                                    });
                                }
                            });
                    }

                    ui.separator();

                    ui.horizontal(|ui| {
                        if ui.button("Close").clicked() {
                            should_close = true;
                        }
                    });
                });
        }

        // Process actions
        if let Some(symbol) = symbol_to_add {
            // Open variable editor with symbol info pre-filled
            self.open_variable_editor_from_symbol(&symbol);
            should_close = true;
        }
        if should_close {
            self.show_elf_symbols_dialog = false;
        }
    }

    /// Render the Variables page - ELF selection, variable browser, and variable table
    fn render_variables_page(&mut self, ctx: &egui::Context) {
        // Left panel: ELF file selection and variable browser
        egui::SidePanel::left("variable_browser")
            .default_width(350.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Variable Browser");
                ui.separator();

                // ELF file selection
                ui.horizontal(|ui| {
                    ui.label("ELF File:");
                    if let Some(path) = &self.elf_file_path {
                        let filename = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Unknown".to_string());
                        ui.label(filename).on_hover_text(path.display().to_string());
                    } else {
                        ui.label("(none)");
                    }
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("ELF/AXF", &["elf", "axf", "out"])
                            .pick_file()
                        {
                            self.elf_file_path = Some(path.clone());
                            // Parse the ELF file
                            match parse_elf(&path) {
                                Ok(info) => {
                                    tracing::info!(
                                        "Parsed ELF: {} variables, {} functions",
                                        info.variable_count(),
                                        info.function_count()
                                    );
                                    self.elf_symbols =
                                        info.get_variables().into_iter().cloned().collect();
                                    self.elf_info = Some(info);
                                    self.variable_selector.update_filter(self.elf_info.as_ref());
                                }
                                Err(e) => {
                                    self.last_error = Some(format!("Failed to parse ELF: {}", e));
                                    self.elf_info = None;
                                    self.elf_symbols.clear();
                                }
                            }
                        }
                    }
                });

                if self.elf_info.is_some() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} variables available", self.elf_symbols.len()));
                    });
                }

                ui.separator();

                // Search filter
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    let response = ui.text_edit_singleline(&mut self.variable_selector.query);
                    if response.changed() {
                        self.variable_selector.update_filter(self.elf_info.as_ref());
                    }
                    if ui.button("Clear").clicked() {
                        self.variable_selector.query.clear();
                        self.variable_selector.update_filter(self.elf_info.as_ref());
                    }
                });

                ui.separator();

                // Variable tree browser
                let mut variables_to_add: Vec<crate::types::Variable> = Vec::new();
                let mut toggle_expand_path: Option<String> = None;

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.variable_selector.filtered_symbols.is_empty() {
                            if self.elf_info.is_some() {
                                ui.colored_label(Color32::GRAY, "No matching variables");
                            } else {
                                ui.colored_label(
                                    Color32::GRAY,
                                    "Load an ELF file to browse variables",
                                );
                            }
                        } else {
                            for idx in 0..self.variable_selector.filtered_symbols.len() {
                                let symbol = &self.variable_selector.filtered_symbols[idx];
                                let root_path = idx.to_string();

                                Self::render_type_tree(
                                    ui,
                                    &symbol.display_name,
                                    symbol.address,
                                    self.elf_info
                                        .as_ref()
                                        .and_then(|info| info.symbol_type_handle(symbol)),
                                    &root_path,
                                    0,
                                    &self.variable_selector.expanded_paths,
                                    self.variable_selector.selected_index == Some(idx),
                                    &mut toggle_expand_path,
                                    &mut variables_to_add,
                                    &mut None,
                                    None,
                                );
                            }
                        }
                    });

                // Handle toggle expand
                if let Some(path) = toggle_expand_path {
                    self.variable_selector.toggle_expanded(&path);
                }

                // Handle adding variables
                for var in variables_to_add {
                    self.add_variable(var);
                }
            });

        // Track actions to perform after the loop (to avoid borrow issues)
        let mut var_to_remove: Option<u32> = None;
        let mut var_to_edit_converter: Option<(u32, String)> = None;
        let mut var_to_open_detail: Option<u32> = None;
        let mut var_toggle_enabled: Option<(u32, bool)> = None;
        let mut var_toggle_graph: Option<(u32, bool)> = None;

        // Main panel: Selected variables table
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Selected Variables");
            ui.separator();

            if self.config.variables.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(50.0);
                    ui.label("No variables selected yet.");
                    ui.label("Use the browser on the left to add variables.");
                });
            } else {
                // Variable table with controls
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("variables_table")
                        .num_columns(9)
                        .striped(true)
                        .min_col_width(50.0)
                        .show(ui, |ui| {
                            // Header row
                            ui.strong("");  // Color swatch
                            ui.strong("Name");
                            ui.strong("Address");
                            ui.strong("Type");
                            ui.strong("Sample");
                            ui.strong("Graph");
                            ui.strong("Converter");
                            ui.strong("Value");
                            ui.strong("");
                            ui.end_row();

                            // Data rows
                            for var in &self.config.variables {
                                let var_color = Color32::from_rgba_unmultiplied(
                                    var.color[0],
                                    var.color[1],
                                    var.color[2],
                                    var.color[3],
                                );

                                // Color swatch (small colored rectangle)
                                let (rect, _response) = ui.allocate_exact_size(
                                    egui::vec2(16.0, 16.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(rect, 2.0, var_color);

                                // Variable name (double-click to open detail dialog)
                                let name_response = ui.add(
                                    egui::Label::new(&var.name)
                                        .sense(egui::Sense::click()),
                                );
                                if name_response.double_clicked() {
                                    var_to_open_detail = Some(var.id);
                                }
                                name_response.on_hover_text("Double-click to edit variable");

                                // Address (double-click to open detail dialog)
                                let addr_response = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("0x{:08X}", var.address))
                                            .monospace(),
                                    )
                                    .sense(egui::Sense::click()),
                                );
                                if addr_response.double_clicked() {
                                    var_to_open_detail = Some(var.id);
                                }
                                addr_response.on_hover_text("Double-click to edit variable");

                                // Type
                                ui.label(var.var_type.to_string());

                                // Sample checkbox (enables/disables data collection)
                                let mut enabled = var.enabled;
                                if ui.checkbox(&mut enabled, "").on_hover_text("Enable sampling").changed() {
                                    var_toggle_enabled = Some((var.id, enabled));
                                }

                                // Graph checkbox (shows/hides in plot)
                                let mut show_graph = var.show_in_graph;
                                if ui.checkbox(&mut show_graph, "").on_hover_text("Show in graph").changed() {
                                    var_toggle_graph = Some((var.id, show_graph));
                                }

                                // Converter script (clickable to edit)
                                let converter = var.converter_script.as_deref().unwrap_or("");
                                let converter_text = if converter.is_empty() {
                                    "✏ Add..."
                                } else if converter.len() > 20 {
                                    "✏ Edit..."
                                } else {
                                    converter
                                };
                                let converter_response = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(converter_text)
                                            .color(if converter.is_empty() {
                                                egui::Color32::GRAY
                                            } else {
                                                egui::Color32::LIGHT_BLUE
                                            })
                                            .underline(),
                                    )
                                    .sense(egui::Sense::click()),
                                );
                                if converter_response.clicked() {
                                    var_to_edit_converter = Some((var.id, converter.to_string()));
                                }
                                if !converter.is_empty() {
                                    converter_response.on_hover_text(converter);
                                }

                                // Current value with color indicator
                                let is_writable = var.is_writable();
                                let is_connected =
                                    self.connection_status == ConnectionStatus::Connected;
                                let can_edit = is_writable && is_connected;

                                if let Some(data) = self.variable_data.get(&var.id) {
                                    if let Some(point) = data.last() {
                                        let value_text = if var.unit.is_empty() {
                                            format!("{:.3}", point.converted_value)
                                        } else {
                                            format!("{:.3} {}", point.converted_value, var.unit)
                                        };

                                        // Show value with variable's color
                                        ui.horizontal(|ui| {
                                            ui.colored_label(var_color, "●");
                                            let response = ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&value_text)
                                                        .monospace()
                                                        .color(if can_edit {
                                                            ui.visuals().text_color()
                                                        } else {
                                                            egui::Color32::GRAY
                                                        }),
                                                )
                                                .sense(egui::Sense::click()),
                                            );

                                            // Capture double_clicked before consuming response
                                            let was_double_clicked = response.double_clicked();

                                            if can_edit {
                                                response.on_hover_text("Double-click to edit value");
                                                if was_double_clicked {
                                                    self.value_editor_var_id = Some(var.id);
                                                    self.value_editor_input =
                                                        format!("{}", point.raw_value);
                                                    self.value_editor_error = None;
                                                    self.show_value_editor = true;
                                                }
                                            } else if !is_writable {
                                                response.on_hover_text(
                                                    "Cannot edit: has converter or non-primitive type",
                                                );
                                            } else {
                                                response.on_hover_text("Cannot edit: not connected");
                                            }
                                        });
                                    } else {
                                        ui.label("-");
                                    }
                                } else {
                                    ui.label("-");
                                }

                                // Remove button
                                if ui.small_button("🗑").on_hover_text("Remove").clicked() {
                                    var_to_remove = Some(var.id);
                                }

                                ui.end_row();
                            }
                        });
                });
            }
        });

        // Handle toggle enabled
        if let Some((id, enabled)) = var_toggle_enabled {
            if let Some(var) = self.config.find_variable_mut(id) {
                var.enabled = enabled;
                if enabled {
                    self.frontend
                        .send_command(BackendCommand::AddVariable(var.clone()));
                } else {
                    self.frontend
                        .send_command(BackendCommand::RemoveVariable(id));
                }
            }
        }

        // Handle toggle graph visibility
        if let Some((id, show)) = var_toggle_graph {
            if let Some(var) = self.config.find_variable_mut(id) {
                var.show_in_graph = show;
            }
            // Also update variable_data if it exists
            if let Some(data) = self.variable_data.get_mut(&id) {
                data.variable.show_in_graph = show;
            }
        }

        // Handle converter editor opening
        if let Some((id, script)) = var_to_edit_converter {
            self.converter_editor_var_id = Some(id);
            self.converter_editor_script = script;
            self.converter_editor_state = ScriptEditorState::default();
            self.show_converter_editor = true;
        }

        // Handle opening variable detail dialog
        if let Some(id) = var_to_open_detail {
            if let Some(var) = self.config.find_variable(id) {
                self.variable_detail_id = Some(id);
                self.variable_detail_color = var.color;
                self.variable_detail_name = var.name.clone();
                self.variable_detail_unit = var.unit.clone();
                self.show_variable_detail = true;
            }
        }

        // Handle removal
        if let Some(id) = var_to_remove {
            self.config.remove_variable(id);
            self.variable_data.remove(&id);
            self.frontend
                .send_command(BackendCommand::RemoveVariable(id));
        }

        // Render converter editor dialog
        self.render_converter_editor_dialog(ctx);

        // Render value editor dialog
        self.render_value_editor_dialog(ctx);

        // Render variable detail dialog
        self.render_variable_detail_dialog(ctx);
    }

    /// Render the converter script editor dialog
    fn render_converter_editor_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_converter_editor {
            return;
        }

        let mut should_close = false;
        let mut should_save = false;

        // Get variable name for the title
        let var_name = self
            .converter_editor_var_id
            .and_then(|id| self.config.variables.iter().find(|v| v.id == id))
            .map(|v| v.name.clone())
            .unwrap_or_else(|| "Variable".to_string());

        egui::Window::new(format!("Converter: {}", var_name))
            .resizable(true)
            .collapsible(false)
            .default_width(550.0)
            .default_height(350.0)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    // Script editor
                    ScriptEditor::new(
                        &mut self.converter_editor_script,
                        &mut self.converter_editor_state,
                        "converter_editor_dialog",
                    )
                    .show(ui);

                    ui.separator();

                    // Buttons
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            should_save = true;
                            should_close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            should_close = true;
                        }
                        if ui.button("Clear").clicked() {
                            self.converter_editor_script.clear();
                        }
                    });
                });
            });

        // Process actions
        if should_save {
            if let Some(id) = self.converter_editor_var_id {
                // Update the variable's converter script
                if let Some(var) = self.config.variables.iter_mut().find(|v| v.id == id) {
                    if self.converter_editor_script.trim().is_empty() {
                        var.converter_script = None;
                    } else {
                        var.converter_script = Some(self.converter_editor_script.clone());
                    }
                    // Notify backend of the update
                    self.frontend
                        .send_command(BackendCommand::UpdateVariable(var.clone()));
                }
            }
        }

        if should_close {
            self.show_converter_editor = false;
            self.converter_editor_var_id = None;
            self.converter_editor_script.clear();
        }
    }

    /// Render the value editor dialog for writing variable values
    fn render_value_editor_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_value_editor {
            return;
        }

        let mut should_close = false;
        let mut should_write = false;

        // Get variable info for the dialog
        let var_info = self.value_editor_var_id.and_then(|id| {
            self.config
                .variables
                .iter()
                .find(|v| v.id == id)
                .map(|v| (v.name.clone(), v.var_type, v.is_writable()))
        });

        let (var_name, var_type, is_writable) = match var_info {
            Some((name, vtype, writable)) => (name, vtype, writable),
            None => {
                self.show_value_editor = false;
                return;
            }
        };

        // Check if we can write
        let can_write = is_writable && self.connection_status == ConnectionStatus::Connected;

        egui::Window::new(format!("Edit Value: {}", var_name))
            .resizable(false)
            .collapsible(false)
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    // Show variable info
                    ui.horizontal(|ui| {
                        ui.label("Type:");
                        ui.label(var_type.to_string());
                    });

                    // Show current value
                    if let Some(id) = self.value_editor_var_id {
                        if let Some(data) = self.variable_data.get(&id) {
                            if let Some(point) = data.last() {
                                ui.horizontal(|ui| {
                                    ui.label("Current:");
                                    ui.label(format!("{:.6}", point.raw_value));
                                });
                            }
                        }
                    }

                    ui.separator();

                    // Value input
                    ui.horizontal(|ui| {
                        ui.label("New value:");
                        let response = ui.text_edit_singleline(&mut self.value_editor_input);

                        // Submit on Enter
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            if can_write {
                                should_write = true;
                            }
                        }
                    });

                    // Show error if any
                    if let Some(ref error) = self.value_editor_error {
                        ui.colored_label(Color32::RED, error);
                    }

                    // Show warnings if not writable
                    if !is_writable {
                        if !var_type.is_writable() {
                            ui.colored_label(Color32::YELLOW, "⚠ Raw types cannot be written");
                        } else {
                            ui.colored_label(
                                Color32::YELLOW,
                                "⚠ Variables with converters cannot be written",
                            );
                        }
                    }
                    if self.connection_status != ConnectionStatus::Connected {
                        ui.colored_label(Color32::YELLOW, "⚠ Not connected to probe");
                    }

                    ui.separator();

                    // Buttons
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(can_write, |ui| {
                            if ui.button("Write").clicked() {
                                should_write = true;
                            }
                        });
                        if ui.button("Cancel").clicked() {
                            should_close = true;
                        }
                    });
                });
            });

        // Process actions
        if should_write {
            // Parse the input value
            match self.value_editor_input.trim().parse::<f64>() {
                Ok(value) => {
                    if let Some(id) = self.value_editor_var_id {
                        // Send write command to backend
                        self.frontend.write_variable(id, value);
                        should_close = true;
                    }
                }
                Err(_) => {
                    // Try parsing as integer (hex, binary, etc.)
                    let input = self.value_editor_input.trim();
                    let parse_result: Option<f64> =
                        if input.starts_with("0x") || input.starts_with("0X") {
                            u64::from_str_radix(&input[2..], 16).ok().map(|v| v as f64)
                        } else if input.starts_with("0b") || input.starts_with("0B") {
                            u64::from_str_radix(&input[2..], 2).ok().map(|v| v as f64)
                        } else {
                            // Try parsing as integer
                            input.parse::<i64>().ok().map(|v| v as f64)
                        };

                    match parse_result {
                        Some(value) => {
                            if let Some(id) = self.value_editor_var_id {
                                self.frontend.write_variable(id, value);
                                should_close = true;
                            }
                        }
                        None => {
                            self.value_editor_error = Some("Invalid number format".to_string());
                        }
                    }
                }
            }
        }

        if should_close {
            self.show_value_editor = false;
            self.value_editor_var_id = None;
            self.value_editor_input.clear();
            self.value_editor_error = None;
        }
    }

    /// Render the variable detail dialog (comprehensive variable editor with color picker)
    fn render_variable_detail_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_variable_detail {
            return;
        }

        let var_id = match self.variable_detail_id {
            Some(id) => id,
            None => {
                self.show_variable_detail = false;
                return;
            }
        };

        // Get current variable info for display
        let (_var_name, var_address, var_type_str, var_enabled, var_show_graph, current_value) = {
            if let Some(var) = self.config.find_variable(var_id) {
                let value = self
                    .variable_data
                    .get(&var_id)
                    .and_then(|d| d.last())
                    .map(|p| p.converted_value);
                (
                    var.name.clone(),
                    var.address,
                    var.var_type.to_string(),
                    var.enabled,
                    var.show_in_graph,
                    value,
                )
            } else {
                self.show_variable_detail = false;
                return;
            }
        };

        let mut should_close = false;
        let mut should_save = false;

        egui::Window::new("Variable Details")
            .collapsible(false)
            .resizable(true)
            .default_width(400.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::Grid::new("variable_detail_grid")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        // Name (editable)
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.variable_detail_name);
                        ui.end_row();

                        // Address (read-only)
                        ui.label("Address:");
                        ui.label(egui::RichText::new(format!("0x{:08X}", var_address)).monospace());
                        ui.end_row();

                        // Type (read-only)
                        ui.label("Type:");
                        ui.label(&var_type_str);
                        ui.end_row();

                        // Unit (editable)
                        ui.label("Unit:");
                        ui.text_edit_singleline(&mut self.variable_detail_unit);
                        ui.end_row();

                        // Current value (read-only)
                        ui.label("Current Value:");
                        if let Some(val) = current_value {
                            let value_text = if self.variable_detail_unit.is_empty() {
                                format!("{:.6}", val)
                            } else {
                                format!("{:.6} {}", val, self.variable_detail_unit)
                            };
                            ui.label(egui::RichText::new(value_text).monospace());
                        } else {
                            ui.colored_label(Color32::GRAY, "No data");
                        }
                        ui.end_row();

                        // Sampling enabled
                        ui.label("Sampling:");
                        ui.label(if var_enabled {
                            "✓ Enabled"
                        } else {
                            "✗ Disabled"
                        });
                        ui.end_row();

                        // Show in graph
                        ui.label("Show in Graph:");
                        ui.label(if var_show_graph { "✓ Yes" } else { "✗ No" });
                        ui.end_row();
                    });

                ui.separator();

                // Color picker section
                ui.horizontal(|ui| {
                    ui.label("Plot Color:");

                    // Color preview swatch
                    let color = Color32::from_rgba_unmultiplied(
                        self.variable_detail_color[0],
                        self.variable_detail_color[1],
                        self.variable_detail_color[2],
                        self.variable_detail_color[3],
                    );
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 4.0, color);

                    // Color picker button
                    let mut srgba = egui::Color32::from_rgba_unmultiplied(
                        self.variable_detail_color[0],
                        self.variable_detail_color[1],
                        self.variable_detail_color[2],
                        self.variable_detail_color[3],
                    );
                    if ui.color_edit_button_srgba(&mut srgba).changed() {
                        self.variable_detail_color = srgba.to_array();
                    }
                });

                // Quick color presets
                ui.horizontal(|ui| {
                    ui.label("Presets:");
                    let presets = [
                        ("Red", [255, 0, 0, 255]),
                        ("Green", [0, 200, 0, 255]),
                        ("Blue", [0, 100, 255, 255]),
                        ("Yellow", [255, 200, 0, 255]),
                        ("Cyan", [0, 200, 200, 255]),
                        ("Magenta", [255, 0, 255, 255]),
                        ("Orange", [255, 128, 0, 255]),
                        ("White", [255, 255, 255, 255]),
                        ("Black", [0, 0, 0, 255]),
                    ];

                    for (name, color) in presets {
                        let c =
                            Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
                        if ui
                            .add(
                                egui::Button::new("")
                                    .fill(c)
                                    .min_size(egui::vec2(20.0, 20.0)),
                            )
                            .on_hover_text(name)
                            .clicked()
                        {
                            self.variable_detail_color = color;
                        }
                    }
                });

                ui.separator();

                // Action buttons
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        should_save = true;
                        should_close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        should_close = true;
                    }
                });
            });

        // Apply changes
        if should_save {
            if let Some(var) = self.config.find_variable_mut(var_id) {
                var.name = self.variable_detail_name.clone();
                var.unit = self.variable_detail_unit.clone();
                var.color = self.variable_detail_color;
            }
            // Also update variable_data if it exists
            if let Some(data) = self.variable_data.get_mut(&var_id) {
                data.variable.name = self.variable_detail_name.clone();
                data.variable.unit = self.variable_detail_unit.clone();
                data.variable.color = self.variable_detail_color;
            }
        }

        if should_close {
            self.show_variable_detail = false;
            self.variable_detail_id = None;
        }
    }

    /// Render the Visualizer page - plots and data display
    fn render_visualizer_page(&mut self, ctx: &egui::Context) {
        // Toolbar for visualizer controls
        egui::TopBottomPanel::top("visualizer_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Collection controls
                if self.connection_status == ConnectionStatus::Connected {
                    if self.settings.collecting {
                        if ui.button("⏹ Stop").clicked() {
                            self.settings.collecting = false;
                            self.frontend.send_command(BackendCommand::StopCollection);
                        }
                    } else {
                        if ui.button("▶ Start").clicked() {
                            self.settings.collecting = true;
                            self.frontend.send_command(BackendCommand::StartCollection);
                        }
                    }
                } else {
                    ui.add_enabled(false, egui::Button::new("▶ Start"));
                    ui.label("Connect to a probe first");
                }

                ui.separator();

                // Clear data button
                if ui.button("🗑 Clear").clicked() {
                    for data in self.variable_data.values_mut() {
                        data.clear();
                    }
                }

                ui.separator();

                // Stats - show rate with color indicating if throttled
                let target_rate = self.config.collection.poll_rate_hz as f64;
                let actual_rate = self.stats.effective_sample_rate;
                // Consider throttled if actual rate is less than 90% of target
                let is_throttled = actual_rate > 0.0 && actual_rate < target_rate * 0.9;
                let rate_color = if is_throttled {
                    Color32::from_rgb(255, 100, 100) // Red when throttled
                } else if actual_rate > 0.0 {
                    Color32::from_rgb(100, 255, 100) // Green when at target
                } else {
                    Color32::GRAY // Gray when not collecting
                };

                ui.horizontal(|ui| {
                    ui.label("Rate:");
                    ui.colored_label(rate_color, format!("{:.0} Hz", actual_rate));
                    if is_throttled {
                        ui.colored_label(
                            Color32::from_rgb(255, 200, 100),
                            format!("(target: {} Hz)", self.config.collection.poll_rate_hz),
                        );
                    }
                    ui.label(format!("| Success: {:.1}%", self.stats.success_rate()));
                });
            });

            // Second row for axis controls
            ui.horizontal(|ui| {
                // X-axis controls
                ui.label("X-Axis:");

                // Autoscale X toggle
                let autoscale_x_text = if self.settings.autoscale_x {
                    "🔄 Auto"
                } else {
                    "🔄 Manual"
                };
                if ui
                    .selectable_label(self.settings.autoscale_x, autoscale_x_text)
                    .on_hover_text("Auto-scale X axis (follow latest data)")
                    .clicked()
                {
                    self.settings.toggle_autoscale_x();
                }

                // Lock X toggle
                let lock_x_text = if self.settings.lock_x { "🔒" } else { "🔓" };
                if ui
                    .selectable_label(self.settings.lock_x, lock_x_text)
                    .on_hover_text(if self.settings.lock_x {
                        "X-axis locked (click to unlock)"
                    } else {
                        "X-axis unlocked (click to lock)"
                    })
                    .clicked()
                {
                    self.settings.toggle_lock_x();
                }

                ui.separator();

                // Time window control (only enabled when not autoscaling X)
                ui.add_enabled_ui(!self.settings.autoscale_x, |ui| {
                    ui.label("Time window:");
                    let max_window = self.settings.max_time_window;
                    if ui
                        .add(
                            egui::Slider::new(
                                &mut self.settings.display_time_window,
                                0.5..=max_window,
                            )
                            .suffix("s")
                            .logarithmic(true),
                        )
                        .changed()
                    {
                        // Clamp to max
                        self.settings.display_time_window =
                            self.settings.display_time_window.clamp(0.1, max_window);
                    }
                });

                ui.separator();

                // Y-axis controls
                ui.label("Y-Axis:");

                // Autoscale Y toggle
                let autoscale_y_text = if self.settings.autoscale_y {
                    "🔄 Auto"
                } else {
                    "🔄 Manual"
                };
                if ui
                    .selectable_label(self.settings.autoscale_y, autoscale_y_text)
                    .on_hover_text("Auto-scale Y axis (fit to visible data)")
                    .clicked()
                {
                    self.settings.toggle_autoscale_y();
                }

                // Lock Y toggle
                let lock_y_text = if self.settings.lock_y { "🔒" } else { "🔓" };
                if ui
                    .selectable_label(self.settings.lock_y, lock_y_text)
                    .on_hover_text(if self.settings.lock_y {
                        "Y-axis locked (click to unlock)"
                    } else {
                        "Y-axis unlocked (click to lock)"
                    })
                    .clicked()
                {
                    self.settings.toggle_lock_y();
                }

                ui.separator();

                // Reset view button
                if ui
                    .button("↺ Reset View")
                    .on_hover_text("Reset to autoscale on both axes")
                    .clicked()
                {
                    self.settings.autoscale_x = true;
                    self.settings.autoscale_y = true;
                    self.settings.follow_latest = true;
                    self.settings.lock_x = false;
                    self.settings.lock_y = false;
                    self.settings.x_min = None;
                    self.settings.x_max = None;
                    self.settings.y_min = None;
                    self.settings.y_max = None;
                    self.settings.display_time_window = 10.0;
                }
            });
        });

        // Side panel with variable legend
        egui::SidePanel::right("visualizer_legend")
            .default_width(200.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Variables");
                ui.separator();

                for var in &self.config.variables {
                    if !var.enabled {
                        continue;
                    }

                    ui.horizontal(|ui| {
                        // Color indicator
                        let color = Color32::from_rgba_unmultiplied(
                            var.color[0],
                            var.color[1],
                            var.color[2],
                            var.color[3],
                        );
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, color);

                        ui.label(&var.name);
                    });

                    // Show stats (value is double-clickable to edit if writable)
                    let is_writable = var.is_writable();
                    let is_connected = self.connection_status == ConnectionStatus::Connected;
                    let can_edit = is_writable && is_connected;
                    let var_id = var.id;

                    if let Some(data) = self.variable_data.get(&var.id) {
                        ui.indent(var.id, |ui| {
                            if let Some(last) = data.last() {
                                let value_text = format!("Value: {:.3}", last.converted_value);
                                let response = ui.add(
                                    egui::Label::new(egui::RichText::new(&value_text).color(
                                        if can_edit {
                                            egui::Color32::WHITE
                                        } else {
                                            egui::Color32::GRAY
                                        },
                                    ))
                                    .sense(egui::Sense::click()),
                                );

                                if can_edit {
                                    let response =
                                        response.on_hover_text("Double-click to edit value");
                                    if response.double_clicked() {
                                        self.value_editor_var_id = Some(var_id);
                                        self.value_editor_input = format!("{}", last.raw_value);
                                        self.value_editor_error = None;
                                        self.show_value_editor = true;
                                    }
                                } else if !is_writable {
                                    response.on_hover_text(
                                        "Cannot edit: has converter or non-primitive type",
                                    );
                                } else {
                                    response.on_hover_text("Cannot edit: not connected");
                                }
                            }
                            let stats = data.statistics();
                            ui.label(format!("Min: {:.3}", stats.0));
                            ui.label(format!("Max: {:.3}", stats.1));
                            ui.label(format!("Avg: {:.3}", stats.2));
                        });
                    }

                    ui.separator();
                }
            });

        // Main plot area
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_plot(ui);
        });

        // Render value editor dialog
        self.render_value_editor_dialog(ctx);
    }

    /// Render the Settings page - configuration options
    fn render_settings_page(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Settings");
                ui.separator();

                // ============ Project Section ============
                ui.heading("📁 Project");
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("Project Name:");
                    ui.text_edit_singleline(&mut self.project_name);
                });

                ui.add_space(8.0);
                ui.label("Project File (.datavisproj):");
                ui.horizontal(|ui| {
                    if let Some(ref path) = self.project_file_path {
                        ui.label(path.display().to_string());
                    } else {
                        ui.label("(no project file)");
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("💾 Save Project").clicked() {
                        if let Some(ref path) = self.project_file_path {
                            self.save_project_to_path(path.clone());
                        } else {
                            // Prompt for path
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter(
                                    "DataVis Project",
                                    &[crate::config::PROJECT_FILE_EXTENSION],
                                )
                                .set_file_name("project.datavisproj")
                                .save_file()
                            {
                                self.save_project_to_path(path);
                            }
                        }
                    }
                    if ui.button("💾 Save Project As...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("DataVis Project", &[crate::config::PROJECT_FILE_EXTENSION])
                            .set_file_name("project.datavisproj")
                            .save_file()
                        {
                            self.save_project_to_path(path);
                        }
                    }
                    if ui.button("📂 Load Project...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("DataVis Project", &[crate::config::PROJECT_FILE_EXTENSION])
                            .pick_file()
                        {
                            self.load_project_from_path(path);
                        }
                    }
                });

                ui.separator();

                // ============ Probe Connection Section ============
                ui.heading("🔌 Probe Connection");
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("Target Chip:");
                    ui.text_edit_singleline(&mut self.target_chip_input);
                    if ui.button("Apply").clicked() {
                        self.config.probe.target_chip = self.target_chip_input.clone();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Speed (kHz):");
                    ui.add(
                        egui::DragValue::new(&mut self.config.probe.speed_khz).range(100..=50000),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("Connect Under Reset:");
                    egui::ComboBox::from_id_salt("connect_under_reset")
                        .selected_text(self.config.probe.connect_under_reset.to_string())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.config.probe.connect_under_reset,
                                crate::config::ConnectUnderReset::None,
                                "None (normal attach)",
                            );
                            ui.selectable_value(
                                &mut self.config.probe.connect_under_reset,
                                crate::config::ConnectUnderReset::Software,
                                "Software (SYSRESETREQ)",
                            );
                            ui.selectable_value(
                                &mut self.config.probe.connect_under_reset,
                                crate::config::ConnectUnderReset::Hardware,
                                "Hardware (NRST pin)",
                            );
                            ui.selectable_value(
                                &mut self.config.probe.connect_under_reset,
                                crate::config::ConnectUnderReset::Core,
                                "Core Reset (VECTRESET)",
                            );
                        });
                });

                ui.checkbox(&mut self.config.probe.halt_on_connect, "Halt on connect");

                ui.horizontal(|ui| {
                    ui.label("Memory Access:");
                    let current_mode = self.config.probe.memory_access_mode;
                    egui::ComboBox::from_id_salt("memory_access_mode")
                        .selected_text(current_mode.to_string())
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(
                                    &mut self.config.probe.memory_access_mode,
                                    crate::config::MemoryAccessMode::Background,
                                    "Background (Running)",
                                )
                                .changed()
                            {
                                self.frontend
                                    .send_command(BackendCommand::SetMemoryAccessMode(
                                        self.config.probe.memory_access_mode,
                                    ));
                            }
                            if ui
                                .selectable_value(
                                    &mut self.config.probe.memory_access_mode,
                                    crate::config::MemoryAccessMode::Halted,
                                    "Halted (Per-batch)",
                                )
                                .changed()
                            {
                                self.frontend
                                    .send_command(BackendCommand::SetMemoryAccessMode(
                                        self.config.probe.memory_access_mode,
                                    ));
                            }
                            if ui
                                .selectable_value(
                                    &mut self.config.probe.memory_access_mode,
                                    crate::config::MemoryAccessMode::HaltedPersistent,
                                    "Halted (Persistent)",
                                )
                                .changed()
                            {
                                self.frontend
                                    .send_command(BackendCommand::SetMemoryAccessMode(
                                        self.config.probe.memory_access_mode,
                                    ));
                            }
                        });
                })
                .response
                .on_hover_text(
                    "Background: Read while target runs (slower)\n\
                     Halted: Briefly halt for each read batch (faster)\n\
                     Persistent: Keep target halted (fastest)",
                );

                ui.horizontal(|ui| {
                    ui.label("Probe:");
                    egui::ComboBox::from_id_salt("settings_probe_selector")
                        .selected_text(
                            self.selected_probe_index
                                .and_then(|i| self.available_probes.get(i))
                                .map(|p| p.display_name())
                                .as_deref()
                                .unwrap_or("Select probe..."),
                        )
                        .show_ui(ui, |ui| {
                            for (i, probe) in self.available_probes.iter().enumerate() {
                                let is_selected = self.selected_probe_index == Some(i);
                                if ui
                                    .selectable_label(is_selected, probe.display_name())
                                    .clicked()
                                {
                                    self.selected_probe_index = Some(i);
                                    #[cfg(feature = "mock-probe")]
                                    {
                                        self.use_mock_probe = probe.is_mock();
                                    }
                                }
                            }
                        });
                    if ui.button("Refresh").clicked() {
                        self.frontend.send_command(BackendCommand::RefreshProbes);
                    }
                });

                ui.horizontal(|ui| match self.connection_status {
                    ConnectionStatus::Connected => {
                        if ui.button("Disconnect").clicked() {
                            self.frontend.send_command(BackendCommand::Disconnect);
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
                            // Tell backend whether to use mock probe (only with feature)
                            #[cfg(feature = "mock-probe")]
                            self.frontend.use_mock_probe(self.use_mock_probe);

                            if let Some(idx) = self.selected_probe_index {
                                let selector = match self.available_probes.get(idx) {
                                    Some(DetectedProbe::Real(info)) => Some(format!(
                                        "{:04x}:{:04x}",
                                        info.vendor_id, info.product_id
                                    )),
                                    #[cfg(feature = "mock-probe")]
                                    Some(DetectedProbe::Mock(_)) => None,
                                    _ => None,
                                };
                                self.frontend.send_command(BackendCommand::Connect {
                                    selector,
                                    target: self.config.probe.target_chip.clone(),
                                    probe_config: self.config.probe.clone(),
                                });
                            }
                        }
                    }
                });

                ui.separator();

                // ============ Data Collection Section ============
                ui.heading("📊 Data Collection");
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("Poll Rate (Hz):");
                    let mut rate = self.config.collection.poll_rate_hz;
                    if ui
                        .add(egui::DragValue::new(&mut rate).range(1..=10000))
                        .changed()
                    {
                        self.config.collection.poll_rate_hz = rate;
                        self.frontend
                            .send_command(BackendCommand::SetPollRate(rate));
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Max Data Points:");
                    ui.add(
                        egui::DragValue::new(&mut self.config.collection.max_data_points)
                            .range(100..=1000000),
                    );
                });

                ui.separator();

                // ============ Data Persistence Section ============
                ui.heading("💿 Data Persistence");
                ui.add_space(4.0);

                ui.checkbox(
                    &mut self.persistence_config.enabled,
                    "Enable data persistence",
                );

                if self.persistence_config.enabled {
                    ui.horizontal(|ui| {
                        ui.label("Persistence File:");
                        if let Some(ref path) = self.persistence_config.file_path {
                            ui.label(path.display().to_string());
                        } else {
                            ui.label("(not set)");
                        }
                        if ui.button("Browse...").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("CSV", &["csv"])
                                .add_filter("JSON Lines", &["jsonl"])
                                .add_filter("Binary", &["bin"])
                                .set_file_name("data.csv")
                                .save_file()
                            {
                                self.persistence_config.file_path = Some(path);
                            }
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Max File Size:");
                        let mut size_gb = self.persistence_config.max_file_size as f64
                            / (1024.0 * 1024.0 * 1024.0);
                        if ui
                            .add(egui::Slider::new(&mut size_gb, 0.1..=2.0).suffix(" GB"))
                            .changed()
                        {
                            self.persistence_config.max_file_size =
                                (size_gb * 1024.0 * 1024.0 * 1024.0) as u64;
                        }
                        ui.label(format!(
                            "({})",
                            crate::config::format_file_size(self.persistence_config.max_file_size)
                        ));
                    });

                    ui.horizontal(|ui| {
                        ui.label("Format:");
                        egui::ComboBox::from_id_salt("persistence_format")
                            .selected_text(self.persistence_config.format.to_string())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.persistence_config.format,
                                    crate::config::PersistenceFormat::Csv,
                                    "CSV",
                                );
                                ui.selectable_value(
                                    &mut self.persistence_config.format,
                                    crate::config::PersistenceFormat::JsonLines,
                                    "JSON Lines",
                                );
                                ui.selectable_value(
                                    &mut self.persistence_config.format,
                                    crate::config::PersistenceFormat::Binary,
                                    "Binary",
                                );
                            });
                    });

                    ui.checkbox(
                        &mut self.persistence_config.include_variable_name,
                        "Include variable name",
                    );
                    ui.checkbox(
                        &mut self.persistence_config.include_variable_address,
                        "Include variable address",
                    );
                    ui.checkbox(
                        &mut self.persistence_config.append_mode,
                        "Append to existing file",
                    );
                }

                ui.separator();

                // ============ Display Section ============
                ui.heading("🖥️ Display");
                ui.add_space(4.0);

                ui.checkbox(&mut self.config.ui.show_grid, "Show Grid");
                ui.checkbox(&mut self.config.ui.show_legend, "Show Legend");
                ui.checkbox(&mut self.config.ui.auto_scale_y, "Auto-scale Y Axis");

                ui.horizontal(|ui| {
                    ui.label("Line Width:");
                    ui.add(egui::Slider::new(&mut self.config.ui.line_width, 0.5..=5.0));
                });

                ui.horizontal(|ui| {
                    ui.label("Default Time Window:");
                    ui.add(
                        egui::Slider::new(&mut self.config.ui.time_window_seconds, 1.0..=120.0)
                            .suffix("s"),
                    );
                });

                // Show any errors
                if let Some(ref error) = self.last_error {
                    ui.separator();
                    ui.colored_label(Color32::RED, format!("Error: {}", error));
                    if ui.button("Dismiss").clicked() {
                        self.last_error = None;
                    }
                }
            });
        });
    }

    /// Save the current project to a file path
    fn save_project_to_path(&mut self, path: PathBuf) {
        let project = crate::config::ProjectFile {
            version: 1,
            name: self.project_name.clone(),
            config: self.config.clone(),
            binary_path: self.elf_file_path.clone(),
            persistence: self.persistence_config.clone(),
        };

        match project.save(&path) {
            Ok(()) => {
                self.project_file_path = Some(path.clone());

                // Update app state with recent project
                self.app_state.add_recent_project(
                    &path,
                    &self.project_name,
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
                self.project_name = project.name.clone();
                self.project_file_path = Some(path.clone());

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
                            tracing::info!("Loaded ELF from project: {:?}", binary_path);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse ELF from project: {}", e);
                        }
                    }
                }

                // Update target chip input
                self.target_chip_input = self.config.probe.target_chip.clone();

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

    /// Render the duplicate variable confirmation dialog
    fn render_duplicate_confirm_dialog(&mut self, ctx: &egui::Context) {
        let mut should_close = false;
        let mut confirmed = false;

        egui::Window::new("Duplicate Variable")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if let Some(ref var) = self.pending_variable {
                    ui.label(format!(
                        "A variable at address 0x{:08X} already exists.",
                        var.address
                    ));
                    ui.label("Do you want to add it anyway?");

                    ui.separator();

                    ui.horizontal(|ui| {
                        if ui.button("Add Anyway").clicked() {
                            confirmed = true;
                            should_close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            should_close = true;
                        }
                    });
                } else {
                    should_close = true;
                }
            });

        if confirmed {
            if let Some(var) = self.pending_variable.take() {
                self.add_variable_confirmed(var);
            }
        }

        if should_close {
            self.show_duplicate_confirm = false;
            self.pending_variable = None;
        }
    }
}

impl eframe::App for DataVisApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process backend messages
        let had_messages = self.process_backend_messages();

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
                    let text = format!("{} {}", page.icon(), page.name());
                    if ui.selectable_label(selected, text).clicked() {
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
                    ui.colored_label(status_color, format!("● {}", status_text));

                    // Show collection status on visualizer
                    if self.current_page == AppPage::Visualizer && self.settings.collecting {
                        if self.settings.paused {
                            ui.colored_label(Color32::YELLOW, "⏸ Paused");
                        } else {
                            ui.colored_label(Color32::GREEN, "● Recording");
                        }
                    }
                });
            });
        });

        // Render current page
        match self.current_page {
            AppPage::Variables => self.render_variables_page(ctx),
            AppPage::Visualizer => self.render_visualizer_page(ctx),
            AppPage::Settings => self.render_settings_page(ctx),
        }

        // Dialogs (can appear on any page)
        if self.show_add_variable_dialog {
            self.render_add_variable_dialog(ctx);
        }

        // Duplicate variable confirmation dialog
        if self.show_duplicate_confirm {
            self.render_duplicate_confirm_dialog(ctx);
        }
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
        if let Some(ref path) = self.project_file_path {
            let project = crate::config::ProjectFile {
                version: 1,
                name: self.project_name.clone(),
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
