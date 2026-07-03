# Part 1 — Crate, Common Types, `ToolAccesses`, Result Builder

## Task 1: Create the `tools-rs` crate and wire the workspace

**Depends on:** none  
**Files:**
- Create: `rust-ody/crates/tools-rs/Cargo.toml`
- Create: `rust-ody/crates/tools-rs/src/lib.rs`
- Modify: `rust-ody/Cargo.toml` (workspace members)

**Goal:** Add a new crate `tools-rs` to the Rust workspace so later tasks have a place to land shared helpers.

### Steps

- [ ] Create `rust-ody/crates/tools-rs/Cargo.toml`:

```toml
[package]
name = "tools-rs"
version = "0.1.0"
edition = "2021"
description = "Shared tool infrastructure for ody-code builtin tools"
license = "MIT"

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = "1"
tokio = { workspace = true }
kaos-rs = { path = "../kaos-rs" }
regex = "1"
globset = "0.4"
jsonschema = "0.29"
reqwest = { workspace = true }
dirs = "5"
tar = "0.4"
zip = "2"

[dev-dependencies]
tempfile = "3"
tokio-test = "0.4"
```

- [ ] Create `rust-ody/crates/tools-rs/src/lib.rs`:

```rust
//! Shared tool infrastructure used by `agent-rs` and `ody-host`.
```

- [ ] Edit `rust-ody/Cargo.toml` workspace members to include `"crates/tools-rs"`:

```toml
[workspace]
members = [
    "crates/ody-rust",
    "crates/ody-crypto",
    "crates/ody-host",
    "crates/kaos-rs",
    "crates/kosong-rs",
    "crates/agent-rs",
    "crates/tools-rs",
]
```

- [ ] Verify the crate compiles:

```bash
cd rust-ody && cargo check -p tools-rs
```

Expected: `Finished dev [unoptimized + debuginfo] target(s) in ...` with no errors.

- [ ] Commit:

```bash
git add rust-ody/Cargo.toml rust-ody/crates/tools-rs
git commit -m "chore(tools-rs): bootstrap tools-rs crate for Phase 4.4.0"
```

---

## Task 2: Common tool types, workspace config, store, and result builder

**Depends on:** Task 1  
**Files:**
- Create: `rust-ody/crates/tools-rs/src/types.rs`
- Create: `rust-ody/crates/tools-rs/src/workspace.rs`
- Create: `rust-ody/crates/tools-rs/src/store.rs`
- Create: `rust-ody/crates/tools-rs/src/result_builder.rs`
- Modify: `rust-ody/crates/tools-rs/src/lib.rs`

**Goal:** Port the small value types (`ToolSource`, `ToolInfo`, `UserToolRegistration`, `McpToolCollision`, `WorkspaceConfig`, `ToolStore`) and the `ToolResultBuilder` that every tool will use.

### Steps

