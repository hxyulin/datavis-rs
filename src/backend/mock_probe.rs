//! Mock Probe Implementation for Testing
//!
//! This module provides a mock debug probe that can be used for testing
//! the application without real hardware. It simulates memory reads with
//! configurable patterns.
//!
//! # Features
//!
//! - **Pattern-based data generation**: Generate test data using various patterns
//! - **Simulated memory**: Read and write to virtual memory regions
//! - **Configurable per-variable**: Each variable can have its own data pattern
//! - **Noise simulation**: Add configurable noise to generated values
//!
//! # Data Patterns
//!
//! The mock probe supports several data generation patterns:
//!
//! - [`MockDataPattern::Constant`] - Fixed value (useful for testing static displays)
//! - [`MockDataPattern::Sine`] - Sinusoidal wave with configurable frequency/amplitude
//! - [`MockDataPattern::Counter`] - Incrementing counter with wrap-around
//! - [`MockDataPattern::Random`] - Random values within a range
//! - [`MockDataPattern::Sawtooth`] - Linear ramp that resets periodically
//! - [`MockDataPattern::Square`] - Square wave alternating between two values
//! - [`MockDataPattern::Triangle`] - Triangle wave
//!
//! # Example
//!
//! ```ignore
//! use datavis_rs::backend::mock_probe::{MockProbeBackend, MockDataPattern, MockVariableConfig};
//!
//! let mut probe = MockProbeBackend::new()
//!     .with_default_pattern(MockDataPattern::Sine {
//!         frequency: 1.0,
//!         amplitude: 100.0,
//!         offset: 50.0,
//!     });
//!
//! probe.connect("MockTarget")?;
//!
//! // Configure a specific variable with a different pattern
//! probe.configure_variable(MockVariableConfig::new(1, MockDataPattern::Counter {
//!     step: 1.0,
//!     min: 0.0,
//!     max: 100.0,
//! }));
//!
//! // Read values
//! let value = probe.read_variable(&my_variable)?;
//! ```
//!
//! # Enabling
//!
//! The mock probe is only available when the `mock-probe` feature is enabled:
//!
//! ```bash
//! cargo run --features mock-probe
//! ```

use crate::error::{DataVisError, Result};
use crate::types::{Variable, VariableType};
use std::collections::HashMap;
use std::time::Instant;

use super::mock_fault::*;
use super::probe_trait::{DebugProbe, ProbeStats};

/// Pattern for generating mock data
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MockDataPattern {
    /// Constant value
    Constant(f64),
    /// Sine wave with frequency and amplitude
    Sine {
        frequency: f64,
        amplitude: f64,
        offset: f64,
    },
    /// Counter that increments
    Counter { step: f64, min: f64, max: f64 },
    /// Random values within range
    Random { min: f64, max: f64 },
    /// Sawtooth wave
    Sawtooth { period: f64, amplitude: f64 },
    /// Square wave
    Square { period: f64, amplitude: f64 },
    /// Triangle wave
    Triangle { period: f64, amplitude: f64 },
}

impl Default for MockDataPattern {
    fn default() -> Self {
        MockDataPattern::Sine {
            frequency: 1.0,
            amplitude: 100.0,
            offset: 0.0,
        }
    }
}

/// Configuration for a mock variable
#[derive(Debug, Clone)]
pub struct MockVariableConfig {
    /// Variable ID
    pub variable_id: u32,
    /// Data generation pattern
    pub pattern: MockDataPattern,
    /// Current counter value (for Counter pattern)
    counter_value: f64,
    /// Noise amplitude to add (0.0 = no noise)
    pub noise_amplitude: f64,
}

impl MockVariableConfig {
    /// Create a new mock variable config
    pub fn new(variable_id: u32, pattern: MockDataPattern) -> Self {
        Self {
            variable_id,
            pattern,
            counter_value: 0.0,
            noise_amplitude: 0.0,
        }
    }

    /// Add noise to the generated values
    pub fn with_noise(mut self, amplitude: f64) -> Self {
        self.noise_amplitude = amplitude;
        self
    }

