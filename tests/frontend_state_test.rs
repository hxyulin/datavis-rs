//! Integration tests for frontend state management
//!
//! Tests that covered is_pane_data_stale / staleness detection were removed
//! in stage 3 of the architecture redesign when global_data_freshness,
//! pane_data_freshness, and is_pane_data_stale were deleted.

mod common;

use datavis_rs::backend::FrontendReceiver;
use datavis_rs::config::settings::RuntimeSettings;
use datavis_rs::config::{AppConfig, AppState, DataPersistenceConfig};
use datavis_rs::frontend::state::{SharedContext, SharedMut, SharedState};
use datavis_rs::frontend::topics::Topics;

/// Helper to create a minimal SharedState for testing
fn create_test_shared_state<'a>(
    bridge: &'a FrontendReceiver,
    config: &'a mut AppConfig,
    settings: &'a mut RuntimeSettings,
    app_state: &'a mut AppState,
    topics: &'a mut Topics,
    persistence_config: &'a mut DataPersistenceConfig,
    last_error: &'a mut Option<String>,
) -> SharedState<'a> {
    SharedState {
        ctx: SharedContext {
            frontend: bridge,
            elf_info: None,
            elf_symbols: &[],
            elf_file_path: None,
            display_time: 0.0,
            current_pane_id: None,
        },
        state: SharedMut {
            config,
            settings,
            app_state,
            persistence_config,
            last_error,
            topics,
        },
    }
}

#[test]
fn test_shared_state_construction() {
    let (_backend, bridge) = datavis_rs::backend::SwdBackend::new(AppConfig::default());
    let mut config = AppConfig::default();
    let mut settings = RuntimeSettings::default();
    let mut app_state = AppState::default();
    let mut topics = Topics::default();
    let mut persistence = DataPersistenceConfig::default();
    let mut last_error = None;

    let shared = create_test_shared_state(
        &bridge,
        &mut config,
        &mut settings,
        &mut app_state,
        &mut topics,
        &mut persistence,
        &mut last_error,
    );

    // Verify the shared state is accessible
    assert!(shared.state.config.variables.is_empty());
    assert!(!shared.state.settings.collecting);
}
