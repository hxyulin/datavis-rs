//! Rhai Script Engine Implementation
//!
//! This module provides the main scripting engine for variable converters.
//!
//! ## Dynamic Variables
//!
//! The following dynamic variables are available in scripts:
//! - `value` / `raw` - The current raw value being converted
//! - `time()` - Time in seconds since collection started (pauses when paused)
//! - `dt()` - Delta time since last sample in seconds
//! - `prev()` - Previous converted value (NaN if not available)
//! - `prev_raw()` - Previous raw value (NaN if not available)
//!
//! ## Transformer Functions
//!
//! - `derivative(current, previous, dt)` - Compute rate of change
//! - `integrate(current, accumulated, dt)` - Compute integral (accumulate)
//! - `smooth(current, previous, alpha)` - Exponential smoothing (EWMA)
//! - `lowpass(current, previous, cutoff_hz, dt)` - First-order lowpass filter
//! - `highpass(current, previous, prev_output, cutoff_hz, dt)` - First-order highpass filter
//! - `deadband(value, center, width)` - Apply deadband/hysteresis
//! - `rate_limit(current, previous, max_rate, dt)` - Limit rate of change

use crate::error::{DataVisError, Result};
use crate::scripting::{CompiledConverter, ScriptCache, SharedScriptCache};
use rhai::{Dynamic, Engine, Scope};
use std::sync::{Arc, RwLock};

/// Execution context passed to scripts, containing timing and historical data
#[derive(Debug, Clone, Default)]
pub struct ExecutionContext {
    /// Time since collection started in seconds (pauses when collection is paused)
    pub time_secs: f64,
    /// Delta time since last sample in seconds
    pub dt_secs: f64,
    /// Previous raw value (NaN if not available)
    pub prev_raw: f64,
    /// Previous converted value (NaN if not available)
    pub prev_converted: f64,
}

impl ExecutionContext {
    /// Create a new execution context
    pub fn new(time_secs: f64, dt_secs: f64, prev_raw: f64, prev_converted: f64) -> Self {
        Self {
            time_secs,
            dt_secs,
            prev_raw,
            prev_converted,
        }
    }

    /// Create context with no previous values
    pub fn first_sample(time_secs: f64) -> Self {
        Self {
            time_secs,
            dt_secs: 0.0,
            prev_raw: f64::NAN,
            prev_converted: f64::NAN,
        }
    }
}

/// Shared script context that can be accessed from within Rhai scripts
/// This is updated before each script execution and read via registered functions
#[derive(Debug, Clone, Default)]
pub struct ScriptContext {
    /// Current execution context
    context: ExecutionContext,
}

impl ScriptContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, ctx: ExecutionContext) {
        self.context = ctx;
    }

    /// Get time since collection started in seconds
    pub fn time(&mut self) -> f64 {
        self.context.time_secs
    }

    /// Get delta time since last sample in seconds
    pub fn dt(&mut self) -> f64 {
        self.context.dt_secs
    }

    /// Get previous converted value (NaN if not available)
    pub fn prev(&mut self) -> f64 {
        self.context.prev_converted
    }

    /// Get previous raw value (NaN if not available)
    pub fn prev_raw(&mut self) -> f64 {
        self.context.prev_raw
    }

    /// Check if previous value is available
    pub fn has_prev(&mut self) -> bool {
        !self.context.prev_converted.is_nan()
    }
}

/// Thread-safe shared script context
pub type SharedScriptContext = Arc<RwLock<ScriptContext>>;

/// The main script engine for executing variable converters
pub struct ScriptEngine {
    /// The Rhai engine instance
    engine: Engine,
    /// Cache of compiled scripts
    cache: SharedScriptCache,
    /// Shared context for dynamic variable access
    context: SharedScriptContext,
}

impl ScriptEngine {
    /// Create a new script engine with default configuration
    pub fn new() -> Self {
        let context = Arc::new(RwLock::new(ScriptContext::new()));
        let mut engine = Engine::new();
        Self::configure_engine(&mut engine, context.clone());

        Self {
            engine,
            cache: Arc::new(std::sync::RwLock::new(ScriptCache::new())),
            context,
        }
    }

