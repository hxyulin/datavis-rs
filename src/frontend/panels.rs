//! Panel components for the frontend UI
//!
//! This module provides reusable panel components for the data visualizer.
//! Each panel encapsulates a specific piece of UI functionality and can be
//! composed together to build the main application interface.
//!
//! # Panel Trait
//!
//! The [`Panel`] trait provides a unified interface for panel components.
//! Each panel implements this trait to standardize rendering and action handling.
//!
//! # Panels
//!
//! - [`ConnectionPanel`] - Displays connection status and probe controls
//! - [`StatsPanel`] - Shows real-time collection statistics
//! - [`VariableListPanel`] - Lists observed variables with enable/disable controls
//! - [`CollectionControlPanel`] - Start/stop/pause data collection
//! - [`TimeWindowPanel`] - Adjust the visible time window for plotting
//! - [`ProbeListPanel`] - Select from available debug probes
//! - [`SettingsPanel`] - Application settings and configuration

use crate::config::AppConfig;
use crate::types::{CollectionStats, ConnectionStatus, VariableData};
use egui::{Color32, RichText, Ui};
use std::collections::HashMap;

// ============================================================================
// Panel Trait System
// ============================================================================

/// Response from panel rendering
///
/// Contains any actions the panel wants to perform and whether content changed.
#[derive(Default)]
pub struct PanelResponse<A = ()> {
    /// Actions to perform after rendering
    pub actions: Vec<A>,
    /// Whether the panel contents changed (for optimization)
    pub changed: bool,
}

impl<A> PanelResponse<A> {
    /// Create a new empty panel response
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
            changed: false,
        }
    }

    /// Add an action to the response
    pub fn with_action(mut self, action: A) -> Self {
        self.actions.push(action);
        self
    }

    /// Mark the panel as having changed
    pub fn with_changed(mut self, changed: bool) -> Self {
        self.changed = changed;
        self
    }

    /// Check if any actions were produced
    pub fn has_actions(&self) -> bool {
        !self.actions.is_empty()
    }

    /// Take all actions from the response
    pub fn take_actions(&mut self) -> Vec<A> {
        std::mem::take(&mut self.actions)
    }
}

/// Trait for panel components
///
/// Panels are reusable UI components that render a specific piece of functionality.
/// Each panel defines its own action type and props (context needed for rendering).
///
/// # Example
///
/// ```ignore
/// pub enum MyPanelAction {
///     ButtonClicked,
///     ValueChanged(f64),
/// }
///
/// pub struct MyPanelProps<'a> {
///     pub value: &'a mut f64,
///     pub enabled: bool,
/// }
///
/// pub struct MyPanel;
///
/// impl Panel for MyPanel {
///     type Action = MyPanelAction;
///     type Props<'a> = MyPanelProps<'a>;
///
///     fn render(ui: &mut Ui, props: Self::Props<'_>) -> PanelResponse<Self::Action> {
///         let mut response = PanelResponse::new();
///         // Render panel content...
///         response
///     }
/// }
/// ```
pub trait Panel {
    /// The action type this panel can produce
    type Action;

    /// The props/context needed to render this panel
    type Props<'a>;

