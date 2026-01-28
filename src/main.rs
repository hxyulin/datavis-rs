//! SWD Data Visualizer - Main Entry Point
//!
//! This application provides real-time data visualization for embedded systems
//! using Serial Wire Debug (SWD) interface.

use datavis_rs::{
    config::{AppConfig, AppState, ProjectFile},
    frontend::DataVisApp,
    pipeline::{PipelineBridge, PipelineBuilder, PipelineNodeIds},
};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
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

    // Create the pipeline bridge and spawn the pipeline thread
    let (bridge, cmd_rx, msg_tx) = PipelineBridge::new();
    let running = Arc::new(AtomicBool::new(true));
    let builder = PipelineBuilder::new(config.clone());
    let running_clone = running.clone();
    let (node_ids_tx, node_ids_rx) = std::sync::mpsc::sync_channel::<PipelineNodeIds>(1);
    let backend_handle = std::thread::spawn(move || {
        let (mut pipeline, node_ids) = builder.build_default(cmd_rx, msg_tx, running_clone);
        let _ = node_ids_tx.send(node_ids);
        pipeline.run();
    });
    let node_ids = node_ids_rx.recv().expect("Failed to receive pipeline node IDs");

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
                bridge,
                config,
                app_state,
                project_path,
                node_ids,
            )))
        }),
    );

    // Signal backend to stop and wait for it
    tracing::info!("Shutting down...");
    drop(backend_handle);

    result
}
