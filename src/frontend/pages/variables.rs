//! Variables page - ELF browser, variable list, and selection
//!
//! This page provides:
//! - Left panel: ELF file browser with variable tree
//! - Center panel: Selected variables table with controls
//! - Dialog management for variable editing

use std::collections::HashSet;

use egui::{Color32, Context, Ui};

use super::Page;
use crate::backend::{DwarfDiagnostics, ElfInfo, ElfSymbol, TypeHandle};
use crate::frontend::dialogs::{
    show_dialog, show_dialog_with_title, ConverterEditorAction, ConverterEditorContext,
    ConverterEditorDialog, ConverterEditorState, ElfSymbolsState, ValueEditorAction,
    ValueEditorContext, ValueEditorDialog, ValueEditorState, VariableDetailAction,
    VariableDetailContext, VariableDetailDialog, VariableDetailState,
};
use crate::frontend::state::{AppAction, SharedState};
use crate::types::{ConnectionStatus, Variable};

/// Maximum number of array elements to display when expanding an array
const MAX_ARRAY_ELEMENTS: u64 = 1024;

/// State specific to the Variables page
#[derive(Default)]
pub struct VariablesPageState {
    /// Variable selector/autocomplete state
    pub selector: VariableSelectorState,
    /// Whether selector dropdown is visible
    pub show_selector: bool,
    /// Currently selected variable for detail view
    pub selected_id: Option<u32>,
    /// Advanced mode shows all options (converter, plot style, graph toggle)
    pub advanced_mode: bool,

    // Dialog states (owned by the page, not shared)
    /// Converter editor dialog
    pub converter_editor_open: bool,
    pub converter_editor_state: ConverterEditorState,
    /// Value editor dialog
    pub value_editor_open: bool,
    pub value_editor_state: ValueEditorState,
    /// Variable detail dialog
    pub variable_detail_open: bool,
    pub variable_detail_state: VariableDetailState,
    /// ELF symbols dialog
    pub elf_symbols_open: bool,
    pub elf_symbols_state: ElfSymbolsState,

    // Inline color picker state
    /// Variable ID for which color picker is open
    pub color_picker_var_id: Option<u32>,
    /// Temporary color being edited
    pub color_picker_color: [u8; 4],
}

/// State for variable autocomplete/selector
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
    /// Set of expanded paths for tree view
    pub expanded_paths: HashSet<String>,
    /// Whether to show unreadable variables (optimized out, extern, etc.)
    pub show_unreadable: bool,
}

