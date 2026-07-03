# Part 2 — Path Security Policy, Sensitive Files, Rule / Path Matching

**Goal:** Port the lexical path-security guard (`path-access`), sensitive-file detector, and rule/path glob matcher from TypeScript into pure Rust helpers in `tools-rs`.

**Architecture:** All helpers are I/O-free and operate on path strings. `path_access` defines canonicalization, workspace containment, and policy enforcement; `sensitive` is a standalone basename/path matcher; `path_glob_match` builds equivalent path spellings before delegating to `globset`; `rule_match` combines glob/path matching with negated permission rules. Every helper is tested with behavioral asserts, including explicit "must survive" cases for the sensitive-file filter.

**Tech Stack:** Rust 2021, `globset`, `regex`, `thiserror`, `serde`.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| Responsibility | Path |
|---|---|
| Policy module root | `rust-ody/crates/tools-rs/src/policies/mod.rs` |
| Path security policy | `rust-ody/crates/tools-rs/src/policies/path_access.rs` |
| Sensitive-file detector | `rust-ody/crates/tools-rs/src/policies/sensitive.rs` |
| Glob/path matcher | `rust-ody/crates/tools-rs/src/policies/path_glob_match.rs` |
| Rule subject matcher | `rust-ody/crates/tools-rs/src/policies/rule_match.rs` |
| Crate public export | `rust-ody/crates/tools-rs/src/lib.rs` |

---

## Dependency Overview

```
Task 4  Path security policy + sensitive files
   │
   └──► Task 5  Rule / path matching
```

- Task 4 depends on Task 2 from Part 1 (`WorkspaceConfig` in `tools-rs/src/workspace.rs`).
- Task 5 depends on Task 4 (`path_access` canonicalization and `PathClass`).
- Tasks 4 and 5 are independent of the other Part 1 helpers.

---

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| Windows path canonicalization on a POSIX host may behave differently from `pathe` | Implement a pure string-based normalizer that handles drive letters, UNC roots, and `..` resolution without relying on the host OS `std::path`. |
| `globset` brace expansion semantics differ from `picomatch` | Implement recursive top-level brace expansion before passing patterns to `globset`; L1 fixtures will cover nested braces and escaped specials. |
| Sensitive-file filter false positives | Include the exact TS "must survive" inputs in the Rust tests and assert they return `false`. |

---

## Task 4: Path security policy and sensitive-file detection

**Depends on:** Part 1 Task 2 (`WorkspaceConfig` in `tools-rs/src/workspace.rs`)  
**Files:**
- Create: `rust-ody/crates/tools-rs/src/policies/mod.rs`
- Create: `rust-ody/crates/tools-rs/src/policies/path_access.rs`
- Create: `rust-ody/crates/tools-rs/src/policies/sensitive.rs`
- Modify: `rust-ody/crates/tools-rs/src/lib.rs`

**Goal:** Port the lexical path guard (`canonicalize_path`, `is_within_workspace`, `resolve_path_access`, `assert_path_allowed`) and the sensitive-file detector so builtin file tools can reject dangerous paths without filesystem I/O.

### Steps

- [ ] Create `rust-ody/crates/tools-rs/src/policies/mod.rs`:

```rust
pub mod path_access;
pub mod sensitive;
```

- [ ] Write the failing tests first in `rust-ody/crates/tools-rs/src/policies/path_access.rs`:

