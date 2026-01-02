//! Rhai Scripting Engine for Variable Converters
//!
//! This module provides a scripting engine based on Rhai that allows users
//! to define custom conversion functions for transforming raw values read
//! from memory into meaningful data.
//!
//! ## Dynamic Variables
//!
//! The following dynamic variables and functions are available in scripts:
//!
//! - `value` / `raw` - The current raw value being converted
//! - `time()` - Time in seconds since collection started (pauses when paused)
//! - `dt()` - Delta time since last sample in seconds
//! - `prev()` - Previous converted value (NaN if not available)
//! - `prev_raw()` - Previous raw value (NaN if not available)
//! - `has_prev()` - Returns true if previous value is available
//!
//! ## Transformer Functions
//!
//! These functions help with common data processing tasks:
//!
//! - `derivative(value)` - Compute rate of change (uses prev() and dt() automatically)
//! - `derivative(current, previous, dt)` - Explicit derivative calculation
//! - `integrate(value)` - Accumulate value over time (uses prev() and dt())
//! - `integrate(current, accumulated, dt)` - Explicit integration
//! - `smooth(value, alpha)` - Exponential smoothing/EWMA (alpha 0-1, higher = smoother)
//! - `smooth(current, previous, alpha)` - Explicit smoothing
//! - `lowpass(value, cutoff_hz)` - First-order lowpass filter
//! - `lowpass(current, previous, cutoff_hz, dt)` - Explicit lowpass
//! - `highpass(current, prev_input, prev_output, cutoff_hz, dt)` - First-order highpass
//! - `deadband(value, center, width)` - Apply deadband/hysteresis
//! - `rate_limit(value, max_rate)` - Limit rate of change (uses prev() and dt())
//! - `rate_limit(current, previous, max_rate, dt)` - Explicit rate limiting
//! - `hysteresis(input, prev_output, low_thresh, high_thresh, low_val, high_val)` - Hysteresis
//!
//! ## Example Scripts
//!
//! Converting ADC counts to voltage:
//! ```rhai
//! // ADC is 12-bit (0-4095), reference voltage is 3.3V
//! fn convert(raw) {
//!     raw * 3.3 / 4095.0
//! }
//! ```
//!
//! Converting raw temperature sensor value:
//! ```rhai
//! // Temperature sensor with offset and scale
//! fn convert(raw) {
//!     (raw - 500.0) / 10.0  // Result in Celsius
//! }
//! ```
//!
//! Computing velocity from position:
//! ```rhai
//! // Derivative of position gives velocity
//! derivative(value)
//! ```
//!
//! Smoothing noisy sensor data:
//! ```rhai
//! // Apply exponential smoothing with alpha=0.8
//! smooth(value, 0.8)
//! ```
//!
//! Low-pass filtering at 10 Hz:
//! ```rhai
//! // Remove high-frequency noise
//! lowpass(value, 10.0)
//! ```
//!
//! Time-based signal generation (for testing):
//! ```rhai
//! // Generate a sine wave
//! sin(time() * 2.0 * pi())
//! ```

mod engine;

pub use engine::{ExecutionContext, ScriptContext, ScriptEngine, SharedScriptContext};

use crate::error::{DataVisError, Result};
use rhai::{Engine, AST};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// A compiled converter script that can be executed efficiently
#[derive(Clone)]
pub struct CompiledConverter {
    /// The compiled AST
    ast: AST,
    /// The original source code
    source: String,
    /// Name/identifier for this converter
    name: String,
}

impl CompiledConverter {
    /// Get the source code of this converter
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Get the name of this converter
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Debug for CompiledConverter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledConverter")
            .field("name", &self.name)
            .field("source", &self.source)
            .finish()
    }
}

/// Cache for compiled scripts to avoid recompilation
#[derive(Default)]
pub struct ScriptCache {
    /// Map from script source to compiled converter
    cache: HashMap<String, CompiledConverter>,
}

