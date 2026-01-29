//! TabViewer implementation for the workspace
//!
//! Dispatches rendering to individual pane modules via the Pane trait.

use std::collections::HashMap;
use std::path::PathBuf;

use egui::{Ui, WidgetText};

use crate::backend::{ElfInfo, ElfSymbol};
use crate::config::settings::RuntimeSettings;
use crate::config::{AppConfig, AppState, DataPersistenceConfig};
use crate::frontend::pane_trait::Pane;
use crate::frontend::state::{AppAction, SharedState};
use crate::frontend::topics::Topics;
use crate::pipeline::bridge::PipelineBridge;

use super::{PaneEntry, PaneId, PaneKind};

/// Tab viewer that bridges egui_dock with our pane system.
///
/// Holds mutable borrows to all shared state fields so that
/// SharedState can be constructed per-frame inside ui().
pub struct WorkspaceTabViewer<'a> {
    pub frontend: &'a PipelineBridge,
    pub config: &'a mut AppConfig,
    pub settings: &'a mut RuntimeSettings,
    pub app_state: &'a mut AppState,
    pub elf_info: Option<&'a ElfInfo>,
    pub elf_symbols: &'a [ElfSymbol],
    pub elf_file_path: Option<&'a PathBuf>,
    pub persistence_config: &'a mut DataPersistenceConfig,
    pub last_error: &'a mut Option<String>,
    pub display_time: f64,
    pub topics: &'a mut Topics,
    // Workspace state
    pub pane_states: &'a mut HashMap<PaneId, Box<dyn Pane>>,
    pub pane_entries: &'a HashMap<PaneId, PaneEntry>,
    pub actions: Vec<AppAction>,
    // Pane kind lists for the "+" popup
    pub singleton_pane_kinds: Vec<(PaneKind, &'static str)>,
    pub multi_pane_kinds: Vec<(PaneKind, &'static str)>,
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
        let Some(pane) = self.pane_states.get_mut(tab) else {
            ui.label("Pane not found");
            return;
        };

        // Construct SharedState from individual borrows
        let mut shared = SharedState {
            frontend: self.frontend,
            config: self.config,
            settings: self.settings,
            app_state: self.app_state,
            elf_info: self.elf_info,
            elf_symbols: self.elf_symbols,
            elf_file_path: self.elf_file_path,
            persistence_config: self.persistence_config,
            last_error: self.last_error,
            display_time: self.display_time,
            topics: self.topics,
            current_pane_id: Some(*tab),
        };

        // Polymorphic dispatch via Pane trait
        let pane_actions = pane.render(&mut shared, ui);
        self.actions.extend(pane_actions);
    }

    fn on_close(&mut self, tab: &mut PaneId) -> egui_dock::widgets::tab_viewer::OnCloseResponse {
        // Allow closing; cleanup happens in the main app
        self.actions.push(AppAction::ClosePane(*tab));
        egui_dock::widgets::tab_viewer::OnCloseResponse::Close
    }

    fn is_closeable(&self, _tab: &PaneId) -> bool {
        true
    }

    fn add_popup(
        &mut self,
        ui: &mut Ui,
        _surface: egui_dock::SurfaceIndex,
        _node: egui_dock::NodeIndex,
    ) {
        ui.set_min_width(150.0);
        ui.label("Add Pane");
        ui.separator();

        // Multi-instance visualizers
        for &(kind, name) in &self.multi_pane_kinds {
            if ui.button(format!("New {}", name)).clicked() {
                self.actions.push(AppAction::NewVisualizer(kind));
                ui.close();
            }
        }

        if !self.multi_pane_kinds.is_empty() && !self.singleton_pane_kinds.is_empty() {
            ui.separator();
        }

        // Singletons (only show if not already open)
        for &(kind, name) in &self.singleton_pane_kinds {
            let already_open = self.pane_entries.values().any(|e| e.kind == kind);
            ui.add_enabled_ui(!already_open, |ui| {
                if ui.button(name).clicked() {
                    self.actions.push(AppAction::OpenPane(kind));
                    ui.close();
                }
            });
        }
    }
}
