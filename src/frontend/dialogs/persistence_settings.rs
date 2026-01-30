//! Persistence settings dialog
//!
//! Extracted from settings.rs persistence section.
//! Covers enable, file path, format, max size, options.

use egui::Ui;

use crate::config::{DataPersistenceConfig, PersistenceFormat};
use crate::frontend::dialogs::{Dialog, DialogAction, DialogState, DialogWindowConfig};

/// State for the persistence settings dialog
#[derive(Debug, Clone)]
pub struct PersistenceSettingsState {
    pub enabled: bool,
    pub file_path: Option<std::path::PathBuf>,
    pub max_file_size: u64,
    pub format: PersistenceFormat,
    pub include_variable_name: bool,
    pub include_variable_address: bool,
    pub append_mode: bool,
}

impl Default for PersistenceSettingsState {
    fn default() -> Self {
        let defaults = DataPersistenceConfig::default();
        Self {
            enabled: defaults.enabled,
            file_path: defaults.file_path,
            max_file_size: defaults.max_file_size,
            format: defaults.format,
            include_variable_name: defaults.include_variable_name,
            include_variable_address: defaults.include_variable_address,
            append_mode: defaults.append_mode,
        }
    }
}

impl PersistenceSettingsState {
    /// Create state from the current persistence config
    pub fn from_config(config: &DataPersistenceConfig) -> Self {
        Self {
            enabled: config.enabled,
            file_path: config.file_path.clone(),
            max_file_size: config.max_file_size,
            format: config.format,
            include_variable_name: config.include_variable_name,
            include_variable_address: config.include_variable_address,
            append_mode: config.append_mode,
        }
    }

    /// Convert back to config
    pub fn to_config(&self) -> DataPersistenceConfig {
        DataPersistenceConfig {
            enabled: self.enabled,
            file_path: self.file_path.clone(),
            max_file_size: self.max_file_size,
            format: self.format,
            include_variable_name: self.include_variable_name,
            include_variable_address: self.include_variable_address,
            append_mode: self.append_mode,
        }
    }
}

impl DialogState for PersistenceSettingsState {}

/// Actions produced by the persistence settings dialog
#[derive(Debug, Clone)]
pub enum PersistenceSettingsAction {
    /// Apply settings
    Apply(PersistenceSettingsState),
}

/// Context for rendering
pub struct PersistenceSettingsContext;

/// The persistence settings dialog
pub struct PersistenceSettingsDialog;

impl Dialog for PersistenceSettingsDialog {
    type State = PersistenceSettingsState;
    type Action = PersistenceSettingsAction;
    type Context<'a> = PersistenceSettingsContext;

    fn title(_state: &Self::State) -> &'static str {
        "Data Persistence"
    }

    fn window_config() -> DialogWindowConfig {
        DialogWindowConfig {
            default_width: 450.0,
            ..Default::default()
        }
    }

    fn render(
        state: &mut Self::State,
        _ctx: Self::Context<'_>,
        ui: &mut Ui,
    ) -> DialogAction<Self::Action> {
        ui.checkbox(&mut state.enabled, "Enable data persistence");

        if state.enabled {
            ui.add_space(4.0);

            egui::Grid::new("persistence_settings_grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    ui.label("File:");
                    ui.horizontal(|ui| {
                        if let Some(ref path) = state.file_path {
                            ui.label(path.display().to_string());
                        } else {
                            ui.label("(not set)");
                        }
                        if ui.button("Browse...").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("CSV", &["csv"])
                                .add_filter("JSON Lines", &["jsonl"])
                                .add_filter("Binary", &["bin"])
                                .set_file_name("data.csv")
                                .save_file()
                            {
                                state.file_path = Some(path);
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("Format:");
                    egui::ComboBox::from_id_salt("persist_settings_format")
                        .selected_text(state.format.to_string())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.format, PersistenceFormat::Csv, "CSV");
                            ui.selectable_value(
                                &mut state.format,
                                PersistenceFormat::JsonLines,
                                "JSON Lines",
                            );
                            ui.selectable_value(
                                &mut state.format,
                                PersistenceFormat::Binary,
                                "Binary",
                            );
                        });
                    ui.end_row();

                    ui.label("Max File Size:");
                    ui.horizontal(|ui| {
                        let mut size_gb = state.max_file_size as f64 / (1024.0 * 1024.0 * 1024.0);
                        if ui
                            .add(egui::Slider::new(&mut size_gb, 0.1..=2.0).suffix(" GB"))
                            .changed()
                        {
                            state.max_file_size = (size_gb * 1024.0 * 1024.0 * 1024.0) as u64;
                        }
                        ui.label(format!(
                            "({})",
                            crate::config::format_file_size(state.max_file_size)
                        ));
                    });
                    ui.end_row();
                });

            ui.add_space(4.0);
            ui.checkbox(&mut state.include_variable_name, "Include variable name");
            ui.checkbox(
                &mut state.include_variable_address,
                "Include variable address",
            );
            ui.checkbox(&mut state.append_mode, "Append to existing file");
        }

        ui.add_space(8.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                return DialogAction::CloseWithAction(PersistenceSettingsAction::Apply(
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
