//! Plot rendering module using egui_plot
//!
//! This module provides advanced plotting functionality for visualizing
//! variable data in real-time using the egui_plot crate.
//!
//! # Features
//!
//! - **Real-time plotting**: Efficiently render time-series data as it arrives
//! - **Auto-scaling**: Automatic X and Y axis scaling to fit visible data
//! - **Axis locking**: Lock X/Y axes to prevent accidental zoom/pan
//! - **Time window control**: Configurable display window with max limit
//! - **Multiple variables**: Plot multiple variables with distinct colors
//! - **Interactive**: Zoom, pan, and inspect data points
//!
//! # Main Types
//!
//! - [`PlotView`] - Main plot configuration and rendering state
//! - [`PlotStatistics`] - Statistical analysis of variable data
//! - [`PlotCursor`] - Cursor tracking for data inspection
//! - [`ColorPalette`] - Color generation for multiple variables

use crate::config::settings::RuntimeSettings;
use crate::config::UiConfig;
use crate::types::VariableData;
use egui::{Color32, Ui};
use egui_plot::{
    Corner, GridMark, Legend, Line, Plot, PlotBounds, PlotPoint, PlotPoints, PlotUi, VLine,
};
use std::collections::HashMap;

/// Plot view configuration and state
#[derive(Debug, Clone)]
pub struct PlotView {
    /// Whether to show the legend
    pub show_legend: bool,
    /// Whether to show grid lines
    pub show_grid: bool,
    /// Line width for all plots
    pub line_width: f32,
    /// Whether to auto-scale the Y axis
    pub auto_scale_y: bool,
    /// Whether to auto-scale the X axis (follow latest)
    pub auto_scale_x: bool,
    /// Whether the X axis is locked (user cannot zoom/pan)
    pub lock_x: bool,
    /// Whether the Y axis is locked (user cannot zoom/pan)
    pub lock_y: bool,
    /// Manual Y-axis bounds (if not auto-scaling)
    pub y_bounds: Option<(f64, f64)>,
    /// Manual X-axis bounds (if not auto-scaling)
    pub x_bounds: Option<(f64, f64)>,
    /// Time window to display in seconds
    pub time_window: f64,
    /// Maximum allowed time window in seconds
    pub max_time_window: f64,
    /// Whether to follow the latest data
    pub follow_latest: bool,
    /// Current time offset for scrolling
    pub time_offset: f64,
    /// Whether to show markers at data points
    pub show_markers: bool,
    /// Marker radius
    pub marker_radius: f32,
    /// Whether the plot is currently being dragged
    pub is_dragging: bool,
    /// Last known plot bounds for restoring after drag
    pub last_bounds: Option<PlotBounds>,
}

impl Default for PlotView {
    fn default() -> Self {
        Self {
            show_legend: true,
            show_grid: true,
            line_width: 1.5,
            auto_scale_y: true,
            auto_scale_x: true,
            lock_x: false,
            lock_y: false,
            y_bounds: None,
            x_bounds: None,
            time_window: 10.0,
            max_time_window: 300.0,
            follow_latest: true,
            time_offset: 0.0,
            show_markers: false,
            marker_radius: 2.0,
            is_dragging: false,
            last_bounds: None,
        }
    }
}

impl PlotView {
    /// Create a new PlotView from UI configuration
    pub fn from_config(config: &UiConfig) -> Self {
        Self {
            show_legend: config.show_legend,
            show_grid: config.show_grid,
            line_width: config.line_width,
            auto_scale_y: config.auto_scale_y,
            time_window: config.time_window_seconds,
            ..Default::default()
        }
    }

    /// Update PlotView from runtime settings
    pub fn update_from_settings(&mut self, settings: &RuntimeSettings) {
        self.time_window = settings.display_time_window;
        self.follow_latest = settings.follow_latest;
        self.auto_scale_x = settings.autoscale_x;
        self.auto_scale_y = settings.autoscale_y;
        self.lock_x = settings.lock_x;
        self.lock_y = settings.lock_y;
        self.max_time_window = settings.max_time_window;

        if settings.is_manual_y_scale() {
            self.y_bounds = Some((settings.y_min.unwrap(), settings.y_max.unwrap()));
        } else {
            self.y_bounds = None;
        }

        if settings.is_manual_x_scale() {
            self.x_bounds = Some((settings.x_min.unwrap(), settings.x_max.unwrap()));
        } else {
            self.x_bounds = None;
        }
    }

