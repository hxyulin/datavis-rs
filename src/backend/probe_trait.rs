//! DebugProbe trait for unified probe interface
//!
//! This module provides a common trait for all debug probe implementations,
//! enabling both real hardware probes (via probe-rs) and mock probes for testing.

use crate::config::MemoryAccessMode;
use crate::error::Result;
use crate::types::Variable;
use std::collections::VecDeque;

/// Size of the rolling window for recent read times
const RECENT_WINDOW_SIZE: usize = 100;

/// Statistics for probe operations
///
/// Tracks success rates, timing, throughput, and latency metrics for probe operations.
#[derive(Debug, Clone)]
pub struct ProbeStats {
    /// Total number of successful reads
    pub successful_reads: u64,
    /// Total number of failed reads
    pub failed_reads: u64,
    /// Total read time in microseconds
    pub total_read_time_us: u64,
    /// Last read time in microseconds
    pub last_read_time_us: u64,
    /// Total bytes read
    pub total_bytes_read: u64,

    // Latency tracking
    /// Minimum read time observed (microseconds)
    pub min_read_time_us: u64,
    /// Maximum read time observed (microseconds)
    pub max_read_time_us: u64,
    /// Rolling window of recent read times for jitter calculation
    pub recent_read_times: VecDeque<u64>,

    // Bulk read stats
    /// Number of bulk read operations performed
    pub bulk_reads_performed: u64,
    /// Number of individual reads saved by bulk optimization
    pub individual_reads_saved: u64,
}

impl Default for ProbeStats {
    fn default() -> Self {
        Self {
            successful_reads: 0,
            failed_reads: 0,
            total_read_time_us: 0,
            last_read_time_us: 0,
            total_bytes_read: 0,
            min_read_time_us: u64::MAX,
            max_read_time_us: 0,
            recent_read_times: VecDeque::with_capacity(RECENT_WINDOW_SIZE),
            bulk_reads_performed: 0,
            individual_reads_saved: 0,
        }
    }
}

impl ProbeStats {
    /// Calculate average read time in microseconds
    pub fn avg_read_time_us(&self) -> f64 {
        if self.successful_reads == 0 {
            0.0
        } else {
            self.total_read_time_us as f64 / self.successful_reads as f64
        }
    }

    /// Calculate success rate as percentage
    pub fn success_rate(&self) -> f64 {
        let total = self.successful_reads + self.failed_reads;
        if total == 0 {
            100.0
        } else {
            (self.successful_reads as f64 / total as f64) * 100.0
        }
    }

    /// Record a successful read operation with latency tracking
    pub fn record_success(&mut self, time_us: u64, bytes: u64) {
        self.successful_reads += 1;
        self.total_read_time_us += time_us;
        self.last_read_time_us = time_us;
        self.total_bytes_read += bytes;

        // Update min/max
        if time_us < self.min_read_time_us {
            self.min_read_time_us = time_us;
        }
        if time_us > self.max_read_time_us {
            self.max_read_time_us = time_us;
        }

        // Update rolling window
        self.recent_read_times.push_back(time_us);
        if self.recent_read_times.len() > RECENT_WINDOW_SIZE {
            self.recent_read_times.pop_front();
        }
    }

    /// Record a failed read operation
    pub fn record_failure(&mut self) {
        self.failed_reads += 1;
    }

    /// Calculate jitter (max - min) over recent window in microseconds
    pub fn jitter_us(&self) -> u64 {
        if self.recent_read_times.is_empty() {
            return 0;
        }
        let min = self.recent_read_times.iter().min().copied().unwrap_or(0);
        let max = self.recent_read_times.iter().max().copied().unwrap_or(0);
        max.saturating_sub(min)
    }

    /// Calculate standard deviation of recent read times in microseconds
    pub fn stddev_us(&self) -> f64 {
        if self.recent_read_times.len() < 2 {
            return 0.0;
        }
        let mean = self.recent_read_times.iter().sum::<u64>() as f64
            / self.recent_read_times.len() as f64;
        let variance = self
            .recent_read_times
            .iter()
            .map(|&t| (t as f64 - mean).powi(2))
            .sum::<f64>()
            / (self.recent_read_times.len() - 1) as f64;
        variance.sqrt()
    }

    /// Get the recent min read time (from rolling window)
    pub fn recent_min_us(&self) -> u64 {
        self.recent_read_times.iter().min().copied().unwrap_or(0)
    }

    /// Get the recent max read time (from rolling window)
    pub fn recent_max_us(&self) -> u64 {
        self.recent_read_times.iter().max().copied().unwrap_or(0)
    }