    /// Create a new script engine with a shared cache
    pub fn with_cache(cache: SharedScriptCache) -> Self {
        let context = Arc::new(RwLock::new(ScriptContext::new()));
        let mut engine = Engine::new();
        Self::configure_engine(&mut engine, context.clone());

        Self {
            engine,
            cache,
            context,
        }
    }

    /// Configure the Rhai engine with built-in functions and safety limits
    fn configure_engine(engine: &mut Engine, context: SharedScriptContext) {
        // Set safety limits
        engine.set_max_expr_depths(64, 64);
        engine.set_max_call_levels(32);
        engine.set_max_operations(10_000);
        engine.set_max_string_size(10_000);
        engine.set_max_array_size(1_000);
        engine.set_max_map_size(1_000);

        // Register the ScriptContext type
        engine.register_type_with_name::<ScriptContext>("ScriptContext");

        // Register dynamic context accessor functions
        // These read from the shared context that's updated before each execution
        {
            let ctx = context.clone();
            engine.register_fn("time", move || -> f64 {
                ctx.read().map(|c| c.context.time_secs).unwrap_or(0.0)
            });
        }
        {
            let ctx = context.clone();
            engine.register_fn("dt", move || -> f64 {
                ctx.read().map(|c| c.context.dt_secs).unwrap_or(0.0)
            });
        }
        {
            let ctx = context.clone();
            engine.register_fn("prev", move || -> f64 {
                ctx.read()
                    .map(|c| c.context.prev_converted)
                    .unwrap_or(f64::NAN)
            });
        }
        {
            let ctx = context.clone();
            engine.register_fn("prev_raw", move || -> f64 {
                ctx.read().map(|c| c.context.prev_raw).unwrap_or(f64::NAN)
            });
        }
        {
            let ctx = context.clone();
            engine.register_fn("has_prev", move || -> bool {
                ctx.read()
                    .map(|c| !c.context.prev_converted.is_nan())
                    .unwrap_or(false)
            });
        }

        // ===== Transformer Functions =====

        // Derivative: compute rate of change
        // Usage: derivative(value, prev(), dt()) or derivative(value, prev_value, delta_time)
        engine.register_fn(
            "derivative",
            |current: f64, previous: f64, dt: f64| -> f64 {
                if dt > 0.0 && !previous.is_nan() {
                    (current - previous) / dt
                } else {
                    0.0
                }
            },
        );

        // Simple derivative using context (just pass current value)
        {
            let ctx = context.clone();
            engine.register_fn("derivative", move |current: f64| -> f64 {
                let (prev, dt) = ctx
                    .read()
                    .map(|c| (c.context.prev_converted, c.context.dt_secs))
                    .unwrap_or((f64::NAN, 0.0));
                if dt > 0.0 && !prev.is_nan() {
                    (current - prev) / dt
                } else {
                    0.0
                }
            });
        }

        // Integrate: accumulate value over time
        // Usage: integrate(value, accumulated, dt) - returns new accumulated value
        engine.register_fn(
            "integrate",
            |current: f64, accumulated: f64, dt: f64| -> f64 {
                if !accumulated.is_nan() {
                    accumulated + current * dt
                } else {
                    current * dt
                }
            },
        );

        // Simple integrate using context
        {
            let ctx = context.clone();
            engine.register_fn("integrate", move |current: f64| -> f64 {
                let (prev, dt) = ctx
                    .read()
                    .map(|c| (c.context.prev_converted, c.context.dt_secs))
                    .unwrap_or((f64::NAN, 0.0));
                if !prev.is_nan() {
                    prev + current * dt
                } else {
                    current * dt
                }
            });
        }

        // Smooth: exponential weighted moving average (EWMA)
        // Usage: smooth(value, prev(), alpha) where alpha is 0-1 (higher = more smoothing)
        engine.register_fn("smooth", |current: f64, previous: f64, alpha: f64| -> f64 {
            let alpha = alpha.clamp(0.0, 1.0);
            if !previous.is_nan() {
                alpha * previous + (1.0 - alpha) * current
            } else {
                current
            }
        });

        // Simple smooth using context
        {
            let ctx = context.clone();
            engine.register_fn("smooth", move |current: f64, alpha: f64| -> f64 {
                let alpha = alpha.clamp(0.0, 1.0);
                let prev = ctx
                    .read()
                    .map(|c| c.context.prev_converted)
                    .unwrap_or(f64::NAN);
                if !prev.is_nan() {
                    alpha * prev + (1.0 - alpha) * current
                } else {
                    current
                }
            });
        }

        // Lowpass filter: first-order IIR lowpass
        // Usage: lowpass(value, prev(), cutoff_hz, dt())
        engine.register_fn(
            "lowpass",
            |current: f64, previous: f64, cutoff_hz: f64, dt: f64| -> f64 {
                if !previous.is_nan() && dt > 0.0 && cutoff_hz > 0.0 {
                    let rc = 1.0 / (2.0 * std::f64::consts::PI * cutoff_hz);
                    let alpha = dt / (rc + dt);
                    previous + alpha * (current - previous)
                } else {
                    current
                }
            },
        );

        // Simple lowpass using context
        {
            let ctx = context.clone();
            engine.register_fn("lowpass", move |current: f64, cutoff_hz: f64| -> f64 {
                let (prev, dt) = ctx
                    .read()
                    .map(|c| (c.context.prev_converted, c.context.dt_secs))
                    .unwrap_or((f64::NAN, 0.0));
                if !prev.is_nan() && dt > 0.0 && cutoff_hz > 0.0 {
                    let rc = 1.0 / (2.0 * std::f64::consts::PI * cutoff_hz);
                    let alpha = dt / (rc + dt);
                    prev + alpha * (current - prev)
                } else {
                    current
                }
            });
        }

        // Highpass filter: first-order IIR highpass
        // Note: This needs the previous output value, so user must track it
        // Usage: highpass(value, prev_input, prev_output, cutoff_hz, dt)
        engine.register_fn(
            "highpass",
            |current: f64, prev_input: f64, prev_output: f64, cutoff_hz: f64, dt: f64| -> f64 {
                if !prev_input.is_nan() && !prev_output.is_nan() && dt > 0.0 && cutoff_hz > 0.0 {
                    let rc = 1.0 / (2.0 * std::f64::consts::PI * cutoff_hz);
                    let alpha = rc / (rc + dt);
                    alpha * (prev_output + current - prev_input)
                } else {
                    0.0 // Highpass output is 0 when no previous data
                }
            },
        );

        // Deadband: ignore small changes around a center value
        // Usage: deadband(value, center, width) - returns center if within deadband
        engine.register_fn("deadband", |value: f64, center: f64, width: f64| -> f64 {
            if (value - center).abs() < width / 2.0 {
                center
            } else {
                value
            }
        });

        // Rate limiter: limit how fast a value can change
        // Usage: rate_limit(value, prev(), max_rate, dt())
        engine.register_fn(
            "rate_limit",
            |current: f64, previous: f64, max_rate: f64, dt: f64| -> f64 {
                if !previous.is_nan() && dt > 0.0 {
                    let max_change = max_rate * dt;
                    let change = current - previous;
                    if change.abs() > max_change {
                        previous + change.signum() * max_change
                    } else {
                        current
                    }
                } else {
                    current
                }
            },
        );

        // Simple rate_limit using context
        {
            let ctx = context.clone();
            engine.register_fn("rate_limit", move |current: f64, max_rate: f64| -> f64 {
                let (prev, dt) = ctx
                    .read()
                    .map(|c| (c.context.prev_converted, c.context.dt_secs))
                    .unwrap_or((f64::NAN, 0.0));
                if !prev.is_nan() && dt > 0.0 {
                    let max_change = max_rate * dt;
                    let change = current - prev;
                    if change.abs() > max_change {
                        prev + change.signum() * max_change
                    } else {
                        current
                    }
                } else {
                    current
                }
            });
        }

        // Hysteresis: only change output when input crosses thresholds
        // Usage: hysteresis(value, prev_output, low_threshold, high_threshold, low_value, high_value)
        engine.register_fn(
            "hysteresis",
            |input: f64,
             prev_output: f64,
             low_thresh: f64,
             high_thresh: f64,
             low_val: f64,
             high_val: f64|
             -> f64 {
                if input >= high_thresh {
                    high_val
                } else if input <= low_thresh {
                    low_val
                } else if !prev_output.is_nan() {
                    prev_output
                } else {
                    low_val // Default to low if no previous
                }
            },
        );

        // ===== Mathematical Functions =====

        engine.register_fn("abs", |x: f64| x.abs());
        engine.register_fn("sqrt", |x: f64| x.sqrt());
        engine.register_fn("pow", |x: f64, y: f64| x.powf(y));
        engine.register_fn("exp", |x: f64| x.exp());
        engine.register_fn("ln", |x: f64| x.ln());
        engine.register_fn("log", |x: f64| x.ln()); // Alias for natural log
        engine.register_fn("log10", |x: f64| x.log10());
        engine.register_fn("log2", |x: f64| x.log2());
        engine.register_fn("sin", |x: f64| x.sin());
        engine.register_fn("cos", |x: f64| x.cos());
        engine.register_fn("tan", |x: f64| x.tan());
        engine.register_fn("asin", |x: f64| x.asin());
        engine.register_fn("acos", |x: f64| x.acos());
        engine.register_fn("atan", |x: f64| x.atan());
        engine.register_fn("atan2", |y: f64, x: f64| y.atan2(x));
        engine.register_fn("sinh", |x: f64| x.sinh());
        engine.register_fn("cosh", |x: f64| x.cosh());
        engine.register_fn("tanh", |x: f64| x.tanh());

        // Rounding functions
        engine.register_fn("floor", |x: f64| x.floor());
        engine.register_fn("ceil", |x: f64| x.ceil());
        engine.register_fn("round", |x: f64| x.round());
        engine.register_fn("trunc", |x: f64| x.trunc());
        engine.register_fn("fract", |x: f64| x.fract());

        // Clamping and limiting
        engine.register_fn("clamp", |x: f64, min: f64, max: f64| x.clamp(min, max));
        engine.register_fn("min", |a: f64, b: f64| a.min(b));
        engine.register_fn("max", |a: f64, b: f64| a.max(b));

        // Bit manipulation (for integer values cast as f64)
        engine.register_fn("bit_and", |a: i64, b: i64| a & b);
        engine.register_fn("bit_or", |a: i64, b: i64| a | b);
        engine.register_fn("bit_xor", |a: i64, b: i64| a ^ b);
        engine.register_fn("bit_not", |a: i64| !a);
        engine.register_fn("bit_shl", |a: i64, b: i64| a << b);
        engine.register_fn("bit_shr", |a: i64, b: i64| a >> b);

        // Type conversions
        engine.register_fn("to_int", |x: f64| x as i64);
        engine.register_fn("to_float", |x: i64| x as f64);

        // Constants
        engine.register_fn("pi", || std::f64::consts::PI);
        engine.register_fn("e", || std::f64::consts::E);

        // Utility functions
        engine.register_fn("is_nan", |x: f64| x.is_nan());
        engine.register_fn("is_finite", |x: f64| x.is_finite());
        engine.register_fn("is_infinite", |x: f64| x.is_infinite());
        engine.register_fn("sign", |x: f64| {
            if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            }
        });

