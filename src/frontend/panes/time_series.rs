//! Time Series pane - Plot display with toolbar and legend
//!
//! Extracted from the Visualizer page. All ctx-level panels are converted to ui-level layouts.

use std::collections::HashMap;

use egui::{Color32, Ui};
use egui_plot::{HLine, PlotPoint, Polygon, VLine};

use crate::frontend::dialogs::{
    ExportConfigState, TriggerConfigState, ValueEditorState,
};
use crate::frontend::markers::{MarkerManager, MarkerType};
use crate::frontend::plot::{PlotCursor, PlotStatistics};
use crate::frontend::state::{AppAction, SharedState};
use crate::session::{SessionMetadata, SessionPlayer, SessionRecorder, SessionState};
use crate::types::ConnectionStatus;

/// A horizontal threshold/reference line
#[derive(Debug, Clone)]
pub struct ThresholdLine {
    pub id: u32,
    pub value: f64,
    pub label: String,
    pub color: [u8; 4],
    pub visible: bool,
}

impl ThresholdLine {
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

/// State for the Time Series pane
pub struct TimeSeriesState {
    pub advanced_mode: bool,
    // Dialog states
    pub value_editor_open: bool,
    pub value_editor_state: ValueEditorState,
    pub trigger_config_open: bool,
    pub trigger_config_state: TriggerConfigState,
    pub export_config_open: bool,
    pub export_config_state: ExportConfigState,
    // Cursor
    pub cursor: PlotCursor,
    pub variable_statistics: HashMap<u32, PlotStatistics>,
    // Markers
    pub markers: MarkerManager,
    pub new_marker_name: String,
    pub new_marker_type: MarkerType,
    // Secondary Y-axis
    pub enable_secondary_axis: bool,
    pub secondary_y_min: Option<f64>,
    pub secondary_y_max: Option<f64>,
    pub secondary_autoscale_y: bool,
    // Session
    pub session_recorder: SessionRecorder,
    pub session_player: SessionPlayer,
    pub session_name: String,
    // Threshold lines
    pub threshold_lines: Vec<ThresholdLine>,
}

impl Default for TimeSeriesState {
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
            variable_statistics: HashMap::new(),
            markers: MarkerManager::default(),
            new_marker_name: String::new(),
            new_marker_type: MarkerType::default(),
            enable_secondary_axis: false,
            secondary_y_min: None,
            secondary_y_max: None,
            secondary_autoscale_y: true,
            session_recorder: SessionRecorder::new(),
            session_player: SessionPlayer::new(),
            session_name: String::new(),
            threshold_lines: Vec::new(),
        }
    }
}

/// Render the time series pane (inside &mut Ui, not &Context)
pub fn render(
    state: &mut TimeSeriesState,
    shared: &mut SharedState<'_>,
    ui: &mut Ui,
) -> Vec<AppAction> {
    let mut actions = Vec::new();

    // Toolbar at the top
    render_toolbar(state, shared, ui, &mut actions);
    ui.separator();

    // Main content: plot fills all remaining space
    render_plot(state, shared, ui);

    actions
}

/// Render dialogs that belong to this pane
pub fn render_dialogs(
    state: &mut TimeSeriesState,
    shared: &mut SharedState<'_>,
    ctx: &egui::Context,
    actions: &mut Vec<AppAction>,
) {
    use crate::frontend::dialogs::{
        show_dialog, ExportConfigAction, ExportConfigContext, ExportConfigDialog,
        TriggerConfigAction, TriggerConfigContext, TriggerConfigDialog, ValueEditorAction,
        ValueEditorContext, ValueEditorDialog,
    };

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
        let total_samples: usize = shared
            .variable_data
            .values()
            .map(|d| d.data_points.len())
            .sum();
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
                    tracing::info!(
                        "Export requested: format={:?}, file={:?}, variables={:?}, time_range={:?}-{:?}",
                        format,
                        file_path,
                        variables.len(),
                        time_start,
                        time_end
                    );
                    shared.settings.export = settings;
                }
                ExportConfigAction::BrowseFile => {}
            }
        }
    }
}

// ============================================================================
// Toolbar rendering
// ============================================================================

fn render_toolbar(
    state: &mut TimeSeriesState,
    shared: &mut SharedState<'_>,
    ui: &mut Ui,
    actions: &mut Vec<AppAction>,
) {
    if state.advanced_mode {
        render_toolbar_advanced(state, shared, ui, actions);
    } else {
        render_toolbar_simple(state, shared, ui, actions);
    }
}

