# Part 3: Tool types + `ToolManager` + `ToolManagerContext`

本部分迁移 `packages/agent-core/src/agent/tool/*`，把 Agent 的工具面抽象成可独立测试的 Rust 模块。关键决策：MCP 相关能力在 4.3.2 只保留接口形状与本地碰撞检测；`initialize_builtin_tools` 只实现无外部依赖的核心 builtin（Read/Write/Edit/Glob/Grep/Bash），其余在 4.3.9 补齐。

---

### Task 1: 定义 tool 类型

**Depends on:** 4.3.0 records 层（`UserToolRegistration`、`ToolStoreUpdate` 已定义）

**Files:**
- Create: `rust-ody/crates/agent-rs/src/tool/mod.rs`
- Create: `rust-ody/crates/agent-rs/src/tool/types.rs`
- Modify: `rust-ody/crates/agent-rs/src/lib.rs`
- Test: `rust-ody/crates/agent-rs/src/tool/types.rs`（内联 `#[cfg(test)]`）

**目标：** 定义 `ToolSource`、`ToolInfo`、`UserToolRegistration`（复用 records 版本）、`McpToolCollision`、`McpServerRegistrationResult`、`ExecutableTool`，序列化字段名与 TS 一致。

- [ ] 新建 `rust-ody/crates/agent-rs/src/tool/mod.rs`：

```rust
pub mod manager;
pub mod types;

pub use manager::{ToolManager, ToolManagerContext};
pub use types::*;
```

- [ ] 新建 `rust-ody/crates/agent-rs/src/tool/types.rs`：

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// Re-export the records-layer payload so ToolManager and the WAL use the same type.
pub use crate::records::nested::UserToolRegistration;

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
pub struct McpToolCollision {
    pub qualified: String,
    pub tool_name: String,
    pub collides_with: McpCollisionTarget,
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
pub struct McpServerRegistrationResult {
    pub registered: Vec<String>,
    pub collisions: Vec<McpToolCollision>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutableTool {
    pub name: String,
    pub description: String,
    pub parameters: JsonValue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_source_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&ToolSource::Builtin).unwrap(),
            "\"builtin\""
        );
        assert_eq!(
            serde_json::to_string(&ToolSource::Mcp).unwrap(),
            "\"mcp\""
        );
    }

    #[test]
    fn mcp_collision_uses_camel_case_and_tag() {
        let collision = McpToolCollision {
            qualified: "mcp__a__b".into(),
            tool_name: "b".into(),
            collides_with: McpCollisionTarget::OtherServer {
                server_name: "x".into(),
            },
        };
        let json = serde_json::to_string(&collision).unwrap();
        assert!(json.contains("\"toolName\""));
        assert!(json.contains("\"kind\":\"other_server\""));
        assert!(json.contains("\"serverName\""));

        let round: McpToolCollision = serde_json::from_str(&json).unwrap();
        assert_eq!(round, collision);
    }
}
```

- [ ] 修改 `rust-ody/crates/agent-rs/src/lib.rs`，加入 `tool` 模块导出：

```rust
pub mod config;
pub mod records;
pub mod tool;
pub mod usage;

pub use records::*;
```

- [ ] 运行类型检查：

```bash
cd rust-ody && cargo check -p agent-rs --workspace --tests
```

预期输出：无错误，`Finished dev [unoptimized + debuginfo] target(s)`。

- [ ] 运行 tool 类型单元测试：

```bash
cd rust-ody && cargo test -p agent-rs --lib tool::types
```

预期输出：`test result: ok. 2 passed; 0 failed`。

- [ ] Commit：`feat(agent-rs): add tool types aligned with TS`

---

### Task 2: `ToolManagerContext` trait + `ToolManager` 骨架（user tool、active、store）

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/agent-rs/src/tool/manager.rs`
- Modify: `rust-ody/crates/agent-rs/src/tool/mod.rs`
- Test: `rust-ody/crates/agent-rs/tests/tool_manager.rs`