```rust
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
        assert!(is_within_directory("/workspace-evil/secrets.txt", "/workspace", PathClass::Posix));
        assert!(!is_within_directory("/workspace/file.txt", "/workspace-evil", PathClass::Posix));
        assert!(is_within_directory("/workspace", "/workspace", PathClass::Posix));
    }

    #[test]
    fn additional_dirs_are_union_not_prefix() {
        let cfg = WorkspaceConfig {
            workspace_dir: "/workspace".into(),
            additional_dirs: vec!["/app-data".into()],
        };
        assert!(is_within_workspace("/app-data/file.txt", &cfg, PathClass::Posix));
        assert!(!is_within_workspace("/app-data-evil/file.txt", &cfg, PathClass::Posix));
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
        assert_eq!(normalize_user_path("/c/Users/foo/file.txt", PathClass::Win32), "C:/Users/foo/file.txt");
        assert_eq!(normalize_user_path("/cygdrive/d/Projects", PathClass::Win32), "D:/Projects");
        assert_eq!(normalize_user_path("/C/Users/foo", PathClass::Win32), "C:/Users/foo");
        assert_eq!(normalize_user_path("/c/", PathClass::Win32), "C:/");
        assert_eq!(normalize_user_path("/c", PathClass::Win32), "C:/");
    }

    #[test]
    fn leaves_posix_paths_alone() {
        assert_eq!(normalize_user_path("/c/Users/foo", PathClass::Posix), "/c/Users/foo");
        assert_eq!(normalize_user_path("/cygdrive/x", PathClass::Posix), "/cygdrive/x");
    }
}
```

- [ ] Write the failing tests first in `rust-ody/crates/tools-rs/src/policies/sensitive.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_sensitive_basenames() {
        for path in [".env", "/app/.env", "project/.env"] {
            assert!(is_sensitive_file(path), "{path}");
        }
    }

    #[test]
    fn flags_env_variants() {
        for path in [".env.local", ".env.production", "/app/.env.staging"] {
            assert!(is_sensitive_file(path), "{path}");
        }
    }

    #[test]
    fn flags_cloud_credentials() {
        for path in ["/home/user/.aws/credentials", "/home/user/.gcp/credentials", ".aws/credentials", ".gcp/credentials", "credentials"] {
            assert!(is_sensitive_file(path), "{path}");
        }
    }

    #[test]
    fn matches_ssh_key_variants() {
        assert!(is_sensitive_file("id_rsa"));
        assert!(is_sensitive_file("/home/user/.ssh/id_rsa"));
        assert!(is_sensitive_file("id_ed25519_old"));
        assert!(is_sensitive_file("id_rsa.bak"));
    }

    #[test]
    fn must_survive_inputs_are_not_sensitive() {
        // These inputs are explicitly required to pass through the filter.
        for path in [
            "app.py",
            "config.yml",
            "README.md",
            "package.json",
            "server.key.example",
            "id_rsa.pub",
            "credentials.json",
            ".envrc",
            "environment.py",
            ".env_example",
            ".env.example",
            ".ENV.EXAMPLE",
            ".env.sample",
            ".ENV.SAMPLE",
            ".env.template",
            ".ENV.TEMPLATE",
            "/app/.env.example",
            "/app/.ENV.EXAMPLE",
            "id_rsafoo",
        ] {
            assert!(!is_sensitive_file(path), "{path} must not be flagged");
        }
    }
}
```

- [ ] Run the tests and confirm they fail because the implementation is missing:

```bash
cd rust-ody && cargo test -p tools-rs policies::path_access::tests
```

Expected failure: `cannot find module `policies` in module `tools_rs`` or similar.

