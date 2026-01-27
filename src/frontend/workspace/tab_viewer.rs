//! TabViewer implementation for the workspace
//!
//! Dispatches rendering to individual pane modules based on PaneKind.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use egui::{Ui, WidgetText};

use crate::backend::{ElfInfo, ElfSymbol, FrontendReceiver};
use crate::config::settings::RuntimeSettings;
use crate::config::{AppConfig, AppState, DataPersistenceConfig};
use crate::frontend::panes;
use crate::frontend::state::{AppAction, SharedState};
use crate::types::{CollectionStats, ConnectionStatus, VariableData};

use super::{PaneEntry, PaneId, PaneKind, PaneState};

/// Tab viewer that bridges egui_dock with our pane system.
///
/// Holds mutable borrows to all shared state fields so that
/// SharedState can be constructed per-frame inside ui().
pub struct WorkspaceTabViewer<'a> {
    pub frontend: &'a FrontendReceiver,
    pub connection_status: ConnectionStatus,
    pub config: &'a mut AppConfig,
    pub settings: &'a mut RuntimeSettings,
    pub app_state: &'a mut AppState,
    pub variable_data: &'a mut HashMap<u32, VariableData>,
    pub stats: &'a CollectionStats,
    pub elf_info: Option<&'a ElfInfo>,
    pub elf_symbols: &'a [ElfSymbol],
    pub elf_file_path: Option<&'a PathBuf>,
    pub persistence_config: &'a mut DataPersistenceConfig,
    pub last_error: &'a mut Option<String>,
    pub start_time: Instant,
    // Workspace state
    pub pane_states: &'a mut HashMap<PaneId, PaneState>,
    pub pane_entries: &'a HashMap<PaneId, PaneEntry>,
    pub actions: Vec<AppAction>,
}

impl egui_dock::TabViewer for WorkspaceTabViewer<'_> {
    type Tab = PaneId;

    fn title(&mut self, tab: &mut PaneId) -> WidgetText {
        self.pane_entries
            .get(tab)
            .map(|e| WidgetText::from(&e.title))
            .unwrap_or_else(|| WidgetText::from("Unknown"))
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut PaneId) {
        let Some(entry) = self.pane_entries.get(tab) else {
            ui.label("Pane not found");
            return;
        };
        let kind = entry.kind;

        let Some(state) = self.pane_states.get_mut(tab) else {
            ui.label("Pane state not found");
            return;
        };

        // Construct SharedState from individual borrows
        let mut shared = SharedState {
            frontend: self.frontend,
            connection_status: self.connection_status,
            config: self.config,
            settings: self.settings,
            app_state: self.app_state,
            variable_data: self.variable_data,
            stats: self.stats,
            elf_info: self.elf_info,
            elf_symbols: self.elf_symbols,
            elf_file_path: self.elf_file_path,
            persistence_config: self.persistence_config,
            last_error: self.last_error,
            start_time: self.start_time,
        };

        // Dispatch to the appropriate pane render function
        let pane_actions = match (kind, state) {
            (PaneKind::VariableBrowser, PaneState::VariableBrowser(s)) => {
                panes::variable_browser::render(s, &mut shared, ui)
            }
            (PaneKind::VariableList, PaneState::VariableList(s)) => {
                panes::variable_list::render(s, &mut shared, ui)
            }
            (PaneKind::Settings, PaneState::Settings(s)) => {
                panes::settings::render(s, &mut shared, ui)
            }
            (PaneKind::TimeSeries, PaneState::TimeSeries(s)) => {
                panes::time_series::render(s, &mut shared, ui)
            }
            (PaneKind::Watcher, PaneState::Watcher(s)) => {
                panes::watcher::render(s, &mut shared, ui)
            }
            (PaneKind::FftView, PaneState::FftView(s)) => {
                panes::fft_view::render(s, &mut shared, ui)
            }
            _ => {
                ui.label("Mismatched pane kind/state");
                Vec::new()
            }
        };

        self.actions.extend(pane_actions);
    }

    fn on_close(&mut self, tab: &mut PaneId) -> egui_dock::widgets::tab_viewer::OnCloseResponse {
        // Allow closing; cleanup happens in the main app
        self.actions.push(AppAction::ClosePane(*tab));
        egui_dock::widgets::tab_viewer::OnCloseResponse::Close
    }

    fn closeable(&mut self, _tab: &mut PaneId) -> bool {
        true
    }
}
