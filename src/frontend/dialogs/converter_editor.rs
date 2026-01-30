//! Converter Editor Dialog
//!
//! Dialog for editing Rhai converter scripts for variables.

use egui::Ui;

use super::{Dialog, DialogAction, DialogState, DialogWindowConfig};
use crate::frontend::script_editor::{ScriptEditor, ScriptEditorState};

/// State for the converter editor dialog
#[derive(Default)]
pub struct ConverterEditorState {
    /// The variable ID being edited
    pub var_id: Option<u32>,
    /// The converter script being edited
    pub script: String,
    /// State for the script editor widget
    pub editor_state: ScriptEditorState,
}

impl DialogState for ConverterEditorState {
    fn reset(&mut self) {
        self.var_id = None;
        self.script.clear();
        self.editor_state = ScriptEditorState::default();
    }

    fn is_valid(&self) -> bool {
        self.var_id.is_some()
    }
}

impl ConverterEditorState {
    /// Initialize the state for editing a variable's converter script
    pub fn edit(var_id: u32, script: String) -> Self {
        Self {
            var_id: Some(var_id),
            script,
            editor_state: ScriptEditorState::default(),
        }
    }
}

/// Actions that can be returned by the converter editor dialog
#[derive(Debug, Clone)]
pub enum ConverterEditorAction {
    /// Save the converter script for the variable
    Save {
        /// The variable ID
        var_id: u32,
        /// The script (None means clear the script)
        script: Option<String>,
    },
}

/// Context needed to render the converter editor dialog
pub struct ConverterEditorContext<'a> {
    /// The variable name (for the window title)
    pub var_name: &'a str,
}

/// The converter editor dialog
pub struct ConverterEditorDialog;

impl Dialog for ConverterEditorDialog {
    type State = ConverterEditorState;
    type Action = ConverterEditorAction;
    type Context<'a> = ConverterEditorContext<'a>;

    fn title(_state: &Self::State) -> &'static str {
        // We use show_dialog_with_title for dynamic title
        "Converter Editor"
    }

    fn window_config() -> DialogWindowConfig {
        DialogWindowConfig::resizable(550.0, 350.0)
    }

    fn render(
        state: &mut Self::State,
        _ctx: Self::Context<'_>,
        ui: &mut Ui,
    ) -> DialogAction<Self::Action> {
        let mut action = DialogAction::None;

        ui.vertical(|ui| {
            // Script editor
            ScriptEditor::new(
                &mut state.script,
                &mut state.editor_state,
                "converter_editor_dialog",
            )
            .show(ui);

            ui.separator();

            // Buttons
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    if let Some(var_id) = state.var_id {
                        let script = if state.script.trim().is_empty() {
                            None
                        } else {
                            Some(state.script.clone())
                        };
                        action = DialogAction::CloseWithAction(ConverterEditorAction::Save {
                            var_id,
                            script,
                        });
                    }
                }
                if ui.button("Cancel").clicked() {
                    action = DialogAction::Close;
                }
                if ui.button("Clear").clicked() {
                    state.script.clear();
                }
            });
        });

        action
    }
}
