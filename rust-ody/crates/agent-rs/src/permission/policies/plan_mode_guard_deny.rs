use crate::agent_loop::tool_access::ToolResourceAccess;
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

pub struct PlanModeGuardDeny;

impl PermissionPolicy for PlanModeGuardDeny {
    fn name(&self) -> &str {
        "plan-mode-guard-deny"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        None // Factory injects session-mode state
    }
}

/// Evaluate plan-mode guard. Returns deny for Write/Edit outside plan fileset,
/// TaskStop, CronCreate, CronDelete.
pub fn evaluate_plan_mode_guard_deny(
    context: &PermissionPolicyContext<'_>,
    mode_label: &str,
    exit_tool: &str,
    session_mode_file_path: Option<&str>,
    is_writable: impl Fn(&str) -> bool,
) -> Option<PermissionPolicyResolution> {
    let tool_name = &context.tool_call.name;

    if tool_name == "Write" || tool_name == "Edit" {
        if let Some(_plan_path) = session_mode_file_path {
            let write_accesses: Vec<&ToolResourceAccess> = context
                .execution
                .accesses
                .as_ref()
                .map(|a| {
                    a.0.iter()
                        .filter(|r| {
                            if let ToolResourceAccess::File { operation, .. } = r {
                                operation == "write" || operation == "readwrite"
                            } else {
                                false
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let all_in_plan_fileset = write_accesses.iter().all(|r| {
                if let ToolResourceAccess::File { path, .. } = r {
                    is_writable(path)
                } else {
                    false
                }
            });
            if all_in_plan_fileset {
                return None; // All targets are writable plan paths
            }
        }
        return Some(PermissionPolicyResolution::Deny {
            reason: None,
            message: Some(mode_write_denied_message(
                mode_label,
                session_mode_file_path,
                exit_tool,
            )),
        });
    }

    if tool_name == "TaskStop" {
        return Some(PermissionPolicyResolution::Deny {
            reason: None,
            message: Some(format!(
                "TaskStop is not available in {} mode. Call {} to exit {} mode before stopping a background task.",
                mode_label, exit_tool, mode_label
            )),
        });
    }

    if tool_name == "CronCreate" || tool_name == "CronDelete" {
        return Some(PermissionPolicyResolution::Deny {
            reason: None,
            message: Some(format!(
                "{} is not available in {} mode because it would mutate scheduled work that runs after {} exit. Call {} first.",
                tool_name, mode_label, mode_label, exit_tool
            )),
        });
    }

    None
}

fn mode_write_denied_message(
    mode_label: &str,
    session_mode_file_path: Option<&str>,
    exit_tool: &str,
) -> String {
    let mode_proper = capitalized(mode_label);
    match session_mode_file_path {
        None => format!(
            "{} mode is active, but no {} file has been selected yet. Wait for the host to assign one before writing, or call {} to exit {} mode.",
            mode_proper, mode_label, exit_tool, mode_label
        ),
        Some(path) => {
            let stem = path.split('/').last().unwrap_or(path).replace(".md", "");
            format!(
                "{} mode is active. You may only write to the assigned {} file ({}) or .md files inside its \"{}/\" subdirectory (where split parts go) — write split parts there, do NOT merge them into the index and do NOT invent another path. Call {} to exit {} mode before editing other files.",
                mode_proper, mode_label, path, stem, exit_tool, mode_label
            )
        }
    }
}

fn capitalized(s: &str) -> String {
    if s == "game-design" {
        return "Game-design".to_string();
    }
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}