**目标：** 定义 `ToolManagerContext` trait，实现 `ToolManager` 的构造、user tool 注册/注销、active tools、store 读写，并通过 WAL 记录变更。

- [ ] 新建 `rust-ody/crates/agent-rs/src/tool/manager.rs`（初始骨架）：

```rust
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use serde_json::Value as JsonValue;

use crate::records::nested::ToolStoreUpdate;
use crate::records::AgentRecord;
use super::types::{ExecutableTool, ToolInfo, ToolSource, UserToolRegistration};

/// Minimal Agent surface required by `ToolManager`.
pub trait ToolManagerContext: Send + Sync {
    fn log_record(&mut self, record: AgentRecord);
    fn emit_tool_list_updated(&mut self, reason: &str, server_name: Option<&str>);
    fn goal_mutation_tools_hidden(&self) -> bool;
}

struct McpToolEntry {
    tool: ExecutableTool,
    server_name: String,
}

pub struct ToolManager<C: ToolManagerContext> {
    context: C,
    builtin_tools: HashMap<String, ExecutableTool>,
    user_tools: HashMap<String, ExecutableTool>,
    mcp_tools: HashMap<String, McpToolEntry>,
    mcp_tools_by_server: HashMap<String, Vec<String>>,
    enabled_tools: HashSet<String>,
    mcp_access_patterns: Vec<String>,
    store: HashMap<String, JsonValue>,
}

impl<C: ToolManagerContext> ToolManager<C> {
    pub fn new(context: C) -> Self {
        Self {
            context,
            builtin_tools: HashMap::new(),
            user_tools: HashMap::new(),
            mcp_tools: HashMap::new(),
            mcp_tools_by_server: HashMap::new(),
            enabled_tools: HashSet::new(),
            mcp_access_patterns: Vec::new(),
            store: HashMap::new(),
        }
    }

    pub fn register_user_tool(&mut self, input: UserToolRegistration) {
        self.context.log_record(AgentRecord::ToolsRegisterUserTool {
            time: None,
            registration: input.clone(),
        });
        let tool = ExecutableTool {
            name: input.name.clone(),
            description: input.description.clone(),
            parameters: input.parameters.clone(),
        };
        self.user_tools.insert(input.name.clone(), tool);
        self.enabled_tools.insert(input.name);
    }

    pub fn unregister_user_tool(&mut self, name: &str) {
        self.context.log_record(AgentRecord::ToolsUnregisterUserTool {
            time: None,
            name: name.to_owned(),
        });
        self.user_tools.remove(name);
        self.enabled_tools.remove(name);
    }

    pub fn inherit_user_tools(&mut self, parent: &ToolManager<C>) {
        for tool in parent.user_tools.values() {
            if !parent.enabled_tools.contains(&tool.name) {
                continue;
            }
            self.register_user_tool(UserToolRegistration {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.parameters.clone(),
            });
        }
    }

    pub fn set_active_tools(&mut self, names: &[String]) {
        self.context.log_record(AgentRecord::ToolsSetActiveTools {
            time: None,
            names: names.to_vec(),
        });
        self.enabled_tools = names.iter().filter(|n| !is_mcp_pattern(n)).cloned().collect();
        self.mcp_access_patterns = names.iter().filter(|n| is_mcp_pattern(n)).cloned().collect();
    }

    pub fn is_tool_active(&self, name: &str) -> bool {
        self.enabled_tools.contains(name)
            && (self.builtin_tools.contains_key(name) || self.user_tools.contains_key(name))
    }

    pub fn data(&self) -> Vec<ToolInfo> {
        let mut infos: Vec<ToolInfo> = self
            .builtin_tools
            .values()
            .map(|t| ToolInfo {
                name: t.name.clone(),
                description: t.description.clone(),
                active: self.enabled_tools.contains(&t.name),
                source: ToolSource::Builtin,
            })
            .chain(self.user_tools.values().map(|t| ToolInfo {
                name: t.name.clone(),
                description: t.description.clone(),
                active: self.enabled_tools.contains(&t.name),
                source: ToolSource::User,
            }))
            .collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos
    }

    pub fn store_data(&self) -> HashMap<String, JsonValue> {
        self.store.clone()
    }

    pub fn update_store(&mut self, key: &str, value: JsonValue) {
        self.context.log_record(AgentRecord::ToolsUpdateStore {
            time: None,
            update: ToolStoreUpdate {
                key: key.to_owned(),
                value: value.clone(),
            },
        });
        self.store.insert(key.to_owned(), value);
    }

    pub fn into_inner(self) -> C {
        self.context
    }
}

fn is_mcp_pattern(name: &str) -> bool {
    name.starts_with("mcp__")
}
```