- [ ] Create `rust-ody/crates/tools-rs/src/policies/path_access.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::sensitive::is_sensitive_file;
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
        return format!("{}:{}", drive, if rest == "/" { "" } else { rest });
    }

    if let Some(m) = regex::Regex::new(r"^/([A-Za-z])(?:/|$)").unwrap().captures(path) {
        let drive = m[1].to_uppercase();
        let rest = &path[m[0].len() - 1..];
        return format!("{}:{}", drive, if rest == "/" { "" } else { rest });
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

fn normalize_path(path: &str, path_class: PathClass) -> String {
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
        if parts.len() > 1 && parts[1].is_empty() {
            root.push_str(&parts[0][..1].to_uppercase());
            root.push_str(":/");
            i = 2;
            is_abs = true;
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
            message: format!("Cannot resolve \"{}\" against non-absolute cwd \"{}\".", path, cwd),
        });
    }
    let abs = if is_absolute(&normalized_path, path_class) {
        normalized_path
    } else {
        join_paths(cwd, &normalized_path, path_class)
    };
    Ok(normalize_path(&abs, path_class))
}

pub fn is_within_directory(
    candidate: &str,
    base: &str,
    path_class: PathClass,
) -> bool {
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
    resolve_path_access(path, &workspace.workspace_dir, &workspace, ResolvePathAccessOptions {
        operation,
        policy,
        path_class,
        home_dir: home,
    })
    .map(|a| a.path)
}

pub fn assert_path_allowed(
    path: &str,
    cwd: &str,
    config: &WorkspaceConfig,
    options: AssertPathOptions,
) -> Result<String, PathSecurityError> {
    resolve_path_access(path, cwd, config, ResolvePathAccessOptions {
        operation: options.mode,
        path_class: options.path_class,
        policy: Some(WorkspaceAccessPolicy {
            guard_mode: WorkspaceGuardMode::AbsoluteOutsideAllowed,
            check_sensitive: options.check_sensitive.unwrap_or(DEFAULT_WORKSPACE_ACCESS_POLICY.check_sensitive),
        }),
        home_dir: None,
    })
    .map(|a| a.path)
}
```

- [ ] Create `rust-ody/crates/tools-rs/src/policies/sensitive.rs`:

```rust
use std::collections::HashSet;

const SENSITIVE_BASENAMES: &[&str] = &[".env", "id_rsa", "id_ed25519", "id_ecdsa", "credentials"];

const SENSITIVE_PATH_SUFFIXES: &[&[&str]] = &[&[".aws", "credentials"], &[".gcp", "credentials"]];

const ENV_PREFIX: &str = ".env.";

const ENV_EXEMPTIONS: &[&str] = &[".env.example", ".env.sample", ".env.template"];

const SENSITIVE_BASENAME_PREFIXES: &[&str] = &["id_rsa", "id_ed25519", "id_ecdsa", "credentials"];

const PUBLIC_KEY_BASENAMES: &[&str] = &["id_rsa.pub", "id_ed25519.pub", "id_ecdsa.pub"];

const SENSITIVE_DOT_VARIANT_SUFFIXES: &[&str] = &[
    ".bak", ".backup", ".copy", ".disabled", ".key", ".old", ".orig", ".pem", ".save", ".tmp",
];

fn comparable(path: &str) -> String {
    path.to_lowercase()
}

fn basename(path: &str) -> &str {
    path.rsplit_once(&['/', '\\'][..])
        .map(|(_, name)| name)
        .unwrap_or(path)
}

pub fn is_sensitive_file(path: &str) -> bool {
    let name = basename(path);
    let comparable_name = comparable(name);
    let comparable_path = comparable(path);

    if ENV_EXEMPTIONS.contains(&comparable_name.as_str()) {
        return false;
    }
    if PUBLIC_KEY_BASENAMES.contains(&comparable_name.as_str()) {
        return false;
    }

    let sensitive_basenames: HashSet<&str> = SENSITIVE_BASENAMES.iter().copied().collect();
    if sensitive_basenames.contains(comparable_name.as_str()) {
        return true;
    }

    if comparable_name.starts_with(ENV_PREFIX) {
        return true;
    }

    let dot_variant_suffixes: HashSet<&str> = SENSITIVE_DOT_VARIANT_SUFFIXES.iter().copied().collect();
    for prefix in SENSITIVE_BASENAME_PREFIXES {
        if comparable_name == *prefix {
            return true;
        }
        if comparable_name.len() > prefix.len() && comparable_name.starts_with(prefix) {
            let suffix = &comparable_name[prefix.len()..];
            let next = suffix.chars().next().unwrap();
            if next == '-' || next == '_' {
                return true;
            }
            if next == '.' && dot_variant_suffixes.contains(suffix) {
                return true;
            }
        }
    }

    for suffix_parts in SENSITIVE_PATH_SUFFIXES {
        let suffix = suffix_parts.join("/");
        let comparable_suffix = comparable(&suffix);
        if comparable_path.ends_with(&format!("/{}", comparable_suffix))
            || comparable_path.contains(&format!("/{}/", comparable_suffix))
        {
            return true;
        }
    }

    false
}
```