        // Linear interpolation
        engine.register_fn("lerp", |a: f64, b: f64, t: f64| a + (b - a) * t);

        // Map value from one range to another
        engine.register_fn(
            "map_range",
            |x: f64, in_min: f64, in_max: f64, out_min: f64, out_max: f64| {
                (x - in_min) * (out_max - out_min) / (in_max - in_min) + out_min
            },
        );

        // Moving average helper (returns the value - actual averaging done externally)
        engine.register_fn("identity", |x: f64| x);
    }

    /// Compile a script and cache it
    pub fn compile(&self, name: &str, source: &str) -> Result<CompiledConverter> {
        let mut cache = self
            .cache
            .write()
            .map_err(|e| DataVisError::Script(format!("Failed to acquire cache lock: {}", e)))?;

        cache.get_or_compile(&self.engine, name, source)
    }

    /// Execute a compiled converter with a raw value and execution context
    pub fn execute(
        &self,
        converter: &CompiledConverter,
        raw_value: f64,
        ctx: ExecutionContext,
    ) -> Result<f64> {
        // Update the shared context before execution
        {
            let mut context = self.context.write().map_err(|e| {
                DataVisError::Script(format!("Failed to acquire context lock: {}", e))
            })?;
            context.update(ctx);
        }

        let mut scope = Scope::new();
        scope.push("value", raw_value);
        scope.push("raw", raw_value);

        // First, try calling a 'convert' function if it exists
        let result =
            self.engine
                .call_fn::<f64>(&mut scope, &converter.ast, "convert", (raw_value,));

        match result {
            Ok(value) => Ok(value),
            Err(_) => {
                // If no 'convert' function, evaluate the script as an expression
                // where 'value' or 'raw' contains the input
                self.engine
                    .eval_ast_with_scope::<Dynamic>(&mut scope, &converter.ast)
                    .map_err(|e| DataVisError::Script(format!("Execution error: {}", e)))
                    .and_then(|v| {
                        // Try to get as f64 first (covers both int and float cases)
                        if let Ok(f) = v.as_float() {
                            Ok(f)
                        } else if let Ok(i) = v.as_int() {
                            Ok(i as f64)
                        } else {
                            Err(DataVisError::Script(
                                "Script must return a numeric value".to_string(),
                            ))
                        }
                    })
            }
        }
    }

    /// Execute a compiled converter with just a raw value (no context - uses defaults)
    pub fn execute_simple(&self, converter: &CompiledConverter, raw_value: f64) -> Result<f64> {
        self.execute(converter, raw_value, ExecutionContext::default())
    }

    /// Compile and execute a script in one step (for one-off conversions)
    pub fn eval(&self, source: &str, raw_value: f64) -> Result<f64> {
        let converter = self.compile("temp", source)?;
        self.execute_simple(&converter, raw_value)
    }

    /// Compile and execute a script with context
    pub fn eval_with_context(
        &self,
        source: &str,
        raw_value: f64,
        ctx: ExecutionContext,
    ) -> Result<f64> {
        let converter = self.compile("temp", source)?;
        self.execute(&converter, raw_value, ctx)
    }

    /// Validate a script without executing it
    pub fn validate(&self, source: &str) -> Result<()> {
        self.engine
            .compile(source)
            .map(|_| ())
            .map_err(|e| DataVisError::Script(format!("Validation error: {}", e)))
    }

    /// Clear the script cache
    pub fn clear_cache(&self) -> Result<()> {
        let mut cache = self
            .cache
            .write()
            .map_err(|e| DataVisError::Script(format!("Failed to acquire cache lock: {}", e)))?;
        cache.clear();
        Ok(())
    }

    /// Get a reference to the underlying Rhai engine
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Get a reference to the shared cache
    pub fn cache(&self) -> &SharedScriptCache {
        &self.cache
    }

    /// Get a reference to the shared context
    pub fn context(&self) -> &SharedScriptContext {
        &self.context
    }
}

