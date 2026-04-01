//! Pure variable management logic extracted from action dispatch.
//! All functions take data maps directly, enabling unit testing without PipelineBridge.

use std::collections::HashMap;

use crate::pipeline::bridge::PipelineCommand;
use crate::types::{Variable, VariableData};

/// Set a variable's poll rate and propagate constraints through the tree.
/// Returns UpdateVariable commands for all changed variables.
pub fn set_variable_poll_rate(
    variables: &mut HashMap<u32, Variable>,
    id: u32,
    rate_hz: u32,
) -> Vec<PipelineCommand> {
    if let Some(var) = variables.get_mut(&id) {
        var.poll_rate_hz = rate_hz;
    }
    if rate_hz > 0 {
        clamp_children_rate(variables, id, rate_hz);
    }
    raise_ancestor_rate(variables, id, rate_hz);
    // Return update commands for all variables
    variables
        .values()
        .map(|v| PipelineCommand::UpdateVariable(v.clone()))
        .collect()
}

/// Toggle enabled state for an entire variable tree recursively.
/// Returns UpdateVariable commands for each toggled variable.
pub fn toggle_tree_enabled(
    variables: &mut HashMap<u32, Variable>,
    root_id: u32,
    enabled: bool,
) -> Vec<PipelineCommand> {
    let mut commands = Vec::new();
    toggle_tree_enabled_inner(variables, root_id, enabled, &mut commands);
    commands
}

fn toggle_tree_enabled_inner(
    variables: &mut HashMap<u32, Variable>,
    id: u32,
    enabled: bool,
    commands: &mut Vec<PipelineCommand>,
) {
    if let Some(var) = variables.get_mut(&id) {
        var.enabled = enabled;
        commands.push(PipelineCommand::UpdateVariable(var.clone()));
    }
    let child_ids: Vec<u32> = variables
        .values()
        .filter(|v| v.parent_id == Some(id))
        .map(|v| v.id)
        .collect();
    for child_id in child_ids {
        toggle_tree_enabled_inner(variables, child_id, enabled, commands);
    }
}

/// Rename a variable and propagate prefix changes to children.
/// Also updates VariableData entries.
/// Returns UpdateVariable commands.
pub fn rename_variable(
    variables: &mut HashMap<u32, Variable>,
    variable_data: &mut HashMap<u32, VariableData>,
    id: u32,
    new_name: String,
) -> Vec<PipelineCommand> {
    let mut commands = Vec::new();
    let old_name = match variables.get(&id) {
        Some(v) => v.name.clone(),
        None => return commands,
    };

    // Rename the target variable
    if let Some(var) = variables.get_mut(&id) {
        var.name = new_name.clone();
        commands.push(PipelineCommand::UpdateVariable(var.clone()));
    }
    if let Some(data) = variable_data.get_mut(&id) {
        data.variable.name = new_name.clone();
    }

    // Propagate prefix change to children
    let child_ids: Vec<u32> = variables
        .values()
        .filter(|v| v.parent_id == Some(id))
        .map(|v| v.id)
        .collect();
    for child_id in child_ids {
        if let Some(child) = variables.get_mut(&child_id) {
            if let Some(suffix) = child.name.strip_prefix(&old_name) {
                child.name = format!("{}{}", new_name, suffix);
            }
        }
        if let Some(child_data) = variable_data.get_mut(&child_id) {
            if let Some(suffix) = child_data.variable.name.strip_prefix(&old_name) {
                child_data.variable.name = format!("{}{}", new_name, suffix);
            }
        }
    }

    commands
}

/// Clamp children's poll rates so they don't exceed the parent's rate.
pub fn clamp_children_rate(variables: &mut HashMap<u32, Variable>, parent_id: u32, max_rate: u32) {
    let child_ids: Vec<u32> = variables
        .values()
        .filter(|v| v.parent_id == Some(parent_id))
        .map(|v| v.id)
        .collect();
    for child_id in child_ids {
        if let Some(child) = variables.get_mut(&child_id) {
            if child.poll_rate_hz > max_rate && child.poll_rate_hz != 0 {
                child.poll_rate_hz = max_rate;
            }
        }
        clamp_children_rate(variables, child_id, max_rate);
    }
}

