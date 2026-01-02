//! Configuration module for DataVis-RS
//!
//! This module handles application configuration including:
//! - Application state persistence (recent projects, last session)
//! - Project files (.datavisproj) for complete project state
//! - Runtime settings during execution
//!
//! # App Data Location
//!
//! Application data is stored in the platform-appropriate location:
//! - **Linux**: `~/.local/share/dev.hxyulin.datavis-rs/`
//! - **macOS**: `~/Library/Application Support/dev.hxyulin.datavis-rs/`
//! - **Windows**: `%APPDATA%\dev.hxyulin.datavis-rs\`
//!
//! # Files
//!
//! - `app_state.json` - Recent projects list and last session info
//! - Project files (`.datavisproj`) - Saved wherever the user chooses
//!
//! # Example
//!
//! ```ignore
//! use datavis_rs::config::{AppState, ProjectFile};
//!
//! // Load or create app state
//! let mut state = AppState::load_or_default();
//!
//! // Open a recent project
//! if let Some(recent) = state.recent_projects.first() {
//!     let project = ProjectFile::load(&recent.path)?;
//! }
//!
//! // Save a project and add to recents
//! project.save("my_project.datavisproj")?;
//! state.add_recent_project("my_project.datavisproj");
//! state.save()?;
//! ```

pub mod settings;

pub use settings::*;

use crate::error::{DataVisError, Result};
use crate::types::{Variable, VariableType};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Application identifier for data directories
pub const APP_ID: &str = "dev.hxyulin.datavis-rs";

/// App state filename
pub const APP_STATE_FILE: &str = "app_state.json";

/// Project file extension
pub const PROJECT_FILE_EXTENSION: &str = "datavisproj";

/// Maximum number of recent projects to remember
pub const MAX_RECENT_PROJECTS: usize = 10;

/// Default polling rate in Hz
pub const DEFAULT_POLL_RATE_HZ: u32 = 100;

/// Default timeout for SWD operations in milliseconds
pub const DEFAULT_TIMEOUT_MS: u64 = 100;

/// Default maximum data persistence file size (2GB)
pub const DEFAULT_MAX_PERSISTENCE_SIZE: u64 = 2 * 1024 * 1024 * 1024;

// ==================== App Data Directory ====================

/// Get the application data directory path
///
/// Creates the directory if it doesn't exist.
pub fn app_data_dir() -> Option<PathBuf> {
    dirs_next::data_dir().map(|p| p.join(APP_ID))
}

/// Ensure the app data directory exists
pub fn ensure_app_data_dir() -> Result<PathBuf> {
    let dir = app_data_dir().ok_or_else(|| {
        DataVisError::Config("Could not determine app data directory".to_string())
    })?;

    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| {
            DataVisError::Config(format!("Failed to create app data directory: {}", e))
        })?;
    }

    Ok(dir)
}

/// Get the path to the app state file
pub fn app_state_path() -> Option<PathBuf> {
    app_data_dir().map(|p| p.join(APP_STATE_FILE))
}

// ==================== Recent Project Entry ====================

/// Information about a recently opened project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProject {
    /// Path to the project file
    pub path: PathBuf,

    /// Project name (from the project file)
    pub name: String,

    /// Last opened timestamp (Unix seconds)
    pub last_opened: u64,

    /// Target chip used in this project
    pub target_chip: Option<String>,
}

impl RecentProject {
    /// Create a new recent project entry
    pub fn new(path: impl Into<PathBuf>, name: impl Into<String>) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            path: path.into(),
            name: name.into(),
            last_opened: now,
            target_chip: None,
        }
    }

    /// Update the last opened timestamp
    pub fn touch(&mut self) {
        self.last_opened = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
    }

    /// Check if the project file still exists
    pub fn exists(&self) -> bool {
        self.path.exists()
    }
}

// ==================== App State ====================

/// Persistent application state
///
/// This stores user preferences and history that persists across sessions,
/// separate from individual project files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    /// Version for future migration support
    #[serde(default = "default_app_state_version")]
    pub version: u32,

    /// Recently opened projects
    #[serde(default)]
    pub recent_projects: Vec<RecentProject>,

    /// Path to the last opened project (for "restore session" functionality)
    #[serde(default)]
    pub last_project_path: Option<PathBuf>,

    /// Last used target chip (for quick connect)
    #[serde(default)]
    pub last_target_chip: Option<String>,

    /// Last used probe selector
    #[serde(default)]
    pub last_probe_selector: Option<String>,

    /// UI preferences that persist across projects
    #[serde(default)]
    pub ui_preferences: UiPreferences,
}

