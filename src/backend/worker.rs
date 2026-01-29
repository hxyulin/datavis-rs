//! Backend Worker Thread Implementation
//!
//! This module contains the main worker loop that runs in a separate thread
//! and handles all SWD polling operations. It communicates with the UI thread
//! through crossbeam channels.
//!
//! # Responsibilities
//!
//! The worker thread handles:
//!
//! - **Command processing**: Responds to UI commands (connect, start, stop, etc.)
//! - **Variable polling**: Periodically reads variable values at the configured rate
//! - **Value conversion**: Applies Rhai converter scripts to raw values
//! - **Statistics tracking**: Monitors read success rates and timing
//! - **Error handling**: Gracefully handles probe errors and timeouts
//!
//! # Rate Limiting
//!
//! The worker implements precise rate limiting to achieve the configured
//! polling rate (default 100 Hz). It tracks timing and adjusts sleep
//! durations to maintain consistent sample rates.
//!
//! # Script Execution Context
//!
//! For each variable read, the worker provides execution context to Rhai scripts:
//!
//! - `value` / `raw` - Current raw value
//! - `time()` - Time since collection started
//! - `dt()` - Time since last sample
//! - `prev()` / `prev_raw()` - Previous values for derivative calculations

use crate::backend::converter_engine::ConverterEngine;
use crate::backend::{BackendCommand, BackendMessage, ProbeBackend};
use crate::backend::probe_trait::DebugProbe;
use crate::backend::read_manager::DependentReadPlanner;
use crate::config::AppConfig;
use crate::types::{CollectionStats, ConnectionStatus, Variable};
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(feature = "mock-probe")]
use crate::backend::MockProbeBackend;

/// Commands that can be sent to the SWD worker
#[derive(Debug, Clone)]
pub enum SwdCommand {
    /// Read a single variable
    Read { variable_id: u32 },
    /// Read all enabled variables
    ReadAll,
    /// Write a value to a variable
    Write { variable_id: u32, value: f64 },
    /// Halt the target
    Halt,
    /// Resume the target
    Resume,
    /// Reset the target
    Reset { halt: bool },
}

/// Responses from the SWD worker
#[derive(Debug, Clone)]
pub enum SwdResponse {
    /// Read completed successfully
    ReadComplete {
        variable_id: u32,
        raw_value: f64,
        converted_value: f64,
        timestamp: Duration,
    },
    /// Read failed
    ReadError { variable_id: u32, error: String },
    /// Write completed
    WriteComplete { variable_id: u32 },
    /// Write failed
    WriteError { variable_id: u32, error: String },
    /// Target halted
    Halted,
    /// Target running
    Running,
    /// Reset completed
    ResetComplete,
    /// Error occurred
    Error(String),
}

/// Routes data to global and per-pane streams
///
/// This structure manages pane subscriptions and filters data for each pane.
/// It will be used in Phase 2 when we replace the pipeline with direct routing.
#[derive(Debug, Clone, Default)]
pub struct DataRouter {
    /// Which panes subscribe to which variables (pane_id → var_ids)
    pane_subscriptions: HashMap<u64, std::collections::HashSet<u32>>,
}

impl DataRouter {
    /// Create a new data router
    pub fn new() -> Self {
        Self::default()
    }

    /// Update pane subscriptions (called from UI config changes)
    pub fn subscribe_pane(&mut self, pane_id: u64, var_ids: std::collections::HashSet<u32>) {
        self.pane_subscriptions.insert(pane_id, var_ids);
    }

    /// Remove a pane subscription
    #[allow(dead_code)]
    pub fn unsubscribe_pane(&mut self, pane_id: u64) {
        self.pane_subscriptions.remove(&pane_id);
    }

    /// Get the current subscriptions for a pane
    #[allow(dead_code)]
    pub fn get_pane_subscriptions(&self, pane_id: u64) -> Option<&std::collections::HashSet<u32>> {
        self.pane_subscriptions.get(&pane_id)
    }

