use crate::agent_loop::tool_access::ToolResourceAccess;
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

pub struct PlanModeToolApprove;

impl PermissionPolicy for PlanModeToolApprove {
    fn name(&self) -> &str {
        "plan-mode-tool-approve"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        None
    }
}

pub fn evaluate_plan_mode_tool_approve(
    context: &PermissionPolicyContext<'_>,
    session_mode_active: bool,
    session_mode_file_path: Option<&str>,
) -> Option<PermissionPolicyResolution> {
    let tool_name = &context.tool_call.name;

    if tool_name == "EnterPlanMode"
        || tool_name == "EnterDesignMode"
        || tool_name == "EnterOfficeHoursMode"
        || tool_name == "EnterGameDesignMode"
    {
        return Some(PermissionPolicyResolution::Approve {
            reason: None,
            execution_metadata: None,
        });
    }

    if (tool_name == "Write" || tool_name == "Edit") && session_mode_active {
        if let Some(plan_path) = session_mode_file_path {
            if writes_only_plan_file(context, plan_path) {
                return Some(PermissionPolicyResolution::Approve {
                    reason: None,
                    execution_metadata: None,
                });
            }
        }
    }

    if tool_name == "ExitPlanMode"
        || tool_name == "ExitDesignMode"
        || tool_name == "ExitOfficeHoursMode"
        || tool_name == "ExitGameDesignMode"
    {
        if !session_mode_active {
            return Some(PermissionPolicyResolution::Approve {
                reason: None,
                execution_metadata: None,
            });
        }
        return Some(PermissionPolicyResolution::Approve {
            reason: None,
            execution_metadata: None,
        });
    }

    None
}

fn writes_only_plan_file(context: &PermissionPolicyContext<'_>, plan_path: &str) -> bool {
    context
        .execution
        .accesses
        .as_ref()
        .map(|a| {
            a.0.iter().all(|r| {
                if let ToolResourceAccess::File {
                    operation, path, ..
                } = r
                {
                    (operation == "write" || operation == "readwrite") && path == plan_path
                } else {
                    true
                }
            })
        })
        .unwrap_or(false)
}