    /// Check if X axis allows user interaction (zoom/pan)
    pub fn can_interact_x(&self) -> bool {
        !self.lock_x && !self.auto_scale_x
    }

    /// Check if Y axis allows user interaction (zoom/pan)
    pub fn can_interact_y(&self) -> bool {
        !self.lock_y && !self.auto_scale_y
    }

    /// Set manual X-axis bounds
    pub fn set_x_bounds(&mut self, min: f64, max: f64) {
        self.auto_scale_x = false;
        self.x_bounds = Some((min, max));
    }

    /// Clear manual X-axis bounds and enable auto-scaling
    pub fn clear_x_bounds(&mut self) {
        self.auto_scale_x = true;
        self.x_bounds = None;
    }

    /// Update time window from zoom, respecting max limit
    pub fn update_time_window_from_bounds(&mut self, x_min: f64, x_max: f64) {
        let new_window = (x_max - x_min).clamp(0.1, self.max_time_window);
        self.time_window = new_window;
    }

    /// Render the main plot with all enabled variables
    /// Returns the new time window if it was changed by user zoom
    pub fn render(
        &mut self,
        ui: &mut Ui,
        variable_data: &HashMap<u32, VariableData>,
        current_time: f64,
    ) -> Option<f64> {
        // Determine zoom/drag permissions based on lock and autoscale settings
        let allow_x_zoom = !self.lock_x;
        let allow_y_zoom = !self.lock_y;
        let allow_x_drag = !self.lock_x && !self.auto_scale_x;
        let allow_y_drag = !self.lock_y && !self.auto_scale_y;

        let mut plot = Plot::new("main_data_plot")
            .allow_zoom([allow_x_zoom, allow_y_zoom])
            .allow_drag([allow_x_drag, allow_y_drag])
            .allow_scroll([!self.lock_x, !self.lock_y])
            .allow_boxed_zoom(allow_x_zoom || allow_y_zoom)
            .show_axes(true)
            .show_grid(self.show_grid)
            .x_axis_label("Time (s)")
            .y_axis_label("Value");

        // Configure legend
        if self.show_legend {
            plot = plot.legend(
                Legend::default()
                    .position(Corner::RightTop)
                    .background_alpha(0.8),
            );
        }

        // Custom grid formatting
        plot = plot.x_grid_spacer(|grid_input| {
            create_time_grid_marks(grid_input.bounds, grid_input.base_step_size)
        });

        // Set auto bounds behavior based on autoscale settings
        plot = plot.auto_bounds([false, self.auto_scale_y]);

        // Calculate bounds for setting
        let time_window = self.time_window;
        let auto_scale_x = self.auto_scale_x;
        let auto_scale_y = self.auto_scale_y;
        let x_bounds_manual = self.x_bounds;
        let y_bounds_manual = self.y_bounds;

        let mut new_time_window: Option<f64> = None;

        // Show the plot
        let response = plot.show(ui, |plot_ui| {
            // Calculate and set bounds
            if auto_scale_x {
                // Auto-scale X: follow latest data with time window
                let x_max = current_time;
                let x_min = (x_max - time_window).max(0.0);

                // Calculate Y bounds from visible data if autoscaling Y
                let (y_min, y_max) = if auto_scale_y {
                    calculate_y_bounds_for_range(variable_data, x_min, x_max)
                } else if let Some((ymin, ymax)) = y_bounds_manual {
                    (ymin, ymax)
                } else {
                    (-1.0, 1.0)
                };

                plot_ui.set_plot_bounds(PlotBounds::from_min_max([x_min, y_min], [x_max, y_max]));
            } else if let Some((x_min, x_max)) = x_bounds_manual {
                // Manual X bounds
                let (y_min, y_max) = if auto_scale_y {
                    calculate_y_bounds_for_range(variable_data, x_min, x_max)
                } else if let Some((ymin, ymax)) = y_bounds_manual {
                    (ymin, ymax)
                } else {
                    (-1.0, 1.0)
                };

                plot_ui.set_plot_bounds(PlotBounds::from_min_max([x_min, y_min], [x_max, y_max]));
            }

            self.render_data_lines(plot_ui, variable_data);

            // Render current time indicator if following latest
            if self.follow_latest && auto_scale_x {
                let vline = VLine::new("current_time", current_time)
                    .color(Color32::from_rgba_unmultiplied(255, 255, 255, 64))
                    .width(1.0);
                plot_ui.vline(vline);
            }
        });

        // Handle plot interactions - update time window from zoom
        if response.response.dragged() {
            self.is_dragging = true;
            if !self.lock_x {
                self.follow_latest = false;
                self.auto_scale_x = false;
            }
        }

        if response.response.drag_stopped() {
            self.is_dragging = false;
            // Capture the new bounds after drag
            if !self.lock_x && !self.auto_scale_x {
                let bounds = response.transform.bounds();
                let x_range = bounds.max()[0] - bounds.min()[0];
                let clamped_range = x_range.clamp(0.1, self.max_time_window);
                self.time_window = clamped_range;
                self.x_bounds = Some((bounds.min()[0], bounds.max()[0]));
                new_time_window = Some(clamped_range);
            }
        }

        // Check for zoom changes (scroll wheel zoom)
        if response.response.hovered() {
            let scroll_delta = ui.input(|i| i.raw_scroll_delta);
            if scroll_delta.y.abs() > 0.0 && !self.lock_x {
                // User is zooming - disable autoscale and capture new bounds
                if self.auto_scale_x {
                    self.auto_scale_x = false;
                    self.follow_latest = false;
                }
                // The bounds will be captured on next frame
            }
        }

        // After any zoom interaction, check the actual plot bounds
        if !self.auto_scale_x && !self.lock_x {
            let bounds = response.transform.bounds();
            let x_range = bounds.max()[0] - bounds.min()[0];
            if x_range > 0.0 && (x_range - self.time_window).abs() > 0.01 {
                let clamped_range = x_range.clamp(0.1, self.max_time_window);
                self.time_window = clamped_range;
                self.x_bounds = Some((bounds.min()[0], bounds.max()[0]));
                new_time_window = Some(clamped_range);
            }
        }

        new_time_window
    }

