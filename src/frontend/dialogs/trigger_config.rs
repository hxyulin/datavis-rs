//! Trigger configuration dialog
//!
//! Dialog for configuring trigger settings including:
//! - Trigger variable selection
//! - Trigger condition (rising/falling edge, threshold)
//! - Pre/post trigger buffer durations

use egui::Ui;

use super::{Dialog, DialogAction, DialogState, DialogWindowConfig};
use crate::config::settings::{TriggerCondition, TriggerSettings};
use crate::types::Variable;

/// State for the trigger configuration dialog
#[derive(Debug, Clone)]
pub struct TriggerConfigState {
    /// Whether triggering is enabled
    pub enabled: bool,
    /// Selected variable ID
    pub variable_id: Option<u32>,
    /// Trigger condition
    pub condition: TriggerCondition,
    /// Threshold value (as string for editing)
    pub threshold_input: String,
    /// Pre-trigger duration in milliseconds
    pub pre_trigger_ms: u64,
    /// Post-trigger duration in milliseconds
    pub post_trigger_ms: u64,
}

impl Default for TriggerConfigState {
    fn default() -> Self {
        Self {
            enabled: false,
            variable_id: None,
            condition: TriggerCondition::RisingEdge,
            threshold_input: "0.0".to_string(),
            pre_trigger_ms: 100,
            post_trigger_ms: 1000,
        }
    }
}

impl DialogState for TriggerConfigState {
    fn is_valid(&self) -> bool {
        // Valid if disabled, or if enabled with a selected variable and valid threshold
        !self.enabled || (self.variable_id.is_some() && self.threshold_input.parse::<f64>().is_ok())
    }
}

impl TriggerConfigState {
    /// Create state from existing trigger settings
    pub fn from_settings(settings: &TriggerSettings) -> Self {
        Self {
            enabled: settings.enabled,
            variable_id: settings.variable_id,
            condition: settings.condition,
            threshold_input: format!("{}", settings.threshold),
            pre_trigger_ms: settings.pre_trigger.as_millis() as u64,
            post_trigger_ms: settings.post_trigger.as_millis() as u64,
        }
    }

    /// Convert state to trigger settings
    pub fn to_settings(&self) -> TriggerSettings {
        TriggerSettings {
            enabled: self.enabled,
            variable_id: self.variable_id,
            condition: self.condition,
            threshold: self.threshold_input.parse().unwrap_or(0.0),
            pre_trigger: std::time::Duration::from_millis(self.pre_trigger_ms),
            post_trigger: std::time::Duration::from_millis(self.post_trigger_ms),
            armed: false,
            triggered: false,
        }
    }
}

/// Actions that can be returned from the trigger config dialog
#[derive(Debug, Clone)]
pub enum TriggerConfigAction {
    /// Update trigger settings
    UpdateSettings(TriggerSettings),
    /// Arm the trigger
    Arm,
    /// Disarm the trigger
    Disarm,
    /// Reset the trigger
    Reset,
}

/// Context for rendering the trigger config dialog
pub struct TriggerConfigContext<'a> {
    /// Available variables to trigger on
    pub variables: &'a [Variable],
    /// Current trigger state (armed, triggered)
    pub is_armed: bool,
    pub is_triggered: bool,
}

/// Trigger configuration dialog
pub struct TriggerConfigDialog;

impl Dialog for TriggerConfigDialog {
    type State = TriggerConfigState;
    type Action = TriggerConfigAction;
    type Context<'a> = TriggerConfigContext<'a>;

