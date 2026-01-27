//! Marker/Bookmark system for data visualization
//!
//! This module provides functionality for marking specific time points
//! in the data for easy reference and navigation.
//!
//! # Features
//!
//! - Named markers at specific time points
//! - Color-coded marker types (event, error, note)
//! - Jump-to-marker navigation
//! - Export data between markers

use egui::Color32;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Marker type for categorization and color coding
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkerType {
    /// Generic event marker (blue)
    Event,
    /// Error or issue marker (red)
    Error,
    /// Note/annotation marker (yellow)
    Note,
    /// Start of a region marker (green)
    RegionStart,
    /// End of a region marker (green)
    RegionEnd,
    /// Custom marker (user-defined color)
    Custom,
}

impl MarkerType {
    /// Get the default color for this marker type
    pub fn color(&self) -> Color32 {
        match self {
            MarkerType::Event => Color32::from_rgb(100, 149, 237), // Cornflower blue
            MarkerType::Error => Color32::from_rgb(255, 99, 71),   // Tomato red
            MarkerType::Note => Color32::from_rgb(255, 215, 0),    // Gold
            MarkerType::RegionStart => Color32::from_rgb(50, 205, 50), // Lime green
            MarkerType::RegionEnd => Color32::from_rgb(34, 139, 34),   // Forest green
            MarkerType::Custom => Color32::from_rgb(200, 200, 200),    // Gray
        }
    }

    /// Get the display name for this marker type
    pub fn display_name(&self) -> &'static str {
        match self {
            MarkerType::Event => "Event",
            MarkerType::Error => "Error",
            MarkerType::Note => "Note",
            MarkerType::RegionStart => "Region Start",
            MarkerType::RegionEnd => "Region End",
            MarkerType::Custom => "Custom",
        }
    }

    /// Get all marker types
    pub fn all() -> &'static [MarkerType] {
        &[
            MarkerType::Event,
            MarkerType::Error,
            MarkerType::Note,
            MarkerType::RegionStart,
            MarkerType::RegionEnd,
            MarkerType::Custom,
        ]
    }
}

impl Default for MarkerType {
    fn default() -> Self {
        MarkerType::Event
    }
}

/// A marker/bookmark at a specific time point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marker {
    /// Unique identifier
    pub id: u32,
    /// Marker name/label
    pub name: String,
    /// Time position (as Duration since start)
    pub time: Duration,
    /// Marker type
    pub marker_type: MarkerType,
    /// Optional description/notes
    pub description: Option<String>,
    /// Custom color override (if Custom type)
    pub custom_color: Option<[u8; 4]>,
    /// Whether marker is visible
    pub visible: bool,
}

impl Marker {
    /// Create a new marker
    pub fn new(id: u32, name: impl Into<String>, time: Duration, marker_type: MarkerType) -> Self {
        Self {
            id,
            name: name.into(),
            time,
            marker_type,
            description: None,
            custom_color: None,
            visible: true,
        }
    }

    /// Get the time as seconds
    pub fn time_secs(&self) -> f64 {
        self.time.as_secs_f64()
    }

    /// Get the color to use for this marker
    pub fn color(&self) -> Color32 {
        if let Some(custom) = self.custom_color {
            Color32::from_rgba_unmultiplied(custom[0], custom[1], custom[2], custom[3])
        } else {
            self.marker_type.color()
        }
    }

