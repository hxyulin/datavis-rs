//! Pre-built fault scenario helpers for integration tests

#[cfg(feature = "mock-probe")]
use datavis_rs::backend::{
    FaultConfig, GlobalFaults, LatencyProfile, MockDataPattern, MockProbeBackend, VariableFaults,
};

/// Create a mock probe with a 10% read failure rate
#[cfg(feature = "mock-probe")]
pub fn flaky_probe() -> MockProbeBackend {
    MockProbeBackend::new()
        .with_default_pattern(MockDataPattern::Constant(42.0))
        .with_read_failure_rate(0.1)
}

/// Create a mock probe with normally distributed latency jitter
#[cfg(feature = "mock-probe")]
pub fn jittery_probe() -> MockProbeBackend {
    MockProbeBackend::new()
        .with_default_pattern(MockDataPattern::Constant(42.0))
        .with_latency_profile(LatencyProfile::Normal {
            mean_us: 200,
            stddev_us: 50,
        })
}

/// Create a mock probe that randomly disconnects
#[cfg(feature = "mock-probe")]
pub fn unstable_probe(disconnect_rate: f64) -> MockProbeBackend {
    let config = FaultConfig {
        global: GlobalFaults {
            disconnect_rate,
            ..Default::default()
        },
        ..Default::default()
    };
    MockProbeBackend::new()
        .with_default_pattern(MockDataPattern::Constant(42.0))
        .with_fault_config(config)
}

/// Create a mock probe where one specific variable always times out
#[cfg(feature = "mock-probe")]
pub fn partial_failure_probe(failing_var_id: u32) -> MockProbeBackend {
    MockProbeBackend::new()
        .with_default_pattern(MockDataPattern::Constant(42.0))
        .with_variable_fault(
            failing_var_id,
            VariableFaults {
                always_timeout: true,
                ..Default::default()
            },
        )
}

/// Create a mock probe with latency that increases over time
#[cfg(feature = "mock-probe")]
pub fn degrading_probe() -> MockProbeBackend {
    MockProbeBackend::new()
        .with_default_pattern(MockDataPattern::Constant(42.0))
        .with_latency_profile(LatencyProfile::Degrading {
            start_us: 50,
            increase_per_read_us: 10.0,
        })
}
