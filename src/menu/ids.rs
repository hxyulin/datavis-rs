//! Menu item identifiers for event handling

use muda::MenuId as MudaMenuId;

/// Menu item identifiers
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MenuId {
    // Project
    ProjectRename,
    ProjectSave,
    ProjectSaveAs,
    ProjectRecent(usize),

    // File
    FileNewProject,
    FileOpenProject,
    FileQuit,

    // View
    ViewToolbar,
    ViewStatusBar,
    ViewVariableBrowser,
    ViewVariableList,
    ViewSessionCapture,
    ViewNewTimeSeries,
    ViewNewWatcher,
    ViewNewFft,
    ViewResetLayout,

    // Tools
    ToolsConnectionSettings,
    ToolsLoadElf,
    ToolsBrowseSymbols,
    ToolsCollectionSettings,
    ToolsPersistence,
    ToolsPreferences,

    // Help
    HelpGettingStarted,
    HelpShortcuts,
    HelpAbout,
}

impl MenuId {
    /// Convert to muda MenuId
    pub fn to_muda_id(&self) -> MudaMenuId {
        MudaMenuId::new(self.as_str())
    }

    /// Try to parse from a muda MenuId
    pub fn from_muda_id(id: &MudaMenuId) -> Option<Self> {
        Self::from_str(id.as_ref())
    }

    fn as_str(&self) -> &str {
        match self {
            Self::ProjectRename => "project_rename",
            Self::ProjectSave => "project_save",
            Self::ProjectSaveAs => "project_save_as",
            Self::ProjectRecent(n) => {
                // We'll handle recent projects specially in the event handler
                // by checking the prefix
                Box::leak(format!("project_recent_{}", n).into_boxed_str())
            }
            Self::FileNewProject => "file_new_project",
            Self::FileOpenProject => "file_open_project",
            Self::FileQuit => "file_quit",
            Self::ViewToolbar => "view_toolbar",
            Self::ViewStatusBar => "view_status_bar",
            Self::ViewVariableBrowser => "view_variable_browser",
            Self::ViewVariableList => "view_variable_list",
            Self::ViewSessionCapture => "view_session_capture",
            Self::ViewNewTimeSeries => "view_new_time_series",
            Self::ViewNewWatcher => "view_new_watcher",
            Self::ViewNewFft => "view_new_fft",
            Self::ViewResetLayout => "view_reset_layout",
            Self::ToolsConnectionSettings => "tools_connection_settings",
            Self::ToolsLoadElf => "tools_load_elf",
            Self::ToolsBrowseSymbols => "tools_browse_symbols",
            Self::ToolsCollectionSettings => "tools_collection_settings",
            Self::ToolsPersistence => "tools_persistence",
            Self::ToolsPreferences => "tools_preferences",
            Self::HelpGettingStarted => "help_getting_started",
            Self::HelpShortcuts => "help_shortcuts",
            Self::HelpAbout => "help_about",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "project_rename" => Some(Self::ProjectRename),
            "project_save" => Some(Self::ProjectSave),
            "project_save_as" => Some(Self::ProjectSaveAs),
            "file_new_project" => Some(Self::FileNewProject),
            "file_open_project" => Some(Self::FileOpenProject),
            "file_quit" => Some(Self::FileQuit),
            "view_toolbar" => Some(Self::ViewToolbar),
            "view_status_bar" => Some(Self::ViewStatusBar),
            "view_variable_browser" => Some(Self::ViewVariableBrowser),
            "view_variable_list" => Some(Self::ViewVariableList),
            "view_session_capture" => Some(Self::ViewSessionCapture),
            "view_new_time_series" => Some(Self::ViewNewTimeSeries),
            "view_new_watcher" => Some(Self::ViewNewWatcher),
            "view_new_fft" => Some(Self::ViewNewFft),
            "view_reset_layout" => Some(Self::ViewResetLayout),
            "tools_connection_settings" => Some(Self::ToolsConnectionSettings),
            "tools_load_elf" => Some(Self::ToolsLoadElf),
            "tools_browse_symbols" => Some(Self::ToolsBrowseSymbols),
            "tools_collection_settings" => Some(Self::ToolsCollectionSettings),
            "tools_persistence" => Some(Self::ToolsPersistence),
            "tools_preferences" => Some(Self::ToolsPreferences),
            "help_getting_started" => Some(Self::HelpGettingStarted),
            "help_shortcuts" => Some(Self::HelpShortcuts),
            "help_about" => Some(Self::HelpAbout),
            s if s.starts_with("project_recent_") => {
                let n = s.strip_prefix("project_recent_")?.parse().ok()?;
                Some(Self::ProjectRecent(n))
            }
            _ => None,
        }
    }
}
