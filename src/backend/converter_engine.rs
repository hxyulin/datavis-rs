//! ConverterEngine - Simplified converter execution for the backend worker
//!
//! This module extracts the converter logic from ScriptTransformNode into a
//! lightweight component that can be directly integrated into BackendWorker.
//! It handles per-variable Rhai script execution with state tracking for
//! stateful converters (derivatives, filters, etc.).

use crate::scripting::{CompiledConverter, ExecutionContext, ScriptEngine};
use crate::types::Variable;
use std::collections::HashMap;
use std::time::Duration;

/// State for a single converter across executions
#[derive(Debug, Clone)]
struct ConverterState {
    prev_raw: f64,
    prev_converted: f64,
    prev_time: Duration,
}

/// Engine for applying per-variable converters to probe data
pub struct ConverterEngine {
    /// Rhai script engine for executing converters
    engine: ScriptEngine,

    /// Compiled Rhai converters per variable
    converters: HashMap<u32, CompiledConverter>,

    /// Previous values for stateful converters (derivatives, filters)
    prev_state: HashMap<u32, ConverterState>,
}

impl ConverterEngine {
    /// Create a new converter engine
    pub fn new() -> Self {
        Self {
            engine: ScriptEngine::new(),
            converters: HashMap::new(),
            prev_state: HashMap::new(),
        }
    }

    /// Apply converters to probe results, returning (var_id, timestamp, raw, converted)
    ///
    /// This performs in-place modification of converter state while producing
    /// a new Vec with the converted values.
    pub fn apply_converters(
        &mut self,
        results: &[(u32, Duration, f64)], // (var_id, timestamp, raw_value)
    ) -> Vec<(u32, Duration, f64, f64)> {
        // (var_id, timestamp, raw, converted)
        results
            .iter()
            .map(|(var_id, timestamp, raw)| {
                let converted = if let Some(converter) = self.converters.get(var_id) {
                    // Build execution context from previous state
                    let ctx = if let Some(state) = self.prev_state.get(var_id) {
                        let dt_secs = timestamp
                            .saturating_sub(state.prev_time)
                            .as_secs_f64();
                        ExecutionContext::new(
                            timestamp.as_secs_f64(),
                            dt_secs,
                            state.prev_raw,
                            state.prev_converted,
                        )
                    } else {
                        // First sample for this variable
                        ExecutionContext::first_sample(timestamp.as_secs_f64())
                    };

                    // Execute converter
                    match self.engine.execute(converter, *raw, ctx) {
                        Ok(val) => {
                            // Update state on success
                            self.prev_state.insert(
                                *var_id,
                                ConverterState {
                                    prev_raw: *raw,
                                    prev_converted: val,
                                    prev_time: *timestamp,
                                },
                            );
                            val
                        }
                        Err(e) => {
                            // On error, fallback to raw and still update state
                            tracing::trace!("Converter error for var {}: {}", var_id, e);
                            self.prev_state.insert(
                                *var_id,
                                ConverterState {
                                    prev_raw: *raw,
                                    prev_converted: *raw,
                                    prev_time: *timestamp,
                                },
                            );
                            *raw
                        }
                    }
                } else {
                    // No converter - raw = converted
                    // Still track state in case converter is added later
                    self.prev_state.insert(
                        *var_id,
                        ConverterState {
                            prev_raw: *raw,
                            prev_converted: *raw,
                            prev_time: *timestamp,
                        },
                    );
                    *raw
                };

                (*var_id, *timestamp, *raw, converted)
            })
            .collect()
    }

