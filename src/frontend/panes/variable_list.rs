//! Variable List pane - Displays selected variables with controls
//!
//! Extracted from the center panel of the Variables page.

use egui::{Color32, Ui};

use crate::frontend::dialogs::{
    ConverterEditorState, ValueEditorState, VariableDetailState,
};
use crate::frontend::state::{AppAction, SharedState};
use crate::types::{ConnectionStatus, Variable};

/// State for the Variable List pane
pub struct VariableListState {
    /// Advanced mode shows all options (converter, plot style, graph toggle)
    pub advanced_mode: bool,
    // Dialog states
    pub converter_editor_open: bool,
    pub converter_editor_state: ConverterEditorState,
    pub value_editor_open: bool,
    pub value_editor_state: ValueEditorState,
    pub variable_detail_open: bool,
    pub variable_detail_state: VariableDetailState,
    // Inline color picker
    pub color_picker_var_id: Option<u32>,
    pub color_picker_color: [u8; 4],
}

impl Default for VariableListState {
    fn default() -> Self {
        Self {
            advanced_mode: false,
            converter_editor_open: false,
            converter_editor_state: ConverterEditorState::default(),
            value_editor_open: false,
            value_editor_state: ValueEditorState::default(),
            variable_detail_open: false,
            variable_detail_state: VariableDetailState::default(),
            color_picker_var_id: None,
            color_picker_color: [255, 255, 255, 255],
        }
    }
}

/// Render the variable list pane
pub fn render(
    state: &mut VariableListState,
    shared: &mut SharedState<'_>,
    ui: &mut Ui,
) -> Vec<AppAction> {
    let mut actions = Vec::new();

    // Deferred actions to avoid borrow issues
    let mut var_to_remove: Option<u32> = None;
    let mut var_to_edit_converter: Option<(u32, String)> = None;
    let mut var_to_open_detail: Option<u32> = None;
    let mut var_toggle_enabled: Option<(u32, bool)> = None;
    let mut var_toggle_graph: Option<(u32, bool)> = None;
    let mut var_cycle_plot_style: Option<u32> = None;
    let mut var_update_color: Option<(u32, [u8; 4])> = None;

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
            ui.label("Use the Variable Browser to add variables.");
        });
    } else {
        let advanced_mode = state.advanced_mode;
        egui::ScrollArea::vertical().show(ui, |ui| {
            let variables: Vec<_> = shared.config.variables.clone();
            for var in &variables {
                if advanced_mode {
                    render_variable_card_advanced(
                        state,
                        shared,
                        ui,
                        var,
                        &mut var_to_remove,
                        &mut var_to_edit_converter,
                        &mut var_to_open_detail,
                        &mut var_toggle_enabled,
                        &mut var_toggle_graph,
                        &mut var_cycle_plot_style,
                        &mut var_update_color,
                    );
                } else {
                    render_variable_card_simple(
                        ui,
                        var,
                        shared,
                        &mut var_to_remove,
                        &mut var_toggle_enabled,
                    );
                }
            }
        });
    }

    // Handle deferred actions
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

    actions
}

