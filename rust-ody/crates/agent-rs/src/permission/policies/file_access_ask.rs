use std::collections::HashMap;

use crate::agent_loop::tool_access::ToolResourceAccess;
use crate::agent_loop::types::RunnableToolExecution;
use crate::permission::types::{
    PermissionPolicy, PermissionPolicyContext, PermissionPolicyResolution,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract ToolResourceAccess::File entries from the execution's accesses.
fn file_accesses(execution: &RunnableToolExecution) -> Vec<&ToolResourceAccess> {
    execution
        .accesses
        .as_ref()
        .map(|a| {
            a.0.iter()
                .filter(|r| matches!(r, ToolResourceAccess::File { .. }))
                .collect()
        })
        .unwrap_or_default()
}

/// Filter to write / readwrite file accesses only.
pub fn write_file_accesses(execution: &RunnableToolExecution) -> Vec<&ToolResourceAccess> {
    file_accesses(execution)
        .into_iter()
        .filter(|r| {
            if let ToolResourceAccess::File { operation, .. } = r {
                operation == "write" || operation == "readwrite"
            } else {
                false
            }
        })
        .collect()
}

fn file_access_reason(
    access: &ToolResourceAccess,
    extra: HashMap<&str, bool>,
) -> HashMap<String, serde_json::Value> {
    let (operation, recursive) = match access {
        ToolResourceAccess::File {
            operation,
            recursive,
            ..
        } => (operation.clone(), *recursive),
        _ => ("read".to_string(), None),
    };
    let mut reason = HashMap::new();
    reason.insert(
        "file_access_operation".to_string(),
        serde_json::json!(operation),
    );
    reason.insert(
        "recursive".to_string(),
        serde_json::json!(recursive == Some(true)),
    );
    for (k, v) in extra {
        reason.insert(k.to_string(), serde_json::json!(v));
    }
    reason
}

// ---------------------------------------------------------------------------
// SensitiveFileAccessAsk
// ---------------------------------------------------------------------------
pub struct SensitiveFileAccessAsk;

impl PermissionPolicy for SensitiveFileAccessAsk {
    fn name(&self) -> &str {
        "sensitive-file-access-ask"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        None
    }
}

pub fn evaluate_sensitive_file_access_ask(
    context: &PermissionPolicyContext<'_>,
    is_sensitive: impl Fn(&str) -> bool,
) -> Option<PermissionPolicyResolution> {
    for access in file_accesses(context.execution) {
        if let ToolResourceAccess::File { path, .. } = access {
            if is_sensitive(path) {
                let mut extra = HashMap::new();
                extra.insert("sensitive_path", true);
                return Some(PermissionPolicyResolution::Ask {
                    reason: Some(file_access_reason(access, extra)),
                    resolve_approval: None,
                    resolve_error: None,
                });
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// GitControlPathAccessAsk
// ---------------------------------------------------------------------------
pub struct GitControlPathAccessAsk;

impl PermissionPolicy for GitControlPathAccessAsk {
    fn name(&self) -> &str {
        "git-control-path-access-ask"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        None
    }
}

pub fn evaluate_git_control_path_access_ask(
    context: &PermissionPolicyContext<'_>,
    cwd: &str,
    git_work_tree_marker: Option<(&str, &str)>, // (dotGitPath, controlDirPath)
) -> Option<PermissionPolicyResolution> {
    if cwd.is_empty() {
        return None;
    }
    let accesses = file_accesses(context.execution);
    if accesses.is_empty() {
        return None;
    }

    // Check direct .git path component
    for access in &accesses {
        if let ToolResourceAccess::File { path, .. } = access {
            if has_git_path_component(path, cwd) {
                let mut extra = HashMap::new();
                extra.insert("git_control_path", true);
                return Some(PermissionPolicyResolution::Ask {
                    reason: Some(file_access_reason(access, extra)),
                    resolve_approval: None,
                    resolve_error: None,
                });
            }
        }
    }

    // Check work tree marker paths
    if let Some((dot_git_path, control_dir_path)) = git_work_tree_marker {
        for access in &accesses {
            if let ToolResourceAccess::File { path, .. } = access {
                if is_within_directory(path, dot_git_path)
                    || is_within_directory(path, control_dir_path)
                {
                    let mut extra = HashMap::new();
                    extra.insert("git_control_path", true);
                    return Some(PermissionPolicyResolution::Ask {
                        reason: Some(file_access_reason(access, extra)),
                        resolve_approval: None,
                        resolve_error: None,
                    });
                }
            }
        }
    }

    None
}

fn has_git_path_component(target_path: &str, cwd: &str) -> bool {
    let rel = relative_path(cwd, target_path);
    rel.split(&['/', '\\'][..])
        .any(|p| p.eq_ignore_ascii_case(".git"))
}

// ---------------------------------------------------------------------------
// CwdOutsideFileWriteAsk
// ---------------------------------------------------------------------------
pub struct CwdOutsideFileWriteAsk;

impl PermissionPolicy for CwdOutsideFileWriteAsk {
    fn name(&self) -> &str {
        "cwd-outside-file-write-ask"
    }

    fn evaluate(
        &self,
        _context: &PermissionPolicyContext<'_>,
    ) -> Option<PermissionPolicyResolution> {
        None
    }
}

pub fn evaluate_cwd_outside_file_write_ask(
    context: &PermissionPolicyContext<'_>,
    cwd: &str,
) -> Option<PermissionPolicyResolution> {
    if cwd.is_empty() {
        return None;
    }
    for access in write_file_accesses(context.execution) {
        if let ToolResourceAccess::File { path, .. } = access {
            if !is_within_directory(path, cwd) {
                let mut extra = HashMap::new();
                extra.insert("cwd_outside", true);
                return Some(PermissionPolicyResolution::Ask {
                    reason: Some(file_access_reason(access, extra)),
                    resolve_approval: None,
                    resolve_error: None,
                });
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Path utilities (reuse from kaos-rs in 4.1; inline for now)
// ---------------------------------------------------------------------------

fn normalize_path(p: &str) -> String {
    p.replace('\\', "/").replace("//", "/")
}

fn is_within_directory(target: &str, dir: &str) -> bool {
    let t = normalize_path(target).to_lowercase();
    let d = normalize_path(dir).to_lowercase();
    let d = if d.ends_with('/') {
        d
    } else {
        format!("{}/", d)
    };
    t.starts_with(&d) || t == d.trim_end_matches('/')
}

fn relative_path(from: &str, to: &str) -> String {
    let f = normalize_path(from).to_lowercase();
    let t = normalize_path(to).to_lowercase();
    if t.starts_with(&f) {
        let rest = if f.ends_with('/') {
            &t[f.len()..]
        } else {
            &t[f.len() + 1..]
        };
        rest.to_string()
    } else {
        t
    }
}
