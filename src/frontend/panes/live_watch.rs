//! Live Watch pane - Keil-style on-demand inspection panel.
//!
//! Distinct from `Watcher` (flat read-only table of plotted variables) and
//! from `VariableBrowser` (the symbol picker). Live Watch entries:
//! - are persisted but not plotted/recorded,
//! - expand on demand to show struct members, array elements, and pointee
//!   contents,
//! - are read by a dedicated low-rate scheduler, gated on Start like the
//!   regular variable poll.

use egui::{Color32, Ui};

use crate::frontend::pane_trait::Pane;
use crate::frontend::state::{AppAction, SharedState};
use crate::frontend::workspace::PaneKind;
use crate::types::{ConnectionStatus, PointerState};
use crate::watch;
use crate::watch::{walk_root, WalkOutput, WatchAddress, WatchId, WatchRowKind};

#[derive(Default)]
pub struct LiveWatchState {
    /// Buffer for the ghost "add" row at the bottom of the table.
    pub add_input: String,
    /// Last attempt's error message (e.g. "symbol not in ELF").
    pub add_error: Option<String>,
    /// Root id currently being renamed inline (None = no rename in progress).
    pub rename_id: Option<WatchId>,
    /// Buffer for the rename text input.
    pub rename_buffer: String,
}

pub fn render(
    state: &mut LiveWatchState,
    shared: &mut SharedState<'_>,
    ui: &mut Ui,
) -> Vec<AppAction> {
    let actions: Vec<AppAction> = Vec::new();
    // All mutations to live_watches happen in this function directly; no
    // AppActions are emitted from this pane (the Variable Browser button
    // uses AppAction::AddWatchRoot as its entry point).

    // --- Header: live status indicator -----------------------------------
    ui.horizontal(|ui| {
        ui.heading("Live Watch");
        ui.separator();
        let connected =
            shared.state.topics.connection_status == ConnectionStatus::Connected;
        let collecting = shared.state.settings.collecting;
        let (dot_color, label) = if collecting && connected {
            (Color32::from_rgb(120, 200, 120), "live")
        } else if connected {
            (Color32::from_rgb(200, 180, 80), "stopped")
        } else {
            (Color32::DARK_GRAY, "no probe")
        };
        ui.colored_label(dot_color, crate::frontend::icons::STATUS_DOT);
        ui.label(label);
    });

    // --- Table -----------------------------------------------------------
    // Snapshot roots so we can mutate `expanded_paths`/`expression` while
    // iterating without borrow issues.
    let roots_snapshot: Vec<watch::WatchRoot> =
        shared.state.config.live_watches.clone();

    let mut to_remove: Option<WatchId> = None;
    let mut toggle_expand: Option<(WatchId, String)> = None;
    // Expression rename committed this frame: (root_id, new_expression).
    let mut rename_commit: Option<(WatchId, String)> = None;
    // New root committed via the ghost row this frame.
    let mut add_commit: Option<String> = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("live_watch_grid")
                .num_columns(4)
                .striped(true)
                .min_col_width(60.0)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    // Header row.
                    ui.strong("Name");
                    ui.strong("Type");
                    ui.strong("Address");
                    ui.strong("Value");
                    ui.end_row();

                    // Root rows + their walked subtrees.
                    for root in &roots_snapshot {
                        let resolution = watch::resolve_root(shared.ctx.elf_info, root);
                        let mut walk = WalkOutput::default();
                        walk_root(root, &resolution, &mut walk);

                        for (idx, row) in walk.rows.iter().enumerate() {
                            render_grid_row(
                                ui,
                                state,
                                shared,
                                root,
                                idx == 0,
                                row,
                                &mut to_remove,
                                &mut toggle_expand,
                                &mut rename_commit,
                            );
                        }
                    }

                    // Ghost-add row at the bottom.
                    render_add_row(ui, state, &mut add_commit);
                });
        });

    // --- Apply pane-local mutations --------------------------------------
    if let Some(id) = to_remove {
        shared.state.config.live_watches.retain(|r| r.id != id);
        if state.rename_id == Some(id) {
            state.rename_id = None;
        }
    }
    if let Some((id, path)) = toggle_expand {
        if let Some(r) = shared
            .state
            .config
            .live_watches
            .iter_mut()
            .find(|r| r.id == id)
        {
            r.toggle_expanded(&path);
        }
    }
    if let Some((id, new_expr)) = rename_commit {
        let trimmed = new_expr.trim().to_string();
        if trimmed.is_empty() {
            state.add_error = Some("Symbol name cannot be empty".to_string());
        } else if shared
            .ctx
            .elf_info
            .and_then(|i| i.find_symbol(&trimmed))
            .is_none()
        {
            state.add_error =
                Some(format!("'{}' not found in current ELF", trimmed));
        } else if let Some(r) = shared
            .state
            .config
            .live_watches
            .iter_mut()
            .find(|r| r.id == id)
        {
            r.expression = trimmed;
            // Clear expansions — they were keyed against the old type tree,
            // and the new symbol may have a totally different shape.
            r.expanded_paths.clear();
            r.toggle_expanded("");
            state.add_error = None;
        }
    }
    if let Some(name) = add_commit {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            state.add_error = Some("Enter a symbol name".to_string());
        } else if shared.ctx.elf_info.is_none() {
            state.add_error = Some("Load an ELF file first".to_string());
        } else if shared
            .ctx
            .elf_info
            .and_then(|i| i.find_symbol(&trimmed))
            .is_none()
        {
            state.add_error = Some(format!("'{}' not found in current ELF", trimmed));
        } else if shared
            .state
            .config
            .live_watches
            .iter()
            .any(|r| r.expression == trimmed)
        {
            state.add_error = Some(format!("'{}' is already being watched", trimmed));
            state.add_input.clear();
        } else {
            let mut new_root = watch::WatchRoot::new(&trimmed);
            new_root.toggle_expanded(""); // expand by default
            shared.state.config.live_watches.push(new_root);
            state.add_input.clear();
            state.add_error = None;
        }
    }

    if let Some(err) = &state.add_error {
        ui.colored_label(Color32::from_rgb(220, 100, 100), err);
    }

    actions
}

