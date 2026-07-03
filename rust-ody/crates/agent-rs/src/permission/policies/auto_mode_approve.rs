use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

pub struct AutoModeApprove;

impl PermissionPolicy for AutoModeApprove {
    fn name(&self) -> &str {
        "auto-mode-approve"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        None // Factory gates on mode
    }
}
