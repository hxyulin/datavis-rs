//! Toolbar panel — horizontal bar with connection, collection, and tool buttons.
//!
//! Sits between the menu bar and the dock workspace area.

use egui::{Color32, RichText, Ui};

use crate::backend::DetectedProbe;
use crate::frontend::state::AppAction;
use crate::frontend::topics::Topics;
use crate::config::settings::RuntimeSettings;
use crate::config::AppConfig;
use crate::types::ConnectionStatus;

/// Context needed to render the toolbar.
pub struct ToolbarContext<'a> {
    pub topics: &'a Topics,
    pub config: &'a AppConfig,
    pub settings: &'a RuntimeSettings,
    pub elf_file_path: Option<&'a std::path::PathBuf>,
    pub selected_probe_index: Option<usize>,
    pub target_chip_input: String,
}

/// Mutable state changes from the toolbar
#[derive(Default)]
pub struct ToolbarStateChanges {
    pub selected_probe_index: Option<Option<usize>>,
    pub target_chip_input: Option<String>,
}

/// Result from rendering the toolbar
pub struct ToolbarResult {
    pub actions: Vec<AppAction>,
    pub state_changes: ToolbarStateChanges,
}

/// Render the main application toolbar.
///
/// Returns actions and state changes to be applied by the app.
pub fn render_toolbar(ui: &mut Ui, ctx: &ToolbarContext<'_>) -> ToolbarResult {
    let mut actions = Vec::new();
    let mut state_changes = ToolbarStateChanges::default();

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        // === Connection group ===
        render_connection_group(ui, ctx, &mut actions, &mut state_changes);

        ui.separator();

        // === Collection group ===
        render_collection_group(ui, ctx, &mut actions);

        ui.separator();

        // === Tools group ===
        render_tools_group(ui, ctx, &mut actions);

        // === Right-aligned info group ===
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            render_info_group(ui, ctx);
        });
    });

    ToolbarResult { actions, state_changes }
}

fn render_connection_group(
    ui: &mut Ui,
    ctx: &ToolbarContext<'_>,
    actions: &mut Vec<AppAction>,
    state_changes: &mut ToolbarStateChanges,
) {
    let status = ctx.topics.connection_status;

    match status {
        ConnectionStatus::Connected => {
            // Show connected status indicator
            ui.colored_label(Color32::GREEN, "●");

            let btn = egui::Button::new(
                RichText::new("Disconnect").color(Color32::WHITE),
            )
            .fill(Color32::from_rgb(50, 120, 50));
            if ui.add(btn).on_hover_text("Disconnect from probe").clicked() {
                actions.push(AppAction::Disconnect);
            }
        }
        ConnectionStatus::Connecting => {
            ui.colored_label(Color32::YELLOW, "●");
            ui.add_enabled(false, egui::Button::new("Connecting..."));
        }
        ConnectionStatus::Disconnected | ConnectionStatus::Error => {
            // Status indicator
            let status_color = if status == ConnectionStatus::Error {
                Color32::RED
            } else {
                Color32::GRAY
            };
            ui.colored_label(status_color, "●");

            // Current selected probe index (use state_changes if set, otherwise ctx)
            let current_probe_index = state_changes
                .selected_probe_index
                .unwrap_or(ctx.selected_probe_index);

            // Probe selector dropdown
            let probe_text = if let Some(idx) = current_probe_index {
                if let Some(probe) = ctx.topics.available_probes.get(idx) {
                    match probe {
                        DetectedProbe::Real(info) => {
                            format!("{} ({:04x}:{:04x})", info.probe_type, info.vendor_id, info.product_id)
                        }
                        #[cfg(feature = "mock-probe")]
                        DetectedProbe::Mock(info) => info.name.clone(),
                    }
                } else {
                    "Select probe...".to_string()
                }
            } else {
                "Select probe...".to_string()
            };

            egui::ComboBox::from_id_salt("toolbar_probe_selector")
                .selected_text(&probe_text)
                .width(180.0)
                .show_ui(ui, |ui| {
                    if ctx.topics.available_probes.is_empty() {
                        ui.label("No probes found");
                    } else {
                        for (i, probe) in ctx.topics.available_probes.iter().enumerate() {
                            let label = match probe {
                                DetectedProbe::Real(info) => {
                                    format!("{} ({:04x}:{:04x})", info.probe_type, info.vendor_id, info.product_id)
                                }
                                #[cfg(feature = "mock-probe")]
                                DetectedProbe::Mock(info) => info.name.clone(),
                            };
                            if ui.selectable_label(current_probe_index == Some(i), &label).clicked() {
                                state_changes.selected_probe_index = Some(Some(i));
                            }
                        }
                    }
                    ui.separator();
                    if ui.button("Refresh").clicked() {
                        actions.push(AppAction::RefreshProbes);
                    }
                });

            // Target chip input (compact) - use state changes if available
            let mut target_chip = state_changes
                .target_chip_input
                .clone()
                .unwrap_or_else(|| ctx.target_chip_input.clone());

            let response = ui.add(
                egui::TextEdit::singleline(&mut target_chip)
                    .hint_text("Target chip")
                    .desired_width(100.0)
            );
            if response.changed() {
                state_changes.target_chip_input = Some(target_chip.clone());
            }

            // Connect button
            let can_connect = current_probe_index.is_some();
            let btn = egui::Button::new("Connect");
            let response = ui
                .add_enabled(can_connect, btn)
                .on_hover_text("Connect to selected probe");
            if response.clicked() {
                #[cfg(feature = "mock-probe")]
                if let Some(idx) = current_probe_index {
                    if let Some(DetectedProbe::Mock(_)) = ctx.topics.available_probes.get(idx) {
                        actions.push(AppAction::UseMockProbe(true));
                    }
                }

                if let Some(idx) = current_probe_index {
                    let selector = match ctx.topics.available_probes.get(idx) {
                        Some(DetectedProbe::Real(info)) => Some(format!(
                            "{:04x}:{:04x}",
                            info.vendor_id, info.product_id
                        )),
                        #[cfg(feature = "mock-probe")]
                        Some(DetectedProbe::Mock(_)) => None,
                        _ => None,
                    };
                    actions.push(AppAction::Connect {
                        probe_selector: selector,
                        target: target_chip,
                    });
                }
            }
        }
    }
}