impl ScriptCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Get a cached converter or compile and cache it
    pub fn get_or_compile(
        &mut self,
        engine: &Engine,
        name: &str,
        source: &str,
    ) -> Result<CompiledConverter> {
        if let Some(converter) = self.cache.get(source) {
            return Ok(converter.clone());
        }

        let ast = engine
            .compile(source)
            .map_err(|e| DataVisError::Script(format!("Compilation error: {}", e)))?;

        let converter = CompiledConverter {
            ast,
            source: source.to_string(),
            name: name.to_string(),
        };

        self.cache.insert(source.to_string(), converter.clone());
        Ok(converter)
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Remove a specific script from the cache
    pub fn invalidate(&mut self, source: &str) {
        self.cache.remove(source);
    }
}

/// Thread-safe script cache wrapper
pub type SharedScriptCache = Arc<RwLock<ScriptCache>>;

/// Create a new shared script cache
pub fn create_shared_cache() -> SharedScriptCache {
    Arc::new(RwLock::new(ScriptCache::new()))
}

/// Built-in converter scripts for common use cases
pub mod builtins {
    /// Identity converter - returns the raw value unchanged
    pub const IDENTITY: &str = r#"
fn convert(raw) {
    raw
}
"#;

    /// ADC to voltage converter (12-bit, 3.3V reference)
    pub const ADC_TO_VOLTAGE_3V3: &str = r#"
fn convert(raw) {
    raw * 3.3 / 4095.0
}
"#;

    /// ADC to voltage converter (12-bit, 5V reference)
    pub const ADC_TO_VOLTAGE_5V: &str = r#"
fn convert(raw) {
    raw * 5.0 / 4095.0
}
"#;

    /// Temperature from NTC thermistor (simplified)
    pub const NTC_TEMPERATURE: &str = r#"
// Simplified NTC calculation
// raw is the ADC value, assuming 10k NTC with 10k reference resistor
fn convert(raw) {
    let resistance = 10000.0 * raw / (4095.0 - raw);
    let temp_k = 1.0 / (1.0/298.15 + (1.0/3950.0) * log(resistance / 10000.0));
    temp_k - 273.15  // Convert to Celsius
}
"#;

    /// Percentage converter (0-255 to 0-100%)
    pub const BYTE_TO_PERCENT: &str = r#"
fn convert(raw) {
    raw * 100.0 / 255.0
}
"#;

    /// RPM from timer period (assuming timer counts in microseconds)
    pub const PERIOD_TO_RPM: &str = r#"
fn convert(raw) {
    if raw <= 0.0 {
        0.0
    } else {
        60000000.0 / raw  // 60 seconds * 1,000,000 microseconds / period
    }
}
"#;

    /// Signed 16-bit to signed value
    pub const U16_TO_SIGNED: &str = r#"
fn convert(raw) {
    if raw > 32767.0 {
        raw - 65536.0
    } else {
        raw
    }
}
"#;

    /// Fixed-point Q15 to float
    pub const Q15_TO_FLOAT: &str = r#"
fn convert(raw) {
    if raw > 32767.0 {
        (raw - 65536.0) / 32768.0
    } else {
        raw / 32768.0
    }
}
"#;

    /// Fixed-point Q31 to float
    pub const Q31_TO_FLOAT: &str = r#"
fn convert(raw) {
    if raw > 2147483647.0 {
        (raw - 4294967296.0) / 2147483648.0
    } else {
        raw / 2147483648.0
    }
}
"#;

    // ===== Transformer-based converters =====

    /// Derivative - compute rate of change (velocity from position, etc.)
    pub const DERIVATIVE: &str = r#"
// Compute derivative (rate of change) of the raw value
// Useful for: position -> velocity, counts -> rate, etc.
derivative(value)
"#;

    /// Smoothed value using exponential weighted moving average
    pub const SMOOTH_80: &str = r#"
// Exponential smoothing with alpha=0.8 (higher = smoother, more lag)
smooth(value, 0.8)
"#;

    /// Smoothed value using exponential weighted moving average (light)
    pub const SMOOTH_50: &str = r#"
// Light exponential smoothing with alpha=0.5
smooth(value, 0.5)
"#;

    /// Low-pass filter at 10 Hz
    pub const LOWPASS_10HZ: &str = r#"
// First-order lowpass filter with 10 Hz cutoff
// Removes noise above 10 Hz
lowpass(value, 10.0)
"#;

    /// Low-pass filter at 1 Hz
    pub const LOWPASS_1HZ: &str = r#"
// First-order lowpass filter with 1 Hz cutoff
// Heavy filtering for slow-changing signals
lowpass(value, 1.0)
"#;

