use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};
use crate::records::nested::ApprovalResponse;

pub struct ExitPlanModeReviewAsk;

impl PermissionPolicy for ExitPlanModeReviewAsk {
    fn name(&self) -> &str {
        "exit-plan-mode-review-ask"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        None // Factory checks mode, session-mode state, and display
    }
}

pub fn evaluate_exit_plan_mode_review_ask(
    _context: &PermissionPolicyContext<'_>,
    _is_design: bool,
) -> Option<PermissionPolicyResolution> {
    let mut reason = std::collections::HashMap::new();
    reason.insert("has_options".to_string(), serde_json::json!(false));
    Some(PermissionPolicyResolution::Ask {
        reason: Some(reason),
        resolve_approval: Some(
            |_result: &ApprovalResponse| -> Option<Box<PermissionPolicyResolution>> {
                // Full resolution logic (telemetry, selectedLabel handling, exit/reject/cancel)
                // delegated to the factory which has access to PermissionManagerContext.
                None
            },
        ),
        resolve_error: None,
    })
}
