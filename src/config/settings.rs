//! Runtime settings that can be modified during application execution
//!
//! This module contains settings that may change during runtime,
//! separate from the persistent configuration. These settings control
//! the current state of data collection, display options, and triggers.
//!
//! # Main Types
//!
//! - [`RuntimeSettings`] - Current application state (collecting, paused, axis settings)
//! - [`TriggerSettings`] - Conditional capture configuration
//! - [`ExportSettings`] - Data export format options
//! - [`VariableDisplaySettings`] - Per-variable display preferences
//!
//! # Axis Control
//!
//! The runtime settings provide fine-grained control over plot axes:
//!
//! - **Autoscale X/Y**: Automatically adjust axis bounds to fit data
//! - **Lock X/Y**: Prevent user from zooming/panning an axis
//! - **Manual bounds**: Set explicit min/max values
//! - **Time window**: Control the visible time range with configurable max limit
//!
//! # Trigger System
//!
//! Triggers allow conditional data capture based on variable values:
//!
//! - Rising/falling edge detection
//! - Threshold crossing (above/below)
//! - Value change detection
//! - Pre/post trigger buffer capture

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Runtime settings for the application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSettings {
    /// Whether data collection is currently active
    pub collecting: bool,

    /// Whether the connection is paused
    pub paused: bool,

    /// Current time window for display in seconds
    pub display_time_window: f64,

    /// Y-axis minimum (if manual scaling)
    pub y_min: Option<f64>,

    /// Y-axis maximum (if manual scaling)
    pub y_max: Option<f64>,

    /// Whether to follow the latest data (auto-scroll)
    pub follow_latest: bool,

    /// Whether to auto-scale X axis (follow latest data)
    pub autoscale_x: bool,

    /// Whether to auto-scale Y axis (fit to visible data)
    pub autoscale_y: bool,

    /// Whether the X axis is locked (user cannot zoom/pan)
    pub lock_x: bool,

    /// Whether the Y axis is locked (user cannot zoom/pan)
    pub lock_y: bool,

    /// X-axis minimum (when not autoscaling)
    pub x_min: Option<f64>,

    /// X-axis maximum (when not autoscaling)
    pub x_max: Option<f64>,

    /// Maximum allowed time window in seconds
    pub max_time_window: f64,

    /// Trigger settings
    pub trigger: TriggerSettings,

    /// Export settings
    pub export: ExportSettings,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            collecting: false,
            paused: false,
            display_time_window: 10.0,
            y_min: None,
            y_max: None,
            follow_latest: true,
            autoscale_x: true,
            autoscale_y: true,
            lock_x: false,
            lock_y: false,
            x_min: None,
            x_max: None,
            max_time_window: 300.0,
            trigger: TriggerSettings::default(),
            export: ExportSettings::default(),
        }
    }
}

impl RuntimeSettings {
    /// Create new runtime settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Start data collection
    pub fn start_collection(&mut self) {
        self.collecting = true;
        self.paused = false;
    }

    /// Stop data collection
    pub fn stop_collection(&mut self) {
        self.collecting = false;
    }

    /// Toggle pause state
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    /// Set manual Y-axis range
    pub fn set_y_range(&mut self, min: f64, max: f64) {
        self.y_min = Some(min);
        self.y_max = Some(max);
    }

    /// Clear manual Y-axis range (enable auto-scaling)
    pub fn clear_y_range(&mut self) {
        self.y_min = None;
        self.y_max = None;
    }

    /// Check if Y-axis is using manual scaling
    pub fn is_manual_y_scale(&self) -> bool {
        self.y_min.is_some() && self.y_max.is_some()
    }

    /// Check if X-axis is using manual scaling
    pub fn is_manual_x_scale(&self) -> bool {
        self.x_min.is_some() && self.x_max.is_some()
    }

