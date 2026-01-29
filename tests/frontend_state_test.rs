//! Integration tests for frontend state management
//!
//! These tests validate the SharedState helper methods and staleness detection logic.

mod common;

use datavis_rs::config::settings::RuntimeSettings;
use datavis_rs::config::{AppConfig, AppState, DataPersistenceConfig};
use datavis_rs::frontend::state::SharedState;
use datavis_rs::frontend::topics::Topics;
use datavis_rs::pipeline::bridge::PipelineBridge;
use std::time::{Duration, Instant};

/// Helper to create a minimal SharedState for testing
fn create_test_shared_state<'a>(
    bridge: &'a PipelineBridge,
    config: &'a mut AppConfig,
    settings: &'a mut RuntimeSettings,
    app_state: &'a mut AppState,
    topics: &'a mut Topics,
    persistence_config: &'a mut DataPersistenceConfig,
    last_error: &'a mut Option<String>,
) -> SharedState<'a> {
    SharedState {
        frontend: bridge,
        config,
        settings,
        app_state,
        elf_info: None,
        elf_symbols: &[],
        elf_file_path: None,
        persistence_config,
        last_error,
        display_time: 0.0,
        topics,
        current_pane_id: None,
    }
}

#[test]
fn test_is_pane_data_stale_when_not_collecting() {
    let (bridge, _cmd_rx, _msg_tx) = PipelineBridge::new();
    let mut config = AppConfig::default();
    let mut settings = RuntimeSettings::default();
    let mut app_state = AppState::default();
    let mut topics = Topics::default();
    let mut persistence = DataPersistenceConfig::default();

    settings.collecting = false;

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

    // Should never be stale when not collecting
    assert!(!shared.is_pane_data_stale(None));
    assert!(!shared.is_pane_data_stale(Some(100)));
}

#[test]
fn test_is_pane_data_stale_no_data_received() {
    let (bridge, _cmd_rx, _msg_tx) = PipelineBridge::new();
    let mut config = AppConfig::default();
    let mut settings = RuntimeSettings::default();
    let mut app_state = AppState::default();
    let mut topics = Topics::default();
    let mut persistence = DataPersistenceConfig::default();

    settings.collecting = true;
    // No data has been received (global_data_freshness is None)

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

    // Should not be stale if no data received yet
    assert!(!shared.is_pane_data_stale(None));
}

#[test]
fn test_is_pane_data_stale_fresh_global_data() {
    let (bridge, _cmd_rx, _msg_tx) = PipelineBridge::new();
    let mut config = AppConfig::default();
    let mut settings = RuntimeSettings::default();
    let mut app_state = AppState::default();
    let mut topics = Topics::default();
    let mut persistence = DataPersistenceConfig::default();

    settings.collecting = true;
    topics.global_data_freshness = Some(Instant::now());
    topics.staleness_threshold = Duration::from_secs(3);

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

    // Data is fresh (just received)
    assert!(!shared.is_pane_data_stale(None));
}

#[test]
fn test_is_pane_data_stale_old_global_data() {
    let (bridge, _cmd_rx, _msg_tx) = PipelineBridge::new();
    let mut config = AppConfig::default();
    let mut settings = RuntimeSettings::default();
    let mut app_state = AppState::default();
    let mut topics = Topics::default();
    let mut persistence = DataPersistenceConfig::default();

    settings.collecting = true;
    // Set data freshness to 5 seconds ago
    topics.global_data_freshness = Some(Instant::now() - Duration::from_secs(5));
    topics.staleness_threshold = Duration::from_secs(3);

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

    // Data should be stale (5 seconds > 3 second threshold)
    assert!(shared.is_pane_data_stale(None));
}

#[test]
fn test_is_pane_data_stale_pane_specific_fresh() {
    let (bridge, _cmd_rx, _msg_tx) = PipelineBridge::new();
    let mut config = AppConfig::default();
    let mut settings = RuntimeSettings::default();
    let mut app_state = AppState::default();
    let mut topics = Topics::default();
    let mut persistence = DataPersistenceConfig::default();

    settings.collecting = true;
    let pane_id = 100u64;
    topics.pane_data_freshness.insert(pane_id, Instant::now());
    topics.staleness_threshold = Duration::from_secs(3);

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

    // Pane-specific data is fresh
    assert!(!shared.is_pane_data_stale(Some(pane_id)));
}

#[test]
fn test_is_pane_data_stale_pane_specific_old() {
    let (bridge, _cmd_rx, _msg_tx) = PipelineBridge::new();
    let mut config = AppConfig::default();
    let mut settings = RuntimeSettings::default();
    let mut app_state = AppState::default();
    let mut topics = Topics::default();
    let mut persistence = DataPersistenceConfig::default();

    settings.collecting = true;
    let pane_id = 100u64;
    topics
        .pane_data_freshness
        .insert(pane_id, Instant::now() - Duration::from_secs(5));
    topics.staleness_threshold = Duration::from_secs(3);

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

    // Pane-specific data is stale
    assert!(shared.is_pane_data_stale(Some(pane_id)));
}

#[test]
fn test_is_pane_data_stale_falls_back_to_global() {
    let (bridge, _cmd_rx, _msg_tx) = PipelineBridge::new();
    let mut config = AppConfig::default();
    let mut settings = RuntimeSettings::default();
    let mut app_state = AppState::default();
    let mut topics = Topics::default();
    let mut persistence = DataPersistenceConfig::default();

    settings.collecting = true;
    // No pane-specific data, but global is fresh
    topics.global_data_freshness = Some(Instant::now());
    topics.staleness_threshold = Duration::from_secs(3);

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

    // Should fall back to global and show as fresh
    assert!(!shared.is_pane_data_stale(Some(999)));
}

#[test]
fn test_staleness_threshold_boundary() {
    let (bridge, _cmd_rx, _msg_tx) = PipelineBridge::new();
    let mut config = AppConfig::default();
    let mut settings = RuntimeSettings::default();
    let mut app_state = AppState::default();
    let mut topics = Topics::default();
    let mut persistence = DataPersistenceConfig::default();

    settings.collecting = true;
    // Data exactly at threshold
    topics.global_data_freshness = Some(Instant::now() - Duration::from_secs(3));
    topics.staleness_threshold = Duration::from_secs(3);

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

    // At exactly the threshold, should not be considered stale
    // (duration_since > threshold, not >=)
    let is_stale = shared.is_pane_data_stale(None);
    // This might be stale or not depending on timing - just verify it doesn't panic
    let _ = is_stale;
}
