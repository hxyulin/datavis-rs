//! Exporter pane — controls the pipeline ExporterSinkNode.
//!
//! Provides file export controls (path, format, start/stop) and status display.

use egui::Ui;

use crate::frontend::pane_trait::Pane;
use crate::frontend::state::{AppAction, SharedState};
use crate::frontend::workspace::PaneKind;
use crate::pipeline::id::NodeId;
use crate::pipeline::packet::ConfigValue;

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

    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Csv => "csv",
            ExportFormat::Json => "json",
        }
    }
}

/// State for the Exporter pane.
pub struct ExporterPaneState {
    /// Output file path.
    pub export_path: String,
    /// Selected export format.
    pub format: ExportFormat,
}

impl Default for ExporterPaneState {
    fn default() -> Self {
        Self {
            export_path: String::new(),
            format: ExportFormat::Csv,
        }
    }
}

/// Render the exporter pane.
pub fn render(
    state: &mut ExporterPaneState,
    shared: &mut SharedState<'_>,
    ui: &mut Ui,
) -> Vec<AppAction> {
    let mut actions = Vec::new();
    let exporter_node_id = shared.topics.exporter_node_id;

    ui.heading("Data Exporter");
    ui.separator();

    // --- Status ---
    render_status(shared, ui);
    ui.separator();

    // --- Controls ---
    render_controls(state, shared, ui, &mut actions, exporter_node_id);

    actions
}

fn render_status(shared: &SharedState<'_>, ui: &mut Ui) {
    ui.horizontal(|ui| {
        if shared.topics.exporter_active {
            ui.colored_label(egui::Color32::from_rgb(100, 255, 100), "● Active");
            ui.label(format!("{} rows written", shared.topics.exporter_rows_written));
        } else {
            ui.label("Inactive");
        }
    });
}

fn render_controls(
    state: &mut ExporterPaneState,
    shared: &SharedState<'_>,
    ui: &mut Ui,
    actions: &mut Vec<AppAction>,
    exporter_node_id: NodeId,
) {
    // File path
    ui.horizontal(|ui| {
        ui.label("Output path:");
        ui.add(
            egui::TextEdit::singleline(&mut state.export_path)
                .hint_text("Select file...")
                .desired_width(250.0),
        );
        if ui.button("Browse...").clicked() {
            let filter = match state.format {
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
        ui.selectable_value(&mut state.format, ExportFormat::Csv, "CSV");
        ui.selectable_value(&mut state.format, ExportFormat::Json, "JSON");
    });

    ui.separator();

    // Start/Stop
    ui.horizontal(|ui| {
        if shared.topics.exporter_active {
            if ui.button("Stop Export").clicked() {
                actions.push(AppAction::NodeConfig {
                    node_id: exporter_node_id,
                    key: "stop".to_string(),
                    value: ConfigValue::Bool(true),
                });
            }
        } else {
            let can_start = !state.export_path.is_empty();
            ui.add_enabled_ui(can_start, |ui| {
                if ui.button("Start Export").clicked() {
                    // Send path and format config, then start
                    actions.push(AppAction::NodeConfig {
                        node_id: exporter_node_id,
                        key: "path".to_string(),
                        value: ConfigValue::String(state.export_path.clone()),
                    });
                    actions.push(AppAction::NodeConfig {
                        node_id: exporter_node_id,
                        key: "format".to_string(),
                        value: ConfigValue::String(state.format.display_name().to_lowercase()),
                    });
                    actions.push(AppAction::NodeConfig {
                        node_id: exporter_node_id,
                        key: "start".to_string(),
                        value: ConfigValue::Bool(true),
                    });
                }
            });
        }
    });
}

impl Pane for ExporterPaneState {
    fn kind(&self) -> PaneKind { PaneKind::Exporter }

    fn render(&mut self, shared: &mut SharedState, ui: &mut Ui) -> Vec<AppAction> {
        render(self, shared, ui)
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exporter_pane_state_default() {
        let state = ExporterPaneState::default();

        assert_eq!(state.export_path, "");
        assert_eq!(state.format, ExportFormat::Csv);
    }

    #[test]
    fn test_export_format_display_names() {
        assert_eq!(ExportFormat::Csv.display_name(), "CSV");
        assert_eq!(ExportFormat::Json.display_name(), "JSON");
    }

    #[test]
    fn test_export_format_extensions() {
        assert_eq!(ExportFormat::Csv.extension(), "csv");
        assert_eq!(ExportFormat::Json.extension(), "json");
    }

    #[test]
    fn test_export_path_update() {
        let mut state = ExporterPaneState::default();

        state.export_path = "/tmp/test.csv".to_string();
        assert_eq!(state.export_path, "/tmp/test.csv");

        state.export_path = "/data/output.json".to_string();
        assert_eq!(state.export_path, "/data/output.json");
    }

    #[test]
    fn test_format_switching() {
        let mut state = ExporterPaneState::default();

        assert_eq!(state.format, ExportFormat::Csv);

        state.format = ExportFormat::Json;
        assert_eq!(state.format, ExportFormat::Json);

        state.format = ExportFormat::Csv;
        assert_eq!(state.format, ExportFormat::Csv);
    }

    #[test]
    fn test_export_format_equality() {
        assert_eq!(ExportFormat::Csv, ExportFormat::Csv);
        assert_eq!(ExportFormat::Json, ExportFormat::Json);
        assert_ne!(ExportFormat::Csv, ExportFormat::Json);
    }
}