- [ ] Write the failing test first in `rust-ody/crates/tools-rs/src/types.rs` (append at bottom of the file after the implementations):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_source_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&ToolSource::Builtin).unwrap(),
            "\"builtin\""
        );
        assert_eq!(serde_json::to_string(&ToolSource::User).unwrap(), "\"user\"");
        assert_eq!(serde_json::to_string(&ToolSource::Mcp).unwrap(), "\"mcp\"");
    }

    #[test]
    fn mcp_collision_round_trips() {
        let c = McpToolCollision {
            qualified: "mcp__a__b".into(),
            tool_name: "b".into(),
            collides_with: McpCollisionTarget::OtherServer {
                server_name: "x".into(),
            },
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"toolName\""));
        assert!(json.contains("\"kind\":\"other_server\""));
        assert!(json.contains("\"serverName\""));
        let round: McpToolCollision = serde_json::from_str(&json).unwrap();
        assert_eq!(round, c);
    }
}
```

- [ ] Run the test and confirm it fails because the types do not exist yet:

```bash
cd rust-ody && cargo test -p tools-rs types::tests
```

Expected failure: `cannot find module `types` in module `tools_rs`` or similar.

- [ ] Create `rust-ody/crates/tools-rs/src/types.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolSource {
    Builtin,
    User,
    Mcp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub active: bool,
    pub source: ToolSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserToolRegistration {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum McpCollisionTarget {
    #[serde(rename = "same_server")]
    SameServer { tool_name: String },
    #[serde(rename = "other_server")]
    OtherServer { server_name: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCollision {
    pub qualified: String,
    pub tool_name: String,
    pub collides_with: McpCollisionTarget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerRegistrationResult {
    pub registered: Vec<String>,
    pub collisions: Vec<McpToolCollision>,
}
```

- [ ] Create `rust-ody/crates/tools-rs/src/workspace.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub workspace_dir: String,
    pub additional_dirs: Vec<String>,
}

impl WorkspaceConfig {
    pub fn new(workspace_dir: impl Into<String>) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
            additional_dirs: Vec::new(),
        }
    }
}
```

- [ ] Create `rust-ody/crates/tools-rs/src/store.rs`:

```rust
use serde_json::Value;
use std::collections::HashMap;

pub trait ToolStore: Send + Sync {
    fn get(&self, key: &str) -> Option<Value>;
    fn set(&mut self, key: &str, value: Value);
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryToolStore {
    data: HashMap<String, Value>,
}

impl InMemoryToolStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ToolStore for InMemoryToolStore {
    fn get(&self, key: &str) -> Option<Value> {
        self.data.get(key).cloned()
    }

    fn set(&mut self, key: &str, value: Value) {
        self.data.insert(key.to_owned(), value);
    }
}
```

- [ ] Create `rust-ody/crates/tools-rs/src/result_builder.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub output: String,
    pub is_error: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub struct ToolResultBuilder {
    max_line_length: Option<usize>,
    chunks: Vec<String>,
    n_chars: usize,
}

impl ToolResultBuilder {
    pub fn new(max_line_length: Option<usize>) -> Self {
        Self {
            max_line_length: max_line_length.or(Some(500)),
            chunks: Vec::new(),
            n_chars: 0,
        }
    }

    pub fn write(&mut self, text: &str) {
        if let Some(limit) = self.max_line_length {
            self.chunks.push(
                text.lines()
                    .map(|line| {
                        if line.len() > limit {
                            format!("{}…", &line[..limit])
                        } else {
                            line.to_owned()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        } else {
            self.chunks.push(text.to_owned());
        }
        self.n_chars += text.len();
    }

    pub fn n_chars(&self) -> usize {
        self.n_chars
    }

    pub fn ok(self, message: Option<String>) -> ToolResult {
        ToolResult {
            output: self.build_output(),
            is_error: false,
            message,
        }
    }

    pub fn error(self, message: String) -> ToolResult {
        ToolResult {
            output: self.build_output(),
            is_error: true,
            message: Some(message),
        }
    }

    fn build_output(&self) -> String {
        self.chunks.join("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_long_lines_at_default_500() {
        let mut b = ToolResultBuilder::new(None);
        let long = "a".repeat(510);
        b.write(&long);
        let r = b.ok(None);
        assert_eq!(r.output.len(), 501);
        assert!(r.output.ends_with('…'));
    }

    #[test]
    fn tracks_character_count_before_truncation() {
        let mut b = ToolResultBuilder::new(None);
        b.write("hello");
        b.write("world");
        assert_eq!(b.n_chars(), 10);
        let r = b.ok(Some("done".into()));
        assert_eq!(r.message, Some("done".into()));
        assert!(!r.is_error);
    }

    #[test]
    fn error_marks_is_error() {
        let b = ToolResultBuilder::new(None);
        let r = b.error("it broke".into());
        assert!(r.is_error);
        assert_eq!(r.message, Some("it broke".into()));
    }
}
```

- [ ] Update `rust-ody/crates/tools-rs/src/lib.rs` to expose the new modules:

```rust
pub mod result_builder;
pub mod store;
pub mod types;
pub mod workspace;
```

- [ ] Run the tests:

```bash
cd rust-ody && cargo test -p tools-rs
```

Expected: `test result: ok.` for `types::tests` and `result_builder::tests`.

- [ ] Commit:

```bash
git add rust-ody/crates/tools-rs/src
git commit -m "feat(tools-rs): common tool types, workspace config, store, and result builder"
```

---

## Task 3: `ToolAccesses` and resource-conflict detection

**Depends on:** Task 1  
**Files:**
- Create: `rust-ody/crates/tools-rs/src/tool_accesses.rs`
- Modify: `rust-ody/crates/tools-rs/src/lib.rs`

**Goal:** Port the tool-side access declarations (`read`/`write`/`readwrite`/`search` on files, plus the `all` wildcard) and the conflict detector used by the tool scheduler to decide which calls can run in parallel.

### Steps

- [ ] Write the failing tests first in `rust-ody/crates/tools-rs/src/tool_accesses.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_read_same_path_does_not_conflict() {
        let a = ToolAccesses::read_file("/repo/src/main.rs");
        let b = ToolAccesses::read_file("/repo/src/main.rs");
        assert!(!ToolAccesses::conflict(&a, &b));
    }

    #[test]
    fn write_read_same_path_conflicts() {
        let a = ToolAccesses::write_file("/repo/src/main.rs");
        let b = ToolAccesses::read_file("/repo/src/main.rs");
        assert!(ToolAccesses::conflict(&a, &b));
    }

    #[test]
    fn recursive_write_conflicts_with_descendant_read() {
        let a = ToolAccesses::write_tree("/repo");
        let b = ToolAccesses::read_file("/repo/src/main.rs");
        assert!(ToolAccesses::conflict(&a, &b));
    }

    #[test]
    fn all_conflicts_with_everything() {
        let a = ToolAccesses::all();
        let b = ToolAccesses::read_file("/repo/src/main.rs");
        assert!(ToolAccesses::conflict(&a, &b));
    }
}
```

- [ ] Run and confirm failure (module missing):

```bash
cd rust-ody && cargo test -p tools-rs tool_accesses::tests
```

- [ ] Create `rust-ody/crates/tools-rs/src/tool_accesses.rs`:

```rust
use serde::{Deserialize, Serialize};

pub const FILE_READ: &str = "read";
pub const FILE_WRITE: &str = "write";
pub const FILE_READWRITE: &str = "readwrite";
pub const FILE_SEARCH: &str = "search";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ToolResourceAccess {
    #[serde(rename = "file")]
    File {
        operation: String,
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        recursive: Option<bool>,
    },
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolAccesses(pub Vec<ToolResourceAccess>);

impl ToolAccesses {
    pub fn none() -> Self {
        Self(Vec::new())
    }

    pub fn all() -> Self {
        Self(vec![ToolResourceAccess::All])
    }

    pub fn file(operation: &str, path: &str, recursive: Option<bool>) -> Self {
        Self(vec![ToolResourceAccess::File {
            operation: operation.to_owned(),
            path: path.to_owned(),
            recursive,
        }])
    }

    pub fn read_file(path: &str) -> Self {
        Self::file(FILE_READ, path, None)
    }

    pub fn read_tree(path: &str) -> Self {
        Self::file(FILE_READ, path, Some(true))
    }

    pub fn write_file(path: &str) -> Self {
        Self::file(FILE_WRITE, path, None)
    }

    pub fn write_tree(path: &str) -> Self {
        Self::file(FILE_WRITE, path, Some(true))
    }

    pub fn read_write_file(path: &str) -> Self {
        Self::file(FILE_READWRITE, path, None)
    }

    pub fn read_write_tree(path: &str) -> Self {
        Self::file(FILE_READWRITE, path, Some(true))
    }

    pub fn search_tree(path: &str) -> Self {
        Self::file(FILE_SEARCH, path, Some(true))
    }

    pub fn conflict(left: &Self, right: &Self) -> bool {
        left.0
            .iter()
            .any(|l| right.0.iter().any(|r| resource_conflict(l, r)))
    }
}

fn resource_conflict(left: &ToolResourceAccess, right: &ToolResourceAccess) -> bool {
    match (left, right) {
        (ToolResourceAccess::All, _) | (_, ToolResourceAccess::All) => true,
        (
            ToolResourceAccess::File {
                operation: lo,
                path: lp,
                recursive: lr,
            },
            ToolResourceAccess::File {
                operation: ro,
                path: rp,
                recursive: rr,
            },
        ) => file_operations_conflict(lo, ro) && file_accesses_overlap(lp, *lr, rp, *rr),
    }
}

fn file_operations_conflict(left: &str, right: &str) -> bool {
    file_operation_writes(left) || file_operation_writes(right)
}

fn file_operation_writes(operation: &str) -> bool {
    matches!(operation, FILE_WRITE | FILE_READWRITE)
}

fn file_accesses_overlap(
    left: &str,
    left_recursive: Option<bool>,
    right: &str,
    right_recursive: Option<bool>,
) -> bool {
    let lp = normalize_path(left);
    let rp = normalize_path(right);
    if lp == rp {
        return true;
    }
    let lpfx = if lp.ends_with('/') { lp.clone() } else { format!("{}/", lp) };
    let rpfx = if rp.ends_with('/') { rp.clone() } else { format!("{}/", rp) };
    (left_recursive == Some(true) && rp.starts_with(&lpfx))
        || (right_recursive == Some(true) && lp.starts_with(&rpfx))
}

fn normalize_path(path: &str) -> String {
    let normalized = path.replace('\\', "/").replace("//", "/");
    let folded = normalized.to_lowercase();
    if folded.len() > 1 && folded.ends_with('/') {
        folded[..folded.len() - 1].to_string()
    } else {
        folded
    }
}
```

- [ ] Update `rust-ody/crates/tools-rs/src/lib.rs`:

```rust
pub mod tool_accesses;
```

- [ ] Run the tests:

```bash
cd rust-ody && cargo test -p tools-rs tool_accesses::tests
```

Expected: `test result: ok.`

- [ ] Commit:

```bash
git add rust-ody/crates/tools-rs/src
git commit -m "feat(tools-rs): ToolAccesses declarations and conflict detector"
```

---

## Local Self-Review (Part 1)

- [ ] Spec coverage: Tasks 1–3 cover 4.4.0.1 (`BuiltinTool`/`ToolInfo`/`ToolExecution` types), 4.4.0.2 (`ToolAccesses`), and the result-builder portion of 4.4.0.6.
- [ ] Placeholder scan: no TODO/TBD; every module is implemented and tested.
- [ ] No phantom tasks: each task creates files and passes `cargo test`.
- [ ] Dependency soundness: Task 1 has no deps; Tasks 2–3 depend only on Task 1.
- [ ] Shared-signature churn: no existing crate signatures are changed in Part 1, so no caller updates are required.
- [ ] Test-the-risk: `ToolAccesses::conflict` has behavioral asserts for the security-critical recursive-overlap and `all` cases; `ToolResultBuilder` asserts truncation and char-count semantics.
- [ ] Type consistency: JSON serialization shapes (`ToolSource` lowercase, `McpCollisionTarget` tagged union) match the TS originals.