- [ ] Update `rust-ody/crates/tools-rs/src/lib.rs` to expose the policies module:

```rust
pub mod policies;
pub mod result_builder;
pub mod store;
pub mod tool_accesses;
pub mod types;
pub mod workspace;
```

- [ ] Run the tests:

```bash
cd rust-ody && cargo test -p tools-rs policies
```

Expected: `test result: ok.` for all path_access and sensitive tests.

- [ ] Commit:

```bash
git add rust-ody/crates/tools-rs/src

git commit -m "feat(tools-rs): path security policy and sensitive-file detector"
```

---

## Task 5: Rule / path matching

**Depends on:** Task 4 (`path_access` canonicalization and `PathClass`)  
**Files:**
- Create: `rust-ody/crates/tools-rs/src/policies/path_glob_match.rs`
- Create: `rust-ody/crates/tools-rs/src/policies/rule_match.rs`
- Modify: `rust-ody/crates/tools-rs/src/policies/mod.rs`

**Goal:** Port the `globMatch`/`pathGlobMatch` helpers and the permission-rule subject matchers (`matchesGlobRuleSubject`, `matchesPathRuleSubject`, `literalRulePattern`) used by tool permission guards.

### Steps

- [ ] Write the failing tests first in `rust-ody/crates/tools-rs/src/policies/path_glob_match.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_matches_in_same_segment() {
        assert!(glob_match("main.ts", "*.ts", false));
        assert!(!glob_match("src/main.ts", "*.ts", false));
    }

    #[test]
    fn double_star_matches_across_segments() {
        assert!(glob_match("src/deep/main.ts", "src/**/*.ts", false));
        assert!(!glob_match("main.ts", "src/**/*.ts", false));
    }

    #[test]
    fn brace_expansion_matches_alternatives() {
        assert!(glob_match("a/b.ts", "a/{b,c}.ts", false));
        assert!(glob_match("a/c.ts", "a/{b,c}.ts", false));
        assert!(!glob_match("a/d.ts", "a/{b,c}.ts", false));
    }

    #[test]
    fn nested_brace_expansion_works() {
        assert!(glob_match("a/c.ts", "a/{b,{c,d}}.ts", false));
        assert!(glob_match("a/d.ts", "a/{b,{c,d}}.ts", false));
        assert!(!glob_match("a/z.ts", "a/{b,{c,d}}.ts", false));
    }

    #[test]
    fn nocase_option_is_honored() {
        assert!(glob_match("MAIN.TS", "*.ts", true));
        assert!(!glob_match("MAIN.TS", "*.ts", false));
    }

    #[test]
    fn escaped_special_is_literal() {
        assert!(glob_match("a*b", "a\\*b", false));
        assert!(!glob_match("aXb", "a\\*b", false));
    }

    #[test]
    fn question_mark_matches_single_char() {
        assert!(glob_match("aXb", "a?b", false));
        assert!(!glob_match("a/b", "a?b", false));
    }

    #[test]
    fn character_class_matches() {
        assert!(glob_match("abc", "a[bc]c", false));
        assert!(!glob_match("adc", "a[bc]c", false));
    }

    #[test]
    fn path_glob_strips_leading_dot_slash() {
        let opts = PermissionPathMatchOptions {
            cwd: Some("/repo".into()),
            ..Default::default()
        };
        assert!(path_glob_match("./main.ts", "*.ts", Some(&opts)));
    }

    #[test]
    fn path_glob_is_case_insensitive_by_default() {
        assert!(path_glob_match("MAIN.TS", "*.ts", None));
    }

    #[test]
    fn path_glob_uses_canonical_variant() {
        let opts = PermissionPathMatchOptions {
            cwd: Some("/repo".into()),
            ..Default::default()
        };
        assert!(path_glob_match("src/../main.ts", "main.ts", Some(&opts)));
    }
}
```

