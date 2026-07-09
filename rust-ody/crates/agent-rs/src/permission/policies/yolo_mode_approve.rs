use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

pub struct YoloModeApprove;

impl PermissionPolicy for YoloModeApprove {
    fn name(&self) -> &str {
        "yolo-mode-approve"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        // Checked in factory via PermissionManagerContext.mode().
        None
    }
}