fn default_app_state_version() -> u32 {
    1
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            version: 1,
            recent_projects: Vec::new(),
            last_project_path: None,
            last_target_chip: None,
            last_probe_selector: None,
            ui_preferences: UiPreferences::default(),
        }
    }
}

impl AppState {
    /// Load app state from the default location
    pub fn load() -> Result<Self> {
        let path = app_state_path().ok_or_else(|| {
            DataVisError::Config("Could not determine app state path".to_string())
        })?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .map_err(|e| DataVisError::Config(format!("Failed to read app state: {}", e)))?;

        serde_json::from_str(&content)
            .map_err(|e| DataVisError::Config(format!("Failed to parse app state: {}", e)))
    }

    /// Load app state, returning defaults on any error
    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_else(|e| {
            tracing::warn!("Failed to load app state, using defaults: {}", e);
            Self::default()
        })
    }

    /// Save app state to the default location
    pub fn save(&self) -> Result<()> {
        let dir = ensure_app_data_dir()?;
        let path = dir.join(APP_STATE_FILE);

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| DataVisError::Config(format!("Failed to serialize app state: {}", e)))?;

        std::fs::write(&path, content)
            .map_err(|e| DataVisError::Config(format!("Failed to write app state: {}", e)))
    }

    /// Add or update a recent project
    pub fn add_recent_project(
        &mut self,
        path: impl AsRef<Path>,
        name: &str,
        target_chip: Option<&str>,
    ) {
        let path = path.as_ref().to_path_buf();

        // Remove existing entry for this path
        self.recent_projects.retain(|p| p.path != path);

        // Create new entry at the front
        let mut entry = RecentProject::new(path.clone(), name);
        entry.target_chip = target_chip.map(|s| s.to_string());
        self.recent_projects.insert(0, entry);

        // Trim to max size
        self.recent_projects.truncate(MAX_RECENT_PROJECTS);

        // Update last project
        self.last_project_path = Some(path);
    }

    /// Remove a project from recents (e.g., if file was deleted)
    pub fn remove_recent_project(&mut self, path: impl AsRef<Path>) {
        let path = path.as_ref();
        self.recent_projects.retain(|p| p.path != path);

        if self.last_project_path.as_ref() == Some(&path.to_path_buf()) {
            self.last_project_path = None;
        }
    }

    /// Clean up recent projects that no longer exist
    pub fn cleanup_missing_projects(&mut self) {
        self.recent_projects.retain(|p| p.exists());

        if let Some(ref last) = self.last_project_path {
            if !last.exists() {
                self.last_project_path = None;
            }
        }
    }

    /// Update last used connection info
    pub fn update_last_connection(&mut self, target_chip: &str, probe_selector: Option<&str>) {
        self.last_target_chip = Some(target_chip.to_string());
        self.last_probe_selector = probe_selector.map(|s| s.to_string());
    }

    /// Get the most recent project path if it exists
    pub fn get_last_project(&self) -> Option<&Path> {
        self.last_project_path
            .as_ref()
            .filter(|p| p.exists())
            .map(|p| p.as_path())
    }
}

/// UI preferences that persist across all projects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPreferences {
    /// Enable dark mode
    #[serde(default = "default_true")]
    pub dark_mode: bool,

    /// Font scale factor
    #[serde(default = "default_font_scale")]
    pub font_scale: f32,

    /// Remember window position and size
    #[serde(default = "default_true")]
    pub remember_window_state: bool,

    /// Show welcome screen on startup
    #[serde(default = "default_true")]
    pub show_welcome: bool,
}

fn default_true() -> bool {
    true
}

fn default_font_scale() -> f32 {
    1.0
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            dark_mode: true,
            font_scale: 1.0,
            remember_window_state: true,
            show_welcome: true,
        }
    }
}

// ==================== Project File ====================

/// Project file format for saving complete project state
///
/// Projects contain all the configuration needed to reproduce a debugging session:
/// variables, probe settings, UI layout, and the path to the binary being debugged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFile {
    /// Project file format version for future compatibility
    #[serde(default = "default_project_version")]
    pub version: u32,

    /// Project name
    #[serde(default)]
    pub name: String,

    /// The full application configuration
    #[serde(default)]
    pub config: AppConfig,

    /// Path to the selected ELF/binary file (relative or absolute)
    #[serde(default)]
    pub binary_path: Option<PathBuf>,

    /// Data persistence settings
    #[serde(default)]
    pub persistence: DataPersistenceConfig,
}