    /// Render data lines for all enabled variables
    fn render_data_lines(&self, plot_ui: &mut PlotUi, variable_data: &HashMap<u32, VariableData>) {
        for (_id, data) in variable_data {
            if !data.variable.enabled || data.data_points.is_empty() {
                continue;
            }

            // Convert data points to plot points
            let points: Vec<[f64; 2]> = data.as_plot_points();

            if points.is_empty() {
                continue;
            }

            let plot_points = PlotPoints::from(points);

            // Get color from variable configuration
            let color = Color32::from_rgba_unmultiplied(
                data.variable.color[0],
                data.variable.color[1],
                data.variable.color[2],
                data.variable.color[3],
            );

            // Create and render the line
            let line = Line::new(
                format!("{} ({})", data.variable.name, data.variable.unit),
                plot_points,
            )
            .color(color)
            .width(self.line_width);

            plot_ui.line(line);
        }
    }

    /// Render a single variable's data
    pub fn render_single_variable(&self, ui: &mut Ui, data: &VariableData, plot_id: &str) {
        let plot = Plot::new(plot_id)
            .allow_zoom(true)
            .allow_drag(true)
            .show_axes(true)
            .show_grid(self.show_grid)
            .height(150.0)
            .legend(Legend::default().position(Corner::RightTop));

        plot.show(ui, |plot_ui| {
            if data.data_points.is_empty() {
                return;
            }

            let points: PlotPoints = data.as_plot_points().into();

            let color = Color32::from_rgba_unmultiplied(
                data.variable.color[0],
                data.variable.color[1],
                data.variable.color[2],
                data.variable.color[3],
            );

            let line = Line::new(&data.variable.name, points)
                .color(color)
                .width(self.line_width);

            plot_ui.line(line);
        });
    }

    /// Reset the view to follow latest data
    pub fn reset_view(&mut self) {
        self.follow_latest = true;
        self.time_offset = 0.0;
        self.is_dragging = false;
    }

    /// Set manual Y-axis bounds
    pub fn set_y_bounds(&mut self, min: f64, max: f64) {
        self.auto_scale_y = false;
        self.y_bounds = Some((min, max));
    }

    /// Clear manual Y-axis bounds and enable auto-scaling
    pub fn clear_y_bounds(&mut self) {
        self.auto_scale_y = true;
        self.y_bounds = None;
    }