/// Raise ancestor poll rates so parents are at least as fast as their fastest child.
pub fn raise_ancestor_rate(variables: &mut HashMap<u32, Variable>, id: u32, min_rate: u32) {
    if min_rate == 0 {
        return; // 0 means "use global" — don't propagate
    }
    let parent_id = variables.get(&id).and_then(|v| v.parent_id);
    if let Some(pid) = parent_id {
        if let Some(parent) = variables.get_mut(&pid) {
            if parent.poll_rate_hz != 0 && parent.poll_rate_hz < min_rate {
                parent.poll_rate_hz = min_rate;
            }
        }
        raise_ancestor_rate(variables, pid, min_rate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::VariableType;

    fn make_var(id: u32, name: &str, parent: Option<u32>) -> Variable {
        let mut v = Variable::new(name, 0x1000 + id as u64 * 4, VariableType::U32);
        // Override the auto-generated ID for predictable testing
        v.id = id;
        v.parent_id = parent;
        v
    }

    fn make_var_map(vars: Vec<Variable>) -> HashMap<u32, Variable> {
        vars.into_iter().map(|v| (v.id, v)).collect()
    }

    #[test]
    fn test_clamp_children_rate() {
        let mut parent = make_var(1, "parent", None);
        parent.poll_rate_hz = 100;
        let mut child = make_var(2, "child", Some(1));
        child.poll_rate_hz = 200;
        let mut vars = make_var_map(vec![parent, child]);

        clamp_children_rate(&mut vars, 1, 100);
        assert_eq!(vars[&2].poll_rate_hz, 100);
    }

    #[test]
    fn test_clamp_children_deep() {
        let mut parent = make_var(1, "p", None);
        parent.poll_rate_hz = 50;
        let mut child = make_var(2, "c", Some(1));
        child.poll_rate_hz = 100;
        let mut grandchild = make_var(3, "gc", Some(2));
        grandchild.poll_rate_hz = 200;
        let mut vars = make_var_map(vec![parent, child, grandchild]);

        clamp_children_rate(&mut vars, 1, 50);
        assert_eq!(vars[&2].poll_rate_hz, 50);
        assert_eq!(vars[&3].poll_rate_hz, 50);
    }

    #[test]
    fn test_raise_ancestor_rate() {
        let mut parent = make_var(1, "parent", None);
        parent.poll_rate_hz = 50;
        let mut child = make_var(2, "child", Some(1));
        child.poll_rate_hz = 200;
        let mut vars = make_var_map(vec![parent, child]);

        raise_ancestor_rate(&mut vars, 2, 200);
        assert_eq!(vars[&1].poll_rate_hz, 200);
    }

    #[test]
    fn test_raise_ancestor_zero_exempt() {
        let mut parent = make_var(1, "parent", None);
        parent.poll_rate_hz = 50;
        let child = make_var(2, "child", Some(1));
        let mut vars = make_var_map(vec![parent, child]);

        raise_ancestor_rate(&mut vars, 2, 0);
        assert_eq!(vars[&1].poll_rate_hz, 50); // unchanged
    }

    #[test]
    fn test_toggle_tree_enabled_recursive() {
        let mut p = make_var(1, "p", None);
        p.enabled = true;
        let mut c1 = make_var(2, "c1", Some(1));
        c1.enabled = true;
        let mut c2 = make_var(3, "c2", Some(1));
        c2.enabled = true;
        let mut gc = make_var(4, "gc", Some(2));
        gc.enabled = true;
        let mut vars = make_var_map(vec![p, c1, c2, gc]);

        let cmds = toggle_tree_enabled(&mut vars, 1, false);
        assert!(!vars[&1].enabled);
        assert!(!vars[&2].enabled);
        assert!(!vars[&3].enabled);
        assert!(!vars[&4].enabled);
        assert_eq!(cmds.len(), 4); // one UpdateVariable per toggled var
    }

    #[test]
    fn test_toggle_tree_returns_commands() {
        let p = make_var(1, "p", None);
        let c = make_var(2, "c", Some(1));
        let mut vars = make_var_map(vec![p, c]);

        let cmds = toggle_tree_enabled(&mut vars, 1, true);
        assert_eq!(cmds.len(), 2);
    }

    #[test]
    fn test_rename_propagates_prefix() {
        let p = make_var(1, "foo", None);
        let mut c = make_var(2, "foo.x", Some(1));
        c.parent_id = Some(1);
        let mut vars = make_var_map(vec![p, c]);
        let mut data = HashMap::new();

        let _cmds = rename_variable(&mut vars, &mut data, 1, "bar".into());
        assert_eq!(vars[&1].name, "bar");
        assert_eq!(vars[&2].name, "bar.x");
    }

    #[test]
    fn test_rename_no_children() {
        let v = make_var(1, "solo", None);
        let mut vars = make_var_map(vec![v]);
        let mut data = HashMap::new();

        let cmds = rename_variable(&mut vars, &mut data, 1, "renamed".into());
        assert_eq!(vars[&1].name, "renamed");
        assert_eq!(cmds.len(), 1);
    }

    #[test]
    fn test_set_poll_rate_clamps_children() {
        let mut parent = make_var(1, "p", None);
        parent.poll_rate_hz = 200;
        let mut child = make_var(2, "c", Some(1));
        child.poll_rate_hz = 200;
        let mut vars = make_var_map(vec![parent, child]);

        set_variable_poll_rate(&mut vars, 1, 100);
        assert_eq!(vars[&1].poll_rate_hz, 100);
        assert_eq!(vars[&2].poll_rate_hz, 100);
    }

    #[test]
    fn test_set_poll_rate_raises_ancestor() {
        let mut parent = make_var(1, "p", None);
        parent.poll_rate_hz = 50;
        let mut child = make_var(2, "c", Some(1));
        child.poll_rate_hz = 100;
        let mut vars = make_var_map(vec![parent, child]);

        set_variable_poll_rate(&mut vars, 2, 200);
        assert_eq!(vars[&2].poll_rate_hz, 200);
        assert_eq!(vars[&1].poll_rate_hz, 200);
    }
}
