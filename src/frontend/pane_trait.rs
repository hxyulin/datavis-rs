//! Pane trait — polymorphic interface for all pane state types.
//!
//! Each pane state type implements `Pane`, and dispatch is via vtable.

use std::any::Any;

use egui::{Context, Ui};

use crate::frontend::state::{AppAction, SharedState};
use crate::frontend::workspace::PaneKind;

/// Trait implemented by all pane state types.
///
/// Dispatch is polymorphic via vtable.
pub trait Pane: Any {
    /// Pane kind identifier.
    fn kind(&self) -> PaneKind;

    /// Render the pane UI. Returns actions for the app to handle.
    fn render(&mut self, shared: &mut SharedState, ui: &mut Ui) -> Vec<AppAction>;

    /// Render modal dialogs owned by this pane (called after dock rendering).
    fn render_dialogs(
        &mut self,
        _shared: &mut SharedState,
        _ctx: &Context,
    ) -> Vec<AppAction> {
        Vec::new()
    }

    /// Downcast support.
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
