//! Core data types for DataVis-RS
//!
//! This module contains the fundamental data structures used throughout
//! the application for representing variables, data points, and their types.
//!
//! # Main Types
//!
//! - [`VariableType`] - Enum of supported variable types (u8, u16, u32, f32, etc.)
//! - [`Variable`] - Configuration for a variable to observe (address, type, converter)
//! - [`DataPoint`] - A single timestamped value with raw and converted forms
//! - [`VariableData`] - Time-series storage for a variable with statistics
//!
//! # Variable Types
//!
//! Supports all common embedded data types:
//! - Unsigned integers: u8, u16, u32, u64
//! - Signed integers: i8, i16, i32, i64
//! - Floating point: f32, f64
//! - Boolean values
//! - Raw byte arrays (for custom interpretation)
//!
//! # Statistics
//!
//! The [`IncrementalStats`] type provides O(1) updates for running statistics
//! including min, max, and average values as data arrives.
//!
//! # Memory Management
//!
//! Variable data is stored in a ring buffer with a configurable maximum size
//! ([`MAX_DATA_POINTS`]). When the buffer is full, old data is evicted
//! automatically.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Maximum number of data points to retain in memory per variable
pub const MAX_DATA_POINTS: usize = 100_000;

/// Maximum number of points to render per line for performance
pub const MAX_RENDER_POINTS: usize = 2000;

/// Represents the type of a variable being observed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VariableType {
    /// 8-bit unsigned integer
    U8,
    /// 16-bit unsigned integer
    U16,
    /// 32-bit unsigned integer
    #[default]
    U32,
    /// 64-bit unsigned integer
    U64,
    /// 8-bit signed integer
    I8,
    /// 16-bit signed integer
    I16,
    /// 32-bit signed integer
    I32,
    /// 64-bit signed integer
    I64,
    /// 32-bit floating point
    F32,
    /// 64-bit floating point
    F64,
    /// Boolean value
    Bool,
    /// Raw bytes (for custom interpretation)
    Raw(usize),
}

impl VariableType {
    /// Returns the size in bytes of this variable type
    pub fn size_bytes(&self) -> usize {
        match self {
            VariableType::U8 | VariableType::I8 | VariableType::Bool => 1,
            VariableType::U16 | VariableType::I16 => 2,
            VariableType::U32 | VariableType::I32 | VariableType::F32 => 4,
            VariableType::U64 | VariableType::I64 | VariableType::F64 => 8,
            VariableType::Raw(size) => *size,
        }
    }

    /// Returns true if this type is writable (primitive types, not Raw)
    /// Raw types cannot be written because we don't know how to convert a f64 to them
    pub fn is_writable(&self) -> bool {
        !matches!(self, VariableType::Raw(_))
    }

    /// Parse raw bytes into a f64 value for plotting
    pub fn parse_to_f64(&self, bytes: &[u8]) -> Option<f64> {
        if bytes.len() < self.size_bytes() {
            return None;
        }

        Some(match self {
            VariableType::U8 => bytes[0] as f64,
            VariableType::I8 => bytes[0] as i8 as f64,
            VariableType::Bool => {
                if bytes[0] != 0 {
                    1.0
                } else {
                    0.0
                }
            }
            VariableType::U16 => u16::from_le_bytes([bytes[0], bytes[1]]) as f64,
            VariableType::I16 => i16::from_le_bytes([bytes[0], bytes[1]]) as f64,
            VariableType::U32 => {
                u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64
            }
            VariableType::I32 => {
                i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64
            }
            VariableType::F32 => {
                f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64
            }
            VariableType::U64 => u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]) as f64,
            VariableType::I64 => i64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]) as f64,
            VariableType::F64 => f64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
            VariableType::Raw(_) => {
                // For raw bytes, just return the first byte as a value
                bytes[0] as f64
            }
        })
    }
}

impl std::fmt::Display for VariableType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VariableType::U8 => write!(f, "u8"),
            VariableType::U16 => write!(f, "u16"),
            VariableType::U32 => write!(f, "u32"),
            VariableType::U64 => write!(f, "u64"),
            VariableType::I8 => write!(f, "i8"),
            VariableType::I16 => write!(f, "i16"),
            VariableType::I32 => write!(f, "i32"),
            VariableType::I64 => write!(f, "i64"),
            VariableType::F32 => write!(f, "f32"),
            VariableType::F64 => write!(f, "f64"),
            VariableType::Bool => write!(f, "bool"),
            VariableType::Raw(size) => write!(f, "{} bytes", size),
        }
    }
}

