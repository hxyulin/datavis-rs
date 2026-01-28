//! Variable Browser pane - ELF file browser with variable tree
//!
//! Extracted from the left panel of the Variables page.

use std::collections::HashSet;

use egui::{Color32, Ui};

use crate::backend::{ElfInfo, ElfSymbol, TypeHandle};
use crate::frontend::state::{AppAction, ChildVariableSpec, SharedState};
use crate::types::Variable;

use crate::frontend::pane_trait::Pane;
use crate::frontend::workspace::PaneKind;

/// Maximum number of array elements to display when expanding an array
const MAX_ARRAY_ELEMENTS: u64 = 1024;

/// State for the Variable Browser pane
#[derive(Default)]
pub struct VariableBrowserState {
    /// Current search query
    pub query: String,
    /// Filtered symbols matching query
    pub filtered_symbols: Vec<ElfSymbol>,
    /// Currently selected index in filtered list
    pub selected_index: Option<usize>,
    /// Set of expanded paths for tree view
    pub expanded_paths: HashSet<String>,
    /// Whether to show unreadable variables
    pub show_unreadable: bool,
    /// Last seen ELF generation (for auto-refreshing on ELF reload)
    last_elf_generation: u64,
}

impl VariableBrowserState {
    /// Update filtered symbols based on query and show_unreadable setting
    pub fn update_filter(&mut self, elf_info: Option<&ElfInfo>) {
        self.filtered_symbols.clear();
        if let Some(info) = elf_info {
            let results = info.search_variables(&self.query);
            self.filtered_symbols = results
                .into_iter()
                .filter(|s| self.show_unreadable || s.is_readable())
                .cloned()
                .collect();
        }
        if self.filtered_symbols.is_empty() {
            self.selected_index = None;
        } else if let Some(idx) = self.selected_index {
            if idx >= self.filtered_symbols.len() {
                self.selected_index = Some(self.filtered_symbols.len() - 1);
            }
        }
    }

    /// Toggle expansion state for a tree path
    pub fn toggle_expanded(&mut self, path: &str) {
        if self.expanded_paths.contains(path) {
            self.expanded_paths.remove(path);
        } else {
            self.expanded_paths.insert(path.to_string());
        }
    }
}

/// Render the variable browser pane
pub fn render(
    state: &mut VariableBrowserState,
    shared: &mut SharedState<'_>,
    ui: &mut Ui,
) -> Vec<AppAction> {
    let mut actions = Vec::new();
    let mut variables_to_add: Vec<Variable> = Vec::new();
    let mut struct_add_actions: Vec<AppAction> = Vec::new();

    // Auto-refresh when ELF is reloaded
    if shared.topics.elf_generation != state.last_elf_generation {
        state.last_elf_generation = shared.topics.elf_generation;
        state.update_filter(shared.elf_info);
    }

    ui.heading("Variable Browser");
    ui.separator();

    // ELF file selection
    ui.horizontal(|ui| {
        ui.label("ELF File:");
        if let Some(path) = shared.elf_file_path {
            let filename = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            ui.label(filename)
                .on_hover_text(path.display().to_string());
        } else {
            ui.label("(none)");
        }
        if ui.button("Browse...").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("ELF/AXF", &["elf", "axf", "out"])
                .pick_file()
            {
                actions.push(AppAction::LoadElf(path));
            }
        }
    });

    if shared.elf_info.is_some() {
        ui.horizontal(|ui| {
            ui.label(format!(
                "{} variables available",
                shared.elf_symbols.len()
            ));
        });
    }

    ui.separator();

    // Search filter
    ui.horizontal(|ui| {
        ui.label("Search:");
        let response = ui.text_edit_singleline(&mut state.query);
        if response.changed() {
            state.update_filter(shared.elf_info);
        }
        if ui.button("Clear").clicked() {
            state.query.clear();
            state.update_filter(shared.elf_info);
        }
    });

    // Show unreadable toggle
    ui.horizontal(|ui| {
        if ui
            .checkbox(&mut state.show_unreadable, "Show unreadable")
            .on_hover_text("Show optimized out, extern, and local variables")
            .changed()
        {
            state.update_filter(shared.elf_info);
        }
    });

    // Diagnostics summary (collapsible)
    if let Some(elf_info) = shared.elf_info {
        let diagnostics = elf_info.get_diagnostics();
        if diagnostics.total_variables > 0 {
            ui.collapsing("Debug Info Statistics", |ui| {
                render_diagnostics_summary(ui, diagnostics);
            });
        }
    }

    ui.separator();

    // Variable tree browser
    let mut toggle_expand_path: Option<String> = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if state.filtered_symbols.is_empty() {
                if shared.elf_info.is_some() {
                    ui.colored_label(Color32::GRAY, "No matching variables");
                } else {
                    ui.colored_label(Color32::GRAY, "Load an ELF file to browse variables");
                }
            } else {
                for idx in 0..state.filtered_symbols.len() {
                    let symbol = &state.filtered_symbols[idx];
                    let root_path = idx.to_string();

                    let is_unreadable = !symbol.is_readable();
                    if is_unreadable {
                        ui.horizontal(|ui| {
                            ui.add_space(18.0);
                            let status_text = symbol
                                .detailed_unreadable_reason()
                                .unwrap_or_else(|| "Cannot read".to_string());
                            let label = ui.colored_label(Color32::YELLOW, "⚠");
                            label.on_hover_text(&status_text);
                            ui.label(
                                egui::RichText::new(&symbol.display_name).color(Color32::GRAY),
                            );
                            ui.label(
                                egui::RichText::new(format!("- {}", status_text))
                                    .small()
                                    .color(Color32::DARK_GRAY),
                            );
                        });
                    } else {
                        render_type_tree(
                            ui,
                            &symbol.display_name,
                            symbol.address,
                            shared
                                .elf_info
                                .and_then(|info| info.symbol_type_handle(symbol)),
                            &root_path,
                            0,
                            &state.expanded_paths,
                            state.selected_index == Some(idx),
                            &mut toggle_expand_path,
                            &mut variables_to_add,
                            &mut struct_add_actions,
                            &mut None,
                            None,
                        );
                    }
                }
            }
        });

    // Handle toggle expand
    if let Some(path) = toggle_expand_path {
        state.toggle_expanded(&path);
    }

    // Convert variables_to_add into actions
    for var in variables_to_add {
        actions.push(AppAction::AddVariable(var));
    }

    actions.extend(struct_add_actions);

    actions
}

