//! Pane registry — data-driven pane registration.
//!
//! The registry is the single source of truth for all pane kinds:
//! display names, singleton flags, and factory functions.
//! The View menu and workspace pane creation are driven from this data.

use crate::frontend::pane_trait::Pane;
use crate::frontend::panes::{
    FftViewState, PipelineEditorState, RecorderPaneState,
    TimeSeriesState, VariableBrowserState, VariableListState, WatcherState,
};
use crate::frontend::workspace::PaneKind;

/// Metadata for a pane kind, including its factory function.
pub struct PaneKindInfo {
    pub kind: PaneKind,
    pub display_name: &'static str,
    pub is_singleton: bool,
    pub factory: fn() -> Box<dyn Pane>,
}

/// Build the pane registry with all known pane kinds.
pub fn build_registry() -> Vec<PaneKindInfo> {
    vec![
        // Singletons
        PaneKindInfo {
            kind: PaneKind::VariableBrowser,
            display_name: "Variable Browser",
            is_singleton: true,
            factory: || Box::new(VariableBrowserState::default()),
        },
        PaneKindInfo {
            kind: PaneKind::VariableList,
            display_name: "Variables",
            is_singleton: true,
            factory: || Box::new(VariableListState::default()),
        },
        PaneKindInfo {
            kind: PaneKind::Recorder,
            display_name: "Session Capture",
            is_singleton: true,
            factory: || Box::new(RecorderPaneState::default()),
        },
        PaneKindInfo {
            kind: PaneKind::PipelineEditor,
            display_name: "Pipeline Editor",
            is_singleton: true,
            factory: || Box::new(PipelineEditorState::default()),
        },
        // Multi-instance visualizers
        PaneKindInfo {
            kind: PaneKind::TimeSeries,
            display_name: "Time Series",
            is_singleton: false,
            factory: || Box::new(TimeSeriesState::default()),
        },
        PaneKindInfo {
            kind: PaneKind::Watcher,
            display_name: "Watcher",
            is_singleton: false,
            factory: || Box::new(WatcherState::default()),
        },
        PaneKindInfo {
            kind: PaneKind::FftView,
            display_name: "FFT View",
            is_singleton: false,
            factory: || Box::new(FftViewState::default()),
        },
    ]
}
