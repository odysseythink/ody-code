# Part 4: Migration — wire 版本比较 + `v1.0→v1.1→v1.2→v1.3` 迁移链

本部分实现 WAL wire 协议的版本比较与逐记录迁移。TS 侧已经存在 `v1.0→v1.1`（tool call 扁平化）、`v1.1→v1.2`（session approval rule 回填）、`v1.2→v1.3`（仅版本号 bump）三条迁移；Rust 侧必须逐条复刻，确保旧 session 文件在 replay 时被静默升级到当前 `1.3` 格式。

---

### Task 1: 版本比较、迁移注册表与 `v1.2→v1.3` 占位迁移

**Depends on:** `schema.md` Task 2 / Task 3

**Files:**
- Create: `rust-ody/crates/agent-rs/src/records/migration.rs`
- Test: `rust-ody/crates/agent-rs/src/records/migration.rs`（内联 `#[cfg(test)]` 模块）

**目标：** 实现 `compare_wire_versions`、`resolve_wire_migrations`、迁移 trait 注册表，以及 identity 的 `v1.2→v1.3` 迁移；让版本解析链能够正确终止。

- [ ] 新建 `rust-ody/crates/agent-rs/src/records/migration.rs`：

```rust
use anyhow::{bail, Result};
use serde_json::Value;

pub const AGENT_WIRE_PROTOCOL_VERSION: &str = "1.3";

pub trait WireMigration: Send + Sync {
    fn source_version(&self) -> &str;
    fn target_version(&self) -> &str;
    fn migrate_record(&self, record: Value) -> Value;
}

pub type WireMigrationRecord = Value;

/// Compare two `major.minor` wire version strings.
/// Returns positive if a > b, negative if a < b, zero if equal.
pub fn compare_wire_versions(a: &str, b: &str) -> Result<i32> {
    let parse = |s: &str| -> Result<Vec<i64>> {
        s.split('.')
            .map(|part| {
                part.parse::<i64>()
                    .map_err(|_| anyhow::anyhow!("invalid wire version segment: {part}"))
            })
            .collect()
    };
    let parts_a = parse(a)?;
    let parts_b = parse(b)?;
    let max_len = parts_a.len().max(parts_b.len());
    for i in 0..max_len {
        let av = parts_a.get(i).copied().unwrap_or(0);
        let bv = parts_b.get(i).copied().unwrap_or(0);
        if av != bv {
            return Ok((av - bv).signum() as i32);
        }
    }
    Ok(0)
}

pub fn is_newer_wire_version(read_version: &str) -> bool {
    compare_wire_versions(read_version, AGENT_WIRE_PROTOCOL_VERSION)
        .map(|c| c > 0)
        .unwrap_or(false)
}

pub fn resolve_wire_migrations(read_version: &str) -> Result<Vec<&'static dyn WireMigration>> {
    if compare_wire_versions(read_version, AGENT_WIRE_PROTOCOL_VERSION)? >= 0 {
        return Ok(Vec::new());
    }

    let mut migrations: Vec<&'static dyn WireMigration> = Vec::new();
    let mut version = read_version.to_string();
    while compare_wire_versions(&version, AGENT_WIRE_PROTOCOL_VERSION)? < 0 {
        let migration = find_migration(&version)
            .ok_or_else(|| anyhow::anyhow!("Missing wire migration for version {version}"))?;
        migrations.push(migration);
        version = migration.target_version().to_string();
    }
    Ok(migrations)
}

pub fn migrate_wire_record(
    record: WireMigrationRecord,
    migrations: &[&'static dyn WireMigration],
) -> WireMigrationRecord {
    migrations.iter().fold(record, |current, migration| migration.migrate_record(current))
}

fn find_migration(source_version: &str) -> Option<&'static dyn WireMigration> {
    static MIGRATIONS: &[&dyn WireMigration] = &[
        &V1_2ToV1_3,
        &V1_1ToV1_2,
        &V1_0ToV1_1,
    ];
    for migration in MIGRATIONS {
        if migration.source_version() == source_version {
            return Some(*migration);
        }
    }
    None
}

struct V1_2ToV1_3;

impl WireMigration for V1_2ToV1_3 {
    fn source_version(&self) -> &str {
        "1.2"
    }

    fn target_version(&self) -> &str {
        "1.3"
    }

    fn migrate_record(&self, record: Value) -> Value {
        // v1.3 only changes how BlobStore reads/writes large media URLs.
        // The wire JSON itself requires no transformation.
        record
    }
}
```

