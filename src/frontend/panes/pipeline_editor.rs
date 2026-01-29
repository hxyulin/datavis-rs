//! Pipeline Editor pane — visual display and editing of the pipeline graph.
//!
//! Renders the pipeline topology as a node graph using custom egui painting.
//! Supports:
//! - Viewing nodes and edges with pan/zoom
//! - Edit mode for adding/removing nodes and edges
//! - Script editor panel for RhaiScript nodes

use std::collections::HashMap;

use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};

use crate::frontend::pane_trait::Pane;
use crate::frontend::state::{AppAction, SharedState};
use crate::frontend::workspace::PaneKind;
use crate::pipeline::bridge::TopologySnapshot;
use crate::pipeline::id::NodeId;
use crate::pipeline::node_type::NodeType;
use crate::pipeline::packet::ConfigValue;

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
    /// Edit mode enabled.
    pub edit_mode: bool,
    /// Node being dragged from for edge creation (node_id, start_screen_pos).
    pub dragging_edge_from: Option<(NodeId, Pos2)>,
    /// Node being dragged for repositioning (node_id, offset from mouse to node pos).
    pub dragging_node: Option<(NodeId, Vec2)>,
    /// Whether add node menu is open.
    pub add_node_menu_open: bool,
    /// Script content for each Rhai node (cached locally for editing).
    pub node_scripts: HashMap<NodeId, String>,
    /// Whether script has unsaved changes.
    pub script_dirty: bool,
    /// Filter config for each Filter node: allowed var IDs as comma-separated string.
    pub filter_configs: HashMap<NodeId, String>,
    /// Whether each Filter node is in invert mode.
    pub filter_invert_mode: HashMap<NodeId, bool>,
    /// Whether filter config has unsaved changes.
    pub filter_dirty: bool,
    /// GraphSink pane ID config: node_id -> pane_id string.
    pub graph_sink_pane_ids: HashMap<NodeId, String>,
    /// Currently hovered node (for tooltip display).
    pub hovered_node: Option<NodeId>,
}

