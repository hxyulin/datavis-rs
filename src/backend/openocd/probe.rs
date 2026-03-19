//! OpenOCD DebugProbe implementation
//!
//! Implements the `DebugProbe` trait using OpenOCD's TCL socket interface.
//! Spawns and manages an OpenOCD subprocess, communicating via TCP.

use crate::backend::probe_trait::{DebugProbe, ProbeStats};
use crate::config::ProbeConfig;
use crate::error::{DataVisError, Result};
use crate::types::{Variable, VariableType};

use super::process::OpenOcdProcess;
use super::tcl_client::TclClient;

pub struct OpenOcdProbe {
    process: Option<OpenOcdProcess>,
    client: Option<TclClient>,
    config: ProbeConfig,
    connected: bool,
    stats: ProbeStats,
}

impl OpenOcdProbe {
    pub fn new(config: ProbeConfig) -> Self {
        Self {
            process: None,
            client: None,
            config,
            connected: false,
            stats: ProbeStats::default(),
        }
    }

    /// Read raw bytes for a variable's type size from the appropriate memory command
    fn read_variable_bytes(&mut self, variable: &Variable) -> Result<Vec<u8>> {
        let client = self.client.as_mut().ok_or_else(|| {
            DataVisError::Config("Not connected to OpenOCD".to_string())
        })?;

        let addr = variable.address;
        let size = variable.var_type.size_bytes();

        match size {
            1 => {
                let values = client.read_memory_8(addr, 1)?;
                if values.is_empty() {
                    return Err(DataVisError::Variable("No data returned from mdb".to_string()));
                }
                Ok(vec![values[0]])
            }
            2 => {
                let values = client.read_memory_16(addr, 1)?;
                if values.is_empty() {
                    return Err(DataVisError::Variable("No data returned from mdh".to_string()));
                }
                Ok(values[0].to_le_bytes().to_vec())
            }
            4 => {
                let values = client.read_memory_32(addr, 1)?;
                if values.is_empty() {
                    return Err(DataVisError::Variable("No data returned from mdw".to_string()));
                }
                Ok(values[0].to_le_bytes().to_vec())
            }
            8 => {
                // Read two 32-bit words for 64-bit types
                let values = client.read_memory_32(addr, 2)?;
                if values.len() < 2 {
                    return Err(DataVisError::Variable("Insufficient data for 64-bit read".to_string()));
                }
                // Little-endian: low word first
                let mut bytes = Vec::with_capacity(8);
                bytes.extend_from_slice(&values[0].to_le_bytes());
                bytes.extend_from_slice(&values[1].to_le_bytes());
                Ok(bytes)
            }
            n => {
                // Raw or unknown size - read as bytes
                let values = client.read_memory_8(addr, n)?;
                Ok(values)
            }
        }
    }
}

impl DebugProbe for OpenOcdProbe {
    fn connect(&mut self, _selector: Option<&str>, target: &str) -> Result<()> {
        self.disconnect();

        // Update target chip in config
        if self.config.target_chip != target {
            self.config.target_chip = target.to_string();
        }

        tracing::info!("Starting OpenOCD for target: {}", target);

        // Spawn OpenOCD process
        let process = OpenOcdProcess::spawn(&self.config)?;

        // Connect TCL client
        let client = process.connect_client()?;

        self.process = Some(process);
        self.client = Some(client);
        self.connected = true;
        self.stats = ProbeStats::default();

        tracing::info!("Connected to OpenOCD (target: {})", target);
        Ok(())
    }

