//! Probe-RS backend for SWD communication
//!
//! This module provides the low-level interface to debug probes using probe-rs.
//! It handles connection, memory reading/writing, and error recovery.
//!
//! # Features
//!
//! - **Probe discovery**: List available debug probes (ST-Link, J-Link, CMSIS-DAP, etc.)
//! - **Connection management**: Connect/disconnect with configurable options
//! - **Memory access**: Read and write memory at arbitrary addresses
//! - **Variable access**: Read/write typed variables with automatic byte conversion
//! - **Target control**: Halt, resume, and reset the target MCU
//! - **Statistics**: Track read success rates and timing
//!
//! # Supported Probes
//!
//! Any probe supported by probe-rs, including:
//! - ST-Link (V2, V2-1, V3)
//! - J-Link
//! - CMSIS-DAP compatible probes
//! - ESP-PROG
//! - Raspberry Pi Pico (picoprobe)
//!
//! # Example
//!
//! ```ignore
//! use datavis_rs::backend::ProbeBackend;
//! use datavis_rs::config::ProbeConfig;
//!
//! let mut probe = ProbeBackend::new(ProbeConfig::default());
//!
//! // List available probes
//! for info in ProbeBackend::list_probes() {
//!     println!("Found: {}", info);
//! }
//!
//! // Connect to target
//! probe.connect(None, "STM32F407VGTx")?;
//!
//! // Read a variable
//! let value = probe.read_variable(&my_variable)?;
//! ```

use crate::config::{AppConfig, ConnectUnderReset, MemoryAccessMode, ProbeConfig, ProbeProtocol};
use crate::error::{DataVisError, Result};
use crate::types::{Variable, VariableType};
use probe_rs::{probe::list::Lister, MemoryInterface, Permissions, Session};
use std::time::{Duration, Instant};

/// Information about a detected probe
#[derive(Debug, Clone)]
pub struct ProbeInfo {
    /// Vendor ID
    pub vendor_id: u16,
    /// Product ID
    pub product_id: u16,
    /// Serial number (if available)
    pub serial_number: Option<String>,
    /// Probe type/name
    pub probe_type: String,
}

impl std::fmt::Display for ProbeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref serial) = self.serial_number {
            write!(
                f,
                "{} ({:04x}:{:04x}) - {}",
                self.probe_type, self.vendor_id, self.product_id, serial
            )
        } else {
            write!(
                f,
                "{} ({:04x}:{:04x})",
                self.probe_type, self.vendor_id, self.product_id
            )
        }
    }
}

/// Backend for probe-rs operations
pub struct ProbeBackend {
    /// Active session with the target
    session: Option<Session>,
    /// Configuration for the probe
    config: ProbeConfig,
    /// Read buffer for memory operations
    read_buffer: Vec<u8>,
    /// Statistics
    stats: ProbeStats,
}

/// Statistics for probe operations
#[derive(Debug, Clone, Default)]
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
}

impl ProbeBackend {
    /// Create a new probe backend with the given configuration
    pub fn new(config: ProbeConfig) -> Self {
        Self {
            session: None,
            config,
            read_buffer: vec![0u8; 256], // Pre-allocate buffer
            stats: ProbeStats::default(),
        }
    }

    /// Create a new probe backend from application config
    pub fn from_app_config(config: &AppConfig) -> Self {
        Self::new(config.probe.clone())
    }

    /// List all available probes
    /// This should be called from a background thread, not the UI thread.
    pub fn list_probes() -> Vec<ProbeInfo> {
        let lister = Lister::new();
        lister
            .list_all()
            .into_iter()
            .map(|probe| ProbeInfo {
                vendor_id: probe.vendor_id,
                product_id: probe.product_id,
                serial_number: probe.serial_number.clone(),
                probe_type: probe.probe_type().to_string(),
            })
            .collect()
    }

