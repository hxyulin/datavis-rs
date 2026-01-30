//! Advanced export configuration dialog
//!
//! Dialog for configuring data export with:
//! - Time range selection
//! - Variable selection
//! - Downsampling options
//! - Format configuration
//! - Export preview

use egui::Ui;
use std::collections::HashSet;
use std::path::PathBuf;

use super::{Dialog, DialogAction, DialogState, DialogWindowConfig};
use crate::config::settings::{ExportSettings, TimestampFormat};
use crate::types::Variable;

/// Export format options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// CSV format
    Csv,
    /// JSON format
    Json,
    /// Binary format (raw bytes)
    Binary,
}

impl ExportFormat {
    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            ExportFormat::Csv => "CSV",
            ExportFormat::Json => "JSON",
            ExportFormat::Binary => "Binary",
        }
    }

    /// Get file extension
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Csv => "csv",
            ExportFormat::Json => "json",
            ExportFormat::Binary => "bin",
        }
    }
}

/// Downsampling mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DownsampleMode {
    /// No downsampling - export all data
    #[default]
    None,
    /// Export every Nth sample
    EveryNth(u32),
    /// Target a specific sample rate (Hz)
    TargetRate(u32),
}

/// State for the export configuration dialog
#[derive(Debug, Clone)]
pub struct ExportConfigState {
    /// Export format
    pub format: ExportFormat,
    /// Export settings (CSV options, etc.)
    pub settings: ExportSettings,
    /// Time range start (None for beginning of data)
    pub time_start: Option<f64>,
    /// Time range end (None for end of data)
    pub time_end: Option<f64>,
    /// Use cursor range for time selection
    pub use_cursor_range: bool,
    /// Selected variable IDs to export
    pub selected_variables: HashSet<u32>,
    /// Select all variables
    pub select_all: bool,
    /// Downsampling mode
    pub downsample_mode: DownsampleMode,
    /// Downsample every Nth value
    pub downsample_nth: u32,
    /// Target sample rate for downsampling
    pub target_rate: u32,
    /// Include statistics summary in export
    pub include_statistics: bool,
    /// Export file path
    pub file_path: Option<PathBuf>,
    /// File path input (for text editing)
    pub file_path_input: String,
}

impl Default for ExportConfigState {
    fn default() -> Self {
        Self {
            format: ExportFormat::Csv,
            settings: ExportSettings::default(),
            time_start: None,
            time_end: None,
            use_cursor_range: false,
            selected_variables: HashSet::new(),
            select_all: true,
            downsample_mode: DownsampleMode::None,
            downsample_nth: 10,
            target_rate: 100,
            include_statistics: false,
            file_path: None,
            file_path_input: String::new(),
        }
    }
}

impl DialogState for ExportConfigState {
    fn is_valid(&self) -> bool {
        // Valid if we have at least one variable selected and a valid file path
        (!self.selected_variables.is_empty() || self.select_all) && !self.file_path_input.is_empty()
    }
}

impl ExportConfigState {
    /// Initialize state with available variables
    pub fn init_with_variables(&mut self, variables: &std::collections::HashMap<u32, Variable>) {
        if self.select_all {
            self.selected_variables = variables
                .values()
                .filter(|v| v.enabled)
                .map(|v| v.id)
                .collect();
        }
    }

    /// Set time range from cursor positions
    pub fn set_cursor_range(&mut self, start: f64, end: f64) {
        self.time_start = Some(start.min(end));
        self.time_end = Some(start.max(end));
        self.use_cursor_range = true;
    }

    /// Calculate estimated row count for preview
    pub fn estimate_row_count(&self, total_samples: usize, data_duration: f64) -> usize {
        // Apply time range filter
        let time_range = match (self.time_start, self.time_end) {
            (Some(start), Some(end)) => {
                let ratio = (end - start) / data_duration;
                (total_samples as f64 * ratio) as usize
            }
            _ => total_samples,
        };

        // Apply downsampling
        match self.downsample_mode {
            DownsampleMode::None => time_range,
            DownsampleMode::EveryNth(n) => time_range / n.max(1) as usize,
            DownsampleMode::TargetRate(rate) => {
                if data_duration > 0.0 {
                    (rate as f64 * data_duration) as usize
                } else {
                    time_range
                }
            }
        }
    }
}