/// Visual style for plotting data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlotStyle {
    /// Standard line plot (default)
    #[default]
    Line,
    /// Scatter plot showing individual data points
    Scatter,
    /// Step plot with horizontal-then-vertical transitions
    Step,
    /// Area plot filled to the X-axis
    Area,
}

impl PlotStyle {
    /// Get all available plot styles
    pub fn all() -> &'static [PlotStyle] {
        &[PlotStyle::Line, PlotStyle::Scatter, PlotStyle::Step, PlotStyle::Area]
    }

    /// Get display name for this plot style
    pub fn display_name(&self) -> &'static str {
        match self {
            PlotStyle::Line => "Line",
            PlotStyle::Scatter => "Scatter",
            PlotStyle::Step => "Step",
            PlotStyle::Area => "Area",
        }
    }

    /// Get icon character for this plot style
    pub fn icon(&self) -> &'static str {
        match self {
            PlotStyle::Line => "─",
            PlotStyle::Scatter => "•",
            PlotStyle::Step => "⌐",
            PlotStyle::Area => "▄",
        }
    }

    /// Get the next plot style (for cycling)
    pub fn next(&self) -> PlotStyle {
        match self {
            PlotStyle::Line => PlotStyle::Scatter,
            PlotStyle::Scatter => PlotStyle::Step,
            PlotStyle::Step => PlotStyle::Area,
            PlotStyle::Area => PlotStyle::Line,
        }
    }
}

/// A single data point with timestamp and value
#[derive(Debug, Clone)]
pub struct DataPoint {
    /// Timestamp when the data point was captured (relative to start)
    pub timestamp: Duration,
    /// The raw value read from memory
    pub raw_value: f64,
    /// The converted value (after applying Rhai script, if any)
    pub converted_value: f64,
}

impl DataPoint {
    /// Create a new data point with the same raw and converted value
    pub fn new(timestamp: Duration, value: f64) -> Self {
        Self {
            timestamp,
            raw_value: value,
            converted_value: value,
        }
    }

    /// Create a new data point with separate raw and converted values
    pub fn with_conversion(timestamp: Duration, raw_value: f64, converted_value: f64) -> Self {
        Self {
            timestamp,
            raw_value,
            converted_value,
        }
    }

    /// Create a gap marker (NaN values) to break line continuity in plots
    ///
    /// Used when resuming from pause to prevent drawing a line across
    /// the time gap when data wasn't being collected.
    pub fn gap_marker(timestamp: Duration) -> Self {
        Self {
            timestamp,
            raw_value: f64::NAN,
            converted_value: f64::NAN,
        }
    }

    /// Check if this data point is a gap marker
    pub fn is_gap(&self) -> bool {
        self.raw_value.is_nan() || self.converted_value.is_nan()
    }
}

/// Metadata for pointer variables to track dereferencing state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointerMetadata {
    /// Last read pointer value (address it points to)
    pub cached_address: Option<u64>,

    /// Timestamp of last pointer read (not serialized)
    #[serde(skip)]
    pub last_pointer_read: Option<Instant>,

    /// How often to re-read pointer value (Hz), separate from data poll rate
    pub pointer_poll_rate_hz: u32,

    /// ID of parent pointer variable (for pointed-to members)
    pub pointer_parent_id: Option<u32>,

    /// Offset from dereferenced pointer (for struct members)
    pub offset_from_pointer: u64,

    /// Pointer validity state
    pub pointer_state: PointerState,
}

/// State of a pointer value
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PointerState {
    /// Never read yet
    Unread,
    /// Last read value (non-NULL)
    Valid(u64),
    /// Pointer is NULL
    Null,
    /// Suspicious value (unaligned, out of range)
    Invalid(u64),
    /// Failed to read pointer
    ReadError,
}