    /// Render the panel and return any actions
    fn render(ui: &mut Ui, props: Self::Props<'_>) -> PanelResponse<Self::Action>;
}

// ============================================================================
// Panel Implementations (Legacy - will be migrated to use Panel trait in Phase 4)
// ============================================================================

/// Renders the connection status panel
pub struct ConnectionPanel;

impl ConnectionPanel {
    /// Render the connection status indicator
    pub fn render(
        ui: &mut Ui,
        status: ConnectionStatus,
        target_chip: &mut String,
        on_connect: impl FnOnce(),
        on_disconnect: impl FnOnce(),
    ) {
        ui.horizontal(|ui| {
            // Status indicator
            let (status_text, status_color) = match status {
                ConnectionStatus::Disconnected => ("Disconnected", Color32::GRAY),
                ConnectionStatus::Connecting => ("Connecting...", Color32::YELLOW),
                ConnectionStatus::Connected => ("Connected", Color32::GREEN),
                ConnectionStatus::Error => ("Error", Color32::RED),
            };

            ui.colored_label(status_color, format!("● {}", status_text));

            ui.separator();

            // Target chip input
            ui.label("Target:");
            ui.add(
                egui::TextEdit::singleline(target_chip)
                    .desired_width(150.0)
                    .hint_text("e.g., STM32F407VGTx"),
            );

            // Connect/Disconnect button
            match status {
                ConnectionStatus::Disconnected | ConnectionStatus::Error => {
                    if ui.button("Connect").clicked() {
                        on_connect();
                    }
                }
                ConnectionStatus::Connecting => {
                    ui.add_enabled(false, egui::Button::new("Connecting..."));
                }
                ConnectionStatus::Connected => {
                    if ui.button("Disconnect").clicked() {
                        on_disconnect();
                    }
                }
            }
        });
    }
}

/// Renders the statistics panel
pub struct StatsPanel;

impl StatsPanel {
    /// Render statistics display
    pub fn render(ui: &mut Ui, stats: &CollectionStats, error_message: Option<&str>) {
        ui.horizontal(|ui| {
            // Sample count
            ui.label(RichText::new(format!("Samples: {}", stats.successful_reads)).monospace());

            ui.separator();

            // Error count
            let error_color = if stats.failed_reads > 0 {
                Color32::LIGHT_RED
            } else {
                Color32::GRAY
            };
            ui.colored_label(error_color, format!("Errors: {}", stats.failed_reads));

            ui.separator();

            // Success rate
            let success_rate = stats.success_rate();
            let rate_color = if success_rate >= 99.0 {
                Color32::GREEN
            } else if success_rate >= 95.0 {
                Color32::YELLOW
            } else {
                Color32::RED
            };
            ui.colored_label(rate_color, format!("Success: {:.1}%", success_rate));

            ui.separator();

            // Timing
            ui.label(format!("Avg: {:.1} μs", stats.avg_read_time_us));

            ui.separator();

            // Sample rate
            ui.label(format!("Rate: {:.1} Hz", stats.effective_sample_rate));

            ui.separator();

            // Data transferred
            let kb = stats.total_bytes_read as f64 / 1024.0;
            if kb > 1024.0 {
                ui.label(format!("Data: {:.2} MB", kb / 1024.0));
            } else {
                ui.label(format!("Data: {:.2} KB", kb));
            }
        });

        // Error message display
        if let Some(error) = error_message {
            ui.colored_label(Color32::RED, format!("Warning:{}", error));
        }
    }
}

/// Renders the variable list panel
pub struct VariableListPanel;

impl VariableListPanel {
    /// Render the variable list
    pub fn render(
        ui: &mut Ui,
        variable_data: &HashMap<u32, VariableData>,
        selected_id: &mut Option<u32>,
        on_add: impl FnOnce(),
        mut on_toggle: impl FnMut(u32, bool),
        mut on_remove: impl FnMut(u32),
    ) {
        ui.horizontal(|ui| {
            ui.heading("Variables");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Add").clicked() {
                    on_add();
                }
            });
        });

        ui.separator();

        if variable_data.is_empty() {
            ui.colored_label(Color32::GRAY, "No variables configured");
            ui.label("Click 'Add' to add a variable to observe.");
            return;
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut ids: Vec<u32> = variable_data.keys().copied().collect();
                ids.sort();

                for id in ids {
                    if let Some(data) = variable_data.get(&id) {
                        Self::render_variable_item(
                            ui,
                            data,
                            selected_id,
                            &mut on_toggle,
                            &mut on_remove,
                        );
                    }
                }
            });
    }

    /// Render a single variable item
    fn render_variable_item(
        ui: &mut Ui,
        data: &VariableData,
        selected_id: &mut Option<u32>,
        on_toggle: &mut impl FnMut(u32, bool),
        on_remove: &mut impl FnMut(u32),
    ) {
        let var = &data.variable;
        let id = var.id;
        let is_selected = *selected_id == Some(id);

        let color =
            Color32::from_rgba_unmultiplied(var.color[0], var.color[1], var.color[2], var.color[3]);

        // Variable row
        let _response = ui
            .horizontal(|ui| {
                // Enable/disable checkbox
                let mut enabled = var.enabled;
                if ui.checkbox(&mut enabled, "").changed() {
                    on_toggle(id, enabled);
                }

                // Color indicator
                ui.colored_label(color, "●");

                // Variable name
                let name_response = ui.selectable_label(is_selected, &var.name);
                if name_response.clicked() {
                    *selected_id = if is_selected { None } else { Some(id) };
                }

                // Current value
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(value) = data.last_converted_value {
                        let value_text = if var.unit.is_empty() {
                            format!("{:.3}", value)
                        } else {
                            format!("{:.3} {}", value, var.unit)
                        };
                        ui.label(RichText::new(value_text).monospace());
                    } else {
                        ui.colored_label(Color32::GRAY, "---");
                    }
                });
            })
            .response;

        // Show details when selected
        if is_selected {
            ui.indent(id, |ui| {
                egui::Grid::new(format!("var_details_{}", id))
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Address:");
                        ui.label(RichText::new(format!("0x{:08X}", var.address)).monospace());
                        ui.end_row();

                        ui.label("Type:");
                        ui.label(format!("{}", var.var_type));
                        ui.end_row();

                        if let Some(raw) = data.last_value {
                            ui.label("Raw:");
                            ui.label(RichText::new(format!("{:.6}", raw)).monospace());
                            ui.end_row();
                        }

                        if data.error_count > 0 {
                            ui.label("Errors:");
                            ui.colored_label(Color32::LIGHT_RED, format!("{}", data.error_count));
                            ui.end_row();
                        }

                        if let Some(ref script) = var.converter_script {
                            ui.label("Converter:");
                            ui.label(RichText::new(script).small().monospace());
                            ui.end_row();
                        }
                    });

                ui.horizontal(|ui| {
                    if ui.button("Remove").clicked() {
                        on_remove(id);
                        if *selected_id == Some(id) {
                            *selected_id = None;
                        }
                    }
                });
            });
        }

        ui.separator();
    }
}

