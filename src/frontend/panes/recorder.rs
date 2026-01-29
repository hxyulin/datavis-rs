//! Session Capture pane — combines recording and export functionality.
//!
//! Provides session recording controls (arm/start/stop), status display,
//! saved recordings list, playback controls, and file export controls.

use std::collections::HashMap;

use egui::Ui;

use crate::frontend::pane_trait::Pane;
use crate::frontend::state::{AppAction, SharedState};
use crate::frontend::workspace::PaneKind;
use crate::session::{SessionPlayer, SessionRecording, SessionState};

/// Export layout modes (formerly from pipeline::nodes::exporter_sink)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportLayout {
    /// Long format: one row per sample (timestamp, var_name, value)
    Long,
    /// Wide format: one row per timestamp, columns for each variable
    Wide,
}

/// Which value to export (formerly from pipeline::nodes::exporter_sink)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueChoice {
    /// Export raw value
    Raw,
    /// Export converted value
    Converted,
}

/// Tab selection for the Session Capture pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTab {
    Record,
    Export,
}

/// Export format selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Json,
}

impl ExportFormat {
    pub fn display_name(&self) -> &'static str {
        match self {
            ExportFormat::Csv => "CSV",
            ExportFormat::Json => "JSON",
        }
    }
}

/// State for the Session Capture pane (combined Recorder + Exporter).
pub struct RecorderPaneState {
    // --- Tab state ---
    /// Active tab in the Session Capture pane.
    pub active_tab: CaptureTab,

    // --- Recorder fields ---
    /// Session name input.
    pub session_name: String,
    /// Max frames (0 = unlimited).
    pub max_frames: usize,
    /// Sample interval in milliseconds.
    pub sample_interval_ms: u64,
    /// Playback controller.
    pub session_player: SessionPlayer,

    // --- Exporter fields ---
    /// Output file path for export.
    pub export_path: String,
    /// Selected export format.
    pub export_format: ExportFormat,
    /// Export layout mode (long or wide).
    pub export_layout: ExportLayout,
    /// Per-variable value choice for wide export. Key: VarId raw u32.
    pub value_choices: HashMap<u32, ValueChoice>,
}

impl Default for RecorderPaneState {
    fn default() -> Self {
        Self {
            active_tab: CaptureTab::Record,
            session_name: String::new(),
            max_frames: 0,
            sample_interval_ms: 10,
            session_player: SessionPlayer::new(),
            export_path: String::new(),
            export_format: ExportFormat::Csv,
            export_layout: ExportLayout::Long,
            value_choices: HashMap::new(),
        }
    }
}

/// Render the Session Capture pane.
pub fn render(
    state: &mut RecorderPaneState,
    shared: &mut SharedState<'_>,
    ui: &mut Ui,
) -> Vec<AppAction> {
    let mut actions = Vec::new();

    ui.heading("Session Capture");

    // Tab bar
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.active_tab, CaptureTab::Record, "Record");
        ui.selectable_value(&mut state.active_tab, CaptureTab::Export, "Export");
    });
    ui.separator();

    match state.active_tab {
        CaptureTab::Record => render_record_tab(state, shared, ui, &mut actions),
        CaptureTab::Export => render_export_tab(state, shared, ui, &mut actions),
    }

    actions
}

// ============================================================================
// Record tab
// ============================================================================

fn render_record_tab(
    state: &mut RecorderPaneState,
    shared: &mut SharedState<'_>,
    ui: &mut Ui,
    actions: &mut Vec<AppAction>,
) {
    // --- Recording controls ---
    render_recording_controls(state, shared, ui, actions);
    ui.separator();

    // --- Playback controls ---
    render_playback_controls(state, ui);
    ui.separator();

    // --- Saved recordings list ---
    render_saved_recordings(state, shared, ui);
}