impl Default for PipelineEditorState {
    fn default() -> Self {
        Self {
            node_positions: HashMap::new(),
            pan_offset: Vec2::ZERO,
            zoom: 1.0,
            selected_node: None,
            topology_requested: false,
            edit_mode: false,
            dragging_edge_from: None,
            dragging_node: None,
            add_node_menu_open: false,
            node_scripts: HashMap::new(),
            script_dirty: false,
            filter_configs: HashMap::new(),
            filter_invert_mode: HashMap::new(),
            filter_dirty: false,
            graph_sink_pane_ids: HashMap::new(),
            hovered_node: None,
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

    // Toolbar
    ui.horizontal(|ui| {
        ui.heading("Pipeline Editor");
        ui.separator();
        if ui.button("Refresh").clicked() {
            actions.push(AppAction::RequestTopology);
        }
        ui.separator();
        ui.checkbox(&mut state.edit_mode, "Edit Mode");
        if state.edit_mode {
            ui.separator();
            if ui.button("+ Add Node").clicked() {
                state.add_node_menu_open = !state.add_node_menu_open;
            }
        }
    });
    ui.separator();

    // Add node popup menu
    if state.add_node_menu_open {
        egui::Window::new("Add Node")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::LEFT_TOP, [10.0, 60.0])
            .show(ui.ctx(), |ui| {
                ui.label("Select node type:");
                ui.separator();
                for node_type in NodeType::all() {
                    if ui.button(node_type.display_name()).clicked() {
                        actions.push(AppAction::AddPipelineNode(*node_type));
                        state.add_node_menu_open = false;
                        // Clear positions to trigger re-layout
                        state.node_positions.clear();
                    }
                }
                ui.separator();
                if ui.button("Cancel").clicked() {
                    state.add_node_menu_open = false;
                }
            });
    }

    let Some(topology) = shared.topics.topology.clone() else {
        ui.label("Waiting for topology...");
        return actions;
    };

    // Reset hover state at the start of each frame
    state.hovered_node = None;

    // Auto-layout if node positions are empty
    if state.node_positions.is_empty() && !topology.nodes.is_empty() {
        auto_layout(state, &topology);
    }

    // Calculate canvas size - leave room for selected node info panel
    let available = ui.available_rect_before_wrap();
    let canvas_height = if state.selected_node.is_some() {
        (available.height() * 0.65).max(200.0)
    } else {
        available.height()
    };
    let canvas_size = Vec2::new(available.width(), canvas_height);

    // Draw the graph
    let (response, painter) = ui.allocate_painter(canvas_size, egui::Sense::click_and_drag());
    let canvas_rect = response.rect;

    // Fill background
    painter.rect_filled(canvas_rect, 0.0, Color32::from_gray(30));

    // Handle pan (middle mouse or shift+drag)
    if response.dragged_by(egui::PointerButton::Middle)
        || (response.dragged_by(egui::PointerButton::Primary)
            && ui.input(|i| i.modifiers.shift)
            && state.dragging_edge_from.is_none())
    {
        state.pan_offset += response.drag_delta();
    }

    // Handle zoom (scroll)
    if response.hovered() {
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_delta != 0.0 {
            let factor = 1.0 + scroll_delta * 0.002;
            state.zoom = (state.zoom * factor).clamp(0.25, 4.0);
        }
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

    // Draw edge being dragged
    if let Some((_, start_pos)) = state.dragging_edge_from {
        if let Some(pointer_pos) = response.hover_pos() {
            painter.line_segment(
                [start_pos, pointer_pos],
                Stroke::new(2.0 * state.zoom, Color32::YELLOW),
            );
        }
    }

    // Track which node's output port was clicked (for edge creation)
    let mut clicked_output_port: Option<(NodeId, Pos2)> = None;
    let mut clicked_input_port: Option<NodeId> = None;
    let mut clicked_node: Option<NodeId> = None;
    let mut drag_started_on_node: Option<(NodeId, Vec2)> = None;

    // Draw nodes
    let port_radius = 6.0 * state.zoom;
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
        // Input port (left side)
        let input_port_pos = Pos2::new(node_rect.left(), node_rect.center().y);
        let input_port_hovered = response
            .hover_pos()
            .map(|p| (p - input_port_pos).length() < port_radius * 1.5)
            .unwrap_or(false);
        let input_port_color = if input_port_hovered && state.edit_mode {
            Color32::LIGHT_GREEN
        } else {
            Color32::from_gray(200)
        };
        painter.circle_filled(input_port_pos, port_radius, input_port_color);

        // Output port (right side)
        let output_port_pos = Pos2::new(node_rect.right(), node_rect.center().y);
        let output_port_hovered = response
            .hover_pos()
            .map(|p| (p - output_port_pos).length() < port_radius * 1.5)
            .unwrap_or(false);
        let output_port_color = if output_port_hovered && state.edit_mode {
            Color32::LIGHT_BLUE
        } else {
            Color32::from_gray(200)
        };
        painter.circle_filled(output_port_pos, port_radius, output_port_color);

        // Check for hover (for tooltip)
        if let Some(hover_pos) = ui.ctx().pointer_hover_pos() {
            if node_rect.contains(hover_pos) {
                state.hovered_node = Some(node.id);
            }
        }

        // Check for clicks and drag start
        if let Some(pointer_pos) = response.interact_pointer_pos() {
            let on_output_port = (pointer_pos - output_port_pos).length() < port_radius * 2.0;
            let on_input_port = (pointer_pos - input_port_pos).length() < port_radius * 2.0;
            let on_node_body = node_rect.contains(pointer_pos) && !on_output_port && !on_input_port;

            if response.clicked() {
                // Check output port click (for starting edge drag)
                if state.edit_mode && on_output_port {
                    clicked_output_port = Some((node.id, output_port_pos));
                }
                // Check input port click (for ending edge drag)
                else if state.edit_mode && on_input_port {
                    clicked_input_port = Some(node.id);
                }
                // Check node click (for selection)
                else if on_node_body {
                    clicked_node = Some(node.id);
                }
            }

            // Check for drag start on node body (for repositioning)
            if response.drag_started_by(egui::PointerButton::Primary) && on_node_body && state.dragging_node.is_none() {
                // Calculate offset from mouse to node position (in world coords)
                let world_pos = Pos2::new(
                    (pointer_pos.x - origin.x) / state.zoom,
                    (pointer_pos.y - origin.y) / state.zoom,
                );
                let offset = pos.to_vec2() - world_pos.to_vec2();
                drag_started_on_node = Some((node.id, offset));
            }
        }
    }

    // Handle node dragging (repositioning)
    if let Some((node_id, offset)) = drag_started_on_node {
        state.dragging_node = Some((node_id, offset));
        state.selected_node = Some(node_id);
    }

    if let Some((node_id, offset)) = state.dragging_node {
        if response.dragged_by(egui::PointerButton::Primary) {
            if let Some(pointer_pos) = response.interact_pointer_pos() {
                // Convert screen position to world position
                let world_pos = Pos2::new(
                    (pointer_pos.x - origin.x) / state.zoom + offset.x,
                    (pointer_pos.y - origin.y) / state.zoom + offset.y,
                );
                state.node_positions.insert(node_id, world_pos);
            }
        }

        // Stop dragging on release
        if response.drag_stopped() {
            state.dragging_node = None;
        }
    }

    // Handle port/node clicks (only if not dragging a node)
    if state.dragging_node.is_none() {
        if let Some((from_id, start_pos)) = clicked_output_port {
            state.dragging_edge_from = Some((from_id, start_pos));
        } else if let Some(to_id) = clicked_input_port {
            // Check if we're dragging an edge
            if let Some((from_id, _)) = state.dragging_edge_from.take() {
                if from_id != to_id {
                    actions.push(AppAction::AddPipelineEdge {
                        from_node: from_id,
                        to_node: to_id,
                    });
                }
            }
        } else if let Some(node_id) = clicked_node {
            state.selected_node = Some(node_id);
            state.dragging_edge_from = None;
        } else if response.clicked() {
            // Clicked on empty space - deselect and cancel edge drag
            state.selected_node = None;
            state.dragging_edge_from = None;
        }
    }

    // Handle delete key for selected node
    if state.edit_mode && state.selected_node.is_some() {
        if ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
            let node_id = state.selected_node.unwrap();
            actions.push(AppAction::RemovePipelineNode(node_id));
            state.selected_node = None;
            state.node_positions.clear(); // Trigger re-layout
        }
    }