    /// Set the time window
    pub fn set_time_window(&mut self, seconds: f64) {
        self.time_window = seconds.clamp(0.1, self.max_time_window);
    }

    /// Set maximum time window
    pub fn set_max_time_window(&mut self, seconds: f64) {
        self.max_time_window = seconds.max(1.0);
        // Clamp current time window if needed
        if self.time_window > self.max_time_window {
            self.time_window = self.max_time_window;
        }
    }

    /// Toggle X axis autoscale
    pub fn toggle_autoscale_x(&mut self) {
        self.auto_scale_x = !self.auto_scale_x;
        if self.auto_scale_x {
            self.follow_latest = true;
            self.x_bounds = None;
        }
    }

    /// Toggle Y axis autoscale
    pub fn toggle_autoscale_y(&mut self) {
        self.auto_scale_y = !self.auto_scale_y;
        if self.auto_scale_y {
            self.y_bounds = None;
        }
    }

    /// Toggle X axis lock
    pub fn toggle_lock_x(&mut self) {
        self.lock_x = !self.lock_x;
    }

    /// Toggle Y axis lock
    pub fn toggle_lock_y(&mut self) {
        self.lock_y = !self.lock_y;
    }
}

/// Calculate Y bounds for data within the given X range
fn calculate_y_bounds_for_range(
    variable_data: &HashMap<u32, VariableData>,
    x_min: f64,
    x_max: f64,
) -> (f64, f64) {
    let mut y_min = f64::MAX;
    let mut y_max = f64::MIN;

    for data in variable_data.values() {
        if !data.variable.enabled {
            continue;
        }
        for dp in &data.data_points {
            let t = dp.timestamp.as_secs_f64();
            if t >= x_min && t <= x_max {
                y_min = y_min.min(dp.converted_value);
                y_max = y_max.max(dp.converted_value);
            }
        }
    }

    // Add some padding to Y bounds
    if y_min < f64::MAX && y_max > f64::MIN {
        let y_range = y_max - y_min;
        let padding = if y_range > 0.0 { y_range * 0.1 } else { 1.0 };
        (y_min - padding, y_max + padding)
    } else {
        (-1.0, 1.0)
    }
}

/// Create time-based grid marks for the X axis
fn create_time_grid_marks(bounds: (f64, f64), _base_step: f64) -> Vec<GridMark> {
    let (min, max) = bounds;
    let range = max - min;

    // Determine appropriate step size
    let step = if range < 1.0 {
        0.1
    } else if range < 5.0 {
        0.5
    } else if range < 20.0 {
        1.0
    } else if range < 60.0 {
        5.0
    } else if range < 300.0 {
        30.0
    } else {
        60.0
    };

    let mut marks = Vec::new();
    let start = (min / step).floor() * step;
    let mut current = start;

    while current <= max {
        marks.push(GridMark {
            value: current,
            step_size: step,
        });
        current += step;
    }

    marks
}

/// Statistics for a variable's data
///
/// Provides statistical analysis of variable data including min/max,
/// mean, standard deviation, and RMS. Useful for data analysis overlays.
#[derive(Debug, Clone, Default)]
pub struct PlotStatistics {
    /// Minimum value
    pub min: f64,
    /// Maximum value
    pub max: f64,
    /// Mean value
    pub mean: f64,
    /// Standard deviation
    pub std_dev: f64,
    /// Root mean square
    pub rms: f64,
    /// Number of samples
    pub count: usize,
    /// Time range start (if calculated for a range)
    pub time_start: Option<f64>,
    /// Time range end (if calculated for a range)
    pub time_end: Option<f64>,
}

impl PlotStatistics {
    /// Calculate statistics from variable data
    pub fn from_data(data: &VariableData) -> Self {
        if data.data_points.is_empty() {
            return Self::default();
        }

        let values: Vec<f64> = data.data_points.iter().map(|p| p.converted_value).collect();
        Self::from_values(&values, None, None)
    }

    /// Calculate statistics from variable data within a time range
    pub fn from_data_range(data: &VariableData, time_start: f64, time_end: f64) -> Self {
        let values: Vec<f64> = data
            .data_points
            .iter()
            .filter(|p| {
                let t = p.timestamp.as_secs_f64();
                t >= time_start && t <= time_end
            })
            .map(|p| p.converted_value)
            .collect();

        Self::from_values(&values, Some(time_start), Some(time_end))
    }

