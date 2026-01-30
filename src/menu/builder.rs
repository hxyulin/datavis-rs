//! Native menu bar construction using muda

use muda::{
    accelerator::{Accelerator, Code, Modifiers},
    CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu,
};

// t! macro from rust_i18n (translations loaded via i18n! in lib.rs)
use rust_i18n::t;

use super::ids::MenuId;

/// State needed to build the menu bar
pub struct MenuBarState {
    pub project_name: String,
    pub show_toolbar: bool,
    pub show_status_bar: bool,
    pub recent_projects: Vec<(String, std::path::PathBuf)>,
}

impl Default for MenuBarState {
    fn default() -> Self {
        Self {
            project_name: "Untitled Project".to_string(),
            show_toolbar: true,
            show_status_bar: true,
            recent_projects: Vec::new(),
        }
    }
}

/// Build the native menu bar
pub fn build_menu_bar(state: &MenuBarState) -> Menu {
    let menu = Menu::new();

    // === Project menu (named after project) ===
    let project_menu = build_project_menu(state);
    menu.append(&project_menu).unwrap();

    // === File menu ===
    let file_menu = build_file_menu();
    menu.append(&file_menu).unwrap();

    // === View menu ===
    let view_menu = build_view_menu(state);
    menu.append(&view_menu).unwrap();

    // === Tools menu ===
    let tools_menu = build_tools_menu();
    menu.append(&tools_menu).unwrap();

    // === Help menu ===
    let help_menu = build_help_menu();
    menu.append(&help_menu).unwrap();

    menu
}

fn build_project_menu(state: &MenuBarState) -> Submenu {
    let project_menu = Submenu::new(&state.project_name, true);

    // Save
    project_menu
        .append(&MenuItem::with_id(
            MenuId::ProjectSave.to_muda_id(),
            t!("menu_project_save"),
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyS)),
        ))
        .unwrap();

    // Save As
    project_menu
        .append(&MenuItem::with_id(
            MenuId::ProjectSaveAs.to_muda_id(),
            t!("menu_project_save_as"),
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::SHIFT),
                Code::KeyS,
            )),
        ))
        .unwrap();

    project_menu
        .append(&PredefinedMenuItem::separator())
        .unwrap();

    // Recent Projects submenu
    if !state.recent_projects.is_empty() {
        let recent_menu = Submenu::new(t!("menu_project_recent"), true);
        for (i, (name, _path)) in state.recent_projects.iter().enumerate() {
            recent_menu
                .append(&MenuItem::with_id(
                    MenuId::ProjectRecent(i).to_muda_id(),
                    name,
                    true,
                    None::<Accelerator>,
                ))
                .unwrap();
        }
        project_menu.append(&recent_menu).unwrap();
    }

    project_menu
}

fn build_file_menu() -> Submenu {
    let file_menu = Submenu::new(t!("menu_file"), true);

    // New Project
    file_menu
        .append(&MenuItem::with_id(
            MenuId::FileNewProject.to_muda_id(),
            t!("menu_file_new_project"),
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyN)),
        ))
        .unwrap();

    // Open Project
    file_menu
        .append(&MenuItem::with_id(
            MenuId::FileOpenProject.to_muda_id(),
            t!("menu_file_open_project"),
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyO)),
        ))
        .unwrap();

    file_menu.append(&PredefinedMenuItem::separator()).unwrap();

    // Quit (macOS uses standard Cmd+Q)
    #[cfg(not(target_os = "macos"))]
    {
        file_menu.append(&PredefinedMenuItem::separator()).unwrap();
        file_menu
            .append(&MenuItem::with_id(
                MenuId::FileQuit.to_muda_id(),
                t!("menu_file_quit"),
                true,
                Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyQ)),
            ))
            .unwrap();
    }

    file_menu
}

