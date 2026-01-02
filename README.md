# DataVis-RS: Real-Time SWD Data Visualizer

A powerful real-time data visualization tool for embedded systems using Serial Wire Debug (SWD). Monitor and plot variables directly from your microcontroller's memory with a responsive, never-freezing UI.

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)

## Features

### Real-Time Variable Monitoring
- **Live memory reading** via SWD at configurable poll rates (up to 1000+ Hz)
- **Multiple variable types**: u8, u16, u32, u64, i8, i16, i32, i64, f32, f64, bool
- **Batch polling** for efficient multi-variable observation
- **Non-blocking UI** - the interface stays responsive even during slow SWD operations

### Advanced Plotting
- **Real-time graphs** using egui_plot with smooth rendering
- **Autoscale X/Y axes** with manual override options
- **Axis locking** to prevent accidental zoom/pan
- **Configurable time window** with maximum limit (0.1s to 300s)
- **Multiple variables** on the same plot with distinct colors
- **Statistics display**: min, max, average for each variable

### Rhai Scripting Engine
Transform raw memory values with custom converter scripts:

```rhai
// ADC to voltage (12-bit, 3.3V reference)
fn convert(raw) {
    raw * 3.3 / 4095.0
}
```

Built-in functions for signal processing:
- `derivative(value)` - Compute rate of change
- `integrate(value)` - Accumulate over time
- `smooth(value, alpha)` - Exponential smoothing
- `lowpass(value, cutoff_hz)` - First-order lowpass filter
- `deadband(value, center, width)` - Apply deadband/hysteresis

### ELF/DWARF Symbol Support
- **Load symbols** from ELF/AXF files
- **Type-aware** variable selection with struct/array expansion
- **Automatic address resolution** - no manual address entry needed
- **DWARF debug info parsing** for complete type information

### Project Management
- **Save/load projects** as `.datavisproj` files
- **Persistent configuration** across sessions
- **Export data** to CSV, JSON, or binary formats

### Double-Click Variable Editing
- **Write values** back to target memory by double-clicking displayed values
- **Supported types**: All primitive types (not Raw)
- **Safety**: Only works when probe is connected and variable has no converter

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/hxyulin/datavis-rs.git
cd datavis-rs

# Build in release mode
cargo build --release

# Run the application
cargo run --release
```

### With Mock Probe (for testing)

```bash
cargo run --release --features mock-probe
```

## Quick Start

1. **Connect your debug probe** (ST-Link, J-Link, CMSIS-DAP, etc.)

2. **Launch DataVis-RS**:
   ```bash
   cargo run --release
   ```

3. **Select your target chip** (e.g., `STM32F407VGTx`)

4. **Click Connect** to attach to your target

5. **Add variables** using one of these methods:
   - Load an ELF file and select variables from the symbol browser
   - Manually enter address, type, and name

6. **Click Start** to begin data collection

7. **Use the Visualizer page** to see real-time plots

## Usage

### Variables Page

Add and manage the variables you want to observe:

| Field | Description |
|-------|-------------|
| Name | Display name for the variable |
| Address | Memory address (hex, e.g., `0x20000000`) |
| Type | Data type (u32, f32, etc.) |
| Unit | Optional unit label (V, mA, °C) |
| Converter | Optional Rhai script for value transformation |

### Visualizer Page

Real-time plotting with interactive controls:

| Control | Function |
|---------|----------|
| Auto X | Toggle automatic X-axis following |
| Auto Y | Toggle automatic Y-axis scaling |
| 🔒 X/Y | Lock axis to prevent zoom/pan |
| Time Window | Adjust visible time range |
| Reset View | Return to default view settings |

**Mouse Controls:**
- **Scroll** - Zoom in/out
- **Drag** - Pan the view
- **Double-click value** - Edit variable (when connected)

### Settings Page

Configure application behavior:

- **Probe Settings**: Speed, protocol (SWD/JTAG), connect-under-reset
- **Collection Settings**: Poll rate, timeout, buffer size
- **UI Settings**: Dark mode, grid, line width, font scale
- **Data Persistence**: Automatic logging to file

## Converter Script Examples

### Temperature from NTC Thermistor
```rhai
fn convert(raw) {
    let resistance = 10000.0 * raw / (4095.0 - raw);
    let temp_k = 1.0 / (1.0/298.15 + (1.0/3950.0) * log(resistance / 10000.0));
    temp_k - 273.15  // Celsius
}
```

### Fixed-Point Q15 to Float
```rhai
fn convert(raw) {
    if raw > 32767.0 {
        (raw - 65536.0) / 32768.0
    } else {
        raw / 32768.0
    }
}
```

### Compute Velocity from Position
```rhai
// Uses built-in derivative with automatic dt handling
derivative(value)
```

### Apply Low-Pass Filter
```rhai
// Remove noise above 10 Hz
lowpass(value, 10.0)
```

### Simple Expression (no function needed)
```rhai
value * 2.0 + 10.0
```

## Supported Debug Probes

Any probe supported by [probe-rs](https://probe.rs/):

- **ST-Link** (V2, V2-1, V3)
- **J-Link** (all variants)
- **CMSIS-DAP** compatible probes
- **ESP-PROG**
- **Raspberry Pi Pico** (picoprobe/debugprobe)
- **Raspberry Pi Debug Probe**

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Space` | Start/Stop collection |
| `P` | Pause/Resume |
| `C` | Clear all data |
| `R` | Reset view |
| `Escape` | Close dialogs |

