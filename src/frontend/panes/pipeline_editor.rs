//! Pipeline Editor pane — visual display of the pipeline graph.
//!
//! Renders the pipeline topology as a node graph using custom egui painting.
//! Read-only for now: displays nodes (color-coded by type) and edges as bezier curves.

use std::collections::HashMap;

use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};

use crate::frontend::pane_trait::Pane;
use crate::frontend::state::{AppAction, SharedState};
use crate::frontend::workspace::PaneKind;
use crate::pipeline::bridge::TopologySnapshot;
use crate::pipeline::id::NodeId;

/// State for the Pipeline Editor pane.
pub struct PipelineEditorState {
    /// Per-node UI positions (for layout).
    pub node_positions: HashMap<NodeId, Pos2>,
    /// Pan offset for the canvas.
    pub pan_offset: Vec2,
    /// Zoom level.
    pub zoom: f32,
    /// Selected node (for showing info).
    pub selected_node: Option<NodeId>,
    /// Whether we've requested topology yet.
    pub topology_requested: bool,
}

impl Default for PipelineEditorState {
    fn default() -> Self {
        Self {
            node_positions: HashMap::new(),
            pan_offset: Vec2::ZERO,
            zoom: 1.0,
            selected_node: None,
            topology_requested: false,
        }
    }
}

/// Render the pipeline editor pane.
pub fn render(
    state: &mut PipelineEditorState,
    shared: &mut SharedState<'_>,
    ui: &mut Ui,
) -> Vec<AppAction> {
    let mut actions = Vec::new();

    // Request topology on first render
    if !state.topology_requested {
        state.topology_requested = true;
        actions.push(AppAction::RequestTopology);
    }

    ui.heading("Pipeline Editor");
    ui.separator();

    if ui.button("Refresh").clicked() {
        actions.push(AppAction::RequestTopology);
    }

    let Some(topology) = shared.topics.topology.clone() else {
        ui.label("Waiting for topology...");
        return actions;
    };

    // Auto-layout if node positions are empty
    if state.node_positions.is_empty() && !topology.nodes.is_empty() {
        auto_layout(state, &topology);
    }

    // Draw the graph
    let available = ui.available_rect_before_wrap();
    let (response, painter) = ui.allocate_painter(available.size(), egui::Sense::click_and_drag());
    let canvas_rect = response.rect;

    // Handle pan (drag)
    if response.dragged_by(egui::PointerButton::Middle)
        || (response.dragged_by(egui::PointerButton::Primary)
            && ui.input(|i| i.modifiers.shift))
    {
        state.pan_offset += response.drag_delta();
    }

    // Handle zoom (scroll)
    let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
    if scroll_delta != 0.0 {
        let factor = 1.0 + scroll_delta * 0.002;
        state.zoom = (state.zoom * factor).clamp(0.25, 4.0);
    }

    let origin = canvas_rect.min.to_vec2() + state.pan_offset;

    // Draw edges first (behind nodes)
    for edge in &topology.edges {
        let from_pos = state
            .node_positions
            .get(&edge.from_node)
            .copied()
            .unwrap_or(Pos2::ZERO);
        let to_pos = state
            .node_positions
            .get(&edge.to_node)
            .copied()
            .unwrap_or(Pos2::ZERO);

        let from_screen = Pos2::new(
            from_pos.x * state.zoom + origin.x + NODE_WIDTH * state.zoom,
            from_pos.y * state.zoom + origin.y + NODE_HEIGHT * 0.5 * state.zoom,
        );
        let to_screen = Pos2::new(
            to_pos.x * state.zoom + origin.x,
            to_pos.y * state.zoom + origin.y + NODE_HEIGHT * 0.5 * state.zoom,
        );

        // Draw bezier curve
        let mid_x = (from_screen.x + to_screen.x) * 0.5;
        let cp1 = Pos2::new(mid_x, from_screen.y);
        let cp2 = Pos2::new(mid_x, to_screen.y);

        let points = bezier_points(from_screen, cp1, cp2, to_screen, 32);
        painter.add(egui::Shape::line(
            points,
            Stroke::new(2.0 * state.zoom, Color32::from_gray(150)),
        ));
    }

    // Draw nodes
    for node in &topology.nodes {
        let pos = state
            .node_positions
            .get(&node.id)
            .copied()
            .unwrap_or(Pos2::ZERO);

        let screen_pos = Pos2::new(
            pos.x * state.zoom + origin.x,
            pos.y * state.zoom + origin.y,
        );
        let node_size = Vec2::new(NODE_WIDTH * state.zoom, NODE_HEIGHT * state.zoom);
        let node_rect = Rect::from_min_size(screen_pos, node_size);

        // Determine color based on node name
        let color = node_color(&node.name);
        let is_selected = state.selected_node == Some(node.id);
        let stroke_color = if is_selected {
            Color32::WHITE
        } else {
            Color32::from_gray(80)
        };
        let stroke_width = if is_selected { 3.0 } else { 1.0 };

        // Draw rounded rectangle
        painter.rect_filled(node_rect, 6.0 * state.zoom, color);
        painter.rect_stroke(
            node_rect,
            6.0 * state.zoom,
            Stroke::new(stroke_width * state.zoom, stroke_color),
            egui::StrokeKind::Outside,
        );

        // Draw label
        let text_pos = node_rect.center();
        painter.text(
            text_pos,
            egui::Align2::CENTER_CENTER,
            &node.name,
            egui::FontId::proportional(12.0 * state.zoom),
            Color32::WHITE,
        );

        // Draw port circles
        let port_radius = 4.0 * state.zoom;
        // Input port (left side)
        let input_port = Pos2::new(node_rect.left(), node_rect.center().y);
        painter.circle_filled(input_port, port_radius, Color32::from_gray(200));

        // Output port (right side)
        let output_port = Pos2::new(node_rect.right(), node_rect.center().y);
        painter.circle_filled(output_port, port_radius, Color32::from_gray(200));

        // Handle click to select
        if response.clicked() {
            if let Some(pointer_pos) = response.interact_pointer_pos() {
                if node_rect.contains(pointer_pos) {
                    state.selected_node = Some(node.id);
                }
            }
        }
    }

    // Show selected node info
    if let Some(selected_id) = state.selected_node {
        if let Some(node) = topology.nodes.iter().find(|n| n.id == selected_id) {
            ui.separator();
            ui.label(format!("Selected: {} (ID: {:?})", node.name, node.id));
            ui.label(format!("Ports: {}", node.ports.len()));
            for port in &node.ports {
                ui.label(format!("  {:?}", port));
            }
        }
    }

    actions
}

