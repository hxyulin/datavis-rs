//! Dialog trait system for unified dialog management
//!
//! This module provides a generic trait-based system for dialogs in the application.
//! Each dialog implements the `Dialog` trait, encapsulating its state, actions, and rendering.

use egui::{Align2, Context, Ui};

/// Actions that a dialog can return after rendering
#[derive(Debug, Clone, Default)]
pub enum DialogAction<A> {
    /// Keep the dialog open, no action needed
    #[default]
    None,
    /// Close the dialog without performing any action
    Close,
    /// Close the dialog and perform the specified action
    CloseWithAction(A),
    /// Keep the dialog open but perform the specified action
    Action(A),
}

impl<A> DialogAction<A> {
    /// Check if the action indicates the dialog should close
    pub fn should_close(&self) -> bool {
        matches!(self, DialogAction::Close | DialogAction::CloseWithAction(_))
    }

    /// Extract the action if present
    pub fn into_action(self) -> Option<A> {
        match self {
            DialogAction::CloseWithAction(a) | DialogAction::Action(a) => Some(a),
            _ => None,
        }
    }
}

/// Trait for dialog state management
///
/// Dialog state structs should implement this trait to enable
/// proper lifecycle management (reset on close, validation, etc.)
pub trait DialogState: Default {
    /// Reset the dialog state to its default values
    fn reset(&mut self) {
        *self = Self::default();
    }

    /// Check if the dialog has valid data to proceed with its action
    fn is_valid(&self) -> bool {
        true
    }
}

/// Configuration for dialog window appearance and behavior
#[derive(Debug, Clone)]
pub struct DialogWindowConfig {
    /// Default width of the dialog window
    pub default_width: f32,
    /// Default height of the dialog window (None for auto)
    pub default_height: Option<f32>,
    /// Whether the dialog can be resized
    pub resizable: bool,
    /// Whether the dialog can be collapsed
    pub collapsible: bool,
    /// Optional anchor position (alignment and offset)
    pub anchor: Option<(Align2, [f32; 2])>,
    /// Whether the dialog should be modal (dim background)
    pub modal: bool,
}

impl Default for DialogWindowConfig {
    fn default() -> Self {
        Self {
            default_width: 400.0,
            default_height: None,
            resizable: true,
            collapsible: false,
            anchor: None,
            modal: false,
        }
    }
}

impl DialogWindowConfig {
    /// Create a centered modal dialog configuration
    pub fn centered_modal(width: f32) -> Self {
        Self {
            default_width: width,
            default_height: None,
            resizable: false,
            collapsible: false,
            anchor: Some((Align2::CENTER_CENTER, [0.0, 0.0])),
            modal: true,
        }
    }

    /// Create a resizable dialog with specified size
    pub fn resizable(width: f32, height: f32) -> Self {
        Self {
            default_width: width,
            default_height: Some(height),
            resizable: true,
            collapsible: false,
            anchor: None,
            modal: false,
        }
    }
}

/// Main dialog trait for implementing dialogs
///
/// Each dialog in the application should implement this trait.
/// The trait uses associated types for type-safe state, actions, and context.
///
/// # Example
///
/// ```ignore
/// pub struct MyDialogState { /* ... */ }
/// impl DialogState for MyDialogState { /* ... */ }
///
/// pub enum MyDialogAction { Save, Cancel }
///
/// pub struct MyDialog;
///
/// impl Dialog for MyDialog {
///     type State = MyDialogState;
///     type Action = MyDialogAction;
///     type Context<'a> = &'a SomeData;
///
///     fn title(_state: &Self::State) -> &'static str { "My Dialog" }
///
///     fn render(
///         state: &mut Self::State,
///         ctx: Self::Context<'_>,
///         ui: &mut Ui,
///     ) -> DialogAction<Self::Action> {
///         // Render dialog content...
///         DialogAction::None
///     }
/// }
/// ```
pub trait Dialog {
    /// The state type for this dialog
    type State: DialogState;

    /// The action type this dialog can produce
    type Action;

    /// The context type needed to render this dialog
    type Context<'a>;

    /// Get the window title for this dialog
    fn title(state: &Self::State) -> &'static str;

    /// Get the window configuration for this dialog
    fn window_config() -> DialogWindowConfig {
        DialogWindowConfig::default()
    }

    /// Render the dialog content
    ///
    /// This method should render the dialog's UI and return an action
    /// indicating what should happen (close, perform action, etc.)
    fn render(
        state: &mut Self::State,
        ctx: Self::Context<'_>,
        ui: &mut Ui,
    ) -> DialogAction<Self::Action>;
}

