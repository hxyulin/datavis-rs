//! Custom widgets for the DataVis-RS UI
//!
//! This module provides reusable UI widgets for the application.
//! These widgets encapsulate common UI patterns and can be used
//! throughout the frontend.
//!
//! # Widgets
//!
//! - [`StatusIndicator`] - Colored status dot with label (connected, error, etc.)
//! - [`ValueDisplay`] - Formatted value with label and optional unit
//! - [`IconToggle`] - Toggle button with custom on/off icons
//! - [`CollapsibleSection`] - Expandable/collapsible content section
//! - [`LabeledSeparator`] - Horizontal separator with centered label
//! - [`ColorSwatch`] - Small colored square for color preview
//! - [`Sparkline`] - Mini inline chart for showing recent values
//! - [`AddressInput`] - Hex-formatted memory address input field
//! - [`CollectionProgress`] - Sample count and rate display with spinner

use egui::{Color32, Response, Ui, Widget};

/// A widget that displays a colored status indicator
pub struct StatusIndicator {
    color: Color32,
    label: String,
    tooltip: Option<String>,
}

impl StatusIndicator {
    /// Create a new status indicator with the given color and label
    pub fn new(color: Color32, label: impl Into<String>) -> Self {
        Self {
            color,
            label: label.into(),
            tooltip: None,
        }
    }

    /// Create a connected status indicator
    pub fn connected() -> Self {
        Self::new(Color32::GREEN, "Connected")
    }

    /// Create a disconnected status indicator
    pub fn disconnected() -> Self {
        Self::new(Color32::GRAY, "Disconnected")
    }

    /// Create an error status indicator
    pub fn error() -> Self {
        Self::new(Color32::RED, "Error")
    }

    /// Create a connecting status indicator
    pub fn connecting() -> Self {
        Self::new(Color32::YELLOW, "Connecting...")
    }

    /// Add a tooltip to the indicator
    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

impl Widget for StatusIndicator {
    fn ui(self, ui: &mut Ui) -> Response {
        let response = ui.horizontal(|ui| {
            ui.colored_label(self.color, "●");
            ui.label(&self.label);
        });

        let response = response.response;

        if let Some(tooltip) = self.tooltip {
            response.on_hover_text(tooltip)
        } else {
            response
        }
    }
}

/// A widget for displaying a value with a label and optional unit
pub struct ValueDisplay {
    label: String,
    value: String,
    unit: Option<String>,
    color: Option<Color32>,
}

impl ValueDisplay {
    /// Create a new value display
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            unit: None,
            color: None,
        }
    }

    /// Create a new value display from a numeric value
    pub fn from_f64(label: impl Into<String>, value: f64, precision: usize) -> Self {
        Self::new(
            label,
            format!("{:.precision$}", value, precision = precision),
        )
    }

    /// Add a unit to the display
    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    /// Set the color of the value
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }
}

impl Widget for ValueDisplay {
    fn ui(self, ui: &mut Ui) -> Response {
        ui.horizontal(|ui| {
            ui.label(format!("{}:", self.label));

            let value_text = if let Some(unit) = self.unit {
                format!("{} {}", self.value, unit)
            } else {
                self.value
            };

            if let Some(color) = self.color {
                ui.colored_label(color, value_text);
            } else {
                ui.strong(value_text);
            }
        })
        .response
    }
}

/// A toggle button with an icon
pub struct IconToggle {
    icon_on: String,
    icon_off: String,
    tooltip_on: Option<String>,
    tooltip_off: Option<String>,
}

impl IconToggle {
    /// Create a new icon toggle
    pub fn new(icon_on: impl Into<String>, icon_off: impl Into<String>) -> Self {
        Self {
            icon_on: icon_on.into(),
            icon_off: icon_off.into(),
            tooltip_on: None,
            tooltip_off: None,
        }
    }

    /// Add tooltips for the on/off states
    pub fn with_tooltips(
        mut self,
        tooltip_on: impl Into<String>,
        tooltip_off: impl Into<String>,
    ) -> Self {
        self.tooltip_on = Some(tooltip_on.into());
        self.tooltip_off = Some(tooltip_off.into());
        self
    }

    /// Show the toggle and return whether it was clicked
    pub fn show(&self, ui: &mut Ui, value: &mut bool) -> Response {
        let icon = if *value {
            &self.icon_on
        } else {
            &self.icon_off
        };
        let response = ui.toggle_value(value, icon);

        if *value {
            if let Some(ref tooltip) = self.tooltip_on {
                return response.on_hover_text(tooltip);
            }
        } else {
            if let Some(ref tooltip) = self.tooltip_off {
                return response.on_hover_text(tooltip);
            }
        }

        response
    }
}

/// A collapsible section widget
pub struct CollapsibleSection {
    title: String,
    default_open: bool,
}