    /// Route data to global + per-pane streams
    /// This will be used in Phase 2 when we switch to the new data flow
    #[allow(dead_code)]
    pub fn route(
        &self,
        data: Vec<(u32, Duration, f64, f64)>,
    ) -> (Vec<(u32, Duration, f64, f64)>, HashMap<u64, Vec<(u32, Duration, f64, f64)>>) {
        let global = data.clone(); // All panes see global data

        let mut per_pane = HashMap::new();
        for (pane_id, var_ids) in &self.pane_subscriptions {
            let pane_data: Vec<_> = data
                .iter()
                .filter(|(var_id, ..)| var_ids.contains(var_id))
                .cloned()
                .collect();
            if !pane_data.is_empty() {
                per_pane.insert(*pane_id, pane_data);
            }
        }

        (global, per_pane)
    }
}

/// The backend worker that runs the polling loop
pub struct BackendWorker {
    /// Application configuration
    #[allow(dead_code)]
    config: AppConfig,
    /// Command receiver from the UI
    command_rx: Receiver<BackendCommand>,
    /// Message sender to the UI
    message_tx: Sender<BackendMessage>,
    /// Running flag
    running: Arc<AtomicBool>,
    /// Probe backend for SWD operations (supports both real and mock probes)
    probe: Box<dyn DebugProbe>,
    /// Whether currently using a mock probe (only with mock-probe feature)
    #[cfg(feature = "mock-probe")]
    is_mock_probe: bool,
    /// Converter engine for applying scripts to raw values
    converter_engine: ConverterEngine,
    /// Variables being observed
    variables: HashMap<u32, Variable>,
    /// Current connection status
    connection_status: ConnectionStatus,
    /// Whether data collection is active
    collecting: bool,
    /// Start time for timestamps
    start_time: Instant,
    /// Current polling rate in Hz
    poll_rate_hz: u32,
    /// Statistics
    stats: CollectionStats,
    /// Last poll time for rate limiting
    last_poll_time: Instant,
    /// Last time stats were sent to UI
    last_stats_time: Instant,
    /// Two-stage read planner for pointer dereferencing
    dependent_read_planner: DependentReadPlanner,
    /// Data router for per-pane filtering (Phase 2 - not yet used)
    #[allow(dead_code)]
    data_router: DataRouter,
}

impl BackendWorker {
    /// Create a new backend worker
    pub fn new(
        config: AppConfig,
        command_rx: Receiver<BackendCommand>,
        message_tx: Sender<BackendMessage>,
        running: Arc<AtomicBool>,
    ) -> Self {
        let poll_rate_hz = config.collection.poll_rate_hz;
        let probe: Box<dyn DebugProbe> = Box::new(ProbeBackend::from_app_config(&config));

        Self {
            config,
            command_rx,
            message_tx,
            running,
            probe,
            #[cfg(feature = "mock-probe")]
            is_mock_probe: false,
            converter_engine: ConverterEngine::new(),
            variables: HashMap::new(),
            connection_status: ConnectionStatus::Disconnected,
            collecting: false,
            start_time: Instant::now(),
            poll_rate_hz,
            stats: CollectionStats::default(),
            last_poll_time: Instant::now(),
            last_stats_time: Instant::now(),
            dependent_read_planner: DependentReadPlanner::new(),
            data_router: DataRouter::new(),
        }
    }

    /// Run the main worker loop
    pub fn run(&mut self) {
        tracing::info!("Backend worker started");

        while self.running.load(Ordering::SeqCst) {
            // Process pending commands
            self.process_commands();

            // Perform polling if collecting and connected
            if self.collecting && self.connection_status == ConnectionStatus::Connected {
                self.poll_variables();

                // Send stats periodically (every 500ms)
                if self.last_stats_time.elapsed() >= Duration::from_millis(500) {
                    self.send_stats();
                    self.last_stats_time = Instant::now();
                }
            }

            // Sleep to maintain poll rate
            self.rate_limit();
        }

        // Cleanup
        self.probe.disconnect();

        let _ = self.message_tx.send(BackendMessage::Shutdown);
        tracing::info!("Backend worker stopped");
    }