/// Configuration for a variable to observe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    /// Unique identifier for this variable
    pub id: u32,
    /// Human-readable name
    pub name: String,
    /// Memory address to read from
    pub address: u64,
    /// Type of the variable
    pub var_type: VariableType,
    /// Optional Rhai script for value conversion
    pub converter_script: Option<String>,
    /// Color for plotting (RGBA)
    pub color: [u8; 4],
    /// Whether this variable is currently being observed (sampled)
    pub enabled: bool,
    /// Whether this variable is shown in the graph
    pub show_in_graph: bool,
    /// Unit label for display (e.g., "V", "mA", "°C")
    pub unit: String,
    /// Polling rate for this variable in Hz (0 = use global rate)
    pub poll_rate_hz: u32,
    /// Y-axis assignment (0 = primary left axis, 1 = secondary right axis)
    #[serde(default)]
    pub y_axis: u8,
    /// Visual style for plotting this variable
    #[serde(default)]
    pub plot_style: PlotStyle,
    /// Parent variable ID (None = root-level variable)
    #[serde(default)]
    pub parent_id: Option<u32>,
    /// Pointer metadata for runtime dereferencing (Phase 2 feature)
    #[serde(default)]
    pub pointer_metadata: Option<PointerMetadata>,
}

impl Default for Variable {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::from("Unnamed"),
            address: 0,
            var_type: VariableType::U32,
            converter_script: None,
            color: [0, 0, 0, 255], // Default to black (visible on light themes)
            enabled: true,
            show_in_graph: true,
            unit: String::new(),
            poll_rate_hz: 0,
            y_axis: 0, // Primary (left) axis by default
            plot_style: PlotStyle::default(),
            parent_id: None,
            pointer_metadata: None,
        }
    }
}

/// Global counter for generating unique variable IDs
static NEXT_VARIABLE_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

impl Variable {
    /// Create a new variable with the given parameters
    /// Automatically assigns a distinct color based on the variable ID
    pub fn new(name: impl Into<String>, address: u64, var_type: VariableType) -> Self {
        let id = NEXT_VARIABLE_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self {
            id,
            name: name.into(),
            address,
            var_type,
            color: Self::generate_color(id),
            ..Default::default()
        }
    }

    /// Set the converter script
    pub fn with_converter(mut self, script: impl Into<String>) -> Self {
        self.converter_script = Some(script.into());
        self
    }

    /// Set the display color
    pub fn with_color(mut self, color: [u8; 4]) -> Self {
        self.color = color;
        self
    }

    /// Set the unit label
    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = unit.into();
        self
    }

    /// Set the poll rate
    pub fn with_poll_rate(mut self, hz: u32) -> Self {
        self.poll_rate_hz = hz;
        self
    }

    /// Set whether to show in graph
    pub fn with_show_in_graph(mut self, show: bool) -> Self {
        self.show_in_graph = show;
        self
    }

    /// Set the Y-axis assignment (0 = primary/left, 1 = secondary/right)
    pub fn with_y_axis(mut self, axis: u8) -> Self {
        self.y_axis = axis;
        self
    }

    /// Set the plot style for visualization
    pub fn with_plot_style(mut self, style: PlotStyle) -> Self {
        self.plot_style = style;
        self
    }

    /// Set an auto-generated color based on the variable ID
    /// This generates distinct, visually pleasing colors that work on both
    /// light and dark themes by using medium-saturation, medium-brightness colors
    pub fn with_auto_color(mut self) -> Self {
        self.color = Self::generate_color(self.id);
        self
    }

    /// Generate a distinct color based on an index/ID
    /// Uses the golden ratio to spread hues evenly across the color wheel
    pub fn generate_color(index: u32) -> [u8; 4] {
        // Use golden ratio conjugate for optimal hue distribution
        const GOLDEN_RATIO: f32 = 0.618033988749895;

        // Start with a nice initial hue and spread using golden ratio
        let hue = ((index as f32 * GOLDEN_RATIO) % 1.0) * 360.0;

        // Use medium saturation and value for visibility on both light and dark themes
        let saturation = 0.7;
        let value = 0.85;

        let (r, g, b) = hsv_to_rgb(hue, saturation, value);
        [r, g, b, 255]
    }

    /// Generate a shade/tint of a base color for child variable differentiation.
    /// `child_index` determines the shade variant.
    /// Uses HSV adjustments: varies saturation and value around the parent's base color.
    pub fn generate_child_color(parent_color: [u8; 4], child_index: usize, child_count: usize) -> [u8; 4] {
        let (h, _s, _v) = rgb_to_hsv(parent_color);
        let count = child_count.max(1) as f32;
        let t = if child_count <= 1 { 0.5 } else { child_index as f32 / (count - 1.0) };
        // Vary saturation from 0.4 to 0.9 and value from 1.0 to 0.6
        let new_s = (0.4 + t * 0.5).clamp(0.3, 0.95);
        let new_v = (1.0 - t * 0.4).clamp(0.5, 1.0);
        let (r, g, b) = hsv_to_rgb(h, new_s, new_v);
        [r, g, b, 255]
    }

    /// Check if this variable is a root-level variable (no parent)
    pub fn is_root(&self) -> bool {
        self.parent_id.is_none()
    }

    /// Check if this variable can be written to
    /// A variable is writable if:
    /// - Its type is a primitive (not Raw)
    /// - It has no converter script (converters are one-way, read-only transformations)
    pub fn is_writable(&self) -> bool {
        self.var_type.is_writable() && self.converter_script.is_none()
    }

    /// Synchronize the NEXT_VARIABLE_ID counter with existing variables
    ///
    /// This should be called after loading variables from a saved project
    /// to ensure new variables get IDs that don't conflict with loaded ones.
    pub fn sync_next_id(variables: &std::collections::HashMap<u32, Variable>) {
        if let Some(max_id) = variables.values().map(|v| v.id).max() {
            // Set NEXT_VARIABLE_ID to max + 1, but only if it would increase the counter
            let mut current = NEXT_VARIABLE_ID.load(std::sync::atomic::Ordering::SeqCst);
            while current <= max_id {
                match NEXT_VARIABLE_ID.compare_exchange(
                    current,
                    max_id + 1,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                ) {
                    Ok(_) => break,
                    Err(new_current) => current = new_current,
                }
            }
        }
    }
}

