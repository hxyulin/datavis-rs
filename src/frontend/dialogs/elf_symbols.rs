//! ELF Symbols Dialog
//!
//! Dialog for browsing and selecting symbols from an ELF/AXF file.

use egui::{Color32, Ui};

use super::{Dialog, DialogAction, DialogState, DialogWindowConfig};
use crate::backend::{ElfInfo, ElfSymbol};

/// State for the ELF symbols dialog
#[derive(Default)]
pub struct ElfSymbolsState {
    /// Search filter for symbols
    pub filter: String,
}

impl DialogState for ElfSymbolsState {
    fn reset(&mut self) {
        self.filter.clear();
    }
}

/// Actions that can be returned by the ELF symbols dialog
#[derive(Debug, Clone)]
pub enum ElfSymbolsAction {
    /// Select a symbol to add as a variable
    Select(ElfSymbol),
}

/// Context needed to render the ELF symbols dialog
pub struct ElfSymbolsContext<'a> {
    /// Path to the ELF file (for display)
    pub elf_file_path: Option<&'a std::path::Path>,
    /// List of symbols to display
    pub symbols: &'a [ElfSymbol],
    /// ELF info for type lookups
    pub elf_info: Option<&'a ElfInfo>,
}

/// The ELF symbols dialog
pub struct ElfSymbolsDialog;

impl Dialog for ElfSymbolsDialog {
    type State = ElfSymbolsState;
    type Action = ElfSymbolsAction;
    type Context<'a> = ElfSymbolsContext<'a>;

    fn title(_state: &Self::State) -> &'static str {
        "ELF Symbols"
    }

    fn window_config() -> DialogWindowConfig {
        DialogWindowConfig::resizable(400.0, 500.0)
    }

    fn render(
        state: &mut Self::State,
        ctx: Self::Context<'_>,
        ui: &mut Ui,
    ) -> DialogAction<Self::Action> {
        let mut action = DialogAction::None;

        // File path display
        if let Some(path) = ctx.elf_file_path {
            ui.label(format!("File: {}", path.display()));
        }

        ui.separator();

        // Search filter
        ui.horizontal(|ui| {
            ui.label("Filter:");
            ui.text_edit_singleline(&mut state.filter);
            if ui.button("Clear").clicked() {
                state.filter.clear();
            }
        });

        ui.separator();

        // Show summary
        ui.label(format!("{} variables found", ctx.symbols.len()));

        if ctx.symbols.is_empty() {
            ui.colored_label(Color32::YELLOW, "No variables found in ELF file.");
            ui.label("You can still add variables manually.");
        } else {
            // Symbol list
            egui::ScrollArea::vertical()
                .max_height(350.0)
                .show(ui, |ui| {
                    let filter_lower = state.filter.to_lowercase();
                    for symbol in ctx.symbols {
                        // Match against display name, demangled name, or mangled name
                        if !filter_lower.is_empty() && !symbol.matches(&filter_lower) {
                            continue;
                        }

                        ui.horizontal(|ui| {
                            // Display name (demangled short form)
                            ui.label(&symbol.display_name);
                            ui.label(format!("0x{:08X}", symbol.address));
                            ui.label(format!("{} bytes", symbol.size));

                            // Show type info if available
                            let type_str = ctx
                                .elf_info
                                .map(|info| info.get_symbol_type_name(symbol))
                                .unwrap_or_else(|| symbol.infer_variable_type().to_string());
                            ui.label(type_str);

                            if ui.button("Add").clicked() {
                                action = DialogAction::CloseWithAction(ElfSymbolsAction::Select(
                                    symbol.clone(),
                                ));
                            }
                        })
                        .response
                        .on_hover_ui(|ui| {
                            // Tooltip with full info
                            ui.label(format!("Mangled: {}", symbol.mangled_name));
                            ui.label(format!("Demangled: {}", symbol.demangled_name));
                            ui.label(format!("Section: {}", symbol.section));
                        });
                    }
                });
        }

        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("Close").clicked() {
                action = DialogAction::Close;
            }
        });

        action
    }
}