fn render_recording_controls(
    state: &mut RecorderPaneState,
    shared: &SharedState<'_>,
    ui: &mut Ui,
    _actions: &mut Vec<AppAction>,
) {
    let recorder_state = shared.topics.recorder_state;

    ui.label("Recording Controls");

    match recorder_state {
        SessionState::Idle => {
            ui.horizontal(|ui| {
                ui.label("Session name:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.session_name)
                        .hint_text("Session name...")
                        .desired_width(150.0),
                );
            });

            ui.horizontal(|ui| {
                ui.label("Max frames:");
                let mut max_frames_str = if state.max_frames == 0 {
                    String::from("unlimited")
                } else {
                    state.max_frames.to_string()
                };
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut max_frames_str)
                            .desired_width(80.0),
                    )
                    .changed()
                {
                    state.max_frames = max_frames_str.parse().unwrap_or(0);
                }
            });

            ui.horizontal(|ui| {
                ui.label("Sample interval (ms):");
                ui.add(
                    egui::DragValue::new(&mut state.sample_interval_ms)
                        .range(1..=10000)
                        .speed(1.0),
                );
            });

            if ui.button("Start Recording").clicked() {
                // TODO: Implement recording with new backend architecture
                // Recording functionality needs to be re-implemented without pipeline nodes
                ui.label("Recording not yet implemented in new architecture");
            }
        }
        SessionState::Recording => {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::from_rgb(255, 100, 100), "● REC");
                ui.label(format!("{} frames", shared.topics.recorder_frame_count));
            });

            ui.horizontal(|ui| {
                if ui.button("Stop").clicked() {
                    // TODO: Implement stop recording
                }
                if ui.button("Cancel").clicked() {
                    // TODO: Implement cancel recording
                }
            });
        }
        SessionState::Stopped => {
            ui.label("Recording stopped.");
        }
        _ => {
            ui.label(format!("State: {}", recorder_state.display_name()));
        }
    }
}

fn render_playback_controls(state: &mut RecorderPaneState, ui: &mut Ui) {
    ui.label("Playback");

    if state.session_player.has_recording() {
        let player_state = state.session_player.state();

        ui.horizontal(|ui| {
            match player_state {
                SessionState::Playing => {
                    ui.colored_label(egui::Color32::from_rgb(100, 255, 100), "▶ PLAY");
                    if ui.button("⏸").clicked() {
                        state.session_player.pause();
                    }
                    if ui.button("⏹").clicked() {
                        state.session_player.stop();
                    }
                }
                SessionState::Paused => {
                    ui.colored_label(egui::Color32::from_rgb(255, 255, 100), "⏸ PAUSED");
                    if ui.button("▶").clicked() {
                        state.session_player.play();
                    }
                    if ui.button("⏹").clicked() {
                        state.session_player.stop();
                    }
                }
                SessionState::Stopped => {
                    if ui.button("▶ Play").clicked() {
                        state.session_player.play();
                    }
                    if ui.button("Unload").clicked() {
                        state.session_player.unload();
                    }
                }
                _ => {}
            }
        });

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

        ui.horizontal(|ui| {
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
        });
    } else {
        ui.label("No recording loaded.");
        if ui.button("Load from file...").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Load Session Recording")
                .add_filter("JSON Session", &["json"])
                .pick_file()
            {
                match SessionRecording::load_from_file(&path) {
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
}

fn render_saved_recordings(state: &mut RecorderPaneState, shared: &mut SharedState<'_>, ui: &mut Ui) {
    let recordings = &mut shared.topics.completed_recordings;
    ui.label(format!("Saved Recordings ({})", recordings.len()));

    if recordings.is_empty() {
        ui.label("No saved recordings yet.");
        return;
    }

    let mut play_idx = None;
    let mut save_idx = None;
    let mut remove_idx = None;

    for (i, recording) in recordings.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!(
                "{}: {} frames, {:.1}s",
                recording.metadata.name,
                recording.frame_count(),
                recording.duration().as_secs_f64(),
            ));
            if ui.small_button("Play").clicked() {
                play_idx = Some(i);
            }
            if ui.small_button("Save").clicked() {
                save_idx = Some(i);
            }
            if ui.small_button("Remove").clicked() {
                remove_idx = Some(i);
            }
        });
    }

    if let Some(i) = play_idx {
        let recording = recordings[i].clone();
        state.session_player.load(recording);
        state.session_player.play();
    }
    if let Some(i) = save_idx {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Save Session Recording")
            .add_filter("JSON Session", &["json"])
            .save_file()
        {
            if let Err(e) = recordings[i].save_to_file(&path) {
                tracing::error!("Failed to save session: {}", e);
            }
        }
    }
    if let Some(i) = remove_idx {
        recordings.remove(i);
    }
}

