//! Pane modules for the workspace
//!
//! Each pane provides a render function that takes its own state, SharedState, and &mut Ui.
//! Panes return Vec<AppAction> instead of mutating state directly.

pub mod fft_view;
pub mod recorder;
pub mod time_series;
pub mod variable_browser;
pub mod variable_list;
pub mod watcher;

pub use fft_view::FftViewState;
pub use recorder::RecorderPaneState;
pub use time_series::TimeSeriesState;
pub use variable_browser::VariableBrowserState;
pub use variable_list::VariableListState;
pub use watcher::WatcherState;