- [ ] 修改 `rust-ody/crates/agent-rs/src/tool/mod.rs` 导出 `manager`：

```rust
pub mod manager;
pub mod types;

pub use manager::{ToolManager, ToolManagerContext};
pub use types::*;
```

- [ ] 新建 `rust-ody/crates/agent-rs/tests/tool_manager.rs`（Task 2 先写失败测试）：

```rust
use std::sync::Mutex;

use agent_rs::records::AgentRecord;
use agent_rs::tool::{ToolManager, ToolManagerContext, ToolSource, UserToolRegistration};
use serde_json::json;

#[derive(Debug, Default)]
struct MockCtx {
    records: Mutex<Vec<AgentRecord>>,
    events: Mutex<Vec<(String, Option<String>)>>,
    hide_goal: bool,
}

impl ToolManagerContext for MockCtx {
    fn log_record(&mut self, record: AgentRecord) {
        self.records.lock().unwrap().push(record);
    }

    fn emit_tool_list_updated(&mut self, reason: &str, server_name: Option<&str>) {
        self.events
            .lock()
            .unwrap()
            .push((reason.to_string(), server_name.map(|s| s.to_string())));
    }

    fn goal_mutation_tools_hidden(&self) -> bool {
        self.hide_goal
    }
}

#[test]
fn register_user_tool_logs_and_enables() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.register_user_tool(UserToolRegistration {
        name: "my_tool".into(),
        description: "does a thing".into(),
        parameters: json!({"type": "object"}),
    });

    assert!(mgr.is_tool_active("my_tool"));
    let ctx = mgr.into_inner();
    let records = ctx.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    match &records[0] {
        AgentRecord::ToolsRegisterUserTool { registration, .. } => {
            assert_eq!(registration.name, "my_tool");
        }
        _ => panic!("expected tools.register_user_tool record"),
    }
}

#[test]
fn unregister_user_tool_logs_and_removes() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.register_user_tool(UserToolRegistration {
        name: "my_tool".into(),
        description: "".into(),
        parameters: json!({}),
    });
    mgr.unregister_user_tool("my_tool");

    assert!(!mgr.is_tool_active("my_tool"));
    let ctx = mgr.into_inner();
    assert_eq!(ctx.records.lock().unwrap().len(), 2);
}

#[test]
fn set_active_tools_splits_exact_and_mcp_patterns() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.set_active_tools(&[
        "Read".into(),
        "my_tool".into(),
        "mcp__*".into(),
        "mcp__github__*".into(),
    ]);

    assert!(mgr.is_tool_active("Read") == false); // builtin not registered yet
    let data = mgr.data();
    assert!(data.iter().all(|i| i.source != ToolSource::Mcp));
    let ctx = mgr.into_inner();
    let records = ctx.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    match &records[0] {
        AgentRecord::ToolsSetActiveTools { names, .. } => {
            assert_eq!(names.len(), 4);
        }
        _ => panic!("expected tools.set_active_tools record"),
    }
}

#[test]
fn update_store_logs_and_retains_value() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.update_store("foo", json!({"bar": 1}));
    assert_eq!(mgr.store_data().get("foo").unwrap(), &json!({"bar": 1}));
    let ctx = mgr.into_inner();
    assert_eq!(ctx.records.lock().unwrap().len(), 1);
}
```

