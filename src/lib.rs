//! # DataVis-RS: Real-time embedded variable visualizer
//!
//! A real-time data visualization tool that uses Serial Wire Debug (SWD) to observe
//! variables on embedded devices.
//!
//! ## Two-layer architecture
//!
//! ```text
//! ┌──────────────────────────────────┐
//! │  Frontend  (eframe/egui + egui_dock)  │
//! │  DataVisApp — four pane kinds:         │
//! │    TimeSeries  LiveWatch               │
//! │    VariableList  Recorder              │
//! └───────────────┬──────────────────┘
//!                 │ BackendCommand / BackendMessage
//!                 │ (crossbeam-channel)
//! ┌───────────────▼──────────────────┐
//! │  BackendWorker  (single thread)        │
//! │  probe-rs / OpenOCD / mock probe       │
//! │  + watch scheduler                     │
//! └──────────────────────────────────┘
//! ```
//!
//! There is no middle "pipeline" or routing layer. The frontend talks directly to the
//! backend worker via [`backend::BackendCommand`] / [`backend::BackendMessage`] carried
//! over a pair of `crossbeam-channel` channels bundled in [`backend::FrontendReceiver`].
//!
//! ## Two coexisting subsystems
//!
//! - **Plot** — Variables polled at a configurable rate, optionally transformed by a Rhai
//!   converter script, shown in multi-instance [`frontend::panes::TimeSeriesState`] panes.
//!   Session capture and CSV export live in the [`frontend::panes::RecorderPaneState`] pane.
//! - **Live Watch** — Keil-style on-demand DWARF variable tree; no transformation, no
//!   recording. The [`frontend::panes::LiveWatchState`] pane polls leaves through the
//!   watch scheduler running in the same backend thread.
//!
//! Pane presence in the workspace drives subsystem on/off (lazy init):
//! opening a `TimeSeries` pane auto-starts collection; closing the last one stops it.
//! Same symmetry applies to `LiveWatch` and the watch poll rate.
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
//!     // Backend and frontend communicate via FrontendReceiver — no bridge layer.
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
//!                 None,
//!                 Default::default(),
//!             )))
//!         }),
//!     )
//! }
//! ```

// Initialize i18n at the crate root
rust_i18n::i18n!("locales", fallback = "en");

pub mod app;
pub mod backend;
pub mod config;
pub mod error;
pub mod frontend;
pub mod i18n;
pub mod menu;
pub mod scripting;
pub mod session;
pub mod types;
pub mod watch;

// Re-export commonly used types
pub use app::DataVisApp;
pub use backend::{ProbeBackend, SwdCommand, SwdResponse};
pub use config::{AppConfig, AppState, ProjectFile};
pub use error::{DataVisError, Result};
pub use scripting::{ExecutionContext, ScriptEngine};
pub use types::{DataPoint, Variable, VariableType};
