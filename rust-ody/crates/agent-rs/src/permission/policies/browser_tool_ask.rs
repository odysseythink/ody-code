use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

pub struct BrowserToolAsk;

impl PermissionPolicy for BrowserToolAsk {
    fn name(&self) -> &str {
        "browser-tool-ask"
    }

    fn evaluate(
        &self,
        context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        if context.tool_call.name.starts_with("mcp__chrome-devtools__") {
            let mut reason = std::collections::HashMap::new();
            reason.insert(
                "tool".to_string(),
                serde_json::json!(context.tool_call.name),
            );
            Some(PermissionPolicyResolution::Ask {
                reason: Some(reason),
                resolve_approval: None,
                resolve_error: None,
            })
        } else {
            None
        }
    }
}
