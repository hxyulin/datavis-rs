//! Default workspace layout
//!
//! Builds the initial dock layout with variable browser + variable list on the left,
//! and time series + settings tabs on the right.

use egui_dock::{DockState, NodeIndex};

use super::{PaneKind, Workspace};

/// Build the default dock layout and return the DockState.
///
/// Layout:
/// ```text
/// ┌──────────────┬────────────────────────────────┐
/// │  Variable    │ [Time Series]  [Settings]       │
/// │  Browser     ├────────────────────────────────┤
/// │──────────────│                                │
/// │  Variables   │         Time Series Graph      │
/// │  (list)      │                                │
/// └──────────────┴────────────────────────────────┘
/// ```
pub fn build_default_layout(workspace: &mut Workspace) -> DockState<super::PaneId> {
    let browser_id = workspace.register_pane(PaneKind::VariableBrowser, "Variable Browser");
    let varlist_id = workspace.register_pane(PaneKind::VariableList, "Variables");
    let timeseries_id = workspace.register_pane(PaneKind::TimeSeries, "Time Series");
    let settings_id = workspace.register_pane(PaneKind::Settings, "Settings");

    // Start with time series as the main tab
    let mut dock = DockState::new(vec![timeseries_id]);

    // Add settings as a second tab (behind time series)
    dock.push_to_first_leaf(settings_id);

    // Split left 25% for variable browser
    let [_center, left] = dock
        .main_surface_mut()
        .split_left(NodeIndex::root(), 0.25, vec![browser_id]);

    // Split left panel vertically: top = browser, bottom = variable list
    dock.main_surface_mut()
        .split_below(left, 0.5, vec![varlist_id]);

    dock
}