- [ ] Write the failing tests first in `rust-ody/crates/tools-rs/src/policies/rule_match.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::path_glob_match::PermissionPathMatchOptions;

    #[test]
    fn literal_pattern_wraps_subject() {
        assert_eq!(
            literal_rule_pattern("read", "/repo/src/main.ts"),
            "read(/repo/src/main.ts)"
        );
    }

    #[test]
    fn escapes_glob_metacharacters_in_literal() {
        assert_eq!(
            escape_rule_subject_literal("a*b[c]d{e,f}"),
            "a\\*b\\[c\\]d\\{e\\,f\\}"
        );
    }

    #[test]
    fn glob_subject_match() {
        assert!(matches_glob_rule_subject("*.ts", "main.ts"));
        assert!(!matches_glob_rule_subject("*.py", "main.ts"));
    }

    #[test]
    fn negated_glob_rule_inverts_match() {
        assert!(!matches_glob_rule_subject("!*.ts", "main.ts"));
        assert!(matches_glob_rule_subject("!*.py", "main.ts"));
    }

    #[test]
    fn empty_rule_args_matches_everything() {
        assert!(matches_glob_rule_subject("", "anything"));
    }

    #[test]
    fn path_subject_match() {
        let opts = PermissionPathMatchOptions {
            cwd: Some("/repo".into()),
            ..Default::default()
        };
        assert!(matches_path_rule_subject("src/**/*.ts", "src/main.ts", Some(&opts)));
        assert!(!matches_path_rule_subject("src/**/*.ts", "src/main.py", Some(&opts)));
    }

    #[test]
    fn negated_path_rule_inverts_match() {
        let opts = PermissionPathMatchOptions {
            cwd: Some("/repo".into()),
            ..Default::default()
        };
        assert!(!matches_path_rule_subject("!src/**/*.ts", "src/main.ts", Some(&opts)));
        assert!(matches_path_rule_subject("!src/**/*.py", "src/main.ts", Some(&opts)));
    }
}
```

- [ ] Run the tests and confirm they fail because the modules are not wired:

```bash
cd rust-ody && cargo test -p tools-rs policies::path_glob_match::tests
```

Expected failure: `cannot find module `path_glob_match` in module `policies``.

- [ ] Create `rust-ody/crates/tools-rs/src/policies/path_glob_match.rs`:

