//! Preferences dialog
//!
//! App-wide settings: font scale, dark mode, language, display defaults.
//! Covers settings.rs display section + app-wide preferences.

use egui::Ui;
use rust_i18n::t;

use crate::config::{UiConfig, UiPreferences};
use crate::frontend::dialogs::{Dialog, DialogAction, DialogState, DialogWindowConfig};
use crate::i18n::Language;

/// State for the preferences dialog
#[derive(Debug, Clone)]
pub struct PreferencesState {
    // App-wide (from UiPreferences)
    pub dark_mode: bool,
    pub font_scale: f32,
    pub language: Language,

    // Display defaults (from UiConfig)
    pub show_grid: bool,
    pub show_legend: bool,
    pub auto_scale_y: bool,
    pub line_width: f32,
    pub time_window_seconds: f64,
    pub show_raw_values: bool,
}

impl Default for PreferencesState {
    fn default() -> Self {
        let ui_prefs = UiPreferences::default();
        let ui_config = UiConfig::default();
        Self {
            dark_mode: ui_prefs.dark_mode,
            font_scale: ui_prefs.font_scale,
            language: ui_prefs.language,
            show_grid: ui_config.show_grid,
            show_legend: ui_config.show_legend,
            auto_scale_y: ui_config.auto_scale_y,
            line_width: ui_config.line_width,
            time_window_seconds: ui_config.time_window_seconds,
            show_raw_values: ui_config.show_raw_values,
        }
    }
}

impl PreferencesState {
    /// Create from current config and preferences
    pub fn from_config(ui_config: &UiConfig, ui_prefs: &UiPreferences) -> Self {
        Self {
            dark_mode: ui_prefs.dark_mode,
            font_scale: ui_prefs.font_scale,
            language: ui_prefs.language,
            show_grid: ui_config.show_grid,
            show_legend: ui_config.show_legend,
            auto_scale_y: ui_config.auto_scale_y,
            line_width: ui_config.line_width,
            time_window_seconds: ui_config.time_window_seconds,
            show_raw_values: ui_config.show_raw_values,
        }
    }
}

impl DialogState for PreferencesState {}

/// Actions produced by the preferences dialog
#[derive(Debug, Clone)]
pub enum PreferencesAction {
    /// Apply preferences
    Apply(PreferencesState),
}

/// Context for rendering
pub struct PreferencesContext;

/// The preferences dialog
pub struct PreferencesDialog;

impl Dialog for PreferencesDialog {
    type State = PreferencesState;
    type Action = PreferencesAction;
    type Context<'a> = PreferencesContext;

    fn title(_state: &Self::State) -> &'static str {
        // Note: Can't use t!() here as it returns String, not &'static str
        "Preferences"
    }

    fn window_config() -> DialogWindowConfig {
        DialogWindowConfig {
            default_width: 400.0,
            ..Default::default()
        }
    }

    fn render(
        state: &mut Self::State,
        _ctx: Self::Context<'_>,
        ui: &mut Ui,
    ) -> DialogAction<Self::Action> {
        // === Appearance ===
        ui.heading(t!("pref_appearance"));
        ui.add_space(4.0);

        egui::Grid::new("prefs_appearance_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Language selector
                ui.label(format!("{}:", t!("pref_language")));
                egui::ComboBox::from_id_salt("language_selector")
                    .selected_text(state.language.display_name())
                    .show_ui(ui, |ui| {
                        for lang in Language::all() {
                            ui.selectable_value(&mut state.language, *lang, lang.display_name());
                        }
                    });
                ui.end_row();

                ui.label(format!("{}:", t!("pref_dark_mode")));
                ui.checkbox(&mut state.dark_mode, "");
                ui.end_row();

                ui.label(format!("{}:", t!("pref_font_scale")));
                ui.add(egui::Slider::new(&mut state.font_scale, 0.5..=2.0).step_by(0.1));
                ui.end_row();
            });

        ui.add_space(8.0);

        // === Display Defaults ===
        ui.heading(t!("pref_display_defaults"));
        ui.add_space(4.0);

        egui::Grid::new("prefs_display_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label(format!("{}:", t!("pref_grid")));
                ui.checkbox(&mut state.show_grid, "");
                ui.end_row();

                ui.label(format!("{}:", t!("pref_legend")));
                ui.checkbox(&mut state.show_legend, "");
                ui.end_row();

                ui.label(format!("{}:", t!("pref_auto_scale_y")));
                ui.checkbox(&mut state.auto_scale_y, "");
                ui.end_row();

                ui.label(format!("{}:", t!("pref_line_width")));
                ui.add(egui::Slider::new(&mut state.line_width, 0.5..=5.0));
                ui.end_row();

                ui.label(format!("{}:", t!("pref_time_window")));
                ui.add(
                    egui::Slider::new(&mut state.time_window_seconds, 1.0..=120.0)
                        .suffix(t!("unit_seconds")),
                );
                ui.end_row();

                ui.label(format!("{}:", t!("pref_show_raw_values")));
                ui.checkbox(&mut state.show_raw_values, "");
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(t!("dialog_apply")).clicked() {
                return DialogAction::CloseWithAction(PreferencesAction::Apply(state.clone()));
            }
            if ui.button(t!("dialog_cancel")).clicked() {
                return DialogAction::Close;
            }
            DialogAction::None
        })
        .inner
    }
}
