//! Fault injection types for MockProbeBackend
//!
//! This module defines configurable fault injection types that allow
//! testing error handling, timeout behavior, and degraded performance
//! scenarios without real hardware.

use std::collections::HashMap;

/// Top-level fault configuration
#[derive(Debug, Clone, Default)]
pub struct FaultConfig {
    /// Global fault settings applied to all reads
    pub global: GlobalFaults,
    /// Per-variable fault overrides, keyed by variable ID
    pub variable_faults: HashMap<u32, VariableFaults>,
}

/// Global fault settings applied to all variable reads
#[derive(Debug, Clone)]
pub struct GlobalFaults {
    /// Probability (0.0 to 1.0) that any read will fail
    pub read_failure_rate: f64,
    /// Probability (0.0 to 1.0) that a disconnect will be simulated
    pub disconnect_rate: f64,
    /// Optional corruption applied to read values
    pub corruption: Option<CorruptionConfig>,
    /// Latency profile for simulated read delays
    pub latency_profile: LatencyProfile,
    /// Maximum reads allowed per second (0 = unlimited)
    pub max_reads_per_second: u32,
    /// Optional periodic failure pattern
    pub periodic_failure: Option<PeriodicFailure>,
}

impl Default for GlobalFaults {
    fn default() -> Self {
        Self {
            read_failure_rate: 0.0,
            disconnect_rate: 0.0,
            corruption: None,
            latency_profile: LatencyProfile::Constant(100),
            max_reads_per_second: 0,
            periodic_failure: None,
        }
    }
}

/// Per-variable fault overrides
#[derive(Debug, Clone, Default)]
pub struct VariableFaults {
    /// Override the global read failure rate for this variable
    pub read_failure_rate: Option<f64>,
    /// If true, reads of this variable always return a timeout error
    pub always_timeout: bool,
    /// If true, values read from this variable are always corrupted
    pub corrupt: bool,
}

/// Latency simulation profiles
#[derive(Debug, Clone)]
pub enum LatencyProfile {
    /// Fixed latency in microseconds
    Constant(u64),
    /// Normally distributed latency
    Normal {
        mean_us: u64,
        stddev_us: u64,
    },
    /// Uniformly distributed latency
    Uniform {
        min_us: u64,
        max_us: u64,
    },
    /// Mostly stable with occasional spikes
    WithSpikes {
        base_us: u64,
        spike_us: u64,
        spike_probability: f64,
    },
    /// Latency that increases over time
    Degrading {
        start_us: u64,
        increase_per_read_us: f64,
    },
}

/// Configuration for periodic failure injection
#[derive(Debug, Clone)]
pub struct PeriodicFailure {
    /// Trigger a failure every N reads
    pub every_n_reads: u64,
    /// Number of consecutive failures when triggered
    pub failure_count: u64,
    /// The kind of error to produce
    pub error_kind: FaultErrorKind,
}

/// The kind of error a fault produces
#[derive(Debug, Clone)]
pub enum FaultErrorKind {
    /// Simulated timeout
    Timeout,
    /// Simulated memory access error
    MemoryAccess,
    /// Simulated disconnect
    Disconnect,
    /// Generic error with a custom message
    Generic(String),
}

/// Configuration for value corruption
#[derive(Debug, Clone)]
pub struct CorruptionConfig {
    /// Probability (0.0 to 1.0) that a value will be corrupted
    pub rate: f64,
    /// Magnitude of corruption (multiplied by random offset)
    pub magnitude: f64,
}
