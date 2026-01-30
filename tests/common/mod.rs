//! Common test utilities and helpers

#![allow(dead_code)] // Test utilities may not all be used in every test file

pub mod builders;
pub mod mock_helpers;

use std::time::Duration;

/// Create a test timeout duration
pub fn test_timeout() -> Duration {
    Duration::from_millis(100)
}

/// Assert two floats are approximately equal
pub fn assert_float_eq(a: f64, b: f64, epsilon: f64) {
    assert!(
        (a - b).abs() < epsilon,
        "Expected {} to be approximately equal to {} (epsilon: {})",
        a,
        b,
        epsilon
    );
}
