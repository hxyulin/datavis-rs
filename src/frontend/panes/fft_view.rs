//! FFT View pane - Frequency spectrum analysis
//!
//! Extracted from the FFT panel of the Visualizer page.

use egui::{Color32, Ui};

use crate::analysis::{FftAnalyzer, FftConfig, FftResult, WindowFunction};
use crate::frontend::pane_trait::Pane;
use crate::frontend::state::{AppAction, SharedState};
use crate::frontend::workspace::PaneKind;

/// State for the FFT View pane
pub struct FftViewState {
    /// Selected variable for FFT analysis
    pub target_variable_id: Option<u32>,
    /// FFT analyzer instance
    pub fft_analyzer: FftAnalyzer,
    /// FFT configuration (size, window)
    pub fft_config: FftConfig,
    /// Cached FFT result
    pub fft_result: Option<FftResult>,
    /// Whether to show FFT in dB scale
    pub db_scale: bool,
    /// Whether to use Welch's method (averaged FFT)
    pub averaged: bool,
}

impl Default for FftViewState {
    fn default() -> Self {
        Self {
            target_variable_id: None,
            fft_analyzer: FftAnalyzer::new(),
            fft_config: FftConfig::default(),
            fft_result: None,
            db_scale: true,
            averaged: true,
        }
    }
}

/// Render the FFT view pane
pub fn render(
    state: &mut FftViewState,
    shared: &mut SharedState<'_>,
    ui: &mut Ui,
) -> Vec<AppAction> {
    use egui_plot::{Line, Plot, PlotPoints};

    // Toolbar
    ui.horizontal(|ui| {
        ui.heading("Frequency Analysis");
        ui.separator();

        // Variable selector
        ui.label("Variable:");
        egui::ComboBox::from_id_salt("fft_pane_variable_selector")
            .selected_text(
                state
                    .target_variable_id
                    .and_then(|id| shared.config.variables.get(&id))
                    .map(|v| v.name.as_str())
                    .unwrap_or("Select..."),
            )
            .width(120.0)
            .show_ui(ui, |ui| {
                for var in shared.config.variables.values() {
                    if var.enabled && var.show_in_graph {
                        let is_selected = state.target_variable_id == Some(var.id);
                        if ui.selectable_label(is_selected, &var.name).clicked() {
                            state.target_variable_id = Some(var.id);
                            state.fft_result = None;
                        }
                    }
                }
            });

        ui.separator();

        // FFT size selector
        ui.label("Size:");
        egui::ComboBox::from_id_salt("fft_pane_size_selector")
            .selected_text(format!("{}", state.fft_config.fft_size))
            .width(80.0)
            .show_ui(ui, |ui| {
                for &size in FftConfig::available_sizes() {
                    let is_selected = state.fft_config.fft_size == size;
                    if ui
                        .selectable_label(is_selected, format!("{}", size))
                        .clicked()
                    {
                        state.fft_config.fft_size = size;
                        state.fft_result = None;
                    }
                }
            });

        // Window function selector
        ui.label("Window:");
        egui::ComboBox::from_id_salt("fft_pane_window_selector")
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

        // Scale toggle
        let scale_text = if state.db_scale { "dB" } else { "Linear" };
        if ui
            .selectable_label(state.db_scale, scale_text)
            .on_hover_text("Toggle between dB and linear magnitude scale")
            .clicked()
        {
            state.db_scale = !state.db_scale;
        }

        // Averaging toggle
        let avg_text = if state.averaged { "Averaged" } else { "Single" };
        if ui
            .selectable_label(state.averaged, avg_text)
            .on_hover_text("Use Welch's method (overlapping segments) for smoother spectrum")
            .clicked()
        {
            state.averaged = !state.averaged;
            state.fft_result = None;
        }

        // Compute button
        if ui
            .button("Compute")
            .on_hover_text("Recompute FFT")
            .clicked()
        {
            state.fft_result = None;
        }
    });

    ui.separator();

    // Compute FFT if we have a selected variable
    if let Some(var_id) = state.target_variable_id {
        if state.fft_result.is_none() {
            if let Some(data) = shared.topics.variable_data.get(&var_id) {
                if !data.data_points.is_empty() {
                    let samples: Vec<f64> = data
                        .data_points
                        .iter()
                        .map(|p| p.converted_value)
                        .collect();

                    let sample_rate = if data.data_points.len() >= 2 {
                        let duration = data.data_points.back().unwrap().timestamp.as_secs_f64()
                            - data
                                .data_points
                                .front()
                                .unwrap()
                                .timestamp
                                .as_secs_f64();
                        if duration > 0.0 {
                            data.data_points.len() as f64 / duration
                        } else {
                            shared.config.collection.poll_rate_hz as f64
                        }
                    } else {
                        shared.config.collection.poll_rate_hz as f64
                    };

                    let result = if state.averaged {
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
                ui.label(format!(
                    "Samples: {} | Resolution: {:.2} Hz | Nyquist: {:.1} Hz",
                    result.sample_count,
                    result.frequency_resolution,
                    result.sample_rate / 2.0
                ));

                if let Some((peak_freq, peak_mag)) = result.peak() {
                    ui.separator();
                    let peak_display = if state.db_scale {
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

            let plot_points: Vec<[f64; 2]> = if state.db_scale {
                result.plot_points_db()
            } else {
                result.plot_points()
            };

            let y_label = if state.db_scale {
                "Magnitude (dB)"
            } else {
                "Magnitude"
            };

            let plot = Plot::new("fft_pane_plot")
                .x_axis_label("Frequency (Hz)")
                .y_axis_label(y_label)
                .allow_zoom(true)
                .allow_drag(true)
                .legend(egui_plot::Legend::default().position(egui_plot::Corner::RightTop));

            let color = state
                .target_variable_id
                .and_then(|id| shared.config.variables.get(&id))
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

    Vec::new()
}

impl Pane for FftViewState {
    fn kind(&self) -> PaneKind { PaneKind::FftView }

    fn render(&mut self, shared: &mut SharedState, ui: &mut Ui) -> Vec<AppAction> {
        render(self, shared, ui)
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
