use std::net::{SocketAddr, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::ProbeConfig;
use crate::error::{DataVisError, Result};

use super::chip_map;
use super::tcl_client::TclClient;

pub struct OpenOcdProcess {
    child: Child,
    tcl_port: u16,
}

impl OpenOcdProcess {
    /// Spawn OpenOCD with the given configuration
    pub fn spawn(config: &ProbeConfig) -> Result<Self> {
        let openocd_bin = find_openocd_binary(config)?;
        let tcl_port = find_free_port()?;

        // Determine interface config
        let interface = config.openocd_interface.clone()
            .or_else(|| {
                // Try to derive from probe type - use stlink as default
                Some("stlink".to_string())
            })
            .unwrap_or_else(|| "stlink".to_string());

        // Determine target config
        let target = config.openocd_target.clone()
            .or_else(|| chip_map::chip_to_target(&config.target_chip).map(|s| s.to_string()))
            .ok_or_else(|| DataVisError::Config(
                format!("Could not determine OpenOCD target config for chip '{}'. Please specify in Settings.", config.target_chip)
            ))?;

        // Determine transport
        let transport = match config.protocol {
            crate::config::ProbeProtocol::Swd => "swd",
            crate::config::ProbeProtocol::Jtag => "jtag",
        };

        let mut cmd = Command::new(&openocd_bin);

        // Add scripts directory if using bundled OpenOCD
        if let Some(scripts_dir) = find_scripts_dir(&openocd_bin) {
            cmd.arg("-s").arg(scripts_dir);
        }

        cmd.arg("-f").arg(format!("interface/{}.cfg", interface))
            .arg("-f").arg(format!("target/{}.cfg", target))
            .arg("-c").arg(format!("tcl_port {}", tcl_port))
            .arg("-c").arg("gdb_port disabled")
            .arg("-c").arg("telnet_port disabled")
            .arg("-c").arg(format!("adapter speed {}", config.speed_khz))
            .arg("-c").arg(format!("transport select {}", transport));

        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped());

        tracing::info!("Spawning OpenOCD: {:?}", cmd);

        let child = cmd.spawn()
            .map_err(|e| DataVisError::Config(format!("Failed to spawn OpenOCD: {}", e)))?;

        let mut process = Self { child, tcl_port };

        // Wait for TCL port to become ready
        process.wait_for_ready()?;

        Ok(process)
    }

    /// Get the TCL port
    pub fn tcl_port(&self) -> u16 {
        self.tcl_port
    }

    /// Get the TCL socket address
    pub fn tcl_addr(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.tcl_port))
    }

    /// Connect a TCL client to this process
    pub fn connect_client(&self) -> Result<TclClient> {
        TclClient::connect(self.tcl_addr())
    }

    /// Wait for the TCL port to become ready
    fn wait_for_ready(&mut self) -> Result<()> {
        let addr = self.tcl_addr();
        let timeout = Duration::from_secs(10);
        let start = Instant::now();
        let poll_interval = Duration::from_millis(100);

        tracing::info!("Waiting for OpenOCD TCL port {} to become ready...", self.tcl_port);

        while start.elapsed() < timeout {
            // Check if process has exited
            if let Some(status) = self.child.try_wait()
                .map_err(|e| DataVisError::Config(format!("Failed to check OpenOCD process: {}", e)))?
            {
                // Read stderr for error message
                let stderr = self.child.stderr.as_mut()
                    .and_then(|s| {
                        let mut buf = String::new();
                        std::io::Read::read_to_string(s, &mut buf).ok()?;
                        Some(buf)
                    })
                    .unwrap_or_default();
                return Err(DataVisError::Config(
                    format!("OpenOCD exited with status {} before TCL port was ready. stderr: {}", status, stderr.trim())
                ));
            }

            // Try to connect
            if TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok() {
                tracing::info!("OpenOCD TCL port ready");
                return Ok(());
            }

            std::thread::sleep(poll_interval);
        }

        // Timeout - kill the process
        let _ = self.child.kill();
        Err(DataVisError::Config(format!(
            "Timed out waiting for OpenOCD TCL port {} to become ready", self.tcl_port
        )))
    }

    /// Gracefully shutdown OpenOCD
    pub fn shutdown(mut self) {
        // Try to send shutdown command via TCL
        if let Ok(mut client) = self.connect_client() {
            let _ = client.execute("shutdown");
            // Give it a moment to shut down
            std::thread::sleep(Duration::from_millis(500));
        }

        // Check if still running and kill if necessary
        match self.child.try_wait() {
            Ok(Some(_)) => {} // Already exited
            _ => {
                tracing::warn!("OpenOCD didn't shut down gracefully, killing process");
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }
    }
}

impl Drop for OpenOcdProcess {
    fn drop(&mut self) {
        // Kill the process if it's still running
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Find the OpenOCD binary
fn find_openocd_binary(config: &ProbeConfig) -> Result<String> {
    // 1. Config override
    if let Some(ref path) = config.openocd_path {
        if std::path::Path::new(path).exists() {
            return Ok(path.clone());
        }
        return Err(DataVisError::Config(format!("OpenOCD binary not found at configured path: {}", path)));
    }

    // 2. Bundled path
    if let Some(bundled) = find_bundled_openocd() {
        return Ok(bundled);
    }

    // 3. System PATH
    if which_openocd().is_some() {
        return Ok("openocd".to_string());
    }

    Err(DataVisError::Config(
        "OpenOCD not found. Install OpenOCD or specify the path in Settings.".to_string()
    ))
}

/// Find bundled OpenOCD binary
fn find_bundled_openocd() -> Option<String> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();

    // macOS: .app bundle layout, then portable layout
    #[cfg(target_os = "macos")]
    {
        let path = exe_dir.join("../Resources/openocd/bin/openocd");
        if path.exists() {
            return path.to_str().map(|s| s.to_string());
        }
        let path = exe_dir.join("openocd/bin/openocd");
        if path.exists() {
            return path.to_str().map(|s| s.to_string());
        }
    }

    // Windows: <exe_dir>/openocd/bin/openocd.exe
    #[cfg(target_os = "windows")]
    {
        let path = exe_dir.join("openocd/bin/openocd.exe");
        if path.exists() {
            return path.to_str().map(|s| s.to_string());
        }
    }

    // Linux: <exe_dir>/openocd/bin/openocd
    #[cfg(target_os = "linux")]
    {
        let path = exe_dir.join("openocd/bin/openocd");
        if path.exists() {
            return path.to_str().map(|s| s.to_string());
        }
    }

    None
}

/// Find scripts directory for bundled OpenOCD
fn find_scripts_dir(openocd_bin: &str) -> Option<String> {
    let bin_path = std::path::Path::new(openocd_bin);
    let parent = bin_path.parent()?.parent()?;

    // Standard layout: ../share/openocd/scripts
    let standard = parent.join("share/openocd/scripts");
    if standard.exists() {
        return standard.to_str().map(|s| s.to_string());
    }

    // xPack layout: ../openocd/scripts
    let xpack = parent.join("openocd/scripts");
    if xpack.exists() {
        return xpack.to_str().map(|s| s.to_string());
    }

    None
}

/// Check if openocd is available on system PATH
fn which_openocd() -> Option<String> {
    Command::new("which")
        .arg("openocd")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Find a free TCP port
fn find_free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| DataVisError::Config(format!("Failed to find free port: {}", e)))?;
    let port = listener.local_addr()
        .map_err(|e| DataVisError::Config(format!("Failed to get local addr: {}", e)))?
        .port();
    Ok(port)
}
