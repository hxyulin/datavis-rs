//! Variable Change Detection Dialog
//!
//! Dialog for handling detected changes in variables when ELF file is reloaded.

use egui::{Color32, Ui};

use super::{Dialog, DialogAction, DialogState, DialogWindowConfig};
use crate::frontend::{VariableChange, VariableChangeType};

/// State for the variable change dialog
///
/// Note: This dialog manages its own `changes` list with selection state,
/// so the state is more complex than typical dialogs.
#[derive(Debug, Default)]
pub struct VariableChangeState {
    /// List of detected changes with selection state
    pub changes: Vec<VariableChange>,
}

impl DialogState for VariableChangeState {
    fn reset(&mut self) {
        self.changes.clear();
    }

    fn is_valid(&self) -> bool {
        !self.changes.is_empty()
    }
}

impl VariableChangeState {
    /// Initialize state with a list of changes
    pub fn with_changes(changes: Vec<VariableChange>) -> Self {
        Self { changes }
    }

    /// Get count of selected changes
    pub fn selected_count(&self) -> usize {
        self.changes.iter().filter(|c| c.selected).count()
    }

    /// Select all changes
    pub fn select_all(&mut self) {
        for change in &mut self.changes {
            change.selected = true;
        }
    }

    /// Deselect all changes
    pub fn deselect_all(&mut self) {
        for change in &mut self.changes {
            change.selected = false;
        }
    }
}

/// Actions that can be returned by the variable change dialog
#[derive(Debug, Clone)]
pub enum VariableChangeAction {
    /// Apply all selected changes
    UpdateSelected,
    /// Apply all changes (selects all first)
    UpdateAll,
}

/// Context needed to render the variable change dialog (none needed)
pub struct VariableChangeContext;

/// The variable change dialog
pub struct VariableChangeDialog;

impl Dialog for VariableChangeDialog {
    type State = VariableChangeState;
    type Action = VariableChangeAction;
    type Context<'a> = VariableChangeContext;

    fn title(_state: &Self::State) -> &'static str {
        "Variable Changes Detected"
    }

    fn window_config() -> DialogWindowConfig {
        DialogWindowConfig {
            default_width: 550.0,
            default_height: Some(400.0),
            resizable: true,
            collapsible: false,
            anchor: Some((egui::Align2::CENTER_CENTER, [0.0, 0.0])),
            modal: false,
        }
    }

    fn render(
        state: &mut Self::State,
        _ctx: Self::Context<'_>,
        ui: &mut Ui,
    ) -> DialogAction<Self::Action> {
        let mut action = DialogAction::None;

        let total = state.changes.len();
        let selected = state.selected_count();

        ui.label("The following variables have changed since the last ELF load:");
        ui.add_space(8.0);

        // Summary counts
        let address_changes = state
            .changes
            .iter()
            .filter(|c| matches!(c.change_type, VariableChangeType::AddressChanged { .. }))
            .count();
        let type_changes = state
            .changes
            .iter()
            .filter(|c| matches!(c.change_type, VariableChangeType::TypeChanged { .. }))
            .count();
        let not_found = state
            .changes
            .iter()
            .filter(|c| matches!(c.change_type, VariableChangeType::NotFound))
            .count();

        ui.horizontal(|ui| {
            ui.label(format!("{} changes detected:", total));
            if address_changes > 0 {
                ui.colored_label(Color32::YELLOW, format!("{} address", address_changes));
            }
            if type_changes > 0 {
                ui.colored_label(Color32::LIGHT_BLUE, format!("{} type", type_changes));
            }
            if not_found > 0 {
                ui.colored_label(Color32::RED, format!("{} missing", not_found));
            }
        });

        ui.separator();

        // Selection controls
        ui.horizontal(|ui| {
            if ui.button("Select All").clicked() {
                state.select_all();
            }
            if ui.button("Deselect All").clicked() {
                state.deselect_all();
            }
            ui.label(format!("{} of {} selected", selected, total));
        });

        ui.separator();

        // Changes list with scroll area
        egui::ScrollArea::vertical()
            .max_height(250.0)
            .show(ui, |ui| {
                for change in &mut state.changes {
                    ui.horizontal(|ui| {
                        // Checkbox for selection
                        ui.checkbox(&mut change.selected, "");

                        // Change type indicator and color
                        let (indicator, color) = match &change.change_type {
                            VariableChangeType::AddressChanged { .. } => ("ADDR", Color32::YELLOW),
                            VariableChangeType::TypeChanged { .. } => ("TYPE", Color32::LIGHT_BLUE),
                            VariableChangeType::NotFound => ("MISS", Color32::RED),
                        };
                        ui.colored_label(color, format!("[{}]", indicator));

                        // Variable name
                        ui.strong(&change.variable_name);

                        // Change details
                        match &change.change_type {
                            VariableChangeType::AddressChanged {
                                old_address,
                                new_address,
                            } => {
                                ui.label(format!(
                                    "0x{:08X} -> 0x{:08X}",
                                    old_address, new_address
                                ));
                            }
                            VariableChangeType::TypeChanged {
                                old_type,
                                new_type,
                                new_type_name,
                            } => {
                                ui.label(format!("{} -> {} ({})", old_type, new_type, new_type_name));
                            }
                            VariableChangeType::NotFound => {
                                ui.label("Not found in ELF");
                            }
                        }
                    });
                    ui.add_space(2.0);
                }
            });

        ui.separator();

        // Action buttons
        ui.horizontal(|ui| {
            // Update Selected button
            let update_enabled = selected > 0;
            if ui
                .add_enabled(
                    update_enabled,
                    egui::Button::new(format!("Update Selected ({})", selected)),
                )
                .clicked()
            {
                action = DialogAction::CloseWithAction(VariableChangeAction::UpdateSelected);
            }

            // Update All button
            if ui.button(format!("Update All ({})", total)).clicked() {
                action = DialogAction::CloseWithAction(VariableChangeAction::UpdateAll);
            }

            // Discard All button
            if ui.button("Discard All").clicked() {
                action = DialogAction::Close;
            }
        });

        // Help text for missing variables
        if not_found > 0 {
            ui.add_space(4.0);
            ui.small("Note: Selecting 'missing' variables will remove them from the watch list.");
        }

        action
    }
}