```rust
use globset::GlobBuilder;

use crate::path_access::{canonicalize_path, PathClass};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PermissionPathMatchOptions {
    pub cwd: Option<String>,
    pub path_class: Option<PathClass>,
    pub home_dir: Option<String>,
    pub case_insensitive_paths: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PathMatchSemantics {
    path_class: PathClass,
}

/// Match a glob pattern against a value. Supports `*`, `**`, `?`, character
/// classes, backslash escaping, and recursive brace expansion.
pub fn glob_match(value: &str, pattern: &str, nocase: bool) -> bool {
    for p in expand_braces(pattern) {
        let glob = match GlobBuilder::new(&p)
            .literal_separator(true)
            .backslash_escape(true)
            .case_insensitive(nocase)
            .build()
        {
            Ok(g) => g,
            Err(_) => continue,
        };
        if glob.compile_matcher().is_match(value) {
            return true;
        }
    }
    false
}

/// Match file path fields, normalizing equivalent spellings (`./a`,
/// `dir/../a`, Windows separators) before glob matching.
pub fn path_glob_match(
    value: &str,
    pattern: &str,
    options: Option<&PermissionPathMatchOptions>,
) -> bool {
    let semantics = path_match_semantics(value, pattern, options);
    let nocase = options.and_then(|o| o.case_insensitive_paths).unwrap_or(true);

    if glob_match(value, pattern, nocase) {
        return true;
    }

    let value_variants = path_variants(value, &semantics, options);
    let pattern_variants = path_variants(pattern, &semantics, options);
    for value_variant in &value_variants {
        for pattern_variant in &pattern_variants {
            if glob_match(value_variant, pattern_variant, nocase) {
                return true;
            }
        }
    }
    false
}

fn path_variants(
    value: &str,
    semantics: &PathMatchSemantics,
    options: Option<&PermissionPathMatchOptions>,
) -> Vec<String> {
    let mut variants = std::collections::HashSet::new();
    add_path_variant(&mut variants, value, semantics.path_class);
    add_path_variant(
        &mut variants,
        &strip_leading_dot_path(value, semantics.path_class),
        semantics.path_class,
    );
    if let Some(canonical) = canonicalize_path_pattern(value, semantics, options) {
        add_path_variant(&mut variants, &canonical, semantics.path_class);
    }
    variants.into_iter().collect()
}

fn canonicalize_path_pattern(
    value: &str,
    semantics: &PathMatchSemantics,
    options: Option<&PermissionPathMatchOptions>,
) -> Option<String> {
    let expanded = expand_user_path(value, semantics.path_class, options.and_then(|o| o.home_dir.as_deref()));
    let cwd = options
        .and_then(|o| o.cwd.as_deref())
        .or_else(|| default_cwd_for_path(&expanded, semantics.path_class))?;
    canonicalize_path(&expanded, cwd, semantics.path_class).ok()
}

fn default_cwd_for_path(value: &str, path_class: PathClass) -> Option<String> {
    if !is_absolute_path(value, path_class) {
        return None;
    }
    match path_class {
        PathClass::Posix => Some("/".to_string()),
        PathClass::Win32 => {
            let s = value.replace('\\', "/");
            if s.starts_with("//") {
                let rest = &s[2..];
                let first = rest.find('/')?;
                let after = &rest[first + 1..];
                let second = after.find('/').unwrap_or(after.len());
                Some(format!("//{}/{}", &rest[..first], &after[..second]))
            } else if s.len() >= 2 && s.as_bytes()[1] == b':' {
                let drive = s[..2].to_uppercase();
                Some(format!("{}/", drive))
            } else {
                Some("C:/".to_string())
            }
        }
    }
}

fn is_absolute_path(path: &str, path_class: PathClass) -> bool {
    match path_class {
        PathClass::Posix => path.starts_with('/'),
        PathClass::Win32 => {
            path.starts_with("//")
                || path.starts_with("\\\\")
                || (path.len() >= 2
                    && path.as_bytes()[1] == b':'
                    && path.as_bytes()[0].is_ascii_alphabetic())
        }
    }
}

fn expand_user_path(value: &str, path_class: PathClass, home_dir: Option<&str>) -> String {
    let home = match home_dir {
        Some(h) => h,
        None => return value.to_string(),
    };
    if value == "~" {
        return home.to_string();
    }
    if value.starts_with("~/") || (path_class == PathClass::Win32 && value.starts_with("~\\")) {
        let rest = &value[2..];
        return format!("{}/{}", home.replace('\\', "/"), rest);
    }
    value.to_string()
}

fn path_match_semantics(
    value: &str,
    pattern: &str,
    options: Option<&PermissionPathMatchOptions>,
) -> PathMatchSemantics {
    let path_class = options.and_then(|o| o.path_class).unwrap_or_else(|| {
        let is_win32 = [value, pattern].iter().any(|candidate| {
            candidate.starts_with("\\\\")
                || candidate.starts_with("//")
                || candidate.contains('\\')
                || (candidate.len() >= 2
                    && candidate.as_bytes()[1] == b':'
                    && candidate.as_bytes()[0].is_ascii_alphabetic())
        });
        if is_win32 {
            PathClass::Win32
        } else {
            PathClass::Posix
        }
    });
    PathMatchSemantics { path_class }
}

fn add_path_variant(variants: &mut std::collections::HashSet<String>, value: &str, path_class: PathClass) {
    variants.insert(value.to_string());
    if path_class == PathClass::Win32 {
        variants.insert(value.replace('\\', "/"));
    }
}

fn strip_leading_dot_path(value: &str, path_class: PathClass) -> String {
    if value.starts_with("./") {
        value[2..].to_string()
    } else if path_class == PathClass::Win32 && value.starts_with(".\\") {
        value[2..].to_string()
    } else {
        value.to_string()
    }
}

/// Recursively expand `{a,b}` braces, ignoring braces inside `[...]` character
/// classes. Returns the original pattern if no braces are present.
fn expand_braces(pattern: &str) -> Vec<String> {
    let mut bracket_depth = 0i32;
    let mut brace_start: Option<usize> = None;

    for (i, &b) in pattern.as_bytes().iter().enumerate() {
        match b {
            b'[' => bracket_depth += 1,
            b']' => bracket_depth = (bracket_depth - 1).max(0),
            b'{' if bracket_depth == 0 => {
                brace_start = Some(i);
                break;
            }
            _ => {}
        }
    }

    let start = match brace_start {
        Some(s) => s,
        None => return vec![pattern.to_string()],
    };

    let mut i = start + 1;
    let mut inner_bracket_depth = 0i32;
    let mut inner_brace_depth = 0i32;
    let mut brace_end: Option<usize> = None;

    while i < pattern.len() {
        match pattern.as_bytes()[i] {
            b'[' => inner_bracket_depth += 1,
            b']' => inner_bracket_depth = (inner_bracket_depth - 1).max(0),
            b'{' if inner_bracket_depth == 0 => inner_brace_depth += 1,
            b'}' if inner_bracket_depth == 0 => {
                if inner_brace_depth == 0 {
                    brace_end = Some(i);
                    break;
                }
                inner_brace_depth -= 1;
            }
            _ => {}
        }
        i += 1;
    }

    let end = match brace_end {
        Some(e) => e,
        None => return vec![pattern.to_string()],
    };

    let prefix = &pattern[..start];
    let inner = &pattern[start + 1..end];
    let suffix = &pattern[end + 1..];

    let mut out = Vec::new();
    for choice in split_top_level_commas(inner) {
        let partial = format!("{}{}{}", prefix, choice, suffix);
        for expanded in expand_braces(&partial) {
            out.push(expanded);
        }
    }
    out
}

fn split_top_level_commas(inner: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut start = 0;
    let mut bracket_depth = 0i32;
    let mut brace_depth = 0i32;

    for (i, &b) in inner.as_bytes().iter().enumerate() {
        match b {
            b'[' => bracket_depth += 1,
            b']' => bracket_depth = (bracket_depth - 1).max(0),
            b'{' if bracket_depth == 0 => brace_depth += 1,
            b'}' if bracket_depth == 0 => brace_depth = (brace_depth - 1).max(0),
            b',' if bracket_depth == 0 && brace_depth == 0 => {
                items.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    items.push(&inner[start..]);
    items
}
```