    fn disconnect(&mut self) {
        self.connected = false;
        self.client = None;

        if let Some(process) = self.process.take() {
            process.shutdown();
        }
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn read_variable(&mut self, variable: &Variable) -> Result<f64> {
        let start = std::time::Instant::now();

        let bytes = self.read_variable_bytes(variable)?;

        let read_time = start.elapsed();
        let read_time_us = read_time.as_micros() as u64;
        self.stats.record_success(read_time_us, bytes.len() as u64);

        variable
            .var_type
            .parse_to_f64(&bytes)
            .ok_or_else(|| DataVisError::Variable("Failed to parse value".to_string()))
    }

    fn read_variables(&mut self, variables: &[Variable]) -> Vec<Result<f64>> {
        let start = std::time::Instant::now();
        let mut results = Vec::with_capacity(variables.len());
        let mut total_bytes = 0u64;

        for variable in variables {
            match self.read_variable_bytes(variable) {
                Ok(bytes) => {
                    total_bytes += bytes.len() as u64;
                    match variable.var_type.parse_to_f64(&bytes) {
                        Some(value) => results.push(Ok(value)),
                        None => results.push(Err(DataVisError::Variable(
                            format!("Failed to parse value for '{}'", variable.name),
                        ))),
                    }
                }
                Err(e) => {
                    self.stats.record_failure();
                    results.push(Err(e));
                }
            }
        }

        let read_time = start.elapsed();
        let read_time_us = read_time.as_micros() as u64;
        if total_bytes > 0 {
            self.stats.record_success(read_time_us, total_bytes);
        }

        results
    }

    fn write_variable(&mut self, variable: &Variable, value: f64) -> Result<()> {
        let client = self.client.as_mut().ok_or_else(|| {
            DataVisError::Config("Not connected to OpenOCD".to_string())
        })?;

        let addr = variable.address;

        match variable.var_type {
            VariableType::U8 | VariableType::I8 | VariableType::Bool => {
                let byte_val = match variable.var_type {
                    VariableType::I8 => value as i8 as u8,
                    VariableType::Bool => if value != 0.0 { 1 } else { 0 },
                    _ => value as u8,
                };
                client.write_memory_8(addr, byte_val)
            }
            VariableType::U16 | VariableType::I16 => {
                let val = match variable.var_type {
                    VariableType::I16 => value as i16 as u16,
                    _ => value as u16,
                };
                client.write_memory_16(addr, val)
            }
            VariableType::U32 | VariableType::I32 | VariableType::F32 => {
                let val = match variable.var_type {
                    VariableType::I32 => (value as i32) as u32,
                    VariableType::F32 => (value as f32).to_bits(),
                    _ => value as u32,
                };
                client.write_memory_32(addr, val)
            }
            VariableType::U64 | VariableType::I64 | VariableType::F64 => {
                let bytes = match variable.var_type {
                    VariableType::I64 => (value as i64).to_le_bytes(),
                    VariableType::F64 => value.to_le_bytes(),
                    _ => (value as u64).to_le_bytes(),
                };
                // Write as two 32-bit words (little-endian)
                let low = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                let high = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                client.write_memory_32(addr, low)?;
                client.write_memory_32(addr + 4, high)
            }
            VariableType::Raw(_) => {
                Err(DataVisError::Variable("Cannot write raw type variables".to_string()))
            }
        }
    }

    fn read_memory(&mut self, address: u64, size: usize) -> Result<Vec<u8>> {
        let client = self.client.as_mut().ok_or_else(|| {
            DataVisError::Config("Not connected to OpenOCD".to_string())
        })?;
        client.read_memory_8(address, size)
    }

    fn write_memory(&mut self, address: u64, data: &[u8]) -> Result<()> {
        let client = self.client.as_mut().ok_or_else(|| {
            DataVisError::Config("Not connected to OpenOCD".to_string())
        })?;

        // Write aligned 32-bit chunks where possible, then remaining bytes
        let mut offset = 0usize;

        // Write individual bytes for simplicity (OpenOCD handles alignment)
        while offset < data.len() {
            client.write_memory_8(address + offset as u64, data[offset])?;
            offset += 1;
        }

        Ok(())
    }

    fn halt(&mut self) -> Result<()> {
        let client = self.client.as_mut().ok_or_else(|| {
            DataVisError::Config("Not connected to OpenOCD".to_string())
        })?;
        client.execute("halt")?;
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        let client = self.client.as_mut().ok_or_else(|| {
            DataVisError::Config("Not connected to OpenOCD".to_string())
        })?;
        client.execute("resume")?;
        Ok(())
    }

    fn reset(&mut self, halt: bool) -> Result<()> {
        let client = self.client.as_mut().ok_or_else(|| {
            DataVisError::Config("Not connected to OpenOCD".to_string())
        })?;
        if halt {
            client.execute("reset halt")?;
        } else {
            client.execute("reset run")?;
        }
        Ok(())
    }

    fn is_halted(&mut self) -> Result<bool> {
        let client = self.client.as_mut().ok_or_else(|| {
            DataVisError::Config("Not connected to OpenOCD".to_string())
        })?;
        let response = client.execute("$_TARGETNAME curstate")?;
        Ok(response.trim() == "halted")
    }

    fn stats(&self) -> &ProbeStats {
        &self.stats
    }

    fn stats_mut(&mut self) -> &mut ProbeStats {
        &mut self.stats
    }
}

impl Drop for OpenOcdProbe {
    fn drop(&mut self) {
        self.disconnect();
    }
}
