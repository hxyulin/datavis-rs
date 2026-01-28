//! ProbeSource node — reads variables from the debug probe.
//!
//! Replaces `BackendWorker::poll_variables()` + `ReadManager`.

use crate::backend::probe_trait::DebugProbe;
use crate::backend::ProbeBackend;
use crate::config::{AppConfig, MemoryAccessMode, ProbeConfig};
use crate::pipeline::id::VarId;
use crate::pipeline::node::NodeContext;
use crate::pipeline::packet::PipelineEvent;
use crate::pipeline::port::{PortDescriptor, PortDirection, PortKind};
use crate::types::{CollectionStats, Variable};
use std::collections::HashMap;

#[cfg(feature = "mock-probe")]
use crate::backend::MockProbeBackend;

/// Ports for ProbeSourceNode.
static PORTS: &[PortDescriptor] = &[PortDescriptor {
    name: "out",
    direction: PortDirection::Output,
    kind: PortKind::DataStream,
}];

/// ProbeSource: owns the debug probe, reads variables each tick.
pub struct ProbeSourceNode {
    probe: Box<dyn DebugProbe>,
    #[cfg(feature = "mock-probe")]
    is_mock: bool,
    variables: HashMap<u32, Variable>,
    /// Map from legacy variable ID to VarId in the pipeline tree.
    var_id_map: HashMap<u32, VarId>,
    next_var_id: u32,
    connected: bool,
    #[allow(dead_code)]
    config: AppConfig,
    stats: CollectionStats,
}

impl ProbeSourceNode {
    pub fn new(config: AppConfig) -> Self {
        let probe: Box<dyn DebugProbe> = Box::new(ProbeBackend::from_app_config(&config));
        Self {
            probe,
            #[cfg(feature = "mock-probe")]
            is_mock: false,
            variables: HashMap::new(),
            var_id_map: HashMap::new(),
            next_var_id: 0,
            connected: false,
            config,
            stats: CollectionStats::default(),
        }
    }

    pub fn name(&self) -> &str {
        "ProbeSource"
    }

    pub fn ports(&self) -> &[PortDescriptor] {
        PORTS
    }

    pub fn on_activate(&mut self, _ctx: &mut NodeContext) {
        // Resume core if halted
        if self.connected {
            match self.probe.resume() {
                Ok(()) => tracing::info!("Core resumed for data collection"),
                Err(e) => {
                    tracing::warn!("Failed to resume core (may already be running): {}", e)
                }
            }
        }
        self.stats = CollectionStats::default();
    }

    pub fn on_data(&mut self, ctx: &mut NodeContext) {
        if !self.connected {
            return;
        }

        let timestamp = ctx.timestamp;
        let enabled_vars: Vec<Variable> = self
            .variables
            .values()
            .filter(|v| v.enabled)
            .cloned()
            .collect();

        if enabled_vars.is_empty() {
            return;
        }

        // Batch read
        let results = self.probe.read_variables(&enabled_vars);

        ctx.output.timestamp = timestamp;

        for (var, result) in enabled_vars.iter().zip(results.into_iter()) {
            match result {
                Ok(raw_value) => {
                    self.stats.successful_reads += 1;
                    self.stats.total_bytes_read += var.var_type.size_bytes() as u64;

                    let var_id = self
                        .var_id_map
                        .get(&var.id)
                        .copied()
                        .unwrap_or(VarId(var.id));

                    // Write raw value; conversion is done by ScriptTransform downstream
                    ctx.output.push_value(var_id, raw_value, raw_value);
                }
                Err(e) => {
                    self.stats.failed_reads += 1;
                    ctx.output_events.push(PipelineEvent::VariableError {
                        var_id: self
                            .var_id_map
                            .get(&var.id)
                            .copied()
                            .unwrap_or(VarId(var.id)),
                        message: e.to_string(),
                    });
                }
            }
        }

        // Update stats from probe
        let probe_stats = self.probe.stats();
        self.stats.avg_read_time_us = probe_stats.avg_read_time_us();
        self.stats.min_latency_us = probe_stats.recent_min_us();
        self.stats.max_latency_us = probe_stats.recent_max_us();
        self.stats.jitter_us = probe_stats.jitter_us();
        self.stats.bulk_reads = probe_stats.bulk_reads_performed;
        self.stats.reads_saved_by_bulk = probe_stats.individual_reads_saved;

        if self.stats.avg_read_time_us > 0.0 {
            self.stats.effective_sample_rate = 1_000_000.0 / self.stats.avg_read_time_us;
        }
    }

    pub fn on_deactivate(&mut self, _ctx: &mut NodeContext) {
        // Nothing to do — probe stays connected
    }

    pub fn on_config_change(
        &mut self,
        _key: &str,
        _value: &crate::pipeline::packet::ConfigValue,
        _ctx: &mut NodeContext,
    ) {
    }

    // ── Pipeline-specific API ──

    pub fn connect(
        &mut self,
        selector: Option<&str>,
        target: &str,
        _probe_config: &ProbeConfig,
    ) -> Result<(), String> {
        match self.probe.connect(selector, target) {
            Ok(()) => {
                self.connected = true;
                tracing::info!("ProbeSource connected to target: {}", target);
                Ok(())
            }
            Err(e) => {
                self.connected = false;
                Err(format!("Failed to connect: {}", e))
            }
        }
    }

    pub fn disconnect(&mut self) {
        self.probe.disconnect();
        self.connected = false;
    }

    pub fn add_variable(&mut self, var: &Variable) {
        let var_id = VarId(self.next_var_id);
        self.next_var_id += 1;
        self.var_id_map.insert(var.id, var_id);
        self.variables.insert(var.id, var.clone());
    }

    pub fn remove_variable(&mut self, id: u32) {
        self.variables.remove(&id);
        self.var_id_map.remove(&id);
    }

    pub fn update_variable(&mut self, var: &Variable) {
        self.variables.insert(var.id, var.clone());
    }

    pub fn write_variable(&mut self, id: u32, value: f64) -> Result<(), String> {
        let var = self
            .variables
            .get(&id)
            .ok_or_else(|| format!("Variable {} not found", id))?
            .clone();

        if !self.connected {
            return Err("Not connected to probe".into());
        }

        self.probe
            .write_variable(&var, value)
            .map_err(|e| format!("Write failed: {}", e))
    }

    pub fn set_memory_access_mode(&mut self, mode: MemoryAccessMode) {
        self.probe.set_memory_access_mode(mode);
    }

    #[cfg(feature = "mock-probe")]
    pub fn set_use_mock(&mut self, use_mock: bool) {
        if use_mock && !self.is_mock {
            if self.connected {
                self.probe.disconnect();
                self.connected = false;
            }
            self.probe = Box::new(MockProbeBackend::new());
            self.is_mock = true;
        } else if !use_mock && self.is_mock {
            if self.connected {
                self.probe.disconnect();
                self.connected = false;
            }
            self.probe = Box::new(ProbeBackend::from_app_config(&self.config));
            self.is_mock = false;
        }
    }

    pub fn collection_stats(&self) -> CollectionStats {
        let mut stats = self.stats.clone();
        stats.memory_access_mode = self.probe.memory_access_mode().to_string();
        stats
    }
}