/// Render diagnostics summary showing DWARF parsing statistics
fn render_diagnostics_summary(ui: &mut Ui, diagnostics: &crate::backend::DwarfDiagnostics) {
    ui.label(format!("Total variables: {}", diagnostics.total_variables));

    let readable = diagnostics.with_valid_address + diagnostics.address_zero;
    ui.label(format!(
        "Readable: {} ({:.0}%)",
        readable,
        if diagnostics.total_variables > 0 {
            100.0 * readable as f64 / diagnostics.total_variables as f64
        } else {
            0.0
        }
    ));

    if diagnostics.optimized_out > 0 {
        ui.colored_label(
            Color32::YELLOW,
            format!("Optimized out: {}", diagnostics.optimized_out),
        )
        .on_hover_text("Variables removed by compiler optimization");
    }

    if diagnostics.register_only > 0 {
        ui.colored_label(
            Color32::LIGHT_BLUE,
            format!("Register-only: {}", diagnostics.register_only),
        )
        .on_hover_text("Values live in CPU registers without memory backing");
    }

    if diagnostics.implicit_value > 0 {
        ui.label(format!(
            "Implicit values: {}",
            diagnostics.implicit_value
        ))
        .on_hover_text("Computed values (DW_OP_stack_value)");
    }

    if diagnostics.multi_piece > 0 {
        ui.label(format!("Multi-piece: {}", diagnostics.multi_piece))
            .on_hover_text("Variables split across registers/memory");
    }

    if diagnostics.artificial > 0 {
        ui.colored_label(
            Color32::DARK_GRAY,
            format!("Compiler-generated: {}", diagnostics.artificial),
        )
        .on_hover_text("Artificial variables (this pointers, VLA bounds, etc.)");
    }

    if diagnostics.local_variables > 0 {
        ui.colored_label(
            Color32::GRAY,
            format!("Local variables: {}", diagnostics.local_variables),
        )
        .on_hover_text("Stack/register variables requiring runtime context");
    }

    if diagnostics.extern_declarations > 0 {
        ui.label(format!(
            "Extern declarations: {}",
            diagnostics.extern_declarations
        ))
        .on_hover_text("Variables declared but defined elsewhere");
    }

    if diagnostics.compile_time_constants > 0 {
        ui.label(format!(
            "Compile-time constants: {}",
            diagnostics.compile_time_constants
        ))
        .on_hover_text("Constants with no runtime storage");
    }

    if diagnostics.address_zero > 0 {
        ui.label(format!("Address 0: {}", diagnostics.address_zero))
            .on_hover_text("Variables at address 0 (may be valid on embedded targets)");
    }

    if diagnostics.implicit_pointer > 0 {
        ui.label(format!(
            "Implicit pointers: {}",
            diagnostics.implicit_pointer
        ))
        .on_hover_text("Pointers to optimized-out data");
    }

    if diagnostics.unresolved_types > 0 {
        ui.colored_label(
            Color32::RED,
            format!("Unresolved types: {}", diagnostics.unresolved_types),
        )
        .on_hover_text("Variables with missing type information");
    }
}