- [ ] 运行测试，确认失败：

```bash
cd rust-ody && cargo test -p agent-rs --test tool_manager
```

预期失败：`error[E0433]: failed to resolve: use of undeclared crate or module 'manager'`（因为 `manager.rs` 尚未创建或 `mod.rs` 未导出）。

- [ ] 完成实现并再次运行：

```bash
cd rust-ody && cargo test -p agent-rs --test tool_manager
```

预期输出：`test result: ok. 4 passed; 0 failed`。

- [ ] Commit：`feat(agent-rs): add ToolManager skeleton with user tools and store`

---

### Task 3: 核心 builtin 初始化与 `loop_tools` 排序/过滤

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/tool/manager.rs`
- Modify: `rust-ody/crates/agent-rs/tests/tool_manager.rs`

**目标：** 实现 `initialize_builtin_tools`（仅核心 6 个 builtin），以及 `loop_tools` 的排序与 goal 突变工具隐藏逻辑，与 TS `ToolManager.loopTools` 一致。

- [ ] 在 `rust-ody/crates/agent-rs/src/tool/manager.rs` 的 `impl<C: ToolManagerContext> ToolManager<C>` 内追加：

```rust
    pub fn initialize_builtin_tools(&mut self) {
        self.builtin_tools = core_builtin_tools()
            .into_iter()
            .map(|t| (t.name.clone(), t))
            .collect();
    }

    pub fn loop_tools(&self) -> Vec<&ExecutableTool> {
        let mut names: Vec<String> = self.enabled_tools.iter().cloned().collect();
        let mcp_names: Vec<String> = self
            .mcp_tools
            .keys()
            .filter(|name| self.is_mcp_tool_enabled(name))
            .cloned()
            .collect();
        names.extend(mcp_names);
        names.sort_unstable();
        names.dedup();

        if self.context.goal_mutation_tools_hidden() {
            names.retain(|name| name != "SetGoalBudget" && name != "UpdateGoal");
        }

        names
            .iter()
            .filter_map(|name| {
                self.user_tools
                    .get(name)
                    .or_else(|| self.mcp_tools.get(name).map(|entry| &entry.tool))
                    .or_else(|| self.builtin_tools.get(name))
            })
            .collect()
    }

    fn is_mcp_tool_enabled(&self, name: &str) -> bool {
        self.mcp_access_patterns
            .iter()
            .any(|pattern| matches_mcp_pattern(name, pattern))
    }
```

- [ ] 在同一文件底部追加核心 builtin 列表与匹配辅助函数：

```rust
fn core_builtin_tools() -> Vec<ExecutableTool> {
    vec![
        ExecutableTool {
            name: "Read".into(),
            description: "Read a text file from the local filesystem.".into(),
            parameters: json_schema_object(&["path"]),
        },
        ExecutableTool {
            name: "Write".into(),
            description: "Write or overwrite a text file.".into(),
            parameters: json_schema_object(&["path", "content"]),
        },
        ExecutableTool {
            name: "Edit".into(),
            description: "Apply a targeted edit to a text file.".into(),
            parameters: json_schema_object(&["path", "old_string", "new_string"]),
        },
        ExecutableTool {
            name: "Glob".into(),
            description: "Find files matching a glob pattern.".into(),
            parameters: json_schema_object(&["pattern"]),
        },
        ExecutableTool {
            name: "Grep".into(),
            description: "Search file contents with a regex.".into(),
            parameters: json_schema_object(&["pattern", "path"]),
        },
        ExecutableTool {
            name: "Bash".into(),
            description: "Execute a shell command.".into(),
            parameters: json_schema_object(&["command"]),
        },
    ]
}

