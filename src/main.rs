//! DataVis-RS - Main Entry Point
//!
//! This application provides real-time data visualization for embedded systems
//! using Serial Wire Debug (SWD) interface.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use datavis_rs::{
    config::{AppConfig, AppState, ProjectFile, UiSessionState},
    frontend::DataVisApp,
    i18n::set_language,
    menu::{build_menu_bar, MenuBarState},
    pipeline::{PipelineBridge, PipelineBuilder, PipelineNodeIds},
};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
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

    tracing::info!("Starting DataVis-RS");

    // Load application state (recent projects, preferences, etc.)
    let mut app_state = AppState::load_or_default();

    // Load UI session state (window position, workspace layout, etc.)
    let ui_session = UiSessionState::load();
    tracing::debug!("Loaded UI session state: show_toolbar={}, show_status_bar={}",
        ui_session.show_toolbar, ui_session.show_status_bar);

    // Initialize language from saved preferences
    set_language(app_state.ui_preferences.language);

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
    let node_ids = node_ids_rx
        .recv()
        .expect("Failed to receive pipeline node IDs");

    // Prepare menu bar state for building native menu
    let project_name = if let Some(ref path) = project_path {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled Project")
            .to_string()
    } else {
        "Untitled Project".to_string()
    };

    let recent_projects: Vec<_> = app_state
        .recent_projects
        .iter()
        .map(|r| (r.name.clone(), r.path.clone()))
        .collect();

    let menu_state = MenuBarState {
        project_name,
        show_toolbar: ui_session.show_toolbar,
        show_status_bar: ui_session.show_status_bar,
        recent_projects,
    };

    // Build the native menu bar (translations are applied here)
    let menu = build_menu_bar(&menu_state);

    // Configure eframe options using window state from UI session
    let window_size = ui_session.window.size;
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([window_size.0 as f32, window_size.1 as f32])
        .with_min_inner_size([800.0, 600.0])
        .with_title("Data Visualizer")
        .with_icon(egui::IconData::default())
        .with_maximized(ui_session.window.maximized);

    // Restore window position if available
    if let Some((x, y)) = ui_session.window.position {
        viewport = viewport.with_position([x as f32, y as f32]);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    // Run the eframe application
    let result = eframe::run_native(
        "Data Visualizer",
        native_options,
        Box::new(move |cc| {
            // Initialize native menu bar with window handle
            #[cfg(target_os = "macos")]
            {
                menu.init_for_nsapp();
                tracing::info!("Native menu bar initialized for macOS");
            }

            #[cfg(target_os = "windows")]
            {
                use raw_window_handle::HasWindowHandle;
                if let Ok(handle) = cc.window_handle() {
                    use raw_window_handle::RawWindowHandle;
                    if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
                        unsafe {
                            menu.init_for_hwnd(win32_handle.hwnd.get() as isize);
                        }
                        tracing::info!("Native menu bar initialized for Windows");
                    }
                }
            }

            #[cfg(target_os = "linux")]
            {
                // Linux requires GTK, which we don't have direct access to in eframe
                // Fall back to egui menus on Linux
                tracing::info!("Native menus not available on Linux, using egui menus");
            }

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
                Some(menu),
                ui_session,
            )))
        }),
    );

    // Signal backend to stop and wait for it
    tracing::info!("Shutting down...");
    drop(backend_handle);

    result
}