impl VariableSelectorState {
    /// Update filtered symbols based on query and show_unreadable setting
    pub fn update_filter(&mut self, elf_info: Option<&ElfInfo>) {
        self.filtered_symbols.clear();
        if let Some(info) = elf_info {
            let results = info.search_variables(&self.query);
            // Filter based on show_unreadable setting
            self.filtered_symbols = results
                .into_iter()
                .filter(|s| self.show_unreadable || s.is_readable())
                .cloned()
                .collect();
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

    /// Toggle expansion state for a tree path
    pub fn toggle_expanded(&mut self, path: &str) {
        if self.expanded_paths.contains(path) {
            self.expanded_paths.remove(path);
        } else {
            self.expanded_paths.insert(path.to_string());
        }
    }
}

pub struct VariablesPage;

impl Page for VariablesPage {
    type State = VariablesPageState;

    fn render(
        state: &mut Self::State,
        shared: &mut SharedState<'_>,
        ctx: &Context,
    ) -> Vec<AppAction> {
        let mut actions = Vec::new();

        // Track deferred actions to avoid borrow issues
        let mut var_to_remove: Option<u32> = None;
        let mut var_to_edit_converter: Option<(u32, String)> = None;
        let mut var_to_open_detail: Option<u32> = None;
        let mut var_toggle_enabled: Option<(u32, bool)> = None;
        let mut var_toggle_graph: Option<(u32, bool)> = None;
        let mut var_cycle_plot_style: Option<u32> = None;
        let mut var_update_color: Option<(u32, [u8; 4])> = None;
        let mut variables_to_add: Vec<Variable> = Vec::new();

        // Left panel: ELF browser
        egui::SidePanel::left("variable_browser")
            .default_width(350.0)
            .resizable(true)
            .show(ctx, |ui| {
                Self::render_elf_browser(state, shared, ui, &mut actions, &mut variables_to_add);
            });

        // Center panel: Variable list
        egui::CentralPanel::default().show(ctx, |ui| {
            Self::render_variable_list(
                state,
                shared,
                ui,
                &mut var_to_remove,
                &mut var_to_edit_converter,
                &mut var_to_open_detail,
                &mut var_toggle_enabled,
                &mut var_toggle_graph,
                &mut var_cycle_plot_style,
                &mut var_update_color,
            );
        });

        // Handle deferred actions
        for var in variables_to_add {
            actions.push(AppAction::AddVariable(var));
        }

        if let Some((id, enabled)) = var_toggle_enabled {
            if let Some(var) = shared.config.find_variable_mut(id) {
                var.enabled = enabled;
                actions.push(AppAction::UpdateVariable(var.clone()));
            }
        }

        if let Some((id, show)) = var_toggle_graph {
            if let Some(var) = shared.config.find_variable_mut(id) {
                var.show_in_graph = show;
            }
            if let Some(data) = shared.variable_data.get_mut(&id) {
                data.variable.show_in_graph = show;
            }
        }

        if let Some(id) = var_cycle_plot_style {
            if let Some(var) = shared.config.find_variable_mut(id) {
                var.plot_style = var.plot_style.next();
            }
            if let Some(data) = shared.variable_data.get_mut(&id) {
                data.variable.plot_style = data.variable.plot_style.next();
            }
        }

        if let Some((id, color)) = var_update_color {
            if let Some(var) = shared.config.find_variable_mut(id) {
                var.color = color;
            }
            if let Some(data) = shared.variable_data.get_mut(&id) {
                data.variable.color = color;
            }
        }

        if let Some((id, script)) = var_to_edit_converter {
            state.converter_editor_state = ConverterEditorState::edit(id, script);
            state.converter_editor_open = true;
        }

        if let Some(id) = var_to_open_detail {
            if let Some(var) = shared.config.find_variable(id) {
                state.variable_detail_state =
                    VariableDetailState::for_variable(id, &var.name, &var.unit, var.color);
                state.variable_detail_open = true;
            }
        }

        if let Some(id) = var_to_remove {
            actions.push(AppAction::RemoveVariable(id));
        }

        // Render dialogs and collect their actions
        Self::render_dialogs(state, shared, ctx, &mut actions);

        actions
    }
}

impl VariablesPage {
    fn render_elf_browser(
        state: &mut VariablesPageState,
        shared: &mut SharedState<'_>,
        ui: &mut Ui,
        actions: &mut Vec<AppAction>,
        variables_to_add: &mut Vec<Variable>,
    ) {
        ui.heading("Variable Browser");
        ui.separator();

        // ELF file selection
        ui.horizontal(|ui| {
            ui.label("ELF File:");
            if let Some(path) = shared.elf_file_path {
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
                    actions.push(AppAction::LoadElf(path));
                }
            }
        });

        if shared.elf_info.is_some() {
            ui.horizontal(|ui| {
                ui.label(format!("{} variables available", shared.elf_symbols.len()));
            });
        }

        ui.separator();

        // Search filter
        ui.horizontal(|ui| {
            ui.label("Search:");
            let response = ui.text_edit_singleline(&mut state.selector.query);
            if response.changed() {
                state.selector.update_filter(shared.elf_info);
            }
            if ui.button("Clear").clicked() {
                state.selector.query.clear();
                state.selector.update_filter(shared.elf_info);
            }
        });

        // Show unreadable toggle
        ui.horizontal(|ui| {
            if ui.checkbox(&mut state.selector.show_unreadable, "Show unreadable")
                .on_hover_text("Show optimized out, extern, and local variables")
                .changed()
            {
                state.selector.update_filter(shared.elf_info);
            }
        });

        // Diagnostics summary (collapsible)
        if let Some(elf_info) = shared.elf_info {
            let diagnostics = elf_info.get_diagnostics();
            if diagnostics.total_variables > 0 {
                ui.collapsing("Debug Info Statistics", |ui| {
                    Self::render_diagnostics_summary(ui, diagnostics);
                });
            }
        }

        ui.separator();

        // Variable tree browser
        let mut toggle_expand_path: Option<String> = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if state.selector.filtered_symbols.is_empty() {
                    if shared.elf_info.is_some() {
                        ui.colored_label(Color32::GRAY, "No matching variables");
                    } else {
                        ui.colored_label(Color32::GRAY, "Load an ELF file to browse variables");
                    }
                } else {
                    for idx in 0..state.selector.filtered_symbols.len() {
                        let symbol = &state.selector.filtered_symbols[idx];
                        let root_path = idx.to_string();

                        // Show status indicator for unreadable variables
                        let is_unreadable = !symbol.is_readable();
                        if is_unreadable {
                            ui.horizontal(|ui| {
                                ui.add_space(18.0); // Align with expand button
                                let status_text = symbol.detailed_unreadable_reason()
                                    .unwrap_or_else(|| "Cannot read".to_string());
                                let label = ui.colored_label(Color32::YELLOW, "⚠");
                                label.on_hover_text(&status_text);
                                ui.label(egui::RichText::new(&symbol.display_name).color(Color32::GRAY));
                                ui.label(egui::RichText::new(format!("- {}", status_text)).small().color(Color32::DARK_GRAY));
                            });
                        } else {
                            Self::render_type_tree(
                                ui,
                                &symbol.display_name,
                                symbol.address,
                                shared
                                    .elf_info
                                    .and_then(|info| info.symbol_type_handle(symbol)),
                                &root_path,
                                0,
                                &state.selector.expanded_paths,
                                state.selector.selected_index == Some(idx),
                                &mut toggle_expand_path,
                                variables_to_add,
                                &mut None,
                                None,
                            );
                        }
                    }
                }
            });