    /// Calculate statistics from a slice of values
    fn from_values(values: &[f64], time_start: Option<f64>, time_end: Option<f64>) -> Self {
        if values.is_empty() {
            return Self {
                time_start,
                time_end,
                ..Default::default()
            };
        }

        let count = values.len();

        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let sum: f64 = values.iter().sum();
        let mean = sum / count as f64;

        let variance: f64 = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count as f64;
        let std_dev = variance.sqrt();

        // RMS: sqrt(mean of squares)
        let sum_of_squares: f64 = values.iter().map(|v| v * v).sum();
        let rms = (sum_of_squares / count as f64).sqrt();

        Self {
            min,
            max,
            mean,
            std_dev,
            rms,
            count,
            time_start,
            time_end,
        }
    }

    /// Get the peak-to-peak range
    pub fn peak_to_peak(&self) -> f64 {
        self.max - self.min
    }

    /// Check if this is a valid (non-empty) statistics
    pub fn is_valid(&self) -> bool {
        self.count > 0
    }
}

/// Cursor information for the plot
///
/// Tracks the cursor position and finds the nearest data point
/// for hover tooltips and data inspection. Supports dual cursors
/// for range measurements.
#[derive(Debug, Clone, Default)]
pub struct PlotCursor {
    /// Current cursor position in plot coordinates (time, value)
    pub position: Option<PlotPoint>,
    /// First marker cursor position (for range measurements)
    pub cursor_a: Option<PlotPoint>,
    /// Second marker cursor position (for range measurements)
    pub cursor_b: Option<PlotPoint>,
    /// Whether cursor mode is enabled
    pub enabled: bool,
    /// Nearest data points to cursor (variable_id -> (point, variable_name))
    pub nearest_points: HashMap<u32, (PlotPoint, String)>,
}