fn render_collection_group(ui: &mut Ui, ctx: &ToolbarContext<'_>, actions: &mut Vec<AppAction>) {
    let connected = ctx.topics.connection_status == ConnectionStatus::Connected;

    if ctx.settings.collecting {
        // Stop button
        let btn = egui::Button::new(
            RichText::new("Stop").color(Color32::WHITE),
        )
        .fill(Color32::from_rgb(180, 50, 50));
        if ui
            .add_enabled(connected, btn)
            .on_hover_text("Stop collection (Space)")
            .clicked()
        {
            actions.push(AppAction::StopCollection);
        }

        // Pause button
        let pause_text = if ctx.settings.paused { "Resume" } else { "Pause" };
        if ui
            .add_enabled(connected, egui::Button::new(pause_text))
            .on_hover_text("Pause/resume collection (P)")
            .clicked()
        {
            actions.push(AppAction::TogglePause);
        }
    } else {
        // Start button
        let btn = egui::Button::new(
            RichText::new("Start").color(Color32::WHITE),
        )
        .fill(Color32::from_rgb(50, 120, 50));
        if ui
            .add_enabled(connected, btn)
            .on_hover_text("Start collection (Space)")
            .clicked()
        {
            actions.push(AppAction::StartCollection);
        }
    }
}

fn render_tools_group(ui: &mut Ui, ctx: &ToolbarContext<'_>, actions: &mut Vec<AppAction>) {
    // Load ELF
    if ui
        .button("Load ELF")
        .on_hover_text("Load an ELF binary to browse variables")
        .clicked()
    {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Load ELF Binary")
            .add_filter("ELF files", &["elf", "axf", "out", "bin"])
            .pick_file()
        {
            actions.push(AppAction::LoadElf(path));
        }
    }

    // Clear data
    if ui
        .button("Clear")
        .on_hover_text("Clear all collected data (Ctrl+L)")
        .clicked()
    {
        actions.push(AppAction::ClearData);
    }

    // Show current ELF file name if loaded
    if let Some(elf_path) = ctx.elf_file_path {
        if let Some(name) = elf_path.file_name().and_then(|n| n.to_str()) {
            ui.separator();
            ui.label(
                RichText::new(format!("ELF: {}", name))
                    .small()
                    .color(Color32::from_rgb(150, 150, 200)),
            );
        }
    }
}

fn render_info_group(ui: &mut Ui, ctx: &ToolbarContext<'_>) {
    let stats = &ctx.topics.stats;

    // Effective sample rate
    let actual_rate = stats.effective_sample_rate;
    let rate_color = if actual_rate > 0.0 {
        Color32::from_rgb(100, 255, 100)
    } else {
        Color32::GRAY
    };
    ui.colored_label(rate_color, format!("{:.0} Hz", actual_rate));
    ui.label("Rate:");

    // Collection status indicator
    if ctx.settings.collecting {
        if ctx.settings.paused {
            ui.colored_label(Color32::YELLOW, "Paused");
        } else {
            ui.colored_label(Color32::GREEN, "Recording");
        }
    }
}
