//! Watcher pane - Live value table
//!
//! Simple table showing current variable values, stats, and point counts.

use egui::{Color32, Ui};

use crate::frontend::pane_trait::Pane;
use crate::frontend::state::{AppAction, SharedState};
use crate::frontend::workspace::PaneKind;

/// State for the Watcher pane
#[derive(Default)]
pub struct WatcherState {
    /// Whether to show statistics columns
    pub show_stats: bool,
    /// Whether to show raw (unconverted) values
    pub show_raw: bool,
}

/// Render the watcher pane
pub fn render(
    state: &mut WatcherState,
    shared: &mut SharedState<'_>,
    ui: &mut Ui,
) -> Vec<AppAction> {
    ui.horizontal(|ui| {
        ui.heading("Watcher");
        ui.separator();
        ui.checkbox(&mut state.show_stats, "Stats");
        ui.checkbox(&mut state.show_raw, "Raw");
    });
    ui.separator();

    if shared.config.variables.is_empty() {
        ui.colored_label(Color32::GRAY, "No variables configured");
        return Vec::new();
    }

    let num_cols = if state.show_stats {
        if state.show_raw {
            8
        } else {
            7
        }
    } else if state.show_raw {
        4
    } else {
        3
    };

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("watcher_grid")
                .num_columns(num_cols)
                .striped(true)
                .min_col_width(60.0)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    // Header
                    ui.strong("Name");
                    ui.strong("Value");
                    ui.strong("Unit");
                    if state.show_raw {
                        ui.strong("Raw");
                    }
                    if state.show_stats {
                        ui.strong("Min");
                        ui.strong("Max");
                        ui.strong("Avg");
                        ui.strong("Points");
                    }
                    ui.end_row();

                    for var in &shared.config.variables {
                        if !var.enabled {
                            continue;
                        }

                        let color = Color32::from_rgba_unmultiplied(
                            var.color[0],
                            var.color[1],
                            var.color[2],
                            var.color[3],
                        );

                        ui.colored_label(color, &var.name);

                        if let Some(data) = shared.topics.variable_data.get(&var.id) {
                            if let Some(last) = data.last() {
                                ui.label(
                                    egui::RichText::new(format!("{:.4}", last.converted_value))
                                        .monospace(),
                                );
                            } else {
                                ui.label("—");
                            }

                            ui.label(if var.unit.is_empty() {
                                "—"
                            } else {
                                &var.unit
                            });

                            if state.show_raw {
                                if let Some(last) = data.last() {
                                    ui.label(
                                        egui::RichText::new(format!("{:.6}", last.raw_value))
                                            .monospace()
                                            .small(),
                                    );
                                } else {
                                    ui.label("—");
                                }
                            }

                            if state.show_stats {
                                let (min, max, avg) = data.statistics();
                                ui.label(format!("{:.4}", min));
                                ui.label(format!("{:.4}", max));
                                ui.label(format!("{:.4}", avg));
                                ui.label(format!("{}", data.data_points.len()));
                            }
                        } else {
                            ui.label("—");
                            ui.label("—");
                            if state.show_raw {
                                ui.label("—");
                            }
                            if state.show_stats {
                                ui.label("—");
                                ui.label("—");
                                ui.label("—");
                                ui.label("0");
                            }
                        }

                        ui.end_row();
                    }
                });
        });

    Vec::new()
}

impl Pane for WatcherState {
    fn kind(&self) -> PaneKind { PaneKind::Watcher }

    fn render(&mut self, shared: &mut SharedState, ui: &mut Ui) -> Vec<AppAction> {
        render(self, shared, ui)
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