## Configuration

### App Data Location

Application state is stored in the platform-appropriate data directory:

- **Linux**: `~/.local/share/dev.hxyulin.datavis-rs/`
- **macOS**: `~/Library/Application Support/dev.hxyulin.datavis-rs/`
- **Windows**: `%APPDATA%\dev.hxyulin.datavis-rs\`

The app automatically saves:
- Recent projects list
- Last opened project (restored on startup)
- Last used target chip and probe
- UI preferences (dark mode, font scale)

### Project Files

Projects are saved as `.datavisproj` files (JSON format) and contain:

```json
{
  "version": 1,
  "name": "My Project",
  "config": {
    "probe": {
      "target_chip": "STM32F407VGTx",
      "speed_khz": 4000,
      "protocol": "Swd"
    },
    "collection": {
      "poll_rate_hz": 100,
      "timeout_ms": 100,
      "max_data_points": 10000
    },
    "ui": {
      "time_window_seconds": 10.0,
      "show_grid": true,
      "line_width": 1.5,
      "auto_scale_y": true,
      "auto_scale_x": true,
      "max_time_window": 300.0
    },
    "variables": []
  },
  "binary_path": "/path/to/firmware.elf"
}
```

Projects are auto-saved on exit and restored on the next launch.

## Development

### Running Tests

```bash
# Run all tests
cargo test

# Run with mock probe feature
cargo test --features mock-probe

# Run with debug logging
RUST_LOG=debug cargo run
```

### Project Structure

```
src/
├── main.rs           # Application entry point
├── lib.rs            # Library exports
├── types.rs          # Core data types (Variable, DataPoint, etc.)
├── error.rs          # Error handling
├── backend/          # SWD polling thread
│   ├── mod.rs        # Backend commands and messages
│   ├── worker.rs     # Main polling loop
│   ├── probe.rs      # probe-rs interface
│   ├── mock_probe.rs # Mock probe for testing
│   ├── elf_parser.rs # ELF symbol parsing
│   └── dwarf_parser.rs # DWARF type info parsing
├── frontend/         # egui UI
│   ├── mod.rs        # Main application state
│   ├── plot.rs       # Plotting with egui_plot
│   ├── panels.rs     # Reusable UI panels
│   └── widgets.rs    # Custom widgets
├── config/           # Configuration management
│   ├── mod.rs        # Config types and project files
│   └── settings.rs   # Runtime settings
└── scripting/        # Rhai scripting
    ├── mod.rs        # Script cache and builtins
    └── engine.rs     # Script engine with custom functions
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| `eframe` / `egui` | UI framework |
| `egui_plot` | Real-time plotting |
| `probe-rs` | Debug probe communication |
| `rhai` | Scripting engine |
| `crossbeam-channel` | Thread-safe messaging |
| `serde` / `toml` | Configuration serialization |
| `gimli` / `object` | ELF/DWARF parsing |
| `tracing` | Logging |

## Troubleshooting

### Probe not detected
- Ensure the probe is connected and drivers are installed
- Try running with elevated permissions (sudo on Linux)
- Check that no other software is using the probe

### High CPU usage
- Reduce the poll rate in settings
- Reduce the time window or max data points
- Disable variables you're not actively monitoring

### Slow or laggy UI
- The UI should never freeze - if it does, please report a bug
- High poll rates with many variables may increase CPU usage

## License

MIT License - See [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## Acknowledgments

- [probe-rs](https://probe.rs/) - The excellent Rust embedded debugging toolkit
- [egui](https://github.com/emilk/egui) - Immediate mode GUI library
- [Rhai](https://rhai.rs/) - Embedded scripting language