    /// Generate a value based on the pattern and elapsed time
    pub fn generate_value(&mut self, elapsed_secs: f64) -> f64 {
        let base_value = match self.pattern {
            MockDataPattern::Constant(v) => v,
            MockDataPattern::Sine {
                frequency,
                amplitude,
                offset,
            } => offset + amplitude * (2.0 * std::f64::consts::PI * frequency * elapsed_secs).sin(),
            MockDataPattern::Counter { step, min, max } => {
                self.counter_value += step;
                if self.counter_value > max {
                    self.counter_value = min;
                } else if self.counter_value < min {
                    self.counter_value = max;
                }
                self.counter_value
            }
            MockDataPattern::Random { min, max } => min + rand_simple() * (max - min),
            MockDataPattern::Sawtooth { period, amplitude } => {
                let t = elapsed_secs % period;
                amplitude * (t / period)
            }
            MockDataPattern::Square { period, amplitude } => {
                let t = elapsed_secs % period;
                if t < period / 2.0 {
                    amplitude
                } else {
                    -amplitude
                }
            }
            MockDataPattern::Triangle { period, amplitude } => {
                let t = elapsed_secs % period;
                let half = period / 2.0;
                if t < half {
                    amplitude * (2.0 * t / half - 1.0)
                } else {
                    amplitude * (1.0 - 2.0 * (t - half) / half)
                }
            }
        };

        // Add noise if configured
        if self.noise_amplitude > 0.0 {
            base_value + (rand_simple() - 0.5) * 2.0 * self.noise_amplitude
        } else {
            base_value
        }
    }
}

/// Simple pseudo-random number generator (no external dependency)
fn rand_simple() -> f64 {
    use std::cell::Cell;
    thread_local! {
        static SEED: Cell<u64> = const { Cell::new(12345) };
    }
    SEED.with(|seed| {
        let mut s = seed.get();
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        seed.set(s);
        (s as f64) / (u64::MAX as f64)
    })
}

/// Generate a normally distributed random number using Box-Muller transform
fn rand_normal(mean: f64, stddev: f64) -> f64 {
    let u1 = rand_simple().max(1e-10); // avoid log(0)
    let u2 = rand_simple();
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    mean + stddev * z
}

/// Mock memory that can be read/written
#[derive(Debug, Default)]
pub struct MockMemory {
    /// Memory regions mapped by base address
    regions: HashMap<u64, Vec<u8>>,
}

impl MockMemory {
    /// Create a new empty mock memory
    pub fn new() -> Self {
        Self {
            regions: HashMap::new(),
        }
    }

    /// Add a memory region
    pub fn add_region(&mut self, base_address: u64, size: usize) {
        self.regions.insert(base_address, vec![0u8; size]);
    }

    /// Write data to memory
    pub fn write(&mut self, address: u64, data: &[u8]) -> bool {
        // Find region containing this address
        for (&base, region) in &mut self.regions {
            let end = base + region.len() as u64;
            if address >= base && address + data.len() as u64 <= end {
                let offset = (address - base) as usize;
                region[offset..offset + data.len()].copy_from_slice(data);
                return true;
            }
        }
        false
    }

    /// Read data from memory
    pub fn read(&self, address: u64, size: usize) -> Option<Vec<u8>> {
        for (&base, region) in &self.regions {
            let end = base + region.len() as u64;
            if address >= base && address + size as u64 <= end {
                let offset = (address - base) as usize;
                return Some(region[offset..offset + size].to_vec());
            }
        }
        None
    }

    /// Write a value to memory at the given address
    pub fn write_value<T: ToBytes>(&mut self, address: u64, value: T) -> bool {
        self.write(address, &value.to_le_bytes_vec())
    }
}

/// Trait for converting values to bytes
pub trait ToBytes {
    fn to_le_bytes_vec(&self) -> Vec<u8>;
}

