//! Integration tests for mock probe fault injection
//!
//! These tests validate that the fault injection framework correctly
//! simulates various error conditions and degraded performance.

mod common;

#[cfg(feature = "mock-probe")]
mod fault_tests {
    use datavis_rs::backend::{
        CorruptionConfig, FaultConfig, FaultErrorKind, GlobalFaults, LatencyProfile,
        MockDataPattern, MockProbeBackend, PeriodicFailure, VariableFaults,
    };
    use datavis_rs::types::{Variable, VariableType};
    use std::time::Instant;

    fn test_variable(id: u32) -> Variable {
        let mut var = Variable::new("test_var", 0x2000_0000 + (id as u64 * 4), VariableType::U32);
        var.id = id;
        var
    }

    #[test]
    fn test_read_failure_rate() {
        let mut probe = MockProbeBackend::new()
            .with_default_pattern(MockDataPattern::Constant(42.0))
            .with_read_failure_rate(0.5);
        probe.connect(None, "TestTarget").unwrap();

        let var = test_variable(1);
        let mut failures = 0;
        let total = 200;

        for _ in 0..total {
            if probe.read_variable(&var).is_err() {
                failures += 1;
            }
            // Reconnect if disconnected by fault
            if !probe.is_connected() {
                probe.connect(None, "TestTarget").unwrap();
            }
        }

        let failure_rate = failures as f64 / total as f64;
        // Allow wide tolerance since it's probabilistic
        assert!(
            failure_rate > 0.2 && failure_rate < 0.8,
            "Expected ~50% failure rate, got {:.1}% ({} failures out of {})",
            failure_rate * 100.0,
            failures,
            total
        );
    }

    #[test]
    fn test_always_timeout_variable() {
        let mut probe = MockProbeBackend::new()
            .with_default_pattern(MockDataPattern::Constant(42.0))
            .with_variable_fault(
                1,
                VariableFaults {
                    always_timeout: true,
                    ..Default::default()
                },
            );
        probe.connect(None, "TestTarget").unwrap();

        let failing_var = test_variable(1);
        let ok_var = test_variable(2);

        // The failing variable should always timeout
        for _ in 0..10 {
            let result = probe.read_variable(&failing_var);
            assert!(result.is_err(), "Variable 1 should always fail");
            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("Timeout") || err_msg.contains("timeout"),
                "Error should be a timeout, got: {}",
                err_msg
            );
        }

        // The other variable should succeed
        let result = probe.read_variable(&ok_var);
        assert!(result.is_ok(), "Variable 2 should succeed");
    }

    #[test]
    fn test_periodic_failures() {
        let config = FaultConfig {
            global: GlobalFaults {
                periodic_failure: Some(PeriodicFailure {
                    every_n_reads: 5,
                    failure_count: 1,
                    error_kind: FaultErrorKind::Timeout,
                }),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut probe = MockProbeBackend::new()
            .with_default_pattern(MockDataPattern::Constant(42.0))
            .with_fault_config(config);
        probe.connect(None, "TestTarget").unwrap();

        let var = test_variable(1);
        let mut results = Vec::new();

        for _ in 0..20 {
            results.push(probe.read_variable(&var).is_ok());
        }

        // Every 5th read (at positions 1-based: 1, 6, 11, 16) should fail
        // read_counter goes 1,2,3,4,5,6,...
        // cycle_pos = counter % 5 => 1,2,3,4,0,1,2,3,4,0,...
        // Fails when cycle_pos > 0 && cycle_pos <= 1 => cycle_pos == 1
        // That's reads 1, 6, 11, 16 (0-indexed: 0, 5, 10, 15)
        let failure_count = results.iter().filter(|&&ok| !ok).count();
        assert!(
            failure_count >= 3 && failure_count <= 5,
            "Expected ~4 periodic failures in 20 reads, got {}",
            failure_count
        );
    }

    #[test]
    fn test_corruption_changes_values() {
        let config = FaultConfig {
            global: GlobalFaults {
                corruption: Some(CorruptionConfig {
                    rate: 1.0, // Always corrupt
                    magnitude: 1000.0,
                }),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut probe = MockProbeBackend::new()
            .with_default_pattern(MockDataPattern::Constant(42.0))
            .with_fault_config(config);
        probe.connect(None, "TestTarget").unwrap();

        let var = test_variable(1);

        let mut different_count = 0;
        let total = 50;

        for _ in 0..total {
            let value = probe.read_variable(&var).unwrap();
            if (value - 42.0).abs() > 0.001 {
                different_count += 1;
            }
        }

        // With 100% corruption rate and magnitude 1000, virtually all values should differ
        assert!(
            different_count > total / 2,
            "Expected most values to be corrupted, but only {}/{} differed from expected",
            different_count,
            total
        );
    }

    #[test]
    fn test_latency_spike_profile() {
        let mut probe = MockProbeBackend::new()
            .with_default_pattern(MockDataPattern::Constant(42.0))
            .with_latency_profile(LatencyProfile::WithSpikes {
                base_us: 50,
                spike_us: 5000, // 5ms spikes
                spike_probability: 0.2,
            });
        probe.connect(None, "TestTarget").unwrap();

        let var = test_variable(1);
        let mut fast_count = 0;
        let mut slow_count = 0;
        let total = 100;

        for _ in 0..total {
            let start = Instant::now();
            let _ = probe.read_variable(&var);
            let elapsed = start.elapsed();

            if elapsed.as_micros() > 2000 {
                slow_count += 1;
            } else {
                fast_count += 1;
            }
        }

        // Most should be fast, some should be slow
        assert!(
            fast_count > 50,
            "Expected most reads to be fast, got {} fast out of {}",
            fast_count,
            total
        );
        assert!(
            slow_count > 5,
            "Expected some slow reads (spikes), got {} slow out of {}",
            slow_count,
            total
        );
    }

    #[test]
    fn test_no_faults_by_default() {
        let mut probe = MockProbeBackend::new()
            .with_default_pattern(MockDataPattern::Constant(42.0));
        probe.connect(None, "TestTarget").unwrap();

        let var = test_variable(1);

        // All reads should succeed when no fault config is set
        for i in 0..100 {
            let result = probe.read_variable(&var);
            assert!(
                result.is_ok(),
                "Read {} should succeed with no faults, got: {:?}",
                i,
                result.err()
            );
        }
    }
}