    // Show selected node info panel
    if let Some(selected_id) = state.selected_node {
        if let Some(node) = topology.nodes.iter().find(|n| n.id == selected_id) {
            ui.separator();
            ui.horizontal(|ui| {
                ui.strong(format!("Selected: {}", node.name));
                ui.label(format!("(ID: {:?})", node.id));
                if state.edit_mode {
                    ui.separator();
                    if ui
                        .button("Delete")
                        .on_hover_text("Delete this node (or press Delete key)")
                        .clicked()
                    {
                        actions.push(AppAction::RemovePipelineNode(selected_id));
                        state.selected_node = None;
                        state.node_positions.clear();
                    }
                }
            });

            // Show ports
            ui.collapsing("Ports", |ui| {
                for port in &node.ports {
                    ui.label(format!(
                        "  {} ({:?}, {:?})",
                        port.name, port.direction, port.kind
                    ));
                }
            });

            // Show script editor for Rhai Script nodes
            let is_rhai_node = node.name.contains("Rhai") || node.name == "Rhai Script";
            if is_rhai_node {
                ui.separator();
                ui.label("Script:");

                // Get or initialize script content
                let script = state
                    .node_scripts
                    .entry(selected_id)
                    .or_insert_with(|| "// Enter your Rhai script here\nsamples".to_string())
                    .clone();

                let mut edited_script = script.clone();
                let text_edit = egui::TextEdit::multiline(&mut edited_script)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(8)
                    .font(egui::TextStyle::Monospace);

                let response = ui.add(text_edit);
                if response.changed() {
                    state.node_scripts.insert(selected_id, edited_script.clone());
                    state.script_dirty = true;
                }

                ui.horizontal(|ui| {
                    if ui
                        .button("Apply Script")
                        .on_hover_text("Send script to the pipeline node")
                        .clicked()
                    {
                        let script_content = state
                            .node_scripts
                            .get(&selected_id)
                            .cloned()
                            .unwrap_or_default();
                        actions.push(AppAction::NodeConfig {
                            node_id: selected_id,
                            key: "script".to_string(),
                            value: ConfigValue::String(script_content),
                        });
                        state.script_dirty = false;
                    }
                    if state.script_dirty {
                        ui.label("(unsaved changes)");
                    }
                });

                // Show example scripts
                ui.collapsing("Example Scripts", |ui| {
                    if ui.button("Passthrough").clicked() {
                        state
                            .node_scripts
                            .insert(selected_id, "samples".to_string());
                        state.script_dirty = true;
                    }
                    if ui.button("Double all values").clicked() {
                        state.node_scripts.insert(
                            selected_id,
                            r#"let len = samples.len();
for i in 0..len {
    samples[i].converted = samples[i].converted * 2.0;
}
samples"#
                                .to_string(),
                        );
                        state.script_dirty = true;
                    }
                    if ui.button("Lowpass filter 10Hz").clicked() {
                        state.node_scripts.insert(
                            selected_id,
                            r#"let len = samples.len();
for i in 0..len {
    samples[i].converted = lowpass(samples[i].converted, 10.0);
}
samples"#
                                .to_string(),
                        );
                        state.script_dirty = true;
                    }
                });
            }

            // Show filter config UI for Filter nodes
            let is_filter_node = node.name == "Filter";
            if is_filter_node {
                ui.separator();
                ui.label("Variable Filter:");

                // Get or initialize filter config
                let filter_config = state
                    .filter_configs
                    .entry(selected_id)
                    .or_insert_with(String::new)
                    .clone();

                let mut edited_config = filter_config.clone();
                ui.horizontal(|ui| {
                    ui.label("Allowed var IDs:");
                    let response = ui.text_edit_singleline(&mut edited_config);
                    if response.changed() {
                        state.filter_configs.insert(selected_id, edited_config.clone());
                        state.filter_dirty = true;
                    }
                });
                ui.label("(comma-separated, e.g., \"0,1,5\". Empty = passthrough all)");

                // Invert mode checkbox
                let mut invert = *state.filter_invert_mode.get(&selected_id).unwrap_or(&false);
                if ui.checkbox(&mut invert, "Invert mode (block listed vars instead)").changed() {
                    state.filter_invert_mode.insert(selected_id, invert);
                    state.filter_dirty = true;
                }

                ui.horizontal(|ui| {
                    if ui
                        .button("Apply Filter")
                        .on_hover_text("Send filter config to the pipeline node")
                        .clicked()
                    {
                        let config = state
                            .filter_configs
                            .get(&selected_id)
                            .cloned()
                            .unwrap_or_default();
                        let invert = *state.filter_invert_mode.get(&selected_id).unwrap_or(&false);

                        actions.push(AppAction::NodeConfig {
                            node_id: selected_id,
                            key: "allowed_vars".to_string(),
                            value: ConfigValue::String(config),
                        });
                        actions.push(AppAction::NodeConfig {
                            node_id: selected_id,
                            key: "invert_mode".to_string(),
                            value: ConfigValue::Bool(invert),
                        });
                        state.filter_dirty = false;
                    }
                    if ui.button("Clear Filter").clicked() {
                        state.filter_configs.insert(selected_id, String::new());
                        state.filter_invert_mode.insert(selected_id, false);
                        actions.push(AppAction::NodeConfig {
                            node_id: selected_id,
                            key: "clear".to_string(),
                            value: ConfigValue::Bool(true),
                        });
                        state.filter_dirty = false;
                    }
                    if state.filter_dirty {
                        ui.label("(unsaved changes)");
                    }
                });

                // Show quick variable selection from variable tree
                ui.collapsing("Quick Select Variables", |ui| {
                    ui.label("Click to add/remove from filter:");
                    for var_node in &shared.topics.variable_tree {
                        if var_node.is_leaf && var_node.enabled {
                            let var_id_str = var_node.id.0.to_string();
                            let current_config = state
                                .filter_configs
                                .get(&selected_id)
                                .cloned()
                                .unwrap_or_default();
                            let ids: Vec<&str> = current_config.split(',')
                                .map(|s| s.trim())
                                .filter(|s| !s.is_empty())
                                .collect();
                            let is_selected = ids.contains(&var_id_str.as_str());

                            let label = if is_selected {
                                format!("✓ {} (ID: {})", var_node.short_name, var_node.id.0)
                            } else {
                                format!("  {} (ID: {})", var_node.short_name, var_node.id.0)
                            };

                            if ui.button(&label).clicked() {
                                let mut id_set: std::collections::HashSet<String> = ids
                                    .iter()
                                    .map(|s| s.to_string())
                                    .collect();
                                if is_selected {
                                    id_set.remove(&var_id_str);
                                } else {
                                    id_set.insert(var_id_str);
                                }
                                let new_config: Vec<String> = id_set.into_iter().collect();
                                state.filter_configs.insert(selected_id, new_config.join(","));
                                state.filter_dirty = true;
                            }
                        }
                    }
                });
            }

            // Show GraphSink config UI
            let is_graph_sink = node.name == "GraphSink";
            if is_graph_sink {
                ui.separator();
                ui.label("Graph Sink Configuration:");

                // Get or initialize pane_id config
                let pane_id_str = state
                    .graph_sink_pane_ids
                    .entry(selected_id)
                    .or_insert_with(String::new)
                    .clone();

                let mut edited_pane_id = pane_id_str.clone();
                ui.horizontal(|ui| {
                    ui.label("Linked Pane ID:");
                    let response = ui.text_edit_singleline(&mut edited_pane_id);
                    if response.changed() {
                        state.graph_sink_pane_ids.insert(selected_id, edited_pane_id.clone());
                    }
                });
                ui.label("(Enter a pane ID number, or leave empty for broadcast mode)");

                ui.horizontal(|ui| {
                    if ui
                        .button("Apply")
                        .on_hover_text("Set the pane ID for this GraphSink")
                        .clicked()
                    {
                        let pane_id_config = state
                            .graph_sink_pane_ids
                            .get(&selected_id)
                            .cloned()
                            .unwrap_or_default();

                        if pane_id_config.is_empty() {
                            // Clear pane_id (broadcast mode)
                            actions.push(AppAction::NodeConfig {
                                node_id: selected_id,
                                key: "pane_id".to_string(),
                                value: ConfigValue::String("none".to_string()),
                            });
                        } else if let Ok(pane_id) = pane_id_config.parse::<i64>() {
                            actions.push(AppAction::NodeConfig {
                                node_id: selected_id,
                                key: "pane_id".to_string(),
                                value: ConfigValue::Int(pane_id),
                            });
                        }
                    }

                    if ui.button("Create New Pane").clicked() {
                        // Create a new TimeSeries pane and link this GraphSink to it
                        actions.push(AppAction::NewVisualizer(crate::frontend::workspace::PaneKind::TimeSeries));
                        // Note: The pane ID won't be available until the pane is created
                        // User needs to manually enter the pane ID after creation
                    }
                });

                ui.small("Tip: Create a new pane, then enter its ID here to link this GraphSink to it.");
            }
        }
    }