impl ToBytes for u8 {
    fn to_le_bytes_vec(&self) -> Vec<u8> {
        vec![*self]
    }
}
impl ToBytes for u16 {
    fn to_le_bytes_vec(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}
impl ToBytes for u32 {
    fn to_le_bytes_vec(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}
impl ToBytes for u64 {
    fn to_le_bytes_vec(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}
impl ToBytes for i8 {
    fn to_le_bytes_vec(&self) -> Vec<u8> {
        vec![*self as u8]
    }
}
impl ToBytes for i16 {
    fn to_le_bytes_vec(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}
impl ToBytes for i32 {
    fn to_le_bytes_vec(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}
impl ToBytes for i64 {
    fn to_le_bytes_vec(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}
impl ToBytes for f32 {
    fn to_le_bytes_vec(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}
impl ToBytes for f64 {
    fn to_le_bytes_vec(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}

/// Mock probe backend for testing without real hardware
pub struct MockProbeBackend {
    /// Whether the mock probe is "connected"
    connected: bool,
    /// Start time for pattern generation
    start_time: Instant,
    /// Mock variable configurations
    variable_configs: HashMap<u32, MockVariableConfig>,
    /// Default pattern for unconfigured variables
    default_pattern: MockDataPattern,
    /// Mock memory for direct reads
    memory: MockMemory,
    /// Target name (simulated)
    target_name: String,
    /// Simulated read delay in microseconds
    read_delay_us: u64,
    /// If true, always generate patterns instead of reading from memory
    /// This is useful for testing when you want interesting data
    pattern_only_mode: bool,
    /// Probe statistics
    stats: ProbeStats,
    /// Simulated halt state
    halted: bool,
    /// Fault injection configuration
    fault_config: Option<FaultConfig>,
    /// Total number of reads performed (for periodic faults)
    read_counter: u64,
    /// Accumulated latency increase for degrading profiles
    accumulated_latency_increase: f64,
    /// Number of reads in the current second (for rate limiting)
    reads_this_second: u32,
    /// Start of the current rate-limit second
    second_start: Instant,
}

impl MockProbeBackend {
    /// Create a new mock probe backend
    pub fn new() -> Self {
        let mut memory = MockMemory::new();
        // Add a default RAM region (typical STM32 SRAM)
        memory.add_region(0x2000_0000, 128 * 1024); // 128KB at 0x20000000

        Self {
            connected: false,
            start_time: Instant::now(),
            variable_configs: HashMap::new(),
            default_pattern: MockDataPattern::default(),
            memory,
            target_name: "MockTarget".to_string(),
            read_delay_us: 100,      // Simulate 100us read time
            pattern_only_mode: true, // Default to pattern mode for interesting data
            stats: ProbeStats::default(),
            halted: false,
            fault_config: None,
            read_counter: 0,
            accumulated_latency_increase: 0.0,
            reads_this_second: 0,
            second_start: Instant::now(),
        }
    }

    /// Create a mock probe with a specific target name
    pub fn with_target(mut self, name: impl Into<String>) -> Self {
        self.target_name = name.into();
        self
    }

    /// Set the simulated read delay
    pub fn with_read_delay(mut self, delay_us: u64) -> Self {
        self.read_delay_us = delay_us;
        self
    }

    /// Set the default pattern for unconfigured variables
    pub fn with_default_pattern(mut self, pattern: MockDataPattern) -> Self {
        self.default_pattern = pattern;
        self
    }

    /// Set pattern-only mode (if true, always generate patterns instead of reading memory)
    pub fn with_pattern_only_mode(mut self, enabled: bool) -> Self {
        self.pattern_only_mode = enabled;
        self
    }

    /// Set a fault injection configuration
    pub fn with_fault_config(mut self, config: FaultConfig) -> Self {
        self.fault_config = Some(config);
        self
    }

    /// Set a global read failure rate
    pub fn with_read_failure_rate(mut self, rate: f64) -> Self {
        let config = self.fault_config.get_or_insert_with(FaultConfig::default);
        config.global.read_failure_rate = rate;
        self
    }

    /// Set a latency profile
    pub fn with_latency_profile(mut self, profile: LatencyProfile) -> Self {
        let config = self.fault_config.get_or_insert_with(FaultConfig::default);
        config.global.latency_profile = profile;
        self
    }

    /// Add a per-variable fault configuration
    pub fn with_variable_fault(mut self, var_id: u32, fault: VariableFaults) -> Self {
        let config = self.fault_config.get_or_insert_with(FaultConfig::default);
        config.variable_faults.insert(var_id, fault);
        self
    }