    /// Update converter script for a variable
    ///
    /// If `script` is None, the converter is removed.
    /// If `script` is Some, it will be compiled and cached.
    pub fn update_converter(&mut self, var_id: u32, var_name: &str, script: Option<String>) {
        if let Some(script) = script {
            match self.engine.compile(var_name, &script) {
                Ok(compiled) => {
                    self.converters.insert(var_id, compiled);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to compile converter for '{}' (id={}): {}",
                        var_name,
                        var_id,
                        e
                    );
                    // Remove converter on compilation failure
                    self.converters.remove(&var_id);
                }
            }
        } else {
            // No script - remove converter
            self.converters.remove(&var_id);
        }
    }

    /// Add a variable with its converter (if any)
    pub fn add_variable(&mut self, var: &Variable) {
        if let Some(ref script) = var.converter_script {
            match self.engine.compile(&var.name, script) {
                Ok(compiled) => {
                    self.converters.insert(var.id, compiled);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to compile converter for '{}' (id={}): {}",
                        var.name,
                        var.id,
                        e
                    );
                }
            }
        }
    }

    /// Remove a variable and its converter state
    pub fn remove_variable(&mut self, var_id: u32) {
        self.converters.remove(&var_id);
        self.prev_state.remove(&var_id);
    }

    /// Clear all converter state (called on collection start/reset)
    pub fn clear_state(&mut self) {
        self.prev_state.clear();
    }

    /// Get the number of active converters
    #[allow(dead_code)]
    pub fn converter_count(&self) -> usize {
        self.converters.len()
    }
}

impl Default for ConverterEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_no_converter() {
        let mut engine = ConverterEngine::new();

