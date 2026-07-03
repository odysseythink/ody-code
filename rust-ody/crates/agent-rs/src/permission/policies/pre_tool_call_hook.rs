use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

pub struct PreToolCallHook;

impl PermissionPolicy for PreToolCallHook {
    fn name(&self) -> &str {
        "pre-tool-call-hook"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        None
    }
}
