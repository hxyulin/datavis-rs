//! Script Editor Widget with Autocomplete
//!
//! A custom text editor widget for editing Rhai converter scripts with
//! syntax highlighting, autocomplete, and inline documentation.
//!
//! # Features
//!
//! - **Syntax highlighting**: Basic Rhai syntax coloring for keywords, strings, and comments
//! - **Autocomplete**: Suggestions for built-in functions, transformers, and variables
//! - **Inline help**: Documentation for available functions and their signatures
//! - **Validation feedback**: Real-time script validation with error display
//!
//! # Usage
//!
//! The script editor is used in the variable configuration dialog to edit
//! converter scripts. Users can type function names and get autocomplete
//! suggestions with documentation.
//!
//! # Available Script Items
//!
//! The editor provides autocomplete for several categories of items:
//!
//! - **Context**: Variables like `value`, `raw`, and functions like `time()`, `dt()`
//! - **Transformers**: Signal processing functions like `derivative()`, `smooth()`, `lowpass()`
//! - **Math**: Mathematical functions like `sin()`, `cos()`, `sqrt()`, `log()`
//! - **Utility**: Helper functions like `map_range()`, `clamp()`, `abs()`
//! - **Constants**: Mathematical constants like `pi()`, `e()`

use egui::{text::LayoutJob, Color32, FontId, RichText, TextFormat, Ui};

/// Documentation for a function or variable available in scripts
#[derive(Debug, Clone)]
pub struct ScriptItem {
    /// Name of the function/variable
    pub name: String,
    /// Short description
    pub description: String,
    /// Signature (for functions)
    pub signature: Option<String>,
    /// Category for grouping
    pub category: ScriptItemCategory,
}

/// Categories for script items
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptItemCategory {
    /// Dynamic context variables: time(), dt(), prev(), etc.
    Context,
    /// Transformer functions: derivative(), smooth(), lowpass(), etc.
    Transformer,
    /// Mathematical functions: sin(), cos(), sqrt(), etc.
    Math,
    /// Utility functions: clamp(), map_range(), etc.
    Utility,
    /// Built-in variables: value, raw
    Variable,
    /// Constants: pi(), e()
    Constant,
}

impl ScriptItemCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Context => "Context",
            Self::Transformer => "Transformers",
            Self::Math => "Math",
            Self::Utility => "Utility",
            Self::Variable => "Variables",
            Self::Constant => "Constants",
        }
    }

    pub fn color(&self) -> Color32 {
        match self {
            Self::Context => Color32::from_rgb(86, 156, 214), // Blue
            Self::Transformer => Color32::from_rgb(220, 220, 170), // Yellow
            Self::Math => Color32::from_rgb(181, 206, 168),   // Green
            Self::Utility => Color32::from_rgb(206, 145, 120), // Orange
            Self::Variable => Color32::from_rgb(156, 220, 254), // Light blue
            Self::Constant => Color32::from_rgb(184, 215, 163), // Light green
        }
    }
}