fn build_view_menu(state: &MenuBarState) -> Submenu {
    let view_menu = Submenu::new(t!("menu_view"), true);

    // Chrome toggles
    view_menu
        .append(&CheckMenuItem::with_id(
            MenuId::ViewToolbar.to_muda_id(),
            t!("menu_view_toolbar"),
            true,
            state.show_toolbar,
            None::<Accelerator>,
        ))
        .unwrap();

    view_menu
        .append(&CheckMenuItem::with_id(
            MenuId::ViewStatusBar.to_muda_id(),
            t!("menu_view_status_bar"),
            true,
            state.show_status_bar,
            None::<Accelerator>,
        ))
        .unwrap();

    view_menu.append(&PredefinedMenuItem::separator()).unwrap();

    // Singleton panes
    view_menu
        .append(&MenuItem::with_id(
            MenuId::ViewVariableBrowser.to_muda_id(),
            t!("menu_view_variable_browser"),
            true,
            None::<Accelerator>,
        ))
        .unwrap();

    view_menu
        .append(&MenuItem::with_id(
            MenuId::ViewVariableList.to_muda_id(),
            t!("menu_view_variable_list"),
            true,
            None::<Accelerator>,
        ))
        .unwrap();

    view_menu
        .append(&MenuItem::with_id(
            MenuId::ViewSessionCapture.to_muda_id(),
            t!("menu_view_session_capture"),
            true,
            None::<Accelerator>,
        ))
        .unwrap();

    view_menu.append(&PredefinedMenuItem::separator()).unwrap();

    // Multi-instance visualizers
    view_menu
        .append(&MenuItem::with_id(
            MenuId::ViewNewTimeSeries.to_muda_id(),
            t!("menu_view_new_time_series"),
            true,
            None::<Accelerator>,
        ))
        .unwrap();

    view_menu
        .append(&MenuItem::with_id(
            MenuId::ViewNewWatcher.to_muda_id(),
            t!("menu_view_new_watcher"),
            true,
            None::<Accelerator>,
        ))
        .unwrap();

    view_menu
        .append(&MenuItem::with_id(
            MenuId::ViewNewFft.to_muda_id(),
            t!("menu_view_new_fft"),
            true,
            None::<Accelerator>,
        ))
        .unwrap();

    view_menu.append(&PredefinedMenuItem::separator()).unwrap();

    // Reset Layout
    view_menu
        .append(&MenuItem::with_id(
            MenuId::ViewResetLayout.to_muda_id(),
            t!("menu_view_reset_layout"),
            true,
            None::<Accelerator>,
        ))
        .unwrap();

    view_menu
}

fn build_tools_menu() -> Submenu {
    let tools_menu = Submenu::new(t!("menu_tools"), true);

    // Connection Settings
    tools_menu
        .append(&MenuItem::with_id(
            MenuId::ToolsConnectionSettings.to_muda_id(),
            t!("menu_tools_connection_settings"),
            true,
            None::<Accelerator>,
        ))
        .unwrap();

    tools_menu.append(&PredefinedMenuItem::separator()).unwrap();

    // Load ELF
    tools_menu
        .append(&MenuItem::with_id(
            MenuId::ToolsLoadElf.to_muda_id(),
            t!("menu_tools_load_elf"),
            true,
            None::<Accelerator>,
        ))
        .unwrap();

    // Browse ELF Symbols
    tools_menu
        .append(&MenuItem::with_id(
            MenuId::ToolsBrowseSymbols.to_muda_id(),
            t!("menu_tools_browse_symbols"),
            true,
            None::<Accelerator>,
        ))
        .unwrap();

    tools_menu.append(&PredefinedMenuItem::separator()).unwrap();

    // Collection Settings
    tools_menu
        .append(&MenuItem::with_id(
            MenuId::ToolsCollectionSettings.to_muda_id(),
            t!("menu_tools_collection_settings"),
            true,
            None::<Accelerator>,
        ))
        .unwrap();

    // Data Persistence
    tools_menu
        .append(&MenuItem::with_id(
            MenuId::ToolsPersistence.to_muda_id(),
            t!("menu_tools_persistence"),
            true,
            None::<Accelerator>,
        ))
        .unwrap();

    tools_menu.append(&PredefinedMenuItem::separator()).unwrap();

    // Preferences
    tools_menu
        .append(&MenuItem::with_id(
            MenuId::ToolsPreferences.to_muda_id(),
            t!("menu_tools_preferences"),
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::Comma)),
        ))
        .unwrap();

    tools_menu
}

fn build_help_menu() -> Submenu {
    let help_menu = Submenu::new(t!("menu_help"), true);

    // Getting Started
    help_menu
        .append(&MenuItem::with_id(
            MenuId::HelpGettingStarted.to_muda_id(),
            t!("menu_help_getting_started"),
            true,
            None::<Accelerator>,
        ))
        .unwrap();

    help_menu.append(&PredefinedMenuItem::separator()).unwrap();

    // Keyboard Shortcuts
    help_menu
        .append(&MenuItem::with_id(
            MenuId::HelpShortcuts.to_muda_id(),
            t!("menu_help_shortcuts"),
            true,
            None::<Accelerator>,
        ))
        .unwrap();

    help_menu.append(&PredefinedMenuItem::separator()).unwrap();

    // About
    help_menu
        .append(&MenuItem::with_id(
            MenuId::HelpAbout.to_muda_id(),
            t!("menu_help_about"),
            true,
            None::<Accelerator>,
        ))
        .unwrap();

    help_menu
}
