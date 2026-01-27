//! Settings page - Probe, collection, UI, and persistence settings
//!
//! This page provides configuration for:
//! - Project management (save/load)
//! - Probe connection settings
//! - Data collection parameters
//! - Data persistence options
//! - Display settings

use std::path::PathBuf;

use egui::{Color32, Context, Ui};

use super::Page;
use crate::backend::DetectedProbe;
use crate::frontend::dialogs::{
    show_dialog, DuplicateConfirmAction, DuplicateConfirmContext, DuplicateConfirmDialog,
    DuplicateConfirmState,
};
use crate::frontend::state::{AppAction, SharedState};
use crate::types::ConnectionStatus;

/// State specific to the Settings page
#[derive(Default)]
pub struct SettingsPageState {
    /// Available probes list (cached from backend)
    pub available_probes: Vec<DetectedProbe>,
    /// Selected probe index
    pub selected_probe_index: Option<usize>,
    /// Target chip input field
    pub target_chip_input: String,
    /// Project name input
    pub project_name: String,
    /// Path to the current project file
    pub project_file_path: Option<PathBuf>,
    /// Mock probe enabled (feature-gated)
    #[cfg(feature = "mock-probe")]
    pub use_mock_probe: bool,
    /// Duplicate confirmation dialog state
    pub duplicate_confirm_open: bool,
    pub duplicate_confirm_state: DuplicateConfirmState,
}

pub struct SettingsPage;

impl Page for SettingsPage {
    type State = SettingsPageState;

    fn render(
        state: &mut Self::State,
        shared: &mut SharedState<'_>,
        ctx: &Context,
    ) -> Vec<AppAction> {
        let mut actions = Vec::new();

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Settings");
                ui.separator();

                Self::render_project_section(state, shared, ui, &mut actions);
                ui.separator();

                Self::render_probe_section(state, shared, ui, &mut actions);
                ui.separator();

                Self::render_collection_section(shared, ui, &mut actions);
                ui.separator();

                Self::render_persistence_section(shared, ui);
                ui.separator();

                Self::render_display_section(shared, ui);

                // Show errors at the bottom
                if let Some(ref error) = shared.last_error {
                    ui.separator();
                    ui.colored_label(Color32::RED, format!("Error: {}", error));
                    if ui.button("Dismiss").clicked() {
                        *shared.last_error = None;
                    }
                }
            });
        });

        // Render dialogs
        Self::render_dialogs(state, shared, ctx, &mut actions);

        actions
    }
}