/// Render a type tree node for struct/array/pointer navigation
#[allow(clippy::too_many_arguments)]
fn render_type_tree(
    ui: &mut Ui,
    name: &str,
    address: u64,
    type_handle: Option<TypeHandle>,
    path: &str,
    indent_level: usize,
    expanded_paths: &HashSet<String>,
    is_selected: bool,
    toggle_expand_path: &mut Option<String>,
    variables_to_add: &mut Vec<Variable>,
    struct_add_actions: &mut Vec<AppAction>,
    symbol_to_use: &mut Option<ElfSymbol>,
    root_symbol: Option<&ElfSymbol>,
) {
    let is_expanded = expanded_paths.contains(path);
    let type_name = type_handle
        .as_ref()
        .map(|h| h.type_name())
        .unwrap_or_else(|| "unknown".to_string());
    let size = type_handle.as_ref().and_then(|h| h.size()).unwrap_or(0);

    let can_expand = type_handle
        .as_ref()
        .map(|h| h.is_expandable())
        .unwrap_or(false);

    let is_addable = type_handle
        .as_ref()
        .map(|h| h.is_addable())
        .unwrap_or(true);

    let underlying = type_handle.as_ref().map(|h| h.underlying());

    let is_pointer = underlying
        .as_ref()
        .map(|h| h.is_pointer_or_reference())
        .unwrap_or(false);

    let members = underlying.as_ref().and_then(|h| {
        if let Some(members) = h.members() {
            return Some((members.to_vec(), h.clone()));
        }
        None
    });

    let array_info = underlying.as_ref().and_then(|h| {
        if !h.is_pointer_or_reference() && h.is_array() {
            let count = h.array_count().unwrap_or(0);
            let elem_size = h.element_size().unwrap_or(0);
            let elem_type = h.element_type();
            if count > 0 && elem_size > 0 {
                Some((count.min(MAX_ARRAY_ELEMENTS), elem_size, elem_type))
            } else {
                None
            }
        } else {
            None
        }
    });

    ui.horizontal(|ui| {
        ui.add_space((indent_level * 20) as f32);

        if can_expand {
            let expand_icon = if is_expanded { "v" } else { ">" };
            if ui.small_button(expand_icon).clicked() {
                *toggle_expand_path = Some(path.to_string());
            }
        } else {
            ui.add_space(18.0);
        }

        let display_text = if indent_level == 0 {
            format!(
                "{} @ 0x{:08X} ({} bytes) - {}",
                name, address, size, type_name
            )
        } else {
            let short_name = name.rsplit('.').next().unwrap_or(name);
            format!(".{}: {} @ 0x{:08X}", short_name, type_name, address)
        };

        let response = ui.selectable_label(is_selected && indent_level == 0, &display_text);

        if response.double_clicked() {
            if let Some(sym) = root_symbol {
                *symbol_to_use = Some(sym.clone());
            }
        }

        if is_addable {
            let hover_text = if is_pointer {
                "Add pointer value as variable"
            } else {
                "Add as variable"
            };
            if ui.small_button("+").on_hover_text(hover_text).clicked() {
                let var_type = type_handle
                    .as_ref()
                    .map(|h| h.to_variable_type())
                    .unwrap_or(crate::types::VariableType::U32);
                let var = Variable::new(name, address, var_type);
                variables_to_add.push(var);
            }
        }

        // "Add all" button for expandable (struct/array) types with children
        if can_expand && !is_pointer {
            if ui.small_button("+all").on_hover_text("Add struct with all fields").clicked() {
                let parent_type = type_handle
                    .as_ref()
                    .map(|h| h.to_variable_type())
                    .unwrap_or(crate::types::VariableType::U32);
                let parent_var = Variable::new(name, address, parent_type);
                let children = collect_children_from_type(&type_handle, name, address);
                struct_add_actions.push(AppAction::AddStructVariable {
                    parent: parent_var,
                    children,
                });
            }
        }
    });

    // Render expanded members or array elements
    if is_expanded {
        if let Some((member_list, parent_handle)) = members {
            for member in &member_list {
                let member_path = format!("{}.{}", path, member.name);
                let member_addr = address + member.offset;
                let full_name = format!("{}.{}", name, member.name);
                let member_type_handle = parent_handle.member_type(member);

                render_type_tree(
                    ui,
                    &full_name,
                    member_addr,
                    Some(member_type_handle),
                    &member_path,
                    indent_level + 1,
                    expanded_paths,
                    false,
                    toggle_expand_path,
                    variables_to_add,
                    struct_add_actions,
                    symbol_to_use,
                    root_symbol,
                );
            }
        }

        if let Some((count, elem_size, elem_type)) = array_info {
            for i in 0..count {
                let elem_path = format!("{}[{}]", path, i);
                let elem_addr = address + (i * elem_size);
                let full_name = format!("{}[{}]", name, i);

                render_type_tree(
                    ui,
                    &full_name,
                    elem_addr,
                    elem_type.clone(),
                    &elem_path,
                    indent_level + 1,
                    expanded_paths,
                    false,
                    toggle_expand_path,
                    variables_to_add,
                    struct_add_actions,
                    symbol_to_use,
                    root_symbol,
                );
            }

            if count == MAX_ARRAY_ELEMENTS {
                let original_count = underlying
                    .as_ref()
                    .and_then(|h| h.array_count())
                    .unwrap_or(0);
                if original_count > MAX_ARRAY_ELEMENTS {
                    ui.horizontal(|ui| {
                        ui.add_space(((indent_level + 1) * 20) as f32);
                        ui.colored_label(
                            Color32::YELLOW,
                            format!(
                                "... {} more elements (truncated)",
                                original_count - MAX_ARRAY_ELEMENTS
                            ),
                        );
                    });
                }
            }
        }
    }
}