    /// Connect to a probe
    ///
    /// If `selector` is None, connects to the first available probe.
    pub fn connect(&mut self, selector: Option<&str>) -> Result<()> {
        // Disconnect if already connected
        self.disconnect();

        tracing::info!(
            "Connecting with config: target={}, speed={}kHz, protocol={:?}, connect_under_reset={:?}, halt_on_connect={}",
            self.config.target_chip,
            self.config.speed_khz,
            self.config.protocol,
            self.config.connect_under_reset,
            self.config.halt_on_connect
        );

        // List available probes
        let lister = Lister::new();
        let probes = lister.list_all();

        if probes.is_empty() {
            tracing::error!("No probes found");
            return Err(DataVisError::Config("No probes found".to_string()));
        }

        tracing::debug!("Found {} probe(s)", probes.len());

        // Select the probe
        let probe = if let Some(selector_str) = selector {
            // Try to find probe matching the selector
            let probe_info = probes.into_iter().find(|p| {
                let selector_lower = selector_str.to_lowercase();
                // Match by VID:PID format or serial number
                let vid_pid = format!("{:04x}:{:04x}", p.vendor_id, p.product_id);
                vid_pid == selector_lower
                    || p.serial_number
                        .as_ref()
                        .map(|s| s.to_lowercase().contains(&selector_lower))
                        .unwrap_or(false)
            });

            probe_info
                .ok_or_else(|| DataVisError::Config(format!("Probe not found: {}", selector_str)))?
                .open()?
        } else {
            // Use the first available probe
            probes
                .first()
                .ok_or_else(|| DataVisError::Config("No probes available".to_string()))?
                .open()?
        };

        // Configure probe speed
        let mut probe = probe;
        tracing::debug!("Setting probe speed to {} kHz", self.config.speed_khz);
        if let Err(e) = probe.set_speed(self.config.speed_khz) {
            tracing::warn!("Failed to set probe speed: {}", e);
        }

        // Select protocol
        let protocol = match self.config.protocol {
            ProbeProtocol::Swd => probe_rs::probe::WireProtocol::Swd,
            ProbeProtocol::Jtag => probe_rs::probe::WireProtocol::Jtag,
        };

        tracing::debug!("Selecting protocol: {:?}", protocol);
        if let Err(e) = probe.select_protocol(protocol) {
            tracing::error!("Failed to select protocol: {}", e);
            return Err(e.into());
        }

        // Attach to target
        tracing::debug!("Looking up target: {}", self.config.target_chip);
        let target = match probe_rs::config::get_target_by_name(&self.config.target_chip) {
            Ok(t) => {
                tracing::debug!("Found target: {}", t.name);
                t
            }
            Err(e) => {
                tracing::error!("Failed to find target '{}': {}", self.config.target_chip, e);
                return Err(e.into());
            }
        };
        let permissions = Permissions::default();

        tracing::info!(
            "Attaching with method: {:?}",
            self.config.connect_under_reset
        );
        let mut session = match self.config.connect_under_reset {
            ConnectUnderReset::None => {
                // Normal attach without reset
                tracing::debug!("Using normal attach (no reset)");
                match probe.attach(target, permissions) {
                    Ok(s) => {
                        tracing::info!("Normal attach successful");
                        s
                    }
                    Err(e) => {
                        tracing::error!("Normal attach failed: {}", e);
                        return Err(e.into());
                    }
                }
            }
            ConnectUnderReset::Hardware => {
                // Hardware reset using NRST pin (attach_under_reset uses hardware reset by default)
                tracing::debug!("Using attach under hardware reset (NRST pin)");
                match probe.attach_under_reset(target, permissions) {
                    Ok(s) => {
                        tracing::info!("Attach under hardware reset successful");
                        s
                    }
                    Err(e) => {
                        tracing::error!(
                            "Attach under hardware reset failed: {}. Ensure NRST pin is connected.",
                            e
                        );
                        return Err(e.into());
                    }
                }
            }
            ConnectUnderReset::Software | ConnectUnderReset::Core => {
                // For software/core reset, attach first then reset
                // probe-rs attach_under_reset uses hardware reset, so we do it manually
                tracing::debug!("Using normal attach followed by software/core reset");
                let mut sess = match probe.attach(target, permissions) {
                    Ok(s) => {
                        tracing::debug!("Initial attach successful, will perform reset");
                        s
                    }
                    Err(e) => {
                        tracing::error!("Initial attach failed (before reset): {}", e);
                        return Err(e.into());
                    }
                };
                if let Ok(mut core) = sess.core(0) {
                    match self.config.connect_under_reset {
                        ConnectUnderReset::Software => {
                            // SYSRESETREQ - software reset, resets core and peripherals
                            match core.reset() {
                                Ok(()) => tracing::info!("Software reset (SYSRESETREQ) successful"),
                                Err(e) => tracing::warn!("Software reset failed: {}", e),
                            }
                        }
                        ConnectUnderReset::Core => {
                            // VECTRESET - core reset only, peripherals keep running
                            // Use reset_and_halt then resume for core-only reset behavior
                            match core.reset_and_halt(Duration::from_millis(100)) {
                                Ok(_) => {
                                    tracing::info!(
                                        "Core reset (VECTRESET) successful, core halted"
                                    );
                                    if !self.config.halt_on_connect {
                                        match core.run() {
                                            Ok(()) => tracing::info!("Core resumed after reset"),
                                            Err(e) => {
                                                tracing::warn!("Failed to resume core: {}", e)
                                            }
                                        }
                                    }
                                }
                                Err(e) => tracing::warn!("Core reset failed: {}", e),
                            }
                        }
                        _ => {}
                    }
                } else {
                    tracing::warn!("Failed to access core 0 for reset");
                }
                sess
            }
        };

        // Halt the core if requested (for non-Core reset methods, Core handles this above)
        if self.config.halt_on_connect && self.config.connect_under_reset != ConnectUnderReset::Core
        {
            match session.core(0) {
                Ok(mut core) => {
                    match core.halt(Duration::from_millis(500)) {
                        Ok(_) => {
                            // Wait for the core to actually halt
                            let timeout = Duration::from_secs(2);
                            let start = Instant::now();
                            loop {
                                match core.status() {
                                    Ok(status) => {
                                        if status.is_halted() {
                                            tracing::info!("Core halted successfully");
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("Failed to read core status: {}", e);
                                        break;
                                    }
                                }
                                if start.elapsed() > timeout {
                                    tracing::warn!("Timeout waiting for core to halt");
                                    break;
                                }
                                std::thread::sleep(Duration::from_millis(10));
                            }
                        }
                        Err(e) => tracing::warn!("Failed to halt core: {}", e),
                    }
                }
                Err(e) => tracing::warn!("Failed to access core 0 for halt: {}", e),
            }
        }

        self.session = Some(session);
        self.stats = ProbeStats::default();

        tracing::info!("Connected to target: {}", self.config.target_chip);
        Ok(())
    }

    /// Disconnect from the current probe
    pub fn disconnect(&mut self) {
        if self.session.take().is_some() {
            tracing::info!("Disconnected from probe");
        }
    }

    /// Check if connected to a probe
    pub fn is_connected(&self) -> bool {
        self.session.is_some()
    }

    /// Read a variable's value from memory
    pub fn read_variable(&mut self, variable: &Variable) -> Result<f64> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| DataVisError::Config("Not connected to probe".to_string()))?;