fn render_toolbar_simple(
    state: &mut TimeSeriesState,
    shared: &mut SharedState<'_>,
    ui: &mut Ui,
    actions: &mut Vec<AppAction>,
) {
    ui.horizontal(|ui| {
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

        if ui.button("Clear").clicked() {
            actions.push(AppAction::ClearData);
        }

        ui.separator();

        let actual_rate = shared.stats.effective_sample_rate;
        let rate_color = if actual_rate > 0.0 {
            Color32::from_rgb(100, 255, 100)
        } else {
            Color32::GRAY
        };
        ui.label("Rate:");
        ui.colored_label(rate_color, format!("{:.0} Hz", actual_rate));

        ui.separator();

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

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.checkbox(&mut state.advanced_mode, "Advanced");
        });
    });
}

fn render_toolbar_advanced(
    state: &mut TimeSeriesState,
    shared: &mut SharedState<'_>,
    ui: &mut Ui,
    actions: &mut Vec<AppAction>,
) {
    // Row 1: Collection controls and stats
    ui.horizontal(|ui| {
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

        if ui.button("Clear").clicked() {
            actions.push(AppAction::ClearData);
        }

        if ui
            .button("Export...")
            .on_hover_text("Export data to file")
            .clicked()
        {
            state.export_config_state = ExportConfigState::default();
            if let Some((start, end)) = state.cursor.time_range() {
                state.export_config_state.set_cursor_range(start, end);
            }
            state.export_config_open = true;
        }

        ui.separator();

        // Stats
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

        ui.label("Rate:");
        ui.colored_label(rate_color, format!("{:.0} Hz", actual_rate));
        if is_throttled {
            ui.colored_label(
                Color32::from_rgb(255, 200, 100),
                format!("(target: {} Hz)", shared.config.collection.poll_rate_hz),
            );
        }
        ui.label(format!("| Success: {:.1}%", shared.stats.success_rate()));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.checkbox(&mut state.advanced_mode, "Advanced");
        });
    });

    // Row 2: Axis controls
    ui.horizontal(|ui| {
        ui.label("X-Axis:");

        let autoscale_x_text = if shared.settings.autoscale_x {
            "Auto"
        } else {
            "Manual"
        };
        if ui
            .selectable_label(shared.settings.autoscale_x, autoscale_x_text)
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
            .clicked()
        {
            shared.settings.toggle_lock_x();
        }

        ui.separator();

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

        ui.label("Y-Axis:");

        let autoscale_y_text = if shared.settings.autoscale_y {
            "Auto"
        } else {
            "Manual"
        };
        if ui
            .selectable_label(shared.settings.autoscale_y, autoscale_y_text)
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
            .clicked()
        {
            shared.settings.toggle_lock_y();
        }

        ui.separator();

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
            state.secondary_autoscale_y = true;
            state.secondary_y_min = None;
            state.secondary_y_max = None;
        }

        ui.separator();

        let secondary_text = if state.enable_secondary_axis {
            "Y2: On"
        } else {
            "Y2: Off"
        };
        if ui
            .selectable_label(state.enable_secondary_axis, secondary_text)
            .clicked()
        {
            state.enable_secondary_axis = !state.enable_secondary_axis;
        }

        if state.enable_secondary_axis {
            let y2_autoscale_text = if state.secondary_autoscale_y {
                "Auto"
            } else {
                "Manual"
            };
            if ui
                .selectable_label(state.secondary_autoscale_y, y2_autoscale_text)
                .clicked()
            {
                state.secondary_autoscale_y = !state.secondary_autoscale_y;
            }
        }
    });

    // Row 3: Cursor and statistics
    ui.horizontal(|ui| {
        ui.label("Cursor:");

        let cursor_text = if state.cursor.enabled {
            "Enabled"
        } else {
            "Disabled"
        };
        if ui
            .selectable_label(state.cursor.enabled, cursor_text)
            .clicked()
        {
            state.cursor.enabled = !state.cursor.enabled;
        }

        ui.add_enabled_ui(state.cursor.enabled, |ui| {
            if ui.button("Set A").clicked() {
                state.cursor.set_cursor_a();
            }
            if ui.button("Set B").clicked() {
                state.cursor.set_cursor_b();
            }
            if ui.button("Clear").clicked() {
                state.cursor.clear_cursors();
                state.variable_statistics.clear();
            }

            if let Some(dt) = state.cursor.time_delta() {
                ui.separator();
                ui.label(format!("ΔT: {:.3}s", dt));
                if dt > 0.0 {
                    ui.label(format!("({:.1} Hz)", 1.0 / dt));
                }
            }
        });

        ui.separator();

        // Trigger controls
        ui.label("Trigger:");
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

        if trigger_enabled {
            if trigger_armed {
                if ui.button("Disarm").clicked() {
                    shared.settings.trigger.disarm();
                }
            } else if ui.button("Arm").clicked() {
                shared.settings.trigger.arm();
            }
            if trigger_triggered && ui.button("Reset").clicked() {
                shared.settings.trigger.reset();
            }
        }

        if ui.button("Config...").clicked() {
            state.trigger_config_state =
                TriggerConfigState::from_settings(&shared.settings.trigger);
            state.trigger_config_open = true;
        }
    });

    // Row 4: Markers and FFT
    ui.horizontal(|ui| {
        ui.label("Markers:");

        let current_time = shared.start_time.elapsed();
        if ui.button("Add").clicked() {
            let name = if state.new_marker_name.is_empty() {
                format!("Marker {}", state.markers.len() + 1)
            } else {
                std::mem::take(&mut state.new_marker_name)
            };
            state.markers.add(name, current_time, state.new_marker_type);
        }

        egui::ComboBox::from_id_salt("ts_marker_type_selector")
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

        ui.add(
            egui::TextEdit::singleline(&mut state.new_marker_name)
                .hint_text("Marker name...")
                .desired_width(100.0),
        );

        ui.separator();

        if !state.markers.is_empty() {
            if ui.button("<").clicked() {
                if let Some(marker) = state.markers.prev_before(current_time) {
                    let marker_time = marker.time_secs();
                    shared.settings.autoscale_x = false;
                    let window = shared.settings.display_time_window;
                    shared.settings.x_min = Some(marker_time - window / 2.0);
                    shared.settings.x_max = Some(marker_time + window / 2.0);
                }
            }

            if ui.button(">").clicked() {
                if let Some(marker) = state.markers.next_after(current_time) {
                    let marker_time = marker.time_secs();
                    shared.settings.autoscale_x = false;
                    let window = shared.settings.display_time_window;
                    shared.settings.x_min = Some(marker_time - window / 2.0);
                    shared.settings.x_max = Some(marker_time + window / 2.0);
                }
            }

            ui.label(format!("({} markers)", state.markers.len()));

            if ui.button("Clear All").clicked() {
                state.markers.clear();
            }
        }

    });

    // Row 5: Session recording and playback
    ui.horizontal(|ui| {
        ui.label("Session:");

        let recorder_state = state.session_recorder.state();
        let player_state = state.session_player.state();

        if player_state == SessionState::Idle || player_state == SessionState::Stopped {
            match recorder_state {
                SessionState::Idle => {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.session_name)
                            .hint_text("Session name...")
                            .desired_width(100.0),
                    );

                    if ui.button("Record").clicked() {
                        let name = if state.session_name.is_empty() {
                            format!(
                                "Session {}",
                                chrono::Local::now().format("%Y-%m-%d %H:%M")
                            )
                        } else {
                            std::mem::take(&mut state.session_name)
                        };
                        let mut metadata = SessionMetadata::new(name);
                        metadata.poll_rate_hz = shared.config.collection.poll_rate_hz;
                        state
                            .session_recorder
                            .set_variables(&shared.config.variables);
                        state.session_recorder.start_recording(metadata);
                    }
                }
                SessionState::Recording => {
                    ui.colored_label(Color32::from_rgb(255, 100, 100), "REC");
                    ui.label(format!(
                        "{:.1}s ({} frames)",
                        state.session_recorder.recording_duration().as_secs_f64(),
                        state.session_recorder.frame_count()
                    ));

                    if ui.button("Stop").clicked() {
                        state.session_recorder.stop_recording();
                    }
                    if ui.button("Cancel").clicked() {
                        state.session_recorder.cancel_recording();
                    }
                }
                SessionState::Stopped => {
                    ui.label(format!(
                        "Recorded: {:.1}s",
                        state
                            .session_recorder
                            .recording()
                            .duration()
                            .as_secs_f64()
                    ));

                    if ui.button("Play").clicked() {
                        let recording = state.session_recorder.take_recording();
                        state.session_player.load(recording);
                        state.session_player.play();
                    }
                    if ui.button("Save").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Save Session Recording")
                            .add_filter("JSON Session", &["json"])
                            .save_file()
                        {
                            if let Err(e) = state.session_recorder.recording().save_to_file(&path)
                            {
                                tracing::error!("Failed to save session: {}", e);
                            }
                        }
                    }
                    if ui.button("Discard").clicked() {
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
                    if ui.button("||").clicked() {
                        state.session_player.pause();
                    }
                    if ui.button("Stop").clicked() {
                        state.session_player.stop();
                    }
                }
                SessionState::Paused => {
                    ui.colored_label(Color32::from_rgb(255, 255, 100), "PAUSED");
                    if ui.button(">").clicked() {
                        state.session_player.play();
                    }
                    if ui.button("Stop").clicked() {
                        state.session_player.stop();
                    }
                }
                SessionState::Stopped => {
                    if ui.button(">").clicked() {
                        state.session_player.play();
                    }
                    if ui.button("Unload").clicked() {
                        state.session_player.unload();
                    }
                }
                _ => {}
            }

            let progress = state.session_player.progress();
            let total = state.session_player.total_duration();
            let current = state.session_player.current_time();

            ui.label(format!(
                "{:.1}s / {:.1}s",
                current.as_secs_f64(),
                total.as_secs_f64()
            ));

            let mut progress_slider = (progress * 100.0) as f32;
            if ui
                .add(egui::Slider::new(&mut progress_slider, 0.0..=100.0).show_value(false))
                .changed()
            {
                state
                    .session_player
                    .seek_progress(progress_slider as f64 / 100.0);
            }

            ui.label("Speed:");
            let mut speed = state.session_player.playback_speed() as f32;
            if ui
                .add(egui::Slider::new(&mut speed, 0.1..=4.0).suffix("x"))
                .changed()
            {
                state.session_player.set_playback_speed(speed as f64);
            }

            let mut loop_enabled = state.session_player.loop_playback();
            if ui.checkbox(&mut loop_enabled, "Loop").changed() {
                state.session_player.set_loop_playback(loop_enabled);
            }
        } else if ui.button("Load").clicked() {
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
    });
}