    /// Set manual X-axis range
    pub fn set_x_range(&mut self, min: f64, max: f64) {
        self.x_min = Some(min);
        self.x_max = Some(max);
        self.autoscale_x = false;
    }

    /// Clear manual X-axis range (enable auto-scaling)
    pub fn clear_x_range(&mut self) {
        self.x_min = None;
        self.x_max = None;
        self.autoscale_x = true;
    }

    /// Update time window from user zoom, respecting max limit
    pub fn update_time_window_from_zoom(&mut self, new_window: f64) {
        self.display_time_window = new_window.clamp(0.1, self.max_time_window);
    }

    /// Toggle X axis lock
    pub fn toggle_lock_x(&mut self) {
        self.lock_x = !self.lock_x;
    }

    /// Toggle Y axis lock
    pub fn toggle_lock_y(&mut self) {
        self.lock_y = !self.lock_y;
    }

    /// Toggle X axis autoscale
    pub fn toggle_autoscale_x(&mut self) {
        self.autoscale_x = !self.autoscale_x;
        if self.autoscale_x {
            self.follow_latest = true;
            self.x_min = None;
            self.x_max = None;
        }
    }

    /// Toggle Y axis autoscale
    pub fn toggle_autoscale_y(&mut self) {
        self.autoscale_y = !self.autoscale_y;
        if self.autoscale_y {
            self.y_min = None;
            self.y_max = None;
        }
    }
}

/// Trigger settings for conditional data capture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerSettings {
    /// Whether triggering is enabled
    pub enabled: bool,

    /// Variable ID to trigger on
    pub variable_id: Option<u32>,

    /// Trigger condition
    pub condition: TriggerCondition,

    /// Trigger threshold value
    pub threshold: f64,

    /// Pre-trigger buffer duration
    pub pre_trigger: Duration,

    /// Post-trigger capture duration
    pub post_trigger: Duration,

    /// Whether the trigger is currently armed
    pub armed: bool,

    /// Whether the trigger has fired
    pub triggered: bool,
}

impl Default for TriggerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            variable_id: None,
            condition: TriggerCondition::RisingEdge,
            threshold: 0.0,
            pre_trigger: Duration::from_millis(100),
            post_trigger: Duration::from_secs(1),
            armed: false,
            triggered: false,
        }
    }
}

impl TriggerSettings {
    /// Arm the trigger
    pub fn arm(&mut self) {
        self.armed = true;
        self.triggered = false;
    }

    /// Disarm the trigger
    pub fn disarm(&mut self) {
        self.armed = false;
        self.triggered = false;
    }

    /// Check if a value transition should trigger
    pub fn check_trigger(&mut self, previous: f64, current: f64) -> bool {
        if !self.enabled || !self.armed || self.triggered {
            return false;
        }

        let should_trigger = match self.condition {
            TriggerCondition::RisingEdge => previous < self.threshold && current >= self.threshold,
            TriggerCondition::FallingEdge => previous > self.threshold && current <= self.threshold,
            TriggerCondition::Above => current > self.threshold,
            TriggerCondition::Below => current < self.threshold,
            TriggerCondition::Equal => (current - self.threshold).abs() < f64::EPSILON,
            TriggerCondition::Change => (current - previous).abs() > self.threshold,
        };

        if should_trigger {
            self.triggered = true;
        }

        should_trigger
    }

    /// Reset the trigger state
    pub fn reset(&mut self) {
        self.armed = false;
        self.triggered = false;
    }
}

/// Trigger condition types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerCondition {
    /// Trigger when value crosses threshold going up
    RisingEdge,
    /// Trigger when value crosses threshold going down
    FallingEdge,
    /// Trigger when value is above threshold
    Above,
    /// Trigger when value is below threshold
    Below,
    /// Trigger when value equals threshold
    Equal,
    /// Trigger when value changes by more than threshold
    Change,
}

