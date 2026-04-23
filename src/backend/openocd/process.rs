use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::{OpenOcdMode, ProbeConfig};
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

        // Determine interface config. Priority:
        //   1. Explicit user override (normalized so "interface/cmsis-dap.cfg",
        //      "cmsis-dap.cfg" and "cmsis-dap" all resolve the same way).
        //   2. Auto-detect from the first probe probe-rs can see.
        //   3. Fall back to "stlink" (most common default) with a warning.
        let interface = config.openocd_interface.clone()
            .map(|s| normalize_script_name(&s, "interface"))
            .or_else(detect_interface_from_probes)
            .unwrap_or_else(|| {
                tracing::warn!(
                    "Could not auto-detect OpenOCD interface; defaulting to 'stlink'. \
                     If your probe is not an ST-Link, set 'Interface Override' in \
                     Connection Settings (e.g. 'cmsis-dap', 'jlink')."
                );
                "stlink".to_string()
            });
        tracing::info!("OpenOCD interface: {}", interface);

        let target = config.openocd_target.clone()
            .map(|s| normalize_script_name(&s, "target"))
            .or_else(|| chip_map::chip_to_target(&config.target_chip).map(|s| s.to_string()))
            .ok_or_else(|| DataVisError::Config(
                format!("Could not determine OpenOCD target config for chip '{}'. Please specify in Settings.", config.target_chip)
            ))?;

        // Determine transport. stlink.cfg auto-selects hla_swd on load and
        // rejects later `transport select` calls, so we issue ours BEFORE the
        // interface config and fall back to hla_swd when the interface requires
        // it.
        let transport = match (interface.as_str(), config.protocol) {
            ("stlink", crate::config::ProbeProtocol::Swd) => "hla_swd",
            ("stlink", crate::config::ProbeProtocol::Jtag) => "hla_jtag",
            (_, crate::config::ProbeProtocol::Swd) => "swd",
            (_, crate::config::ProbeProtocol::Jtag) => "jtag",
        };

        let mut cmd = Command::new(&openocd_bin);

        // Add scripts directory if using bundled OpenOCD
        if let Some(scripts_dir) = find_scripts_dir(&openocd_bin) {
            cmd.arg("-s").arg(scripts_dir);
        }

        cmd.arg("-f").arg(format!("interface/{}.cfg", interface))
            .arg("-c").arg(format!("transport select {}", transport))
            .arg("-c").arg(format!("adapter speed {}", config.speed_khz))
            .arg("-f").arg(format!("target/{}.cfg", target))
            .arg("-c").arg(format!("tcl_port {}", tcl_port))
            .arg("-c").arg("gdb_port disabled")
            .arg("-c").arg("telnet_port disabled");

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
                let stderr = stderr.trim();
                let hint = if stderr.contains("open failed") {
                    " — the probe either isn't connected, is in use by another tool, \
                     or the configured interface doesn't match the probe type \
                     (try setting 'Interface Override' in Connection Settings)"
                } else {
                    ""
                };
                return Err(DataVisError::Config(format!(
                    "OpenOCD exited with status {} before TCL port was ready{}. stderr: {}",
                    status, hint, stderr
                )));
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

/// Ask probe-rs for the currently-attached probes and, if one is present,
/// map its probe-type string to an OpenOCD interface config name.
///
/// This gives users a reasonable default when they haven't filled in the
/// Interface Override field, so that e.g. a CMSIS-DAP probe doesn't try to
/// load `interface/stlink.cfg` and fail with the cryptic "open failed".
fn detect_interface_from_probes() -> Option<String> {
    let probes = crate::backend::ProbeBackend::list_probes();
    let probe = probes.first()?;
    let detected = super::chip_map::probe_type_to_interface(&probe.probe_type)?;
    tracing::info!(
        "Auto-detected OpenOCD interface '{}' from probe type '{}'",
        detected,
        probe.probe_type
    );
    Some(detected.to_string())
}