#[allow(clippy::too_many_arguments)]
fn render_grid_row(
    ui: &mut Ui,
    state: &mut LiveWatchState,
    shared: &SharedState<'_>,
    root: &watch::WatchRoot,
    is_root_row: bool,
    row: &watch::WatchRow,
    to_remove: &mut Option<WatchId>,
    toggle_expand: &mut Option<(WatchId, String)>,
    rename_commit: &mut Option<(WatchId, String)>,
) {
    // --- Column 1: indent + arrow + name (renameable on root rows) -----
    ui.horizontal(|ui| {
        ui.add_space((row.depth as f32) * 14.0);

        let can_expand = matches!(
            row.kind,
            WatchRowKind::Struct
                | WatchRowKind::Array { .. }
                | WatchRowKind::Pointer { can_expand: true }
        );
        if can_expand {
            let icon = if root.is_expanded(&row.path) {
                crate::frontend::icons::TREE_EXPANDED
            } else {
                crate::frontend::icons::TREE_COLLAPSED
            };
            if ui.small_button(icon).clicked() {
                *toggle_expand = Some((row.root_id, row.path.clone()));
            }
        } else {
            ui.add_space(18.0);
        }

        if is_root_row && state.rename_id == Some(row.root_id) {
            // Inline rename mode.
            let resp = ui.add(
                egui::TextEdit::singleline(&mut state.rename_buffer)
                    .desired_width(180.0),
            );
            resp.request_focus();
            let commit = resp.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if commit {
                *rename_commit = Some((row.root_id, state.rename_buffer.clone()));
                state.rename_id = None;
            } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                state.rename_id = None;
            }
        } else {
            let label_text = egui::RichText::new(&row.display_name);
            let label_text = if is_root_row {
                label_text.strong()
            } else {
                label_text
            };
            let resp = ui.add(
                egui::Label::new(label_text)
                    .sense(egui::Sense::click())
                    .truncate(),
            );
            if is_root_row && resp.double_clicked() {
                state.rename_id = Some(row.root_id);
                state.rename_buffer = root.expression.clone();
            }
            if is_root_row {
                resp.on_hover_text("Double-click to rename");
            }
        }
    });

    // --- Column 2: type ------------------------------------------------
    ui.label(
        egui::RichText::new(&row.type_name)
            .monospace()
            .color(Color32::DARK_GRAY),
    );

    // --- Column 3: address --------------------------------------------
    let addr_text = match &row.address {
        WatchAddress::Static(a) => format!("0x{:08X}", a),
        WatchAddress::PointerDeref { .. } => "<dynamic>".to_string(),
    };
    ui.label(
        egui::RichText::new(addr_text)
            .monospace()
            .color(Color32::DARK_GRAY),
    );

    // --- Column 4: value + (close button on root rows) ---------------
    ui.horizontal(|ui| {
        render_value_cell(ui, shared, row);
        if is_root_row {
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if ui
                        .small_button(crate::frontend::icons::CLOSE)
                        .on_hover_text("Remove watch")
                        .clicked()
                    {
                        *to_remove = Some(row.root_id);
                    }
                },
            );
        }
    });

    ui.end_row();
}