impl Default for ScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ScriptEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptEngine")
            .field("cache_size", &self.cache.read().map(|c| c.cache.len()).ok())
            .finish()
    }
}

// Make ScriptEngine Send + Sync safe
unsafe impl Send for ScriptEngine {}
unsafe impl Sync for ScriptEngine {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_creation() {
        let engine = ScriptEngine::new();
        assert!(engine.cache.read().unwrap().cache.is_empty());
    }

    #[test]
    fn test_simple_conversion() {
        let engine = ScriptEngine::new();
        let result = engine.eval("value * 2.0", 5.0).unwrap();
        assert_eq!(result, 10.0);
    }

    #[test]
    fn test_function_conversion() {
        let engine = ScriptEngine::new();
        let script = r#"
fn convert(raw) {
    raw * 3.3 / 4095.0
}
"#;
        let result = engine.eval(script, 2048.0).unwrap();
        assert!((result - 1.65).abs() < 0.01);
    }

    #[test]
    fn test_math_functions() {
        let engine = ScriptEngine::new();

        // Test sqrt
        assert_eq!(engine.eval("sqrt(16.0)", 0.0).unwrap(), 4.0);

        // Test pow
        assert_eq!(engine.eval("pow(2.0, 3.0)", 0.0).unwrap(), 8.0);

        // Test clamp
        assert_eq!(
            engine.eval("clamp(value, 0.0, 100.0)", 150.0).unwrap(),
            100.0
        );
        assert_eq!(engine.eval("clamp(value, 0.0, 100.0)", -50.0).unwrap(), 0.0);
        assert_eq!(engine.eval("clamp(value, 0.0, 100.0)", 50.0).unwrap(), 50.0);
    }

