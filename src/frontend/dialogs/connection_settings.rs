//! Connection settings dialog
//!
//! Extracted from settings.rs probe section.
//! Covers speed, connect-under-reset, halt, memory access, protocol.

use egui::Ui;

use crate::config::{ConnectUnderReset, MemoryAccessMode, ProbeConfig};
use crate::frontend::dialogs::{Dialog, DialogAction, DialogState, DialogWindowConfig};

/// State for the connection settings dialog
#[derive(Debug, Clone)]
pub struct ConnectionSettingsState {
    pub speed_khz: u32,
    pub connect_under_reset: ConnectUnderReset,
    pub halt_on_connect: bool,
    pub memory_access_mode: MemoryAccessMode,
    pub usb_timeout_ms: u64,
    pub bulk_read_gap_threshold: usize,
}

impl Default for ConnectionSettingsState {
    fn default() -> Self {
        let defaults = ProbeConfig::default();
        Self {
            speed_khz: defaults.speed_khz,
            connect_under_reset: defaults.connect_under_reset,
            halt_on_connect: defaults.halt_on_connect,
            memory_access_mode: defaults.memory_access_mode,
            usb_timeout_ms: defaults.usb_timeout_ms,
            bulk_read_gap_threshold: defaults.bulk_read_gap_threshold,
        }
    }
}

impl ConnectionSettingsState {
    /// Create state from the current probe config
    pub fn from_config(config: &ProbeConfig) -> Self {
        Self {
            speed_khz: config.speed_khz,
            connect_under_reset: config.connect_under_reset,
            halt_on_connect: config.halt_on_connect,
            memory_access_mode: config.memory_access_mode,
            usb_timeout_ms: config.usb_timeout_ms,
            bulk_read_gap_threshold: config.bulk_read_gap_threshold,
        }
    }
}

impl DialogState for ConnectionSettingsState {}

/// Actions produced by the connection settings dialog
#[derive(Debug, Clone)]
pub enum ConnectionSettingsAction {
    /// Apply settings to config
    Apply(ConnectionSettingsState),
}

/// Context for rendering
pub struct ConnectionSettingsContext;

/// The connection settings dialog
pub struct ConnectionSettingsDialog;

impl Dialog for ConnectionSettingsDialog {
    type State = ConnectionSettingsState;
    type Action = ConnectionSettingsAction;
    type Context<'a> = ConnectionSettingsContext;

    fn title(_state: &Self::State) -> &'static str {
        "Connection Settings"
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
        egui::Grid::new("connection_settings_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("Speed (kHz):");
                ui.add(
                    egui::DragValue::new(&mut state.speed_khz)
                        .range(100..=50000)
                        .speed(100),
                );
                ui.end_row();

                ui.label("Connect Under Reset:");
                egui::ComboBox::from_id_salt("conn_settings_reset")
                    .selected_text(state.connect_under_reset.to_string())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.connect_under_reset,
                            ConnectUnderReset::None,
                            "None (normal attach)",
                        );
                        ui.selectable_value(
                            &mut state.connect_under_reset,
                            ConnectUnderReset::Software,
                            "Software (SYSRESETREQ)",
                        );
                        ui.selectable_value(
                            &mut state.connect_under_reset,
                            ConnectUnderReset::Hardware,
                            "Hardware (NRST pin)",
                        );
                        ui.selectable_value(
                            &mut state.connect_under_reset,
                            ConnectUnderReset::Core,
                            "Core Reset (VECTRESET)",
                        );
                    });
                ui.end_row();

                ui.label("Halt on Connect:");
                ui.checkbox(&mut state.halt_on_connect, "");
                ui.end_row();

                ui.label("Memory Access:");
                egui::ComboBox::from_id_salt("conn_settings_memory_access")
                    .selected_text(state.memory_access_mode.to_string())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.memory_access_mode,
                            MemoryAccessMode::Background,
                            "Background (Running)",
                        );
                        ui.selectable_value(
                            &mut state.memory_access_mode,
                            MemoryAccessMode::Halted,
                            "Halted (Per-batch)",
                        );
                        ui.selectable_value(
                            &mut state.memory_access_mode,
                            MemoryAccessMode::HaltedPersistent,
                            "Halted (Persistent)",
                        );
                    });
                ui.end_row();
            });

        ui.add_space(4.0);

        egui::CollapsingHeader::new("Advanced")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("conn_settings_advanced_grid")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("USB Timeout (ms):");
                        ui.add(
                            egui::DragValue::new(&mut state.usb_timeout_ms)
                                .range(100..=10000)
                                .speed(100),
                        );
                        ui.end_row();

                        ui.label("Bulk Read Gap (bytes):");
                        ui.add(
                            egui::DragValue::new(&mut state.bulk_read_gap_threshold)
                                .range(0..=1024)
                                .speed(8),
                        );
                        ui.end_row();
                    });
            });

        ui.add_space(8.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                return DialogAction::CloseWithAction(ConnectionSettingsAction::Apply(
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
