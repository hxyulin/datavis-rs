//! Visualizer page - Plot, time controls, legend
//!
//! This page provides:
//! - Top toolbar: Collection controls, stats display, axis controls, trigger controls
//! - Right panel: Variable legend with statistics
//! - Center panel: Real-time plot with cursor support and trigger visualization
//! - Bottom panel: FFT/Frequency analysis view (toggleable)

use egui::{Color32, Context, Ui};
use egui_plot::{HLine, PlotPoint, Polygon, VLine};
use std::collections::HashMap;

use super::Page;
use crate::analysis::{FftAnalyzer, FftConfig, FftResult, WindowFunction};
use crate::session::{SessionMetadata, SessionPlayer, SessionRecorder, SessionState};
use crate::frontend::dialogs::{
    show_dialog, ExportConfigAction, ExportConfigContext, ExportConfigDialog, ExportConfigState,
    TriggerConfigAction, TriggerConfigContext, TriggerConfigDialog, TriggerConfigState,
    ValueEditorAction, ValueEditorContext, ValueEditorDialog, ValueEditorState,
};
use crate::frontend::markers::{MarkerManager, MarkerType};
use crate::frontend::plot::{PlotCursor, PlotStatistics};
use crate::frontend::state::{AppAction, SharedState};
use crate::types::ConnectionStatus;

/// State specific to the Visualizer page
pub struct VisualizerPageState {
    /// Advanced mode shows all controls (triggers, markers, FFT, cursors, etc.)
    pub advanced_mode: bool,
    /// Value editor dialog state (for editing values from legend)
    pub value_editor_open: bool,
    pub value_editor_state: ValueEditorState,
    /// Trigger configuration dialog state
    pub trigger_config_open: bool,
    pub trigger_config_state: TriggerConfigState,
    /// Export configuration dialog state
    pub export_config_open: bool,
    pub export_config_state: ExportConfigState,
    /// Cursor state for data inspection
    pub cursor: PlotCursor,
    /// Whether to show the statistics panel
    pub show_statistics_panel: bool,
    /// Cached statistics per variable (updated on cursor move or range change)
    pub variable_statistics: HashMap<u32, PlotStatistics>,
    /// Marker manager for bookmarks
    pub markers: MarkerManager,
    /// Whether markers panel is expanded
    pub show_markers_panel: bool,
    /// New marker name input
    pub new_marker_name: String,
    /// New marker type selection
    pub new_marker_type: MarkerType,
    /// Whether to show FFT panel
    pub show_fft_panel: bool,
    /// FFT analyzer
    pub fft_analyzer: FftAnalyzer,
    /// FFT configuration
    pub fft_config: FftConfig,
    /// Selected variable for FFT analysis
    pub fft_variable_id: Option<u32>,
    /// Cached FFT result
    pub fft_result: Option<FftResult>,
    /// Whether to show FFT in dB scale
    pub fft_db_scale: bool,
    /// Whether to use Welch's method (averaged FFT)
    pub fft_averaged: bool,
    /// Whether to enable secondary Y-axis
    pub enable_secondary_axis: bool,
    /// Y-axis bounds for secondary axis (if manual)
    pub secondary_y_min: Option<f64>,
    pub secondary_y_max: Option<f64>,
    /// Whether secondary Y-axis is autoscaled
    pub secondary_autoscale_y: bool,
    /// Session recorder
    pub session_recorder: SessionRecorder,
    /// Session player
    pub session_player: SessionPlayer,
    /// Session name input for recording
    pub session_name: String,
    /// Threshold lines for reference
    pub threshold_lines: Vec<ThresholdLine>,
    /// Input for new threshold value
    pub new_threshold_value: String,
    /// Color for new threshold line
    pub new_threshold_color: [u8; 4],
}

/// A horizontal threshold/reference line
#[derive(Debug, Clone)]
pub struct ThresholdLine {
    /// Unique ID
    pub id: u32,
    /// Y-axis value
    pub value: f64,
    /// Display label
    pub label: String,
    /// Line color (RGBA)
    pub color: [u8; 4],
    /// Whether the line is visible
    pub visible: bool,
}

impl ThresholdLine {
    /// Create a new threshold line
    pub fn new(id: u32, value: f64, label: impl Into<String>, color: [u8; 4]) -> Self {
        Self {
            id,
            value,
            label: label.into(),
            color,
            visible: true,
        }
    }
}

impl Default for VisualizerPageState {
    fn default() -> Self {
        Self {
            advanced_mode: false,
            value_editor_open: false,
            value_editor_state: ValueEditorState::default(),
            trigger_config_open: false,
            trigger_config_state: TriggerConfigState::default(),
            export_config_open: false,
            export_config_state: ExportConfigState::default(),
            cursor: PlotCursor::default(),
            show_statistics_panel: false,
            variable_statistics: HashMap::new(),
            markers: MarkerManager::default(),
            show_markers_panel: false,
            new_marker_name: String::new(),
            new_marker_type: MarkerType::default(),
            show_fft_panel: false,
            fft_analyzer: FftAnalyzer::new(),
            fft_config: FftConfig::default(),
            fft_variable_id: None,
            fft_result: None,
            fft_db_scale: true,
            fft_averaged: true,
            enable_secondary_axis: false,
            secondary_y_min: None,
            secondary_y_max: None,
            secondary_autoscale_y: true,
            session_recorder: SessionRecorder::new(),
            session_player: SessionPlayer::new(),
            session_name: String::new(),
            threshold_lines: Vec::new(),
            new_threshold_value: String::new(),
            new_threshold_color: [255, 200, 0, 255], // Yellow default
        }
    }
}

/// Counter for unique threshold line IDs
static NEXT_THRESHOLD_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

pub struct VisualizerPage;

impl Page for VisualizerPage {
    type State = VisualizerPageState;

    fn render(
        state: &mut Self::State,
        shared: &mut SharedState<'_>,
        ctx: &Context,
    ) -> Vec<AppAction> {
        let mut actions = Vec::new();

        // Top toolbar
        egui::TopBottomPanel::top("visualizer_toolbar").show(ctx, |ui| {
            Self::render_toolbar(state, shared, ui, &mut actions);
        });

        // Right panel: Variable legend
        egui::SidePanel::right("visualizer_legend")
            .default_width(220.0)
            .resizable(true)
            .show(ctx, |ui| {
                Self::render_legend(state, shared, ui);
            });

        // Bottom panel: Statistics (if enabled and has cursor range)
        if state.show_statistics_panel {
            egui::TopBottomPanel::bottom("visualizer_statistics")
                .default_height(100.0)
                .resizable(true)
                .show(ctx, |ui| {
                    Self::render_statistics_panel(state, shared, ui);
                });
        }

        // Bottom panel: FFT (if enabled)
        if state.show_fft_panel {
            egui::TopBottomPanel::bottom("visualizer_fft")
                .default_height(200.0)
                .resizable(true)
                .show(ctx, |ui| {
                    Self::render_fft_panel(state, shared, ui);
                });
        }

        // Center panel: Plot
        egui::CentralPanel::default().show(ctx, |ui| {
            Self::render_plot(state, shared, ui);
        });

        // Render value editor dialog if open
        Self::render_dialogs(state, shared, ctx, &mut actions);

        actions
    }
}

impl VisualizerPage {
    fn render_toolbar(
        state: &mut VisualizerPageState,
        shared: &mut SharedState<'_>,
        ui: &mut Ui,
        actions: &mut Vec<AppAction>,
    ) {
        if state.advanced_mode {
            Self::render_toolbar_advanced(state, shared, ui, actions);
        } else {
            Self::render_toolbar_simple(state, shared, ui, actions);
        }
    }