impl std::fmt::Display for TriggerCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TriggerCondition::RisingEdge => write!(f, "Rising Edge"),
            TriggerCondition::FallingEdge => write!(f, "Falling Edge"),
            TriggerCondition::Above => write!(f, "Above"),
            TriggerCondition::Below => write!(f, "Below"),
            TriggerCondition::Equal => write!(f, "Equal"),
            TriggerCondition::Change => write!(f, "Change"),
        }
    }
}

/// Export settings for data export functionality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportSettings {
    /// Include timestamps in export
    pub include_timestamps: bool,

    /// Include raw values in export
    pub include_raw_values: bool,

    /// Include converted values in export
    pub include_converted_values: bool,

    /// Timestamp format for export
    pub timestamp_format: TimestampFormat,

    /// Decimal separator for CSV export
    pub decimal_separator: char,

    /// Field separator for CSV export
    pub field_separator: char,

    /// Include header row in CSV export
    pub include_header: bool,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            include_timestamps: true,
            include_raw_values: false,
            include_converted_values: true,
            timestamp_format: TimestampFormat::Seconds,
            decimal_separator: '.',
            field_separator: ',',
            include_header: true,
        }
    }
}

/// Timestamp format options for export
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimestampFormat {
    /// Seconds since start (floating point)
    Seconds,
    /// Milliseconds since start (integer)
    Milliseconds,
    /// Microseconds since start (integer)
    Microseconds,
    /// ISO 8601 datetime format
    Iso8601,
    /// Unix timestamp (seconds since epoch)
    UnixTimestamp,
}

impl std::fmt::Display for TimestampFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimestampFormat::Seconds => write!(f, "Seconds"),
            TimestampFormat::Milliseconds => write!(f, "Milliseconds"),
            TimestampFormat::Microseconds => write!(f, "Microseconds"),
            TimestampFormat::Iso8601 => write!(f, "ISO 8601"),
            TimestampFormat::UnixTimestamp => write!(f, "Unix Timestamp"),
        }
    }
}

/// Display settings for individual variables
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableDisplaySettings {
    /// Variable ID
    pub variable_id: u32,

    /// Whether to show this variable in the main plot
    pub show_in_plot: bool,

    /// Whether to show this variable in the data table
    pub show_in_table: bool,

    /// Custom Y-axis for this variable (if separate axes are used)
    pub y_axis_index: usize,

    /// Display precision (decimal places)
    pub precision: usize,

    /// Number format
    pub number_format: NumberFormat,
}

impl Default for VariableDisplaySettings {
    fn default() -> Self {
        Self {
            variable_id: 0,
            show_in_plot: true,
            show_in_table: true,
            y_axis_index: 0,
            precision: 3,
            number_format: NumberFormat::Decimal,
        }
    }
}

/// Number format options for display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumberFormat {
    /// Decimal format (e.g., 123.456)
    Decimal,
    /// Scientific notation (e.g., 1.23e2)
    Scientific,
    /// Engineering notation (e.g., 123.4e0)
    Engineering,
    /// Hexadecimal format (for raw values)
    Hexadecimal,
    /// Binary format (for raw values)
    Binary,
}