/// Incremental statistics tracker using Welford's online algorithm
/// This allows O(1) updates for mean, min, and max without iterating all data
#[derive(Debug, Clone, Default)]
pub struct IncrementalStats {
    /// Current count of values
    pub count: u64,
    /// Running sum for average calculation
    pub sum: f64,
    /// Current minimum value
    pub min: f64,
    /// Current maximum value
    pub max: f64,
}

impl IncrementalStats {
    /// Create a new empty stats tracker
    pub fn new() -> Self {
        Self {
            count: 0,
            sum: 0.0,
            min: f64::MAX,
            max: f64::MIN,
        }
    }

    /// Add a new value to the statistics
    #[inline]
    pub fn push(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;
        self.min = self.min.min(value);
        self.max = self.max.max(value);
    }

    /// Remove a value from the statistics (when evicting from ring buffer)
    /// Note: min/max become approximate after removal, but this is acceptable
    /// for a rolling window. They will self-correct as new data comes in.
    #[inline]
    pub fn pop(&mut self, value: f64) {
        if self.count > 0 {
            self.count -= 1;
            self.sum -= value;
        }
    }

    /// Get the current average
    #[inline]
    pub fn average(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum / self.count as f64
        }
    }

    /// Get statistics as (min, max, avg) tuple
    #[inline]
    pub fn as_tuple(&self) -> (f64, f64, f64) {
        if self.count == 0 {
            (0.0, 0.0, 0.0)
        } else {
            (self.min, self.max, self.average())
        }
    }

    /// Reset the statistics
    pub fn reset(&mut self) {
        self.count = 0;
        self.sum = 0.0;
        self.min = f64::MAX;
        self.max = f64::MIN;
    }

    /// Recalculate min/max from data points (called periodically to fix drift)
    pub fn recalculate_minmax(&mut self, data_points: &VecDeque<DataPoint>) {
        if data_points.is_empty() {
            self.min = f64::MAX;
            self.max = f64::MIN;
        } else {
            self.min = f64::MAX;
            self.max = f64::MIN;
            for dp in data_points {
                self.min = self.min.min(dp.converted_value);
                self.max = self.max.max(dp.converted_value);
            }
        }
    }
}