/// Render dialogs that belong to this pane (called from DataVisApp after dock rendering)
pub fn render_dialogs(
    state: &mut VariableListState,
    shared: &mut SharedState<'_>,
    ctx: &egui::Context,
    actions: &mut Vec<AppAction>,
) {
    use crate::frontend::dialogs::{
        show_dialog, show_dialog_with_title, ConverterEditorAction, ConverterEditorContext,
        ConverterEditorDialog, ValueEditorAction, ValueEditorContext, ValueEditorDialog,
        VariableDetailAction, VariableDetailContext, VariableDetailDialog,
    };

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

    let frame = egui::Frame::new()
        .fill(ui.visuals().widgets.noninteractive.bg_fill)
        .inner_margin(6.0)
        .outer_margin(2.0)
        .corner_radius(4.0);

    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 3.0, var_color);

            ui.label(&var.name);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("×").on_hover_text("Remove").clicked() {
                    *var_to_remove = Some(var.id);
                }

                let mut enabled = var.enabled;
                if ui
                    .checkbox(&mut enabled, "")
                    .on_hover_text(if enabled {
                        "Sampling (click to disable)"
                    } else {
                        "Not sampling (click to enable)"
                    })
                    .changed()
                {
                    *var_toggle_enabled = Some((var.id, enabled));
                }

                if let Some(data) = shared.variable_data.get(&var.id) {
                    if let Some(point) = data.last() {
                        let value_text = if var.unit.is_empty() {
                            format!("{:.3}", point.converted_value)
                        } else {
                            format!("{:.3} {}", point.converted_value, var.unit)
                        };
                        ui.label(
                            egui::RichText::new(value_text)
                                .monospace()
                                .color(var_color),
                        );
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

#[allow(clippy::too_many_arguments)]
fn render_variable_card_advanced(
    state: &mut VariableListState,
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

    let frame = egui::Frame::new()
        .fill(ui.visuals().widgets.noninteractive.bg_fill)
        .inner_margin(8.0)
        .outer_margin(2.0)
        .corner_radius(4.0);

    frame.show(ui, |ui| {
        // Line 1: [Color] Name | Value | [S] [G] [Style] [×]
        ui.horizontal(|ui| {
            let swatch_size = egui::vec2(24.0, 24.0);
            let (rect, swatch_response) =
                ui.allocate_exact_size(swatch_size, egui::Sense::click());
            ui.painter().rect_filled(rect, 4.0, var_color);
            ui.painter().rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(
                    1.0,
                    ui.visuals().widgets.noninteractive.fg_stroke.color,
                ),
                egui::StrokeKind::Outside,
            );

            if swatch_response.clicked() {
                if state.color_picker_var_id == Some(var.id) {
                    state.color_picker_var_id = None;
                } else {
                    state.color_picker_var_id = Some(var.id);
                    state.color_picker_color = var.color;
                }
            }
            swatch_response.on_hover_text("Click to change color");

            let name_response = ui.add(
                egui::Label::new(egui::RichText::new(&var.name).strong())
                    .sense(egui::Sense::click()),
            );
            if name_response.clicked() {
                *var_to_open_detail = Some(var.id);
            }
            name_response.on_hover_text("Click to edit details");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("×")
                    .on_hover_text("Remove variable")
                    .clicked()
                {
                    *var_to_remove = Some(var.id);
                }

                ui.add_space(4.0);

                let style_text = var.plot_style.display_name();
                let style_btn =
                    ui.add(egui::Button::new(egui::RichText::new(style_text).small()));
                if style_btn.clicked() {
                    *var_cycle_plot_style = Some(var.id);
                }
                style_btn.on_hover_text("Click to cycle plot style");

                let (graph_text, graph_color) = if var.show_in_graph {
                    ("Graph", ui.visuals().widgets.active.fg_stroke.color)
                } else {
                    ("Graph", Color32::GRAY)
                };
                let graph_btn = ui.add(egui::Button::new(
                    egui::RichText::new(graph_text).small().color(graph_color),
                ));
                if graph_btn.clicked() {
                    *var_toggle_graph = Some((var.id, !var.show_in_graph));
                }
                graph_btn.on_hover_text(if var.show_in_graph {
                    "Showing in graph (click to hide)"
                } else {
                    "Hidden from graph (click to show)"
                });

                let (sample_text, sample_color) = if var.enabled {
                    ("Sample", Color32::from_rgb(100, 200, 100))
                } else {
                    ("Sample", Color32::GRAY)
                };
                let sample_btn = ui.add(egui::Button::new(
                    egui::RichText::new(sample_text)
                        .small()
                        .color(sample_color),
                ));
                if sample_btn.clicked() {
                    *var_toggle_enabled = Some((var.id, !var.enabled));
                }
                sample_btn.on_hover_text(if var.enabled {
                    "Sampling enabled (click to disable)"
                } else {
                    "Sampling disabled (click to enable)"
                });

                ui.add_space(8.0);

                if let Some(data) = shared.variable_data.get(&var.id) {
                    if let Some(point) = data.last() {
                        let value_text = if var.unit.is_empty() {
                            format!("{:.3}", point.converted_value)
                        } else {
                            format!("{:.3} {}", point.converted_value, var.unit)
                        };

                        let is_writable = var.is_writable();
                        let is_connected =
                            shared.connection_status == ConnectionStatus::Connected;
                        let can_edit = is_writable && is_connected;

                        let value_response = ui.add(
                            egui::Label::new(
                                egui::RichText::new(&value_text)
                                    .monospace()
                                    .size(14.0)
                                    .color(var_color),
                            )
                            .sense(if can_edit {
                                egui::Sense::click()
                            } else {
                                egui::Sense::hover()
                            }),
                        );

                        if can_edit && value_response.double_clicked() {
                            state.value_editor_state =
                                ValueEditorState::for_variable(var.id);
                            state.value_editor_state.input =
                                format!("{}", point.raw_value);
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
            ui.add_space(32.0);

            let type_addr = format!("{} @ 0x{:08X}", var.var_type, var.address);
            ui.label(
                egui::RichText::new(type_addr)
                    .small()
                    .color(Color32::GRAY)
                    .monospace(),
            );

            ui.add_space(12.0);
            ui.label(egui::RichText::new("│").small().color(Color32::DARK_GRAY));
            ui.add_space(12.0);

            let has_converter = var
                .converter_script
                .as_ref()
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            let converter_text = if has_converter {
                "ƒ Converter"
            } else {
                "No converter"
            };
            let converter_color = if has_converter {
                Color32::LIGHT_BLUE
            } else {
                Color32::DARK_GRAY
            };

            let converter_response = ui.add(
                egui::Label::new(
                    egui::RichText::new(converter_text)
                        .small()
                        .color(converter_color),
                )
                .sense(egui::Sense::click()),
            );

            if converter_response.clicked() {
                let script = var.converter_script.as_deref().unwrap_or("").to_string();
                *var_to_edit_converter = Some((var.id, script));
            }

            if has_converter {
                converter_response.on_hover_text(format!(
                    "Click to edit: {}",
                    var.converter_script.as_deref().unwrap_or("")
                ));
            } else {
                converter_response.on_hover_text("Click to add a converter script");
            }
        });
    });

    // Color picker popup
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
                        let c = Color32::from_rgba_unmultiplied(
                            color[0], color[1], color[2], color[3],
                        );
                        if ui
                            .add(
                                egui::Button::new("")
                                    .fill(c)
                                    .min_size(egui::vec2(18.0, 18.0)),
                            )
                            .on_hover_text(*name)
                            .clicked()
                        {
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
