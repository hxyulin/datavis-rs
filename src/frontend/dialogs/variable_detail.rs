//! Variable detail dialog for viewing and editing variable properties
//!
//! This dialog shows detailed information about a variable and allows editing
//! the name, unit, and color.

use super::{Dialog, DialogAction, DialogState, DialogWindowConfig};
use egui::{Align2, Color32, Ui};

/// State for the variable detail dialog
#[derive(Debug, Default)]
pub struct VariableDetailState {
    /// The variable ID being viewed
    pub var_id: Option<u32>,
    /// Editable name
    pub name: String,
    /// Editable unit
    pub unit: String,
    /// Editable color
    pub color: [u8; 4],
}

impl DialogState for VariableDetailState {
    fn reset(&mut self) {
        self.var_id = None;
        self.name.clear();
        self.unit.clear();
        self.color = [0, 0, 0, 255];
    }

    fn is_valid(&self) -> bool {
        self.var_id.is_some() && !self.name.is_empty()
    }
}

impl VariableDetailState {
    /// Create a new state for viewing a variable
    pub fn for_variable(var_id: u32, name: &str, unit: &str, color: [u8; 4]) -> Self {
        Self {
            var_id: Some(var_id),
            name: name.to_string(),
            unit: unit.to_string(),
            color,
        }
    }
}

/// Action from the variable detail dialog
#[derive(Debug, Clone)]
pub enum VariableDetailAction {
    /// Save the edited variable properties
    Save {
        var_id: u32,
        name: String,
        unit: String,
        color: [u8; 4],
    },
}

/// Context needed to render the variable detail dialog
pub struct VariableDetailContext {
    /// Variable address
    pub address: u64,
    /// Variable type as string
    pub var_type: String,
    /// Whether sampling is enabled
    pub enabled: bool,
    /// Whether shown in graph
    pub show_in_graph: bool,
    /// Current converted value (if available)
    pub current_value: Option<f64>,
}

/// The variable detail dialog
pub struct VariableDetailDialog;

impl Dialog for VariableDetailDialog {
    type State = VariableDetailState;
    type Action = VariableDetailAction;
    type Context<'a> = VariableDetailContext;

    fn title(_state: &Self::State) -> &'static str {
        "Variable Details"
    }

    fn window_config() -> DialogWindowConfig {
        DialogWindowConfig {
            default_width: 400.0,
            default_height: None,
            resizable: true,
            collapsible: false,
            anchor: Some((Align2::CENTER_CENTER, [0.0, 0.0])),
            modal: false,
        }
    }

    fn render(
        state: &mut Self::State,
        ctx: Self::Context<'_>,
        ui: &mut Ui,
    ) -> DialogAction<Self::Action> {
        let var_id = match state.var_id {
            Some(id) => id,
            None => return DialogAction::Close,
        };

        egui::Grid::new("variable_detail_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Name (editable)
                ui.label("Name:");
                ui.text_edit_singleline(&mut state.name);
                ui.end_row();

                // Address (read-only)
                ui.label("Address:");
                ui.label(egui::RichText::new(format!("0x{:08X}", ctx.address)).monospace());
                ui.end_row();

                // Type (read-only)
                ui.label("Type:");
                ui.label(&ctx.var_type);
                ui.end_row();

                // Unit (editable)
                ui.label("Unit:");
                ui.text_edit_singleline(&mut state.unit);
                ui.end_row();

                // Current value (read-only)
                ui.label("Current Value:");
                if let Some(val) = ctx.current_value {
                    let value_text = if state.unit.is_empty() {
                        format!("{:.6}", val)
                    } else {
                        format!("{:.6} {}", val, state.unit)
                    };
                    ui.label(egui::RichText::new(value_text).monospace());
                } else {
                    ui.colored_label(Color32::GRAY, "No data");
                }
                ui.end_row();

                // Sampling enabled
                ui.label("Sampling:");
                ui.label(if ctx.enabled {
                    "✓ Enabled"
                } else {
                    "✗ Disabled"
                });
                ui.end_row();

                // Show in graph
                ui.label("Show in Graph:");
                ui.label(if ctx.show_in_graph { "✓ Yes" } else { "✗ No" });
                ui.end_row();
            });

        ui.separator();

        // Color picker section
        ui.horizontal(|ui| {
            ui.label("Plot Color:");

            // Color preview swatch
            let color = Color32::from_rgba_unmultiplied(
                state.color[0],
                state.color[1],
                state.color[2],
                state.color[3],
            );
            let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 4.0, color);

            // Color picker button
            let mut srgba = Color32::from_rgba_unmultiplied(
                state.color[0],
                state.color[1],
                state.color[2],
                state.color[3],
            );
            if ui.color_edit_button_srgba(&mut srgba).changed() {
                state.color = srgba.to_array();
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

            for (name, preset_color) in presets {
                let c = Color32::from_rgba_unmultiplied(
                    preset_color[0],
                    preset_color[1],
                    preset_color[2],
                    preset_color[3],
                );
                if ui
                    .add(
                        egui::Button::new("")
                            .fill(c)
                            .min_size(egui::vec2(20.0, 20.0)),
                    )
                    .on_hover_text(name)
                    .clicked()
                {
                    state.color = preset_color;
                }
            }
        });

        ui.separator();

        // Action buttons
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                return DialogAction::CloseWithAction(VariableDetailAction::Save {
                    var_id,
                    name: state.name.clone(),
                    unit: state.unit.clone(),
                    color: state.color,
                });
            }
            if ui.button("Cancel").clicked() {
                return DialogAction::Close;
            }
            DialogAction::None
        })
        .inner
    }
}
