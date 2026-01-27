//! Page modules for the frontend
//!
//! Each page implements the Page trait, receiving shared state
//! via context and returning actions instead of mutating directly.
//!
//! This design enables:
//! - Clear dependency injection through `SharedState`
//! - Testable page logic (pages are pure functions of state)
//! - Decoupled architecture (pages don't know about each other)
//! - Centralized action handling in the main app

mod settings;
mod variables;
mod visualizer;

pub use settings::{SettingsPage, SettingsPageState};
pub use variables::{VariablesPage, VariablesPageState};
pub use visualizer::{VisualizerPage, VisualizerPageState};

use crate::frontend::state::{AppAction, SharedState};
use egui::Context;

/// Trait for page components
///
/// Pages receive shared state via `SharedState` and return actions
/// instead of mutating the main app directly. This pattern is similar
/// to the Dialog trait but for full-page components.
///
/// # Example
///
/// ```ignore
/// pub struct MyPageState {
///     selection: Option<u32>,
/// }
///
/// impl Default for MyPageState {
///     fn default() -> Self {
///         Self { selection: None }
///     }
/// }
///
/// pub struct MyPage;
///
/// impl Page for MyPage {
///     type State = MyPageState;
///
///     fn render(
///         state: &mut Self::State,
///         shared: &mut SharedState<'_>,
///         ctx: &Context,
///     ) -> Vec<AppAction> {
///         let mut actions = Vec::new();
///
///         egui::CentralPanel::default().show(ctx, |ui| {
///             if ui.button("Do something").clicked() {
///                 actions.push(AppAction::SomeAction);
///             }
///         });
///
///         actions
///     }
/// }
/// ```
pub trait Page {
    /// Page-specific state (dialogs, selections, UI state)
    ///
    /// This state is owned by the main app and passed to the page
    /// during rendering. It persists across frames.
    type State: Default;

    /// Render the page and return any actions to perform
    ///
    /// This method receives:
    /// - `state`: Mutable reference to page-specific state
    /// - `shared`: Mutable reference to shared application state
    /// - `ctx`: The egui context for rendering
    ///
    /// Returns a vector of actions that the main app should handle.
    /// Actions are processed after the page finishes rendering.
    fn render(
        state: &mut Self::State,
        shared: &mut SharedState<'_>,
        ctx: &Context,
    ) -> Vec<AppAction>;
}