fn json_schema_object(required: &[&str]) -> JsonValue {
    serde_json::json!({
        "type": "object",
        "required": required,
        "properties": {}
    })
}

fn matches_mcp_pattern(name: &str, pattern: &str) -> bool {
    if pattern == name {
        return true;
    }
    if pattern.ends_with('*') && name.starts_with(&pattern[..pattern.len() - 1]) {
        return true;
    }
    false
}
```

注意：需要在 `manager.rs` 顶部引入 `serde_json::Value as JsonValue`（已有）和 `ExecutableTool`（已有）。`matches_mcp_pattern` 是 4.3.2 的简化匹配器；4.3.9 将替换为 picomatch 兼容实现。

- [ ] 在 `rust-ody/crates/agent-rs/tests/tool_manager.rs` 中追加：

```rust
#[test]
fn initialize_builtin_tools_populates_core_tools() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.initialize_builtin_tools();
    let data = mgr.data();
    let names: Vec<_> = data.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"Read"));
    assert!(names.contains(&"Write"));
    assert!(names.contains(&"Edit"));
    assert!(names.contains(&"Glob"));
    assert!(names.contains(&"Grep"));
    assert!(names.contains(&"Bash"));
}

#[test]
fn loop_tools_sorted_and_includes_active_builtin() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.initialize_builtin_tools();
    mgr.set_active_tools(&["Write".into(), "Read".into(), "Grep".into()]);
    let tools = mgr.loop_tools();
    let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["Grep", "Read", "Write"]);
}