/// Actions from the export config dialog
#[derive(Debug, Clone)]
pub enum ExportConfigAction {
    /// Export data with current settings
    Export {
        format: ExportFormat,
        settings: ExportSettings,
        time_start: Option<f64>,
        time_end: Option<f64>,
        variables: HashSet<u32>,
        downsample_mode: DownsampleMode,
        include_statistics: bool,
        file_path: PathBuf,
    },
    /// Browse for file path
    BrowseFile,
}

/// Context for rendering the export config dialog
pub struct ExportConfigContext<'a> {
    /// Available variables
    pub variables: &'a std::collections::HashMap<u32, Variable>,
    /// Total number of data points across all variables
    pub total_samples: usize,
    /// Data duration in seconds
    pub data_duration: f64,
    /// Cursor range (if available)
    pub cursor_range: Option<(f64, f64)>,
}

/// Export configuration dialog
pub struct ExportConfigDialog;

impl Dialog for ExportConfigDialog {
    type State = ExportConfigState;
    type Action = ExportConfigAction;
    type Context<'a> = ExportConfigContext<'a>;

    fn title(_state: &Self::State) -> &'static str {
        "Export Data"
    }

    fn window_config() -> DialogWindowConfig {
        DialogWindowConfig {
            default_width: 450.0,
            default_height: Some(500.0),
            resizable: true,
            collapsible: false,
            anchor: None,
            modal: false,
        }
    }

    fn render(
        state: &mut Self::State,
        ctx: Self::Context<'_>,
        ui: &mut Ui,
    ) -> DialogAction<Self::Action> {
        let mut action = DialogAction::None;

        // Initialize selected variables if select_all is true
        if state.select_all && state.selected_variables.is_empty() {
            state.init_with_variables(ctx.variables);
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            // Format selection
            ui.heading("Format");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.format, ExportFormat::Csv, "CSV");
                ui.selectable_value(&mut state.format, ExportFormat::Json, "JSON");
                ui.selectable_value(&mut state.format, ExportFormat::Binary, "Binary");
            });

            ui.add_space(8.0);

            // Time range section
            ui.heading("Time Range");
            ui.horizontal(|ui| {
                if ui
                    .radio(
                        !state.use_cursor_range && state.time_start.is_none(),
                        "All Data",
                    )
                    .clicked()
                {
                    state.time_start = None;
                    state.time_end = None;
                    state.use_cursor_range = false;
                }

                if let Some((start, end)) = ctx.cursor_range {
                    if ui
                        .radio(state.use_cursor_range, "Cursor Range")
                        .on_hover_text(format!("{:.3}s - {:.3}s", start, end))
                        .clicked()
                    {
                        state.set_cursor_range(start, end);
                    }
                }

                if ui
                    .radio(
                        !state.use_cursor_range && state.time_start.is_some(),
                        "Custom",
                    )
                    .clicked()
                {
                    state.use_cursor_range = false;
                    state.time_start = Some(0.0);
                    state.time_end = Some(ctx.data_duration);
                }
            });

            // Custom time range inputs
            if !state.use_cursor_range && state.time_start.is_some() {
                ui.horizontal(|ui| {
                    ui.label("Start:");
                    let mut start = state.time_start.unwrap_or(0.0);
                    if ui
                        .add(egui::DragValue::new(&mut start).suffix("s").speed(0.1))
                        .changed()
                    {
                        state.time_start = Some(start.max(0.0));
                    }

                    ui.label("End:");
                    let mut end = state.time_end.unwrap_or(ctx.data_duration);
                    if ui
                        .add(egui::DragValue::new(&mut end).suffix("s").speed(0.1))
                        .changed()
                    {
                        state.time_end = Some(end.max(state.time_start.unwrap_or(0.0)));
                    }
                });
            }

            ui.add_space(8.0);

            // Variable selection
            ui.heading("Variables");
            ui.horizontal(|ui| {
                if ui.checkbox(&mut state.select_all, "Select All").changed() && state.select_all {
                    state.init_with_variables(ctx.variables);
                }
            });

            if !state.select_all {
                ui.group(|ui| {
                    egui::ScrollArea::vertical()
                        .max_height(100.0)
                        .show(ui, |ui| {
                            for var in ctx.variables.values() {
                                if var.enabled {
                                    let mut selected = state.selected_variables.contains(&var.id);
                                    if ui.checkbox(&mut selected, &var.name).changed() {
                                        if selected {
                                            state.selected_variables.insert(var.id);
                                        } else {
                                            state.selected_variables.remove(&var.id);
                                        }
                                    }
                                }
                            }
                        });
                });
            }

            let selected_count = if state.select_all {
                ctx.variables.values().filter(|v| v.enabled).count()
            } else {
                state.selected_variables.len()
            };
            ui.label(
                egui::RichText::new(format!("{} variables selected", selected_count))
                    .small()
                    .weak(),
            );

            ui.add_space(8.0);

            // Downsampling
            ui.heading("Downsampling");
            ui.horizontal(|ui| {
                if ui
                    .radio(state.downsample_mode == DownsampleMode::None, "None")
                    .clicked()
                {
                    state.downsample_mode = DownsampleMode::None;
                }
                if ui
                    .radio(
                        matches!(state.downsample_mode, DownsampleMode::EveryNth(_)),
                        "Every Nth",
                    )
                    .clicked()
                {
                    state.downsample_mode = DownsampleMode::EveryNth(state.downsample_nth);
                }
                if ui
                    .radio(
                        matches!(state.downsample_mode, DownsampleMode::TargetRate(_)),
                        "Target Rate",
                    )
                    .clicked()
                {
                    state.downsample_mode = DownsampleMode::TargetRate(state.target_rate);
                }
            });

            match state.downsample_mode {
                DownsampleMode::EveryNth(_) => {
                    ui.horizontal(|ui| {
                        ui.label("Keep every:");
                        if ui
                            .add(egui::DragValue::new(&mut state.downsample_nth).range(1..=1000))
                            .changed()
                        {
                            state.downsample_mode =
                                DownsampleMode::EveryNth(state.downsample_nth.max(1));
                        }
                        ui.label("samples");
                    });
                }
                DownsampleMode::TargetRate(_) => {
                    ui.horizontal(|ui| {
                        ui.label("Target rate:");
                        if ui
                            .add(
                                egui::DragValue::new(&mut state.target_rate)
                                    .range(1..=10000)
                                    .suffix(" Hz"),
                            )
                            .changed()
                        {
                            state.downsample_mode =
                                DownsampleMode::TargetRate(state.target_rate.max(1));
                        }
                    });
                }
                _ => {}
            }

            ui.add_space(8.0);

            // Format-specific options
            if state.format == ExportFormat::Csv {
                ui.collapsing("CSV Options", |ui| {
                    ui.checkbox(&mut state.settings.include_header, "Include header row");
                    ui.checkbox(&mut state.settings.include_timestamps, "Include timestamps");
                    ui.checkbox(&mut state.settings.include_raw_values, "Include raw values");
                    ui.checkbox(
                        &mut state.settings.include_converted_values,
                        "Include converted values",
                    );

                    ui.horizontal(|ui| {
                        ui.label("Timestamp format:");
                        egui::ComboBox::from_id_salt("timestamp_format")
                            .selected_text(state.settings.timestamp_format.to_string())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.settings.timestamp_format,
                                    TimestampFormat::Seconds,
                                    "Seconds",
                                );
                                ui.selectable_value(
                                    &mut state.settings.timestamp_format,
                                    TimestampFormat::Milliseconds,
                                    "Milliseconds",
                                );
                                ui.selectable_value(
                                    &mut state.settings.timestamp_format,
                                    TimestampFormat::Microseconds,
                                    "Microseconds",
                                );
                            });
                    });

                    ui.horizontal(|ui| {
                        ui.label("Field separator:");
                        let mut sep_str = state.settings.field_separator.to_string();
                        if ui
                            .add(egui::TextEdit::singleline(&mut sep_str).desired_width(30.0))
                            .changed()
                        {
                            if let Some(c) = sep_str.chars().next() {
                                state.settings.field_separator = c;
                            }
                        }
                    });
                });
            }

            // Statistics option
            ui.checkbox(&mut state.include_statistics, "Include statistics summary")
                .on_hover_text("Add min/max/mean/stddev for each variable");

            ui.add_space(8.0);

            // File path
            ui.heading("Output File");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.file_path_input)
                        .hint_text("Select file...")
                        .desired_width(300.0),
                );
                if ui.button("Browse...").clicked() {
                    action = DialogAction::Action(ExportConfigAction::BrowseFile);
                }
            });

            // Preview
            ui.add_space(8.0);
            ui.heading("Preview");
            let estimated_rows = state.estimate_row_count(ctx.total_samples, ctx.data_duration);
            let estimated_cols = selected_count
                + if state.settings.include_timestamps {
                    1
                } else {
                    0
                };

            ui.label(format!(
                "Estimated: {} rows x {} columns",
                estimated_rows, estimated_cols
            ));

            if state.format == ExportFormat::Csv && estimated_rows > 0 {
                // Show sample header preview
                let mut header_preview = Vec::new();
                if state.settings.include_timestamps {
                    header_preview.push("timestamp");
                }
                for var in ctx.variables.values() {
                    if (state.select_all || state.selected_variables.contains(&var.id))
                        && var.enabled
                    {
                        header_preview.push(&var.name);
                    }
                }
                let header_str = header_preview.join(&state.settings.field_separator.to_string());
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Header:").small());
                    ui.label(egui::RichText::new(&header_str).monospace().small());
                });
            }
        });

        ui.add_space(8.0);
        ui.separator();

        // Action buttons
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    action = DialogAction::Close;
                }

                let can_export = state.is_valid();
                if ui
                    .add_enabled(can_export, egui::Button::new("Export"))
                    .clicked()
                {
                    let file_path = PathBuf::from(&state.file_path_input);
                    action = DialogAction::CloseWithAction(ExportConfigAction::Export {
                        format: state.format,
                        settings: state.settings.clone(),
                        time_start: state.time_start,
                        time_end: state.time_end,
                        variables: if state.select_all {
                            ctx.variables
                                .values()
                                .filter(|v| v.enabled)
                                .map(|v| v.id)
                                .collect()
                        } else {
                            state.selected_variables.clone()
                        },
                        downsample_mode: state.downsample_mode,
                        include_statistics: state.include_statistics,
                        file_path,
                    });
                }
            });
        });

        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_config_state_default() {
        let state = ExportConfigState::default();
        assert_eq!(state.format, ExportFormat::Csv);
        assert!(state.select_all);
        assert_eq!(state.downsample_mode, DownsampleMode::None);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_estimate_row_count() {
        let state = ExportConfigState::default();

        // No downsampling
        assert_eq!(state.estimate_row_count(1000, 10.0), 1000);

        // With time range
        let mut state_with_range = state.clone();
        state_with_range.time_start = Some(2.0);
        state_with_range.time_end = Some(7.0);
        // 5 seconds out of 10 = 50%
        assert_eq!(state_with_range.estimate_row_count(1000, 10.0), 500);

        // With downsampling every 10th
        let mut state_downsampled = ExportConfigState::default();
        state_downsampled.downsample_mode = DownsampleMode::EveryNth(10);
        assert_eq!(state_downsampled.estimate_row_count(1000, 10.0), 100);
    }

    #[test]
    fn test_export_format() {
        assert_eq!(ExportFormat::Csv.extension(), "csv");
        assert_eq!(ExportFormat::Json.extension(), "json");
        assert_eq!(ExportFormat::Binary.extension(), "bin");
    }
}