fn default_project_version() -> u32 {
    1
}

impl Default for ProjectFile {
    fn default() -> Self {
        Self {
            version: 1,
            name: "Untitled Project".to_string(),
            config: AppConfig::default(),
            binary_path: None,
            persistence: DataPersistenceConfig::default(),
        }
    }
}

impl ProjectFile {
    /// Create a new project file with default settings
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Create a project from an existing config
    pub fn from_config(name: impl Into<String>, config: AppConfig) -> Self {
        Self {
            version: 1,
            name: name.into(),
            config,
            binary_path: None,
            persistence: DataPersistenceConfig::default(),
        }
    }

    /// Load a project file from disk
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).map_err(|e| {
            DataVisError::Config(format!("Failed to read project file {:?}: {}", path, e))
        })?;

        // Parse as JSON (primary format)
        serde_json::from_str(&content).map_err(|e| {
            DataVisError::Config(format!("Failed to parse project file {:?}: {}", path, e))
        })
    }

    /// Load a project file, returning defaults if any error occurs
    pub fn load_or_default(path: impl AsRef<Path>) -> Self {
        Self::load(path).unwrap_or_default()
    }

    /// Save project file to disk as JSON
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                DataVisError::Config(format!("Failed to create project directory: {}", e))
            })?;
        }

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| DataVisError::Config(format!("Failed to serialize project: {}", e)))?;

        std::fs::write(path, content).map_err(|e| {
            DataVisError::Config(format!("Failed to write project file {:?}: {}", path, e))
        })
    }

    /// Set the binary/ELF path
    pub fn with_binary_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.binary_path = Some(path.into());
        self
    }
}

// ==================== Data Persistence Config ====================

/// Configuration for data persistence (saving collected data to file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPersistenceConfig {
    /// Whether data persistence is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Path to the persistence file
    #[serde(default)]
    pub file_path: Option<PathBuf>,

    /// Maximum file size in bytes (default 2GB)
    #[serde(default = "default_max_persistence_size")]
    pub max_file_size: u64,

    /// Whether to include variable name in each record
    #[serde(default = "default_true")]
    pub include_variable_name: bool,

    /// Whether to include variable address in each record
    #[serde(default)]
    pub include_variable_address: bool,

    /// Output format
    #[serde(default)]
    pub format: PersistenceFormat,

    /// Whether to append to existing file or overwrite
    #[serde(default)]
    pub append_mode: bool,
}

fn default_max_persistence_size() -> u64 {
    DEFAULT_MAX_PERSISTENCE_SIZE
}

impl Default for DataPersistenceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            file_path: None,
            max_file_size: DEFAULT_MAX_PERSISTENCE_SIZE,
            include_variable_name: true,
            include_variable_address: false,
            format: PersistenceFormat::Csv,
            append_mode: false,
        }
    }
}

/// Format for persisted data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PersistenceFormat {
    /// CSV format - human readable, easy to import into spreadsheets
    #[default]
    Csv,
    /// JSON Lines format - one JSON object per line
    JsonLines,
    /// Binary format - compact, fast, but not human readable
    Binary,
}

impl std::fmt::Display for PersistenceFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistenceFormat::Csv => write!(f, "CSV"),
            PersistenceFormat::JsonLines => write!(f, "JSON Lines"),
            PersistenceFormat::Binary => write!(f, "Binary"),
        }
    }
}

/// A single persisted data record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedDataRecord {
    /// Timestamp in microseconds since start
    pub timestamp_us: u64,
    /// Variable name (if included)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_name: Option<String>,
    /// Variable address (if included)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_address: Option<u64>,
    /// Variable ID
    pub variable_id: u32,
    /// Raw value
    pub raw_value: f64,
    /// Converted value
    pub converted_value: f64,
}

// ==================== App Config ====================

/// Application configuration stored in project files
///
/// This contains all the settings needed for a debugging session.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// Probe connection configuration
    #[serde(default)]
    pub probe: ProbeConfig,

    /// Variables to observe
    #[serde(default)]
    pub variables: Vec<Variable>,

    /// UI configuration
    #[serde(default)]
    pub ui: UiConfig,

    /// Data collection configuration
    #[serde(default)]
    pub collection: CollectionConfig,
}

impl AppConfig {
    /// Create a new default configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a variable to observe
    pub fn add_variable(&mut self, variable: Variable) {
        self.variables.push(variable);
    }

    /// Remove a variable by ID
    pub fn remove_variable(&mut self, id: u32) -> bool {
        let len_before = self.variables.len();
        self.variables.retain(|v| v.id != id);
        self.variables.len() < len_before
    }

