//! Connection settings dialog
//!
//! Extracted from settings.rs probe section.
//! Covers speed, connect-under-reset, halt, memory access, protocol.

use egui::Ui;

use crate::config::{BackendType, ConnectUnderReset, OpenOcdMode, ProbeConfig};
use crate::frontend::dialogs::{Dialog, DialogAction, DialogState, DialogWindowConfig};

/// State for the connection settings dialog
#[derive(Debug, Clone)]
pub struct ConnectionSettingsState {
    pub speed_khz: u32,
    pub connect_under_reset: ConnectUnderReset,
    pub halt_on_connect: bool,
    pub usb_timeout_ms: u64,
    pub bulk_read_gap_threshold: usize,
    pub max_bulk_read_size: usize,
    pub disable_bulk_reads: bool,
    pub backend_type: BackendType,
    pub openocd_path: String,
    pub openocd_interface: String,
    pub openocd_target: String,
    /// True when the user wants to connect to an already-running OpenOCD
    /// rather than spawning a new subprocess.
    pub openocd_mode_external: bool,
    /// Host for External mode. Kept around even in Spawn mode so toggling
    /// back and forth doesn't lose the value.
    pub openocd_external_host: String,
    /// Port for External mode.
    pub openocd_external_port: u16,
}

impl Default for ConnectionSettingsState {
    fn default() -> Self {
        let defaults = ProbeConfig::default();
        Self::from_config(&defaults)
    }
}

impl ConnectionSettingsState {
    /// Create state from the current probe config
    pub fn from_config(config: &ProbeConfig) -> Self {
        // Flatten OpenOcdMode into bool + host/port. Defaults for host/port
        // come from `default_external()` when the user is currently in Spawn
        // mode so the UI has something sensible to show if they flip.
        let (openocd_mode_external, openocd_external_host, openocd_external_port) =
            match &config.openocd_mode {
                OpenOcdMode::Spawn => match OpenOcdMode::default_external() {
                    OpenOcdMode::External { host, port } => (false, host, port),
                    OpenOcdMode::Spawn => unreachable!(),
                },
                OpenOcdMode::External { host, port } => (true, host.clone(), *port),
            };

        Self {
            speed_khz: config.speed_khz,
            connect_under_reset: config.connect_under_reset,
            halt_on_connect: config.halt_on_connect,
            usb_timeout_ms: config.usb_timeout_ms,
            bulk_read_gap_threshold: config.bulk_read_gap_threshold,
            max_bulk_read_size: config.max_bulk_read_size,
            disable_bulk_reads: config.disable_bulk_reads,
            backend_type: config.backend_type,
            openocd_path: config.openocd_path.clone().unwrap_or_default(),
            openocd_interface: config.openocd_interface.clone().unwrap_or_default(),
            openocd_target: config.openocd_target.clone().unwrap_or_default(),
            openocd_mode_external,
            openocd_external_host,
            openocd_external_port,
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
                ui.label("Backend:");
                egui::ComboBox::from_id_salt("conn_settings_backend")
                    .selected_text(state.backend_type.to_string())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.backend_type,
                            BackendType::ProbeRs,
                            "probe-rs",
                        );
                        ui.selectable_value(
                            &mut state.backend_type,
                            BackendType::OpenOcd,
                            "OpenOCD",
                        );
                    });
                ui.end_row();

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
            });

        ui.add_space(4.0);

        // OpenOCD-specific settings (visible when backend is OpenOCD)
        if state.backend_type == BackendType::OpenOcd {
            egui::CollapsingHeader::new("OpenOCD")
                .default_open(true)
                .show(ui, |ui| {
                    egui::Grid::new("conn_settings_openocd_mode_grid")
                        .num_columns(2)
                        .spacing([10.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Mode:");
                            egui::ComboBox::from_id_salt("conn_settings_openocd_mode")
                                .selected_text(if state.openocd_mode_external {
                                    "Connect to existing"
                                } else {
                                    "Spawn new"
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut state.openocd_mode_external,
                                        false,
                                        "Spawn new",
                                    );
                                    ui.selectable_value(
                                        &mut state.openocd_mode_external,
                                        true,
                                        "Connect to existing",
                                    );
                                });
                            ui.end_row();
                        });

                    if state.openocd_mode_external {
                        // External mode: only host + port are relevant.
                        egui::Grid::new("conn_settings_openocd_external_grid")
                            .num_columns(2)
                            .spacing([10.0, 8.0])
                            .show(ui, |ui| {
                                ui.label("Host:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut state.openocd_external_host)
                                        .hint_text("127.0.0.1"),
                                );
                                ui.end_row();

                                ui.label("Port:");
                                ui.add(
                                    egui::DragValue::new(&mut state.openocd_external_port)
                                        .range(1..=65535)
                                        .speed(1),
                                );
                                ui.end_row();
                            });
                    } else {
                        // Spawn mode: subprocess-related overrides.
                        egui::Grid::new("conn_settings_openocd_spawn_grid")
                            .num_columns(2)
                            .spacing([10.0, 8.0])
                            .show(ui, |ui| {
                                ui.label("OpenOCD Path:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut state.openocd_path)
                                        .hint_text("Bundled / System PATH"),
                                );
                                ui.end_row();

                                ui.label("Interface Override:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut state.openocd_interface)
                                        .hint_text("Auto-detect from probe"),
                                );
                                ui.end_row();

                                ui.label("Target Override:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut state.openocd_target)
                                        .hint_text("Auto-detect from chip"),
                                );
                                ui.end_row();
                            });
                    }
                });

            ui.add_space(4.0);
        }

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

                        ui.label("Max Bulk Read Size (bytes):");
                        ui.add(
                            egui::DragValue::new(&mut state.max_bulk_read_size)
                                .range(0..=4096)
                                .speed(32),
                        );
                        ui.end_row();

                        ui.label("Disable Bulk Reads:");
                        ui.checkbox(&mut state.disable_bulk_reads, "")
                            .on_hover_text(
                                "Read each variable individually instead of grouping. \
                                 Helps with probes that have issues with larger reads.",
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