const NODE_WIDTH: f32 = 140.0;
const NODE_HEIGHT: f32 = 50.0;
const NODE_SPACING_X: f32 = 200.0;
const NODE_SPACING_Y: f32 = 80.0;

/// Simple left-to-right topological layout.
fn auto_layout(state: &mut PipelineEditorState, topology: &TopologySnapshot) {
    // Build adjacency for topological depth calculation
    let mut depth_map: HashMap<NodeId, usize> = HashMap::new();

    // Initialize all nodes to depth 0
    for node in &topology.nodes {
        depth_map.insert(node.id, 0);
    }

    // Iteratively propagate depths through edges
    for _ in 0..topology.nodes.len() {
        for edge in &topology.edges {
            let from_depth = depth_map.get(&edge.from_node).copied().unwrap_or(0);
            let to_depth = depth_map.get(&edge.to_node).copied().unwrap_or(0);
            if to_depth <= from_depth {
                depth_map.insert(edge.to_node, from_depth + 1);
            }
        }
    }

    // Group nodes by depth
    let max_depth = depth_map.values().max().copied().unwrap_or(0);
    let mut columns: Vec<Vec<NodeId>> = vec![Vec::new(); max_depth + 1];
    for node in &topology.nodes {
        let depth = depth_map.get(&node.id).copied().unwrap_or(0);
        columns[depth].push(node.id);
    }

    // Assign positions
    let start_x = 40.0;
    let start_y = 40.0;
    for (col, nodes) in columns.iter().enumerate() {
        for (row, &node_id) in nodes.iter().enumerate() {
            state.node_positions.insert(
                node_id,
                Pos2::new(
                    start_x + col as f32 * NODE_SPACING_X,
                    start_y + row as f32 * NODE_SPACING_Y,
                ),
            );
        }
    }
}

/// Determine node color based on its name/type.
fn node_color(name: &str) -> Color32 {
    let name_lower = name.to_lowercase();
    if name_lower.contains("source") || name_lower.contains("probe") {
        Color32::from_rgb(60, 140, 60) // Green for sources
    } else if name_lower.contains("transform") || name_lower.contains("filter") || name_lower.contains("script") {
        Color32::from_rgb(60, 100, 180) // Blue for transforms
    } else if name_lower.contains("sink") || name_lower.contains("ui") || name_lower.contains("recorder") || name_lower.contains("exporter") {
        Color32::from_rgb(200, 120, 40) // Orange for sinks
    } else {
        Color32::from_rgb(100, 100, 100) // Gray for unknown
    }
}

/// Compute points along a cubic bezier curve.
fn bezier_points(p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2, segments: usize) -> Vec<Pos2> {
    (0..=segments)
        .map(|i| {
            let t = i as f32 / segments as f32;
            let u = 1.0 - t;
            let tt = t * t;
            let uu = u * u;
            let uuu = uu * u;
            let ttt = tt * t;
            Pos2::new(
                uuu * p0.x + 3.0 * uu * t * p1.x + 3.0 * u * tt * p2.x + ttt * p3.x,
                uuu * p0.y + 3.0 * uu * t * p1.y + 3.0 * u * tt * p2.y + ttt * p3.y,
            )
        })
        .collect()
}

impl Pane for PipelineEditorState {
    fn kind(&self) -> PaneKind { PaneKind::PipelineEditor }

    fn render(&mut self, shared: &mut SharedState, ui: &mut Ui) -> Vec<AppAction> {
        render(self, shared, ui)
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