    /// Find a variable by ID
    pub fn find_variable(&self, id: u32) -> Option<&Variable> {
        self.variables.iter().find(|v| v.id == id)
    }

    /// Find a variable by ID (mutable)
    pub fn find_variable_mut(&mut self, id: u32) -> Option<&mut Variable> {
        self.variables.iter_mut().find(|v| v.id == id)
    }

    /// Get enabled variables
    pub fn enabled_variables(&self) -> impl Iterator<Item = &Variable> {
        self.variables.iter().filter(|v| v.enabled)
    }

    /// Create a sample configuration with example variables
    pub fn sample() -> Self {
        let mut config = Self::default();

        // Add some sample variables
        config.add_variable(
            Variable::new("counter", 0x2000_0000, VariableType::U32)
                .with_color([255, 100, 100, 255])
                .with_unit("count"),
        );

        config.add_variable(
            Variable::new("adc_value", 0x2000_0004, VariableType::U16)
                .with_color([100, 255, 100, 255])
                .with_unit("V")
                .with_converter("value * 3.3 / 4096.0"),
        );

        config.add_variable(
            Variable::new("temperature", 0x2000_0008, VariableType::F32)
                .with_color([100, 100, 255, 255])
                .with_unit("°C"),
        );

        config
    }
}

// ==================== Probe Config ====================

/// Debug probe connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeConfig {
    /// Probe selector (e.g., "VID:PID:SERIAL" or index)
    pub probe_selector: Option<String>,

    /// Target chip name (e.g., "STM32F407VGTx")
    pub target_chip: String,

    /// Communication speed in kHz
    pub speed_khz: u32,

    /// Protocol to use
    pub protocol: ProbeProtocol,

    /// Connect under reset method (None = normal attach without reset)
    #[serde(default)]
    pub connect_under_reset: ConnectUnderReset,

    /// Whether to halt the target on connect
    pub halt_on_connect: bool,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            probe_selector: None,
            target_chip: "STM32F407VGTx".to_string(),
            speed_khz: 4000,
            protocol: ProbeProtocol::Swd,
            connect_under_reset: ConnectUnderReset::default(),
            halt_on_connect: false,
        }
    }
}

/// Connect under reset method options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConnectUnderReset {
    /// Normal attach without reset
    #[default]
    None,
    /// Software reset using SYSRESETREQ (most compatible, doesn't require reset pin)
    Software,
    /// Hardware reset using the reset pin (requires NRST pin connected)
    Hardware,
    /// Core reset only (resets the core but not peripherals)
    Core,
}

impl std::fmt::Display for ConnectUnderReset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectUnderReset::None => write!(f, "None"),
            ConnectUnderReset::Software => write!(f, "Software (SYSRESETREQ)"),
            ConnectUnderReset::Hardware => write!(f, "Hardware (NRST pin)"),
            ConnectUnderReset::Core => write!(f, "Core Reset"),
        }
    }
}

/// Probe protocol options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ProbeProtocol {
    /// Serial Wire Debug
    #[default]
    Swd,
    /// JTAG
    Jtag,
}

impl std::fmt::Display for ProbeProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeProtocol::Swd => write!(f, "SWD"),
            ProbeProtocol::Jtag => write!(f, "JTAG"),
        }
    }
}

// ==================== UI Config ====================

/// UI configuration for plotting and display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Default time window for plots in seconds
    pub time_window_seconds: f64,

    /// Show grid on plots
    pub show_grid: bool,

    /// Show legend on plots
    pub show_legend: bool,

    /// Plot line width in pixels
    pub line_width: f32,

    /// Enable plot anti-aliasing
    pub anti_aliasing: bool,

    /// Auto-scale Y axis
    pub auto_scale_y: bool,

    /// Auto-scale X axis (follow latest data)
    pub auto_scale_x: bool,

    /// Maximum time window for plots in seconds
    pub max_time_window: f64,

    /// Show raw values alongside converted values
    pub show_raw_values: bool,

    /// Panel sizes (persisted)
    pub side_panel_width: f32,
    pub bottom_panel_height: f32,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            time_window_seconds: 10.0,
            show_grid: true,
            show_legend: true,
            line_width: 1.5,
            anti_aliasing: true,
            auto_scale_y: true,
            auto_scale_x: true,
            max_time_window: 300.0,
            show_raw_values: false,
            side_panel_width: 250.0,
            bottom_panel_height: 150.0,
        }
    }
}

// ==================== Collection Config ====================