    /// Reset all statistics
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Unified interface for debug probes
///
/// This trait provides a common interface for both real hardware probes
/// (via probe-rs) and mock probes for testing. Implementations must be
/// `Send` to allow use across threads.
///
/// # Example
///
/// ```ignore
/// fn read_all_variables(probe: &mut dyn DebugProbe, vars: &[Variable]) -> Vec<Result<f64>> {
///     probe.read_variables(vars)
/// }
/// ```
pub trait DebugProbe: Send {
    /// Connect to the target
    ///
    /// # Arguments
    /// * `selector` - Optional probe selector (e.g., "0483:374b:serial")
    /// * `target` - Target chip name (e.g., "STM32F407VGTx")
    fn connect(&mut self, selector: Option<&str>, target: &str) -> Result<()>;

    /// Disconnect from the target
    fn disconnect(&mut self);

    /// Check if connected to a target
    fn is_connected(&self) -> bool;

    /// Read a single variable's value
    ///
    /// The value is interpreted according to the variable's type.
    fn read_variable(&mut self, variable: &Variable) -> Result<f64>;

    /// Read multiple variables efficiently
    ///
    /// Default implementation reads one by one, but implementations
    /// may override for batch optimization.
    fn read_variables(&mut self, variables: &[Variable]) -> Vec<Result<f64>> {
        variables.iter().map(|v| self.read_variable(v)).collect()
    }

    /// Write a variable's value
    ///
    /// The value is converted to bytes according to the variable's type.
    fn write_variable(&mut self, variable: &Variable, value: f64) -> Result<()>;

    /// Read raw memory from the target
    ///
    /// # Arguments
    /// * `address` - Memory address to read from
    /// * `size` - Number of bytes to read
    fn read_memory(&mut self, address: u64, size: usize) -> Result<Vec<u8>>;

    /// Write raw memory to the target
    ///
    /// # Arguments
    /// * `address` - Memory address to write to
    /// * `data` - Bytes to write
    fn write_memory(&mut self, address: u64, data: &[u8]) -> Result<()>;

    /// Halt the target core
    fn halt(&mut self) -> Result<()>;

    /// Resume the target core
    fn resume(&mut self) -> Result<()>;

    /// Reset the target
    ///
    /// # Arguments
    /// * `halt` - If true, halt the target after reset
    fn reset(&mut self, halt: bool) -> Result<()>;

    /// Check if the target is halted
    fn is_halted(&mut self) -> Result<bool>;

    /// Get probe operation statistics
    fn stats(&self) -> &ProbeStats;

    /// Get mutable reference to probe statistics
    fn stats_mut(&mut self) -> &mut ProbeStats;

    /// Reset probe statistics
    fn reset_stats(&mut self) {
        self.stats_mut().reset();
    }

    /// Get the current memory access mode
    fn memory_access_mode(&self) -> MemoryAccessMode;

    /// Set the memory access mode
    fn set_memory_access_mode(&mut self, mode: MemoryAccessMode);
}

/// Information about a detected probe (for listing)
#[derive(Debug, Clone)]
pub enum DetectedProbeInfo {
    /// A real hardware probe
    Hardware {
        /// Vendor ID
        vendor_id: u16,
        /// Product ID
        product_id: u16,
        /// Serial number (if available)
        serial_number: Option<String>,
        /// Probe type/name
        probe_type: String,
    },
    /// A mock probe for testing
    #[cfg(feature = "mock-probe")]
    Mock {
        /// Display name
        name: String,
        /// Description
        description: String,
    },
}

impl DetectedProbeInfo {
    /// Get a display-friendly name for this probe
    pub fn display_name(&self) -> String {
        match self {
            DetectedProbeInfo::Hardware {
                probe_type,
                vendor_id,
                product_id,
                serial_number,
            } => {
                if let Some(ref serial) = serial_number {
                    format!(
                        "{} ({:04x}:{:04x}) - {}",
                        probe_type, vendor_id, product_id, serial
                    )
                } else {
                    format!("{} ({:04x}:{:04x})", probe_type, vendor_id, product_id)
                }
            }
            #[cfg(feature = "mock-probe")]
            DetectedProbeInfo::Mock { name, .. } => format!("{} (Mock)", name),
        }
    }

    /// Check if this is a mock probe
    pub fn is_mock(&self) -> bool {
        #[cfg(feature = "mock-probe")]
        {
            matches!(self, DetectedProbeInfo::Mock { .. })
        }
        #[cfg(not(feature = "mock-probe"))]
        {
            false
        }
    }
}

impl std::fmt::Display for DetectedProbeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}