    /// Set a custom color
    pub fn with_custom_color(mut self, color: [u8; 4]) -> Self {
        self.custom_color = Some(color);
        self.marker_type = MarkerType::Custom;
        self
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Manager for markers
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarkerManager {
    /// All markers
    markers: Vec<Marker>,
    /// Next marker ID
    next_id: u32,
}

impl MarkerManager {
    /// Create a new empty marker manager
    pub fn new() -> Self {
        Self {
            markers: Vec::new(),
            next_id: 1,
        }
    }

    /// Add a new marker
    pub fn add(&mut self, name: impl Into<String>, time: Duration, marker_type: MarkerType) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let marker = Marker::new(id, name, time, marker_type);
        self.markers.push(marker);
        self.sort_by_time();
        id
    }

    /// Add a marker with full configuration
    pub fn add_marker(&mut self, mut marker: Marker) -> u32 {
        marker.id = self.next_id;
        self.next_id += 1;
        let id = marker.id;
        self.markers.push(marker);
        self.sort_by_time();
        id
    }

    /// Remove a marker by ID
    pub fn remove(&mut self, id: u32) -> bool {
        if let Some(pos) = self.markers.iter().position(|m| m.id == id) {
            self.markers.remove(pos);
            true
        } else {
            false
        }
    }

    /// Get a marker by ID
    pub fn get(&self, id: u32) -> Option<&Marker> {
        self.markers.iter().find(|m| m.id == id)
    }

    /// Get a mutable reference to a marker by ID
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Marker> {
        self.markers.iter_mut().find(|m| m.id == id)
    }

    /// Get all markers
    pub fn all(&self) -> &[Marker] {
        &self.markers
    }

    /// Get visible markers
    pub fn visible(&self) -> impl Iterator<Item = &Marker> {
        self.markers.iter().filter(|m| m.visible)
    }

    /// Get markers in a time range
    pub fn in_range(&self, start: Duration, end: Duration) -> impl Iterator<Item = &Marker> {
        self.markers
            .iter()
            .filter(move |m| m.time >= start && m.time <= end)
    }

    /// Get the nearest marker to a time point
    pub fn nearest(&self, time: Duration) -> Option<&Marker> {
        self.markers.iter().min_by(|a, b| {
            let diff_a = if a.time > time {
                a.time - time
            } else {
                time - a.time
            };
            let diff_b = if b.time > time {
                b.time - time
            } else {
                time - b.time
            };
            diff_a.cmp(&diff_b)
        })
    }

    /// Find the next marker after a given time
    pub fn next_after(&self, time: Duration) -> Option<&Marker> {
        self.markers.iter().find(|m| m.time > time)
    }

    /// Find the previous marker before a given time
    pub fn prev_before(&self, time: Duration) -> Option<&Marker> {
        self.markers.iter().rev().find(|m| m.time < time)
    }

    /// Get count of markers
    pub fn len(&self) -> usize {
        self.markers.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.markers.is_empty()
    }

    /// Clear all markers
    pub fn clear(&mut self) {
        self.markers.clear();
    }

    /// Sort markers by time
    fn sort_by_time(&mut self) {
        self.markers.sort_by(|a, b| a.time.cmp(&b.time));
    }

    /// Get markers of a specific type
    pub fn by_type(&self, marker_type: MarkerType) -> impl Iterator<Item = &Marker> {
        self.markers.iter().filter(move |m| m.marker_type == marker_type)
    }

    /// Update a marker's name
    pub fn rename(&mut self, id: u32, new_name: impl Into<String>) -> bool {
        if let Some(marker) = self.get_mut(id) {
            marker.name = new_name.into();
            true
        } else {
            false
        }
    }

    /// Toggle marker visibility
    pub fn toggle_visibility(&mut self, id: u32) -> bool {
        if let Some(marker) = self.get_mut(id) {
            marker.visible = !marker.visible;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marker_creation() {
        let marker = Marker::new(1, "Test", Duration::from_secs(5), MarkerType::Event);
        assert_eq!(marker.id, 1);
        assert_eq!(marker.name, "Test");
        assert_eq!(marker.time_secs(), 5.0);
        assert_eq!(marker.marker_type, MarkerType::Event);
    }

    #[test]
    fn test_marker_manager_add() {
        let mut manager = MarkerManager::new();
        let id1 = manager.add("First", Duration::from_secs(10), MarkerType::Event);
        let id2 = manager.add("Second", Duration::from_secs(5), MarkerType::Note);

        assert_eq!(manager.len(), 2);
        // Should be sorted by time
        assert_eq!(manager.all()[0].name, "Second");
        assert_eq!(manager.all()[1].name, "First");
    }

    #[test]
    fn test_marker_manager_remove() {
        let mut manager = MarkerManager::new();
        let id = manager.add("Test", Duration::from_secs(5), MarkerType::Event);

        assert!(manager.remove(id));
        assert!(manager.is_empty());
        assert!(!manager.remove(id)); // Already removed
    }

    #[test]
    fn test_marker_manager_navigation() {
        let mut manager = MarkerManager::new();
        manager.add("A", Duration::from_secs(5), MarkerType::Event);
        manager.add("B", Duration::from_secs(10), MarkerType::Event);
        manager.add("C", Duration::from_secs(15), MarkerType::Event);

        // Next after
        let next = manager.next_after(Duration::from_secs(7));
        assert!(next.is_some());
        assert_eq!(next.unwrap().name, "B");

        // Prev before
        let prev = manager.prev_before(Duration::from_secs(12));
        assert!(prev.is_some());
        assert_eq!(prev.unwrap().name, "B");

        // Nearest
        let nearest = manager.nearest(Duration::from_secs(11));
        assert!(nearest.is_some());
        assert_eq!(nearest.unwrap().name, "B");
    }

    #[test]
    fn test_marker_type_colors() {
        // Just verify colors are defined
        for marker_type in MarkerType::all() {
            let _color = marker_type.color();
            let _name = marker_type.display_name();
        }
    }
}