- [ ] 在同文件底部追加版本解析与注册表测试：

```rust
#[cfg(test)]
mod version_tests {
    use super::*;

    #[test]
    fn compare_versions_orders_correctly() {
        assert!(compare_wire_versions("1.2", "1.3").unwrap() < 0);
        assert!(compare_wire_versions("1.3", "1.2").unwrap() > 0);
        assert_eq!(compare_wire_versions("1.3", "1.3").unwrap(), 0);
        assert!(compare_wire_versions("1.10", "1.3").unwrap() > 0);
    }

    #[test]
    fn is_newer_detects_future_version() {
        assert!(is_newer_wire_version("1.4"));
        assert!(!is_newer_wire_version("1.3"));
        assert!(!is_newer_wire_version("1.2"));
    }

    #[test]
    fn resolve_migrations_chains_to_current() {
        let migrations = resolve_wire_migrations("1.0").unwrap();
        let versions: Vec<&str> = migrations
            .iter()
            .map(|m| format!("{}->{}" , m.source_version(), m.target_version()))
            .collect();
        assert_eq!(versions, vec!["1.0->1.1", "1.1->1.2", "1.2->1.3"]);
    }

    #[test]
    fn resolve_migrations_empty_at_current() {
        assert!(resolve_wire_migrations("1.3").unwrap().is_empty());
    }

    #[test]
    fn migrate_wire_record_identity_for_v1_2_to_v1_3() {
        let record = serde_json::json!({"type":"metadata","protocol_version":"1.2"});
        let migrations = resolve_wire_migrations("1.2").unwrap();
        let migrated = migrate_wire_record(record.clone(), &migrations);
        assert_eq!(migrated, record);
    }
}
```

- [ ] 运行测试，确认通过：

```bash
cd rust-ody && cargo test -p agent-rs version_ --lib
```

预期输出：`test result: ok. 4 passed; 0 failed`。

- [ ] Commit：`feat(agent-rs): wire version comparison and migration registry`

---

### Task 2: `v1.0→v1.1` tool call 扁平化迁移

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/records/migration.rs`
- Test: `rust-ody/crates/agent-rs/src/records/migration.rs`（追加内联测试）

**目标：** 把旧格式 `message.toolCalls[].function: { name, arguments }` 扁平化为 `message.toolCalls[].name` / `.arguments`。

- [ ] 在 `migration.rs` 中加入 `V1_0ToV1_1` 迁移实现：

```rust
struct V1_0ToV1_1;

impl WireMigration for V1_0ToV1_1 {
    fn source_version(&self) -> &str {
        "1.0"
    }

    fn target_version(&self) -> &str {
        "1.1"
    }

    fn migrate_record(&self, record: Value) -> Value {
        let mut record = record;
        if record.get("type").and_then(|v| v.as_str()) != Some("context.append_message") {
            return record;
        }

        let Some(message) = record.get_mut("message") else {
            return record;
        };
        let Some(tool_calls) = message.get_mut("toolCalls").and_then(|v| v.as_array_mut()) else {
            return record;
        };

        for tool_call in tool_calls.iter_mut() {
            let Some(obj) = tool_call.as_object_mut() else { continue };
            if let Some(function) = obj.remove("function") {
                if let Some(function_obj) = function.as_object() {
                    if let Some(name) = function_obj.get("name").cloned() {
                        obj.insert("name".to_string(), name);
                    }
                    if let Some(arguments) = function_obj.get("arguments").cloned() {
                        obj.insert("arguments".to_string(), arguments);
                    }
                }
            }
        }

        record
    }
}
```

- [ ] 追加迁移测试：

```rust
#[cfg(test)]
mod v1_0_to_v1_1_tests {
    use super::*;