    /// Check fault injection rules and possibly return an error
    fn check_fault(&mut self, variable: &Variable) -> Result<()> {
        let fault_config = match &self.fault_config {
            Some(c) => c.clone(),
            None => return Ok(()),
        };

        self.read_counter += 1;

        // Check periodic failures
        if let Some(ref periodic) = fault_config.global.periodic_failure {
            if periodic.every_n_reads > 0 {
                let cycle_pos = self.read_counter % periodic.every_n_reads;
                if cycle_pos > 0 && cycle_pos <= periodic.failure_count {
                    return Err(self.fault_error_to_datavis(&periodic.error_kind, variable));
                }
            }
        }

        // Check per-variable faults
        if let Some(var_fault) = fault_config.variable_faults.get(&variable.id) {
            if var_fault.always_timeout {
                return Err(DataVisError::Timeout(format!(
                    "Simulated timeout for variable {}",
                    variable.id
                )));
            }
            if let Some(rate) = var_fault.read_failure_rate {
                if rand_simple() < rate {
                    return Err(DataVisError::Config(format!(
                        "Simulated per-variable read failure for variable {}",
                        variable.id
                    )));
                }
            }
        }

        // Check global read failure rate
        if fault_config.global.read_failure_rate > 0.0
            && rand_simple() < fault_config.global.read_failure_rate
        {
            return Err(DataVisError::Config(
                "Simulated read failure".to_string(),
            ));
        }

        // Check disconnect rate
        if fault_config.global.disconnect_rate > 0.0
            && rand_simple() < fault_config.global.disconnect_rate
        {
            self.connected = false;
            return Err(DataVisError::Config(
                "Simulated disconnect".to_string(),
            ));
        }

        // Check rate limiting
        if fault_config.global.max_reads_per_second > 0 {
            let now = Instant::now();
            if now.duration_since(self.second_start).as_secs_f64() >= 1.0 {
                self.reads_this_second = 0;
                self.second_start = now;
            }
            self.reads_this_second += 1;
            if self.reads_this_second > fault_config.global.max_reads_per_second {
                return Err(DataVisError::Timeout(
                    "Rate limit exceeded".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Apply simulated latency based on the configured profile
    fn apply_fault_latency(&mut self) {
        let fault_config = match &self.fault_config {
            Some(c) => c,
            None => return,
        };

        let delay_us = match &fault_config.global.latency_profile {
            LatencyProfile::Constant(us) => *us as f64,
            LatencyProfile::Normal { mean_us, stddev_us } => {
                rand_normal(*mean_us as f64, *stddev_us as f64).max(0.0)
            }
            LatencyProfile::Uniform { min_us, max_us } => {
                *min_us as f64 + rand_simple() * (*max_us - *min_us) as f64
            }
            LatencyProfile::WithSpikes {
                base_us,
                spike_us,
                spike_probability,
            } => {
                if rand_simple() < *spike_probability {
                    *spike_us as f64
                } else {
                    *base_us as f64
                }
            }
            LatencyProfile::Degrading {
                start_us,
                increase_per_read_us,
            } => {
                let delay = *start_us as f64 + self.accumulated_latency_increase;
                self.accumulated_latency_increase += increase_per_read_us;
                delay
            }
        };

        if delay_us > 0.0 {
            std::thread::sleep(std::time::Duration::from_micros(delay_us as u64));
        }
    }

    /// Optionally corrupt a read value based on fault configuration
    fn apply_corruption(&self, value: f64, variable: &Variable) -> f64 {
        let fault_config = match &self.fault_config {
            Some(c) => c,
            None => return value,
        };

        // Check per-variable corruption
        if let Some(var_fault) = fault_config.variable_faults.get(&variable.id) {
            if var_fault.corrupt {
                let offset = (rand_simple() - 0.5) * 2.0 * 1000.0;
                return value + offset;
            }
        }

        // Check global corruption
        if let Some(ref corruption) = fault_config.global.corruption {
            if rand_simple() < corruption.rate {
                let offset = (rand_simple() - 0.5) * 2.0 * corruption.magnitude;
                return value + offset;
            }
        }

        value
    }

    /// Convert a FaultErrorKind to a DataVisError
    fn fault_error_to_datavis(
        &self,
        kind: &FaultErrorKind,
        variable: &Variable,
    ) -> DataVisError {
        match kind {
            FaultErrorKind::Timeout => {
                DataVisError::Timeout("Simulated periodic timeout".to_string())
            }
            FaultErrorKind::MemoryAccess => DataVisError::MemoryAccess {
                address: variable.address,
                message: "Simulated periodic memory access error".to_string(),
            },
            FaultErrorKind::Disconnect => {
                DataVisError::Config("Simulated periodic disconnect".to_string())
            }
            FaultErrorKind::Generic(msg) => DataVisError::Config(msg.clone()),
        }
    }

    /// Configure a specific variable's mock behavior
    pub fn configure_variable(&mut self, config: MockVariableConfig) {
        self.variable_configs.insert(config.variable_id, config);
    }

    /// Get access to mock memory for setup
    pub fn memory_mut(&mut self) -> &mut MockMemory {
        &mut self.memory
    }

    /// Connect to the mock probe
    pub fn connect(&mut self, _selector: Option<&str>, target: &str) -> Result<()> {
        self.target_name = target.to_string();
        self.connected = true;
        self.start_time = Instant::now();
        tracing::info!("Mock probe connected to target: {}", target);
        Ok(())
    }

    /// Disconnect from the mock probe
    pub fn disconnect(&mut self) {
        self.connected = false;
        tracing::info!("Mock probe disconnected");
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Read a variable's value
    pub fn read_variable(&mut self, variable: &Variable) -> Result<f64> {
        if !self.connected {
            return Err(DataVisError::Config("Mock probe not connected".to_string()));
        }

        // Fault injection: check faults before reading
        if self.fault_config.is_some() {
            self.check_fault(variable)?;
            self.apply_fault_latency();
        } else {
            // Original read delay when no fault config is set
            if self.read_delay_us > 0 {
                std::thread::sleep(std::time::Duration::from_micros(self.read_delay_us));
            }
        }

        let elapsed = self.start_time.elapsed().as_secs_f64();

        // Check if we have a specific config for this variable
        if let Some(config) = self.variable_configs.get_mut(&variable.id) {
            let value = config.generate_value(elapsed);
            let value = self.apply_corruption(value, variable);
            return Ok(value);
        }

        // If not in pattern-only mode, try to read from mock memory first
        if !self.pattern_only_mode {
            if let Some(bytes) = self
                .memory
                .read(variable.address, variable.var_type.size_bytes())
            {
                if let Some(value) = variable.var_type.parse_to_f64(&bytes) {
                    let value = self.apply_corruption(value, variable);
                    return Ok(value);
                }
            }
        }

        // Generate data based on default pattern + address for variation
        // Use address to create unique but deterministic patterns per variable
        let address_hash = (variable.address as f64) / 10000.0;
        let phase_offset = (variable.address % 1000) as f64 / 1000.0 * std::f64::consts::PI * 2.0;

        let value = match self.default_pattern {
            MockDataPattern::Constant(v) => v + address_hash,
            MockDataPattern::Sine {
                frequency,
                amplitude,
                offset,
            } => {
                // Each variable gets a unique phase and slightly different frequency
                let var_freq = frequency * (1.0 + (variable.address % 100) as f64 * 0.01);
                offset
                    + amplitude
                        * (2.0 * std::f64::consts::PI * var_freq * elapsed + phase_offset).sin()
            }
            MockDataPattern::Counter { step, min, max } => {
                // Create a counter that varies by address
                let range = max - min;
                let pos = (elapsed * step + address_hash * 10.0) % range;
                min + pos
            }
            MockDataPattern::Random { min, max } => min + rand_simple() * (max - min),
            MockDataPattern::Sawtooth { period, amplitude } => {
                let t = (elapsed + phase_offset / (2.0 * std::f64::consts::PI) * period) % period;
                amplitude * (t / period)
            }
            MockDataPattern::Square { period, amplitude } => {
                let t = (elapsed + phase_offset / (2.0 * std::f64::consts::PI) * period) % period;
                if t < period / 2.0 {
                    amplitude
                } else {
                    -amplitude
                }
            }
            MockDataPattern::Triangle { period, amplitude } => {
                let t = (elapsed + phase_offset / (2.0 * std::f64::consts::PI) * period) % period;
                let half = period / 2.0;
                if t < half {
                    amplitude * (2.0 * t / half - 1.0)
                } else {
                    amplitude * (1.0 - 2.0 * (t - half) / half)
                }
            }
        };

        // Apply corruption if fault config is set
        let value = self.apply_corruption(value, variable);

        Ok(value)
    }

    /// Read raw memory
    pub fn read_memory(&mut self, address: u64, size: usize) -> Result<Vec<u8>> {
        if !self.connected {
            return Err(DataVisError::Config("Mock probe not connected".to_string()));
        }

        self.memory
            .read(address, size)
            .ok_or_else(|| DataVisError::MemoryAccess {
                address,
                message: "Address not in mock memory".to_string(),
            })
    }

    /// Write to mock memory
    pub fn write_memory(&mut self, address: u64, data: &[u8]) -> Result<()> {
        if !self.connected {
            return Err(DataVisError::Config("Mock probe not connected".to_string()));
        }

        if self.memory.write(address, data) {
            Ok(())
        } else {
            Err(DataVisError::MemoryAccess {
                address,
                message: "Failed to write to mock memory".to_string(),
            })
        }
    }

    /// Write a variable's value to memory
    ///
    /// Converts the f64 value to the appropriate byte representation based on
    /// the variable's type and writes it to mock memory.
    pub fn write_variable(&mut self, variable: &Variable, value: f64) -> Result<()> {
        if !self.connected {
            return Err(DataVisError::Config("Mock probe not connected".to_string()));
        }

        let bytes = match variable.var_type {
            VariableType::U8 => vec![(value as u8)],
            VariableType::I8 => vec![(value as i8 as u8)],
            VariableType::Bool => vec![if value != 0.0 { 1 } else { 0 }],
            VariableType::U16 => (value as u16).to_le_bytes().to_vec(),
            VariableType::I16 => (value as i16).to_le_bytes().to_vec(),
            VariableType::U32 => (value as u32).to_le_bytes().to_vec(),
            VariableType::I32 => (value as i32).to_le_bytes().to_vec(),
            VariableType::F32 => (value as f32).to_le_bytes().to_vec(),
            VariableType::U64 => (value as u64).to_le_bytes().to_vec(),
            VariableType::I64 => (value as i64).to_le_bytes().to_vec(),
            VariableType::F64 => value.to_le_bytes().to_vec(),
            VariableType::Raw(_) => {
                return Err(DataVisError::Variable(
                    "Cannot write raw type variables".to_string(),
                ))
            }
        };

        self.write_memory(variable.address, &bytes)
    }

    /// Get target name
    pub fn target_name(&self) -> &str {
        &self.target_name
    }
}

impl Default for MockProbeBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Implementation of the DebugProbe trait for MockProbeBackend
///
/// This allows MockProbeBackend to be used interchangeably with real probe
/// implementations through the trait object interface.
impl DebugProbe for MockProbeBackend {
    fn connect(&mut self, selector: Option<&str>, target: &str) -> Result<()> {
        MockProbeBackend::connect(self, selector, target)
    }

    fn disconnect(&mut self) {
        MockProbeBackend::disconnect(self)
    }

    fn is_connected(&self) -> bool {
        MockProbeBackend::is_connected(self)
    }

    fn read_variable(&mut self, variable: &Variable) -> Result<f64> {
        let start = Instant::now();
        let result = MockProbeBackend::read_variable(self, variable);
        // Ensure minimum 1μs to avoid division by zero in rate calculations
        let elapsed = start.elapsed().as_micros().max(1) as u64;

        match &result {
            Ok(_) => self
                .stats
                .record_success(elapsed, variable.var_type.size_bytes() as u64),
            Err(_) => self.stats.record_failure(),
        }

        result
    }

    fn read_variables(&mut self, variables: &[Variable]) -> Vec<Result<f64>> {
        variables.iter().map(|v| self.read_variable(v)).collect()
    }

    fn write_variable(&mut self, variable: &Variable, value: f64) -> Result<()> {
        MockProbeBackend::write_variable(self, variable, value)
    }

    fn read_memory(&mut self, address: u64, size: usize) -> Result<Vec<u8>> {
        MockProbeBackend::read_memory(self, address, size)
    }

    fn write_memory(&mut self, address: u64, data: &[u8]) -> Result<()> {
        MockProbeBackend::write_memory(self, address, data)
    }

    fn halt(&mut self) -> Result<()> {
        if !self.connected {
            return Err(DataVisError::Config("Mock probe not connected".to_string()));
        }
        self.halted = true;
        tracing::info!("Mock probe halted");
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        if !self.connected {
            return Err(DataVisError::Config("Mock probe not connected".to_string()));
        }
        self.halted = false;
        tracing::info!("Mock probe resumed");
        Ok(())
    }

    fn reset(&mut self, halt: bool) -> Result<()> {
        if !self.connected {
            return Err(DataVisError::Config("Mock probe not connected".to_string()));
        }
        self.halted = halt;
        self.start_time = Instant::now(); // Reset the pattern generation time
        tracing::info!("Mock probe reset (halt={})", halt);
        Ok(())
    }

    fn is_halted(&mut self) -> Result<bool> {
        if !self.connected {
            return Err(DataVisError::Config("Mock probe not connected".to_string()));
        }
        Ok(self.halted)
    }

    fn stats(&self) -> &ProbeStats {
        &self.stats
    }

    fn stats_mut(&mut self) -> &mut ProbeStats {
        &mut self.stats
    }
}

/// Information about a mock probe (for listing)
#[derive(Debug, Clone)]
pub struct MockProbeInfo {
    pub name: String,
    pub description: String,
}

impl MockProbeInfo {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
        }
    }
}

/// List available mock probes
pub fn list_mock_probes() -> Vec<MockProbeInfo> {
    vec![
        MockProbeInfo::new("Mock Probe (Sine)", "Generates sinusoidal test data"),
        MockProbeInfo::new("Mock Probe (Random)", "Generates random test data"),
        MockProbeInfo::new(
            "Mock Probe (Counter)",
            "Generates incrementing counter data",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_probe_connect() {
        let mut probe = MockProbeBackend::new();
        assert!(!probe.is_connected());

        probe.connect(None, "TestTarget").unwrap();
        assert!(probe.is_connected());

        probe.disconnect();
        assert!(!probe.is_connected());
    }

    #[test]
    fn test_mock_memory() {
        let mut memory = MockMemory::new();
        memory.add_region(0x2000_0000, 1024);

        // Write and read back
        let data = [1u8, 2, 3, 4];
        assert!(memory.write(0x2000_0000, &data));

        let read = memory.read(0x2000_0000, 4).unwrap();
        assert_eq!(read, data);
    }

    #[test]
    fn test_mock_variable_patterns() {
        let mut config = MockVariableConfig::new(1, MockDataPattern::Constant(42.0));
        assert_eq!(config.generate_value(0.0), 42.0);
        assert_eq!(config.generate_value(1.0), 42.0);

        let mut counter = MockVariableConfig::new(
            2,
            MockDataPattern::Counter {
                step: 1.0,
                min: 0.0,
                max: 10.0,
            },
        );
        assert_eq!(counter.generate_value(0.0), 1.0);
        assert_eq!(counter.generate_value(0.0), 2.0);
        assert_eq!(counter.generate_value(0.0), 3.0);
    }

    #[test]
    fn test_mock_probe_read_variable() {
        let mut probe = MockProbeBackend::new();
        probe.connect(None, "Test").unwrap();

        let var = Variable::new("test", 0x2000_0000, crate::types::VariableType::U32);
        let value = probe.read_variable(&var).unwrap();

        // Should return some value (either from pattern or memory)
        assert!(value.is_finite());
    }
}