/// Normalize a user-entered OpenOCD script name by stripping whitespace, a
/// leading "{prefix}/" directory, and a trailing ".cfg" extension so that
/// inputs like "interface/cmsis-dap.cfg", "cmsis-dap.cfg", and "cmsis-dap"
/// all produce "cmsis-dap".
fn normalize_script_name(input: &str, prefix: &str) -> String {
    let trimmed = input.trim();
    let without_dir = trimmed
        .strip_prefix(&format!("{prefix}/"))
        .unwrap_or(trimmed);
    without_dir
        .strip_suffix(".cfg")
        .unwrap_or(without_dir)
        .to_string()
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

/// An active OpenOCD TCL endpoint — either one we spawned and own, or an
/// external server we just connected to.
pub enum OpenOcdConnection {
    /// We started the `openocd` process and are responsible for tearing it down.
    Spawned(OpenOcdProcess),
    /// A pre-existing TCL server we have no process-level control over.
    External { addr: SocketAddr },
}

impl OpenOcdConnection {
    /// Establish a connection according to `config.openocd_mode`.
    pub fn connect(config: &ProbeConfig) -> Result<Self> {
        match &config.openocd_mode {
            OpenOcdMode::Spawn => Ok(Self::Spawned(OpenOcdProcess::spawn(config)?)),
            OpenOcdMode::External { host, port } => {
                let addr = resolve_addr(host, *port)?;
                // Probe once with a short timeout so we fail fast with a
                // descriptive error if nothing is listening, instead of
                // hanging on the first TCL command.
                TcpStream::connect_timeout(&addr, Duration::from_secs(2))
                    .map_err(|e| DataVisError::Config(format!(
                        "Failed to connect to existing OpenOCD at {}: {}", addr, e
                    )))?;
                tracing::info!("Connected to external OpenOCD TCL server at {}", addr);
                Ok(Self::External { addr })
            }
        }
    }

    /// Address of the TCL server.
    pub fn tcl_addr(&self) -> SocketAddr {
        match self {
            Self::Spawned(p) => p.tcl_addr(),
            Self::External { addr } => *addr,
        }
    }

    /// Open a fresh TCL client against this connection.
    pub fn connect_client(&self) -> Result<TclClient> {
        TclClient::connect(self.tcl_addr())
    }

    /// Terminate the underlying OpenOCD if we own it; no-op for external.
    pub fn shutdown(self) {
        match self {
            Self::Spawned(p) => p.shutdown(),
            Self::External { .. } => {
                // We don't own the external OpenOCD — do NOT send `shutdown`.
                tracing::info!("Disconnecting from external OpenOCD (leaving it running)");
            }
        }
    }
}

/// Resolve a host + port pair to a concrete `SocketAddr`, picking the first
/// address returned by DNS/the OS. Accepts IPv4, IPv6, and hostnames.
fn resolve_addr(host: &str, port: u16) -> Result<SocketAddr> {
    (host, port)
        .to_socket_addrs()
        .map_err(|e| DataVisError::Config(format!("Invalid OpenOCD host '{}:{}': {}", host, port, e)))?
        .next()
        .ok_or_else(|| DataVisError::Config(format!("Host '{}' did not resolve to any address", host)))
}

#[cfg(test)]
mod tests {
    use super::normalize_script_name;

    #[test]
    fn normalize_strips_prefix_and_extension() {
        assert_eq!(normalize_script_name("cmsis-dap", "interface"), "cmsis-dap");
        assert_eq!(normalize_script_name("cmsis-dap.cfg", "interface"), "cmsis-dap");
        assert_eq!(
            normalize_script_name("interface/cmsis-dap.cfg", "interface"),
            "cmsis-dap"
        );
        assert_eq!(
            normalize_script_name("  interface/cmsis-dap.cfg  ", "interface"),
            "cmsis-dap"
        );
        assert_eq!(normalize_script_name("stm32f4x", "target"), "stm32f4x");
        assert_eq!(
            normalize_script_name("target/stm32f4x.cfg", "target"),
            "stm32f4x"
        );
    }

    #[test]
    fn normalize_does_not_strip_other_prefixes() {
        // A "target/" prefix should survive when the context is "interface".
        assert_eq!(
            normalize_script_name("target/stm32f4x.cfg", "interface"),
            "target/stm32f4x"
        );
    }
}