    #[test]
    fn test_map_range() {
        let engine = ScriptEngine::new();
        // Map 50 from 0-100 to 0-1000 should give 500
        let result = engine
            .eval("map_range(value, 0.0, 100.0, 0.0, 1000.0)", 50.0)
            .unwrap();
        assert_eq!(result, 500.0);
    }

    #[test]
    fn test_validation() {
        let engine = ScriptEngine::new();

        // Valid script
        assert!(engine.validate("value * 2.0").is_ok());

        // Invalid script
        assert!(engine.validate("value * ").is_err());
    }

    #[test]
    fn test_caching() {
        let engine = ScriptEngine::new();

        let script = "value * 2.0";
        let _ = engine.compile("test", script).unwrap();
        let _ = engine.compile("test", script).unwrap();

        // Should only have one entry in cache
        assert_eq!(engine.cache.read().unwrap().cache.len(), 1);
    }

    #[test]
    fn test_trig_functions() {
        let engine = ScriptEngine::new();

        // sin(0) = 0
        assert!((engine.eval("sin(0.0)", 0.0).unwrap()).abs() < 0.0001);

        // cos(0) = 1
        assert!((engine.eval("cos(0.0)", 0.0).unwrap() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_time_function() {
        let engine = ScriptEngine::new();

        // Test time() returns the context time
        let ctx = ExecutionContext::new(5.5, 0.01, f64::NAN, f64::NAN);
        let result = engine.eval_with_context("time()", 0.0, ctx).unwrap();
        assert!((result - 5.5).abs() < 0.0001);
    }

    #[test]
    fn test_dt_function() {
        let engine = ScriptEngine::new();

        // Test dt() returns the delta time
        let ctx = ExecutionContext::new(5.5, 0.01, f64::NAN, f64::NAN);
        let result = engine.eval_with_context("dt()", 0.0, ctx).unwrap();
        assert!((result - 0.01).abs() < 0.0001);
    }

    #[test]
    fn test_derivative_function() {
        let engine = ScriptEngine::new();

        // Derivative of value going from 10 to 20 over 0.1 seconds = 100
        let ctx = ExecutionContext::new(1.0, 0.1, 10.0, 10.0);
        let result = engine
            .eval_with_context("derivative(value)", 20.0, ctx)
            .unwrap();
        assert!((result - 100.0).abs() < 0.0001);
    }

    #[test]
    fn test_derivative_no_prev() {
        let engine = ScriptEngine::new();

        // Derivative with no previous value should return 0
        let ctx = ExecutionContext::first_sample(1.0);
        let result = engine
            .eval_with_context("derivative(value)", 20.0, ctx)
            .unwrap();
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_integrate_function() {
        let engine = ScriptEngine::new();

        // Integrate value of 10 with previous accumulated 5, dt of 0.1 = 5 + 10*0.1 = 6
        let ctx = ExecutionContext::new(1.0, 0.1, 0.0, 5.0);
        let result = engine
            .eval_with_context("integrate(value)", 10.0, ctx)
            .unwrap();
        assert!((result - 6.0).abs() < 0.0001);
    }

    #[test]
    fn test_smooth_function() {
        let engine = ScriptEngine::new();

        // Smooth with alpha=0.5, prev=10, current=20 => 0.5*10 + 0.5*20 = 15
        let ctx = ExecutionContext::new(1.0, 0.1, 20.0, 10.0);
        let result = engine
            .eval_with_context("smooth(value, 0.5)", 20.0, ctx)
            .unwrap();
        assert!((result - 15.0).abs() < 0.0001);
    }

    #[test]
    fn test_lowpass_function() {
        let engine = ScriptEngine::new();

        // Lowpass filter test - just verify it runs and returns reasonable value
        let ctx = ExecutionContext::new(1.0, 0.01, 10.0, 10.0);
        let result = engine
            .eval_with_context("lowpass(value, 10.0)", 20.0, ctx)
            .unwrap();
        // Result should be between prev (10) and current (20)
        assert!(result > 10.0 && result < 20.0);
    }

    #[test]
    fn test_deadband_function() {
        let engine = ScriptEngine::new();

        // Value within deadband should return center
        let result = engine.eval("deadband(10.5, 10.0, 2.0)", 0.0).unwrap();
        assert_eq!(result, 10.0);

        // Value outside deadband should return value
        let result = engine.eval("deadband(15.0, 10.0, 2.0)", 0.0).unwrap();
        assert_eq!(result, 15.0);
    }

    #[test]
    fn test_rate_limit_function() {
        let engine = ScriptEngine::new();

        // Rate limit: prev=10, current=20, max_rate=50, dt=0.1 => max change = 5
        // So result should be 10 + 5 = 15
        let ctx = ExecutionContext::new(1.0, 0.1, 10.0, 10.0);
        let result = engine
            .eval_with_context("rate_limit(value, 50.0)", 20.0, ctx)
            .unwrap();
        assert!((result - 15.0).abs() < 0.0001);

        // Small change within limit should pass through
        let ctx = ExecutionContext::new(1.0, 0.1, 10.0, 10.0);
        let result = engine
            .eval_with_context("rate_limit(value, 50.0)", 12.0, ctx)
            .unwrap();
        assert!((result - 12.0).abs() < 0.0001);
    }

    #[test]
    fn test_has_prev_function() {
        let engine = ScriptEngine::new();

        // With previous value
        let ctx = ExecutionContext::new(1.0, 0.1, 10.0, 10.0);
        let converter = engine
            .compile("test", "if has_prev() { 1.0 } else { 0.0 }")
            .unwrap();
        let result = engine.execute(&converter, 0.0, ctx).unwrap();
        assert_eq!(result, 1.0);

        // Without previous value
        let ctx = ExecutionContext::first_sample(1.0);
        let result = engine.execute(&converter, 0.0, ctx).unwrap();
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_prev_functions() {
        let engine = ScriptEngine::new();

        // Test prev() and prev_raw() return correct values
        let ctx = ExecutionContext::new(1.0, 0.1, 5.0, 10.0);

        let result = engine
            .eval_with_context("prev()", 0.0, ctx.clone())
            .unwrap();
        assert_eq!(result, 10.0);

        let result = engine.eval_with_context("prev_raw()", 0.0, ctx).unwrap();
        assert_eq!(result, 5.0);
    }

    #[test]
    fn test_time_based_script() {
        let engine = ScriptEngine::new();

        // Test using time in a conversion (e.g., sinusoidal test signal)
        let ctx = ExecutionContext::new(std::f64::consts::PI / 2.0, 0.01, f64::NAN, f64::NAN);
        let result = engine.eval_with_context("sin(time())", 0.0, ctx).unwrap();
        assert!((result - 1.0).abs() < 0.0001); // sin(PI/2) = 1
    }
}