#[test]
fn loop_tools_hides_goal_mutation_tools_when_no_goal() {
    let mut ctx = MockCtx::default();
    ctx.hide_goal = true;
    let mut mgr = ToolManager::new(ctx);
    mgr.register_user_tool(UserToolRegistration {
        name: "SetGoalBudget".into(),
        description: "".into(),
        parameters: json!({}),
    });
    mgr.register_user_tool(UserToolRegistration {
        name: "UpdateGoal".into(),
        description: "".into(),
        parameters: json!({}),
    });
    mgr.register_user_tool(UserToolRegistration {
        name: "Read".into(),
        description: "".into(),
        parameters: json!({}),
    });
    mgr.set_active_tools(&[
        "SetGoalBudget".into(),
        "UpdateGoal".into(),
        "Read".into(),
    ]);
    let names: Vec<_> = mgr.loop_tools().iter().map(|t| t.name.clone()).collect();
    assert_eq!(names, vec!["Read"]);
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p agent-rs --test tool_manager
```

预期输出：`test result: ok. 7 passed; 0 failed`。

- [ ] Commit：`feat(agent-rs): add core builtin tools and loop_tools sorting/filtering`

---

### Task 4: MCP 注册/注销接口桩与碰撞检测

**Depends on:** Task 3

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/tool/manager.rs`
- Modify: `rust-ody/crates/agent-rs/tests/tool_manager.rs`

**目标：** 保留 `register_mcp_server` / `unregister_mcp_server` 的接口形状与事件，实现同 server / 跨 server 的工具名碰撞检测，使 `loop_tools` 能根据 `mcp_access_patterns` 暴露 MCP 工具。

- [ ] 在 `rust-ody/crates/agent-rs/src/tool/manager.rs` 中追加方法（放在 `impl<C: ToolManagerContext> ToolManager<C>` 内）：

```rust
    pub fn register_mcp_server(
        &mut self,
        server_name: &str,
        tools: &[kosong_rs::provider::Tool],
        enabled_tools: Option<&HashSet<String>>,
    ) -> McpServerRegistrationResult {
        self.unregister_mcp_server(server_name);

        let mut registered: Vec<String> = Vec::new();
        let mut collisions: Vec<McpToolCollision> = Vec::new();
        let mut seen_in_this_call: HashMap<String, String> = HashMap::new();

        for tool in tools {
            if let Some(enabled) = enabled_tools {
                if !enabled.contains(&tool.name) {
                    continue;
                }
            }

            let qualified = qualify_mcp_tool_name(server_name, &tool.name);

            if let Some(first_name) = seen_in_this_call.get(&qualified) {
                collisions.push(McpToolCollision {
                    qualified: qualified.clone(),
                    tool_name: tool.name.clone(),
                    collides_with: super::types::McpCollisionTarget::SameServer {
                        tool_name: first_name.clone(),
                    },
                });
                continue;
            }

            if let Some(existing) = self.mcp_tools.get(&qualified) {
                collisions.push(McpToolCollision {
                    qualified: qualified.clone(),
                    tool_name: tool.name.clone(),
                    collides_with: super::types::McpCollisionTarget::OtherServer {
                        server_name: existing.server_name.clone(),
                    },
                });
                continue;
            }

            seen_in_this_call.insert(qualified.clone(), tool.name.clone());
            let wrapped = ExecutableTool {
                name: qualified.clone(),
                description: tool.description.clone(),
                parameters: tool.parameters.clone(),
            };
            self.mcp_tools.insert(
                qualified.clone(),
                McpToolEntry {
                    tool: wrapped,
                    server_name: server_name.to_owned(),
                },
            );
            registered.push(qualified);
        }

        self.mcp_tools_by_server
            .insert(server_name.to_owned(), registered.clone());

        McpServerRegistrationResult {
            registered,
            collisions,
        }
    }

    pub fn unregister_mcp_server(&mut self, server_name: &str) -> bool {
        let Some(existing) = self.mcp_tools_by_server.remove(server_name) else {
            return false;
        };
        for qualified in existing {
            self.mcp_tools.remove(&qualified);
        }
        true
    }
```

- [ ] 在同一文件底部追加辅助函数：

```rust
fn qualify_mcp_tool_name(server_name: &str, tool_name: &str) -> String {
    format!("mcp__{}__{}", server_name, tool_name)
}
```

- [ ] 修改 `data()` 方法，使其同时产出 MCP 工具信息。将 Task 2 中的 `data()` 替换为：

```rust
    pub fn data(&self) -> Vec<ToolInfo> {
        let mut infos: Vec<ToolInfo> = self
            .builtin_tools
            .values()
            .map(|t| ToolInfo {
                name: t.name.clone(),
                description: t.description.clone(),
                active: self.enabled_tools.contains(&t.name),
                source: ToolSource::Builtin,
            })
            .chain(self.user_tools.values().map(|t| ToolInfo {
                name: t.name.clone(),
                description: t.description.clone(),
                active: self.enabled_tools.contains(&t.name),
                source: ToolSource::User,
            }))
            .chain(self.mcp_tools.values().map(|entry| ToolInfo {
                name: entry.tool.name.clone(),
                description: entry.tool.description.clone(),
                active: self.is_mcp_tool_enabled(&entry.tool.name),
                source: ToolSource::Mcp,
            }))
            .collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos
    }
```

- [ ] 在 `rust-ody/crates/agent-rs/tests/tool_manager.rs` 中追加：

```rust
use kosong_rs::provider::Tool as KosongTool;

#[test]
fn register_mcp_server_qualifies_names_and_respects_enabled_filter() {
    let mut mgr = ToolManager::new(MockCtx::default());
    let result = mgr.register_mcp_server(
        "github",
        &[
            KosongTool {
                name: "list_repos".into(),
                description: "".into(),
                parameters: json!({}),
            },
            KosongTool {
                name: "create_issue".into(),
                description: "".into(),
                parameters: json!({}),
            },
        ],
        Some(&{
            let mut set = std::collections::HashSet::new();
            set.insert("list_repos".into());
            set
        }),
    );

    assert_eq!(result.registered, vec!["mcp__github__list_repos"]);
    assert!(result.collisions.is_empty());

    mgr.set_active_tools(&["mcp__github__*".into()]);
    let tools = mgr.loop_tools();
    let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["mcp__github__list_repos"]);
}

#[test]
fn mcp_collisions_detected_within_and_across_servers() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.register_mcp_server(
        "github",
        &[KosongTool {
            name: "list".into(),
            description: "".into(),
            parameters: json!({}),
        }],
        None,
    );

    let result = mgr.register_mcp_server(
        "github",
        &[
            KosongTool {
                name: "list".into(),
                description: "".into(),
                parameters: json!({}),
            },
            KosongTool {
                name: "list".into(),
                description: "".into(),
                parameters: json!({}),
            },
        ],
        None,
    );

    assert_eq!(result.collisions.len(), 2);
    assert!(result
        .collisions
        .iter()
        .any(|c| matches!(c.collides_with, agent_rs::tool::McpCollisionTarget::SameServer { .. })));
    assert!(result.collisions.iter().any(|c| matches!(
        c.collides_with,
        agent_rs::tool::McpCollisionTarget::OtherServer { .. }
    )));
}

#[test]
fn unregister_mcp_server_removes_tools() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.register_mcp_server(
        "github",
        &[KosongTool {
            name: "list".into(),
            description: "".into(),
            parameters: json!({}),
        }],
        None,
    );
    mgr.set_active_tools(&["mcp__github__*".into()]);
    assert_eq!(mgr.loop_tools().len(), 1);

    mgr.unregister_mcp_server("github");
    assert_eq!(mgr.loop_tools().len(), 0);
}
```

注意：需要在测试文件顶部 `use agent_rs::tool::{...}` 中加入 `McpCollisionTarget` 或直接用路径引用。

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p agent-rs --test tool_manager
```

预期输出：`test result: ok. 10 passed; 0 failed`。

- [ ] Commit：`feat(agent-rs): add MCP registration stubs with collision detection`

---

### Task 5: 生成 `ToolManager` fixture 与 round-trip 对照

**Depends on:** Task 4

**Files:**
- Create: `rust-ody/crates/agent-rs/src/bin/generate_tool_fixture.rs`
- Create: `rust-ody/crates/agent-rs/tests/fixtures/tools-rust.json`
- Modify: `rust-ody/crates/agent-rs/Cargo.toml`
- Test: `rust-ody/crates/agent-rs/tests/tool_fixture_parity.rs`

**目标：** 让 Rust 生成一份 `ToolInfo[]` JSON fixture，供后续 `parity.md` 做 TS↔Rust 字段对照；Rust 侧先做 round-trip 自检。

- [ ] 在 `rust-ody/crates/agent-rs/Cargo.toml` 末尾新增 bin：

```toml
[[bin]]
name = "generate-tool-fixture"
path = "src/bin/generate_tool_fixture.rs"
```

- [ ] 新建 `rust-ody/crates/agent-rs/src/bin/generate_tool_fixture.rs`：

```rust
use std::{env, fs, path::PathBuf};

use agent_rs::records::AgentRecord;
use agent_rs::tool::{ToolManager, ToolManagerContext, UserToolRegistration};
use serde_json::json;

struct NoopCtx;

impl ToolManagerContext for NoopCtx {
    fn log_record(&mut self, _record: AgentRecord) {}
    fn emit_tool_list_updated(&mut self, _reason: &str, _server_name: Option<&str>) {}
    fn goal_mutation_tools_hidden(&self) -> bool {
        false
    }
}

fn main() {
    let mut mgr = ToolManager::new(NoopCtx);
    mgr.initialize_builtin_tools();
    mgr.register_user_tool(UserToolRegistration {
        name: "custom_user_tool".into(),
        description: "A user-registered tool for fixture generation.".into(),
        parameters: json!({"type": "object"}),
    });
    mgr.set_active_tools(&[
        "Read".into(),
        "Grep".into(),
        "custom_user_tool".into(),
    ]);

    let infos = mgr.data();
    let out_dir = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap()
        .join("tests/fixtures");
    fs::create_dir_all(&out_dir).unwrap();
    fs::write(
        out_dir.join("tools-rust.json"),
        serde_json::to_string_pretty(&infos).unwrap(),
    )
    .unwrap();
}
```

- [ ] 生成 fixture：

```bash
cd rust-ody && cargo run -p agent-rs --bin generate-tool-fixture
```

预期输出：`tests/fixtures/tools-rust.json` 被创建，内容包含 `Read`（active）、`Grep`（active）、`custom_user_tool`（active）以及其它未激活 builtin。

- [ ] 新建 `rust-ody/crates/agent-rs/tests/tool_fixture_parity.rs`：

```rust
use agent_rs::tool::{ToolInfo, ToolSource};

#[test]
fn rust_tools_fixture_round_trips() {
    let json = include_str!("fixtures/tools-rust.json");
    let infos: Vec<ToolInfo> = serde_json::from_str(json).unwrap();

    let active: Vec<_> = infos.iter().filter(|i| i.active).map(|i| i.name.as_str()).collect();
    assert!(active.contains(&"Read"));
    assert!(active.contains(&"Grep"));
    assert!(active.contains(&"custom_user_tool"));

    let read = infos.iter().find(|i| i.name == "Read").unwrap();
    assert_eq!(read.source, ToolSource::Builtin);

    let custom = infos.iter().find(|i| i.name == "custom_user_tool").unwrap();
    assert_eq!(custom.source, ToolSource::User);

    let re = serde_json::to_string_pretty(&infos).unwrap();
    let infos2: Vec<ToolInfo> = serde_json::from_str(&re).unwrap();
    assert_eq!(infos, infos2);
}
```

- [ ] 运行 fixture 测试：

```bash
cd rust-ody && cargo test -p agent-rs --test tool_fixture_parity
```

预期输出：`test result: ok. 1 passed; 0 failed`。

- [ ] 运行整 crate 类型检查：

```bash
cd rust-ody && cargo check -p agent-rs --workspace --tests
```

预期输出：无错误。

- [ ] Commit：`test(agent-rs): add ToolManager L1 fixture for TS parity`

---

## Local Self-Review

- [ ] 1. Spec-coverage：本部分覆盖 Roadmap 4.3.2.3（`ToolManager` + tool types）。
- [ ] 2. Placeholder扫描：无 TODO/TBD；MCP 执行逻辑与完整 builtin 列表明确标记为 4.3.9/4.4 补齐；4.3.2 的 MCP 匹配器是简化前缀匹配，已说明。
- [ ] 3. No phantom tasks：Task 1 产出类型；Task 2 产出 `ToolManager` 骨架与行为测试；Task 3 产出核心 builtin 与 `loop_tools`；Task 4 产出 MCP 接口桩与碰撞测试；Task 5 产出 fixture。
- [ ] 4. Dependency soundness：Task 2 依赖 Task 1；Task 3 依赖 Task 2；Task 4 依赖 Task 3；Task 5 依赖 Task 4；仅依赖 4.3.0 records 层。
- [ ] 5. Caller & build soundness：`lib.rs` 新增 `pub mod tool`，无其他 crate 调用方；`Cargo.toml` 新增 bin；每次任务以 `cargo check -p agent-rs --workspace --tests` 验证。
- [ ] 6. Test-the-risk：`register_user_tool` / `unregister_user_tool` / `set_active_tools` / `update_store` 均断言 WAL 记录与状态变化；`loop_tools` 断言排序与 goal 工具隐藏；MCP 注册断言碰撞检测与 access pattern 过滤。
- [ ] 7. Type一致性：`ToolSource`、`ToolInfo`、`UserToolRegistration`、`McpToolCollision` 字段名/序列化与 TS 源一致；`AgentRecord` tools 变体复用 4.3.0 records 层定义；`ExecutableTool` 与 TS `ExecutableTool` 的元数据字段对齐。