        let results = vec![(1, Duration::from_secs(0), 42.0)];
        let converted = engine.apply_converters(&results);

        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0], (1, Duration::from_secs(0), 42.0, 42.0));
    }

    #[test]
    fn test_simple_converter() {
        let mut engine = ConverterEngine::new();

        // Add a simple scale converter
        engine.update_converter(1, "test_var", Some("value * 2.0".to_string()));

        let results = vec![(1, Duration::from_secs(0), 21.0)];
        let converted = engine.apply_converters(&results);

        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0], (1, Duration::from_secs(0), 21.0, 42.0));
    }

    #[test]
    fn test_adc_converter() {
        let mut engine = ConverterEngine::new();

        // ADC to voltage: 12-bit (0-4095), 3.3V reference
        engine.update_converter(
            1,
            "adc_voltage",
            Some("value * 3.3 / 4095.0".to_string()),
        );

        let results = vec![(1, Duration::from_secs(0), 2048.0)];
        let converted = engine.apply_converters(&results);

        assert_eq!(converted.len(), 1);
        let (_, _, raw, conv) = converted[0];
        assert_eq!(raw, 2048.0);
        assert!((conv - 1.65).abs() < 0.01); // ~1.65V
    }

    #[test]
    fn test_stateful_converter_smooth() {
        let mut engine = ConverterEngine::new();

        // Smooth converter with alpha=0.5
        engine.update_converter(1, "smoothed", Some("smooth(value, 0.5)".to_string()));

        // First sample: no previous data, should return raw value
        let results1 = vec![(1, Duration::from_millis(0), 10.0)];
        let converted1 = engine.apply_converters(&results1);
        assert_eq!(converted1[0].3, 10.0);

        // Second sample: value jumps to 20
        // smooth(20, 10, 0.5) = 0.5 * 10 + 0.5 * 20 = 15
        let results2 = vec![(1, Duration::from_millis(100), 20.0)];
        let converted2 = engine.apply_converters(&results2);
        assert!((converted2[0].3 - 15.0).abs() < 0.01);

        // Third sample: value stays at 20
        // smooth(20, 15, 0.5) = 0.5 * 15 + 0.5 * 20 = 17.5
        let results3 = vec![(1, Duration::from_millis(200), 20.0)];
        let converted3 = engine.apply_converters(&results3);
        assert!((converted3[0].3 - 17.5).abs() < 0.01);
    }

    #[test]
    fn test_stateful_converter_lowpass() {
        let mut engine = ConverterEngine::new();

        // 10 Hz lowpass filter
        engine.update_converter(1, "filtered", Some("lowpass(value, 10.0)".to_string()));

        // First sample: no filtering
        let results1 = vec![(1, Duration::from_millis(0), 10.0)];
        let converted1 = engine.apply_converters(&results1);
        assert_eq!(converted1[0].3, 10.0);

        // Second sample: step to 20, should be filtered (between 10 and 20)
        let results2 = vec![(1, Duration::from_millis(10), 20.0)];
        let converted2 = engine.apply_converters(&results2);
        let filtered = converted2[0].3;
        assert!(filtered > 10.0 && filtered < 20.0, "filtered = {}", filtered);
    }

    #[test]
    fn test_invalid_converter_fallback() {
        let mut engine = ConverterEngine::new();

        // Invalid script should fail to compile
        engine.update_converter(1, "bad_var", Some("invalid syntax (".to_string()));

        // Should have no converter after failed compilation
        assert_eq!(engine.converter_count(), 0);

        // Apply should return raw value
        let results = vec![(1, Duration::from_secs(0), 42.0)];
        let converted = engine.apply_converters(&results);
        assert_eq!(converted[0], (1, Duration::from_secs(0), 42.0, 42.0));
    }

    #[test]
    fn test_multiple_variables() {
        let mut engine = ConverterEngine::new();

        engine.update_converter(1, "var1", Some("value * 2.0".to_string()));
        engine.update_converter(2, "var2", Some("value + 100.0".to_string()));
        engine.update_converter(3, "var3", None); // No converter

        let results = vec![
            (1, Duration::from_secs(0), 10.0),
            (2, Duration::from_secs(0), 50.0),
            (3, Duration::from_secs(0), 99.0),
        ];

        let converted = engine.apply_converters(&results);

        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0].3, 20.0); // 10 * 2
        assert_eq!(converted[1].3, 150.0); // 50 + 100
        assert_eq!(converted[2].3, 99.0); // no conversion
    }

    #[test]
    fn test_add_remove_variable() {
        let mut engine = ConverterEngine::new();

        let mut var = Variable::default();
        var.id = 1;
        var.name = "test".to_string();
        var.converter_script = Some("value * 3.0".to_string());

        engine.add_variable(&var);
        assert_eq!(engine.converter_count(), 1);

        engine.remove_variable(1);
        assert_eq!(engine.converter_count(), 0);
    }

    #[test]
    fn test_clear_state() {
        let mut engine = ConverterEngine::new();

        engine.update_converter(1, "var1", Some("derivative(value)".to_string()));

        // Build up some state
        let results1 = vec![(1, Duration::from_millis(0), 10.0)];
        engine.apply_converters(&results1);

        let results2 = vec![(1, Duration::from_millis(100), 20.0)];
        engine.apply_converters(&results2);

        assert_eq!(engine.prev_state.len(), 1);

        // Clear state
        engine.clear_state();
        assert_eq!(engine.prev_state.len(), 0);

        // Next sample should be treated as first sample
        let results3 = vec![(1, Duration::from_millis(200), 30.0)];
        let converted3 = engine.apply_converters(&results3);
        assert_eq!(converted3[0].3, 0.0); // Derivative with no prev = 0
    }

    #[test]
    fn test_state_persistence_across_calls() {
        let mut engine = ConverterEngine::new();

        // Integrator: accumulates value over time
        engine.update_converter(1, "integral", Some("integrate(value)".to_string()));

        // Each sample adds value * dt to accumulator
        let samples = vec![
            (Duration::from_millis(0), 10.0),
            (Duration::from_millis(100), 10.0),
            (Duration::from_millis(200), 10.0),
        ];

        let mut accumulated = 0.0;
        for (timestamp, value) in samples {
            let results = vec![(1, timestamp, value)];
            let converted = engine.apply_converters(&results);
            accumulated = converted[0].3;
        }

        // After 3 samples of value=10 over 0.2 seconds total:
        // Sample 1: 10 * 0 = 0 (first sample, dt=0)
        // Sample 2: 0 + 10 * 0.1 = 1
        // Sample 3: 1 + 10 * 0.1 = 2
        assert!((accumulated - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_converter_update_replaces_old() {
        let mut engine = ConverterEngine::new();

        // Add first converter
        engine.update_converter(1, "var1", Some("value * 2.0".to_string()));

        let results = vec![(1, Duration::from_secs(0), 10.0)];
        let converted = engine.apply_converters(&results);
        assert_eq!(converted[0].3, 20.0);

        // Update to different converter
        engine.update_converter(1, "var1", Some("value * 3.0".to_string()));

        let results = vec![(1, Duration::from_secs(1), 10.0)];
        let converted = engine.apply_converters(&results);
        assert_eq!(converted[0].3, 30.0);

        // Remove converter
        engine.update_converter(1, "var1", None);

        let results = vec![(1, Duration::from_secs(2), 10.0)];
        let converted = engine.apply_converters(&results);
        assert_eq!(converted[0].3, 10.0); // No conversion
    }
}
