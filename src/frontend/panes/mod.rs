//! Pane modules for the workspace
//!
//! Each pane provides a render function that takes its own state, SharedState, and &mut Ui.
//! Panes return Vec<AppAction> instead of mutating state directly.

pub mod live_watch;
pub mod recorder;
pub mod time_series;
pub mod variable_list;

pub use live_watch::LiveWatchState;
pub use recorder::RecorderPaneState;
pub use time_series::TimeSeriesState;
pub use variable_list::VariableListState;
