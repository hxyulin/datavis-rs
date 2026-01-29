//! Collection settings dialog
//!
//! Extracted from settings.rs collection section.
//! Covers poll rate, max data points, timeout.

use egui::Ui;

use crate::config::CollectionConfig;
use crate::frontend::dialogs::{Dialog, DialogAction, DialogState, DialogWindowConfig};

/// State for the collection settings dialog
#[derive(Debug, Clone)]
pub struct CollectionSettingsState {
    pub poll_rate_hz: u32,
    pub max_data_points: usize,
    pub timeout_ms: u64,
}

impl Default for CollectionSettingsState {
    fn default() -> Self {
        let defaults = CollectionConfig::default();
        Self {
            poll_rate_hz: defaults.poll_rate_hz,
            max_data_points: defaults.max_data_points,
            timeout_ms: defaults.timeout_ms,
        }
    }
}

impl CollectionSettingsState {
    /// Create state from the current collection config
    pub fn from_config(config: &CollectionConfig) -> Self {
        Self {
            poll_rate_hz: config.poll_rate_hz,
            max_data_points: config.max_data_points,
            timeout_ms: config.timeout_ms,
        }
    }
}

impl DialogState for CollectionSettingsState {}

/// Actions produced by the collection settings dialog
#[derive(Debug, Clone)]
pub enum CollectionSettingsAction {
    /// Apply settings
    Apply(CollectionSettingsState),
}

/// Context for rendering
pub struct CollectionSettingsContext;

/// The collection settings dialog
pub struct CollectionSettingsDialog;

impl Dialog for CollectionSettingsDialog {
    type State = CollectionSettingsState;
    type Action = CollectionSettingsAction;
    type Context<'a> = CollectionSettingsContext;

    fn title(_state: &Self::State) -> &'static str {
        "Collection Settings"
    }

    fn window_config() -> DialogWindowConfig {
        DialogWindowConfig {
            default_width: 350.0,
            ..Default::default()
        }
    }

    fn render(
        state: &mut Self::State,
        _ctx: Self::Context<'_>,
        ui: &mut Ui,
    ) -> DialogAction<Self::Action> {
        egui::Grid::new("collection_settings_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("Poll Rate (Hz):");
                ui.add(
                    egui::DragValue::new(&mut state.poll_rate_hz)
                        .range(1..=10000)
                        .speed(10),
                );
                ui.end_row();

                ui.label("Timeout (ms):");
                ui.add(
                    egui::DragValue::new(&mut state.timeout_ms)
                        .range(10..=5000)
                        .speed(10),
                );
                ui.end_row();

                ui.label("Max Data Points:");
                ui.add(
                    egui::DragValue::new(&mut state.max_data_points)
                        .range(1000..=1_000_000)
                        .speed(1000),
                );
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                return DialogAction::CloseWithAction(CollectionSettingsAction::Apply(
                    state.clone(),
                ));
            }
            if ui.button("Cancel").clicked() {
                return DialogAction::Close;
            }
            DialogAction::None
        })
        .inner
    }
}
