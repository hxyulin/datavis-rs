//! # DataVis-RS: SWD-based Data Visualizer
//!
//! A real-time data visualization tool that uses Serial Wire Debug (SWD) to observe
//! variables on embedded devices. The architecture follows the "SWD-Observer Pattern"
//! which separates the SWD polling backend from the UI rendering frontend.
//!
//! ## Architecture
//!
//! - **Backend**: Handles SWD polling via probe-rs in a separate thread
//! - **Frontend**: Renders the UI using eframe/egui with egui_plot for graphs
//! - **Scripting**: Rhai-based variable converters for transforming raw values
//! - **Communication**: Crossbeam channels for thread-safe data transfer
//!
//! ## Configuration
//!
//! Application state (recent projects, preferences) is stored in the platform-appropriate
//! data directory under `dev.hxyulin.datavis-rs`:
//!
//! - **Linux**: `~/.local/share/dev.hxyulin.datavis-rs/`
//! - **macOS**: `~/Library/Application Support/dev.hxyulin.datavis-rs/`
//! - **Windows**: `%APPDATA%\dev.hxyulin.datavis-rs\`
//!
//! ## Example
//!
//! ```ignore
//! use datavis_rs::{
//!     backend::SwdBackend,
//!     config::{AppConfig, AppState, ProjectFile},
//!     frontend::DataVisApp,
//! };
//!
//! fn main() -> eframe::Result<()> {
//!     // Load app state (recent projects, preferences)
//!     let app_state = AppState::load_or_default();
//!
//!     // Load last project or use defaults
//!     let (config, project_path) = if let Some(path) = app_state.get_last_project() {
//!         match ProjectFile::load(path) {
//!             Ok(project) => (project.config, Some(path.to_path_buf())),
//!             Err(_) => (AppConfig::default(), None),
//!         }
//!     } else {
//!         (AppConfig::default(), None)
//!     };
//!
//!     let (backend, frontend_receiver) = SwdBackend::new(config.clone());
//!
//!     std::thread::spawn(move || backend.run());
//!
//!     let native_options = eframe::NativeOptions::default();
//!     eframe::run_native(
//!         "DataVis-RS",
//!         native_options,
//!         Box::new(|cc| {
//!             Ok(Box::new(DataVisApp::new(
//!                 cc,
//!                 frontend_receiver,
//!                 config,
//!                 app_state,
//!                 project_path,
//!             )))
//!         }),
//!     )
//! }
//! ```

pub mod analysis;
pub mod app;
pub mod backend;
pub mod config;
pub mod error;
pub mod frontend;
pub mod scripting;
pub mod session;
pub mod types;

// Re-export commonly used types
pub use app::DataVisApp;
pub use backend::{ProbeBackend, SwdCommand, SwdResponse};
pub use config::{AppConfig, AppState, ProjectFile};
pub use error::{DataVisError, Result};
pub use scripting::{ExecutionContext, ScriptEngine};
pub use types::{DataPoint, Variable, VariableType};
