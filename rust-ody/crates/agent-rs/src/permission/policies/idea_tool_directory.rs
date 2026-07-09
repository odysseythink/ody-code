use crate::agent_loop::tool_access::ToolResourceAccess;
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

pub struct IdeaToolDirectory;

impl PermissionPolicy for IdeaToolDirectory {
    fn name(&self) -> &str {
        "idea-tool-directory-approve"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        None // Factory injects cwd
    }
}

pub fn evaluate_idea_tool_directory_approve(
    context: &PermissionPolicyContext<'_>,
    cwd: &str,
) -> Option<PermissionPolicyResolution> {
    if cwd.is_empty() {
        return None;
    }
    let ideas_dir = normalize_join(cwd, ".ody-code/ideas");
    let prefix = if ideas_dir.ends_with('/') {
        ideas_dir.clone()
    } else {
        format!("{}/", ideas_dir)
    };

    let mut found_write = false;
    if let Some(accesses) = &context.execution.accesses {
        for access in &accesses.0 {
            if let ToolResourceAccess::File {
                operation, path, ..
            } = access
            {
                if operation != "write" && operation != "readwrite" {
                    continue;
                }
                let np = normalize_path(path);
                if !np.starts_with(&prefix) {
                    return None;
                }
                found_write = true;
            }
        }
    }
    if found_write {
        Some(PermissionPolicyResolution::Approve {
            reason: None,
            execution_metadata: None,
        })
    } else {
        None
    }
}

fn normalize_path(p: &str) -> String {
    p.replace('\\', "/").replace("//", "/")
}

fn normalize_join(a: &str, b: &str) -> String {
    if a.ends_with('/') {
        format!("{}{}", a, b)
    } else {
        format!("{}/{}", a, b)
    }
}