- [ ] Create `rust-ody/crates/tools-rs/src/policies/rule_match.rs`:

```rust
use regex::Regex;

use crate::path_glob_match::{glob_match, path_glob_match, PermissionPathMatchOptions};

pub fn literal_rule_pattern(tool_name: &str, subject: &str) -> String {
    format!("{}({})", tool_name, escape_rule_subject_literal(subject))
}

pub fn escape_rule_subject_literal(subject: &str) -> String {
    Regex::new(r"[\\*?\[\]{}()!+@|]")
        .unwrap()
        .replace_all(subject, "\\$0")
        .to_string()
}

pub fn matches_glob_rule_subject(rule_args: &str, subject: &str) -> bool {
    match_rule_subjects(rule_args, &[subject], |pattern, value| glob_match(value, pattern, true))
}

pub fn matches_path_rule_subject(
    rule_args: &str,
    subject: &str,
    options: Option<&PermissionPathMatchOptions>,
) -> bool {
    match_rule_subjects(rule_args, &[subject], |pattern, value| {
        path_glob_match(value, pattern, options)
    })
}

fn match_rule_subjects(
    rule_args: &str,
    subjects: &[&str],
    matcher: impl Fn(&str, &str) -> bool,
) -> bool {
    if rule_args.is_empty() {
        return true;
    }
    let negated = rule_args.starts_with('!');
    let positive_pattern = if negated { &rule_args[1..] } else { rule_args };
    let hit = subjects.iter().any(|subject| matcher(positive_pattern, subject));
    if negated {
        !hit
    } else {
        hit
    }
}
```