/// Time series data storage for a variable
#[derive(Debug)]
pub struct VariableData {
    /// The variable configuration
    pub variable: Variable,
    /// Ring buffer of data points
    pub data_points: VecDeque<DataPoint>,
    /// Time when data collection started
    pub start_time: Instant,
    /// Last received value
    pub last_value: Option<f64>,
    /// Last converted value
    pub last_converted_value: Option<f64>,
    /// Number of read errors
    pub error_count: u64,
    /// Last error message
    pub last_error: Option<String>,
    /// Incremental statistics for efficient average calculation
    pub stats: IncrementalStats,
    /// Counter for periodic min/max recalculation (every N pops)
    stats_recalc_counter: u32,
}

/// How often to recalculate exact min/max (every N evictions from ring buffer)
const STATS_RECALC_INTERVAL: u32 = 1000;

impl VariableData {
    /// Create new variable data storage
    pub fn new(variable: Variable) -> Self {
        Self {
            variable,
            data_points: VecDeque::with_capacity(MAX_DATA_POINTS),
            start_time: Instant::now(),
            last_value: None,
            last_converted_value: None,
            error_count: 0,
            last_error: None,
            stats: IncrementalStats::new(),
            stats_recalc_counter: 0,
        }
    }

    /// Add a new data point, removing old ones if necessary
    pub fn push(&mut self, point: DataPoint) {
        self.last_value = Some(point.raw_value);
        self.last_converted_value = Some(point.converted_value);

        // Update incremental stats with new value
        self.stats.push(point.converted_value);

        // If we need to evict an old value, update stats
        if self.data_points.len() >= MAX_DATA_POINTS {
            if let Some(old) = self.data_points.pop_front() {
                self.stats.pop(old.converted_value);
                self.stats_recalc_counter += 1;

                // Periodically recalculate exact min/max to correct any drift
                if self.stats_recalc_counter >= STATS_RECALC_INTERVAL {
                    self.stats_recalc_counter = 0;
                    // We need to recalc after pushing the new point
                }
            }
        }

        self.data_points.push_back(point);

        // Do the recalculation after the new point is added
        if self.stats_recalc_counter == 0 && self.data_points.len() == MAX_DATA_POINTS {
            self.stats.recalculate_minmax(&self.data_points);
        }
    }

    /// Clear all data points and reset error state
    pub fn clear(&mut self) {
        self.data_points.clear();
        self.last_value = None;
        self.last_converted_value = None;
        self.start_time = Instant::now();
        self.stats.reset();
        self.stats_recalc_counter = 0;
        self.clear_errors();
    }

    /// Get data points as plot points (time in seconds, converted value)
    pub fn as_plot_points(&self) -> Vec<[f64; 2]> {
        self.data_points
            .iter()
            .map(|dp| [dp.timestamp.as_secs_f64(), dp.converted_value])
            .collect()
    }

    /// Get the time range of the data
    pub fn time_range(&self) -> Option<(f64, f64)> {
        if self.data_points.is_empty() {
            return None;
        }
        let first = self.data_points.front()?.timestamp.as_secs_f64();
        let last = self.data_points.back()?.timestamp.as_secs_f64();
        Some((first, last))
    }

    /// Get the value range of the data
    pub fn value_range(&self) -> Option<(f64, f64)> {
        if self.data_points.is_empty() {
            return None;
        }
        let mut min = f64::MAX;
        let mut max = f64::MIN;
        for dp in &self.data_points {
            min = min.min(dp.converted_value);
            max = max.max(dp.converted_value);
        }
        Some((min, max))
    }

    /// Record an error
    pub fn record_error(&mut self, error: impl Into<String>) {
        self.error_count += 1;
        self.last_error = Some(error.into());
    }

    /// Check if there have been any errors
    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    /// Get the error rate as a percentage (errors / total reads)
    pub fn error_rate(&self) -> f64 {
        let total = self.data_points.len() as u64 + self.error_count;
        if total == 0 {
            0.0
        } else {
            (self.error_count as f64 / total as f64) * 100.0
        }
    }

    /// Clear error state (e.g., when clearing data)
    pub fn clear_errors(&mut self) {
        self.error_count = 0;
        self.last_error = None;
    }

    /// Get the last data point
    pub fn last(&self) -> Option<&DataPoint> {
        self.data_points.back()
    }