// ============================================================================
// Plot
// ============================================================================

fn render_plot(state: &mut TimeSeriesState, shared: &mut SharedState<'_>, ui: &mut Ui) {
    use egui_plot::{AxisHints, Line, Plot, PlotPoints, Points};

    let current_time = shared.start_time.elapsed().as_secs_f64();

    let (x_min, x_max) = if shared.settings.autoscale_x {
        let window = shared.settings.display_time_window;
        (current_time - window, current_time)
    } else if let (Some(min), Some(max)) = (shared.settings.x_min, shared.settings.x_max) {
        (min, max)
    } else {
        let window = shared.settings.display_time_window;
        (current_time - window, current_time)
    };

    let mut plot = Plot::new("ts_data_plot")
        .legend(egui_plot::Legend::default())
        .x_axis_label("Time (s)")
        .y_axis_label("Value (Y1)");

    if state.enable_secondary_axis {
        let y_axes = vec![
            AxisHints::new_y()
                .label("Y1 (Left)")
                .placement(egui_plot::HPlacement::Left),
            AxisHints::new_y()
                .label("Y2 (Right)")
                .placement(egui_plot::HPlacement::Right),
        ];
        plot = plot.custom_y_axes(y_axes);
    }

    if !shared.settings.autoscale_x || shared.settings.lock_x {
        plot = plot.include_x(x_min).include_x(x_max);
    }

    if !shared.settings.autoscale_y || shared.settings.lock_y {
        if let (Some(y_min), Some(y_max)) = (shared.settings.y_min, shared.settings.y_max) {
            plot = plot.include_y(y_min).include_y(y_max);
        }
    }

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

        for var in &shared.config.variables {
            if !var.enabled || !var.show_in_graph {
                continue;
            }

            if let Some(data) = shared.variable_data.get(&var.id) {
                let raw_points = data.as_plot_points();
                let points = decimate_points(&raw_points, MAX_RENDER_POINTS);

                if points.is_empty() {
                    continue;
                }

                let color = Color32::from_rgba_unmultiplied(
                    var.color[0],
                    var.color[1],
                    var.color[2],
                    var.color[3],
                );

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
                        let step_points = to_step_points(&points);
                        let line = Line::new(display_name, PlotPoints::from(step_points))
                            .color(color)
                            .width(line_width);
                        plot_ui.line(line);
                    }
                    PlotStyle::Area => {
                        let outline = Line::new(
                            format!("{}_outline", &display_name),
                            PlotPoints::from(points.clone()),
                        )
                        .color(color)
                        .width(line_width);
                        plot_ui.line(outline);

                        let area_points = create_area_polygon(&points);
                        let fill_color = Color32::from_rgba_unmultiplied(
                            color.r(),
                            color.g(),
                            color.b(),
                            50,
                        );
                        let polygon =
                            Polygon::new(display_name, PlotPoints::from(area_points))
                                .fill_color(fill_color);
                        plot_ui.polygon(polygon);
                    }
                }
            }
        }

        // Draw trigger threshold line
        if shared.settings.trigger.enabled {
            let threshold = shared.settings.trigger.threshold;
            let trigger_color = if shared.settings.trigger.triggered {
                Color32::from_rgb(100, 255, 100)
            } else if shared.settings.trigger.armed {
                Color32::from_rgb(255, 255, 100)
            } else {
                Color32::from_rgb(255, 100, 100)
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

        // Draw cursor lines
        if cursor_enabled {
            if let Some(pos_a) = cursor_a {
                let vline_a = VLine::new("Cursor A", pos_a.x)
                    .color(Color32::from_rgb(100, 255, 100))
                    .width(1.5);
                plot_ui.vline(vline_a);
            }

            if let Some(pos_b) = cursor_b {
                let vline_b = VLine::new("Cursor B", pos_b.x)
                    .color(Color32::from_rgb(255, 255, 100))
                    .width(1.5);
                plot_ui.vline(vline_b);
            }

            if let (Some(pos_a), Some(pos_b)) = (cursor_a, cursor_b) {
                let hline = HLine::new("Range", (pos_a.y + pos_b.y) / 2.0)
                    .color(Color32::from_rgba_unmultiplied(150, 150, 255, 30));
                plot_ui.hline(hline);
            }
        }

        // Capture bounds
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
        if let Some(hover_pos) = response.response.hover_pos() {
            let plot_pos = response.transform.value_from_position(hover_pos);
            state
                .cursor
                .update_position(Some(PlotPoint::new(plot_pos.x, plot_pos.y)));
            state.cursor.find_nearest(shared.variable_data);
        } else {
            state.cursor.update_position(None);
        }

        if response.response.clicked() {
            if state.cursor.cursor_a.is_none() {
                state.cursor.set_cursor_a();
            } else if state.cursor.cursor_b.is_none() {
                state.cursor.set_cursor_b();
                update_range_statistics(state, shared);
            }
        }

        if response.response.secondary_clicked() {
            state.cursor.clear_cursors();
            state.variable_statistics.clear();
        }

        if state.cursor.position.is_some() && !state.cursor.nearest_points.is_empty() {
            let tooltip_text = format_cursor_tooltip(state);
            response.response.on_hover_text(tooltip_text);
        }
    }
}