/// Collect leaf children from a struct/array type handle for AddStructVariable.
/// Walks members recursively, collecting only leaf (addable, non-expandable) fields.
fn collect_children_from_type(
    type_handle: &Option<TypeHandle>,
    parent_name: &str,
    parent_address: u64,
) -> Vec<ChildVariableSpec> {
    let mut children = Vec::new();
    let Some(handle) = type_handle else {
        return children;
    };

    let underlying = handle.underlying();

    // Struct members
    if let Some(members) = underlying.members() {
        for member in members {
            let member_addr = parent_address + member.offset;
            let full_name = format!("{}.{}", parent_name, member.name);
            let member_type = handle.member_type(member);
            let member_underlying = member_type.underlying();

            if member_underlying.is_expandable() && !member_underlying.is_pointer_or_reference() {
                // Recurse into nested structs
                let nested = collect_children_from_type(
                    &Some(member_type),
                    &full_name,
                    member_addr,
                );
                children.extend(nested);
            } else if member_type.is_addable() {
                children.push(ChildVariableSpec {
                    name: full_name,
                    address: member_addr,
                    var_type: member_type.to_variable_type(),
                });
            }
        }
    }

    // Array elements
    if !underlying.is_pointer_or_reference() && underlying.is_array() {
        let count = underlying.array_count().unwrap_or(0).min(MAX_ARRAY_ELEMENTS);
        let elem_size = underlying.element_size().unwrap_or(0);
        let elem_type = underlying.element_type();
        if count > 0 && elem_size > 0 {
            for i in 0..count {
                let elem_addr = parent_address + (i * elem_size);
                let full_name = format!("{}[{}]", parent_name, i);
                if let Some(ref et) = elem_type {
                    let et_underlying = et.underlying();
                    if et_underlying.is_expandable() && !et_underlying.is_pointer_or_reference() {
                        let nested = collect_children_from_type(
                            &elem_type,
                            &full_name,
                            elem_addr,
                        );
                        children.extend(nested);
                    } else if et.is_addable() {
                        children.push(ChildVariableSpec {
                            name: full_name,
                            address: elem_addr,
                            var_type: et.to_variable_type(),
                        });
                    }
                }
            }
        }
    }

    children
}

impl Pane for VariableBrowserState {
    fn kind(&self) -> PaneKind { PaneKind::VariableBrowser }

    fn render(&mut self, shared: &mut SharedState, ui: &mut Ui) -> Vec<AppAction> {
        render(self, shared, ui)
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