// ============================================================================
// Export tab
// ============================================================================

fn render_export_tab(
    state: &mut RecorderPaneState,
    shared: &mut SharedState<'_>,
    ui: &mut Ui,
    _actions: &mut Vec<AppAction>,
) {

    // --- Status ---
    ui.horizontal(|ui| {
        if shared.topics.exporter_active {
            ui.colored_label(egui::Color32::from_rgb(100, 255, 100), "● Active");
            ui.label(format!("{} rows written", shared.topics.exporter_rows_written));
        } else {
            ui.label("Inactive");
        }
    });
    ui.separator();

    // --- Controls ---
    // File path
    ui.horizontal(|ui| {
        ui.label("Output path:");
        ui.add(
            egui::TextEdit::singleline(&mut state.export_path)
                .hint_text("Select file...")
                .desired_width(250.0),
        );
        if ui.button("Browse...").clicked() {
            let filter = match state.export_format {
                ExportFormat::Csv => ("CSV Files", vec!["csv"]),
                ExportFormat::Json => ("JSON Files", vec!["json"]),
            };
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Export Data")
                .add_filter(filter.0, &filter.1)
                .save_file()
            {
                state.export_path = path.to_string_lossy().to_string();
            }
        }
    });

    // Format
    ui.horizontal(|ui| {
        ui.label("Format:");
        ui.selectable_value(&mut state.export_format, ExportFormat::Csv, "CSV");
        ui.selectable_value(&mut state.export_format, ExportFormat::Json, "JSON");
    });

    // Layout (CSV only)
    if state.export_format == ExportFormat::Csv {
        ui.horizontal(|ui| {
            ui.label("Layout:");
            ui.selectable_value(
                &mut state.export_layout,
                ExportLayout::Long,
                "Long (one row per sample)",
            );
            ui.selectable_value(
                &mut state.export_layout,
                ExportLayout::Wide,
                "Wide (all variables per row)",
            );
        });

        // Per-variable value choice (Wide mode only)
        if state.export_layout == ExportLayout::Wide {
            ui.separator();
            ui.label("Variable Value Types:");
            ui.horizontal(|ui| {
                if ui.button("All Converted").clicked() {
                    state.value_choices.clear();
                }
                if ui.button("All Raw").clicked() {
                    for vnode in &shared.topics.variable_tree {
                        if vnode.is_leaf && vnode.enabled {
                            state.value_choices.insert(vnode.id.0, ValueChoice::Raw);
                        }
                    }
                }
            });

            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for vnode in &shared.topics.variable_tree {
                        if !vnode.is_leaf || !vnode.enabled {
                            continue;
                        }
                        let choice = state
                            .value_choices
                            .entry(vnode.id.0)
                            .or_insert(ValueChoice::Converted);
                        ui.horizontal(|ui| {
                            ui.label(&vnode.name);
                            ui.selectable_value(choice, ValueChoice::Converted, "Converted");
                            ui.selectable_value(choice, ValueChoice::Raw, "Raw");
                        });
                    }
                });
        }
    }

    ui.separator();

    // Start/Stop
    ui.horizontal(|ui| {
        if shared.topics.exporter_active {
            if ui.button("Stop Export").clicked() {
                // TODO: Implement stop export with new architecture
            }
        } else {
            let _can_start = !state.export_path.is_empty();
            if ui.button("Start Export").clicked() {
                // TODO: Implement CSV export with new backend architecture
                // Export functionality needs to be re-implemented without pipeline nodes
            }
        }
    });
}

impl Pane for RecorderPaneState {
    fn kind(&self) -> PaneKind { PaneKind::Recorder }

    fn render(&mut self, shared: &mut SharedState, ui: &mut Ui) -> Vec<AppAction> {
        render(self, shared, ui)
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
