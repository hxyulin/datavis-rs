# DataVis-RS

Real-time data visualization for embedded systems using Serial Wire Debug (SWD).

![Screenshot](docs/screenshots/main.png)

## Features

- **Live Variable Monitoring** — Read memory via SWD at up to 1000+ Hz with batch polling
- **Real-Time Plotting** — Interactive graphs with autoscale, axis locking, and statistics
- **ELF/DWARF Support** — Load symbols from firmware files with automatic type detection
- **Rhai Scripting** — Transform values with custom converters (filters, derivatives, unit conversion)
- **Project Management** — Save/load configurations as `.datavisproj` files

## Installation

### Pre-Built Downloads

Download the latest release for your platform from the [Releases](https://github.com/hxyulin/datavis-rs/releases) page:

| Platform | Installer | Portable |
|----------|-----------|----------|
| Windows | `.msi` / `.exe` (NSIS) | `.exe` |
| macOS (Apple Silicon) | `.dmg` | Binary |
| macOS (Intel) | `.dmg` | Binary |
| Linux | `.deb` / `.AppImage` | Binary |

### Build from Source

```bash
git clone https://github.com/hxyulin/datavis-rs.git
cd datavis-rs
cargo build --release
```

The binary will be at `target/release/datavis-rs`.

## Quick Start

1. Connect your debug probe (ST-Link, J-Link, CMSIS-DAP, etc.)
2. Launch DataVis-RS
3. Select your target chip (e.g., `STM32F407VGTx`)
4. Click **Connect**
5. Load an ELF file or add variables manually
6. Click **Start** to begin data collection

## Supported Debug Probes

Any probe supported by [probe-rs](https://probe.rs/):

- ST-Link (V2, V2-1, V3)
- J-Link (all variants)
- CMSIS-DAP compatible probes
- ESP-PROG
- Raspberry Pi Debug Probe / Picoprobe

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Space` | Start/Stop collection |
| `P` | Pause/Resume |
| `C` | Clear all data |
| `R` | Reset view |
| `Escape` | Close dialogs |

## Screenshots

### Variables Panel
![Variables](docs/screenshots/variables.png)

### Visualizer
![Visualizer](docs/screenshots/visualizer.png)

### Settings
![Settings](docs/screenshots/settings.png)

## Configuration

Application data is stored in:
- **Linux**: `~/.local/share/dev.hxyulin.datavis-rs/`
- **macOS**: `~/Library/Application Support/dev.hxyulin.datavis-rs/`
- **Windows**: `%APPDATA%\dev.hxyulin.datavis-rs\`

## Development

### Building

```bash
cargo build --release
```

### Testing with Mock Probe

```bash
cargo run --release --features mock-probe
```

### Running Tests

The project has comprehensive test coverage with 310+ tests:

```bash
# Run all tests
cargo test --all-features

# Run only unit tests
cargo test --lib --all-features

# Run only integration tests
cargo test --test '*' --all-features

# Run with coverage
cargo install cargo-tarpaulin
cargo tarpaulin --all-features --workspace --out Html
```

**Test Categories:**
- **Unit Tests** (255+): Backend worker, ReadManager, type system, DWARF parser, frontend state, UI components
- **Integration Tests** (42+): Backend lifecycle, variable workflows, ELF parsing, state management
- **Property-Based Tests** (13+): Bulk read optimization, statistics calculation, type parsing invariants

**Coverage Targets:**
- Overall: 70%+
- Critical paths (backend worker, parsers): 80%+
- Frontend business logic: 60-70%

**Continuous Integration:**

Tests run automatically on push/PR via GitHub Actions:
- Multi-platform testing (Ubuntu, Windows, macOS)
- Multiple Rust versions (stable, beta)
- Code coverage reporting via Codecov
- Clippy linting and rustfmt checks

## License

MIT License — See [LICENSE](LICENSE) for details.
