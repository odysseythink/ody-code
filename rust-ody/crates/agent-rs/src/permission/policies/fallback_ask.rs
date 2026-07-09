use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

pub struct FallbackAsk;

impl PermissionPolicy for FallbackAsk {
    fn name(&self) -> &str {
        "fallback-ask"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        Some(PermissionPolicyResolution::Ask {
            reason: None,
            resolve_approval: None,
            resolve_error: None,
        })
    }
}