impl SettingsPage {
    fn render_project_section(
        state: &mut SettingsPageState,
        _shared: &mut SharedState<'_>,
        ui: &mut Ui,
        actions: &mut Vec<AppAction>,
    ) {
        ui.heading("Project");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Project Name:");
            ui.text_edit_singleline(&mut state.project_name);
        });

        ui.add_space(8.0);
        ui.label("Project File (.datavisproj):");
        ui.horizontal(|ui| {
            if let Some(ref path) = state.project_file_path {
                ui.label(path.display().to_string());
            } else {
                ui.label("(no project file)");
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Save Project").clicked() {
                if let Some(ref path) = state.project_file_path {
                    actions.push(AppAction::SaveProject(path.clone()));
                } else {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter(
                            "DataVis Project",
                            &[crate::config::PROJECT_FILE_EXTENSION],
                        )
                        .set_file_name("project.datavisproj")
                        .save_file()
                    {
                        actions.push(AppAction::SaveProject(path));
                    }
                }
            }
            if ui.button("Save Project As...").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        "DataVis Project",
                        &[crate::config::PROJECT_FILE_EXTENSION],
                    )
                    .set_file_name("project.datavisproj")
                    .save_file()
                {
                    actions.push(AppAction::SaveProject(path));
                }
            }
            if ui.button("Load Project...").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        "DataVis Project",
                        &[crate::config::PROJECT_FILE_EXTENSION],
                    )
                    .pick_file()
                {
                    actions.push(AppAction::LoadProject(path));
                }
            }
        });
    }

    fn render_probe_section(
        state: &mut SettingsPageState,
        shared: &mut SharedState<'_>,
        ui: &mut Ui,
        actions: &mut Vec<AppAction>,
    ) {
        ui.heading("Probe Connection");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Target Chip:");
            ui.text_edit_singleline(&mut state.target_chip_input);
            if ui.button("Apply").clicked() {
                shared.config.probe.target_chip = state.target_chip_input.clone();
            }
        });

        ui.horizontal(|ui| {
            ui.label("Speed (kHz):");
            ui.add(egui::DragValue::new(&mut shared.config.probe.speed_khz).range(100..=50000));
        });

        ui.horizontal(|ui| {
            ui.label("Connect Under Reset:");
            egui::ComboBox::from_id_salt("connect_under_reset")
                .selected_text(shared.config.probe.connect_under_reset.to_string())
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut shared.config.probe.connect_under_reset,
                        crate::config::ConnectUnderReset::None,
                        "None (normal attach)",
                    );
                    ui.selectable_value(
                        &mut shared.config.probe.connect_under_reset,
                        crate::config::ConnectUnderReset::Software,
                        "Software (SYSRESETREQ)",
                    );
                    ui.selectable_value(
                        &mut shared.config.probe.connect_under_reset,
                        crate::config::ConnectUnderReset::Hardware,
                        "Hardware (NRST pin)",
                    );
                    ui.selectable_value(
                        &mut shared.config.probe.connect_under_reset,
                        crate::config::ConnectUnderReset::Core,
                        "Core Reset (VECTRESET)",
                    );
                });
        });

        ui.checkbox(
            &mut shared.config.probe.halt_on_connect,
            "Halt on connect",
        );

        ui.horizontal(|ui| {
            ui.label("Memory Access:");
            let old_mode = shared.config.probe.memory_access_mode;
            egui::ComboBox::from_id_salt("memory_access_mode")
                .selected_text(old_mode.to_string())
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut shared.config.probe.memory_access_mode,
                        crate::config::MemoryAccessMode::Background,
                        "Background (Running)",
                    );
                    ui.selectable_value(
                        &mut shared.config.probe.memory_access_mode,
                        crate::config::MemoryAccessMode::Halted,
                        "Halted (Per-batch)",
                    );
                    ui.selectable_value(
                        &mut shared.config.probe.memory_access_mode,
                        crate::config::MemoryAccessMode::HaltedPersistent,
                        "Halted (Persistent)",
                    );
                });
            if shared.config.probe.memory_access_mode != old_mode {
                actions.push(AppAction::SetMemoryAccessMode(
                    shared.config.probe.memory_access_mode,
                ));
            }
        })
        .response
        .on_hover_text(
            "Background: Read while target runs (slower)\n\
             Halted: Briefly halt for each read batch (faster)\n\
             Persistent: Keep target halted (fastest)",
        );

        ui.horizontal(|ui| {
            ui.label("Probe:");
            egui::ComboBox::from_id_salt("settings_probe_selector")
                .selected_text(
                    state
                        .selected_probe_index
                        .and_then(|i| state.available_probes.get(i))
                        .map(|p| p.display_name())
                        .as_deref()
                        .unwrap_or("Select probe..."),
                )
                .show_ui(ui, |ui| {
                    for (i, probe) in state.available_probes.iter().enumerate() {
                        let is_selected = state.selected_probe_index == Some(i);
                        if ui
                            .selectable_label(is_selected, probe.display_name())
                            .clicked()
                        {
                            state.selected_probe_index = Some(i);
                            #[cfg(feature = "mock-probe")]
                            {
                                state.use_mock_probe = probe.is_mock();
                            }
                        }
                    }
                });
            if ui.button("Refresh").clicked() {
                actions.push(AppAction::RefreshProbes);
            }
        });

        ui.horizontal(|ui| {
            match shared.connection_status {
                ConnectionStatus::Connected => {
                    if ui.button("Disconnect").clicked() {
                        actions.push(AppAction::Disconnect);
                    }
                }
                ConnectionStatus::Connecting => {
                    ui.add_enabled(false, egui::Button::new("Connecting..."));
                }
                _ => {
                    let can_connect = state.selected_probe_index.is_some();
                    if ui
                        .add_enabled(can_connect, egui::Button::new("Connect"))
                        .clicked()
                    {
                        #[cfg(feature = "mock-probe")]
                        actions.push(AppAction::UseMockProbe(state.use_mock_probe));

                        if let Some(idx) = state.selected_probe_index {
                            let selector = match state.available_probes.get(idx) {
                                Some(DetectedProbe::Real(info)) => Some(format!(
                                    "{:04x}:{:04x}",
                                    info.vendor_id, info.product_id
                                )),
                                #[cfg(feature = "mock-probe")]
                                Some(DetectedProbe::Mock(_)) => None,
                                #[cfg(not(feature = "mock-probe"))]
                                _ => None,
                            };
                            actions.push(AppAction::Connect {
                                probe_selector: selector,
                                target: shared.config.probe.target_chip.clone(),
                            });
                        }
                    }
                }
            }
        });
    }

    fn render_collection_section(
        shared: &mut SharedState<'_>,
        ui: &mut Ui,
        actions: &mut Vec<AppAction>,
    ) {
        ui.heading("Data Collection");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Poll Rate (Hz):");
            let mut rate = shared.config.collection.poll_rate_hz;
            if ui
                .add(egui::DragValue::new(&mut rate).range(1..=10000))
                .changed()
            {
                shared.config.collection.poll_rate_hz = rate;
                actions.push(AppAction::SetPollRate(rate));
            }
        });

        ui.horizontal(|ui| {
            ui.label("Max Data Points:");
            ui.add(
                egui::DragValue::new(&mut shared.config.collection.max_data_points)
                    .range(100..=1000000),
            );
        });
    }

    fn render_persistence_section(shared: &mut SharedState<'_>, ui: &mut Ui) {
        ui.heading("Data Persistence");
        ui.add_space(4.0);

        ui.checkbox(
            &mut shared.persistence_config.enabled,
            "Enable data persistence",
        );

        if shared.persistence_config.enabled {
            ui.horizontal(|ui| {
                ui.label("Persistence File:");
                if let Some(ref path) = shared.persistence_config.file_path {
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
                        shared.persistence_config.file_path = Some(path);
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label("Max File Size:");
                let mut size_gb = shared.persistence_config.max_file_size as f64
                    / (1024.0 * 1024.0 * 1024.0);
                if ui
                    .add(egui::Slider::new(&mut size_gb, 0.1..=2.0).suffix(" GB"))
                    .changed()
                {
                    shared.persistence_config.max_file_size =
                        (size_gb * 1024.0 * 1024.0 * 1024.0) as u64;
                }
                ui.label(format!(
                    "({})",
                    crate::config::format_file_size(shared.persistence_config.max_file_size)
                ));
            });

            ui.horizontal(|ui| {
                ui.label("Format:");
                egui::ComboBox::from_id_salt("persistence_format")
                    .selected_text(shared.persistence_config.format.to_string())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut shared.persistence_config.format,
                            crate::config::PersistenceFormat::Csv,
                            "CSV",
                        );
                        ui.selectable_value(
                            &mut shared.persistence_config.format,
                            crate::config::PersistenceFormat::JsonLines,
                            "JSON Lines",
                        );
                        ui.selectable_value(
                            &mut shared.persistence_config.format,
                            crate::config::PersistenceFormat::Binary,
                            "Binary",
                        );
                    });
            });

            ui.checkbox(
                &mut shared.persistence_config.include_variable_name,
                "Include variable name",
            );
            ui.checkbox(
                &mut shared.persistence_config.include_variable_address,
                "Include variable address",
            );
            ui.checkbox(
                &mut shared.persistence_config.append_mode,
                "Append to existing file",
            );
        }
    }

    fn render_display_section(shared: &mut SharedState<'_>, ui: &mut Ui) {
        ui.heading("Display");
        ui.add_space(4.0);

        ui.checkbox(&mut shared.config.ui.show_grid, "Show Grid");
        ui.checkbox(&mut shared.config.ui.show_legend, "Show Legend");
        ui.checkbox(&mut shared.config.ui.auto_scale_y, "Auto-scale Y Axis");

        ui.horizontal(|ui| {
            ui.label("Line Width:");
            ui.add(egui::Slider::new(&mut shared.config.ui.line_width, 0.5..=5.0));
        });

        ui.horizontal(|ui| {
            ui.label("Default Time Window:");
            ui.add(
                egui::Slider::new(&mut shared.config.ui.time_window_seconds, 1.0..=120.0)
                    .suffix("s"),
            );
        });
    }

    fn render_dialogs(
        state: &mut SettingsPageState,
        _shared: &mut SharedState<'_>,
        ctx: &Context,
        actions: &mut Vec<AppAction>,
    ) {
        // Duplicate confirmation dialog
        if state.duplicate_confirm_open {
            let dialog_ctx = DuplicateConfirmContext;

            if let Some(action) = show_dialog::<DuplicateConfirmDialog>(
                ctx,
                &mut state.duplicate_confirm_open,
                &mut state.duplicate_confirm_state,
                dialog_ctx,
            ) {
                match action {
                    DuplicateConfirmAction::Confirm(var) => {
                        actions.push(AppAction::AddVariable(var));
                    }
                }
            }
        }
    }
}