/// Data collection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionConfig {
    /// Global polling rate in Hz
    pub poll_rate_hz: u32,

    /// Timeout for SWD operations in milliseconds
    pub timeout_ms: u64,

    /// Maximum number of data points to keep per variable
    pub max_data_points: usize,

    /// Enable data logging to file
    pub log_to_file: bool,

    /// Log file path (if logging is enabled)
    pub log_file_path: Option<PathBuf>,

    /// Log format
    pub log_format: LogFormat,

    /// Buffer size for channel communication
    pub channel_buffer_size: usize,
}

impl Default for CollectionConfig {
    fn default() -> Self {
        Self {
            poll_rate_hz: DEFAULT_POLL_RATE_HZ,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_data_points: 10_000,
            log_to_file: false,
            log_file_path: None,
            log_format: LogFormat::Csv,
            channel_buffer_size: 1024,
        }
    }
}

/// Log file format options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LogFormat {
    /// CSV format
    #[default]
    Csv,
    /// JSON format
    Json,
    /// Binary format (for efficiency)
    Binary,
}

impl std::fmt::Display for LogFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogFormat::Csv => write!(f, "CSV"),
            LogFormat::Json => write!(f, "JSON"),
            LogFormat::Binary => write!(f, "Binary"),
        }
    }
}

// ==================== Utilities ====================

/// Helper to format bytes as human-readable size
pub fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_default() {
        let state = AppState::default();
        assert!(state.recent_projects.is_empty());
        assert!(state.last_project_path.is_none());
        assert_eq!(state.version, 1);
    }

    #[test]
    fn test_add_recent_project() {
        let mut state = AppState::default();

        state.add_recent_project(
            "/path/to/project1.datavisproj",
            "Project 1",
            Some("STM32F407VGTx"),
        );
        assert_eq!(state.recent_projects.len(), 1);
        assert_eq!(state.recent_projects[0].name, "Project 1");

        state.add_recent_project("/path/to/project2.datavisproj", "Project 2", None);
        assert_eq!(state.recent_projects.len(), 2);
        assert_eq!(state.recent_projects[0].name, "Project 2"); // Most recent first

        // Adding same path again should update, not duplicate
        state.add_recent_project("/path/to/project1.datavisproj", "Project 1 Updated", None);
        assert_eq!(state.recent_projects.len(), 2);
        assert_eq!(state.recent_projects[0].name, "Project 1 Updated");
    }

    #[test]
    fn test_recent_projects_max_limit() {
        let mut state = AppState::default();

        for i in 0..15 {
            state.add_recent_project(
                format!("/path/to/project{}.datavisproj", i),
                &format!("Project {}", i),
                None,
            );
        }

        assert_eq!(state.recent_projects.len(), MAX_RECENT_PROJECTS);
    }

    #[test]
    fn test_app_config_sample() {
        let config = AppConfig::sample();
        assert_eq!(config.variables.len(), 3);
    }

    #[test]
    fn test_add_remove_variable() {
        let mut config = AppConfig::default();
        let var = Variable::new("test", 0x2000_0000, VariableType::U32);
        let id = var.id;

        config.add_variable(var);
        assert_eq!(config.variables.len(), 1);

        assert!(config.find_variable(id).is_some());
        assert!(config.remove_variable(id));
        assert_eq!(config.variables.len(), 0);
    }

    #[test]
    fn test_project_file_serialization() {
        let config = AppConfig::sample();
        let project = ProjectFile::from_config("Test Project", config.clone())
            .with_binary_path("/path/to/firmware.elf");

        let json = serde_json::to_string_pretty(&project).unwrap();
        let parsed: ProjectFile = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.name, "Test Project");
        assert_eq!(parsed.config.variables.len(), config.variables.len());
        assert_eq!(
            parsed.binary_path,
            Some(PathBuf::from("/path/to/firmware.elf"))
        );
    }

    #[test]
    fn test_app_state_serialization() {
        let mut state = AppState::default();
        state.add_recent_project("/test/project.datavisproj", "Test", Some("STM32F407VGTx"));
        state.last_target_chip = Some("STM32F407VGTx".to_string());

        let json = serde_json::to_string_pretty(&state).unwrap();
        let parsed: AppState = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.recent_projects.len(), 1);
        assert_eq!(parsed.last_target_chip, Some("STM32F407VGTx".to_string()));
    }

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(500), "500 bytes");
        assert_eq!(format_file_size(1024), "1.00 KB");
        assert_eq!(format_file_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_file_size(2 * 1024 * 1024 * 1024), "2.00 GB");
    }
}