/// Renders the collection control panel
pub struct CollectionControlPanel;

impl CollectionControlPanel {
    /// Render collection controls
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        ui: &mut Ui,
        connected: bool,
        collecting: bool,
        paused: bool,
        on_start: impl FnOnce(),
        on_stop: impl FnOnce(),
        on_pause: impl FnOnce(),
        on_clear: impl FnOnce(),
    ) {
        ui.horizontal(|ui| {
            if collecting {
                // Stop button
                if ui
                    .add_enabled(connected, egui::Button::new("Stop"))
                    .clicked()
                {
                    on_stop();
                }

                // Pause/Resume button
                if paused {
                    if ui
                        .add_enabled(connected, egui::Button::new("Resume"))
                        .clicked()
                    {
                        on_pause();
                    }
                    ui.colored_label(Color32::YELLOW, "Paused");
                } else {
                    if ui
                        .add_enabled(connected, egui::Button::new("Pause"))
                        .clicked()
                    {
                        on_pause();
                    }
                    ui.colored_label(Color32::GREEN, "● Recording");
                }
            } else {
                // Start button
                if ui
                    .add_enabled(connected, egui::Button::new("Start"))
                    .clicked()
                {
                    on_start();
                }
                ui.colored_label(Color32::GRAY, "○ Stopped");
            }

            ui.separator();

            // Clear button (always available)
            if ui.button("Clear Data").clicked() {
                on_clear();
            }
        });
    }
}

/// Renders the time window control panel
pub struct TimeWindowPanel;

impl TimeWindowPanel {
    /// Render time window controls
    pub fn render(
        ui: &mut Ui,
        time_window: &mut f64,
        follow_latest: &mut bool,
        auto_scale_y: bool,
        on_toggle_auto_scale: impl FnOnce(),
    ) {
        ui.horizontal(|ui| {
            ui.label("Time Window:");
            ui.add(
                egui::Slider::new(time_window, 0.5..=120.0)
                    .suffix(" s")
                    .logarithmic(true)
                    .clamping(egui::SliderClamping::Always),
            );

            ui.separator();

            ui.checkbox(follow_latest, "Follow Latest");

            ui.separator();

            if auto_scale_y {
                if ui.button("Manual Y Scale").clicked() {
                    on_toggle_auto_scale();
                }
            } else if ui.button("Auto Y Scale").clicked() {
                on_toggle_auto_scale();
            }
        });
    }
}

/// Renders a probe selector panel
pub struct ProbeListPanel;

impl ProbeListPanel {
    /// Render a list of available probes
    pub fn render(
        ui: &mut Ui,
        probes: &[crate::backend::probe::ProbeInfo],
        selected: &mut Option<usize>,
    ) {
        ui.heading("Available Probes");

        if probes.is_empty() {
            ui.colored_label(Color32::YELLOW, "No probes detected");
            ui.label("Make sure your debug probe is connected.");
        } else {
            for (i, probe) in probes.iter().enumerate() {
                let is_selected = *selected == Some(i);

                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(is_selected, format!("{}", probe))
                        .clicked()
                    {
                        *selected = Some(i);
                    }
                });
            }
        }

        ui.separator();

        if ui.button("Refresh").clicked() {
            // Caller should refresh the probe list
        }
    }
}