    /// Get statistics: (min, max, average)
    /// Uses O(1) incremental statistics instead of iterating all data points
    #[inline]
    pub fn statistics(&self) -> (f64, f64, f64) {
        self.stats.as_tuple()
    }

    /// Force recalculation of exact statistics from all data points
    /// Use this if you need guaranteed accurate min/max values
    pub fn recalculate_statistics(&mut self) -> (f64, f64, f64) {
        if self.data_points.is_empty() {
            self.stats.reset();
            return (0.0, 0.0, 0.0);
        }

        self.stats.reset();
        for dp in &self.data_points {
            self.stats.push(dp.converted_value);
        }
        self.stats.as_tuple()
    }
}

/// Represents the connection status to the debug probe
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionStatus {
    /// Not connected to any probe
    #[default]
    Disconnected,
    /// Attempting to connect
    Connecting,
    /// Connected and ready
    Connected,
    /// Connection error occurred
    Error,
}

impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionStatus::Disconnected => write!(f, "Disconnected"),
            ConnectionStatus::Connecting => write!(f, "Connecting..."),
            ConnectionStatus::Connected => write!(f, "Connected"),
            ConnectionStatus::Error => write!(f, "Error"),
        }
    }
}

/// Statistics about the data collection
#[derive(Debug, Clone, Default)]
pub struct CollectionStats {
    /// Number of successful reads
    pub successful_reads: u64,
    /// Number of failed reads
    pub failed_reads: u64,
    /// Average read time in microseconds
    pub avg_read_time_us: f64,
    /// Current effective sample rate in Hz
    pub effective_sample_rate: f64,
    /// Total bytes read
    pub total_bytes_read: u64,
    /// Current memory access mode name (for display)
    pub memory_access_mode: String,
    /// Number of messages dropped due to queue backpressure
    pub dropped_messages: u64,

    // Latency tracking
    /// Minimum read latency in recent window (microseconds)
    pub min_latency_us: u64,
    /// Maximum read latency in recent window (microseconds)
    pub max_latency_us: u64,
    /// Latency jitter (max - min) in microseconds
    pub jitter_us: u64,

    // Bulk read stats
    /// Number of bulk read operations performed
    pub bulk_reads: u64,
    /// Number of individual reads saved by bulk optimization
    pub reads_saved_by_bulk: u64,
}

impl CollectionStats {
    /// Calculate the success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        let total = self.successful_reads + self.failed_reads;
        if total == 0 {
            100.0
        } else {
            (self.successful_reads as f64 / total as f64) * 100.0
        }
    }
}

/// Convert RGB color (u8 array) to HSV (hue 0-360, saturation 0-1, value 0-1)
fn rgb_to_hsv(color: [u8; 4]) -> (f32, f32, f32) {
    let r = color[0] as f32 / 255.0;
    let g = color[1] as f32 / 255.0;
    let b = color[2] as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let hue = if delta < f32::EPSILON {
        0.0
    } else if (max - r).abs() < f32::EPSILON {
        60.0 * (((g - b) / delta) % 6.0)
    } else if (max - g).abs() < f32::EPSILON {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };
    let hue = if hue < 0.0 { hue + 360.0 } else { hue };

    let saturation = if max < f32::EPSILON { 0.0 } else { delta / max };
    let value = max;

    (hue, saturation, value)
}