    /// Process pending commands from the UI
    fn process_commands(&mut self) {
        loop {
            match self.command_rx.try_recv() {
                Ok(cmd) => self.handle_command(cmd),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.running.store(false, Ordering::SeqCst);
                    break;
                }
            }
        }
    }

    /// Handle a single command
    fn handle_command(&mut self, cmd: BackendCommand) {
        match cmd {
            BackendCommand::Connect {
                selector,
                target,
                probe_config,
            } => {
                self.handle_connect(selector, target, probe_config);
            }
            BackendCommand::Disconnect => {
                self.handle_disconnect();
            }
            BackendCommand::StartCollection => {
                self.start_collection();
            }
            BackendCommand::StopCollection => {
                self.stop_collection();
            }
            BackendCommand::AddVariable(var) => {
                self.add_variable(var);
            }
            BackendCommand::RemoveVariable(id) => {
                self.remove_variable(id);
            }
            BackendCommand::UpdateVariable(var) => {
                self.update_variable(var);
            }
            BackendCommand::WriteVariable { id, value } => {
                self.write_variable(id, value);
            }
            BackendCommand::ClearData => {
                self.clear_data();
            }
            BackendCommand::SetPollRate(hz) => {
                self.poll_rate_hz = hz.max(1);
            }
            BackendCommand::SetMemoryAccessMode(mode) => {
                self.probe.set_memory_access_mode(mode);
            }
            BackendCommand::RequestStats => {
                self.send_stats();
            }
            BackendCommand::Shutdown => {
                self.running.store(false, Ordering::SeqCst);
            }
            #[cfg(feature = "mock-probe")]
            BackendCommand::UseMockProbe(use_mock) => {
                // Disconnect current probe if connected
                if self.connection_status != ConnectionStatus::Disconnected {
                    self.probe.disconnect();
                    self.update_connection_status(ConnectionStatus::Disconnected);
                }

                // Swap to the appropriate probe implementation
                if use_mock && !self.is_mock_probe {
                    self.probe = Box::new(MockProbeBackend::new());
                    self.is_mock_probe = true;
                    tracing::info!("Switched to mock probe");
                } else if !use_mock && self.is_mock_probe {
                    self.probe = Box::new(ProbeBackend::from_app_config(&self.config));
                    self.is_mock_probe = false;
                    tracing::info!("Switched to real probe");
                }
            }
            BackendCommand::RefreshProbes => {
                self.refresh_probes();
            }
            BackendCommand::UpdateConverter {
                var_id,
                var_name,
                script,
            } => {
                // Update converter engine
                self.converter_engine.update_converter(var_id, &var_name, script.clone());

                // Also update the variable's converter_script field if it exists
                if let Some(var) = self.variables.get_mut(&var_id) {
                    var.converter_script = script;
                }
            }
            BackendCommand::SubscribePane { pane_id, var_ids } => {
                // Update data router with pane subscriptions
                self.data_router.subscribe_pane(pane_id, var_ids);
            }
        }
    }

    /// Refresh the probe list and send to UI
    fn refresh_probes(&self) {
        let probes = crate::backend::list_all_probes();
        let _ = self
            .message_tx
            .send(crate::backend::BackendMessage::ProbeList(probes));
    }

    /// Check if we are using a mock probe
    #[allow(dead_code)]
    #[inline]
    fn is_using_mock(&self) -> bool {
        #[cfg(feature = "mock-probe")]
        {
            self.is_mock_probe
        }
        #[cfg(not(feature = "mock-probe"))]
        {
            false
        }
    }

    /// Handle connect command
    fn handle_connect(
        &mut self,
        selector: Option<String>,
        target: String,
        _probe_config: crate::config::ProbeConfig,
    ) {
        self.update_connection_status(ConnectionStatus::Connecting);

        // Connect using the trait method (works for both real and mock probes)
        match self.probe.connect(selector.as_deref(), &target) {
            Ok(()) => {
                self.update_connection_status(ConnectionStatus::Connected);
                tracing::info!("Connected to probe (target: {})", target);
            }
            Err(e) => {
                self.update_connection_status(ConnectionStatus::Error);
                let error_msg = format!("Failed to connect: {}", e);
                tracing::error!("{}", error_msg);
                let _ = self
                    .message_tx
                    .send(BackendMessage::ConnectionError(error_msg));
            }
        }
    }

    /// Handle disconnect command
    fn handle_disconnect(&mut self) {
        self.collecting = false;
        self.probe.disconnect();
        self.update_connection_status(ConnectionStatus::Disconnected);
        tracing::info!("Disconnected from probe");
    }

    /// Start data collection
    fn start_collection(&mut self) {
        if self.connection_status == ConnectionStatus::Connected {
            // Resume the core if it was halted (e.g., from halt_on_connect)
            match self.probe.resume() {
                Ok(()) => tracing::info!("Core resumed for data collection"),
                Err(e) => tracing::warn!("Failed to resume core (may already be running): {}", e),
            }

            self.collecting = true;
            self.start_time = Instant::now();
            self.stats = CollectionStats::default();
            tracing::info!("Started data collection");
        }
    }

    /// Stop data collection
    fn stop_collection(&mut self) {
        self.collecting = false;
        tracing::info!("Stopped data collection");
    }

    /// Add a variable to observe
    fn add_variable(&mut self, var: Variable) {
        let id = var.id;

        // Add to converter engine
        self.converter_engine.add_variable(&var);

        self.variables.insert(id, var);
        self.send_variable_list();
    }

    /// Remove a variable
    fn remove_variable(&mut self, id: u32) {
        // Remove from converter engine
        self.converter_engine.remove_variable(id);

        self.variables.remove(&id);
    }

    /// Update a variable's configuration
    fn update_variable(&mut self, var: Variable) {
        let id = var.id;

        // Update converter engine
        self.converter_engine.update_converter(id, &var.name, var.converter_script.clone());

        self.variables.insert(id, var);
        self.send_variable_list();
    }

    /// Write a value to a variable
    fn write_variable(&mut self, id: u32, value: f64) {
        // Find the variable
        let var = match self.variables.get(&id) {
            Some(v) => v.clone(),
            None => {
                tracing::warn!("Write failed: variable {} not found", id);
                let _ = self.message_tx.send(BackendMessage::WriteError {
                    variable_id: id,
                    error: format!("Variable {} not found", id),
                });
                return;
            }
        };

        // Check if connected
        if self.connection_status != ConnectionStatus::Connected {
            tracing::warn!("Write failed: not connected");
            let _ = self.message_tx.send(BackendMessage::WriteError {
                variable_id: id,
                error: "Not connected to probe".to_string(),
            });
            return;
        }

        // Perform the write using the trait method
        match self.probe.write_variable(&var, value) {
            Ok(()) => {
                tracing::info!("Wrote value {} to variable '{}'", value, var.name);
                let _ = self
                    .message_tx
                    .send(BackendMessage::WriteSuccess { variable_id: id });
            }
            Err(e) => {
                tracing::error!("Failed to write to variable '{}': {}", var.name, e);
                let _ = self.message_tx.send(BackendMessage::WriteError {
                    variable_id: id,
                    error: format!("Write failed: {}", e),
                });
            }
        }
    }

    /// Clear collected data
    fn clear_data(&mut self) {
        self.start_time = Instant::now();
        self.stats = CollectionStats::default();
        self.probe.reset_stats();
        // Clear converter engine state
        self.converter_engine.clear_state();
        // Clear pointer read planner cache
        self.dependent_read_planner.clear();
    }

    /// Poll all enabled variables using batched reads for better performance
    /// Supports two-stage pointer dereferencing: read pointers at lower rate,
    /// then read pointed-to data using cached addresses.
    fn poll_variables(&mut self) {
        let timestamp = self.start_time.elapsed();

        // Collect enabled variables
        let enabled_vars: Vec<Variable> = self
            .variables
            .values()
            .filter(|v| v.enabled)
            .cloned()
            .collect();

        if enabled_vars.is_empty() {
            return;
        }

        // Two-stage read planning for pointer support
        let (pointer_vars, mut data_vars) = self.dependent_read_planner.plan_reads(&enabled_vars);

        // Stage 1: Read pointers (if any need updating)
        if !pointer_vars.is_empty() {
            let pointer_results = self.probe.read_variables(&pointer_vars);

            // Update pointer states and cache
            for (var, result) in pointer_vars.iter().zip(pointer_results.iter()) {
                match result {
                    Ok(value) => {
                        // Update pointer state in the main variables map
                        if let Some(var_mut) = self.variables.get_mut(&var.id) {
                            DependentReadPlanner::update_pointer_state(var_mut, *value);
                        }
                    }
                    Err(_) => {
                        // Mark pointer read as failed
                        if let Some(var_mut) = self.variables.get_mut(&var.id) {
                            DependentReadPlanner::mark_pointer_error(var_mut);
                        }
                    }
                }
            }

            // Update planner's timestamp cache
            let updated_vars: Vec<Variable> = pointer_vars.iter()
                .filter_map(|v| self.variables.get(&v.id).cloned())
                .collect();
            self.dependent_read_planner.update_pointer_cache(&updated_vars);
        }

        // Resolve dependent variable addresses using cached pointer values
        let resolved_vars: Vec<Variable> = data_vars.iter()
            .map(|v| {
                // Get latest state from variables map
                self.variables.get(&v.id).cloned().unwrap_or_else(|| v.clone())
            })
            .collect();
        data_vars = self.dependent_read_planner.resolve_addresses(&resolved_vars);

        // Stage 2: Read data variables (with resolved addresses)
        let read_results = self.probe.read_variables(&data_vars);

        // Build probe results vector: (var_id, timestamp, raw_value)
        let mut probe_data = Vec::new();
        for (var, read_result) in data_vars.iter().zip(read_results.into_iter()) {
            match read_result {
                Ok(raw_value) => {
                    self.stats.successful_reads += 1;
                    self.stats.total_bytes_read += var.var_type.size_bytes() as u64;
                    probe_data.push((var.id, timestamp, raw_value));
                }
                Err(e) => {
                    self.stats.failed_reads += 1;
                    self.try_send_message(BackendMessage::ReadError {
                        variable_id: var.id,
                        error: e.to_string(),
                    });
                }
            }
        }

        // Apply converters to all probe data in one call
        // Returns Vec<(var_id, timestamp, raw, converted)>
        let batch = self.converter_engine.apply_converters(&probe_data);

        // Send batch if not empty (using try_send for backpressure)
        if !batch.is_empty() {
            self.try_send_message(BackendMessage::DataBatch(batch));
        }

        // Update stats from probe
        let probe_stats = self.probe.stats();
        self.stats.avg_read_time_us = probe_stats.avg_read_time_us();
        self.stats.successful_reads = probe_stats.successful_reads;
        self.stats.failed_reads = probe_stats.failed_reads;
        self.stats.total_bytes_read = probe_stats.total_bytes_read;

        // Update latency tracking
        self.stats.min_latency_us = probe_stats.recent_min_us();
        self.stats.max_latency_us = probe_stats.recent_max_us();
        self.stats.jitter_us = probe_stats.jitter_us();

        // Update bulk read stats
        self.stats.bulk_reads = probe_stats.bulk_reads_performed;
        self.stats.reads_saved_by_bulk = probe_stats.individual_reads_saved;

        // Calculate rate based on total batch time, not per-variable time
        if self.stats.avg_read_time_us > 0.0 && !data_vars.is_empty() {
            // The avg_read_time_us now represents the entire batch read time
            // So effective rate is simply 1M / batch_time
            self.stats.effective_sample_rate = 1_000_000.0 / self.stats.avg_read_time_us;
        }
    }

    /// Rate limit the polling loop
    fn rate_limit(&mut self) {
        if self.poll_rate_hz == 0 {
            // No rate limiting, just yield
            std::thread::yield_now();
            return;
        }

        let target_interval = Duration::from_micros(1_000_000 / self.poll_rate_hz as u64);
        let elapsed = self.last_poll_time.elapsed();

        if elapsed < target_interval {
            std::thread::sleep(target_interval - elapsed);
        }

        self.last_poll_time = Instant::now();
    }

    /// Update connection status and notify UI
    fn update_connection_status(&mut self, status: ConnectionStatus) {
        self.connection_status = status;
        let _ = self
            .message_tx
            .send(BackendMessage::ConnectionStatus(status));
    }

    /// Send statistics to UI (using try_send for backpressure)
    fn send_stats(&mut self) {
        let mut stats = self.stats.clone();
        stats.memory_access_mode = self.probe.memory_access_mode().to_string();
        self.try_send_message(BackendMessage::Stats(stats));
    }

    /// Send variable list to UI
    fn send_variable_list(&self) {
        let vars: Vec<Variable> = self.variables.values().cloned().collect();
        let _ = self.message_tx.send(BackendMessage::VariableList(vars));
    }

    /// Try to send a message, tracking dropped messages if queue is full
    ///
    /// Uses try_send() to avoid blocking. If the queue is full, the message
    /// is dropped and the dropped_messages counter is incremented.
    fn try_send_message(&mut self, msg: BackendMessage) {
        if self.message_tx.try_send(msg).is_err() {
            self.stats.dropped_messages += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::types::VariableType;
    use crossbeam_channel::bounded;

    fn create_test_worker() -> (
        BackendWorker,
        Receiver<BackendMessage>,
        Sender<BackendCommand>,
    ) {
        let (cmd_tx, cmd_rx) = bounded(16);
        let (msg_tx, msg_rx) = bounded(16);
        let running = Arc::new(AtomicBool::new(true));
        let config = AppConfig::default();

        let worker = BackendWorker::new(config, cmd_rx, msg_tx, running);

        (worker, msg_rx, cmd_tx)
    }

    #[test]
    fn test_worker_creation() {
        let (worker, _, _) = create_test_worker();
        assert!(!worker.collecting);
        assert_eq!(worker.connection_status, ConnectionStatus::Disconnected);
    }

    #[test]
    fn test_add_remove_variable() {
        let (mut worker, _, _) = create_test_worker();

        let var = Variable::new("test", 0x2000_0000, VariableType::U32);
        worker.add_variable(var.clone());

        assert!(worker.variables.contains_key(&var.id));

        worker.remove_variable(var.id);
        assert!(!worker.variables.contains_key(&var.id));
    }

    #[test]
    fn test_poll_rate() {
        let (mut worker, _, _) = create_test_worker();

        assert!(worker.poll_rate_hz > 0);
        worker.poll_rate_hz = 100;
        assert_eq!(worker.poll_rate_hz, 100);
    }

    #[test]
    fn test_collection_control() {
        let (mut worker, msg_rx, _) = create_test_worker();

        // Cannot start collection when disconnected
        worker.start_collection();
        assert!(!worker.collecting);

        // Simulate connected state
        worker.connection_status = ConnectionStatus::Connected;
        worker.start_collection();
        assert!(worker.collecting);

        worker.stop_collection();
        assert!(!worker.collecting);

        // Drain any messages
        while msg_rx.try_recv().is_ok() {}
    }

    #[test]
    fn test_shutdown_command() {
        let (mut worker, _, cmd_tx) = create_test_worker();

        cmd_tx.send(BackendCommand::Shutdown).unwrap();
        worker.process_commands();

        assert!(!worker.running.load(Ordering::SeqCst));
    }

    #[test]
    fn test_should_use_mock() {
        let (worker, _, _) = create_test_worker();

        // When mock-probe feature is not enabled, should always return false
        #[cfg(not(feature = "mock-probe"))]
        assert!(!worker.is_using_mock());

        // When mock-probe feature is enabled, depends on is_mock_probe flag
        #[cfg(feature = "mock-probe")]
        assert!(!worker.is_using_mock()); // defaults to false
    }
}