/// Renders a settings panel
pub struct SettingsPanel;

impl SettingsPanel {
    /// Render the settings panel
    ///
    /// Note: This panel only renders config settings that are part of a project.
    /// App-wide preferences (like dark mode) are handled separately in AppState.
    pub fn render(ui: &mut Ui, config: &mut AppConfig) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Probe Settings");
            ui.separator();

            egui::Grid::new("probe_settings")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Target Chip:");
                    ui.text_edit_singleline(&mut config.probe.target_chip);
                    ui.end_row();

                    ui.label("Speed (kHz):");
                    ui.add(
                        egui::DragValue::new(&mut config.probe.speed_khz)
                            .range(100..=50000)
                            .speed(100),
                    );
                    ui.end_row();

                    ui.label("Connect Under Reset:");
                    egui::ComboBox::from_id_salt("panel_connect_under_reset")
                        .selected_text(config.probe.connect_under_reset.to_string())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config.probe.connect_under_reset,
                                crate::config::ConnectUnderReset::None,
                                "None",
                            );
                            ui.selectable_value(
                                &mut config.probe.connect_under_reset,
                                crate::config::ConnectUnderReset::Software,
                                "Software (SYSRESETREQ)",
                            );
                            ui.selectable_value(
                                &mut config.probe.connect_under_reset,
                                crate::config::ConnectUnderReset::Hardware,
                                "Hardware (NRST)",
                            );
                            ui.selectable_value(
                                &mut config.probe.connect_under_reset,
                                crate::config::ConnectUnderReset::Core,
                                "Core Reset",
                            );
                        });
                    ui.end_row();

                    ui.label("Halt on Connect:");
                    ui.checkbox(&mut config.probe.halt_on_connect, "");
                    ui.end_row();
                });

            ui.add_space(16.0);
            ui.heading("Collection Settings");
            ui.separator();

            egui::Grid::new("collection_settings")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Poll Rate (Hz):");
                    ui.add(
                        egui::DragValue::new(&mut config.collection.poll_rate_hz)
                            .range(1..=10000)
                            .speed(10),
                    );
                    ui.end_row();

                    ui.label("Timeout (ms):");
                    ui.add(
                        egui::DragValue::new(&mut config.collection.timeout_ms)
                            .range(10..=5000)
                            .speed(10),
                    );
                    ui.end_row();

                    ui.label("Max Data Points:");
                    ui.add(
                        egui::DragValue::new(&mut config.collection.max_data_points)
                            .range(1000..=1000000)
                            .speed(1000),
                    );
                    ui.end_row();
                });

            ui.add_space(16.0);
            ui.heading("UI Settings");
            ui.separator();

            egui::Grid::new("ui_settings")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Time Window (s):");
                    ui.add(
                        egui::DragValue::new(&mut config.ui.time_window_seconds)
                            .range(1.0..=120.0)
                            .speed(1.0),
                    );
                    ui.end_row();

                    ui.label("Line Width:");
                    ui.add(
                        egui::DragValue::new(&mut config.ui.line_width)
                            .range(0.5..=5.0)
                            .speed(0.1),
                    );
                    ui.end_row();

                    ui.label("Show Grid:");
                    ui.checkbox(&mut config.ui.show_grid, "");
                    ui.end_row();

                    ui.label("Show Legend:");
                    ui.checkbox(&mut config.ui.show_legend, "");
                    ui.end_row();

                    ui.label("Auto Scale Y:");
                    ui.checkbox(&mut config.ui.auto_scale_y, "");
                    ui.end_row();

                    ui.label("Show Raw Values:");
                    ui.checkbox(&mut config.ui.show_raw_values, "");
                    ui.end_row();
                });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_success_rate_color() {
        // This is a unit test for the logic, not the actual rendering
        let stats = CollectionStats {
            successful_reads: 99,
            failed_reads: 1,
            ..Default::default()
        };
        assert!((stats.success_rate() - 99.0).abs() < 0.1);

        let stats2 = CollectionStats {
            successful_reads: 95,
            failed_reads: 5,
            ..Default::default()
        };
        assert!((stats2.success_rate() - 95.0).abs() < 0.1);
    }
}