    #[test]
    fn flattens_nested_function_tool_call() {
        let record = serde_json::json!({
            "type": "context.append_message",
            "time": 1,
            "message": {
                "role": "assistant",
                "content": [],
                "toolCalls": [
                    {
                        "type": "function",
                        "id": "call_1",
                        "function": {
                            "name": "read",
                            "arguments": "{\"path\":\"x\"}"
                        }
                    }
                ]
            }
        });

        let migrated = migrate_v1_0_to_v1_1(record);
        let tool_call = &migrated["message"]["toolCalls"][0];
        assert_eq!(tool_call["name"], "read");
        assert_eq!(tool_call["arguments"], "{\"path\":\"x\"}");
        assert!(!tool_call.as_object().unwrap().contains_key("function"));
    }

    #[test]
    fn leaves_non_append_message_records_unchanged() {
        let record = serde_json::json!({"type":"metadata","protocol_version":"1.0"});
        let migrated = migrate_v1_0_to_v1_1(record.clone());
        assert_eq!(migrated, record);
    }

    fn migrate_v1_0_to_v1_1(record: Value) -> Value {
        let migration = V1_0ToV1_1;
        migration.migrate_record(record)
    }
}
```

> 注意：测试里的 `migrate_v1_0_to_v1_1` 辅助函数放在同一 `#[cfg(test)]` 模块内；`V1_0ToV1_1` 是 private struct，测试可直接访问。

- [ ] 运行测试，确认通过：

```bash
cd rust-ody && cargo test -p agent-rs v1_0_to_v1_1 --lib
```

预期输出：`test result: ok. 2 passed; 0 failed`。

- [ ] Commit：`feat(agent-rs): implement v1.0 to v1.1 tool-call flattening migration`

---

### Task 3: `v1.1→v1.2` session approval rule 回填迁移

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/agent-rs/Cargo.toml`（追加 `lazy_static`）
- Modify: `rust-ody/crates/agent-rs/src/records/migration.rs`
- Test: `rust-ody/crates/agent-rs/src/records/migration.rs`（追加内联测试）

**目标：** 对 `permission.record_approval_result` 中 decision=approved、scope=session、且缺少 `sessionApprovalRule` 的记录，根据旧 action 标签回填规则 pattern；无法安全还原的动作则跳过。

- [ ] 在 `rust-ody/crates/agent-rs/Cargo.toml` 追加依赖：

```toml
[dependencies]
lazy_static = "1"
# ... existing entries preserved ...
```

- [ ] 在 `migration.rs` 中加入 `V1_1ToV1_2` 迁移实现：

```rust
struct V1_1ToV1_2;

impl WireMigration for V1_1ToV1_2 {
    fn source_version(&self) -> &str {
        "1.1"
    }

    fn target_version(&self) -> &str {
        "1.2"
    }

    fn migrate_record(&self, record: Value) -> Value {
        let mut record = record;
        if record.get("type").and_then(|v| v.as_str())
            != Some("permission.record_approval_result")
        {
            return record;
        }

        let Some(result) = record.get("result") else {
            return record;
        };
        if result.get("decision").and_then(|v| v.as_str()) != Some("approved") {
            return record;
        }
        if result.get("scope").and_then(|v| v.as_str()) != Some("session") {
            return record;
        }
        if record.get("sessionApprovalRule").is_some() {
            return record;
        }

        let action = record
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tool_name = record
            .get("toolName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let pattern = if UNRESTORABLE_ACTIONS.contains(action.as_str()) {
            None
        } else {
            ACTION_TO_PATTERN.get(action.as_str()).copied().or(Some(tool_name.as_str()))
        };

        if let Some(pattern) = pattern {
            if let Some(obj) = record.as_object_mut() {
                obj.insert("sessionApprovalRule".to_string(), Value::String(pattern.to_string()));
            }
        }

        record
    }
}

use std::collections::{HashMap, HashSet};

lazy_static::lazy_static! {
    static ref ACTION_TO_PATTERN: HashMap<&'static str, &'static str> = {
        let mut map = HashMap::new();
        map.insert("run command", "Bash");
        map.insert("stop background task", "TaskStop");
        map.insert("edit file", "Write");
        map.insert("edit file outside of working directory", "Write");
        map.insert("write file", "Write");
        map
    };

    static ref UNRESTORABLE_ACTIONS: HashSet<&'static str> = {
        let mut set = HashSet::new();
        set.insert("run command in plan mode");
        set.insert("run background command");
        set
    };
}
```

- [ ] 追加迁移测试：

```rust
#[cfg(test)]
mod v1_1_to_v1_2_tests {
    use super::*;