        let size = variable.var_type.size_bytes();
        if self.read_buffer.len() < size {
            self.read_buffer.resize(size, 0);
        }

        let start = Instant::now();

        // Get the first core (core 0)
        let mut core = session.core(0)?;

        // Read memory
        let result = core.read(variable.address, &mut self.read_buffer[..size]);

        let read_time = start.elapsed();
        self.stats.last_read_time_us = read_time.as_micros() as u64;

        match result {
            Ok(()) => {
                self.stats.successful_reads += 1;
                self.stats.total_read_time_us += self.stats.last_read_time_us;
                self.stats.total_bytes_read += size as u64;

                // Parse the raw bytes to f64
                variable
                    .var_type
                    .parse_to_f64(&self.read_buffer[..size])
                    .ok_or_else(|| DataVisError::Variable("Failed to parse value".to_string()))
            }
            Err(e) => {
                self.stats.failed_reads += 1;
                Err(DataVisError::MemoryAccess {
                    address: variable.address,
                    message: e.to_string(),
                })
            }
        }
    }

    /// Read multiple variables efficiently (batched read)
    /// This acquires the core once and reads all variables, which is much faster
    /// than calling read_variable individually for each variable.
    ///
    /// The memory_access_mode controls how reads are performed:
    /// - Background: Read while target is running (non-intrusive but slower)
    /// - Halted: Halt target before reading, resume after (faster but intrusive)
    /// - HaltedPersistent: Keep target halted (fastest, use when target doesn't need to run)
    pub fn read_variables(&mut self, variables: &[Variable]) -> Vec<Result<f64>> {
        if variables.is_empty() {
            return Vec::new();
        }

        let session = match self.session.as_mut() {
            Some(s) => s,
            None => {
                return variables
                    .iter()
                    .map(|_| Err(DataVisError::Config("Not connected to probe".to_string())))
                    .collect();
            }
        };

        let access_mode = self.config.memory_access_mode;
        tracing::trace!(
            "Reading {} variables with access mode: {:?}",
            variables.len(),
            access_mode
        );
        let start = Instant::now();

        // Get core once for all reads
        let mut core = match session.core(0) {
            Ok(c) => c,
            Err(e) => {
                let error_msg = format!("Failed to access core: {}", e);
                return variables
                    .iter()
                    .map(|_| Err(DataVisError::Config(error_msg.clone())))
                    .collect();
            }
        };

        // For Halted mode, halt the core before reading
        let was_running = match access_mode {
            MemoryAccessMode::Halted => {
                // Check if core is currently running
                let is_running = core.status().map(|s| !s.is_halted()).unwrap_or(false);
                tracing::trace!("Halted mode: core is_running={}", is_running);
                if is_running {
                    if let Err(e) = core.halt(Duration::from_millis(100)) {
                        tracing::warn!("Failed to halt core for read: {}", e);
                    } else {
                        tracing::trace!("Halted core for batch read");
                    }
                }
                is_running
            }
            MemoryAccessMode::HaltedPersistent => {
                // For persistent halt, halt if not already halted but don't resume
                let is_running = core.status().map(|s| !s.is_halted()).unwrap_or(false);
                tracing::trace!("HaltedPersistent mode: core is_running={}", is_running);
                if is_running {
                    if let Err(e) = core.halt(Duration::from_millis(100)) {
                        tracing::warn!("Failed to halt core for read: {}", e);
                    } else {
                        tracing::info!("Halted core persistently for reads");
                    }
                }
                false // Never resume in persistent mode
            }
            MemoryAccessMode::Background => {
                tracing::trace!("Background mode: reading while target runs");
                false // No halt needed
            }
        };

        let mut results = Vec::with_capacity(variables.len());
        let mut total_bytes = 0usize;

        for variable in variables {
            let size = variable.var_type.size_bytes();
            if self.read_buffer.len() < size {
                self.read_buffer.resize(size, 0);
            }

            match core.read(variable.address, &mut self.read_buffer[..size]) {
                Ok(()) => {
                    self.stats.successful_reads += 1;
                    total_bytes += size;

                    let value = variable
                        .var_type
                        .parse_to_f64(&self.read_buffer[..size])
                        .ok_or_else(|| DataVisError::Variable("Failed to parse value".to_string()));
                    results.push(value);
                }
                Err(e) => {
                    self.stats.failed_reads += 1;
                    results.push(Err(DataVisError::MemoryAccess {
                        address: variable.address,
                        message: e.to_string(),
                    }));
                }
            }
        }

        // For Halted mode (non-persistent), resume the core after reading
        if was_running {
            if let Err(e) = core.run() {
                tracing::warn!("Failed to resume core after read: {}", e);
            } else {
                tracing::trace!("Resumed core after batch read");
            }
        }

        let read_time = start.elapsed();
        self.stats.last_read_time_us = read_time.as_micros() as u64;
        self.stats.total_read_time_us += self.stats.last_read_time_us;
        self.stats.total_bytes_read += total_bytes as u64;

        results
    }

    /// Get the current memory access mode
    pub fn memory_access_mode(&self) -> MemoryAccessMode {
        self.config.memory_access_mode
    }

    /// Set the memory access mode
    pub fn set_memory_access_mode(&mut self, mode: MemoryAccessMode) {
        let old_mode = self.config.memory_access_mode;
        self.config.memory_access_mode = mode;
        tracing::info!("Memory access mode changed: {:?} -> {:?}", old_mode, mode);
    }

    /// Check if the target core is currently halted
    pub fn is_halted(&mut self) -> Result<bool> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| DataVisError::Config("Not connected to probe".to_string()))?;

        let mut core = session.core(0)?;
        Ok(core.status()?.is_halted())
    }

    /// Read raw bytes from a memory address
    pub fn read_memory(&mut self, address: u64, size: usize) -> Result<Vec<u8>> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| DataVisError::Config("Not connected to probe".to_string()))?;

        let mut buffer = vec![0u8; size];

        let start = Instant::now();

        let mut core = session.core(0)?;
        core.read(address, &mut buffer)?;

        let read_time = start.elapsed();
        self.stats.last_read_time_us = read_time.as_micros() as u64;
        self.stats.successful_reads += 1;
        self.stats.total_read_time_us += self.stats.last_read_time_us;
        self.stats.total_bytes_read += size as u64;

        Ok(buffer)
    }

    /// Write raw bytes to a memory address
    pub fn write_memory(&mut self, address: u64, data: &[u8]) -> Result<()> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| DataVisError::Config("Not connected to probe".to_string()))?;

        let mut core = session.core(0)?;
        core.write_8(address, data)?;

        Ok(())
    }

    /// Write a variable's value to memory
    pub fn write_variable(&mut self, variable: &Variable, value: f64) -> Result<()> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| DataVisError::Config("Not connected to probe".to_string()))?;

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

        let mut core = session.core(0)?;
        core.write_8(variable.address, &bytes)?;

        Ok(())
    }

    /// Halt the target core
    pub fn halt(&mut self) -> Result<()> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| DataVisError::Config("Not connected to probe".to_string()))?;

        let mut core = session.core(0)?;
        core.halt(Duration::from_millis(100))?;

        Ok(())
    }

    /// Resume the target core
    pub fn resume(&mut self) -> Result<()> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| DataVisError::Config("Not connected to probe".to_string()))?;

        let mut core = session.core(0)?;
        core.run()?;

        Ok(())
    }

    /// Reset the target
    pub fn reset(&mut self, halt: bool) -> Result<()> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| DataVisError::Config("Not connected to probe".to_string()))?;

        let mut core = session.core(0)?;
        if halt {
            core.reset_and_halt(Duration::from_millis(100))?;
        } else {
            core.reset()?;
        }

        Ok(())
    }

    /// Get the current statistics
    pub fn stats(&self) -> &ProbeStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = ProbeStats::default();
    }

    /// Update configuration
    pub fn set_config(&mut self, config: ProbeConfig) {
        self.config = config;
    }

    /// Get current configuration
    pub fn config(&self) -> &ProbeConfig {
        &self.config
    }
}

impl Drop for ProbeBackend {
    fn drop(&mut self) {
        self.disconnect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probe_backend_creation() {
        let config = ProbeConfig::default();
        let backend = ProbeBackend::new(config);
        assert!(!backend.is_connected());
    }

    #[test]
    fn test_probe_stats() {
        let mut stats = ProbeStats::default();
        assert_eq!(stats.avg_read_time_us(), 0.0);
        assert_eq!(stats.success_rate(), 100.0);

        stats.successful_reads = 9;
        stats.failed_reads = 1;
        assert_eq!(stats.success_rate(), 90.0);

        stats.total_read_time_us = 1000;
        assert!((stats.avg_read_time_us() - 111.11).abs() < 0.1);
    }

    #[test]
    #[ignore = "USB enumeration can hang on some systems (especially macOS)"]
    fn test_list_probes() {
        // This won't find any probes in a test environment, but should not panic
        let probes = ProbeBackend::list_probes();
        // Just verify it returns a vector (empty or not)
        let _ = probes.len();
    }
}
