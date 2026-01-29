//! Menu event handling - maps muda events to AppAction

use crate::frontend::state::AppAction;
use crate::frontend::workspace::PaneKind;

use super::ids::MenuId;

/// Menu event types that need special handling
#[derive(Debug, Clone)]
pub enum MenuEvent {
    /// Standard action that can be converted to AppAction
    Action(AppAction),
    /// Toggle toolbar visibility
    ToggleToolbar,
    /// Toggle status bar visibility
    ToggleStatusBar,
    /// Open connection settings dialog
    OpenConnectionSettings,
    /// Open collection settings dialog
    OpenCollectionSettings,
    /// Open persistence settings dialog
    OpenPersistenceSettings,
    /// Open preferences dialog
    OpenPreferences,
    /// Open ELF symbols browser
    OpenElfSymbols,
    /// Open help dialog
    OpenHelp,
    /// Open keyboard shortcuts
    OpenShortcuts,
    /// Open about dialog
    OpenAbout,
    /// Load ELF file (triggers file picker)
    LoadElf,
    /// Open project (triggers file picker)
    OpenProject,
    /// Save project
    SaveProject,
    /// Save project as (triggers file picker)
    SaveProjectAs,
    /// Load recent project by index
    LoadRecentProject(usize),
    /// Quit application
    Quit,
}

impl MenuEvent {
    /// Convert a menu ID to a menu event
    pub fn from_menu_id(id: &MenuId) -> Option<Self> {
        match id {
            // File menu
            MenuId::FileNewProject => Some(MenuEvent::Action(AppAction::NewProject)),
            MenuId::FileOpenProject => Some(MenuEvent::OpenProject),
            MenuId::FileQuit => Some(MenuEvent::Quit),

            // Project menu
            MenuId::ProjectSave => Some(MenuEvent::SaveProject),
            MenuId::ProjectSaveAs => Some(MenuEvent::SaveProjectAs),
            MenuId::ProjectRename => None, // Handled separately (needs text input)
            MenuId::ProjectRecent(n) => Some(MenuEvent::LoadRecentProject(*n)),

            // View menu
            MenuId::ViewToolbar => Some(MenuEvent::ToggleToolbar),
            MenuId::ViewStatusBar => Some(MenuEvent::ToggleStatusBar),
            MenuId::ViewVariableBrowser => {
                Some(MenuEvent::Action(AppAction::OpenPane(PaneKind::VariableBrowser)))
            }
            MenuId::ViewVariableList => {
                Some(MenuEvent::Action(AppAction::OpenPane(PaneKind::VariableList)))
            }
            MenuId::ViewSessionCapture => {
                Some(MenuEvent::Action(AppAction::OpenPane(PaneKind::Recorder)))
            }
            MenuId::ViewNewTimeSeries => {
                Some(MenuEvent::Action(AppAction::NewVisualizer(PaneKind::TimeSeries)))
            }
            MenuId::ViewNewWatcher => {
                Some(MenuEvent::Action(AppAction::NewVisualizer(PaneKind::Watcher)))
            }
            MenuId::ViewNewFft => {
                Some(MenuEvent::Action(AppAction::NewVisualizer(PaneKind::FftView)))
            }
            MenuId::ViewResetLayout => Some(MenuEvent::Action(AppAction::ResetLayout)),

            // Tools menu
            MenuId::ToolsConnectionSettings => Some(MenuEvent::OpenConnectionSettings),
            MenuId::ToolsLoadElf => Some(MenuEvent::LoadElf),
            MenuId::ToolsBrowseSymbols => Some(MenuEvent::OpenElfSymbols),
            MenuId::ToolsCollectionSettings => Some(MenuEvent::OpenCollectionSettings),
            MenuId::ToolsPersistence => Some(MenuEvent::OpenPersistenceSettings),
            MenuId::ToolsPreferences => Some(MenuEvent::OpenPreferences),

            // Help menu
            MenuId::HelpGettingStarted => Some(MenuEvent::OpenHelp),
            MenuId::HelpShortcuts => Some(MenuEvent::OpenShortcuts),
            MenuId::HelpAbout => Some(MenuEvent::OpenAbout),
        }
    }
}