impl std::fmt::Display for NumberFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NumberFormat::Decimal => write!(f, "Decimal"),
            NumberFormat::Scientific => write!(f, "Scientific"),
            NumberFormat::Engineering => write!(f, "Engineering"),
            NumberFormat::Hexadecimal => write!(f, "Hexadecimal"),
            NumberFormat::Binary => write!(f, "Binary"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_settings_default() {
        let settings = RuntimeSettings::default();
        assert!(!settings.collecting);
        assert!(!settings.paused);
        assert!(settings.follow_latest);
    }

    #[test]
    fn test_trigger_rising_edge() {
        let mut trigger = TriggerSettings {
            enabled: true,
            threshold: 1.0,
            condition: TriggerCondition::RisingEdge,
            ..Default::default()
        };
        trigger.arm();

        // Should not trigger when below threshold
        assert!(!trigger.check_trigger(0.0, 0.5));
        // Should trigger when crossing threshold
        assert!(trigger.check_trigger(0.5, 1.5));
        // Should not trigger again (already triggered)
        assert!(!trigger.check_trigger(0.5, 1.5));
    }

    #[test]
    fn test_trigger_falling_edge() {
        let mut trigger = TriggerSettings {
            enabled: true,
            threshold: 1.0,
            condition: TriggerCondition::FallingEdge,
            ..Default::default()
        };
        trigger.arm();

        // Should not trigger when above threshold
        assert!(!trigger.check_trigger(2.0, 1.5));
        // Should trigger when crossing threshold going down
        assert!(trigger.check_trigger(1.5, 0.5));
    }

    #[test]
    fn test_y_range() {
        let mut settings = RuntimeSettings::default();
        assert!(!settings.is_manual_y_scale());

        settings.set_y_range(-10.0, 10.0);
        assert!(settings.is_manual_y_scale());
        assert_eq!(settings.y_min, Some(-10.0));
        assert_eq!(settings.y_max, Some(10.0));

        settings.clear_y_range();
        assert!(!settings.is_manual_y_scale());
    }

    #[test]
    fn test_x_range() {
        let mut settings = RuntimeSettings::default();
        assert!(!settings.is_manual_x_scale());
        assert!(settings.autoscale_x);

        settings.set_x_range(0.0, 100.0);
        assert!(settings.is_manual_x_scale());
        assert!(!settings.autoscale_x);
        assert_eq!(settings.x_min, Some(0.0));
        assert_eq!(settings.x_max, Some(100.0));

        settings.clear_x_range();
        assert!(!settings.is_manual_x_scale());
        assert!(settings.autoscale_x);
    }

    #[test]
    fn test_time_window_from_zoom() {
        let mut settings = RuntimeSettings::default();
        assert_eq!(settings.max_time_window, 300.0);

        // Normal zoom
        settings.update_time_window_from_zoom(50.0);
        assert_eq!(settings.display_time_window, 50.0);

        // Zoom beyond max should clamp
        settings.update_time_window_from_zoom(500.0);
        assert_eq!(settings.display_time_window, 300.0);

        // Zoom too small should clamp
        settings.update_time_window_from_zoom(0.01);
        assert_eq!(settings.display_time_window, 0.1);
    }

    #[test]
    fn test_axis_locks() {
        let mut settings = RuntimeSettings::default();
        assert!(!settings.lock_x);
        assert!(!settings.lock_y);

        settings.toggle_lock_x();
        assert!(settings.lock_x);

        settings.toggle_lock_y();
        assert!(settings.lock_y);

        settings.toggle_lock_x();
        assert!(!settings.lock_x);
    }

    #[test]
    fn test_autoscale_toggles() {
        let mut settings = RuntimeSettings::default();
        assert!(settings.autoscale_x);
        assert!(settings.autoscale_y);

        // Toggle X autoscale off
        settings.toggle_autoscale_x();
        assert!(!settings.autoscale_x);

        // Toggle back on should clear manual bounds and enable follow_latest
        settings.x_min = Some(0.0);
        settings.x_max = Some(100.0);
        settings.follow_latest = false;
        settings.toggle_autoscale_x();
        assert!(settings.autoscale_x);
        assert!(settings.follow_latest);
        assert!(settings.x_min.is_none());
        assert!(settings.x_max.is_none());

        // Toggle Y autoscale off
        settings.toggle_autoscale_y();
        assert!(!settings.autoscale_y);

        // Toggle back on should clear manual bounds
        settings.y_min = Some(-10.0);
        settings.y_max = Some(10.0);
        settings.toggle_autoscale_y();
        assert!(settings.autoscale_y);
        assert!(settings.y_min.is_none());
        assert!(settings.y_max.is_none());
    }
}
