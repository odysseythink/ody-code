use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

pub struct AutoModeAskUserQuestionDeny;

impl PermissionPolicy for AutoModeAskUserQuestionDeny {
    fn name(&self) -> &str {
        "auto-mode-ask-user-question-deny"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        None // Factory checks mode + tool name
    }
}