/// Convert HSV (hue 0-360, saturation 0-1, value 0-1) to RGB (u8, u8, u8)
fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> (u8, u8, u8) {
    let c = value * saturation;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = value - c;

    let (r, g, b) = match (hue / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variable_type_size() {
        assert_eq!(VariableType::U8.size_bytes(), 1);
        assert_eq!(VariableType::U16.size_bytes(), 2);
        assert_eq!(VariableType::U32.size_bytes(), 4);
        assert_eq!(VariableType::U64.size_bytes(), 8);
        assert_eq!(VariableType::F32.size_bytes(), 4);
        assert_eq!(VariableType::F64.size_bytes(), 8);
        assert_eq!(VariableType::Raw(16).size_bytes(), 16);
    }

    #[test]
    fn test_variable_type_parse() {
        let bytes_u32: [u8; 4] = 1000u32.to_le_bytes();
        assert_eq!(VariableType::U32.parse_to_f64(&bytes_u32), Some(1000.0));

        let bytes_f32: [u8; 4] = 3.14f32.to_le_bytes();
        let parsed = VariableType::F32.parse_to_f64(&bytes_f32).unwrap();
        assert!((parsed - 3.14).abs() < 0.001);
    }

    #[test]
    fn test_variable_data_ring_buffer() {
        let var = Variable::new("test", 0x2000_0000, VariableType::U32);
        let mut data = VariableData::new(var);

        // Add more than MAX_DATA_POINTS
        for i in 0..(MAX_DATA_POINTS + 100) {
            data.push(DataPoint::new(Duration::from_millis(i as u64), i as f64));
        }

        // Should be capped at MAX_DATA_POINTS
        assert_eq!(data.data_points.len(), MAX_DATA_POINTS);
    }

    #[test]
    fn test_incremental_stats() {
        let var = Variable::new("test", 0x2000_0000, VariableType::U32);
        let mut data = VariableData::new(var);

        // Add some data points
        for i in 1..=100 {
            data.push(DataPoint::new(Duration::from_millis(i), i as f64));
        }

        let (min, max, avg) = data.statistics();
        assert_eq!(min, 1.0);
        assert_eq!(max, 100.0);
        assert!((avg - 50.5).abs() < 0.001); // Average of 1..=100
    }

    #[test]
    fn test_incremental_stats_with_eviction() {
        let var = Variable::new("test", 0x2000_0000, VariableType::U32);
        let mut data = VariableData::new(var);

        // Fill the buffer beyond capacity to trigger eviction
        for i in 1..=(MAX_DATA_POINTS + 100) {
            data.push(DataPoint::new(Duration::from_millis(i as u64), i as f64));
        }

        // Should be capped at MAX_DATA_POINTS
        assert_eq!(data.data_points.len(), MAX_DATA_POINTS);

        // Stats should still be tracked correctly
        let (min, max, avg) = data.statistics();

        // Max should be the most recent value
        assert_eq!(max, (MAX_DATA_POINTS + 100) as f64);

        // Average should be reasonable (not NaN or infinite)
        // The sum is decremented on pop, so average should be correct
        assert!(avg.is_finite());
        assert!(avg > 0.0);

        // Min might be stale (only recalculated periodically), but should be valid
        // It should be at least 1.0 (the initial minimum we saw)
        assert!(min >= 1.0);
        assert!(min.is_finite());

        // Force recalculation and verify min is now correct
        data.recalculate_statistics();
        let (min_after, _, _) = data.statistics();
        // After recalc, min should be the actual minimum in the buffer: 101
        assert_eq!(min_after, 101.0);
    }

    #[test]
    fn test_variable_type_is_writable() {
        // Primitive types should be writable
        assert!(VariableType::U8.is_writable());
        assert!(VariableType::U16.is_writable());
        assert!(VariableType::U32.is_writable());
        assert!(VariableType::U64.is_writable());
        assert!(VariableType::I8.is_writable());
        assert!(VariableType::I16.is_writable());
        assert!(VariableType::I32.is_writable());
        assert!(VariableType::I64.is_writable());
        assert!(VariableType::F32.is_writable());
        assert!(VariableType::F64.is_writable());
        assert!(VariableType::Bool.is_writable());

        // Raw types should not be writable
        assert!(!VariableType::Raw(4).is_writable());
        assert!(!VariableType::Raw(16).is_writable());
        assert!(!VariableType::Raw(0).is_writable());
    }

    #[test]
    fn test_variable_is_writable() {
        // Variable with primitive type and no converter should be writable
        let var = Variable::new("test", 0x2000_0000, VariableType::U32);
        assert!(var.is_writable());

        // Variable with converter should not be writable
        let var_with_converter =
            Variable::new("test2", 0x2000_0004, VariableType::U32).with_converter("value * 2");
        assert!(!var_with_converter.is_writable());

        // Variable with Raw type should not be writable
        let var_raw = Variable::new("test3", 0x2000_0008, VariableType::Raw(16));
        assert!(!var_raw.is_writable());

        // Variable with Raw type AND converter should not be writable
        let var_raw_converter =
            Variable::new("test4", 0x2000_000C, VariableType::Raw(4)).with_converter("value");
        assert!(!var_raw_converter.is_writable());
    }
}
