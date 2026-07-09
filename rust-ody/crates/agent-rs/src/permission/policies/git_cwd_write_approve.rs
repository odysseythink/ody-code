use crate::agent_loop::tool_access::ToolResourceAccess;
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

pub struct GitCwdWriteApprove;

impl PermissionPolicy for GitCwdWriteApprove {
    fn name(&self) -> &str {
        "git-cwd-write-approve"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        None // Factory injects cwd + git work tree marker + path_class
    }
}

pub fn evaluate_git_cwd_write_approve(
    context: &PermissionPolicyContext<'_>,
    cwd: &str,
    path_class: &str,
    git_work_tree_marker_exists: bool,
) -> Option<PermissionPolicyResolution> {
    let tool_name = &context.tool_call.name;
    if tool_name != "Write" && tool_name != "Edit" {
        return None;
    }
    if path_class != "posix" {
        return None;
    }
    if cwd.is_empty() {
        return None;
    }

    let all_within_cwd = context
        .execution
        .accesses
        .as_ref()
        .map(|a| {
            a.0.iter().all(|r| {
                if let ToolResourceAccess::File {
                    operation, path, ..
                } = r
                {
                    if operation != "write" && operation != "readwrite" {
                        return true;
                    }
                    is_within_directory_cwd(path, cwd)
                } else {
                    true
                }
            })
        })
        .unwrap_or(false);

    if !all_within_cwd {
        return None;
    }
    if !git_work_tree_marker_exists {
        return None;
    }

    Some(PermissionPolicyResolution::Approve {
        reason: None,
        execution_metadata: None,
    })
}

fn is_within_directory_cwd(target: &str, cwd: &str) -> bool {
    let t = target.replace('\\', "/").replace("//", "/").to_lowercase();
    let d = cwd.replace('\\', "/").replace("//", "/").to_lowercase();
    let d = if d.ends_with('/') {
        d
    } else {
        format!("{}/", d)
    };
    t.starts_with(&d) || t == d.trim_end_matches('/')
}