impl CollapsibleSection {
    /// Create a new collapsible section
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            default_open: true,
        }
    }

    /// Set whether the section is open by default
    pub fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }

    /// Show the section with the given content
    pub fn show<R>(
        self,
        ui: &mut Ui,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> egui::CollapsingResponse<R> {
        egui::CollapsingHeader::new(self.title)
            .default_open(self.default_open)
            .show(ui, add_contents)
    }
}

/// A horizontal separator with a label
pub struct LabeledSeparator {
    label: String,
}

impl LabeledSeparator {
    /// Create a new labeled separator
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

impl Widget for LabeledSeparator {
    fn ui(self, ui: &mut Ui) -> Response {
        ui.horizontal(|ui| {
            ui.separator();
            ui.label(&self.label);
            ui.separator();
        })
        .response
    }
}

/// A color swatch widget
pub struct ColorSwatch {
    color: Color32,
    size: f32,
}

impl ColorSwatch {
    /// Create a new color swatch
    pub fn new(color: Color32) -> Self {
        Self { color, size: 16.0 }
    }

    /// Create a color swatch from RGBA values
    pub fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::new(Color32::from_rgba_unmultiplied(r, g, b, a))
    }

    /// Set the size of the swatch
    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl Widget for ColorSwatch {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(self.size, self.size), egui::Sense::hover());

        if ui.is_rect_visible(rect) {
            ui.painter().rect_filled(rect, 2.0, self.color);
            ui.painter().rect_stroke(
                rect,
                2.0,
                egui::Stroke::new(1.0, Color32::GRAY),
                egui::StrokeKind::Outside,
            );
        }

        response
    }
}

/// A mini sparkline widget for showing recent values
pub struct Sparkline {
    values: Vec<f64>,
    width: f32,
    height: f32,
    color: Color32,
}

impl Sparkline {
    /// Create a new sparkline from values
    pub fn new(values: Vec<f64>) -> Self {
        Self {
            values,
            width: 80.0,
            height: 20.0,
            color: Color32::WHITE,
        }
    }

    /// Set the size of the sparkline
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Set the color of the sparkline
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Widget for Sparkline {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(self.width, self.height), egui::Sense::hover());

        if ui.is_rect_visible(rect) && !self.values.is_empty() {
            let min_val = self.values.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_val = self
                .values
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max);
            let range = (max_val - min_val).max(f64::EPSILON);

            let points: Vec<egui::Pos2> = self
                .values
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let x = rect.left()
                        + (i as f32 / (self.values.len() - 1).max(1) as f32) * rect.width();
                    let y = rect.bottom() - ((v - min_val) / range) as f32 * rect.height();
                    egui::pos2(x, y)
                })
                .collect();

            if points.len() >= 2 {
                for i in 0..points.len() - 1 {
                    ui.painter().line_segment(
                        [points[i], points[i + 1]],
                        egui::Stroke::new(1.0, self.color),
                    );
                }
            }
        }

        response
    }
}

/// A memory address input widget with hex formatting
pub struct AddressInput {
    value: u64,
}

impl AddressInput {
    /// Create a new address input
    pub fn new(value: u64) -> Self {
        Self { value }
    }

    /// Show the input and return the new value if changed
    pub fn show(&mut self, ui: &mut Ui, label: &str) -> Option<u64> {
        let mut text = format!("0x{:08X}", self.value);

        ui.horizontal(|ui| {
            ui.label(label);
            let response = ui.text_edit_singleline(&mut text);

            if response.changed() {
                let trimmed = text.trim_start_matches("0x").trim_start_matches("0X");
                if let Ok(new_value) = u64::from_str_radix(trimmed, 16) {
                    self.value = new_value;
                    return Some(new_value);
                }
            }
            None
        })
        .inner
    }
}

/// A progress indicator for data collection
pub struct CollectionProgress {
    samples: u64,
    rate: f64,
    is_running: bool,
}

impl CollectionProgress {
    /// Create a new collection progress indicator
    pub fn new(samples: u64, rate: f64, is_running: bool) -> Self {
        Self {
            samples,
            rate,
            is_running,
        }
    }
}

impl Widget for CollectionProgress {
    fn ui(self, ui: &mut Ui) -> Response {
        ui.horizontal(|ui| {
            if self.is_running {
                ui.spinner();
            }

            ui.label(format!("{} samples @ {:.1} Hz", self.samples, self.rate));
        })
        .response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_indicator() {
        let indicator = StatusIndicator::connected();
        assert_eq!(indicator.label, "Connected");
        assert_eq!(indicator.color, Color32::GREEN);
    }

    #[test]
    fn test_value_display() {
        let display = ValueDisplay::from_f64("Temperature", 25.5, 1).with_unit("°C");
        assert_eq!(display.label, "Temperature");
        assert_eq!(display.value, "25.5");
        assert_eq!(display.unit, Some("°C".to_string()));
    }

    #[test]
    fn test_color_swatch() {
        let swatch = ColorSwatch::from_rgba(255, 128, 64, 255);
        assert_eq!(
            swatch.color,
            Color32::from_rgba_unmultiplied(255, 128, 64, 255)
        );
    }
}
