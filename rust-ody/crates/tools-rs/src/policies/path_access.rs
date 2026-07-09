use serde::{Deserialize, Serialize};

use crate::policies::sensitive::is_sensitive_file;
use crate::workspace::WorkspaceConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathClass {
    Posix,
    Win32,
}

impl PathClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            PathClass::Posix => "posix",
            PathClass::Win32 => "win32",
        }
    }
}

impl Default for PathClass {
    fn default() -> Self {
        if cfg!(windows) {
            PathClass::Win32
        } else {
            PathClass::Posix
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PathSecurityCode {
    PathOutsideWorkspace,
    PathSensitive,
    PathInvalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathAccessOperation {
    Read,
    Write,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceGuardMode {
    AbsoluteOutsideAllowed,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceAccessPolicy {
    pub guard_mode: WorkspaceGuardMode,
    pub check_sensitive: bool,
}

pub const DEFAULT_WORKSPACE_ACCESS_POLICY: WorkspaceAccessPolicy = WorkspaceAccessPolicy {
    guard_mode: WorkspaceGuardMode::AbsoluteOutsideAllowed,
    check_sensitive: true,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathAccess {
    pub path: String,
    pub outside_workspace: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct PathSecurityError {
    pub code: PathSecurityCode,
    pub raw_path: String,
    pub canonical_path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssertPathOptions {
    pub mode: PathAccessOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_sensitive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_class: Option<PathClass>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvePathAccessOptions {
    pub operation: PathAccessOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<WorkspaceAccessPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_class: Option<PathClass>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvePathAccessPathOptions {
    pub workspace: WorkspaceConfig,
    pub operation: PathAccessOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<WorkspaceAccessPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_class: Option<PathClass>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_dir: Option<String>,
    #[serde(default = "default_expand_home")]
    pub expand_home: bool,
}

fn default_expand_home() -> bool {
    true
}

pub fn normalize_user_path(path: &str, path_class: PathClass) -> String {
    if path_class != PathClass::Win32 || path == "/" {
        return path.to_string();
    }

    if path.starts_with("//") {
        return path.to_string();
    }

    if let Some(m) = regex::Regex::new(r"^/cygdrive/([A-Za-z])(?:/|$)")
        .unwrap()
        .captures(path)
    {
        let drive = m[1].to_uppercase();
        let rest = &path[m[0].len() - 1..];
        return if rest == "/" {
            format!("{}:/", drive)
        } else {
            format!("{}:{}", drive, rest)
        };
    }

    if let Some(m) = regex::Regex::new(r"^/([A-Za-z])(?:/|$)")
        .unwrap()
        .captures(path)
    {
        let drive = m[1].to_uppercase();
        let rest = &path[m[0].len() - 1..];
        if rest.is_empty() || rest == "/" {
            return format!("{}:/", drive);
        }
        // Path like "/c" — rest is the drive letter itself, no trailing path
        if rest.len() == 1 && rest.chars().next().unwrap().is_ascii_alphabetic() {
            return format!("{}:/", drive);
        }
        return format!("{}:{}", drive, rest);
    }

    path.to_string()
}

fn expand_user_path(path: &str, home_dir: Option<&str>, path_class: PathClass) -> String {
    let home = match home_dir {
        Some(h) => h,
        None => return path.to_string(),
    };
    if path == "~" {
        return home.to_string();
    }
    if path.starts_with("~/") || (path_class == PathClass::Win32 && path.starts_with("~\\")) {
        let rest = &path[2..];
        return format!("{}/{}", home.replace('\\', "/"), rest);
    }
    path.to_string()
}

fn is_win32_drive_relative(path: &str) -> bool {
    regex::Regex::new(r"^[A-Za-z]:(?:$|[^/\\])")
        .unwrap()
        .is_match(path)
}

fn is_absolute(path: &str, path_class: PathClass) -> bool {
    match path_class {
        PathClass::Posix => path.starts_with('/'),
        PathClass::Win32 => {
            path.starts_with("//")
                || path.starts_with("\\\\")
                || regex::Regex::new(r"^[A-Za-z]:[/\\]")
                    .unwrap()
                    .is_match(path)
        }
    }
}

fn join_paths(cwd: &str, rel: &str, path_class: PathClass) -> String {
    let cwd_slashes = if path_class == PathClass::Win32 {
        cwd.replace('\\', "/")
    } else {
        cwd.to_string()
    };
    let rel_slashes = if path_class == PathClass::Win32 {
        rel.replace('\\', "/")
    } else {
        rel.to_string()
    };
    let base = cwd_slashes.trim_end_matches('/');
    if rel_slashes.starts_with('/') {
        format!("{}{}", base, rel_slashes)
    } else {
        format!("{}/{}", base, rel_slashes)
    }
}

pub fn normalize_path(path: &str, path_class: PathClass) -> String {
    let s = if path_class == PathClass::Win32 {
        path.replace('\\', "/")
    } else {
        path.to_string()
    };

    let parts: Vec<&str> = s.split('/').collect();
    let mut i = 0usize;
    let mut root = String::new();
    let mut is_abs = false;

    if path_class == PathClass::Win32
        && !parts.is_empty()
        && parts[0].len() == 2
        && parts[0].as_bytes()[1] == b':'
        && parts[0].as_bytes()[0].is_ascii_alphabetic()
    {
        if parts.len() > 1 {
            root.push_str(&parts[0][..1].to_uppercase());
            root.push_str(":/");
            is_abs = true;
            if parts[1].is_empty() {
                // "C://..." or "C:/" — skip the empty segment after separator
                i = 2;
            } else {
                // "C:/workspace/..." — parts[1] is the first path component
                i = 1;
            }
        }
    } else if s.starts_with("//") && !s.starts_with("///") {
        root.push_str("//");
        if parts.len() > 2 {
            root.push_str(parts[2]);
        }
        if parts.len() > 3 && !parts[3].is_empty() {
            root.push('/');
            root.push_str(parts[3]);
        }
        i = if parts.len() > 3 { 4 } else { 3 };
        is_abs = true;
    } else if s.starts_with('/') {
        root.push('/');
        i = 1;
        is_abs = true;
    }

    let mut stack: Vec<&str> = Vec::new();
    for part in &parts[i..] {
        if part.is_empty() || *part == "." {
            continue;
        }
        if *part == ".." {
            if let Some(last) = stack.last() {
                if *last != ".." {
                    stack.pop();
                    continue;
                }
            } else if is_abs {
                continue;
            }
        }
        stack.push(part);
    }

    let body = stack.join("/");
    if root.is_empty() && body.is_empty() {
        return ".".to_string();
    }
    if body.is_empty() {
        return root;
    }
    if root.ends_with('/') {
        format!("{}{}", root, body)
    } else {
        format!("{}/{}", root, body)
    }
}

pub fn canonicalize_path(
    path: &str,
    cwd: &str,
    path_class: PathClass,
) -> Result<String, PathSecurityError> {
    if path.is_empty() {
        return Err(PathSecurityError {
            code: PathSecurityCode::PathInvalid,
            raw_path: path.to_string(),
            canonical_path: path.to_string(),
            message: "Path cannot be empty".to_string(),
        });
    }
    let normalized_path = normalize_user_path(path, path_class);
    if path_class == PathClass::Win32 && is_win32_drive_relative(&normalized_path) {
        return Err(PathSecurityError {
            code: PathSecurityCode::PathInvalid,
            raw_path: path.to_string(),
            canonical_path: normalized_path,
            message: format!(
                "\"{}\" is a drive-relative Windows path. Use an absolute path like C:\\\\path or a path relative to the working directory.",
                path
            ),
        });
    }
    if !is_absolute(&normalized_path, path_class) && !is_absolute(cwd, path_class) {
        return Err(PathSecurityError {
            code: PathSecurityCode::PathInvalid,
            raw_path: path.to_string(),
            canonical_path: normalized_path,
            message: format!(
                "Cannot resolve \"{}\" against non-absolute cwd \"{}\".",
                path, cwd
            ),
        });
    }
    let abs = if is_absolute(&normalized_path, path_class) {
        normalized_path
    } else {
        join_paths(cwd, &normalized_path, path_class)
    };
    Ok(normalize_path(&abs, path_class))
}

pub fn is_within_directory(candidate: &str, base: &str, path_class: PathClass) -> bool {
    let nc = normalize_path(candidate, path_class);
    let nb = normalize_path(base, path_class);
    let comparable_candidate = if path_class == PathClass::Win32 {
        nc.to_lowercase()
    } else {
        nc
    };
    let comparable_base = if path_class == PathClass::Win32 {
        nb.to_lowercase()
    } else {
        nb
    };
    if comparable_candidate == comparable_base {
        return true;
    }
    let prefix = if comparable_base.ends_with('/') {
        comparable_base
    } else {
        format!("{}/", comparable_base)
    };
    comparable_candidate.starts_with(&prefix)
}

pub fn is_within_workspace(
    candidate: &str,
    config: &WorkspaceConfig,
    path_class: PathClass,
) -> bool {
    if is_within_directory(candidate, &config.workspace_dir, path_class) {
        return true;
    }
    for dir in &config.additional_dirs {
        if is_within_directory(candidate, dir, path_class) {
            return true;
        }
    }
    false
}

fn relative_outside_message(path: &str, operation: PathAccessOperation) -> String {
    let verb = match operation {
        PathAccessOperation::Write => "write or edit a file",
        PathAccessOperation::Search => "search",
        PathAccessOperation::Read => "read a file",
    };
    format!(
        "\"{}\" is not an absolute path. You must provide an absolute path to {} outside the working directory.",
        path, verb
    )
}

pub fn resolve_path_access(
    path: &str,
    cwd: &str,
    config: &WorkspaceConfig,
    options: ResolvePathAccessOptions,
) -> Result<PathAccess, PathSecurityError> {
    let path_class = options.path_class.unwrap_or_default();
    let normalized_path = normalize_user_path(path, path_class);
    let expanded_path = expand_user_path(&normalized_path, options.home_dir.as_deref(), path_class);
    let raw_is_absolute = is_absolute(&expanded_path, path_class);
    let canonical = canonicalize_path(&expanded_path, cwd, path_class)?;
    let outside_workspace = !is_within_workspace(&canonical, config, path_class);
    let policy = options.policy.unwrap_or(DEFAULT_WORKSPACE_ACCESS_POLICY);

    if policy.check_sensitive && is_sensitive_file(&canonical) {
        return Err(PathSecurityError {
            code: PathSecurityCode::PathSensitive,
            raw_path: path.to_string(),
            canonical_path: canonical.clone(),
            message: format!(
                "\"{}\" matches a sensitive-file pattern (env / credential / SSH key). Access is blocked to protect secrets.",
                path
            ),
        });
    }

    if outside_workspace {
        match policy.guard_mode {
            WorkspaceGuardMode::AbsoluteOutsideAllowed => {
                if !raw_is_absolute {
                    return Err(PathSecurityError {
                        code: PathSecurityCode::PathOutsideWorkspace,
                        raw_path: path.to_string(),
                        canonical_path: canonical,
                        message: relative_outside_message(path, options.operation),
                    });
                }
            }
            WorkspaceGuardMode::Disabled => {}
        }
    }

    Ok(PathAccess {
        path: canonical,
        outside_workspace,
    })
}

pub fn resolve_path_access_path(
    path: &str,
    options: ResolvePathAccessPathOptions,
) -> Result<String, PathSecurityError> {
    let ResolvePathAccessPathOptions {
        workspace,
        operation,
        policy,
        path_class,
        home_dir,
        expand_home,
    } = options;
    let home = if expand_home { home_dir } else { None };
    resolve_path_access(
        path,
        &workspace.workspace_dir,
        &workspace,
        ResolvePathAccessOptions {
            operation,
            policy,
            path_class,
            home_dir: home,
        },
    )
    .map(|a| a.path)
}

pub fn assert_path_allowed(
    path: &str,
    cwd: &str,
    config: &WorkspaceConfig,
    options: AssertPathOptions,
) -> Result<String, PathSecurityError> {
    resolve_path_access(
        path,
        cwd,
        config,
        ResolvePathAccessOptions {
            operation: options.mode,
            path_class: options.path_class,
            policy: Some(WorkspaceAccessPolicy {
                guard_mode: WorkspaceGuardMode::AbsoluteOutsideAllowed,
                check_sensitive: options
                    .check_sensitive
                    .unwrap_or(DEFAULT_WORKSPACE_ACCESS_POLICY.check_sensitive),
            }),
            home_dir: None,
        },
    )
    .map(|a| a.path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::WorkspaceConfig;

    fn workspace() -> WorkspaceConfig {
        WorkspaceConfig {
            workspace_dir: "/workspace".into(),
            additional_dirs: vec!["/extra".into()],
        }
    }

    fn win_workspace() -> WorkspaceConfig {
        WorkspaceConfig {
            workspace_dir: "C:\\workspace".into(),
            additional_dirs: vec!["D:\\extra".into()],
        }
    }

    #[test]
    fn default_policy_allows_absolute_outside_workspace() {
        let result = resolve_path_access(
            "/etc/hosts",
            "/workspace",
            &workspace(),
            ResolvePathAccessOptions {
                operation: PathAccessOperation::Read,
                policy: Some(DEFAULT_WORKSPACE_ACCESS_POLICY),
                path_class: Some(PathClass::Posix),
                home_dir: None,
            },
        )
        .unwrap();
        assert_eq!(result.path, "/etc/hosts");
        assert!(result.outside_workspace);
    }

    #[test]
    fn default_policy_rejects_relative_escape() {
        let err = resolve_path_access(
            "../../outside.txt",
            "/workspace/project",
            &workspace(),
            ResolvePathAccessOptions {
                operation: PathAccessOperation::Read,
                policy: Some(DEFAULT_WORKSPACE_ACCESS_POLICY),
                path_class: Some(PathClass::Posix),
                home_dir: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, PathSecurityCode::PathOutsideWorkspace);
        assert!(err.message.contains("absolute path"));
    }

    #[test]
    fn disabled_policy_allows_relative_escape() {
        let result = resolve_path_access(
            "../../outside.txt",
            "/workspace/project",
            &workspace(),
            ResolvePathAccessOptions {
                operation: PathAccessOperation::Read,
                policy: Some(WorkspaceAccessPolicy {
                    guard_mode: WorkspaceGuardMode::Disabled,
                    check_sensitive: true,
                }),
                path_class: Some(PathClass::Posix),
                home_dir: None,
            },
        )
        .unwrap();
        assert_eq!(result.path, "/outside.txt");
        assert!(result.outside_workspace);
    }

    #[test]
    fn expands_tilde_against_home_directory() {
        let result = resolve_path_access(
            "~/notes/today.txt",
            "/workspace",
            &workspace(),
            ResolvePathAccessOptions {
                operation: PathAccessOperation::Read,
                policy: Some(DEFAULT_WORKSPACE_ACCESS_POLICY),
                path_class: Some(PathClass::Posix),
                home_dir: Some("/home/test".into()),
            },
        )
        .unwrap();
        assert_eq!(result.path, "/home/test/notes/today.txt");
    }

    #[test]
    fn rejects_sensitive_file_independent_of_workspace_policy() {
        let err = resolve_path_access(
            "/tmp/.env",
            "/workspace",
            &workspace(),
            ResolvePathAccessOptions {
                operation: PathAccessOperation::Read,
                policy: Some(DEFAULT_WORKSPACE_ACCESS_POLICY),
                path_class: Some(PathClass::Posix),
                home_dir: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, PathSecurityCode::PathSensitive);
        assert!(err.message.contains("sensitive-file pattern"));
    }

    #[test]
    fn rejects_empty_path() {
        let err = canonicalize_path("", "/workspace", PathClass::Posix).unwrap_err();
        assert_eq!(err.code, PathSecurityCode::PathInvalid);
    }

    #[test]
    fn is_within_directory_uses_segment_boundaries() {
        assert!(!is_within_directory(
            "/workspace-evil/secrets.txt",
            "/workspace",
            PathClass::Posix
        ));
        assert!(!is_within_directory(
            "/workspace/file.txt",
            "/workspace-evil",
            PathClass::Posix
        ));
        assert!(is_within_directory(
            "/workspace",
            "/workspace",
            PathClass::Posix
        ));
    }

    #[test]
    fn additional_dirs_are_union_not_prefix() {
        let cfg = WorkspaceConfig {
            workspace_dir: "/workspace".into(),
            additional_dirs: vec!["/app-data".into()],
        };
        assert!(is_within_workspace(
            "/app-data/file.txt",
            &cfg,
            PathClass::Posix
        ));
        assert!(!is_within_workspace(
            "/app-data-evil/file.txt",
            &cfg,
            PathClass::Posix
        ));
    }

    #[test]
    fn canonicalizes_win32_paths_without_host_cwd() {
        let result = resolve_path_access(
            "sub\\..\\file.txt",
            "C:\\workspace",
            &win_workspace(),
            ResolvePathAccessOptions {
                operation: PathAccessOperation::Read,
                policy: Some(DEFAULT_WORKSPACE_ACCESS_POLICY),
                path_class: Some(PathClass::Win32),
                home_dir: None,
            },
        )
        .unwrap();
        assert_eq!(result.path, "C:/workspace/file.txt");
        assert!(!result.outside_workspace);
        assert!(is_within_directory(
            "C:/WORKSPACE/file.txt",
            "c:/workspace",
            PathClass::Win32,
        ));
    }

    #[test]
    fn rejects_win32_drive_relative_paths() {
        let err = resolve_path_access(
            "D:outside.txt",
            "C:\\workspace",
            &win_workspace(),
            ResolvePathAccessOptions {
                operation: PathAccessOperation::Read,
                policy: Some(DEFAULT_WORKSPACE_ACCESS_POLICY),
                path_class: Some(PathClass::Win32),
                home_dir: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, PathSecurityCode::PathInvalid);
        assert!(err.message.contains("drive-relative"));
    }

    #[test]
    fn normalizes_msys_and_cygdrive_paths_on_win32() {
        assert_eq!(
            normalize_user_path("/c/Users/foo/file.txt", PathClass::Win32),
            "C:/Users/foo/file.txt"
        );
        assert_eq!(
            normalize_user_path("/cygdrive/d/Projects", PathClass::Win32),
            "D:/Projects"
        );
        assert_eq!(
            normalize_user_path("/C/Users/foo", PathClass::Win32),
            "C:/Users/foo"
        );
        assert_eq!(normalize_user_path("/c/", PathClass::Win32), "C:/");
        assert_eq!(normalize_user_path("/c", PathClass::Win32), "C:/");
    }

    #[test]
    fn leaves_posix_paths_alone() {
        assert_eq!(
            normalize_user_path("/c/Users/foo", PathClass::Posix),
            "/c/Users/foo"
        );
        assert_eq!(
            normalize_user_path("/cygdrive/x", PathClass::Posix),
            "/cygdrive/x"
        );
    }
}