    fn title(_state: &Self::State) -> &'static str {
        "Trigger Configuration"
    }

    fn window_config() -> DialogWindowConfig {
        DialogWindowConfig {
            default_width: 350.0,
            default_height: None,
            resizable: true,
            collapsible: false,
            anchor: None,
            modal: false,
        }
    }

    fn render(
        state: &mut Self::State,
        ctx: Self::Context<'_>,
        ui: &mut Ui,
    ) -> DialogAction<Self::Action> {
        let mut action = DialogAction::None;

        // Enable/Disable toggle
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.enabled, "Enable Trigger");
            if ctx.is_triggered {
                ui.colored_label(egui::Color32::from_rgb(100, 255, 100), " TRIGGERED");
            } else if ctx.is_armed {
                ui.colored_label(egui::Color32::from_rgb(255, 255, 100), " ARMED");
            }
        });

        ui.separator();

        // Variable selection
        ui.add_enabled_ui(state.enabled, |ui| {
            ui.horizontal(|ui| {
                ui.label("Trigger Variable:");
                egui::ComboBox::from_id_salt("trigger_variable")
                    .selected_text(
                        state
                            .variable_id
                            .and_then(|id| ctx.variables.iter().find(|v| v.id == id))
                            .map(|v| v.name.as_str())
                            .unwrap_or("Select..."),
                    )
                    .show_ui(ui, |ui| {
                        for var in ctx.variables {
                            if var.enabled {
                                ui.selectable_value(
                                    &mut state.variable_id,
                                    Some(var.id),
                                    &var.name,
                                );
                            }
                        }
                    });
            });

            ui.add_space(8.0);

            // Condition selection
            ui.horizontal(|ui| {
                ui.label("Condition:");
                egui::ComboBox::from_id_salt("trigger_condition")
                    .selected_text(condition_display_name(state.condition))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.condition,
                            TriggerCondition::RisingEdge,
                            "Rising Edge",
                        )
                        .on_hover_text("Trigger when value crosses threshold going up");
                        ui.selectable_value(
                            &mut state.condition,
                            TriggerCondition::FallingEdge,
                            "Falling Edge",
                        )
                        .on_hover_text("Trigger when value crosses threshold going down");
                        ui.selectable_value(
                            &mut state.condition,
                            TriggerCondition::Above,
                            "Above Threshold",
                        )
                        .on_hover_text("Trigger when value is above threshold");
                        ui.selectable_value(
                            &mut state.condition,
                            TriggerCondition::Below,
                            "Below Threshold",
                        )
                        .on_hover_text("Trigger when value is below threshold");
                        ui.selectable_value(
                            &mut state.condition,
                            TriggerCondition::Equal,
                            "Equal to Threshold",
                        )
                        .on_hover_text("Trigger when value equals threshold");
                        ui.selectable_value(
                            &mut state.condition,
                            TriggerCondition::Change,
                            "Value Change",
                        )
                        .on_hover_text("Trigger when value changes by more than threshold");
                    });
            });

            ui.add_space(8.0);

            // Threshold input
            ui.horizontal(|ui| {
                ui.label("Threshold:");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.threshold_input)
                        .desired_width(100.0)
                        .hint_text("0.0"),
                );
                if state.threshold_input.parse::<f64>().is_err() && !state.threshold_input.is_empty()
                {
                    ui.colored_label(egui::Color32::from_rgb(255, 100, 100), "Invalid");
                }
                // Show help text based on condition
                let help = match state.condition {
                    TriggerCondition::RisingEdge | TriggerCondition::FallingEdge => {
                        "Value to cross"
                    }
                    TriggerCondition::Above | TriggerCondition::Below => "Threshold value",
                    TriggerCondition::Equal => "Target value",
                    TriggerCondition::Change => "Minimum change amount",
                };
                response.on_hover_text(help);
            });

            ui.add_space(8.0);

            // Pre/Post trigger durations
            ui.collapsing("Buffer Settings", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Pre-trigger buffer:");
                    ui.add(egui::DragValue::new(&mut state.pre_trigger_ms).suffix(" ms"));
                });
                ui.horizontal(|ui| {
                    ui.label("Post-trigger capture:");
                    ui.add(egui::DragValue::new(&mut state.post_trigger_ms).suffix(" ms"));
                });
                ui.label(
                    egui::RichText::new("Note: Buffer settings affect how much data is kept around the trigger point.")
                        .small()
                        .weak(),
                );
            });
        });

        ui.add_space(16.0);
        ui.separator();

        // Action buttons
        ui.horizontal(|ui| {
            // Arm/Disarm button
            if state.enabled && state.is_valid() {
                if ctx.is_armed {
                    if ui.button("Disarm").clicked() {
                        action = DialogAction::Action(TriggerConfigAction::Disarm);
                    }
                } else if ui.button("Arm").clicked() {
                    // First update settings, then arm
                    action = DialogAction::Action(TriggerConfigAction::UpdateSettings(
                        state.to_settings(),
                    ));
                }

                if ctx.is_triggered {
                    if ui.button("Reset").clicked() {
                        action = DialogAction::Action(TriggerConfigAction::Reset);
                    }
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    action = DialogAction::Close;
                }
                if ui.button("Apply").clicked() {
                    action =
                        DialogAction::CloseWithAction(TriggerConfigAction::UpdateSettings(
                            state.to_settings(),
                        ));
                }
            });
        });

        action
    }
}

/// Get display name for trigger condition
fn condition_display_name(condition: TriggerCondition) -> &'static str {
    match condition {
        TriggerCondition::RisingEdge => "Rising Edge",
        TriggerCondition::FallingEdge => "Falling Edge",
        TriggerCondition::Above => "Above Threshold",
        TriggerCondition::Below => "Below Threshold",
        TriggerCondition::Equal => "Equal to Threshold",
        TriggerCondition::Change => "Value Change",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_trigger_config_state_default() {
        let state = TriggerConfigState::default();
        assert!(!state.enabled);
        assert!(state.variable_id.is_none());
        assert_eq!(state.condition, TriggerCondition::RisingEdge);
    }

    #[test]
    fn test_trigger_config_state_validation() {
        let mut state = TriggerConfigState::default();
        // Disabled is always valid
        assert!(state.is_valid());

        // Enabled without variable is invalid
        state.enabled = true;
        assert!(!state.is_valid());

        // Enabled with variable but invalid threshold is invalid
        state.variable_id = Some(1);
        state.threshold_input = "not a number".to_string();
        assert!(!state.is_valid());

        // Enabled with variable and valid threshold is valid
        state.threshold_input = "1.5".to_string();
        assert!(state.is_valid());
    }

    #[test]
    fn test_trigger_config_state_round_trip() {
        let settings = TriggerSettings {
            enabled: true,
            variable_id: Some(42),
            condition: TriggerCondition::FallingEdge,
            threshold: 2.5,
            pre_trigger: Duration::from_millis(200),
            post_trigger: Duration::from_millis(500),
            armed: true,
            triggered: false,
        };

        let state = TriggerConfigState::from_settings(&settings);
        let result = state.to_settings();

        assert_eq!(result.enabled, settings.enabled);
        assert_eq!(result.variable_id, settings.variable_id);
        assert_eq!(result.condition, settings.condition);
        assert!((result.threshold - settings.threshold).abs() < 0.001);
        assert_eq!(result.pre_trigger, settings.pre_trigger);
        assert_eq!(result.post_trigger, settings.post_trigger);
        // armed and triggered should be reset
        assert!(!result.armed);
        assert!(!result.triggered);
    }
}
