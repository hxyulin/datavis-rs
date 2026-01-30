//! Status bar panel — bottom bar showing connection, stats, and error info.
//!
//! Sits below the dock workspace area.

use egui::{Color32, RichText, Ui};

use crate::frontend::topics::Topics;
use crate::types::ConnectionStatus;

/// Context needed to render the status bar.
pub struct StatusBarContext<'a> {
    pub topics: &'a Topics,
    pub target_chip: &'a str,
    pub last_error: Option<&'a str>,
}

/// Render the status bar.
pub fn render_status_bar(ui: &mut Ui, ctx: &StatusBarContext<'_>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;

        // === Connection status dot + chip name ===
        let (status_color, status_text) = match ctx.topics.connection_status {
            ConnectionStatus::Connected => (Color32::GREEN, "Connected"),
            ConnectionStatus::Connecting => (Color32::YELLOW, "Connecting"),
            ConnectionStatus::Disconnected => (Color32::GRAY, "Disconnected"),
            ConnectionStatus::Error => (Color32::RED, "Error"),
        };
        ui.colored_label(status_color, "●");
        let chip_display = if ctx.target_chip.is_empty() {
            status_text.to_string()
        } else {
            format!("{}: {}", status_text, ctx.target_chip)
        };
        ui.label(RichText::new(chip_display).small());

        ui.separator();

        let stats = &ctx.topics.stats;

        // === Effective sample rate ===
        let target_rate = stats.effective_sample_rate;
        let rate_color = if target_rate > 0.0 {
            Color32::from_rgb(100, 255, 100)
        } else {
            Color32::GRAY
        };
        ui.label(RichText::new("Rate:").small());
        ui.colored_label(
            rate_color,
            RichText::new(format!("{:.1} Hz", target_rate)).small(),
        );

        ui.separator();

        // === Total sample count ===
        ui.label(RichText::new(format!("Samples: {}", stats.successful_reads)).small());

        ui.separator();

        // === Error count ===
        let error_color = if stats.failed_reads > 0 {
            Color32::LIGHT_RED
        } else {
            Color32::GRAY
        };
        ui.colored_label(
            error_color,
            RichText::new(format!("Errors: {}", stats.failed_reads)).small(),
        );

        ui.separator();

        // === Avg read time ===
        ui.label(RichText::new(format!("Avg: {:.1} μs", stats.avg_read_time_us)).small());

        ui.separator();

        // === Data transferred ===
        let kb = stats.total_bytes_read as f64 / 1024.0;
        let data_text = if kb > 1024.0 {
            format!("Data: {:.2} MB", kb / 1024.0)
        } else {
            format!("Data: {:.2} KB", kb)
        };
        ui.label(RichText::new(data_text).small());

        // === Error message (right-aligned) ===
        if let Some(error) = ctx.last_error {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.colored_label(Color32::RED, RichText::new(error).small());
            });
        }
    });
}