fn render_add_row(
    ui: &mut Ui,
    state: &mut LiveWatchState,
    add_commit: &mut Option<String>,
) {
    // Column 1: TextEdit spanning the name column.
    let resp = ui.add(
        egui::TextEdit::singleline(&mut state.add_input)
            .desired_width(200.0)
            .hint_text("(add symbol)"),
    );
    let commit =
        resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if commit && !state.add_input.is_empty() {
        *add_commit = Some(state.add_input.clone());
    }

    // Empty cells for alignment.
    ui.label("");
    ui.label("");
    ui.label("");
    ui.end_row();
}

fn render_value_cell(ui: &mut Ui, shared: &SharedState<'_>, row: &watch::WatchRow) {
    let key = (row.root_id, row.path.clone());
    let value = shared.state.topics.watch_values.get(&key);

    match &row.kind {
        WatchRowKind::Struct => {
            ui.label(egui::RichText::new("{...}").color(Color32::DARK_GRAY));
        }
        WatchRowKind::Array { count, truncated } => {
            let label = if *truncated {
                format!("[{}+]", count)
            } else {
                format!("[{}]", count)
            };
            ui.label(egui::RichText::new(label).color(Color32::DARK_GRAY));
        }
        WatchRowKind::Opaque => {
            ui.label(egui::RichText::new("-").color(Color32::DARK_GRAY));
        }
        WatchRowKind::Pointer { .. } => match value {
            Some(v) => match (&v.raw, v.pointer_state) {
                (Ok(addr), Some(PointerState::Null)) => {
                    ui.colored_label(
                        Color32::YELLOW,
                        format!("NULL (0x{:08X})", *addr as u64),
                    );
                }
                (Ok(addr), Some(PointerState::Invalid(_))) => {
                    ui.colored_label(
                        Color32::from_rgb(220, 100, 100),
                        format!("INVALID 0x{:08X}", *addr as u64),
                    );
                }
                (Ok(addr), Some(PointerState::Valid(_))) => {
                    ui.label(
                        egui::RichText::new(format!("-> 0x{:08X}", *addr as u64))
                            .monospace(),
                    );
                }
                (Ok(addr), _) => {
                    ui.label(
                        egui::RichText::new(format!("0x{:08X}", *addr as u64))
                            .monospace(),
                    );
                }
                (Err(e), _) => {
                    ui.colored_label(
                        Color32::from_rgb(220, 100, 100),
                        format!("ERR: {}", e),
                    );
                }
            },
            None => {
                ui.label(egui::RichText::new("-").color(Color32::DARK_GRAY));
            }
        },
        WatchRowKind::Primitive => match value {
            Some(v) => match &v.raw {
                Ok(raw) => {
                    let text = format_primitive(*raw, row.var_type);
                    ui.label(egui::RichText::new(text).monospace());
                }
                Err(e) => {
                    ui.colored_label(
                        Color32::from_rgb(220, 100, 100),
                        format!("ERR: {}", e),
                    );
                }
            },
            None => {
                ui.label(egui::RichText::new("-").color(Color32::DARK_GRAY));
            }
        },
    }
}

fn format_primitive(raw: f64, t: crate::types::VariableType) -> String {
    use crate::types::VariableType::*;
    match t {
        Bool => {
            if raw != 0.0 {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        F32 | F64 => format!("{}", raw),
        _ => format!("{}", raw as i64),
    }
}

impl Pane for LiveWatchState {
    fn kind(&self) -> PaneKind {
        PaneKind::LiveWatch
    }

    fn render(&mut self, shared: &mut SharedState, ui: &mut Ui) -> Vec<AppAction> {
        render(self, shared, ui)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let s = LiveWatchState::default();
        assert!(s.add_input.is_empty());
        assert!(s.add_error.is_none());
        assert!(s.rename_id.is_none());
    }

    #[test]
    fn test_format_primitive_bool() {
        use crate::types::VariableType::Bool;
        assert_eq!(format_primitive(0.0, Bool), "false");
        assert_eq!(format_primitive(1.0, Bool), "true");
    }
}
