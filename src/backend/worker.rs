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

use crate::backend::{BackendCommand, BackendMessage, ProbeBackend};
use crate::backend::probe_trait::DebugProbe;
use crate::config::AppConfig;
use crate::scripting::{ExecutionContext, ScriptEngine};
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
    /// Script engine for value conversion
    script_engine: ScriptEngine,
    /// Variables being observed
    variables: HashMap<u32, Variable>,
    /// Compiled converter cache (variable_id -> compiled script)
    converters: HashMap<u32, Option<crate::scripting::CompiledConverter>>,
    /// Previous values for each variable (variable_id -> (prev_raw, prev_converted, prev_timestamp_secs))
    prev_values: HashMap<u32, (f64, f64, f64)>,
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
            script_engine: ScriptEngine::new(),
            variables: HashMap::new(),
            converters: HashMap::new(),
            prev_values: HashMap::new(),
            connection_status: ConnectionStatus::Disconnected,
            collecting: false,
            start_time: Instant::now(),
            poll_rate_hz,
            stats: CollectionStats::default(),
            last_poll_time: Instant::now(),
            last_stats_time: Instant::now(),
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

        // Compile converter if present
        let converter = if let Some(ref script) = var.converter_script {
            match self.script_engine.compile(&var.name, script) {
                Ok(c) => Some(c),
                Err(e) => {
                    tracing::warn!(
                        "Failed to compile converter for variable '{}': {}",
                        var.name,
                        e
                    );
                    None
                }
            }
        } else {
            None
        };

        self.converters.insert(id, converter);
        self.variables.insert(id, var);
        self.send_variable_list();
    }

    /// Remove a variable
    fn remove_variable(&mut self, id: u32) {
        self.variables.remove(&id);
        self.converters.remove(&id);
        self.prev_values.remove(&id);
    }

    /// Update a variable's configuration
    fn update_variable(&mut self, var: Variable) {
        let id = var.id;

        // Recompile converter if changed
        let converter = if let Some(ref script) = var.converter_script {
            match self.script_engine.compile(&var.name, script) {
                Ok(c) => Some(c),
                Err(e) => {
                    tracing::warn!(
                        "Failed to compile converter for variable '{}': {}",
                        var.name,
                        e
                    );
                    None
                }
            }
        } else {
            None
        };

        self.converters.insert(id, converter);
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
        // Clear previous values when clearing data
        self.prev_values.clear();
    }

    /// Poll all enabled variables using batched reads for better performance
    fn poll_variables(&mut self) {
        let timestamp = self.start_time.elapsed();
        let time_secs = timestamp.as_secs_f64();
        let mut batch = Vec::new();

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

        // Use batched read for better performance (single core acquisition)
        // The trait provides read_variables which probes can optimize
        let read_results = self.probe.read_variables(&enabled_vars);

        // Process results
        for (var, read_result) in enabled_vars.iter().zip(read_results.into_iter()) {
            match read_result {
                Ok(raw_value) => {
                    self.stats.successful_reads += 1;
                    self.stats.total_bytes_read += var.var_type.size_bytes() as u64;

                    // Build execution context with previous values
                    let exec_ctx = if let Some(&(prev_raw, prev_converted, prev_time)) =
                        self.prev_values.get(&var.id)
                    {
                        let dt = time_secs - prev_time;
                        ExecutionContext::new(time_secs, dt, prev_raw, prev_converted)
                    } else {
                        ExecutionContext::first_sample(time_secs)
                    };

                    // Apply converter if available
                    let converted_value =
                        if let Some(Some(converter)) = self.converters.get(&var.id) {
                            match self.script_engine.execute(converter, raw_value, exec_ctx) {
                                Ok(v) => v,
                                Err(e) => {
                                    tracing::trace!(
                                        "Converter error for '{}': {}, using raw value",
                                        var.name,
                                        e
                                    );
                                    raw_value
                                }
                            }
                        } else {
                            raw_value
                        };

                    // Store current values as previous for next iteration
                    self.prev_values
                        .insert(var.id, (raw_value, converted_value, time_secs));

                    batch.push((var.id, timestamp, raw_value, converted_value));
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
        if self.stats.avg_read_time_us > 0.0 && !enabled_vars.is_empty() {
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