// ============================================================================
// Helper functions
// ============================================================================

fn format_cursor_tooltip(state: &TimeSeriesState) -> String {
    let mut lines = Vec::new();
    if let Some(pos) = state.cursor.position {
        lines.push(format!("T: {:.3}s", pos.x));
    }
    for (_id, (point, name)) in &state.cursor.nearest_points {
        lines.push(format!("{}: {:.4}", name, point.y));
    }
    lines.join("\n")
}

fn update_range_statistics(state: &mut TimeSeriesState, shared: &SharedState<'_>) {
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

fn decimate_points(points: &[[f64; 2]], max_points: usize) -> Vec<[f64; 2]> {
    if points.len() <= max_points || points.is_empty() {
        return points.to_vec();
    }

    let bucket_size = points.len() / (max_points / 2).max(1);
    let mut result = Vec::with_capacity(max_points);

    result.push(points[0]);

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
        if min_pt[0] < max_pt[0] {
            result.push(min_pt);
            result.push(max_pt);
        } else {
            result.push(max_pt);
            result.push(min_pt);
        }
    }

    if let Some(last) = points.last() {
        result.push(*last);
    }

    result
}

fn to_step_points(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::with_capacity(points.len() * 2);
    for window in points.windows(2) {
        result.push(window[0]);
        result.push([window[1][0], window[0][1]]);
    }
    if let Some(last) = points.last() {
        result.push(*last);
    }
    result
}

fn create_area_polygon(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut polygon = points.to_vec();
    if let (Some(first), Some(last)) = (points.first(), points.last()) {
        polygon.push([last[0], 0.0]);
        polygon.push([first[0], 0.0]);
    }
    polygon
}