    // Show edit mode instructions
    if state.edit_mode {
        ui.separator();
        ui.label("Edit mode: Click output port (right) and then input port (left) to connect. Press Delete to remove selected node.");
    }

    // Show tooltip for hovered node
    if let Some(hovered_id) = state.hovered_node {
        if let Some(node) = topology.nodes.iter().find(|n| n.id == hovered_id) {
            if let Some(node_type) = node.node_type {
                egui::show_tooltip(
                    ui.ctx(),
                    ui.layer_id(),
                    egui::Id::new("node_hover"),
                    |ui| {
                        ui.set_max_width(300.0);

                        // Title
                        ui.label(
                            egui::RichText::new(node_type.display_name())
                                .strong()
                                .size(14.0),
                        );
                        ui.separator();

                        // Description
                        ui.label(node_type.description());

                        // Ports info
                        if !node.ports.is_empty() {
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("Ports:")
                                    .small()
                                    .color(Color32::GRAY),
                            );
                            for port in &node.ports {
                                let arrow = match port.direction {
                                    crate::pipeline::port::PortDirection::Input => "←",
                                    crate::pipeline::port::PortDirection::Output => "→",
                                };
                                ui.label(
                                    egui::RichText::new(format!("{} {}", arrow, port.name))
                                        .small(),
                                );
                            }
                        }
                    },
                );
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
    } else if name_lower.contains("transform")
        || name_lower.contains("filter")
        || name_lower.contains("script")
        || name_lower.contains("rhai")
    {
        Color32::from_rgb(60, 100, 180) // Blue for transforms
    } else if name_lower.contains("sink")
        || name_lower.contains("ui")
        || name_lower.contains("recorder")
        || name_lower.contains("exporter")
    {
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
    fn kind(&self) -> PaneKind {
        PaneKind::PipelineEditor
    }

    fn render(&mut self, shared: &mut SharedState, ui: &mut Ui) -> Vec<AppAction> {
        render(self, shared, ui)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