- [ ] Update `rust-ody/crates/tools-rs/src/policies/mod.rs`:

```rust
pub mod path_access;
pub mod path_glob_match;
pub mod rule_match;
pub mod sensitive;
```

- [ ] Run the tests:

```bash
cd rust-ody && cargo test -p tools-rs policies
```

Expected: `test result: ok.` for `path_access`, `sensitive`, `path_glob_match`, and `rule_match`.

- [ ] Commit:

```bash
git add rust-ody/crates/tools-rs/src

git commit -m "feat(tools-rs): rule and path glob matching"
```

---

## Local Self-Review (Part 2)

- [ ] 1. Spec-coverage table:

| Spec item | Task(s) | Status |
|---|---|---|
| 4.4.0 — Path lexical canonicalization (`canonicalize_path`, `normalize_user_path`) | Task 4 | covered |
| 4.4.0 — Workspace containment / guard modes (`is_within_workspace`, `resolve_path_access`) | Task 4 | covered |
| 4.4.0 — Sensitive-file detection (`is_sensitive_file`) | Task 4 | covered |
| 4.4.0 — Glob matching with braces, `**`, escaping, nocase (`glob_match`) | Task 5 | covered |
| 4.4.0 — Path-aware glob matching with normalized variants (`path_glob_match`) | Task 5 | covered |
| 4.4.0 — Permission rule subject matching and negation (`matches_*_rule_subject`, `literal_rule_pattern`) | Task 5 | covered |
| 4.4.0 — L1 parity fixtures for the helpers above | Part 5 (fixtures-ci.md) | downstream |

- [ ] 2. Placeholder scan: no TODO/TBD, no deferred implementation notes; every function body is provided.
- [ ] 3. No phantom tasks: each task creates source files, updates `lib.rs`/`mod.rs`, and ends with a passing `cargo test -p tools-rs policies`.
- [ ] 4. Dependency soundness: Task 4 depends only on Part 1 Task 2 (`WorkspaceConfig`); Task 5 depends only on Task 4 (`PathClass`, `canonicalize_path`). No symbol is used before it is defined.
- [ ] 5. Caller & build soundness: Part 2 only adds new modules and does not change any existing public signatures, so no caller updates are required. Each task ends with a crate-level test run.
- [ ] 6. Test-the-risk:
  - `path_access` asserts that relative workspace escapes throw `PathSecurityError` and that sensitive files are rejected.
  - `sensitive` asserts the exact TS "must survive" inputs return `false`.
  - `path_glob_match` asserts `*`, `**`, braces, nested braces, escaped specials, character classes, and path normalization.
  - `rule_match` asserts positive/negated matches and literal escaping.
- [ ] 7. Type consistency: `PathClass` and canonicalization types defined in Task 4 are reused verbatim in Task 5's `PermissionPathMatchOptions`.
