# Testing Guide

This document describes the testing strategy, architecture, and guidelines for datavis-rs.

## Table of Contents

- [Overview](#overview)
- [Test Organization](#test-organization)
- [Running Tests](#running-tests)
- [Coverage Targets](#coverage-targets)
- [Writing Tests](#writing-tests)
- [Property-Based Testing](#property-based-testing)
- [Integration Testing](#integration-testing)
- [CI/CD](#cicd)

## Overview

The project maintains 310+ tests with comprehensive coverage across backend, frontend, and integration layers:

- **Unit Tests**: 255+ tests for individual components
- **Integration Tests**: 42+ tests for end-to-end workflows
- **Property-Based Tests**: 13+ generative tests for algorithmic invariants

### Coverage Metrics

| Component | Target | Status |
|-----------|--------|--------|
| Backend Worker | 80%+ | ✅ |
| ReadManager | 85%+ | ✅ |
| TypeTable | 75%+ | ✅ |
| DwarfParser | 60%+ | ✅ |
| Frontend State | 70%+ | ✅ |
| UI Panes | 40%+ | ✅ |
| **Overall** | **70%+** | ✅ |

## Test Organization

### Unit Tests

Located inline with source code using `#[cfg(test)] mod tests`:

```
src/
├── backend/
│   ├── worker.rs           # 11 tests (command processing, rate limiting)
│   ├── read_manager.rs     # 26 tests (bulk reads, property-based)
│   ├── type_table.rs       # 13 tests (type resolution, templates)
│   └── dwarf_parser.rs     # 19 tests (ELF parsing, C/C++ support)
├── frontend/
│   ├── topics.rs           # 12 tests (state management)
│   ├── plot.rs             # 26 tests (statistics, property-based)
│   └── panes/
│       ├── time_series.rs  # 15 tests (threshold lines, markers)
│       ├── watcher.rs      # 3 tests (state toggles)
│       └── exporter.rs     # 6 tests (format selection)
```

### Integration Tests

Located in `tests/` directory:

```
tests/
├── backend_lifecycle_integration.rs    # 8 tests
├── variable_workflow_integration.rs    # 8 tests
├── elf_parsing_integration.rs          # 13 tests
├── frontend_state_test.rs              # 9 tests
├── infrastructure_test.rs              # 4 tests
├── common/                             # Shared utilities
│   ├── mod.rs
│   ├── builders.rs                     # Test data builders
│   └── mock_helpers.rs                 # Mock construction
└── fixtures/                           # ELF test binaries
    ├── test_arm.elf
    ├── test_cpp.elf
    ├── test_struct.elf
    ├── test_pointer.elf
    └── test_complex_c.elf
```

## Running Tests

### Basic Commands

```bash
# All tests
cargo test --all-features

# Unit tests only
cargo test --lib --all-features

# Integration tests only
cargo test --test '*' --all-features

# Specific test
cargo test --lib read_manager::tests::test_bulk_reads

# With output
cargo test --all-features -- --nocapture
```

### Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate HTML report
cargo tarpaulin --all-features --workspace --out Html

# Generate Cobertura XML for CI
cargo tarpaulin --all-features --workspace --out Xml

# Open report
open tarpaulin-report.html
```

### Mock Probe Testing

Tests use the `mock-probe` feature for hardware-independent testing:

```bash
# Run backend tests with mock probe
cargo test --lib backend::worker --features mock-probe

# Run integration tests
cargo test --test backend_lifecycle_integration --features mock-probe
```

## Coverage Targets

### Critical Paths (80%+ required)

- **Backend Worker** (`src/backend/worker.rs`): Main polling loop, command processing
- **ReadManager** (`src/backend/read_manager.rs`): Bulk read optimization, memory operations
- **Type System** (`src/backend/type_table.rs`, `src/backend/dwarf_parser.rs`): Type resolution, DWARF parsing

### High Priority (70%+)

- **Frontend State** (`src/frontend/state.rs`, `src/frontend/topics.rs`): State management, data freshness
- **Dialogs** (`src/frontend/dialogs/*`): Validation logic, settings conversion

### Medium Priority (40-60%)

- **UI Panes** (`src/frontend/panes/*`): State logic (non-rendering)
- **Plot Utilities** (`src/frontend/plot.rs`): Statistics, transformations

## Writing Tests

### Unit Test Template

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_name() {
        // Arrange
        let component = Component::new();

        // Act
        let result = component.do_something();

        // Assert
        assert_eq!(result, expected);
    }
}
```

### Best Practices

1. **Test one thing per test**: Each test should verify a single behavior
2. **Use descriptive names**: `test_bulk_reads_cover_all_variables` > `test_reads`
3. **Arrange-Act-Assert**: Clear structure for readability
4. **Avoid sleeps**: Use deterministic mocks instead of timing dependencies
5. **Clean up resources**: Ensure tests are independent and don't leak state

### Testing UI Components

UI pane tests focus on **state logic** without rendering:

```rust
#[test]
fn test_time_series_state_default() {
    let state = TimeSeriesState::default();

    assert!(!state.advanced_mode);
    assert!(state.threshold_lines.is_empty());
    assert!(state.decimation_cache.is_empty());
}

#[test]
fn test_threshold_line_management() {
    let mut state = TimeSeriesState::default();

    state.threshold_lines.push(ThresholdLine::new(1, 10.0, "Low", [0, 255, 0, 255]));
    assert_eq!(state.threshold_lines.len(), 1);

    state.threshold_lines.retain(|l| l.id != 1);
    assert_eq!(state.threshold_lines.len(), 0);
}
```

## Property-Based Testing

Using `proptest` for generative testing of algorithmic invariants.

### ReadManager Properties

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_bulk_reads_cover_all_variables(
        addresses in prop::collection::vec(0x2000_0000u64..0x2000_0100, 1..50)
    ) {
        let manager = ReadManager::new(64);
        let vars: Vec<_> = addresses.iter().enumerate()
            .map(|(i, &addr)| create_test_variable(&format!("var{}", i), addr, VariableType::U32))
            .collect();

        let regions = manager.plan_reads(&vars);

        // Property: Every variable must be in exactly one region
        for (i, _) in vars.iter().enumerate() {
            let count = regions.iter()
                .filter(|r| r.variable_indices.contains(&i))
                .count();
            prop_assert_eq!(count, 1);
        }
    }
}
```

### Statistics Properties

```rust
proptest! {
    #[test]
    fn test_statistics_min_max_invariant(
        values in prop::collection::vec(any::<f64>(), 1..100)
    ) {
        let stats = PlotStatistics::from_data(&data);

        if stats.is_valid() {
            // Property: min <= max (for valid stats)
            prop_assert!(stats.min <= stats.max || (stats.min.is_nan() && stats.max.is_nan()));
        }
    }
}
```

## Integration Testing

### Backend Lifecycle

Test complete workflows from creation to shutdown:

```rust
#[test]
#[cfg(feature = "mock-probe")]
fn test_backend_connect_with_mock_probe() {
    let config = AppConfig::default();
    let (backend, frontend) = SwdBackend::new(config.clone());

    let handle = thread::spawn(move || backend.run());

    frontend.use_mock_probe(true);
    frontend.connect(None, "MockTarget".to_string(), config.probe);

    thread::sleep(Duration::from_millis(100));

    let messages = frontend.drain();
    let has_connection = messages.iter().any(|msg|
        matches!(msg, BackendMessage::ConnectionStatus(_))
    );
    assert!(has_connection);

    frontend.shutdown();
    handle.join().unwrap();
}
```

### ELF Parsing

Test with real compiled ELF binaries:

```rust
const TEST_ARM_ELF: &[u8] = include_bytes!("fixtures/test_arm.elf");

#[test]
fn test_parse_basic_c_elf() {
    let result = DwarfParser::parse_bytes(TEST_ARM_ELF).expect("Should parse");

    let var_names: Vec<_> = result.symbols.iter().map(|s| s.name.as_str()).collect();

    assert!(var_names.contains(&"global_counter"));
    assert!(var_names.contains(&"sensor_data"));
    assert!(!result.type_table.is_empty());
}
```

### Test Fixtures

ELF fixtures are built at compile time from C/C++ sources:

```bash
cd tests/fixtures
make  # Builds all test ELF files with DWARF debug info
```

## CI/CD

### GitHub Actions Workflow

Located at `.github/workflows/test.yml`:

- **Test Matrix**: Ubuntu, Windows, macOS × Rust stable/beta
- **Coverage**: Tarpaulin on Ubuntu, uploaded to Codecov
- **Linting**: Clippy with `-D warnings`
- **Formatting**: rustfmt check

### Running Locally

Simulate CI checks:

```bash
# All tests
cargo test --all-features

# Clippy (same as CI)
cargo clippy --all-features --all-targets -- -D warnings

# Formatting (same as CI)
cargo fmt --all -- --check

# Coverage
cargo tarpaulin --all-features --workspace --timeout 300
```

### Coverage Enforcement

CI fails if:
- Any test fails
- Clippy reports warnings
- Code is not formatted

Coverage is reported but doesn't block CI (informational only).

## Troubleshooting

### Common Issues

**Property-based tests fail intermittently:**
```bash
# Proptest saves failures to proptest-regressions/
# Re-run with the specific seed to reproduce:
cargo test test_name
```

**Integration tests hang:**
```bash
# Check for deadlocks in backend threads
# Increase timeouts or add logging
cargo test --test backend_lifecycle_integration -- --nocapture
```

**Mock probe not available:**
```bash
# Ensure mock-probe feature is enabled
cargo test --features mock-probe
```

### Performance

Tests should complete in < 2 minutes:

```bash
cargo test --all-features --release  # Faster execution
```

## Contributing

When adding new features:

1. **Write tests first** (TDD recommended for complex logic)
2. **Add property-based tests** for algorithms with invariants
3. **Update integration tests** if changing public APIs
4. **Run full test suite** before submitting PR
5. **Check coverage** doesn't decrease significantly

### Test Coverage Goals

New code should maintain or improve coverage:

- Critical paths: 80%+
- Business logic: 70%+
- UI state: 60%+

Use `cargo tarpaulin` to verify coverage locally before pushing.

---

For questions or issues with tests, open an issue or discussion on GitHub.