/// Show a dialog using the Dialog trait
///
/// This helper function handles the common dialog lifecycle:
/// - Only renders if `is_open` is true
/// - Creates the window with the dialog's configuration
/// - Calls the dialog's render method
/// - Handles closing and state reset
///
/// Returns `Some(action)` if the dialog produced an action, `None` otherwise.
///
/// # Example
///
/// ```ignore
/// if let Some(action) = show_dialog::<MyDialog>(
///     ctx,
///     &mut self.my_dialog_open,
///     &mut self.my_dialog_state,
///     &my_context_data,
/// ) {
///     match action {
///         MyDialogAction::Save => { /* handle save */ }
///         MyDialogAction::Cancel => { /* handle cancel */ }
///     }
/// }
/// ```
pub fn show_dialog<D: Dialog>(
    ctx: &Context,
    is_open: &mut bool,
    state: &mut D::State,
    dialog_ctx: D::Context<'_>,
) -> Option<D::Action> {
    if !*is_open {
        return None;
    }

    let config = D::window_config();
    let mut action_result: Option<D::Action> = None;
    let mut should_close = false;

    // Build the window
    let mut window = egui::Window::new(D::title(state))
        .collapsible(config.collapsible)
        .resizable(config.resizable)
        .default_width(config.default_width);

    if let Some(height) = config.default_height {
        window = window.default_height(height);
    }

    if let Some((align, offset)) = config.anchor {
        window = window.anchor(align, offset);
    }

    // Show the window
    window.show(ctx, |ui| {
        let action = D::render(state, dialog_ctx, ui);

        match action {
            DialogAction::None => {}
            DialogAction::Close => {
                should_close = true;
            }
            DialogAction::CloseWithAction(a) => {
                should_close = true;
                action_result = Some(a);
            }
            DialogAction::Action(a) => {
                action_result = Some(a);
            }
        }
    });

    // Handle closing
    if should_close {
        *is_open = false;
        state.reset();
    }

    action_result
}

/// A variant of show_dialog that takes a dynamic title
///
/// Useful when the title depends on runtime data that can't be known
/// at compile time from the state alone.
pub fn show_dialog_with_title<D: Dialog>(
    ctx: &Context,
    title: &str,
    is_open: &mut bool,
    state: &mut D::State,
    dialog_ctx: D::Context<'_>,
) -> Option<D::Action> {
    if !*is_open {
        return None;
    }

    let config = D::window_config();
    let mut action_result: Option<D::Action> = None;
    let mut should_close = false;

    let mut window = egui::Window::new(title)
        .collapsible(config.collapsible)
        .resizable(config.resizable)
        .default_width(config.default_width);

    if let Some(height) = config.default_height {
        window = window.default_height(height);
    }

    if let Some((align, offset)) = config.anchor {
        window = window.anchor(align, offset);
    }

    window.show(ctx, |ui| {
        let action = D::render(state, dialog_ctx, ui);

        match action {
            DialogAction::None => {}
            DialogAction::Close => {
                should_close = true;
            }
            DialogAction::CloseWithAction(a) => {
                should_close = true;
                action_result = Some(a);
            }
            DialogAction::Action(a) => {
                action_result = Some(a);
            }
        }
    });

    if should_close {
        *is_open = false;
        state.reset();
    }

    action_result
}

// Re-export dialog implementations
pub mod collection_settings;
pub mod connection_settings;
pub mod converter_editor;
pub mod duplicate_confirm;
pub mod elf_symbols;
pub mod export_config;
pub mod persistence_settings;
pub mod preferences;
pub mod trigger_config;
pub mod value_editor;
pub mod variable_change;
pub mod variable_detail;

pub use collection_settings::{
    CollectionSettingsAction, CollectionSettingsContext, CollectionSettingsDialog,
    CollectionSettingsState,
};
pub use connection_settings::{
    ConnectionSettingsAction, ConnectionSettingsContext, ConnectionSettingsDialog,
    ConnectionSettingsState,
};
pub use converter_editor::{
    ConverterEditorAction, ConverterEditorContext, ConverterEditorDialog, ConverterEditorState,
};
pub use duplicate_confirm::{
    DuplicateConfirmAction, DuplicateConfirmContext, DuplicateConfirmDialog, DuplicateConfirmState,
};
pub use elf_symbols::{ElfSymbolsAction, ElfSymbolsContext, ElfSymbolsDialog, ElfSymbolsState};
pub use export_config::{
    DownsampleMode, ExportConfigAction, ExportConfigContext, ExportConfigDialog, ExportConfigState,
    ExportFormat,
};
pub use persistence_settings::{
    PersistenceSettingsAction, PersistenceSettingsContext, PersistenceSettingsDialog,
    PersistenceSettingsState,
};
pub use preferences::{PreferencesAction, PreferencesContext, PreferencesDialog, PreferencesState};
pub use trigger_config::{
    TriggerConfigAction, TriggerConfigContext, TriggerConfigDialog, TriggerConfigState,
};
pub use value_editor::{
    ValueEditorAction, ValueEditorContext, ValueEditorDialog, ValueEditorState,
};
pub use variable_change::{
    VariableChangeAction, VariableChangeContext, VariableChangeDialog, VariableChangeState,
};
pub use variable_detail::{
    VariableDetailAction, VariableDetailContext, VariableDetailDialog, VariableDetailState,
};