impl PlotCursor {
    /// Create a new cursor with enabled state
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Default::default()
        }
    }

    /// Update cursor position from plot coordinates
    pub fn update_position(&mut self, position: Option<PlotPoint>) {
        self.position = position;
    }

    /// Set cursor A at the current position
    pub fn set_cursor_a(&mut self) {
        self.cursor_a = self.position;
    }

    /// Set cursor B at the current position
    pub fn set_cursor_b(&mut self) {
        self.cursor_b = self.position;
    }

    /// Clear both cursors
    pub fn clear_cursors(&mut self) {
        self.cursor_a = None;
        self.cursor_b = None;
    }

    /// Get the time delta between cursors (if both set)
    pub fn time_delta(&self) -> Option<f64> {
        match (self.cursor_a, self.cursor_b) {
            (Some(a), Some(b)) => Some((b.x - a.x).abs()),
            _ => None,
        }
    }

    /// Get the value delta between cursors (if both set)
    pub fn value_delta(&self) -> Option<f64> {
        match (self.cursor_a, self.cursor_b) {
            (Some(a), Some(b)) => Some(b.y - a.y),
            _ => None,
        }
    }

    /// Get the time range between cursors (ordered)
    pub fn time_range(&self) -> Option<(f64, f64)> {
        match (self.cursor_a, self.cursor_b) {
            (Some(a), Some(b)) => {
                let (min, max) = if a.x < b.x { (a.x, b.x) } else { (b.x, a.x) };
                Some((min, max))
            }
            _ => None,
        }
    }

    /// Find the nearest data point to the cursor for each enabled variable
    pub fn find_nearest(&mut self, variable_data: &HashMap<u32, VariableData>) {
        self.nearest_points.clear();

        let Some(cursor_pos) = self.position else {
            return;
        };

        for (id, data) in variable_data {
            if !data.variable.enabled || !data.variable.show_in_graph {
                continue;
            }

            // Find nearest point in time (X-axis only for better UX)
            let mut min_distance = f64::INFINITY;
            let mut nearest: Option<PlotPoint> = None;

            for point in &data.data_points {
                let x = point.timestamp.as_secs_f64();
                let y = point.converted_value;

                let distance = (x - cursor_pos.x).abs();

                if distance < min_distance {
                    min_distance = distance;
                    nearest = Some(PlotPoint::new(x, y));
                }
            }

            if let Some(point) = nearest {
                self.nearest_points
                    .insert(*id, (point, data.variable.name.clone()));
            }
        }
    }

    /// Check if we have any active cursors set
    pub fn has_cursors(&self) -> bool {
        self.cursor_a.is_some() || self.cursor_b.is_some()
    }

    /// Check if we have both cursors set (for range measurements)
    pub fn has_range(&self) -> bool {
        self.cursor_a.is_some() && self.cursor_b.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plot_view_default() {
        let view = PlotView::default();
        assert!(view.show_legend);
        assert!(view.show_grid);
        assert!(view.auto_scale_y);
        assert!(view.auto_scale_x);
        assert!(!view.lock_x);
        assert!(!view.lock_y);
        assert!(view.follow_latest);
        assert_eq!(view.time_window, 10.0);
        assert_eq!(view.max_time_window, 300.0);
    }

    #[test]
    fn test_y_bounds() {
        let mut view = PlotView::default();
        assert!(view.auto_scale_y);

        view.set_y_bounds(-5.0, 5.0);
        assert!(!view.auto_scale_y);
        assert_eq!(view.y_bounds, Some((-5.0, 5.0)));

        view.clear_y_bounds();
        assert!(view.auto_scale_y);
        assert!(view.y_bounds.is_none());
    }

    #[test]
    fn test_x_bounds() {
        let mut view = PlotView::default();
        assert!(view.auto_scale_x);

        view.set_x_bounds(0.0, 100.0);
        assert!(!view.auto_scale_x);
        assert_eq!(view.x_bounds, Some((0.0, 100.0)));

        view.clear_x_bounds();
        assert!(view.auto_scale_x);
        assert!(view.x_bounds.is_none());
    }

    #[test]
    fn test_autoscale_toggles() {
        let mut view = PlotView::default();

        view.toggle_autoscale_x();
        assert!(!view.auto_scale_x);

        view.toggle_autoscale_x();
        assert!(view.auto_scale_x);
        assert!(view.follow_latest);

        view.toggle_autoscale_y();
        assert!(!view.auto_scale_y);

        view.toggle_autoscale_y();
        assert!(view.auto_scale_y);
    }

    #[test]
    fn test_lock_toggles() {
        let mut view = PlotView::default();

        view.toggle_lock_x();
        assert!(view.lock_x);

        view.toggle_lock_y();
        assert!(view.lock_y);

        view.toggle_lock_x();
        assert!(!view.lock_x);

        view.toggle_lock_y();
        assert!(!view.lock_y);
    }

    #[test]
    fn test_time_window_clamping() {
        let mut view = PlotView::default();
        assert_eq!(view.max_time_window, 300.0);

        view.set_time_window(500.0);
        assert_eq!(view.time_window, 300.0);

        view.set_time_window(0.01);
        assert_eq!(view.time_window, 0.1);

        view.set_max_time_window(600.0);
        view.set_time_window(500.0);
        assert_eq!(view.time_window, 500.0);
    }

    #[test]
    fn test_interaction_checks() {
        let mut view = PlotView::default();

        // With autoscale on, can't interact
        assert!(!view.can_interact_x());
        assert!(!view.can_interact_y());

        // Disable autoscale, can interact
        view.auto_scale_x = false;
        view.auto_scale_y = false;
        assert!(view.can_interact_x());
        assert!(view.can_interact_y());

        // Lock axes, can't interact
        view.lock_x = true;
        view.lock_y = true;
        assert!(!view.can_interact_x());
        assert!(!view.can_interact_y());
    }

    #[test]
    fn test_time_grid_marks() {
        let marks = create_time_grid_marks((0.0, 10.0), 1.0);
        assert!(!marks.is_empty());

        // All marks should be within bounds or at boundaries
        for mark in &marks {
            assert!(mark.value >= 0.0 && mark.value <= 10.0);
        }
    }

    #[test]
    fn test_plot_statistics_empty() {
        use crate::types::Variable;

        let var = Variable::new("test", 0x2000_0000, crate::types::VariableType::U32);
        let data = VariableData::new(var);
        let stats = PlotStatistics::from_data(&data);

        assert_eq!(stats.count, 0);
        assert!(!stats.is_valid());
    }

    #[test]
    fn test_plot_statistics_with_data() {
        use crate::types::{DataPoint, Variable};
        use std::time::Duration;

        let var = Variable::new("test", 0x2000_0000, crate::types::VariableType::U32);
        let mut data = VariableData::new(var);

        // Add some test data points: 1, 2, 3, 4, 5
        for i in 1..=5 {
            let dp = DataPoint {
                timestamp: Duration::from_secs(i as u64),
                raw_value: i as f64,
                converted_value: i as f64,
            };
            data.data_points.push_back(dp);
        }

        let stats = PlotStatistics::from_data(&data);

        assert!(stats.is_valid());
        assert_eq!(stats.count, 5);
        assert!((stats.min - 1.0).abs() < 0.001);
        assert!((stats.max - 5.0).abs() < 0.001);
        assert!((stats.mean - 3.0).abs() < 0.001);
        assert!((stats.peak_to_peak() - 4.0).abs() < 0.001);
        // RMS of 1,2,3,4,5 = sqrt((1+4+9+16+25)/5) = sqrt(11) ≈ 3.317
        assert!((stats.rms - 3.317).abs() < 0.01);
    }

    #[test]
    fn test_plot_statistics_range() {
        use crate::types::{DataPoint, Variable};
        use std::time::Duration;

        let var = Variable::new("test", 0x2000_0000, crate::types::VariableType::U32);
        let mut data = VariableData::new(var);

        // Add data at times 0-9 with values 0-9
        for i in 0..10 {
            let dp = DataPoint {
                timestamp: Duration::from_secs(i as u64),
                raw_value: i as f64,
                converted_value: i as f64,
            };
            data.data_points.push_back(dp);
        }

        // Get stats for range 2-5 (should include values 2, 3, 4, 5)
        let stats = PlotStatistics::from_data_range(&data, 2.0, 5.0);

        assert!(stats.is_valid());
        assert_eq!(stats.count, 4);
        assert!((stats.min - 2.0).abs() < 0.001);
        assert!((stats.max - 5.0).abs() < 0.001);
        assert!((stats.mean - 3.5).abs() < 0.001);
        assert_eq!(stats.time_start, Some(2.0));
        assert_eq!(stats.time_end, Some(5.0));
    }

    #[test]
    fn test_plot_cursor_default() {
        let cursor = PlotCursor::default();
        assert!(!cursor.enabled);
        assert!(cursor.cursor_a.is_none());
        assert!(cursor.cursor_b.is_none());
        assert!(!cursor.has_cursors());
        assert!(!cursor.has_range());
    }

    #[test]
    fn test_plot_cursor_set_cursors() {
        let mut cursor = PlotCursor::new(true);
        assert!(cursor.enabled);

        cursor.update_position(Some(PlotPoint::new(1.0, 10.0)));
        cursor.set_cursor_a();
        assert!(cursor.cursor_a.is_some());
        assert!(cursor.has_cursors());
        assert!(!cursor.has_range());

        cursor.update_position(Some(PlotPoint::new(3.0, 30.0)));
        cursor.set_cursor_b();
        assert!(cursor.cursor_b.is_some());
        assert!(cursor.has_range());

        // Check time delta
        let dt = cursor.time_delta().unwrap();
        assert!((dt - 2.0).abs() < 0.001);

        // Check value delta
        let dv = cursor.value_delta().unwrap();
        assert!((dv - 20.0).abs() < 0.001);

        // Check time range (should be ordered)
        let (t_min, t_max) = cursor.time_range().unwrap();
        assert!((t_min - 1.0).abs() < 0.001);
        assert!((t_max - 3.0).abs() < 0.001);

        // Clear cursors
        cursor.clear_cursors();
        assert!(!cursor.has_cursors());
        assert!(!cursor.has_range());
    }

    #[test]
    fn test_plot_cursor_time_range_reversed() {
        let mut cursor = PlotCursor::new(true);

        // Set B before A (reversed order)
        cursor.update_position(Some(PlotPoint::new(5.0, 50.0)));
        cursor.set_cursor_a();

        cursor.update_position(Some(PlotPoint::new(2.0, 20.0)));
        cursor.set_cursor_b();

        // Time range should still be ordered
        let (t_min, t_max) = cursor.time_range().unwrap();
        assert!((t_min - 2.0).abs() < 0.001);
        assert!((t_max - 5.0).abs() < 0.001);

        // Time delta should be absolute
        let dt = cursor.time_delta().unwrap();
        assert!((dt - 3.0).abs() < 0.001);
    }
}
