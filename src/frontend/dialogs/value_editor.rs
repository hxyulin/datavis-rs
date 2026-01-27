//! Value editor dialog for writing values to variables
//!
//! This dialog allows users to write new values to writable variables.

use super::{Dialog, DialogAction, DialogState, DialogWindowConfig};
use crate::types::{ConnectionStatus, VariableType};
use egui::{Color32, Ui};

/// State for the value editor dialog
#[derive(Debug, Default)]
pub struct ValueEditorState {
    /// The variable ID being edited
    pub var_id: Option<u32>,
    /// Input text for the new value
    pub input: String,
    /// Error message if parsing fails
    pub error: Option<String>,
}

impl DialogState for ValueEditorState {
    fn reset(&mut self) {
        self.var_id = None;
        self.input.clear();
        self.error = None;
    }

    fn is_valid(&self) -> bool {
        self.var_id.is_some()
    }
}

impl ValueEditorState {
    /// Create a new state for editing a variable
    pub fn for_variable(var_id: u32) -> Self {
        Self {
            var_id: Some(var_id),
            input: String::new(),
            error: None,
        }
    }
}

/// Action from the value editor dialog
#[derive(Debug, Clone)]
pub enum ValueEditorAction {
    /// Write a value to a variable
    Write { var_id: u32, value: f64 },
}

/// Context needed to render the value editor
pub struct ValueEditorContext<'a> {
    /// Variable name
    pub var_name: &'a str,
    /// Variable type
    pub var_type: VariableType,
    /// Whether the variable is writable
    pub is_writable: bool,
    /// Current connection status
    pub connection_status: ConnectionStatus,
    /// Current raw value (if available)
    pub current_value: Option<f64>,
}

/// The value editor dialog
pub struct ValueEditorDialog;

impl Dialog for ValueEditorDialog {
    type State = ValueEditorState;
    type Action = ValueEditorAction;
    type Context<'a> = ValueEditorContext<'a>;

    fn title(_state: &Self::State) -> &'static str {
        "Edit Value"
    }

    fn window_config() -> DialogWindowConfig {
        DialogWindowConfig {
            default_width: 300.0,
            default_height: None,
            resizable: false,
            collapsible: false,
            anchor: None,
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

        // Check if we can write
        let can_write =
            ctx.is_writable && ctx.connection_status == ConnectionStatus::Connected;

        ui.vertical(|ui| {
            // Variable name header
            ui.heading(ctx.var_name);

            ui.add_space(4.0);

            // Show variable info
            ui.horizontal(|ui| {
                ui.label("Type:");
                ui.label(ctx.var_type.to_string());
            });

            // Show current value
            if let Some(value) = ctx.current_value {
                ui.horizontal(|ui| {
                    ui.label("Current:");
                    ui.label(format!("{:.6}", value));
                });
            }

            ui.separator();

            // Value input
            let mut should_write = false;
            ui.horizontal(|ui| {
                ui.label("New value:");
                let response = ui.text_edit_singleline(&mut state.input);

                // Submit on Enter
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if can_write {
                        should_write = true;
                    }
                }
            });

            // Show error if any
            if let Some(ref error) = state.error {
                ui.colored_label(Color32::RED, error);
            }

            // Show warnings if not writable
            if !ctx.is_writable {
                if !ctx.var_type.is_writable() {
                    ui.colored_label(Color32::YELLOW, "Warning:Raw types cannot be written");
                } else {
                    ui.colored_label(
                        Color32::YELLOW,
                        "Warning:Variables with converters cannot be written",
                    );
                }
            }
            if ctx.connection_status != ConnectionStatus::Connected {
                ui.colored_label(Color32::YELLOW, "Warning:Not connected to probe");
            }

            ui.separator();

            // Buttons
            let mut should_close = false;
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

            // Process write action
            if should_write {
                match parse_value(&state.input) {
                    Ok(value) => {
                        return DialogAction::CloseWithAction(ValueEditorAction::Write {
                            var_id,
                            value,
                        });
                    }
                    Err(e) => {
                        state.error = Some(e);
                    }
                }
            }

            if should_close {
                return DialogAction::Close;
            }

            DialogAction::None
        })
        .inner
    }
}

/// Parse a value from the input string, supporting various formats
fn parse_value(input: &str) -> Result<f64, String> {
    let input = input.trim();

    // Try parsing as f64 first
    if let Ok(value) = input.parse::<f64>() {
        return Ok(value);
    }

    // Try parsing as hex
    if input.starts_with("0x") || input.starts_with("0X") {
        if let Ok(v) = u64::from_str_radix(&input[2..], 16) {
            return Ok(v as f64);
        }
    }

    // Try parsing as binary
    if input.starts_with("0b") || input.starts_with("0B") {
        if let Ok(v) = u64::from_str_radix(&input[2..], 2) {
            return Ok(v as f64);
        }
    }

    // Try parsing as integer
    if let Ok(v) = input.parse::<i64>() {
        return Ok(v as f64);
    }

    Err("Invalid number format".to_string())
}
