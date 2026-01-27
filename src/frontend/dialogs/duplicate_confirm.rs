//! Duplicate variable confirmation dialog
//!
//! This dialog is shown when the user attempts to add a variable
//! that already exists at the same address.

use super::{Dialog, DialogAction, DialogState, DialogWindowConfig};
use crate::types::Variable;
use egui::{Align2, Ui};

/// State for the duplicate confirmation dialog
#[derive(Debug, Default)]
pub struct DuplicateConfirmState {
    /// The variable that would be added
    pub pending_variable: Option<Variable>,
}

impl DialogState for DuplicateConfirmState {
    fn reset(&mut self) {
        self.pending_variable = None;
    }

    fn is_valid(&self) -> bool {
        self.pending_variable.is_some()
    }
}

impl DuplicateConfirmState {
    /// Create a new state with the pending variable
    pub fn with_variable(variable: Variable) -> Self {
        Self {
            pending_variable: Some(variable),
        }
    }

    /// Get the pending variable's address for display
    pub fn address(&self) -> Option<u64> {
        self.pending_variable.as_ref().map(|v| v.address)
    }
}

/// Action from the duplicate confirmation dialog
#[derive(Debug, Clone)]
pub enum DuplicateConfirmAction {
    /// User confirmed adding the duplicate variable
    Confirm(Variable),
}

/// Context for rendering (none needed for this simple dialog)
pub struct DuplicateConfirmContext;

/// The duplicate confirmation dialog
pub struct DuplicateConfirmDialog;

impl Dialog for DuplicateConfirmDialog {
    type State = DuplicateConfirmState;
    type Action = DuplicateConfirmAction;
    type Context<'a> = DuplicateConfirmContext;

    fn title(_state: &Self::State) -> &'static str {
        "Duplicate Variable"
    }

    fn window_config() -> DialogWindowConfig {
        DialogWindowConfig {
            default_width: 350.0,
            default_height: None,
            resizable: false,
            collapsible: false,
            anchor: Some((Align2::CENTER_CENTER, [0.0, 0.0])),
            modal: true,
        }
    }

    fn render(
        state: &mut Self::State,
        _ctx: Self::Context<'_>,
        ui: &mut Ui,
    ) -> DialogAction<Self::Action> {
        // If no pending variable, close immediately
        let address = match state.address() {
            Some(addr) => addr,
            None => return DialogAction::Close,
        };

        ui.label(format!(
            "A variable at address 0x{:08X} already exists.",
            address
        ));
        ui.label("Do you want to add it anyway?");

        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("Add Anyway").clicked() {
                if let Some(var) = state.pending_variable.take() {
                    return DialogAction::CloseWithAction(DuplicateConfirmAction::Confirm(var));
                }
            }
            if ui.button("Cancel").clicked() {
                return DialogAction::Close;
            }
            DialogAction::None
        })
        .inner
    }
}