/// Get all available script items for autocomplete
pub fn get_script_items() -> Vec<ScriptItem> {
    vec![
        // === Context Functions ===
        ScriptItem {
            name: "time".to_string(),
            description: "Time in seconds since collection started".to_string(),
            signature: Some("time() -> f64".to_string()),
            category: ScriptItemCategory::Context,
        },
        ScriptItem {
            name: "dt".to_string(),
            description: "Delta time since last sample in seconds".to_string(),
            signature: Some("dt() -> f64".to_string()),
            category: ScriptItemCategory::Context,
        },
        ScriptItem {
            name: "prev".to_string(),
            description: "Previous converted value (NaN if not available)".to_string(),
            signature: Some("prev() -> f64".to_string()),
            category: ScriptItemCategory::Context,
        },
        ScriptItem {
            name: "prev_raw".to_string(),
            description: "Previous raw value (NaN if not available)".to_string(),
            signature: Some("prev_raw() -> f64".to_string()),
            category: ScriptItemCategory::Context,
        },
        ScriptItem {
            name: "has_prev".to_string(),
            description: "Returns true if previous values are available".to_string(),
            signature: Some("has_prev() -> bool".to_string()),
            category: ScriptItemCategory::Context,
        },
        // === Variables ===
        ScriptItem {
            name: "value".to_string(),
            description: "The current raw value being converted".to_string(),
            signature: None,
            category: ScriptItemCategory::Variable,
        },
        ScriptItem {
            name: "raw".to_string(),
            description: "Alias for 'value' - the current raw value".to_string(),
            signature: None,
            category: ScriptItemCategory::Variable,
        },
        // === Transformer Functions ===
        ScriptItem {
            name: "derivative".to_string(),
            description: "Compute rate of change".to_string(),
            signature: Some("derivative(value) or derivative(current, previous, dt)".to_string()),
            category: ScriptItemCategory::Transformer,
        },
        ScriptItem {
            name: "integrate".to_string(),
            description: "Accumulate value over time".to_string(),
            signature: Some("integrate(value) or integrate(current, accumulated, dt)".to_string()),
            category: ScriptItemCategory::Transformer,
        },
        ScriptItem {
            name: "smooth".to_string(),
            description: "Exponential smoothing (EWMA)".to_string(),
            signature: Some("smooth(value, alpha) where alpha 0-1".to_string()),
            category: ScriptItemCategory::Transformer,
        },
        ScriptItem {
            name: "lowpass".to_string(),
            description: "First-order lowpass filter".to_string(),
            signature: Some("lowpass(value, cutoff_hz)".to_string()),
            category: ScriptItemCategory::Transformer,
        },
        ScriptItem {
            name: "highpass".to_string(),
            description: "First-order highpass filter".to_string(),
            signature: Some("highpass(current, prev_in, prev_out, cutoff_hz, dt)".to_string()),
            category: ScriptItemCategory::Transformer,
        },
        ScriptItem {
            name: "deadband".to_string(),
            description: "Ignore small changes around center value".to_string(),
            signature: Some("deadband(value, center, width)".to_string()),
            category: ScriptItemCategory::Transformer,
        },
        ScriptItem {
            name: "rate_limit".to_string(),
            description: "Limit rate of change".to_string(),
            signature: Some("rate_limit(value, max_rate)".to_string()),
            category: ScriptItemCategory::Transformer,
        },
        ScriptItem {
            name: "hysteresis".to_string(),
            description: "Change output only when crossing thresholds".to_string(),
            signature: Some(
                "hysteresis(input, prev_out, low_thresh, high_thresh, low_val, high_val)"
                    .to_string(),
            ),
            category: ScriptItemCategory::Transformer,
        },
        // === Math Functions ===
        ScriptItem {
            name: "abs".to_string(),
            description: "Absolute value".to_string(),
            signature: Some("abs(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "sqrt".to_string(),
            description: "Square root".to_string(),
            signature: Some("sqrt(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "pow".to_string(),
            description: "Power function".to_string(),
            signature: Some("pow(x, y) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "exp".to_string(),
            description: "Exponential (e^x)".to_string(),
            signature: Some("exp(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "ln".to_string(),
            description: "Natural logarithm".to_string(),
            signature: Some("ln(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "log".to_string(),
            description: "Natural logarithm (alias for ln)".to_string(),
            signature: Some("log(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "log10".to_string(),
            description: "Base-10 logarithm".to_string(),
            signature: Some("log10(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "log2".to_string(),
            description: "Base-2 logarithm".to_string(),
            signature: Some("log2(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "sin".to_string(),
            description: "Sine (radians)".to_string(),
            signature: Some("sin(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "cos".to_string(),
            description: "Cosine (radians)".to_string(),
            signature: Some("cos(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "tan".to_string(),
            description: "Tangent (radians)".to_string(),
            signature: Some("tan(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "asin".to_string(),
            description: "Arc sine".to_string(),
            signature: Some("asin(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "acos".to_string(),
            description: "Arc cosine".to_string(),
            signature: Some("acos(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "atan".to_string(),
            description: "Arc tangent".to_string(),
            signature: Some("atan(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "atan2".to_string(),
            description: "Two-argument arc tangent".to_string(),
            signature: Some("atan2(y, x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "sinh".to_string(),
            description: "Hyperbolic sine".to_string(),
            signature: Some("sinh(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "cosh".to_string(),
            description: "Hyperbolic cosine".to_string(),
            signature: Some("cosh(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "tanh".to_string(),
            description: "Hyperbolic tangent".to_string(),
            signature: Some("tanh(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "floor".to_string(),
            description: "Round down to integer".to_string(),
            signature: Some("floor(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "ceil".to_string(),
            description: "Round up to integer".to_string(),
            signature: Some("ceil(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "round".to_string(),
            description: "Round to nearest integer".to_string(),
            signature: Some("round(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "trunc".to_string(),
            description: "Truncate to integer".to_string(),
            signature: Some("trunc(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "fract".to_string(),
            description: "Fractional part".to_string(),
            signature: Some("fract(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        ScriptItem {
            name: "sign".to_string(),
            description: "Sign of value (-1, 0, or 1)".to_string(),
            signature: Some("sign(x) -> f64".to_string()),
            category: ScriptItemCategory::Math,
        },
        // === Utility Functions ===
        ScriptItem {
            name: "clamp".to_string(),
            description: "Clamp value between min and max".to_string(),
            signature: Some("clamp(x, min, max) -> f64".to_string()),
            category: ScriptItemCategory::Utility,
        },
        ScriptItem {
            name: "min".to_string(),
            description: "Minimum of two values".to_string(),
            signature: Some("min(a, b) -> f64".to_string()),
            category: ScriptItemCategory::Utility,
        },
        ScriptItem {
            name: "max".to_string(),
            description: "Maximum of two values".to_string(),
            signature: Some("max(a, b) -> f64".to_string()),
            category: ScriptItemCategory::Utility,
        },
        ScriptItem {
            name: "lerp".to_string(),
            description: "Linear interpolation".to_string(),
            signature: Some("lerp(a, b, t) -> f64".to_string()),
            category: ScriptItemCategory::Utility,
        },
        ScriptItem {
            name: "map_range".to_string(),
            description: "Map value from one range to another".to_string(),
            signature: Some("map_range(x, in_min, in_max, out_min, out_max) -> f64".to_string()),
            category: ScriptItemCategory::Utility,
        },
        ScriptItem {
            name: "to_int".to_string(),
            description: "Convert to integer".to_string(),
            signature: Some("to_int(x) -> i64".to_string()),
            category: ScriptItemCategory::Utility,
        },
        ScriptItem {
            name: "to_float".to_string(),
            description: "Convert to float".to_string(),
            signature: Some("to_float(x) -> f64".to_string()),
            category: ScriptItemCategory::Utility,
        },
        ScriptItem {
            name: "is_nan".to_string(),
            description: "Check if value is NaN".to_string(),
            signature: Some("is_nan(x) -> bool".to_string()),
            category: ScriptItemCategory::Utility,
        },
        ScriptItem {
            name: "is_finite".to_string(),
            description: "Check if value is finite".to_string(),
            signature: Some("is_finite(x) -> bool".to_string()),
            category: ScriptItemCategory::Utility,
        },
        ScriptItem {
            name: "is_infinite".to_string(),
            description: "Check if value is infinite".to_string(),
            signature: Some("is_infinite(x) -> bool".to_string()),
            category: ScriptItemCategory::Utility,
        },
        ScriptItem {
            name: "bit_and".to_string(),
            description: "Bitwise AND".to_string(),
            signature: Some("bit_and(a, b) -> i64".to_string()),
            category: ScriptItemCategory::Utility,
        },
        ScriptItem {
            name: "bit_or".to_string(),
            description: "Bitwise OR".to_string(),
            signature: Some("bit_or(a, b) -> i64".to_string()),
            category: ScriptItemCategory::Utility,
        },
        ScriptItem {
            name: "bit_xor".to_string(),
            description: "Bitwise XOR".to_string(),
            signature: Some("bit_xor(a, b) -> i64".to_string()),
            category: ScriptItemCategory::Utility,
        },
        ScriptItem {
            name: "bit_not".to_string(),
            description: "Bitwise NOT".to_string(),
            signature: Some("bit_not(a) -> i64".to_string()),
            category: ScriptItemCategory::Utility,
        },
        ScriptItem {
            name: "bit_shl".to_string(),
            description: "Bitwise shift left".to_string(),
            signature: Some("bit_shl(a, b) -> i64".to_string()),
            category: ScriptItemCategory::Utility,
        },
        ScriptItem {
            name: "bit_shr".to_string(),
            description: "Bitwise shift right".to_string(),
            signature: Some("bit_shr(a, b) -> i64".to_string()),
            category: ScriptItemCategory::Utility,
        },
        // === Constants ===
        ScriptItem {
            name: "pi".to_string(),
            description: "Pi constant (3.14159...)".to_string(),
            signature: Some("pi() -> f64".to_string()),
            category: ScriptItemCategory::Constant,
        },
        ScriptItem {
            name: "e".to_string(),
            description: "Euler's number (2.71828...)".to_string(),
            signature: Some("e() -> f64".to_string()),
            category: ScriptItemCategory::Constant,
        },
    ]
}

/// State for the script editor
#[derive(Debug, Clone, Default)]
pub struct ScriptEditorState {
    /// Whether autocomplete popup is open
    pub autocomplete_open: bool,
    /// Current autocomplete suggestions
    pub suggestions: Vec<ScriptItem>,
    /// Selected suggestion index
    pub selected_suggestion: usize,
    /// Last word being typed (for filtering suggestions)
    pub current_word: String,
    /// Cursor position when autocomplete started
    pub autocomplete_start_pos: usize,
    /// Validation error message (if any)
    pub validation_error: Option<String>,
    /// Whether to show the help panel
    pub show_help: bool,
}

impl ScriptEditorState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update suggestions based on current word
    pub fn update_suggestions(&mut self, word: &str) {
        let all_items = get_script_items();
        let word_lower = word.to_lowercase();

        self.suggestions = all_items
            .into_iter()
            .filter(|item| item.name.to_lowercase().starts_with(&word_lower))
            .collect();

        self.selected_suggestion = 0;
    }

    /// Select next suggestion
    pub fn select_next(&mut self) {
        if !self.suggestions.is_empty() {
            self.selected_suggestion = (self.selected_suggestion + 1) % self.suggestions.len();
        }
    }

    /// Select previous suggestion
    pub fn select_prev(&mut self) {
        if !self.suggestions.is_empty() {
            self.selected_suggestion = if self.selected_suggestion == 0 {
                self.suggestions.len() - 1
            } else {
                self.selected_suggestion - 1
            };
        }
    }

    /// Get the currently selected suggestion
    pub fn get_selected(&self) -> Option<&ScriptItem> {
        self.suggestions.get(self.selected_suggestion)
    }
}

/// Script editor widget
pub struct ScriptEditor<'a> {
    script: &'a mut String,
    state: &'a mut ScriptEditorState,
}

impl<'a> ScriptEditor<'a> {
    pub fn new(
        script: &'a mut String,
        state: &'a mut ScriptEditorState,
        _id: impl std::hash::Hash,
    ) -> Self {
        Self { script, state }
    }

    /// Validate the script using the provided engine
    pub fn validate(&mut self, engine: &crate::scripting::ScriptEngine) {
        if self.script.trim().is_empty() {
            self.state.validation_error = None;
        } else {
            match engine.validate(self.script) {
                Ok(()) => self.state.validation_error = None,
                Err(e) => self.state.validation_error = Some(e.to_string()),
            }
        }
    }

    /// Show the script editor
    pub fn show(self, ui: &mut Ui) -> egui::Response {
        let Self { script, state } = self;

        let response = ui
            .vertical(|ui| {
                // Toolbar
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(state.show_help, "📖 Help")
                        .on_hover_text("Show available functions")
                        .clicked()
                    {
                        state.show_help = !state.show_help;
                    }

                    ui.separator();

                    // Preset menu
                    ui.menu_button("📋 Presets", |ui| {
                        ui.label(RichText::new("Basic Converters").strong());
                        for (name, source) in crate::scripting::builtins::all() {
                            if ui.button(name).clicked() {
                                *script = source.trim().to_string();
                                ui.close();
                            }
                        }
                        ui.separator();
                        ui.label(RichText::new("Transformers").strong());
                        for (name, source) in crate::scripting::builtins::transformers() {
                            if ui.button(name).clicked() {
                                *script = source.trim().to_string();
                                ui.close();
                            }
                        }
                    });

                    ui.separator();

                    // Validation status
                    if let Some(ref error) = state.validation_error {
                        ui.label(RichText::new("!").color(Color32::YELLOW))
                            .on_hover_text(error);
                    } else if !script.is_empty() {
                        ui.label(RichText::new("✓").color(Color32::GREEN))
                            .on_hover_text("Script is valid");
                    }
                });

                ui.separator();

                // Main editor area
                let editor_response = ui.horizontal_top(|ui| {
                    // Code editor
                    let text_edit = egui::TextEdit::multiline(script)
                        .code_editor()
                        .desired_width(if state.show_help {
                            300.0
                        } else {
                            f32::INFINITY
                        })
                        .desired_rows(8)
                        .font(FontId::monospace(12.0));

                    let output = text_edit.show(ui);
                    let response = output.response;

                    // Handle autocomplete trigger
                    if response.changed() {
                        // Extract current word at cursor
                        if let Some(cursor) = output.cursor_range {
                            let pos = cursor.primary.index;
                            let word = extract_word_at_cursor(script, pos);
                            state.current_word = word.clone();
                            state.autocomplete_start_pos = pos - word.len();

                            if word.len() >= 2 {
                                state.update_suggestions(&word);
                                state.autocomplete_open = !state.suggestions.is_empty();
                            } else {
                                state.autocomplete_open = false;
                            }
                        }
                    }

                    // Handle keyboard navigation for autocomplete
                    if state.autocomplete_open {
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                            state.select_next();
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                            state.select_prev();
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Tab))
                            || ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl)
                        {
                            // Insert selected suggestion
                            if let Some(item) = state.get_selected() {
                                let completion = if item.signature.is_some() {
                                    format!("{}(", item.name)
                                } else {
                                    item.name.clone()
                                };
                                let start = state.autocomplete_start_pos;
                                let end = start + state.current_word.len();
                                script.replace_range(start..end, &completion);
                                state.autocomplete_open = false;
                            }
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            state.autocomplete_open = false;
                        }
                    }

                    // Show autocomplete popup
                    if state.autocomplete_open && !state.suggestions.is_empty() {
                        egui::Popup::from_response(&response)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .show(|ui| {
                                egui::ScrollArea::vertical()
                                    .max_height(200.0)
                                    .show(ui, |ui| {
                                        for (i, item) in state.suggestions.iter().enumerate() {
                                            let is_selected = i == state.selected_suggestion;
                                            let bg_color = if is_selected {
                                                ui.visuals().selection.bg_fill
                                            } else {
                                                Color32::TRANSPARENT
                                            };

                                            let frame = egui::Frame::new().fill(bg_color);
                                            frame.show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        RichText::new(&item.name)
                                                            .color(item.category.color())
                                                            .monospace(),
                                                    );
                                                    ui.label(
                                                        RichText::new(&item.description)
                                                            .small()
                                                            .color(Color32::GRAY),
                                                    );
                                                });
                                            });
                                        }
                                    });
                            });
                    }

                    // Help panel
                    if state.show_help {
                        ui.separator();
                        render_help_panel(ui);
                    }

                    response
                });

                editor_response.inner
            })
            .inner;

        response
    }
}

/// Extract the word being typed at the cursor position
fn extract_word_at_cursor(text: &str, cursor_pos: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut start = cursor_pos;

    // Walk backwards to find start of word
    while start > 0 {
        let c = chars.get(start - 1).copied().unwrap_or(' ');
        if c.is_alphanumeric() || c == '_' {
            start -= 1;
        } else {
            break;
        }
    }

    // Collect characters from start to cursor
    chars[start..cursor_pos].iter().collect()
}

/// Render the help panel showing available functions
fn render_help_panel(ui: &mut Ui) {
    egui::ScrollArea::vertical()
        .max_height(200.0)
        .show(ui, |ui| {
            ui.set_min_width(250.0);

            let items = get_script_items();
            let categories = [
                ScriptItemCategory::Variable,
                ScriptItemCategory::Context,
                ScriptItemCategory::Transformer,
                ScriptItemCategory::Math,
                ScriptItemCategory::Utility,
                ScriptItemCategory::Constant,
            ];

            for category in categories {
                let category_items: Vec<_> =
                    items.iter().filter(|i| i.category == category).collect();
                if category_items.is_empty() {
                    continue;
                }

                ui.collapsing(
                    RichText::new(category.label())
                        .strong()
                        .color(category.color()),
                    |ui| {
                        for item in category_items {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&item.name)
                                        .monospace()
                                        .color(category.color()),
                                );
                                if let Some(ref sig) = item.signature {
                                    ui.label(RichText::new(sig).small().color(Color32::GRAY));
                                }
                            });
                            ui.label(RichText::new(&item.description).small());
                            ui.add_space(2.0);
                        }
                    },
                );
            }
        });
}

/// Syntax highlighter for Rhai scripts (basic implementation)
pub fn highlight_rhai_script(text: &str) -> LayoutJob {
    let mut job = LayoutJob::default();

    let keywords = [
        "fn", "let", "const", "if", "else", "while", "for", "in", "loop", "break", "continue",
        "return", "true", "false", "null",
    ];

    let builtin_functions: Vec<String> =
        get_script_items().iter().map(|i| i.name.clone()).collect();

    let mut chars = text.chars().peekable();
    let mut current_word = String::new();

    let default_format = TextFormat {
        font_id: FontId::monospace(12.0),
        color: Color32::LIGHT_GRAY,
        ..Default::default()
    };

    let keyword_format = TextFormat {
        font_id: FontId::monospace(12.0),
        color: Color32::from_rgb(86, 156, 214), // Blue
        ..Default::default()
    };

    let function_format = TextFormat {
        font_id: FontId::monospace(12.0),
        color: Color32::from_rgb(220, 220, 170), // Yellow
        ..Default::default()
    };

    let number_format = TextFormat {
        font_id: FontId::monospace(12.0),
        color: Color32::from_rgb(181, 206, 168), // Light green
        ..Default::default()
    };

    let comment_format = TextFormat {
        font_id: FontId::monospace(12.0),
        color: Color32::from_rgb(106, 153, 85), // Green
        ..Default::default()
    };

    let string_format = TextFormat {
        font_id: FontId::monospace(12.0),
        color: Color32::from_rgb(206, 145, 120), // Orange
        ..Default::default()
    };

    while let Some(c) = chars.next() {
        if c.is_alphanumeric() || c == '_' {
            current_word.push(c);
        } else {
            // Flush current word
            if !current_word.is_empty() {
                let format = if keywords.contains(&current_word.as_str()) {
                    keyword_format.clone()
                } else if builtin_functions.contains(&current_word) {
                    function_format.clone()
                } else if current_word.chars().all(|c| c.is_numeric() || c == '.') {
                    number_format.clone()
                } else {
                    default_format.clone()
                };

                job.append(&current_word, 0.0, format);
                current_word.clear();
            }

            // Handle special characters
            if c == '/' && chars.peek() == Some(&'/') {
                // Line comment
                let mut comment = String::from("//");
                chars.next(); // consume second /
                while let Some(&next) = chars.peek() {
                    if next == '\n' {
                        break;
                    }
                    comment.push(chars.next().unwrap());
                }
                job.append(&comment, 0.0, comment_format.clone());
            } else if c == '"' {
                // String literal
                let mut string_lit = String::from("\"");
                while let Some(next) = chars.next() {
                    string_lit.push(next);
                    if next == '"' {
                        break;
                    }
                    if next == '\\' {
                        if let Some(escaped) = chars.next() {
                            string_lit.push(escaped);
                        }
                    }
                }
                job.append(&string_lit, 0.0, string_format.clone());
            } else {
                job.append(&c.to_string(), 0.0, default_format.clone());
            }
        }
    }

    // Flush final word
    if !current_word.is_empty() {
        let format = if keywords.contains(&current_word.as_str()) {
            keyword_format
        } else if builtin_functions.contains(&current_word) {
            function_format
        } else if current_word.chars().all(|c| c.is_numeric() || c == '.') {
            number_format
        } else {
            default_format
        };

        job.append(&current_word, 0.0, format);
    }

    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_word_at_cursor() {
        assert_eq!(extract_word_at_cursor("hello", 5), "hello");
        assert_eq!(extract_word_at_cursor("hello world", 5), "hello");
        assert_eq!(extract_word_at_cursor("hello world", 11), "world");
        assert_eq!(extract_word_at_cursor("sin(", 3), "sin");
        assert_eq!(extract_word_at_cursor("deriv", 5), "deriv");
    }

    #[test]
    fn test_script_items() {
        let items = get_script_items();
        assert!(!items.is_empty());

        // Check that we have items in each category
        assert!(items
            .iter()
            .any(|i| i.category == ScriptItemCategory::Context));
        assert!(items
            .iter()
            .any(|i| i.category == ScriptItemCategory::Transformer));
        assert!(items.iter().any(|i| i.category == ScriptItemCategory::Math));
    }

    #[test]
    fn test_suggestions_filter() {
        let mut state = ScriptEditorState::new();
        state.update_suggestions("der");
        assert!(state.suggestions.iter().any(|i| i.name == "derivative"));

        state.update_suggestions("sin");
        assert!(state.suggestions.iter().any(|i| i.name == "sin"));
        assert!(state.suggestions.iter().any(|i| i.name == "sinh"));
    }

    #[test]
    fn test_suggestion_navigation() {
        let mut state = ScriptEditorState::new();
        state.update_suggestions("s");
        let count = state.suggestions.len();

        assert_eq!(state.selected_suggestion, 0);
        state.select_next();
        assert_eq!(state.selected_suggestion, 1);
        state.select_prev();
        assert_eq!(state.selected_suggestion, 0);
        state.select_prev();
        assert_eq!(state.selected_suggestion, count - 1);
    }
}