    fn make_approval_record(action: &str, session_rule: Option<&str>) -> Value {
        let mut record = serde_json::json!({
            "type": "permission.record_approval_result",
            "turnId": 1,
            "toolCallId": "tc1",
            "toolName": "Bash",
            "action": action,
            "result": {
                "decision": "approved",
                "scope": "session"
            }
        });
        if let Some(rule) = session_rule {
            record["sessionApprovalRule"] = Value::String(rule.to_string());
        }
        record
    }

    #[test]
    fn backfills_session_approval_rule_for_known_action() {
        let record = make_approval_record("run command", None);
        let migrated = V1_1ToV1_2.migrate_record(record);
        assert_eq!(migrated["sessionApprovalRule"], "Bash");
    }

    #[test]
    fn falls_back_to_tool_name_for_unknown_action() {
        let record = make_approval_record("custom action", None);
        let migrated = V1_1ToV1_2.migrate_record(record);
        assert_eq!(migrated["sessionApprovalRule"], "Bash");
    }

    #[test]
    fn skips_unrestorable_action() {
        let record = make_approval_record("run background command", None);
        let migrated = V1_1ToV1_2.migrate_record(record);
        assert!(migrated.get("sessionApprovalRule").is_none());
    }

    #[test]
    fn skips_when_scope_is_not_session() {
        let mut record = make_approval_record("run command", None);
        record["result"]["scope"] = "turn".into();
        let migrated = V1_1ToV1_2.migrate_record(record);
        assert!(migrated.get("sessionApprovalRule").is_none());
    }

    #[test]
    fn skips_when_already_has_rule() {
        let record = make_approval_record("run command", Some("ExistingRule"));
        let migrated = V1_1ToV1_2.migrate_record(record);
        assert_eq!(migrated["sessionApprovalRule"], "ExistingRule");
    }
}
```

- [ ] 运行测试，确认通过：

```bash
cd rust-ody && cargo test -p agent-rs v1_1_to_v1_2 --lib
```

预期输出：`test result: ok. 5 passed; 0 failed`。

- [ ] 运行整树类型检查（含测试）：

```bash
cd rust-ody && cargo check -p agent-rs --workspace --tests
```

预期输出：`Finished dev [unoptimized + debuginfo] target(s)`，无错误。

- [ ] Commit：`feat(agent-rs): implement v1.1 to v1.2 session approval rule backfill migration`

---

## Local Self-Review

- [ ] 1. Spec-coverage：本部分覆盖 Roadmap 4.3.0.6（wire migration 链 Rust 化）。
- [ ] 2. Placeholder scan：无 TODO/TBD；三条迁移均给出完整实现与覆盖表。
- [ ] 3. No phantom tasks：Task 1 产出版本比较与注册表；Task 2 产出 v1.0→v1.1；Task 3 产出 v1.1→v1.2。每个任务都有独立测试。
- [ ] 4. Dependency soundness：Task 1 无额外依赖；Task 2/3 依赖 Task 1。无反向依赖。
- [ ] 5. Caller & build soundness：本部分仅新增代码，未修改已有共享签名；结束时运行 `cargo check -p agent-rs --workspace --tests`。
- [ ] 6. Test-the-risk：`resolve_migrations_chains_to_current` 验证从 1.0 到 1.3 的完整链路；`flattens_nested_function_tool_call` 验证中间 tool call 字段被删除且新字段存在；`backfills_session_approval_rule_for_known_action` 与 `skips_unrestorable_action` 覆盖规则回填与跳过边界。
- [ ] 7. Type consistency：`WireMigration` trait 的 `migrate_record` 使用 `serde_json::Value`；`AGENT_WIRE_PROTOCOL_VERSION` 与 TS 一致为 `"1.3"`；迁移链顺序与 TS `MIGRATIONS` 数组一致。
