//! Default workspace layout
//!
//! Builds a 3-pane layout: Variable Browser and Variable List on the left, Time Series on the right.
//! Settings are accessed via toolbar, menu dropdowns, and dialogs instead of a pane.

use egui_dock::{DockState, NodeIndex};

use super::{PaneKind, Workspace};

/// Build the default dock layout and return the DockState.
///
/// Layout:
/// ```text
/// ┌──────────────┬────────────────────────────────┐
/// │  Variable    │                                │
/// │  Browser     │                                │
/// ├──────────────┤     Time Series Plot           │
/// │  Variable    │                                │
/// │  List        │                                │
/// └──────────────┴────────────────────────────────┘
/// ```
pub fn build_default_layout(workspace: &mut Workspace) -> DockState<super::PaneId> {
    let browser_id = workspace.register_pane(PaneKind::VariableBrowser, "Variable Browser");
    let varlist_id = workspace.register_pane(PaneKind::VariableList, "Variable List");
    let timeseries_id = workspace.register_pane(PaneKind::TimeSeries, "Time Series");

    let mut dock = DockState::new(vec![timeseries_id]);

    // Split left 25% for variable browser
    let [_right, left] = dock
        .main_surface_mut()
        .split_left(NodeIndex::root(), 0.25, vec![browser_id]);

    // Split the left panel: browser on top (60%), variable list below (40%)
    dock.main_surface_mut()
        .split_below(left, 0.6, vec![varlist_id]);

    dock
}
