//! UI session state persistence
//!
//! This module handles automatic persistence of UI state between app launches.
//! Session state includes window position/size, workspace layout, and UI chrome visibility.
//!
//! # Design Philosophy
//!
//! | Aspect | UI Session State | Project Files |
//! |--------|-----------------|---------------|
//! | **Purpose** | "Where I was" | "What I was working on" |
//! | **Persistence** | Automatic (on close) | Explicit (File > Save) |
//! | **Scope** | Window, layout, last project | Variables, probe config, ELF |
//! | **Location** | `app_data_dir()/ui_session.json` | User-chosen `.datavisproj` |
//! | **Portability** | Not portable (local machine) | Portable (share with team) |

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::{app_data_dir, ensure_app_data_dir};
use crate::error::DataVisError;
use crate::types::Variable;

/// UI session state filename
pub const UI_SESSION_FILE: &str = "ui_session.json";

/// UI session state persisted between app launches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSessionState {
    /// Version for migration
    #[serde(default = "default_version")]
    pub version: u32,

    /// Window state
    #[serde(default)]
    pub window: WindowState,

    /// Workspace layout (serialized DockState)
    #[serde(default)]
    pub workspace_layout: Option<SerializedWorkspaceLayout>,

    /// UI visibility toggles
    #[serde(default = "default_true")]
    pub show_toolbar: bool,
    #[serde(default = "default_true")]
    pub show_status_bar: bool,

    /// Last opened project path (for auto-restore)
    #[serde(default)]
    pub last_project_path: Option<PathBuf>,

    /// Runtime connection state (not probe config, just selection)
    #[serde(default)]
    pub selected_probe_index: Option<usize>,
    #[serde(default)]
    pub target_chip_input: String,

    /// Last loaded ELF file path (for quick reload)
    #[serde(default)]
    pub elf_file_path: Option<PathBuf>,

    /// Variables configured in this session (indexed by ID for O(1) lookup)
    /// These are auto-saved so one-off debugging sessions persist variables
    #[serde(default)]
    pub variables: HashMap<u32, Variable>,
}

fn default_version() -> u32 {
    1
}

fn default_true() -> bool {
    true
}

impl Default for UiSessionState {
    fn default() -> Self {
        Self {
            version: 1,
            window: WindowState::default(),
            workspace_layout: None,
            show_toolbar: true,
            show_status_bar: true,
            last_project_path: None,
            selected_probe_index: None,
            target_chip_input: String::new(),
            elf_file_path: None,
            variables: HashMap::new(),
        }
    }
}

impl UiSessionState {
    /// Load UI session state from default location
    pub fn load() -> Self {
        let path = app_data_dir().map(|p| p.join(UI_SESSION_FILE));

        if let Some(path) = path {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    match serde_json::from_str(&content) {
                        Ok(state) => {
                            tracing::info!("Loaded UI session state from {:?}", path);
                            return state;
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse UI session state: {}, using defaults", e);
                        }
                    }
                }
            }
        }
        Self::default()
    }

    /// Save UI session state to default location
    pub fn save(&self) -> Result<(), DataVisError> {
        let dir = ensure_app_data_dir()?;
        let path = dir.join(UI_SESSION_FILE);

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| DataVisError::Config(format!("Failed to serialize UI session: {}", e)))?;

        std::fs::write(&path, content)
            .map_err(|e| DataVisError::Config(format!("Failed to write UI session: {}", e)))?;

        tracing::debug!("Saved UI session state to {:?}", path);
        Ok(())
    }
}

/// Window position and size
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    /// Window position (x, y) - None means let OS decide
    #[serde(default)]
    pub position: Option<(i32, i32)>,
    /// Window size (width, height)
    #[serde(default = "default_window_size")]
    pub size: (u32, u32),
    /// Whether window is maximized
    #[serde(default)]
    pub maximized: bool,
}

fn default_window_size() -> (u32, u32) {
    (1280, 720)
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            position: None,
            size: (1280, 720),
            maximized: false,
        }
    }
}

/// Serialized workspace layout that can be stored as JSON
///
/// This stores both the dock structure and the pane metadata needed to
/// reconstruct the workspace on load.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedWorkspaceLayout {
    /// The serialized dock state JSON
    pub dock_json: String,
    /// Pane metadata for reconstruction
    pub panes: Vec<SerializedPane>,
}

/// Serialized pane metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedPane {
    /// Pane ID (u64)
    pub id: u64,
    /// Pane kind (as string for forward compatibility)
    pub kind: String,
    /// Pane title
    pub title: String,
}
