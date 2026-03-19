use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::error::{DataVisError, Result};

/// TCL command terminator used by OpenOCD
const TCL_COMMAND_TERMINATOR: u8 = 0x1a;

pub struct TclClient {
    stream: TcpStream,
}

impl TclClient {
    /// Connect to OpenOCD TCL server
    pub fn connect(addr: SocketAddr) -> Result<Self> {
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
            .map_err(|e| DataVisError::Config(format!("Failed to connect to OpenOCD TCL server at {}: {}", addr, e)))?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| DataVisError::Config(format!("Failed to set read timeout: {}", e)))?;
        stream.set_nodelay(true)
            .map_err(|e| DataVisError::Config(format!("Failed to set TCP_NODELAY: {}", e)))?;
        Ok(Self { stream })
    }

    /// Execute a TCL command and return the raw response
    pub fn execute(&mut self, cmd: &str) -> Result<String> {
        // Send command terminated with 0x1a
        let mut data = cmd.as_bytes().to_vec();
        data.push(TCL_COMMAND_TERMINATOR);
        self.stream.write_all(&data)
            .map_err(|e| DataVisError::Config(format!("Failed to send TCL command: {}", e)))?;

        // Read response until 0x1a
        let mut response = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = self.stream.read(&mut buf)
                .map_err(|e| DataVisError::Config(format!("Failed to read TCL response: {}", e)))?;
            if n == 0 {
                return Err(DataVisError::Config("OpenOCD TCL connection closed".to_string()));
            }
            for &byte in &buf[..n] {
                if byte == TCL_COMMAND_TERMINATOR {
                    let text = String::from_utf8_lossy(&response).to_string();
                    // Check for error in response
                    if text.starts_with("Error:") || text.contains("\nError:") {
                        return Err(DataVisError::Config(format!("OpenOCD error: {}", text.trim())));
                    }
                    return Ok(text);
                }
                response.push(byte);
            }
        }
    }

    /// Read 32-bit words from memory using mdw
    pub fn read_memory_32(&mut self, addr: u64, count: usize) -> Result<Vec<u32>> {
        let response = self.execute(&format!("mdw 0x{:08X} {}", addr, count))?;
        parse_mdw_response(&response)
    }

    /// Read 16-bit halfwords from memory using mdh
    pub fn read_memory_16(&mut self, addr: u64, count: usize) -> Result<Vec<u16>> {
        let response = self.execute(&format!("mdh 0x{:08X} {}", addr, count))?;
        parse_mdh_response(&response)
    }

    /// Read bytes from memory using mdb
    pub fn read_memory_8(&mut self, addr: u64, count: usize) -> Result<Vec<u8>> {
        let response = self.execute(&format!("mdb 0x{:08X} {}", addr, count))?;
        parse_mdb_response(&response)
    }

    /// Write a 32-bit word to memory
    pub fn write_memory_32(&mut self, addr: u64, value: u32) -> Result<()> {
        self.execute(&format!("mww 0x{:08X} 0x{:08X}", addr, value))?;
        Ok(())
    }

    /// Write a 16-bit halfword to memory
    pub fn write_memory_16(&mut self, addr: u64, value: u16) -> Result<()> {
        self.execute(&format!("mwh 0x{:08X} 0x{:04X}", addr, value))?;
        Ok(())
    }

    /// Write a byte to memory
    pub fn write_memory_8(&mut self, addr: u64, value: u8) -> Result<()> {
        self.execute(&format!("mwb 0x{:08X} 0x{:02X}", addr, value))?;
        Ok(())
    }
}

/// Parse mdw response: "0x20000000: 12345678 aabbccdd \n" -> Vec<u32>
fn parse_mdw_response(response: &str) -> Result<Vec<u32>> {
    let mut values = Vec::new();
    for line in response.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        // Format: "0xADDR: VAL1 VAL2 ..."
        if let Some(colon_pos) = line.find(':') {
            let values_part = line[colon_pos + 1..].trim();
            for token in values_part.split_whitespace() {
                let token = token.trim();
                if token.is_empty() { continue; }
                let val = u32::from_str_radix(token.trim_start_matches("0x").trim_start_matches("0X"), 16)
                    .map_err(|e| DataVisError::Config(format!("Failed to parse mdw value '{}': {}", token, e)))?;
                values.push(val);
            }
        }
    }
    Ok(values)
}

/// Parse mdh response similarly for 16-bit values
fn parse_mdh_response(response: &str) -> Result<Vec<u16>> {
    let mut values = Vec::new();
    for line in response.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        if let Some(colon_pos) = line.find(':') {
            let values_part = line[colon_pos + 1..].trim();
            for token in values_part.split_whitespace() {
                let token = token.trim();
                if token.is_empty() { continue; }
                let val = u16::from_str_radix(token.trim_start_matches("0x").trim_start_matches("0X"), 16)
                    .map_err(|e| DataVisError::Config(format!("Failed to parse mdh value '{}': {}", token, e)))?;
                values.push(val);
            }
        }
    }
    Ok(values)
}

/// Parse mdb response for byte values
fn parse_mdb_response(response: &str) -> Result<Vec<u8>> {
    let mut values = Vec::new();
    for line in response.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        if let Some(colon_pos) = line.find(':') {
            let values_part = line[colon_pos + 1..].trim();
            for token in values_part.split_whitespace() {
                let token = token.trim();
                if token.is_empty() { continue; }
                let val = u8::from_str_radix(token.trim_start_matches("0x").trim_start_matches("0X"), 16)
                    .map_err(|e| DataVisError::Config(format!("Failed to parse mdb value '{}': {}", token, e)))?;
                values.push(val);
            }
        }
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mdw_response() {
        let response = "0x20000000: 12345678 aabbccdd \n";
        let values = parse_mdw_response(response).unwrap();
        assert_eq!(values, vec![0x12345678, 0xaabbccdd]);
    }

    #[test]
    fn test_parse_mdw_multiline() {
        let response = "0x20000000: 12345678 \n0x20000004: aabbccdd \n";
        let values = parse_mdw_response(response).unwrap();
        assert_eq!(values, vec![0x12345678, 0xaabbccdd]);
    }

    #[test]
    fn test_parse_mdb_response() {
        let response = "0x20000000: 01 02 03 04 \n";
        let values = parse_mdb_response(response).unwrap();
        assert_eq!(values, vec![0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn test_parse_mdh_response() {
        let response = "0x20000000: 1234 5678 \n";
        let values = parse_mdh_response(response).unwrap();
        assert_eq!(values, vec![0x1234, 0x5678]);
    }

    #[test]
    fn test_parse_empty_response() {
        let values = parse_mdw_response("").unwrap();
        assert!(values.is_empty());
    }
}