    /// Integrate (accumulate) value over time
    pub const INTEGRATE: &str = r#"
// Integrate (accumulate) value over time
// Useful for: velocity -> position, flow rate -> total volume, etc.
integrate(value)
"#;

    /// Rate limiter - limits how fast value can change
    pub const RATE_LIMIT_100: &str = r#"
// Limit rate of change to 100 units per second
rate_limit(value, 100.0)
"#;

    /// Deadband around zero
    pub const DEADBAND_ZERO: &str = r#"
// Apply deadband: values within ±0.5 of zero become zero
deadband(value, 0.0, 1.0)
"#;

    /// Time-based sine wave test signal
    pub const TEST_SINE: &str = r#"
// Generate a 1 Hz sine wave (ignores raw value)
// Useful for testing plot behavior
sin(time() * 2.0 * pi())
"#;

    /// Time-based ramp test signal
    pub const TEST_RAMP: &str = r#"
// Generate a ramp signal (ignores raw value)
// Resets every 10 seconds
time() % 10.0
"#;

    /// List of all built-in converters with names
    pub fn all() -> Vec<(&'static str, &'static str)> {
        vec![
            ("Identity", IDENTITY),
            ("ADC to Voltage (3.3V)", ADC_TO_VOLTAGE_3V3),
            ("ADC to Voltage (5V)", ADC_TO_VOLTAGE_5V),
            ("NTC Temperature", NTC_TEMPERATURE),
            ("Byte to Percent", BYTE_TO_PERCENT),
            ("Period to RPM", PERIOD_TO_RPM),
            ("U16 to Signed", U16_TO_SIGNED),
            ("Q15 to Float", Q15_TO_FLOAT),
            ("Q31 to Float", Q31_TO_FLOAT),
        ]
    }

    /// List of transformer-based converters (use time, derivative, etc.)
    pub fn transformers() -> Vec<(&'static str, &'static str)> {
        vec![
            ("Derivative", DERIVATIVE),
            ("Integrate", INTEGRATE),
            ("Smooth (80%)", SMOOTH_80),
            ("Smooth (50%)", SMOOTH_50),
            ("Lowpass 10 Hz", LOWPASS_10HZ),
            ("Lowpass 1 Hz", LOWPASS_1HZ),
            ("Rate Limit (100/s)", RATE_LIMIT_100),
            ("Deadband (±0.5)", DEADBAND_ZERO),
            ("Test: Sine Wave", TEST_SINE),
            ("Test: Ramp", TEST_RAMP),
        ]
    }

    /// List of all converters (basic + transformers)
    pub fn all_with_transformers() -> Vec<(&'static str, &'static str)> {
        let mut result = all();
        result.extend(transformers());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_cache() {
        let engine = Engine::new();
        let mut cache = ScriptCache::new();

        let script = "fn convert(x) { x * 2.0 }";
        let converter1 = cache.get_or_compile(&engine, "test", script).unwrap();
        let converter2 = cache.get_or_compile(&engine, "test", script).unwrap();

        // Should be the same (cached)
        assert_eq!(converter1.source(), converter2.source());
    }

    #[test]
    fn test_builtin_converters() {
        let builtins = builtins::all();
        assert!(!builtins.is_empty());

        // All builtins should compile
        let engine = Engine::new();
        for (name, source) in builtins {
            let result = engine.compile(source);
            assert!(result.is_ok(), "Built-in '{}' failed to compile", name);
        }
    }

    #[test]
    fn test_transformer_converters() {
        let transformers = builtins::transformers();
        assert!(!transformers.is_empty());

        // All transformer converters should compile
        // Note: We use ScriptEngine here because transformers use registered functions
        let engine = super::ScriptEngine::new();
        for (name, source) in transformers {
            let result = engine.validate(source);
            assert!(
                result.is_ok(),
                "Transformer '{}' failed to compile: {:?}",
                name,
                result.err()
            );
        }
    }

    #[test]
    fn test_all_with_transformers() {
        let all = builtins::all_with_transformers();
        let basic = builtins::all();
        let transformers = builtins::transformers();

        // Should contain both basic and transformer converters
        assert_eq!(all.len(), basic.len() + transformers.len());
    }
}
