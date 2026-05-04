//! Default workspace layout.
//!
//! Layout (after architecture redesign):
//! ```text
//! ┌──────────────┬──────────────────────┬────────────┐
//! │              │                      │            │
//! │  Live Watch  │  Time Series Plot    │ Variables  │
//! │              │                      │            │
//! │              ├──────────────────────┴────────────┤
//! │              │  Session Capture (collapsed)      │
//! └──────────────┴───────────────────────────────────┘
//! ```

use egui_dock::{DockState, NodeIndex};

use super::{PaneKind, Workspace};

pub fn build_default_layout(workspace: &mut Workspace) -> DockState<super::PaneId> {
    let timeseries_id = workspace.register_pane(PaneKind::TimeSeries, "Time Series");
    let live_watch_id = workspace.register_pane(PaneKind::LiveWatch, "Live Watch");
    let var_list_id = workspace.register_pane(PaneKind::VariableList, "Variables");
    let recorder_id = workspace.register_pane(PaneKind::Recorder, "Session Capture");

    let mut dock = DockState::new(vec![timeseries_id]);

    // Left dock: Live Watch (25%)
    dock.main_surface_mut()
        .split_left(NodeIndex::root(), 0.25, vec![live_watch_id]);

    // Right dock: Variables (20%)
    dock.main_surface_mut()
        .split_right(NodeIndex::root(), 0.80, vec![var_list_id]);

    // Bottom dock: Recorder (collapsed-ish, 75/25 split)
    dock.main_surface_mut()
        .split_below(NodeIndex::root(), 0.75, vec![recorder_id]);

    dock
}