        // Handle toggle expand
        if let Some(path) = toggle_expand_path {
            state.selector.toggle_expanded(&path);
        }
    }

    fn render_variable_list(
        state: &mut VariablesPageState,
        shared: &mut SharedState<'_>,
        ui: &mut Ui,
        var_to_remove: &mut Option<u32>,
        var_to_edit_converter: &mut Option<(u32, String)>,
        var_to_open_detail: &mut Option<u32>,
        var_toggle_enabled: &mut Option<(u32, bool)>,
        var_toggle_graph: &mut Option<(u32, bool)>,
        var_cycle_plot_style: &mut Option<u32>,
        var_update_color: &mut Option<(u32, [u8; 4])>,
    ) {
        // Header with title and advanced mode toggle
        ui.horizontal(|ui| {
            ui.heading("Selected Variables");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(&mut state.advanced_mode, "Advanced");
            });
        });
        ui.separator();

        if shared.config.variables.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                ui.label("No variables selected yet.");
                ui.label("Use the browser on the left to add variables.");
            });
        } else {
            let advanced_mode = state.advanced_mode;
            egui::ScrollArea::vertical().show(ui, |ui| {
                // Clone necessary data to avoid borrow conflicts
                let variables: Vec<_> = shared.config.variables.clone();
                for var in &variables {
                    if advanced_mode {
                        Self::render_variable_card_advanced(
                            state,
                            shared,
                            ui,
                            var,
                            var_to_remove,
                            var_to_edit_converter,
                            var_to_open_detail,
                            var_toggle_enabled,
                            var_toggle_graph,
                            var_cycle_plot_style,
                            var_update_color,
                        );
                    } else {
                        Self::render_variable_card_simple(
                            ui,
                            var,
                            shared,
                            var_to_remove,
                            var_toggle_enabled,
                        );
                    }
                }
            });
        }
    }

    /// Render a simple single-line variable card (basic mode)
    fn render_variable_card_simple(
        ui: &mut Ui,
        var: &Variable,
        shared: &SharedState<'_>,
        var_to_remove: &mut Option<u32>,
        var_toggle_enabled: &mut Option<(u32, bool)>,
    ) {
        let var_color = Color32::from_rgba_unmultiplied(
            var.color[0],
            var.color[1],
            var.color[2],
            var.color[3],
        );

        // Simple single-line card
        let frame = egui::Frame::new()
            .fill(ui.visuals().widgets.noninteractive.bg_fill)
            .inner_margin(6.0)
            .outer_margin(2.0)
            .corner_radius(4.0);

        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                // Color swatch
                let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 3.0, var_color);

                // Variable name
                ui.label(&var.name);

                // Spacer
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Delete button
                    if ui.small_button("×").on_hover_text("Remove").clicked() {
                        *var_to_remove = Some(var.id);
                    }

                    // Sample toggle
                    let mut enabled = var.enabled;
                    if ui.checkbox(&mut enabled, "").on_hover_text(
                        if enabled { "Sampling (click to disable)" } else { "Not sampling (click to enable)" }
                    ).changed() {
                        *var_toggle_enabled = Some((var.id, enabled));
                    }

                    // Current value
                    if let Some(data) = shared.variable_data.get(&var.id) {
                        if let Some(point) = data.last() {
                            let value_text = if var.unit.is_empty() {
                                format!("{:.3}", point.converted_value)
                            } else {
                                format!("{:.3} {}", point.converted_value, var.unit)
                            };
                            ui.label(egui::RichText::new(value_text).monospace().color(var_color));
                        } else {
                            ui.label(egui::RichText::new("—").color(Color32::GRAY));
                        }
                    } else {
                        ui.label(egui::RichText::new("—").color(Color32::GRAY));
                    }
                });
            });
        });
    }

    /// Render a variable as a two-line stacked card (advanced mode)
    #[allow(clippy::too_many_arguments)]
    fn render_variable_card_advanced(
        state: &mut VariablesPageState,
        shared: &mut SharedState<'_>,
        ui: &mut Ui,
        var: &Variable,
        var_to_remove: &mut Option<u32>,
        var_to_edit_converter: &mut Option<(u32, String)>,
        var_to_open_detail: &mut Option<u32>,
        var_toggle_enabled: &mut Option<(u32, bool)>,
        var_toggle_graph: &mut Option<(u32, bool)>,
        var_cycle_plot_style: &mut Option<u32>,
        var_update_color: &mut Option<(u32, [u8; 4])>,
    ) {
        let var_color = Color32::from_rgba_unmultiplied(
            var.color[0],
            var.color[1],
            var.color[2],
            var.color[3],
        );

        // Card frame with subtle background
        let frame = egui::Frame::new()
            .fill(ui.visuals().widgets.noninteractive.bg_fill)
            .inner_margin(8.0)
            .outer_margin(2.0)
            .corner_radius(4.0);

        frame.show(ui, |ui| {
            // Line 1: [Color] Name | Value | [S] [G] [Style] [×]
            ui.horizontal(|ui| {
                // Color swatch (clickable for color picker)
                let swatch_size = egui::vec2(24.0, 24.0);
                let (rect, swatch_response) = ui.allocate_exact_size(swatch_size, egui::Sense::click());
                ui.painter().rect_filled(rect, 4.0, var_color);
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.fg_stroke.color),
                    egui::StrokeKind::Outside,
                );

                if swatch_response.clicked() {
                    // Toggle color picker popup
                    if state.color_picker_var_id == Some(var.id) {
                        state.color_picker_var_id = None;
                    } else {
                        state.color_picker_var_id = Some(var.id);
                        state.color_picker_color = var.color;
                    }
                }
                swatch_response.on_hover_text("Click to change color");

                // Variable name (clickable to open detail)
                let name_response = ui.add(
                    egui::Label::new(egui::RichText::new(&var.name).strong())
                        .sense(egui::Sense::click())
                );
                if name_response.clicked() {
                    *var_to_open_detail = Some(var.id);
                }
                name_response.on_hover_text("Click to edit details");

                // Spacer to push value and buttons to the right
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Delete button
                    if ui.small_button("×").on_hover_text("Remove variable").clicked() {
                        *var_to_remove = Some(var.id);
                    }

                    ui.add_space(4.0);

                    // Plot style toggle
                    let style_text = var.plot_style.display_name();
                    let style_btn = ui.add(
                        egui::Button::new(egui::RichText::new(style_text).small())
                    );
                    if style_btn.clicked() {
                        *var_cycle_plot_style = Some(var.id);
                    }
                    style_btn.on_hover_text("Click to cycle plot style");

                    // Graph toggle
                    let (graph_text, graph_color) = if var.show_in_graph {
                        ("Graph", ui.visuals().widgets.active.fg_stroke.color)
                    } else {
                        ("Graph", Color32::GRAY)
                    };
                    let graph_btn = ui.add(
                        egui::Button::new(egui::RichText::new(graph_text).small().color(graph_color))
                    );
                    if graph_btn.clicked() {
                        *var_toggle_graph = Some((var.id, !var.show_in_graph));
                    }
                    graph_btn.on_hover_text(if var.show_in_graph { "Showing in graph (click to hide)" } else { "Hidden from graph (click to show)" });

                    // Sample toggle
                    let (sample_text, sample_color) = if var.enabled {
                        ("Sample", Color32::from_rgb(100, 200, 100))
                    } else {
                        ("Sample", Color32::GRAY)
                    };
                    let sample_btn = ui.add(
                        egui::Button::new(egui::RichText::new(sample_text).small().color(sample_color))
                    );
                    if sample_btn.clicked() {
                        *var_toggle_enabled = Some((var.id, !var.enabled));
                    }
                    sample_btn.on_hover_text(if var.enabled { "Sampling enabled (click to disable)" } else { "Sampling disabled (click to enable)" });

                    ui.add_space(8.0);

                    // Current value (prominent display)
                    if let Some(data) = shared.variable_data.get(&var.id) {
                        if let Some(point) = data.last() {
                            let value_text = if var.unit.is_empty() {
                                format!("{:.3}", point.converted_value)
                            } else {
                                format!("{:.3} {}", point.converted_value, var.unit)
                            };

                            let is_writable = var.is_writable();
                            let is_connected = shared.connection_status == ConnectionStatus::Connected;
                            let can_edit = is_writable && is_connected;

                            let value_response = ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&value_text)
                                        .monospace()
                                        .size(14.0)
                                        .color(var_color)
                                )
                                .sense(if can_edit { egui::Sense::click() } else { egui::Sense::hover() })
                            );

                            if can_edit && value_response.double_clicked() {
                                state.value_editor_state = ValueEditorState::for_variable(var.id);
                                state.value_editor_state.input = format!("{}", point.raw_value);
                                state.value_editor_open = true;
                            }

                            if can_edit {
                                value_response.on_hover_text("Double-click to edit value");
                            }
                        } else {
                            ui.label(egui::RichText::new("—").color(Color32::GRAY));
                        }
                    } else {
                        ui.label(egui::RichText::new("—").color(Color32::GRAY));
                    }
                });
            });

            // Line 2: Type @ Address | Converter status
            ui.horizontal(|ui| {
                ui.add_space(32.0); // Indent to align with name (past color swatch)

                // Type and address (dimmed secondary info)
                let type_addr = format!("{} @ 0x{:08X}", var.var_type, var.address);
                ui.label(egui::RichText::new(type_addr).small().color(Color32::GRAY).monospace());

                ui.add_space(12.0);
                ui.label(egui::RichText::new("│").small().color(Color32::DARK_GRAY));
                ui.add_space(12.0);

                // Converter status (clickable)
                let has_converter = var.converter_script.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
                let converter_text = if has_converter { "ƒ Converter" } else { "No converter" };
                let converter_color = if has_converter { Color32::LIGHT_BLUE } else { Color32::DARK_GRAY };

                let converter_response = ui.add(
                    egui::Label::new(
                        egui::RichText::new(converter_text)
                            .small()
                            .color(converter_color)
                    )
                    .sense(egui::Sense::click())
                );

                if converter_response.clicked() {
                    let script = var.converter_script.as_deref().unwrap_or("").to_string();
                    *var_to_edit_converter = Some((var.id, script));
                }

                if has_converter {
                    converter_response.on_hover_text(format!("Click to edit: {}", var.converter_script.as_deref().unwrap_or("")));
                } else {
                    converter_response.on_hover_text("Click to add a converter script");
                }
            });
        });

        // Color picker popup (shown below the card when active)
        if state.color_picker_var_id == Some(var.id) {
            egui::Frame::popup(ui.style())
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Color:");
                        let mut srgba = Color32::from_rgba_unmultiplied(
                            state.color_picker_color[0],
                            state.color_picker_color[1],
                            state.color_picker_color[2],
                            state.color_picker_color[3],
                        );
                        if ui.color_edit_button_srgba(&mut srgba).changed() {
                            state.color_picker_color = srgba.to_array();
                        }
                    });

                    // Preset colors
                    ui.horizontal(|ui| {
                        let presets: &[([u8; 4], &str)] = &[
                            ([255, 0, 0, 255], "Red"),
                            ([0, 200, 0, 255], "Green"),
                            ([0, 100, 255, 255], "Blue"),
                            ([255, 200, 0, 255], "Yellow"),
                            ([0, 200, 200, 255], "Cyan"),
                            ([255, 0, 255, 255], "Magenta"),
                            ([255, 128, 0, 255], "Orange"),
                        ];

                        for (color, name) in presets {
                            let c = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
                            if ui.add(
                                egui::Button::new("")
                                    .fill(c)
                                    .min_size(egui::vec2(18.0, 18.0))
                            ).on_hover_text(*name).clicked() {
                                state.color_picker_color = *color;
                            }
                        }
                    });

                    ui.horizontal(|ui| {
                        if ui.button("Apply").clicked() {
                            *var_update_color = Some((var.id, state.color_picker_color));
                            state.color_picker_var_id = None;
                        }
                        if ui.button("Cancel").clicked() {
                            state.color_picker_var_id = None;
                        }
                    });
                });
        }
    }

    fn render_dialogs(
        state: &mut VariablesPageState,
        shared: &mut SharedState<'_>,
        ctx: &Context,
        actions: &mut Vec<AppAction>,
    ) {
        // Converter editor dialog
        if state.converter_editor_open {
            let var_id = state.converter_editor_state.var_id;
            if let Some(var_id) = var_id {
                let var_name = shared
                    .config
                    .find_variable(var_id)
                    .map(|v| v.name.clone())
                    .unwrap_or_else(|| "Variable".to_string());

                let title = format!("Converter: {}", var_name);
                let dialog_ctx = ConverterEditorContext {
                    var_name: &var_name,
                };

                if let Some(action) = show_dialog_with_title::<ConverterEditorDialog>(
                    ctx,
                    &title,
                    &mut state.converter_editor_open,
                    &mut state.converter_editor_state,
                    dialog_ctx,
                ) {
                    match action {
                        ConverterEditorAction::Save { var_id, script } => {
                            if let Some(var) =
                                shared.config.variables.iter_mut().find(|v| v.id == var_id)
                            {
                                var.converter_script = script;
                                actions.push(AppAction::UpdateVariable(var.clone()));
                            }
                        }
                    }
                }
            }
        }

        // Value editor dialog
        if state.value_editor_open {
            let var_id = state.value_editor_state.var_id;
            if let Some(var_id) = var_id {
                let (var_name, var_type, is_writable) = match shared.config.find_variable(var_id) {
                    Some(var) => (var.name.clone(), var.var_type, var.is_writable()),
                    None => {
                        state.value_editor_open = false;
                        return;
                    }
                };

                let current_value = shared
                    .variable_data
                    .get(&var_id)
                    .and_then(|d| d.last())
                    .map(|p| p.raw_value);

                let dialog_ctx = ValueEditorContext {
                    var_name: &var_name,
                    var_type,
                    is_writable,
                    connection_status: shared.connection_status,
                    current_value,
                };

                if let Some(action) = show_dialog::<ValueEditorDialog>(
                    ctx,
                    &mut state.value_editor_open,
                    &mut state.value_editor_state,
                    dialog_ctx,
                ) {
                    match action {
                        ValueEditorAction::Write { var_id, value } => {
                            actions.push(AppAction::WriteVariable { id: var_id, value });
                        }
                    }
                }
            }
        }

        // Variable detail dialog
        if state.variable_detail_open {
            let var_id = state.variable_detail_state.var_id;
            if let Some(var_id) = var_id {
                let (address, var_type, enabled, show_in_graph, current_value) =
                    match shared.config.find_variable(var_id) {
                        Some(var) => {
                            let value = shared
                                .variable_data
                                .get(&var_id)
                                .and_then(|d| d.last())
                                .map(|p| p.converted_value);
                            (
                                var.address,
                                var.var_type.to_string(),
                                var.enabled,
                                var.show_in_graph,
                                value,
                            )
                        }
                        None => {
                            state.variable_detail_open = false;
                            return;
                        }
                    };

                let dialog_ctx = VariableDetailContext {
                    address,
                    var_type,
                    enabled,
                    show_in_graph,
                    current_value,
                };

                if let Some(action) = show_dialog::<VariableDetailDialog>(
                    ctx,
                    &mut state.variable_detail_open,
                    &mut state.variable_detail_state,
                    dialog_ctx,
                ) {
                    match action {
                        VariableDetailAction::Save {
                            var_id,
                            name,
                            unit,
                            color,
                        } => {
                            if let Some(var) = shared.config.find_variable_mut(var_id) {
                                var.name = name.clone();
                                var.unit = unit.clone();
                                var.color = color;
                            }
                            if let Some(data) = shared.variable_data.get_mut(&var_id) {
                                data.variable.name = name;
                                data.variable.unit = unit;
                                data.variable.color = color;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Render diagnostics summary showing DWARF parsing statistics
    fn render_diagnostics_summary(ui: &mut Ui, diagnostics: &DwarfDiagnostics) {
        ui.label(format!("Total variables: {}", diagnostics.total_variables));

        let readable = diagnostics.with_valid_address + diagnostics.address_zero;
        ui.label(format!(
            "Readable: {} ({:.0}%)",
            readable,
            if diagnostics.total_variables > 0 {
                100.0 * readable as f64 / diagnostics.total_variables as f64
            } else {
                0.0
            }
        ));

        if diagnostics.optimized_out > 0 {
            ui.colored_label(
                Color32::YELLOW,
                format!("Optimized out: {}", diagnostics.optimized_out),
            )
            .on_hover_text("Variables removed by compiler optimization");
        }

        if diagnostics.register_only > 0 {
            ui.colored_label(
                Color32::LIGHT_BLUE,
                format!("Register-only: {}", diagnostics.register_only),
            )
            .on_hover_text("Values live in CPU registers without memory backing");
        }

        if diagnostics.implicit_value > 0 {
            ui.label(format!("Implicit values: {}", diagnostics.implicit_value))
                .on_hover_text("Computed values (DW_OP_stack_value)");
        }

        if diagnostics.multi_piece > 0 {
            ui.label(format!("Multi-piece: {}", diagnostics.multi_piece))
                .on_hover_text("Variables split across registers/memory");
        }

        if diagnostics.artificial > 0 {
            ui.colored_label(
                Color32::DARK_GRAY,
                format!("Compiler-generated: {}", diagnostics.artificial),
            )
            .on_hover_text("Artificial variables (this pointers, VLA bounds, etc.)");
        }

        if diagnostics.local_variables > 0 {
            ui.colored_label(
                Color32::GRAY,
                format!("Local variables: {}", diagnostics.local_variables),
            )
            .on_hover_text("Stack/register variables requiring runtime context");
        }

        if diagnostics.extern_declarations > 0 {
            ui.label(format!(
                "Extern declarations: {}",
                diagnostics.extern_declarations
            ))
            .on_hover_text("Variables declared but defined elsewhere");
        }

        if diagnostics.compile_time_constants > 0 {
            ui.label(format!(
                "Compile-time constants: {}",
                diagnostics.compile_time_constants
            ))
            .on_hover_text("Constants with no runtime storage");
        }

        if diagnostics.address_zero > 0 {
            ui.label(format!("Address 0: {}", diagnostics.address_zero))
                .on_hover_text("Variables at address 0 (may be valid on embedded targets)");
        }

        if diagnostics.implicit_pointer > 0 {
            ui.label(format!("Implicit pointers: {}", diagnostics.implicit_pointer))
                .on_hover_text("Pointers to optimized-out data");
        }

        if diagnostics.unresolved_types > 0 {
            ui.colored_label(
                Color32::RED,
                format!("Unresolved types: {}", diagnostics.unresolved_types),
            )
            .on_hover_text("Variables with missing type information");
        }
    }

    /// Render a type tree node for struct/array/pointer navigation
    #[allow(clippy::too_many_arguments)]
    fn render_type_tree(
        ui: &mut Ui,
        name: &str,
        address: u64,
        type_handle: Option<TypeHandle>,
        path: &str,
        indent_level: usize,
        expanded_paths: &HashSet<String>,
        is_selected: bool,
        toggle_expand_path: &mut Option<String>,
        variables_to_add: &mut Vec<Variable>,
        symbol_to_use: &mut Option<ElfSymbol>,
        root_symbol: Option<&ElfSymbol>,
    ) {
        let is_expanded = expanded_paths.contains(path);
        let type_name = type_handle
            .as_ref()
            .map(|h| h.type_name())
            .unwrap_or_else(|| "unknown".to_string());
        let size = type_handle.as_ref().and_then(|h| h.size()).unwrap_or(0);

        // Check if this type is expandable
        let can_expand = type_handle
            .as_ref()
            .map(|h| h.is_expandable())
            .unwrap_or(false);

        // Check if this type can be added as a variable
        let is_addable = type_handle.as_ref().map(|h| h.is_addable()).unwrap_or(true);

        // Get the underlying type handle for member access
        let underlying = type_handle.as_ref().map(|h| h.underlying());

        // Check if the underlying type is a pointer/reference
        let is_pointer = underlying
            .as_ref()
            .map(|h| h.is_pointer_or_reference())
            .unwrap_or(false);

        // Get members if this is a struct/union
        let members = underlying.as_ref().and_then(|h| {
            if let Some(members) = h.members() {
                return Some((members.to_vec(), h.clone()));
            }
            None
        });

        // Check if this is an array type
        let array_info = underlying.as_ref().and_then(|h| {
            if !h.is_pointer_or_reference() && h.is_array() {
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
                let expand_icon = if is_expanded { "v" } else { ">" };
                if ui.small_button(expand_icon).clicked() {
                    *toggle_expand_path = Some(path.to_string());
                }
            } else {
                ui.add_space(18.0);
            }

            // Format the display
            let display_text = if indent_level == 0 {
                format!(
                    "{} @ 0x{:08X} ({} bytes) - {}",
                    name, address, size, type_name
                )
            } else {
                let short_name = name.rsplit('.').next().unwrap_or(name);
                format!(".{}: {} @ 0x{:08X}", short_name, type_name, address)
            };

            let response = ui.selectable_label(is_selected && indent_level == 0, &display_text);

            if response.double_clicked() {
                if let Some(sym) = root_symbol {
                    *symbol_to_use = Some(sym.clone());
                }
            }

            // Add button for addable types
            if is_addable {
                let hover_text = if is_pointer {
                    "Add pointer value as variable"
                } else {
                    "Add as variable"
                };
                if ui.small_button("+").on_hover_text(hover_text).clicked() {
                    let var_type = type_handle
                        .as_ref()
                        .map(|h| h.to_variable_type())
                        .unwrap_or(crate::types::VariableType::U32);
                    let var = Variable::new(name, address, var_type);
                    variables_to_add.push(var);
                }
            }
        });

        // Render expanded members or array elements
        if is_expanded {
            // Handle struct/union members
            if let Some((member_list, parent_handle)) = members {
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

            // Handle array elements
            if let Some((count, elem_size, elem_type)) = array_info {
                for i in 0..count {
                    let elem_path = format!("{}[{}]", path, i);
                    let elem_addr = address + (i * elem_size);
                    let full_name = format!("{}[{}]", name, i);

                    Self::render_type_tree(
                        ui,
                        &full_name,
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

                // Show truncation warning
                if count == MAX_ARRAY_ELEMENTS {
                    let original_count = underlying.as_ref().and_then(|h| h.array_count()).unwrap_or(0);
                    if original_count > MAX_ARRAY_ELEMENTS {
                        ui.horizontal(|ui| {
                            ui.add_space(((indent_level + 1) * 20) as f32);
                            ui.colored_label(
                                Color32::YELLOW,
                                format!(
                                    "... {} more elements (truncated)",
                                    original_count - MAX_ARRAY_ELEMENTS
                                ),
                            );
                        });
                    }
                }
            }
        }
    }
}