    /// Simple toolbar for basic mode
    fn render_toolbar_simple(
        state: &mut VisualizerPageState,
        shared: &mut SharedState<'_>,
        ui: &mut Ui,
        actions: &mut Vec<AppAction>,
    ) {
        ui.horizontal(|ui| {
            // Collection controls
            if shared.connection_status == ConnectionStatus::Connected {
                if shared.settings.collecting {
                    if ui.button("Stop").clicked() {
                        actions.push(AppAction::StopCollection);
                    }
                } else if ui.button("Start").clicked() {
                    actions.push(AppAction::StartCollection);
                }
            } else {
                ui.add_enabled(false, egui::Button::new("Start"));
                ui.label("Connect to probe first");
            }

            ui.separator();

            // Clear data
            if ui.button("Clear").clicked() {
                actions.push(AppAction::ClearData);
            }

            ui.separator();

            // Simple rate display
            let actual_rate = shared.stats.effective_sample_rate;
            let rate_color = if actual_rate > 0.0 {
                Color32::from_rgb(100, 255, 100)
            } else {
                Color32::GRAY
            };
            ui.label("Rate:");
            ui.colored_label(rate_color, format!("{:.0} Hz", actual_rate));

            ui.separator();

            // Simple axis controls
            if ui
                .selectable_label(shared.settings.autoscale_y, "Auto Y")
                .on_hover_text("Auto-scale Y axis")
                .clicked()
            {
                shared.settings.toggle_autoscale_y();
            }

            if ui.button("Reset View").clicked() {
                shared.settings.autoscale_x = true;
                shared.settings.autoscale_y = true;
                shared.settings.follow_latest = true;
                shared.settings.lock_x = false;
                shared.settings.lock_y = false;
                shared.settings.x_min = None;
                shared.settings.x_max = None;
                shared.settings.y_min = None;
                shared.settings.y_max = None;
                shared.settings.display_time_window = 10.0;
            }

            // Advanced mode toggle (right-aligned)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(&mut state.advanced_mode, "Advanced");
            });
        });
    }

    /// Full toolbar for advanced mode
    fn render_toolbar_advanced(
        state: &mut VisualizerPageState,
        shared: &mut SharedState<'_>,
        ui: &mut Ui,
        actions: &mut Vec<AppAction>,
    ) {
        // First row: Collection controls and stats
        ui.horizontal(|ui| {
            // Collection controls
            if shared.connection_status == ConnectionStatus::Connected {
                if shared.settings.collecting {
                    if ui.button("Stop").clicked() {
                        actions.push(AppAction::StopCollection);
                    }
                } else if ui.button("Start").clicked() {
                    actions.push(AppAction::StartCollection);
                }
            } else {
                ui.add_enabled(false, egui::Button::new("Start"));
                ui.label("Connect to a probe first");
            }

            ui.separator();

            // Clear data button
            if ui.button("Clear").clicked() {
                actions.push(AppAction::ClearData);
            }

            // Export button
            if ui
                .button("Export...")
                .on_hover_text("Export data to file")
                .clicked()
            {
                // Initialize export state with current data info
                state.export_config_state = ExportConfigState::default();
                if let Some((start, end)) = state.cursor.time_range() {
                    state.export_config_state.set_cursor_range(start, end);
                }
                state.export_config_open = true;
            }

            ui.separator();

            // Stats display
            let target_rate = shared.config.collection.poll_rate_hz as f64;
            let actual_rate = shared.stats.effective_sample_rate;
            let is_throttled = actual_rate > 0.0 && actual_rate < target_rate * 0.9;
            let rate_color = if is_throttled {
                Color32::from_rgb(255, 100, 100)
            } else if actual_rate > 0.0 {
                Color32::from_rgb(100, 255, 100)
            } else {
                Color32::GRAY
            };

            ui.horizontal(|ui| {
                ui.label("Rate:");
                ui.colored_label(rate_color, format!("{:.0} Hz", actual_rate));
                if is_throttled {
                    ui.colored_label(
                        Color32::from_rgb(255, 200, 100),
                        format!("(target: {} Hz)", shared.config.collection.poll_rate_hz),
                    );
                }
                ui.label(format!("| Success: {:.1}%", shared.stats.success_rate()));

                // Show latency stats if we have data
                if shared.stats.avg_read_time_us > 0.0 {
                    ui.separator();
                    let avg_ms = shared.stats.avg_read_time_us / 1000.0;
                    let jitter_ms = shared.stats.jitter_us as f64 / 1000.0;

                    // Color based on jitter - high jitter (>50% of avg) is concerning
                    let jitter_color = if jitter_ms > avg_ms * 0.5 {
                        Color32::from_rgb(255, 150, 50) // Orange for high jitter
                    } else {
                        Color32::GRAY
                    };

                    ui.label(format!("Latency: {:.1}ms", avg_ms));
                    ui.colored_label(jitter_color, format!("(±{:.1}ms)", jitter_ms))
                        .on_hover_text(format!(
                            "Latency jitter (min: {:.1}ms, max: {:.1}ms)",
                            shared.stats.min_latency_us as f64 / 1000.0,
                            shared.stats.max_latency_us as f64 / 1000.0
                        ));
                }

                // Show bulk read optimization stats
                if shared.stats.reads_saved_by_bulk > 0 {
                    ui.separator();
                    ui.label(format!("Bulk: {} reads saved", shared.stats.reads_saved_by_bulk))
                        .on_hover_text(format!(
                            "Bulk read optimization: {} regions read instead of {} individual reads",
                            shared.stats.bulk_reads,
                            shared.stats.bulk_reads + shared.stats.reads_saved_by_bulk
                        ));
                }

                if !shared.stats.memory_access_mode.is_empty() {
                    ui.separator();
                    ui.label(format!("Mode: {}", shared.stats.memory_access_mode));
                }
                // Show dropped messages warning if backpressure occurred
                if shared.stats.dropped_messages > 0 {
                    ui.separator();
                    ui.colored_label(
                        Color32::from_rgb(255, 150, 50),
                        format!("Dropped: {}", shared.stats.dropped_messages),
                    )
                    .on_hover_text("Messages dropped due to UI not keeping up with data rate");
                }
            });

            // Advanced mode toggle (right-aligned)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(&mut state.advanced_mode, "Advanced");
            });
        });

        // Second row: Axis controls
        ui.horizontal(|ui| {
            // X-axis controls
            ui.label("X-Axis:");

            let autoscale_x_text = if shared.settings.autoscale_x {
                "Auto"
            } else {
                "Manual"
            };
            if ui
                .selectable_label(shared.settings.autoscale_x, autoscale_x_text)
                .on_hover_text("Auto-scale X axis (follow latest data)")
                .clicked()
            {
                shared.settings.toggle_autoscale_x();
            }

            let lock_x_text = if shared.settings.lock_x {
                "Locked"
            } else {
                "Unlocked"
            };
            if ui
                .selectable_label(shared.settings.lock_x, lock_x_text)
                .on_hover_text(if shared.settings.lock_x {
                    "X-axis locked (click to unlock)"
                } else {
                    "X-axis unlocked (click to lock)"
                })
                .clicked()
            {
                shared.settings.toggle_lock_x();
            }

            ui.separator();

            // Time window control (only enabled when not autoscaling X)
            ui.add_enabled_ui(!shared.settings.autoscale_x, |ui| {
                ui.label("Time window:");
                let max_window = shared.settings.max_time_window;
                if ui
                    .add(
                        egui::Slider::new(&mut shared.settings.display_time_window, 0.5..=max_window)
                            .suffix("s")
                            .logarithmic(true),
                    )
                    .changed()
                {
                    shared.settings.display_time_window =
                        shared.settings.display_time_window.clamp(0.1, max_window);
                }
            });

            ui.separator();

            // Y-axis controls
            ui.label("Y-Axis:");

            let autoscale_y_text = if shared.settings.autoscale_y {
                "Auto"
            } else {
                "Manual"
            };
            if ui
                .selectable_label(shared.settings.autoscale_y, autoscale_y_text)
                .on_hover_text("Auto-scale Y axis (fit to visible data)")
                .clicked()
            {
                shared.settings.toggle_autoscale_y();
            }

            let lock_y_text = if shared.settings.lock_y {
                "Locked"
            } else {
                "Unlocked"
            };
            if ui
                .selectable_label(shared.settings.lock_y, lock_y_text)
                .on_hover_text(if shared.settings.lock_y {
                    "Y-axis locked (click to unlock)"
                } else {
                    "Y-axis unlocked (click to lock)"
                })
                .clicked()
            {
                shared.settings.toggle_lock_y();
            }

            ui.separator();

            // Reset view button
            if ui
                .button("Reset View")
                .on_hover_text("Reset to autoscale on both axes")
                .clicked()
            {
                shared.settings.autoscale_x = true;
                shared.settings.autoscale_y = true;
                shared.settings.follow_latest = true;
                shared.settings.lock_x = false;
                shared.settings.lock_y = false;
                shared.settings.x_min = None;
                shared.settings.x_max = None;
                shared.settings.y_min = None;
                shared.settings.y_max = None;
                shared.settings.display_time_window = 10.0;
                state.secondary_autoscale_y = true;
                state.secondary_y_min = None;
                state.secondary_y_max = None;
            }

            ui.separator();

            // Secondary Y-axis controls
            let secondary_text = if state.enable_secondary_axis { "Y2: On" } else { "Y2: Off" };
            if ui
                .selectable_label(state.enable_secondary_axis, secondary_text)
                .on_hover_text("Enable secondary Y-axis (right side) for variables with different scales")
                .clicked()
            {
                state.enable_secondary_axis = !state.enable_secondary_axis;
            }

            if state.enable_secondary_axis {
                let y2_autoscale_text = if state.secondary_autoscale_y { "Auto" } else { "Manual" };
                if ui
                    .selectable_label(state.secondary_autoscale_y, y2_autoscale_text)
                    .on_hover_text("Auto-scale secondary Y axis")
                    .clicked()
                {
                    state.secondary_autoscale_y = !state.secondary_autoscale_y;
                }
            }
        });

        // Third row: Cursor and statistics controls
        ui.horizontal(|ui| {
            ui.label("Cursor:");

            // Toggle cursor mode
            let cursor_text = if state.cursor.enabled {
                "Enabled"
            } else {
                "Disabled"
            };
            if ui
                .selectable_label(state.cursor.enabled, cursor_text)
                .on_hover_text("Enable cursor mode for data inspection")
                .clicked()
            {
                state.cursor.enabled = !state.cursor.enabled;
            }

            ui.add_enabled_ui(state.cursor.enabled, |ui| {
                // Set cursor A
                if ui
                    .button("Set A")
                    .on_hover_text("Set cursor A at current position (or click on plot)")
                    .clicked()
                {
                    state.cursor.set_cursor_a();
                }

                // Set cursor B
                if ui
                    .button("Set B")
                    .on_hover_text("Set cursor B at current position")
                    .clicked()
                {
                    state.cursor.set_cursor_b();
                }

                // Clear cursors
                if ui
                    .button("Clear")
                    .on_hover_text("Clear both cursors")
                    .clicked()
                {
                    state.cursor.clear_cursors();
                    state.variable_statistics.clear();
                }

                // Show delta if both cursors set
                if let Some(dt) = state.cursor.time_delta() {
                    ui.separator();
                    ui.label(format!("ΔT: {:.3}s", dt));
                    if dt > 0.0 {
                        ui.label(format!("({:.1} Hz)", 1.0 / dt));
                    }
                }
            });

            ui.separator();

            // Statistics panel toggle
            let stats_text = if state.show_statistics_panel {
                "Stats: On"
            } else {
                "Stats: Off"
            };
            if ui
                .selectable_label(state.show_statistics_panel, stats_text)
                .on_hover_text("Show statistics panel for cursor range")
                .clicked()
            {
                state.show_statistics_panel = !state.show_statistics_panel;
            }

            ui.separator();

            // Trigger controls
            ui.label("Trigger:");

            // Status indicator
            let trigger_enabled = shared.settings.trigger.enabled;
            let trigger_armed = shared.settings.trigger.armed;
            let trigger_triggered = shared.settings.trigger.triggered;

            let (status_text, status_color) = if trigger_triggered {
                ("TRIGGERED", Color32::from_rgb(100, 255, 100))
            } else if trigger_armed {
                ("ARMED", Color32::from_rgb(255, 255, 100))
            } else if trigger_enabled {
                ("Ready", Color32::from_rgb(150, 150, 255))
            } else {
                ("Off", Color32::GRAY)
            };
            ui.colored_label(status_color, status_text);

            // Quick arm/disarm button
            if trigger_enabled {
                if trigger_armed {
                    if ui.button("Disarm").clicked() {
                        shared.settings.trigger.disarm();
                    }
                } else if ui.button("Arm").clicked() {
                    shared.settings.trigger.arm();
                }

                if trigger_triggered {
                    if ui.button("Reset").clicked() {
                        shared.settings.trigger.reset();
                    }
                }
            }

            // Config button
            if ui.button("Config...").clicked() {
                state.trigger_config_state =
                    TriggerConfigState::from_settings(&shared.settings.trigger);
                state.trigger_config_open = true;
            }
        });

        // Fourth row: Markers
        ui.horizontal(|ui| {
            ui.label("Markers:");

            // Add marker button
            let current_time = shared.start_time.elapsed();
            if ui
                .button("Add")
                .on_hover_text("Add marker at current time")
                .clicked()
            {
                let name = if state.new_marker_name.is_empty() {
                    format!("Marker {}", state.markers.len() + 1)
                } else {
                    std::mem::take(&mut state.new_marker_name)
                };
                state.markers.add(name, current_time, state.new_marker_type);
            }

            // Marker type selector
            egui::ComboBox::from_id_salt("marker_type_selector")
                .selected_text(state.new_marker_type.display_name())
                .width(80.0)
                .show_ui(ui, |ui| {
                    for marker_type in MarkerType::all() {
                        ui.selectable_value(
                            &mut state.new_marker_type,
                            *marker_type,
                            marker_type.display_name(),
                        );
                    }
                });

            // Quick name input
            ui.add(
                egui::TextEdit::singleline(&mut state.new_marker_name)
                    .hint_text("Marker name...")
                    .desired_width(100.0),
            );

            ui.separator();

            // Navigation
            if !state.markers.is_empty() {
                // Previous marker
                if ui.button("<").on_hover_text("Jump to previous marker").clicked() {
                    if let Some(marker) = state.markers.prev_before(current_time) {
                        // Jump to marker time
                        let marker_time = marker.time_secs();
                        shared.settings.autoscale_x = false;
                        let window = shared.settings.display_time_window;
                        shared.settings.x_min = Some(marker_time - window / 2.0);
                        shared.settings.x_max = Some(marker_time + window / 2.0);
                    }
                }

                // Next marker
                if ui.button(">").on_hover_text("Jump to next marker").clicked() {
                    if let Some(marker) = state.markers.next_after(current_time) {
                        // Jump to marker time
                        let marker_time = marker.time_secs();
                        shared.settings.autoscale_x = false;
                        let window = shared.settings.display_time_window;
                        shared.settings.x_min = Some(marker_time - window / 2.0);
                        shared.settings.x_max = Some(marker_time + window / 2.0);
                    }
                }

                // Marker count
                ui.label(format!("({} markers)", state.markers.len()));

                // Toggle markers panel
                let panel_text = if state.show_markers_panel { "Hide" } else { "Show" };
                if ui.button(panel_text).on_hover_text("Toggle markers panel").clicked() {
                    state.show_markers_panel = !state.show_markers_panel;
                }

                // Clear all
                if ui.button("Clear All").on_hover_text("Remove all markers").clicked() {
                    state.markers.clear();
                }
            }

            ui.separator();

            // FFT controls
            ui.label("FFT:");
            let fft_text = if state.show_fft_panel { "On" } else { "Off" };
            if ui
                .selectable_label(state.show_fft_panel, fft_text)
                .on_hover_text("Toggle FFT frequency analysis panel")
                .clicked()
            {
                state.show_fft_panel = !state.show_fft_panel;
            }
        });

        // Fifth row: Session recording and playback
        ui.horizontal(|ui| {
            ui.label("Session:");

            let recorder_state = state.session_recorder.state();
            let player_state = state.session_player.state();

            // Recording controls
            if player_state == SessionState::Idle || player_state == SessionState::Stopped {
                match recorder_state {
                    SessionState::Idle => {
                        // Name input for new recording
                        ui.add(
                            egui::TextEdit::singleline(&mut state.session_name)
                                .hint_text("Session name...")
                                .desired_width(100.0),
                        );

                        if ui
                            .button("Record")
                            .on_hover_text("Start recording session")
                            .clicked()
                        {
                            let name = if state.session_name.is_empty() {
                                format!("Session {}", chrono::Local::now().format("%Y-%m-%d %H:%M"))
                            } else {
                                std::mem::take(&mut state.session_name)
                            };
                            let mut metadata = SessionMetadata::new(name);
                            metadata.poll_rate_hz = shared.config.collection.poll_rate_hz;
                            state.session_recorder.set_variables(&shared.config.variables);
                            state.session_recorder.start_recording(metadata);
                        }
                    }
                    SessionState::Recording => {
                        // Recording indicator
                        ui.colored_label(Color32::from_rgb(255, 100, 100), "REC");
                        ui.label(format!(
                            "{:.1}s ({} frames)",
                            state.session_recorder.recording_duration().as_secs_f64(),
                            state.session_recorder.frame_count()
                        ));

                        if ui.button("Stop").on_hover_text("Stop recording").clicked() {
                            state.session_recorder.stop_recording();
                        }

                        if ui.button("Cancel").on_hover_text("Cancel recording").clicked() {
                            state.session_recorder.cancel_recording();
                        }
                    }
                    SessionState::Stopped => {
                        // Recording stopped, offer to play or save
                        ui.label(format!(
                            "Recorded: {:.1}s",
                            state.session_recorder.recording().duration().as_secs_f64()
                        ));

                        if ui.button("Play").on_hover_text("Play back recording").clicked() {
                            let recording = state.session_recorder.take_recording();
                            state.session_player.load(recording);
                            state.session_player.play();
                        }

                        if ui.button("Save").on_hover_text("Save recording to file").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_title("Save Session Recording")
                                .add_filter("JSON Session", &["json"])
                                .save_file()
                            {
                                if let Err(e) = state.session_recorder.recording().save_to_file(&path) {
                                    tracing::error!("Failed to save session: {}", e);
                                }
                            }
                        }

                        if ui.button("Discard").on_hover_text("Discard recording").clicked() {
                            state.session_recorder.cancel_recording();
                        }
                    }
                    _ => {}
                }
            }

            ui.separator();

            // Playback controls
            if state.session_player.has_recording() {
                match player_state {
                    SessionState::Playing => {
                        ui.colored_label(Color32::from_rgb(100, 255, 100), "PLAY");

                        if ui.button("||").on_hover_text("Pause playback").clicked() {
                            state.session_player.pause();
                        }

                        if ui.button("Stop").on_hover_text("Stop playback").clicked() {
                            state.session_player.stop();
                        }
                    }
                    SessionState::Paused => {
                        ui.colored_label(Color32::from_rgb(255, 255, 100), "PAUSED");

                        if ui.button(">").on_hover_text("Resume playback").clicked() {
                            state.session_player.play();
                        }

                        if ui.button("Stop").on_hover_text("Stop playback").clicked() {
                            state.session_player.stop();
                        }
                    }
                    SessionState::Stopped => {
                        if ui.button(">").on_hover_text("Play recording").clicked() {
                            state.session_player.play();
                        }

                        if ui.button("Unload").on_hover_text("Unload recording").clicked() {
                            state.session_player.unload();
                        }
                    }
                    _ => {}
                }

                // Progress display and seek
                let progress = state.session_player.progress();
                let total = state.session_player.total_duration();
                let current = state.session_player.current_time();

                ui.label(format!(
                    "{:.1}s / {:.1}s",
                    current.as_secs_f64(),
                    total.as_secs_f64()
                ));

                // Progress slider
                let mut progress_slider = (progress * 100.0) as f32;
                if ui
                    .add(egui::Slider::new(&mut progress_slider, 0.0..=100.0).show_value(false))
                    .changed()
                {
                    state.session_player.seek_progress(progress_slider as f64 / 100.0);
                }

                // Speed control
                ui.label("Speed:");
                let mut speed = state.session_player.playback_speed() as f32;
                if ui
                    .add(egui::Slider::new(&mut speed, 0.1..=4.0).suffix("x"))
                    .changed()
                {
                    state.session_player.set_playback_speed(speed as f64);
                }

                // Loop toggle
                let mut loop_enabled = state.session_player.loop_playback();
                if ui.checkbox(&mut loop_enabled, "Loop").changed() {
                    state.session_player.set_loop_playback(loop_enabled);
                }
            } else {
                // Load session button
                if ui.button("Load").on_hover_text("Load session from file").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title("Load Session Recording")
                        .add_filter("JSON Session", &["json"])
                        .pick_file()
                    {
                        match crate::session::SessionRecording::load_from_file(&path) {
                            Ok(recording) => {
                                state.session_player.load(recording);
                            }
                            Err(e) => {
                                tracing::error!("Failed to load session: {}", e);
                            }
                        }
                    }
                }
            }
        });
    }

    fn render_legend(state: &mut VisualizerPageState, shared: &mut SharedState<'_>, ui: &mut Ui) {
        ui.heading("Variables");
        ui.separator();

        if state.advanced_mode {
            Self::render_legend_advanced(state, shared, ui);
        } else {
            Self::render_legend_simple(shared, ui);
        }
    }

    /// Simple legend for basic mode - just color, name, and value
    fn render_legend_simple(shared: &SharedState<'_>, ui: &mut Ui) {
        for var in &shared.config.variables {
            if !var.enabled {
                continue;
            }

            let var_color = var.color;
            let color = Color32::from_rgba_unmultiplied(
                var_color[0],
                var_color[1],
                var_color[2],
                var_color[3],
            );

            ui.horizontal(|ui| {
                // Color swatch
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, color);

                // Variable name
                ui.label(&var.name);
            });

            // Current value only
            if let Some(data) = shared.variable_data.get(&var.id) {
                if let Some(last) = data.last() {
                    let value_text = if var.unit.is_empty() {
                        format!("{:.3}", last.converted_value)
                    } else {
                        format!("{:.3} {}", last.converted_value, var.unit)
                    };
                    ui.indent(var.id, |ui| {
                        ui.label(egui::RichText::new(value_text).monospace().color(color));
                    });
                }
            }

            ui.add_space(4.0);
        }
    }

    /// Advanced legend with full stats and controls
    fn render_legend_advanced(state: &mut VisualizerPageState, shared: &mut SharedState<'_>, ui: &mut Ui) {
        // Collect axis toggle requests and context menu actions to avoid borrowing issues
        let mut axis_toggle_requests: Vec<u32> = Vec::new();
        let mut toggle_visibility_requests: Vec<u32> = Vec::new();
        let mut clear_data_requests: Vec<u32> = Vec::new();
        let mut remove_variable_requests: Vec<u32> = Vec::new();
        let mut plot_style_cycle_requests: Vec<u32> = Vec::new();

        for var in &shared.config.variables {
            if !var.enabled {
                continue;
            }

            // Get error state for this variable
            let has_errors = shared
                .variable_data
                .get(&var.id)
                .map(|d| d.has_errors())
                .unwrap_or(false);

            let var_id = var.id;
            let var_y_axis = var.y_axis;
            let var_color = var.color;
            let var_name = var.name.clone();
            let var_show_in_graph = var.show_in_graph;
            let var_plot_style = var.plot_style;

            ui.horizontal(|ui| {
                let color = Color32::from_rgba_unmultiplied(
                    var_color[0],
                    var_color[1],
                    var_color[2],
                    var_color[3],
                );
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, color);

                // Variable name with context menu
                let label_response = ui.add(
                    egui::Label::new(
                        egui::RichText::new(&var_name).color(if var_show_in_graph {
                            ui.style().visuals.text_color()
                        } else {
                            ui.style().visuals.weak_text_color()
                        }),
                    )
                    .sense(egui::Sense::click()),
                );

                // Context menu on right-click
                label_response.context_menu(|ui| {
                    let visibility_text = if var_show_in_graph {
                        "Hide from Graph"
                    } else {
                        "Show in Graph"
                    };
                    if ui.button(visibility_text).clicked() {
                        toggle_visibility_requests.push(var_id);
                        ui.close();
                    }

                    if ui.button("Clear Data").clicked() {
                        clear_data_requests.push(var_id);
                        ui.close();
                    }

                    ui.separator();

                    if ui
                        .button(egui::RichText::new("Remove Variable").color(Color32::from_rgb(255, 100, 100)))
                        .clicked()
                    {
                        remove_variable_requests.push(var_id);
                        ui.close();
                    }
                });

                // Show Y-axis selector if secondary axis is enabled
                if state.enable_secondary_axis {
                    let axis_label = if var_y_axis == 0 { "Y1" } else { "Y2" };
                    let axis_color = if var_y_axis == 0 {
                        Color32::from_rgb(100, 150, 255)
                    } else {
                        Color32::from_rgb(255, 150, 100)
                    };
                    if ui
                        .add(
                            egui::Label::new(egui::RichText::new(axis_label).small().color(axis_color))
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_text("Click to switch Y-axis (Y1=left, Y2=right)")
                        .clicked()
                    {
                        axis_toggle_requests.push(var_id);
                    }
                }

                // Plot style selector (clickable to cycle through styles)
                {
                    use crate::types::PlotStyle;
                    let style_icon = match var_plot_style {
                        PlotStyle::Line => "─",
                        PlotStyle::Scatter => "•",
                        PlotStyle::Step => "⌐",
                        PlotStyle::Area => "▄",
                    };
                    if ui
                        .add(
                            egui::Label::new(egui::RichText::new(style_icon).small())
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_text(format!(
                            "Plot style: {} (click to change)",
                            var_plot_style.display_name()
                        ))
                        .clicked()
                    {
                        plot_style_cycle_requests.push(var_id);
                    }
                }

                // Show error indicator if there are errors
                if has_errors {
                    let error_response = ui.colored_label(Color32::from_rgb(255, 80, 80), " !");
                    if let Some(data) = shared.variable_data.get(&var_id) {
                        let tooltip = if let Some(ref last_error) = data.last_error {
                            format!(
                                "{} errors ({:.1}% rate)\nLast: {}",
                                data.error_count,
                                data.error_rate(),
                                last_error
                            )
                        } else {
                            format!("{} errors ({:.1}% rate)", data.error_count, data.error_rate())
                        };
                        error_response.on_hover_text(tooltip);
                    }
                }
            });

            // Show stats
            let is_writable = var.is_writable();
            let is_connected = shared.connection_status == ConnectionStatus::Connected;
            let can_edit = is_writable && is_connected;
            let var_id = var.id;

            if let Some(data) = shared.variable_data.get(&var.id) {
                ui.indent(var.id, |ui| {
                    if let Some(last) = data.last() {
                        let value_text = format!("Value: {:.3}", last.converted_value);
                        let response = ui.add(
                            egui::Label::new(
                                egui::RichText::new(&value_text).color(if can_edit {
                                    egui::Color32::WHITE
                                } else {
                                    egui::Color32::GRAY
                                }),
                            )
                            .sense(egui::Sense::click()),
                        );

                        if can_edit {
                            let response = response.on_hover_text("Double-click to edit value");
                            if response.double_clicked() {
                                state.value_editor_state = ValueEditorState::for_variable(var_id);
                                state.value_editor_state.input = format!("{}", last.raw_value);
                                state.value_editor_open = true;
                            }
                        } else if !is_writable {
                            response
                                .on_hover_text("Cannot edit: has converter or non-primitive type");
                        } else {
                            response.on_hover_text("Cannot edit: not connected");
                        }
                    }
                    let stats = data.statistics();
                    ui.label(format!("Min: {:.3}", stats.0));
                    ui.label(format!("Max: {:.3}", stats.1));
                    ui.label(format!("Avg: {:.3}", stats.2));
                    ui.label(
                        egui::RichText::new(format!("Points: {}", data.data_points.len()))
                            .small()
                            .weak(),
                    );

                    // Show error count if there are errors
                    if data.has_errors() {
                        ui.colored_label(
                            Color32::from_rgb(255, 100, 100),
                            format!("Errors: {} ({:.1}%)", data.error_count, data.error_rate()),
                        );
                    }
                });
            }

            ui.separator();
        }

        // Apply axis toggle requests
        for var_id in axis_toggle_requests {
            if let Some(v) = shared.config.variables.iter_mut().find(|v| v.id == var_id) {
                v.y_axis = if v.y_axis == 0 { 1 } else { 0 };
            }
        }

        // Apply visibility toggle requests
        for var_id in toggle_visibility_requests {
            if let Some(v) = shared.config.variables.iter_mut().find(|v| v.id == var_id) {
                v.show_in_graph = !v.show_in_graph;
            }
        }

        // Apply clear data requests
        for var_id in clear_data_requests {
            if let Some(data) = shared.variable_data.get_mut(&var_id) {
                data.clear();
            }
        }

        // Apply remove variable requests
        for var_id in remove_variable_requests {
            shared.config.variables.retain(|v| v.id != var_id);
            shared.variable_data.remove(&var_id);
        }

        // Apply plot style cycle requests
        for var_id in plot_style_cycle_requests {
            use crate::types::PlotStyle;
            if let Some(v) = shared.config.variables.iter_mut().find(|v| v.id == var_id) {
                // Cycle through: Line -> Scatter -> Step -> Area -> Line
                v.plot_style = match v.plot_style {
                    PlotStyle::Line => PlotStyle::Scatter,
                    PlotStyle::Scatter => PlotStyle::Step,
                    PlotStyle::Step => PlotStyle::Area,
                    PlotStyle::Area => PlotStyle::Line,
                };
            }
        }

        // Markers section
        if state.show_markers_panel && !state.markers.is_empty() {
            ui.add_space(8.0);
            ui.heading("Markers");
            ui.separator();

            let mut marker_to_remove: Option<u32> = None;
            let mut marker_to_jump: Option<f64> = None;

            for marker in state.markers.all() {
                ui.horizontal(|ui| {
                    // Color indicator
                    let color = marker.color();
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, color);

                    // Name and time
                    let label = ui.add(
                        egui::Label::new(
                            egui::RichText::new(&marker.name).small()
                        )
                        .sense(egui::Sense::click())
                    );

                    if label.clicked() {
                        marker_to_jump = Some(marker.time_secs());
                    }
                    label.on_hover_text(format!(
                        "T: {:.3}s\nType: {}\n{}Click to jump",
                        marker.time_secs(),
                        marker.marker_type.display_name(),
                        if let Some(ref desc) = marker.description {
                            format!("{}\n", desc)
                        } else {
                            String::new()
                        }
                    ));

                    // Time display
                    ui.label(egui::RichText::new(format!("{:.2}s", marker.time_secs())).small().weak());

                    // Delete button
                    if ui.small_button("x").on_hover_text("Remove marker").clicked() {
                        marker_to_remove = Some(marker.id);
                    }
                });
            }

            // Apply deferred actions
            if let Some(id) = marker_to_remove {
                state.markers.remove(id);
            }
            if let Some(time) = marker_to_jump {
                shared.settings.autoscale_x = false;
                let window = shared.settings.display_time_window;
                shared.settings.x_min = Some(time - window / 2.0);
                shared.settings.x_max = Some(time + window / 2.0);
            }
        }

        // Threshold lines section
        ui.add_space(8.0);
        ui.heading("Thresholds");
        ui.separator();

        // Add new threshold line
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.new_threshold_value)
                    .hint_text("Value...")
                    .desired_width(60.0),
            );

            // Color picker (simple)
            let mut color = Color32::from_rgba_unmultiplied(
                state.new_threshold_color[0],
                state.new_threshold_color[1],
                state.new_threshold_color[2],
                state.new_threshold_color[3],
            );
            if ui.color_edit_button_srgba(&mut color).changed() {
                state.new_threshold_color = [color.r(), color.g(), color.b(), color.a()];
            }

            if ui.button("+").on_hover_text("Add threshold line").clicked() {
                if let Ok(value) = state.new_threshold_value.parse::<f64>() {
                    let id = NEXT_THRESHOLD_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let label = format!("{:.2}", value);
                    state.threshold_lines.push(ThresholdLine::new(
                        id,
                        value,
                        label,
                        state.new_threshold_color,
                    ));
                    state.new_threshold_value.clear();
                }
            }
        });

        // Display existing threshold lines
        let mut threshold_to_remove: Option<u32> = None;
        for threshold in &mut state.threshold_lines {
            ui.horizontal(|ui| {
                // Color indicator
                let color = Color32::from_rgba_unmultiplied(
                    threshold.color[0],
                    threshold.color[1],
                    threshold.color[2],
                    threshold.color[3],
                );
                let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, color);

                // Visibility toggle
                ui.checkbox(&mut threshold.visible, "");

                // Value and label
                ui.label(format!("{}: {:.3}", threshold.label, threshold.value));

                // Delete button
                if ui.small_button("x").on_hover_text("Remove threshold").clicked() {
                    threshold_to_remove = Some(threshold.id);
                }
            });
        }

        // Apply deferred removal
        if let Some(id) = threshold_to_remove {
            state.threshold_lines.retain(|t| t.id != id);
        }
    }

    fn render_plot(state: &mut VisualizerPageState, shared: &mut SharedState<'_>, ui: &mut Ui) {
        use egui_plot::{AxisHints, Line, Plot, PlotPoints, Points};

        let current_time = shared.start_time.elapsed().as_secs_f64();

        // Calculate time bounds
        let (x_min, x_max) = if shared.settings.autoscale_x {
            // Auto-scale: show latest data with time window
            let window = shared.settings.display_time_window;
            (current_time - window, current_time)
        } else if let (Some(min), Some(max)) = (shared.settings.x_min, shared.settings.x_max) {
            (min, max)
        } else {
            // Default to current time window
            let window = shared.settings.display_time_window;
            (current_time - window, current_time)
        };

        let mut plot = Plot::new("data_plot")
            .legend(egui_plot::Legend::default())
            .x_axis_label("Time (s)")
            .y_axis_label("Value (Y1)");

        // Configure secondary Y-axis if enabled
        if state.enable_secondary_axis {
            // Create custom Y-axes: primary (left) and secondary (right)
            let y_axes = vec![
                AxisHints::new_y().label("Y1 (Left)").placement(egui_plot::HPlacement::Left),
                AxisHints::new_y().label("Y2 (Right)").placement(egui_plot::HPlacement::Right),
            ];
            plot = plot.custom_y_axes(y_axes);
        }

        // Apply axis bounds
        if !shared.settings.autoscale_x || shared.settings.lock_x {
            plot = plot.include_x(x_min).include_x(x_max);
        }

        if !shared.settings.autoscale_y || shared.settings.lock_y {
            if let (Some(y_min), Some(y_max)) = (shared.settings.y_min, shared.settings.y_max) {
                plot = plot.include_y(y_min).include_y(y_max);
            }
        }

        // Allow drag and zoom unless locked
        plot = plot
            .allow_drag(!shared.settings.lock_x && !shared.settings.lock_y)
            .allow_zoom(!shared.settings.lock_x && !shared.settings.lock_y)
            .allow_scroll(!shared.settings.lock_x && !shared.settings.lock_y);

        let cursor_enabled = state.cursor.enabled;
        let cursor_a = state.cursor.cursor_a;
        let cursor_b = state.cursor.cursor_b;

        let enable_secondary_axis = state.enable_secondary_axis;

        let response = plot.show(ui, |plot_ui| {
            use crate::types::{PlotStyle, MAX_RENDER_POINTS};

            // Draw data lines
            for var in &shared.config.variables {
                if !var.enabled || !var.show_in_graph {
                    continue;
                }

                if let Some(data) = shared.variable_data.get(&var.id) {
                    // Get raw points and decimate for performance
                    let raw_points = data.as_plot_points();
                    let points = Self::decimate_points(&raw_points, MAX_RENDER_POINTS);

                    if points.is_empty() {
                        continue;
                    }

                    let color = Color32::from_rgba_unmultiplied(
                        var.color[0],
                        var.color[1],
                        var.color[2],
                        var.color[3],
                    );

                    // Add axis indicator to name if secondary axis is enabled
                    let display_name = if enable_secondary_axis {
                        if var.y_axis == 1 {
                            format!("{} (Y2)", var.name)
                        } else {
                            format!("{} (Y1)", var.name)
                        }
                    } else {
                        var.name.clone()
                    };

                    let line_width = shared.config.ui.line_width;

                    // Render based on plot style
                    match var.plot_style {
                        PlotStyle::Line => {
                            let line = Line::new(display_name, PlotPoints::from(points))
                                .color(color)
                                .width(line_width);
                            plot_ui.line(line);
                        }
                        PlotStyle::Scatter => {
                            let scatter = Points::new(display_name, PlotPoints::from(points))
                                .color(color)
                                .radius(line_width * 2.0);
                            plot_ui.points(scatter);
                        }
                        PlotStyle::Step => {
                            let step_points = Self::to_step_points(&points);
                            let line = Line::new(display_name, PlotPoints::from(step_points))
                                .color(color)
                                .width(line_width);
                            plot_ui.line(line);
                        }
                        PlotStyle::Area => {
                            // Draw outline first
                            let outline = Line::new(format!("{}_outline", &display_name), PlotPoints::from(points.clone()))
                                .color(color)
                                .width(line_width);
                            plot_ui.line(outline);

                            // Draw filled area
                            let area_points = Self::create_area_polygon(&points);
                            let fill_color = Color32::from_rgba_unmultiplied(
                                color.r(),
                                color.g(),
                                color.b(),
                                50, // Semi-transparent fill
                            );
                            let polygon = Polygon::new(display_name, PlotPoints::from(area_points))
                                .fill_color(fill_color);
                            plot_ui.polygon(polygon);
                        }
                    }
                }
            }

            // Draw trigger threshold line if trigger is enabled
            if shared.settings.trigger.enabled {
                let threshold = shared.settings.trigger.threshold;
                let trigger_color = if shared.settings.trigger.triggered {
                    Color32::from_rgb(100, 255, 100) // Green when triggered
                } else if shared.settings.trigger.armed {
                    Color32::from_rgb(255, 255, 100) // Yellow when armed
                } else {
                    Color32::from_rgb(255, 100, 100) // Red when ready but not armed
                };

                let hline = HLine::new("Trigger", threshold)
                    .color(trigger_color)
                    .width(1.5)
                    .style(egui_plot::LineStyle::dashed_dense());
                plot_ui.hline(hline);
            }

            // Draw markers
            for marker in state.markers.visible() {
                let time = marker.time_secs();
                let color = marker.color();

                let vline = VLine::new(&marker.name, time)
                    .color(color)
                    .width(1.5)
                    .style(egui_plot::LineStyle::dashed_loose());
                plot_ui.vline(vline);
            }

            // Draw threshold lines
            for threshold in &state.threshold_lines {
                if !threshold.visible {
                    continue;
                }
                let color = Color32::from_rgba_unmultiplied(
                    threshold.color[0],
                    threshold.color[1],
                    threshold.color[2],
                    threshold.color[3],
                );
                let hline = HLine::new(&threshold.label, threshold.value)
                    .color(color)
                    .width(1.5)
                    .style(egui_plot::LineStyle::dashed_loose());
                plot_ui.hline(hline);
            }

            // Draw cursor lines if enabled
            if cursor_enabled {
                // Draw cursor A (green)
                if let Some(pos_a) = cursor_a {
                    let vline_a = VLine::new("Cursor A", pos_a.x)
                        .color(Color32::from_rgb(100, 255, 100))
                        .width(1.5);
                    plot_ui.vline(vline_a);
                }

                // Draw cursor B (yellow)
                if let Some(pos_b) = cursor_b {
                    let vline_b = VLine::new("Cursor B", pos_b.x)
                        .color(Color32::from_rgb(255, 255, 100))
                        .width(1.5);
                    plot_ui.vline(vline_b);
                }

                // Draw shaded region between cursors
                if let (Some(pos_a), Some(pos_b)) = (cursor_a, cursor_b) {
                    let (t_min, t_max) = if pos_a.x < pos_b.x {
                        (pos_a.x, pos_b.x)
                    } else {
                        (pos_b.x, pos_a.x)
                    };

                    // Draw a semi-transparent rectangle using lines at the boundaries
                    // (egui_plot doesn't have built-in region highlighting, so we use vertical lines)
                    let hline = HLine::new("Range", (pos_a.y + pos_b.y) / 2.0)
                        .color(Color32::from_rgba_unmultiplied(150, 150, 255, 30));
                    plot_ui.hline(hline);

                    // Add marker points at cursor positions for visibility
                    for (id, (_point, _name)) in &state.cursor.nearest_points {
                        if let Some(var) = shared.config.variables.iter().find(|v| v.id == *id) {
                            let color = Color32::from_rgba_unmultiplied(
                                var.color[0],
                                var.color[1],
                                var.color[2],
                                255,
                            );
                            // Find the value at cursor A time
                            if let Some(data) = shared.variable_data.get(id) {
                                // Get values at cursor times
                                let value_at_a = Self::find_value_at_time(data, t_min);
                                let value_at_b = Self::find_value_at_time(data, t_max);

                                let mut marker_points = Vec::new();
                                if let Some(v) = value_at_a {
                                    marker_points.push([t_min, v]);
                                }
                                if let Some(v) = value_at_b {
                                    marker_points.push([t_max, v]);
                                }

                                if !marker_points.is_empty() {
                                    let markers =
                                        Points::new("", PlotPoints::from(marker_points))
                                            .color(color)
                                            .radius(5.0);
                                    plot_ui.points(markers);
                                }
                            }
                        }
                    }
                }
            }

            // Capture bounds after rendering for manual mode
            if !shared.settings.lock_x {
                let bounds = plot_ui.plot_bounds();
                shared.settings.x_min = Some(bounds.min()[0]);
                shared.settings.x_max = Some(bounds.max()[0]);
            }
            if !shared.settings.lock_y {
                let bounds = plot_ui.plot_bounds();
                shared.settings.y_min = Some(bounds.min()[1]);
                shared.settings.y_max = Some(bounds.max()[1]);
            }
        });

        // Handle cursor interactions
        if cursor_enabled {
            // Update cursor position from hover
            if let Some(hover_pos) = response.response.hover_pos() {
                let plot_pos = response.transform.value_from_position(hover_pos);
                state
                    .cursor
                    .update_position(Some(PlotPoint::new(plot_pos.x, plot_pos.y)));
                state.cursor.find_nearest(shared.variable_data);
            } else {
                state.cursor.update_position(None);
            }

            // Handle click to set cursor
            if response.response.clicked() {
                if state.cursor.cursor_a.is_none() {
                    state.cursor.set_cursor_a();
                } else if state.cursor.cursor_b.is_none() {
                    state.cursor.set_cursor_b();
                    // Update statistics for the range
                    Self::update_range_statistics(state, shared);
                }
            }

            // Handle secondary click to clear
            if response.response.secondary_clicked() {
                state.cursor.clear_cursors();
                state.variable_statistics.clear();
            }

            // Show cursor tooltip with nearest values
            if state.cursor.position.is_some() && !state.cursor.nearest_points.is_empty() {
                let tooltip_text = Self::format_cursor_tooltip(state);
                response.response.on_hover_text(tooltip_text);
            }
        }
    }

    /// Find the interpolated value at a specific time
    fn find_value_at_time(data: &crate::types::VariableData, time: f64) -> Option<f64> {
        if data.data_points.is_empty() {
            return None;
        }

        // Find the two points surrounding the time
        let mut prev: Option<(f64, f64)> = None;
        let mut next: Option<(f64, f64)> = None;

        for point in &data.data_points {
            let t = point.timestamp.as_secs_f64();
            if t <= time {
                prev = Some((t, point.converted_value));
            }
            if t >= time && next.is_none() {
                next = Some((t, point.converted_value));
                break;
            }
        }

        match (prev, next) {
            (Some((t1, v1)), Some((t2, v2))) => {
                if (t2 - t1).abs() < 1e-9 {
                    Some(v1)
                } else {
                    // Linear interpolation
                    let ratio = (time - t1) / (t2 - t1);
                    Some(v1 + ratio * (v2 - v1))
                }
            }
            (Some((_, v)), None) => Some(v),
            (None, Some((_, v))) => Some(v),
            _ => None,
        }
    }

    /// Format cursor tooltip text
    fn format_cursor_tooltip(state: &VisualizerPageState) -> String {
        let mut lines = Vec::new();

        if let Some(pos) = state.cursor.position {
            lines.push(format!("T: {:.3}s", pos.x));
        }

        for (_id, (point, name)) in &state.cursor.nearest_points {
            lines.push(format!("{}: {:.4}", name, point.y));
        }

        lines.join("\n")
    }

    /// Update statistics for the cursor range
    fn update_range_statistics(state: &mut VisualizerPageState, shared: &SharedState<'_>) {
        state.variable_statistics.clear();

        if let Some((t_start, t_end)) = state.cursor.time_range() {
            for var in &shared.config.variables {
                if !var.enabled || !var.show_in_graph {
                    continue;
                }

                if let Some(data) = shared.variable_data.get(&var.id) {
                    let stats = PlotStatistics::from_data_range(data, t_start, t_end);
                    if stats.is_valid() {
                        state.variable_statistics.insert(var.id, stats);
                    }
                }
            }
        }
    }

    /// Render the statistics panel
    fn render_statistics_panel(
        state: &mut VisualizerPageState,
        shared: &SharedState<'_>,
        ui: &mut Ui,
    ) {
        ui.horizontal(|ui| {
            ui.heading("Statistics");
            if let Some((t_start, t_end)) = state.cursor.time_range() {
                ui.label(format!(
                    "(Range: {:.3}s - {:.3}s, ΔT: {:.3}s)",
                    t_start,
                    t_end,
                    t_end - t_start
                ));
            } else {
                ui.label("(Set cursor A and B to see range statistics)");
            }
        });

        ui.separator();

        if state.variable_statistics.is_empty() {
            ui.label("No data in selected range");
            return;
        }

        // Table header
        egui::Grid::new("stats_grid")
            .num_columns(10)
            .striped(true)
            .min_col_width(55.0)
            .show(ui, |ui| {
                ui.strong("Variable");
                ui.strong("Count");
                ui.strong("Min");
                ui.strong("Max");
                ui.strong("Mean");
                ui.strong("Std Dev");
                ui.strong("RMS");
                ui.strong("P-P");
                ui.strong("Rise");
                ui.strong("Fall");
                ui.end_row();

                for var in &shared.config.variables {
                    if let Some(stats) = state.variable_statistics.get(&var.id) {
                        let color = Color32::from_rgba_unmultiplied(
                            var.color[0],
                            var.color[1],
                            var.color[2],
                            255,
                        );
                        ui.colored_label(color, &var.name);
                        ui.label(format!("{}", stats.count));
                        ui.label(format!("{:.4}", stats.min));
                        ui.label(format!("{:.4}", stats.max));
                        ui.label(format!("{:.4}", stats.mean));
                        ui.label(format!("{:.4}", stats.std_dev));
                        ui.label(format!("{:.4}", stats.rms));
                        ui.label(format!("{:.4}", stats.peak_to_peak()));

                        // Calculate rise/fall time from cursor positions
                        if let (Some(pos_a), Some(pos_b)) = (state.cursor.cursor_a, state.cursor.cursor_b) {
                            if let Some(data) = shared.variable_data.get(&var.id) {
                                let (rise_time, fall_time) = Self::calculate_rise_fall_time(
                                    data,
                                    pos_a.x.min(pos_b.x),
                                    pos_a.x.max(pos_b.x),
                                    stats.min,
                                    stats.max,
                                );
                                if let Some(rt) = rise_time {
                                    ui.label(format!("{:.3}s", rt));
                                } else {
                                    ui.label("-");
                                }
                                if let Some(ft) = fall_time {
                                    ui.label(format!("{:.3}s", ft));
                                } else {
                                    ui.label("-");
                                }
                            } else {
                                ui.label("-");
                                ui.label("-");
                            }
                        } else {
                            ui.label("-");
                            ui.label("-");
                        }
                        ui.end_row();
                    }
                }
            });

        // Frequency estimation section
        if let Some((t_start, t_end)) = state.cursor.time_range() {
            ui.add_space(8.0);
            ui.heading("Frequency Estimation");
            ui.separator();

            egui::Grid::new("freq_grid")
                .num_columns(3)
                .striped(true)
                .min_col_width(80.0)
                .show(ui, |ui| {
                    ui.strong("Variable");
                    ui.strong("Est. Freq");
                    ui.strong("Zero Crossings");
                    ui.end_row();

                    for var in &shared.config.variables {
                        if let Some(data) = shared.variable_data.get(&var.id) {
                            let color = Color32::from_rgba_unmultiplied(
                                var.color[0],
                                var.color[1],
                                var.color[2],
                                255,
                            );
                            let (freq, crossings) = Self::estimate_frequency(data, t_start, t_end);
                            ui.colored_label(color, &var.name);
                            if let Some(f) = freq {
                                ui.label(format!("{:.2} Hz", f));
                            } else {
                                ui.label("-");
                            }
                            ui.label(format!("{}", crossings));
                            ui.end_row();
                        }
                    }
                });
        }
    }

    /// Calculate rise and fall time (10% to 90% of range)
    fn calculate_rise_fall_time(
        data: &crate::types::VariableData,
        t_start: f64,
        t_end: f64,
        min_val: f64,
        max_val: f64,
    ) -> (Option<f64>, Option<f64>) {
        let range = max_val - min_val;
        if range <= 0.0 {
            return (None, None);
        }

        let threshold_low = min_val + range * 0.1;
        let threshold_high = min_val + range * 0.9;

        let mut rise_start: Option<f64> = None;
        let mut rise_end: Option<f64> = None;
        let mut fall_start: Option<f64> = None;
        let mut fall_end: Option<f64> = None;

        let points: Vec<_> = data
            .data_points
            .iter()
            .filter(|p| {
                let t = p.timestamp.as_secs_f64();
                t >= t_start && t <= t_end
            })
            .collect();

        // Look for rising edge (10% -> 90%)
        for i in 1..points.len() {
            let prev = points[i - 1];
            let curr = points[i];

            // Rising: crossing from below 10% to above 10%
            if prev.converted_value <= threshold_low && curr.converted_value > threshold_low {
                rise_start = Some(curr.timestamp.as_secs_f64());
            }
            // Rising: crossing from below 90% to above 90%
            if prev.converted_value <= threshold_high && curr.converted_value > threshold_high {
                if rise_start.is_some() && rise_end.is_none() {
                    rise_end = Some(curr.timestamp.as_secs_f64());
                }
            }

            // Falling: crossing from above 90% to below 90%
            if prev.converted_value >= threshold_high && curr.converted_value < threshold_high {
                fall_start = Some(curr.timestamp.as_secs_f64());
            }
            // Falling: crossing from above 10% to below 10%
            if prev.converted_value >= threshold_low && curr.converted_value < threshold_low {
                if fall_start.is_some() && fall_end.is_none() {
                    fall_end = Some(curr.timestamp.as_secs_f64());
                }
            }
        }

        let rise_time = match (rise_start, rise_end) {
            (Some(start), Some(end)) => Some(end - start),
            _ => None,
        };

        let fall_time = match (fall_start, fall_end) {
            (Some(start), Some(end)) => Some(end - start),
            _ => None,
        };

        (rise_time, fall_time)
    }

    /// Estimate frequency based on zero crossings
    fn estimate_frequency(
        data: &crate::types::VariableData,
        t_start: f64,
        t_end: f64,
    ) -> (Option<f64>, usize) {
        let points: Vec<_> = data
            .data_points
            .iter()
            .filter(|p| {
                let t = p.timestamp.as_secs_f64();
                t >= t_start && t <= t_end
            })
            .collect();

        if points.len() < 2 {
            return (None, 0);
        }

        // Calculate mean to use as zero crossing reference
        let mean: f64 = points.iter().map(|p| p.converted_value).sum::<f64>() / points.len() as f64;

        // Count zero crossings (crossings of the mean)
        let mut crossings = 0;
        for i in 1..points.len() {
            let prev = points[i - 1].converted_value - mean;
            let curr = points[i].converted_value - mean;
            if (prev < 0.0 && curr >= 0.0) || (prev >= 0.0 && curr < 0.0) {
                crossings += 1;
            }
        }

        let duration = t_end - t_start;
        if duration > 0.0 && crossings >= 2 {
            // Each full cycle has 2 zero crossings
            let freq = (crossings as f64) / (2.0 * duration);
            (Some(freq), crossings)
        } else {
            (None, crossings)
        }
    }

    /// Render the FFT/frequency analysis panel
    fn render_fft_panel(
        state: &mut VisualizerPageState,
        shared: &SharedState<'_>,
        ui: &mut Ui,
    ) {
        use egui_plot::{Line, Plot, PlotPoints};

        ui.horizontal(|ui| {
            ui.heading("Frequency Analysis");

            ui.separator();

            // Variable selector for FFT
            ui.label("Variable:");
            egui::ComboBox::from_id_salt("fft_variable_selector")
                .selected_text(
                    state
                        .fft_variable_id
                        .and_then(|id| shared.config.variables.iter().find(|v| v.id == id))
                        .map(|v| v.name.as_str())
                        .unwrap_or("Select..."),
                )
                .width(120.0)
                .show_ui(ui, |ui| {
                    for var in &shared.config.variables {
                        if var.enabled && var.show_in_graph {
                            let is_selected = state.fft_variable_id == Some(var.id);
                            if ui.selectable_label(is_selected, &var.name).clicked() {
                                state.fft_variable_id = Some(var.id);
                                state.fft_result = None; // Clear cached result
                            }
                        }
                    }
                });

            ui.separator();

            // FFT size selector
            ui.label("Size:");
            egui::ComboBox::from_id_salt("fft_size_selector")
                .selected_text(format!("{}", state.fft_config.fft_size))
                .width(80.0)
                .show_ui(ui, |ui| {
                    for &size in FftConfig::available_sizes() {
                        let is_selected = state.fft_config.fft_size == size;
                        if ui.selectable_label(is_selected, format!("{}", size)).clicked() {
                            state.fft_config.fft_size = size;
                            state.fft_result = None;
                        }
                    }
                });

            // Window function selector
            ui.label("Window:");
            egui::ComboBox::from_id_salt("fft_window_selector")
                .selected_text(state.fft_config.window.display_name())
                .width(100.0)
                .show_ui(ui, |ui| {
                    for window in WindowFunction::all() {
                        let is_selected = state.fft_config.window == *window;
                        if ui
                            .selectable_label(is_selected, window.display_name())
                            .clicked()
                        {
                            state.fft_config.window = *window;
                            state.fft_result = None;
                        }
                    }
                });

            ui.separator();

            // Scale toggle (dB vs linear)
            let scale_text = if state.fft_db_scale { "dB" } else { "Linear" };
            if ui
                .selectable_label(state.fft_db_scale, scale_text)
                .on_hover_text("Toggle between dB and linear magnitude scale")
                .clicked()
            {
                state.fft_db_scale = !state.fft_db_scale;
            }

            // Averaging toggle (Welch's method)
            let avg_text = if state.fft_averaged { "Averaged" } else { "Single" };
            if ui
                .selectable_label(state.fft_averaged, avg_text)
                .on_hover_text("Use Welch's method (overlapping segments) for smoother spectrum")
                .clicked()
            {
                state.fft_averaged = !state.fft_averaged;
                state.fft_result = None;
            }

            // Compute FFT button
            if ui.button("Compute").on_hover_text("Recompute FFT").clicked() {
                state.fft_result = None; // Force recompute
            }
        });

        ui.separator();

        // Compute FFT if we have a selected variable
        if let Some(var_id) = state.fft_variable_id {
            if state.fft_result.is_none() {
                if let Some(data) = shared.variable_data.get(&var_id) {
                    if !data.data_points.is_empty() {
                        // Extract sample values
                        let samples: Vec<f64> = data
                            .data_points
                            .iter()
                            .map(|p| p.converted_value)
                            .collect();

                        // Estimate sample rate from data
                        let sample_rate = if data.data_points.len() >= 2 {
                            let duration = data.data_points.back().unwrap().timestamp.as_secs_f64()
                                - data.data_points.front().unwrap().timestamp.as_secs_f64();
                            if duration > 0.0 {
                                data.data_points.len() as f64 / duration
                            } else {
                                shared.config.collection.poll_rate_hz as f64
                            }
                        } else {
                            shared.config.collection.poll_rate_hz as f64
                        };

                        // Compute FFT
                        let result = if state.fft_averaged {
                            state.fft_analyzer.compute_averaged(&samples, sample_rate)
                        } else {
                            state.fft_analyzer.compute(&samples, sample_rate)
                        };

                        state.fft_result = Some(result);
                    }
                }
            }

            // Display FFT result
            if let Some(ref result) = state.fft_result {
                ui.horizontal(|ui| {
                    // Show info about the FFT
                    ui.label(format!(
                        "Samples: {} | Resolution: {:.2} Hz | Nyquist: {:.1} Hz",
                        result.sample_count,
                        result.frequency_resolution,
                        result.sample_rate / 2.0
                    ));

                    // Show peak info
                    if let Some((peak_freq, peak_mag)) = result.peak() {
                        ui.separator();
                        let peak_display = if state.fft_db_scale {
                            let db = if peak_mag > 1e-10 {
                                20.0 * peak_mag.log10()
                            } else {
                                -200.0
                            };
                            format!("Peak: {:.1} Hz ({:.1} dB)", peak_freq, db)
                        } else {
                            format!("Peak: {:.1} Hz ({:.4})", peak_freq, peak_mag)
                        };
                        ui.strong(peak_display);
                    }
                });

                // FFT Plot
                let plot_points: Vec<[f64; 2]> = if state.fft_db_scale {
                    result.plot_points_db()
                } else {
                    result.plot_points()
                };

                let y_label = if state.fft_db_scale {
                    "Magnitude (dB)"
                } else {
                    "Magnitude"
                };

                let plot = Plot::new("fft_plot")
                    .x_axis_label("Frequency (Hz)")
                    .y_axis_label(y_label)
                    .allow_zoom(true)
                    .allow_drag(true)
                    .legend(egui_plot::Legend::default().position(egui_plot::Corner::RightTop));

                // Get color for selected variable
                let color = state
                    .fft_variable_id
                    .and_then(|id| shared.config.variables.iter().find(|v| v.id == id))
                    .map(|v| {
                        Color32::from_rgba_unmultiplied(v.color[0], v.color[1], v.color[2], v.color[3])
                    })
                    .unwrap_or(Color32::from_rgb(100, 150, 255));

                plot.show(ui, |plot_ui| {
                    let line = Line::new("Spectrum", PlotPoints::from(plot_points))
                        .color(color)
                        .width(1.5);
                    plot_ui.line(line);
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("No data available for FFT analysis");
                });
            }
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("Select a variable to analyze its frequency spectrum");
            });
        }
    }

    fn render_dialogs(
        state: &mut VisualizerPageState,
        shared: &mut SharedState<'_>,
        ctx: &Context,
        actions: &mut Vec<AppAction>,
    ) {
        // Value editor dialog
        if state.value_editor_open {
            let var_id = state.value_editor_state.var_id;
            if let Some(var_id) = var_id {
                let (var_name, var_type, is_writable) = match shared.config.find_variable(var_id) {
                    Some(var) => (var.name.clone(), var.var_type, var.is_writable()),
                    None => {
                        state.value_editor_open = false;
                        return;
                    }
                };

                let current_value = shared
                    .variable_data
                    .get(&var_id)
                    .and_then(|d| d.last())
                    .map(|p| p.raw_value);

                let dialog_ctx = ValueEditorContext {
                    var_name: &var_name,
                    var_type,
                    is_writable,
                    connection_status: shared.connection_status,
                    current_value,
                };

                if let Some(action) = show_dialog::<ValueEditorDialog>(
                    ctx,
                    &mut state.value_editor_open,
                    &mut state.value_editor_state,
                    dialog_ctx,
                ) {
                    match action {
                        ValueEditorAction::Write { var_id, value } => {
                            actions.push(AppAction::WriteVariable { id: var_id, value });
                        }
                    }
                }
            }
        }

        // Trigger config dialog
        if state.trigger_config_open {
            let dialog_ctx = TriggerConfigContext {
                variables: &shared.config.variables,
                is_armed: shared.settings.trigger.armed,
                is_triggered: shared.settings.trigger.triggered,
            };

            if let Some(action) = show_dialog::<TriggerConfigDialog>(
                ctx,
                &mut state.trigger_config_open,
                &mut state.trigger_config_state,
                dialog_ctx,
            ) {
                match action {
                    TriggerConfigAction::UpdateSettings(settings) => {
                        shared.settings.trigger = settings;
                    }
                    TriggerConfigAction::Arm => {
                        shared.settings.trigger.arm();
                    }
                    TriggerConfigAction::Disarm => {
                        shared.settings.trigger.disarm();
                    }
                    TriggerConfigAction::Reset => {
                        shared.settings.trigger.reset();
                    }
                }
            }
        }

        // Export config dialog
        if state.export_config_open {
            // Calculate total samples and data duration
            let total_samples: usize = shared.variable_data.values().map(|d| d.data_points.len()).sum();
            let data_duration = shared.start_time.elapsed().as_secs_f64();

            let dialog_ctx = ExportConfigContext {
                variables: &shared.config.variables,
                total_samples,
                data_duration,
                cursor_range: state.cursor.time_range(),
            };

            if let Some(action) = show_dialog::<ExportConfigDialog>(
                ctx,
                &mut state.export_config_open,
                &mut state.export_config_state,
                dialog_ctx,
            ) {
                match action {
                    ExportConfigAction::Export {
                        format,
                        settings,
                        time_start,
                        time_end,
                        variables,
                        downsample_mode: _,
                        include_statistics: _,
                        file_path,
                    } => {
                        // TODO: Implement actual export functionality
                        // For now, just log the export request
                        tracing::info!(
                            "Export requested: format={:?}, file={:?}, variables={:?}, time_range={:?}-{:?}",
                            format,
                            file_path,
                            variables.len(),
                            time_start,
                            time_end
                        );
                        // Store the export settings for later use
                        shared.settings.export = settings;
                    }
                    ExportConfigAction::BrowseFile => {
                        // File browser would be handled by native dialog
                        // For now, user can type the path manually
                    }
                }
            }
        }
    }

    /// Decimate data points for efficient rendering
    /// Uses min/max downsampling to preserve signal extremes
    fn decimate_points(points: &[[f64; 2]], max_points: usize) -> Vec<[f64; 2]> {
        if points.len() <= max_points || points.is_empty() {
            return points.to_vec();
        }

        // Calculate bucket size
        let bucket_size = points.len() / (max_points / 2).max(1);
        let mut result = Vec::with_capacity(max_points);

        // Always include first point
        result.push(points[0]);

        // Process middle points in buckets, keeping min and max of each bucket
        for bucket in points[1..points.len().saturating_sub(1)].chunks(bucket_size) {
            if bucket.is_empty() {
                continue;
            }

            let (min_pt, max_pt) = bucket.iter().fold((bucket[0], bucket[0]), |(min, max), pt| {
                (
                    if pt[1] < min[1] { *pt } else { min },
                    if pt[1] > max[1] { *pt } else { max },
                )
            });

            // Add in time order (preserves visual appearance)
            if min_pt[0] < max_pt[0] {
                result.push(min_pt);
                result.push(max_pt);
            } else {
                result.push(max_pt);
                result.push(min_pt);
            }
        }

        // Always include last point
        if let Some(last) = points.last() {
            result.push(*last);
        }

        result
    }

    /// Convert line points to step format (horizontal-then-vertical transitions)
    fn to_step_points(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
        if points.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::with_capacity(points.len() * 2);

        for window in points.windows(2) {
            result.push(window[0]);
            result.push([window[1][0], window[0][1]]); // horizontal then vertical
        }

        if let Some(last) = points.last() {
            result.push(*last);
        }

        result
    }

    /// Create area polygon from line points (filled to X-axis)
    fn create_area_polygon(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
        if points.is_empty() {
            return Vec::new();
        }

        let mut polygon = points.to_vec();
        if let (Some(first), Some(last)) = (points.first(), points.last()) {
            polygon.push([last[0], 0.0]); // down to x-axis
            polygon.push([first[0], 0.0]); // back to start on x-axis
        }
        polygon
    }
}
