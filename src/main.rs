//! SWD Data Visualizer - Main Entry Point
//!
//! This application provides real-time data visualization for embedded systems
//! using Serial Wire Debug (SWD) interface.

use datavis_rs::{
    backend::SwdBackend,
    config::{AppConfig, AppState, ProjectFile},
    frontend::DataVisApp,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn main() -> eframe::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,datavis_rs=trace")),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting SWD Data Visualizer");

    // Load application state (recent projects, preferences, etc.)
    let mut app_state = AppState::load_or_default();

    // Clean up any missing recent projects
    app_state.cleanup_missing_projects();

    // Try to load the last project, or use defaults
    let (config, project_path) = if let Some(last_path) = app_state.get_last_project() {
        tracing::info!("Restoring last session from {:?}", last_path);
        match ProjectFile::load(last_path) {
            Ok(project) => (project.config, Some(last_path.to_path_buf())),
            Err(e) => {
                tracing::warn!("Failed to load last project: {}", e);
                (AppConfig::default(), None)
            }
        }
    } else {
        (AppConfig::default(), None)
    };

    // Create the SWD backend with communication channels
    let (backend, frontend_receiver) = SwdBackend::new(config.clone());

    // Spawn the backend thread
    let backend_handle = std::thread::spawn(move || {
        backend.run();
    });

    // Configure eframe options
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("SWD Data Visualizer"),
        ..Default::default()
    };

    // Run the eframe application
    let result = eframe::run_native(
        "SWD Data Visualizer",
        native_options,
        Box::new(|cc| {
            // Configure egui visuals based on user preference
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals.window_shadow.offset = [0, 0];

            if app_state.ui_preferences.dark_mode {
                cc.egui_ctx.set_visuals(egui::Visuals::dark());
            } else {
                cc.egui_ctx.set_visuals(egui::Visuals::light());
            }

            cc.egui_ctx.set_style(style);

            Ok(Box::new(DataVisApp::new(
                cc,
                frontend_receiver,
                config,
                app_state,
                project_path,
            )))
        }),
    );

    // Signal backend to stop and wait for it
    tracing::info!("Shutting down...");
    drop(backend_handle);

    result
}
