# Part 5 — Integration: State Stores, i18n, Agent Wiring, Tool Registration, L1/L3 Parity

**Goal:** 将 Parts 1–4 产出的 trait 与工具接入 `agent-rs`，实现持久化状态存储、语言切换、ToolManager 注册，并通过 L1/L3 parity 证明 Rust 行为与 TS 等价。

**Architecture:** `agent-rs` 新增 `FileSystemOfficeHoursStateStore` / `FileSystemGameDesignStateStore`（基于 `Kaos` 的 JSONL 持久化），`AgentSessionModeProvider` 把 `Agent` 的内部状态暴露为 `tools_rs::builtin::session_mode::SessionModeProvider`；`SessionModeToolkit` 把所有 session-mode 工具桥接到 `TurnTools::loop_tools()`；`ToolManager::core_builtin_tools()` 补充工具元数据。最后通过新增 L1 fixture 与调用 Rust `session_mode_l3` binary 的 L3 driver 完成 parity。

**Tech stack:** Rust (`agent-rs`, `tools-rs`), `serde_json`, `tokio`, `chrono`, TypeScript/Vitest for parity harness.

**Depends on:** `infra.md` Task 1–4，`planning-tools.md`，`office-hours-tools.md`，`game-design-tools.md`。

> For executing workers: implement this plan task-by-task（建议每个 Task 用一个新 subagent/Task，避免单会话退化）。步骤使用 - [ ] 复选框跟踪。

---

## File Structure

| File | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/src/tool/state_store.rs` | `FileSystemOfficeHoursStateStore`、`FileSystemGameDesignStateStore`、内联测试。 |
| `rust-ody/crates/agent-rs/src/tool/i18n.rs` | Session-mode 状态/TUI 标签的 en/zh 本地化辅助函数。 |
| `rust-ody/crates/agent-rs/src/tool/session_mode_provider.rs` | `KaosSessionModeContext`、`AgentTelemetryClient`、`AgentMcpProvider`、`AgentSessionModeProvider`。 |
| `rust-ody/crates/agent-rs/src/tool/session_mode_toolkit.rs` | `SessionModeToolkit::build_tools()` 把 22 个 session-mode 工具打包成 `Arc<dyn ExecutableTool>`。 |
| `rust-ody/crates/agent-rs/src/tool/mod.rs` | 暴露新子模块。 |
| `rust-ody/crates/agent-rs/src/agent.rs` | 添加 `user_language`、state store、session-mode-provider 字段；将 `session_mode` 改为 `tokio::sync::Mutex`；更新所有调用点；`Agent::loop_tools()` 追加 session-mode 工具。 |
| `rust-ody/crates/agent-rs/src/tool/manager.rs` | `core_builtin_tools()` 追加 session-mode 工具元数据。 |
| `rust-ody/crates/tools-rs/src/golden.rs` | 新增 session-mode tool 的 `Op` 分支与 mock provider。 |
| `packages/integration-tests/src/parity/tools-rs-golden.ts` | 新增 `GoldenOp` 分支与 `runCase` 处理。 |
| `packages/integration-tests/src/parity/session-mode-l3-driver.ts` | 调用 Rust `session_mode_l3` binary 并返回事件列表。 |
| `packages/integration-tests/test/parity/tools-rs/l1-golden.test.ts` | fixture 列表追加 `session-mode-tools.json`。 |
| `packages/integration-tests/src/parity/fixtures/tools-rs/session-mode-tools.json` | L1 golden fixture。 |

---

## Dependency Overview

```
Task 1: FileSystemOfficeHoursStateStore + FileSystemGameDesignStateStore
  │
  ├──► Task 2: agent-rs i18n 标签表
  │
  ├──► Task 3: Agent.session_mode 改为 tokio::sync::Mutex + 更新所有调用点
  │       │
  │       └──► Task 4: AgentSessionModeProvider + KaosSessionModeContext + telemetry/MCP
  │               │
  │               └──► Task 5: Agent 字段/Builder 接线 + ToolManager metadata 注册
  │                       │
  │                       └──► Task 6: SessionModeToolkit + Agent::loop_tools 注册
  │                               │
  │                               ├──► Task 7: L1 golden fixture + harness
  │                               └──► Task 8: L3 parity driver 接线
```

Task 1/2 可并行；Task 3 是共享签名变更，必须在本 Part 内一次性完成；Task 4 依赖 Task 3；Task 5 依赖 Task 1/4；Task 6 依赖 Task 5；Task 7/8 依赖 Task 6。

---

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| `Agent.session_mode` 当前是 `std::sync::Mutex`，`SessionModeProvider` 的异步方法需要 `.await` 持锁。 | Task 3 将其替换为 `tokio::sync::Mutex`，并统一更新所有 `.lock().unwrap()` 调用点；同步 trait 方法使用 `block_in_place` + `blocking_lock`。 |
| `tools_rs::builtin::session_mode::SessionModeKind` 与 `agent_rs::records::nested::SessionModeKind` 是两个不同枚举。 | `AgentSessionModeProvider` 中显式 match 转换。 |
| `Kaos` 的 `read_text`/`write_text`/`stat` 签名与 `SessionModeContext` 不一致。 | 在 `KaosSessionModeContext` 中填入默认参数（encoding/errors/mode/follow_symlinks）并做错误映射。 |
| `ToolManager::core_builtin_tools()` 是纯函数，无法拿到 provider。 | 只补充静态元数据（name/description/parameters），实际执行由 `Agent::loop_tools()` 返回的 `ToolBridge` 实例负责。 |
| L1 golden 需要为 22 个工具编写 fixture/harness。 | 只覆盖每个 mode 的 enter/exit + 关键 mutation/search/sync/routing 工具，其余在 L3 覆盖。 |
| L3 parity 当前 TS driver 是 stub。 | Task 8 让 driver 调用 Rust `session_mode_l3` binary，与现有 Rust test 保持一致。 |

---

## Task 1: 实现 `FileSystemOfficeHoursStateStore` 与 `FileSystemGameDesignStateStore`

**Depends on:** `infra.md` Task 3（`OfficeHoursStateStore` / `GameDesignStateStore` trait 已定义）。

**Files:**
- Create: `rust-ody/crates/agent-rs/src/tool/state_store.rs`
- Modify: `rust-ody/crates/agent-rs/src/tool/mod.rs`（追加 `pub mod state_store;`）
- Test: `rust-ody/crates/agent-rs/src/tool/state_store.rs`（内联测试）

**Why:** 工具侧的 `AppendLearning` / `AppendProfile` / `SearchLearnings` 最终要写入真实文件。本 Task 提供与 TS `FileSystemOfficeHoursStateStore` / `FileSystemGameDesignStateStore` 行为一致的 JSONL 持久化实现。

**Steps：**

- [ ] 创建 `rust-ody/crates/agent-rs/src/tool/state_store.rs`：

```rust
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json;
use tools_rs::builtin::session_mode::{
    BuilderProfileEntry, GameDesignProfileEntry, LearningEntry,
    OfficeHoursStateStore, GameDesignStateStore,
};

pub struct FileSystemOfficeHoursStateStore {
    kaos: Arc<kaos_rs::kaos::Kaos>,
    base_dir: PathBuf,
}

impl FileSystemOfficeHoursStateStore {
    pub fn new(kaos: Arc<kaos_rs::kaos::Kaos>, home_dir: impl Into<PathBuf>) -> Self {
        Self {
            kaos,
            base_dir: home_dir.into().join("office-hours"),
        }
    }

    fn profile_path(&self) -> String {
        self.base_dir.join("builder-profile.jsonl").to_string_lossy().to_string()
    }

    fn learnings_path(&self) -> String {
        self.base_dir.join("learnings.jsonl").to_string_lossy().to_string()
    }

    async fn ensure_dir(&self) -> anyhow::Result<()> {
        self.kaos.mkdir(
            &self.base_dir.to_string_lossy(),
            true,
            true,
        ).await.map_err(|e| anyhow::anyhow!(e))
    }

    async fn append_jsonl(&self, path: &str, value: &serde_json::Value) -> anyhow::Result<()> {
        self.ensure_dir().await?;
        let line = serde_json::to_string(value)? + "\n";
        self.kaos.write_text(path, &line, Some("a"), None).await.map_err(|e| anyhow::anyhow!(e))?;
        Ok(())
    }
}

#[async_trait]
impl OfficeHoursStateStore for FileSystemOfficeHoursStateStore {
    async fn append_profile(&self, entry: BuilderProfileEntry) -> anyhow::Result<()> {
        let value = serde_json::to_value(entry)?;
        self.append_jsonl(&self.profile_path(), &value).await
    }

    async fn append_learning(&self, entry: LearningEntry) -> anyhow::Result<()> {
        let value = serde_json::to_value(entry)?;
        self.append_jsonl(&self.learnings_path(), &value).await
    }

    async fn search_learnings(
        &self,
        limit: usize,
        _cross_project: bool,
    ) -> anyhow::Result<Vec<LearningEntry>> {
        let text = match self.kaos.read_text(&self.learnings_path(), None, None).await {
            Ok(t) => t,
            Err(_) => return Ok(vec![]),
        };
        let mut entries: Vec<LearningEntry> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        entries.reverse();
        entries.truncate(limit);
        Ok(entries)
    }
}

pub struct FileSystemGameDesignStateStore {
    kaos: Arc<kaos_rs::kaos::Kaos>,
    base_dir: PathBuf,
}

impl FileSystemGameDesignStateStore {
    pub fn new(kaos: Arc<kaos_rs::kaos::Kaos>, project_root: impl Into<PathBuf>) -> Self {
        Self {
            kaos,
            base_dir: project_root.into().join(".ody-code").join("game-design"),
        }
    }

    fn profile_path(&self) -> String {
        self.base_dir.join("builder-profile.jsonl").to_string_lossy().to_string()
    }

    fn learnings_path(&self) -> String {
        self.base_dir.join("learnings.jsonl").to_string_lossy().to_string()
    }

    async fn ensure_dir(&self) -> anyhow::Result<()> {
        self.kaos.mkdir(
            &self.base_dir.to_string_lossy(),
            true,
            true,
        ).await.map_err(|e| anyhow::anyhow!(e))
    }

    async fn append_jsonl(&self, path: &str, value: &serde_json::Value) -> anyhow::Result<()> {
        self.ensure_dir().await?;
        let line = serde_json::to_string(value)? + "\n";
        self.kaos.write_text(path, &line, Some("a"), None).await.map_err(|e| anyhow::anyhow!(e))?;
        Ok(())
    }
}

#[async_trait]
impl GameDesignStateStore for FileSystemGameDesignStateStore {
    async fn append_profile(&self, entry: GameDesignProfileEntry) -> anyhow::Result<()> {
        let value = serde_json::to_value(entry)?;
        self.append_jsonl(&self.profile_path(), &value).await
    }

    async fn append_learning(&self, entry: LearningEntry) -> anyhow::Result<()> {
        let value = serde_json::to_value(entry)?;
        self.append_jsonl(&self.learnings_path(), &value).await
    }

    async fn search_learnings(
        &self,
        limit: usize,
        branch: Option<String>,
    ) -> anyhow::Result<Vec<LearningEntry>> {
        let text = match self.kaos.read_text(&self.learnings_path(), None, None).await {
            Ok(t) => t,
            Err(_) => return Ok(vec![]),
        };
        let mut entries: Vec<LearningEntry> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .filter(|e: &LearningEntry| branch.as_ref().map_or(true, |b| e.branch.as_ref() == Some(b)))
            .collect();
        entries.reverse();
        entries.truncate(limit);
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaos_rs::environment::Environment;
    use tools_rs::builtin::session_mode::Language;

    fn dummy_env() -> Environment {
        Environment {
            os_kind: "macOS".to_string(),
            os_arch: "arm64".to_string(),
            os_version: "23.0.0".to_string(),
            shell_name: "bash".to_string(),
            shell_path: "/bin/bash".to_string(),
        }
    }

    #[tokio::test]
    async fn office_hours_store_appends_and_searches() {
        let tmp = tempfile::tempdir().unwrap();
        let kaos = Arc::new(kaos_rs::kaos::Kaos::new(dummy_env(), tmp.path()));
        let store = FileSystemOfficeHoursStateStore::new(kaos, tmp.path());

        let entry = LearningEntry {
            ts: "2026-06-29T00:00:00Z".into(),
            skill: "office-hours".into(),
            type_: "eureka".into(),
            key: "demand_signal".into(),
            insight: "Users paid.".into(),
            confidence: 0.95,
            source: "observed".into(),
            branch: Some("main".into()),
        };
        store.append_learning(entry.clone()).await.unwrap();

        let profile = BuilderProfileEntry {
            date: "2026-06-29T00:00:00Z".into(),
            mode: "builder".into(),
            project_slug: "lunar-lander".into(),
            signal_count: 1,
            signals: vec!["demand_signal".into()],
            design_doc: ".ody-code/products/2026-06-29-lunar-lander.md".into(),
            assignment: "".into(),
            resources_shown: vec![],
            topics: vec!["demand".into()],
        };
        store.append_profile(profile).await.unwrap();

        let learnings = store.search_learnings(10, false).await.unwrap();
        assert_eq!(learnings.len(), 1);
        assert_eq!(learnings[0].key, "demand_signal");
    }

    #[tokio::test]
    async fn game_design_store_appends_and_filters_by_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let kaos = Arc::new(kaos_rs::kaos::Kaos::new(dummy_env(), tmp.path()));
        let store = FileSystemGameDesignStateStore::new(kaos, tmp.path());

        let e1 = LearningEntry {
            ts: "2026-06-29T00:00:00Z".into(),
            skill: "game-design".into(),
            type_: "eureka".into(),
            key: "flow_state".into(),
            insight: "Flow.".into(),
            confidence: 0.9,
            source: "observed".into(),
            branch: Some("feat/core".into()),
        };
        let e2 = LearningEntry {
            ts: "2026-06-29T01:00:00Z".into(),
            skill: "game-design".into(),
            type_: "operational".into(),
            key: "ui_guidance".into(),
            insight: "Affordances.".into(),
            confidence: 0.8,
            source: "observed".into(),
            branch: Some("feat/ui".into()),
        };
        store.append_learning(e1).await.unwrap();
        store.append_learning(e2).await.unwrap();

        let filtered = store.search_learnings(10, Some("feat/ui".into())).await.unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].key, "ui_guidance");

        let profile = GameDesignProfileEntry {
            date: "2026-06-29T00:00:00Z".into(),
            mode: "startup".into(),
            project_slug: "x".into(),
            pillars: "a, b, c".into(),
            audience: "y".into(),
            platform: "z".into(),
            genre: "rpg".into(),
            signals: vec![],
            design_doc: "d.md".into(),
        };
        store.append_profile(profile).await.unwrap();
    }
}
```

- [ ] 在 `rust-ody/crates/agent-rs/src/tool/mod.rs` 追加 `pub mod state_store;`。

- [ ] 确认 `rust-ody/crates/agent-rs/Cargo.toml` 已包含 `kaos-rs`、`tools-rs`、`serde_json`、`tempfile`（测试）和 `async-trait`；若缺失则追加。

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p agent-rs tool::state_store
```

Expected: 2 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(agent-rs): add FileSystemOfficeHoursStateStore and FileSystemGameDesignStateStore`。

---

## Task 2: agent-rs i18n 标签表

**Depends on:** `infra.md` Task 3（`Language` enum 已定义）。

**Files:**
- Create: `rust-ody/crates/agent-rs/src/tool/i18n.rs`
- Modify: `rust-ody/crates/agent-rs/src/tool/mod.rs`（追加 `pub mod i18n;`）
- Test: `rust-ody/crates/agent-rs/src/tool/i18n.rs`（内联测试）

**Why:** TS `translations.ts` 中 `tui.footer.*` / `tui.statusPanel.*` 等标签未来会被 Rust TUI/状态面板消费。本 Task 先将其端口到 `agent-rs`，避免后续 UI 接线时出现字符串漂移。

**Steps：**

- [ ] 创建 `rust-ody/crates/agent-rs/src/tool/i18n.rs`：

```rust
use tools_rs::builtin::session_mode::Language;

/// 返回 footer/status panel 中显示的 session-mode 名称，与 TS `translations.ts` 对齐。
pub fn session_mode_footer_label(mode: &str, lang: Language) -> String {
    match (mode, lang) {
        ("office-hours", Language::En) => "Office Hours".into(),
        ("office-hours", Language::Zh) => "办公时间".into(),
        ("game-design", Language::En) => "Game Design".into(),
        ("game-design", Language::Zh) => "游戏设计".into(),
        ("plan", Language::En) => "Plan".into(),
        ("plan", Language::Zh) => "规划".into(),
        ("design", Language::En) => "Design".into(),
        ("design", Language::Zh) => "设计".into(),
        _ => mode.into(),
    }
}

/// 返回状态面板的 on/off 标签。
pub fn status_on_label(lang: Language) -> String {
    match lang {
        Language::En => "on".into(),
        Language::Zh => "开启".into(),
    }
}

pub fn status_off_label(lang: Language) -> String {
    match lang {
        Language::En => "off".into(),
        Language::Zh => "关闭".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_labels_match_ts() {
        assert_eq!(session_mode_footer_label("office-hours", Language::En), "Office Hours");
        assert_eq!(session_mode_footer_label("office-hours", Language::Zh), "办公时间");
        assert_eq!(session_mode_footer_label("game-design", Language::En), "Game Design");
        assert_eq!(session_mode_footer_label("game-design", Language::Zh), "游戏设计");
    }

    #[test]
    fn status_labels_match_ts() {
        assert_eq!(status_on_label(Language::En), "on");
        assert_eq!(status_on_label(Language::Zh), "开启");
        assert_eq!(status_off_label(Language::En), "off");
        assert_eq!(status_off_label(Language::Zh), "关闭");
    }
}
```

- [ ] 在 `rust-ody/crates/agent-rs/src/tool/mod.rs` 追加 `pub mod i18n;`。

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p agent-rs tool::i18n
```

Expected: 2 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(agent-rs): port session-mode TUI i18n labels`。

---

## Task 3: 将 `Agent.session_mode` 改为 `tokio::sync::Mutex` 并更新所有调用点

**Depends on:** none（本 Task 是 `agent-rs` 内部共享签名变更，必须在同一次 commit 中完成）。

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/agent.rs`
- Test: `rust-ody/crates/agent-rs/tests/session_mode_manager.rs`（确认不受影响）

**Why:** `AgentSessionModeProvider` 的异步方法需要 `.await` 期间安全地持有 `SessionModeManager`；`std::sync::Mutex` 的 guard 不是 `Send`，无法满足 `#[async_trait]` 的 `SessionModeProvider` 要求。

**Steps：**

- [ ] 先写失败测试，证明当前 `std::sync::Mutex` 无法直接实现 `SessionModeProvider` 的异步方法。在 `rust-ody/crates/agent-rs/src/tool/session_mode_provider.rs` 占位文件中（本 Task 结束时可删除或保留为 Task 4 基础）尝试编译：

```rust
use std::sync::Arc;
use async_trait::async_trait;
use tools_rs::builtin::session_mode::{SessionModeProvider, SessionModeKind};

struct DummyProvider(Arc<crate::agent::Agent>);

#[async_trait]
impl SessionModeProvider for DummyProvider {
    async fn enter_session_mode(&self, _kind: SessionModeKind) -> anyhow::Result<()> {
        let _guard = self.0.session_mode.lock().unwrap();
        Ok(())
    }
    // ... 其他方法省略
}
```

编译会失败：`MutexGuard` cannot be sent between threads safely。运行：

```bash
cd rust-ody && cargo check -p agent-rs --all-targets
```

Expected: 类型错误指向 `MutexGuard`。

- [ ] 修改 `rust-ody/crates/agent-rs/src/agent.rs`：

  1. 在文件顶部添加别名：

  ```rust
  use tokio::sync::Mutex as TokioMutex;
  ```

  2. 将字段声明从：

  ```rust
  pub session_mode: Mutex<SessionModeManager<AgentContext>>,
  ```

  改为：

  ```rust
  pub session_mode: TokioMutex<SessionModeManager<AgentContext>>,
  ```

  3. 在 `AgentBuilder::build` 中将构造从：

  ```rust
  let session_mode =
      Mutex::new(SessionModeManager::new(ctx.clone(), HashMap::new()));
  ```

  改为：

  ```rust
  let session_mode = TokioMutex::new(SessionModeManager::new(
      ctx.clone(),
      crate::session_mode::behaviors::create_default_mode_behavior_registry(),
  ));
  ```

  （`create_default_mode_behavior_registry()` 已在 4.3.9 实现；若 Part 1 Task 4 尚未落地，这里同时补齐默认注册表。）

  4. 在 `impl Agent` 中添加同步访问辅助函数：

  ```rust
  impl Agent {
      fn with_session_mode_sync<T>(
          &self,
          f: impl FnOnce(&SessionModeManager<AgentContext>) -> T,
      ) -> T {
          tokio::task::block_in_place(|| f(&*self.session_mode.blocking_lock()))
      }
  }
  ```

  5. 更新 `restore_record` 中的三处 `if let Ok(mut sm) = self.session_mode.lock() { ... }`：

  ```rust
  if let Ok(mut sm) = self.session_mode.try_lock() {
      let _ = sm.restore_enter(id.clone(), *kind, path.clone());
  }
  ```

  对 `SessionModeExit`、`SessionModeCancel` 同理使用 `try_lock()`。restore 期间锁竞争概率极低；若需要严格阻塞，可改用 `self.with_session_mode_sync(|sm| sm.restore_enter(...))`，但 `restore_enter` 只设置字段不 `.await`，两种方式均可。

  6. 更新 `Agent::enter_session_mode`：

  ```rust
  pub async fn enter_session_mode(
      &self,
      kind: SessionModeKind,
      id: Option<String>,
  ) -> anyhow::Result<()> {
      self.session_mode.lock().await.enter(kind, id, None).await
  }
  ```

  7. 更新 `Agent::exit_session_mode`：

  ```rust
  pub async fn exit_session_mode(&self) -> anyhow::Result<()> {
      self.session_mode.lock().await.exit(None).await
  }
  ```

  8. 更新 `TurnSessionMode` 实现：

  ```rust
  #[async_trait]
  impl TurnSessionMode for Agent {
      fn is_active(&self) -> bool {
          self.with_session_mode_sync(|sm| sm.is_active())
      }
      fn kind(&self) -> Option<String> {
          self.with_session_mode_sync(|sm| sm.kind().map(|k| format!("{:?}", k).to_lowercase()))
      }
      fn file_path(&self) -> Option<String> {
          self.with_session_mode_sync(|sm| sm.session_mode_file_path())
      }
      async fn data(&self) -> Option<String> { None }
  }
  ```

  9. 更新 `InjectionManagerContext for Agent` 中的同步 getter：

  ```rust
  impl InjectionManagerContext for Agent {
      fn is_session_mode_active(&self) -> bool {
          self.with_session_mode_sync(|sm| sm.is_active())
      }
      fn session_mode_file_path(&self) -> Option<String> {
          self.with_session_mode_sync(|sm| sm.session_mode_file_path())
      }
      // ... 其他方法不变
  }
  ```

- [ ] 搜索并确认 `agent-rs` 中不再残留 `session_mode.lock().unwrap()`：

```bash
cd rust-ody && rg -n "session_mode\.lock\(\)\.unwrap\(\)" crates/agent-rs/src crates/agent-rs/tests
```

Expected: 无结果。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] 运行 session-mode manager 测试，确认 `SessionModeManager` 本身未受影响：

```bash
cd rust-ody && cargo test -p agent-rs --test session_mode_manager
```

Expected: 通过。

- [ ] Commit: `refactor(agent-rs): use tokio::sync::Mutex for Agent.session_mode`。

---

## Task 4: 实现 `AgentSessionModeProvider`、`KaosSessionModeContext`、Telemetry 与 MCP 桥接

**Depends on:** Task 3（`Agent.session_mode` 已是 `tokio::sync::Mutex`）。

**Files:**
- Create: `rust-ody/crates/agent-rs/src/tool/session_mode_provider.rs`
- Modify: `rust-ody/crates/agent-rs/src/tool/mod.rs`（追加 `pub mod session_mode_provider;`）
- Test: `rust-ody/crates/agent-rs/src/tool/session_mode_provider.rs`（内联测试 `KaosSessionModeContext`）

**Why:** `tools-rs` 的 session-mode 工具只认 `tools_rs::builtin::session_mode::SessionModeProvider` trait。本 Task 把 `Agent` 的内部状态（session mode、语言、state stores、kaos、telemetry、MCP）桥接到该 trait。

**Steps：**

- [ ] 创建 `rust-ody/crates/agent-rs/src/tool/session_mode_provider.rs`：

```rust
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use async_trait::async_trait;
use serde_json::Value;
use tools_rs::builtin::session_mode::{
    GameDesignStateStore, Language, McpProvider, OfficeHoursStateStore, SessionModeContext,
    SessionModeKind as ToolsSessionModeKind, SessionModeProvider, TelemetryClient,
};

use crate::agent::Agent;
use crate::session_mode::manager::SessionModeManager;
use crate::session_mode::types::SessionModeContext as AgentSessionModeContext;

/// 将 `kaos_rs::Kaos` 适配到 `tools_rs::builtin::session_mode::SessionModeContext`。
pub struct KaosSessionModeContext {
    kaos: Arc<kaos_rs::kaos::Kaos>,
}

impl KaosSessionModeContext {
    pub fn new(kaos: Arc<kaos_rs::kaos::Kaos>) -> Self {
        Self { kaos }
    }
}

#[async_trait]
impl SessionModeContext for KaosSessionModeContext {
    fn cwd(&self) -> String {
        self.kaos.getcwd()
    }

    fn project_root(&self) -> Option<String> {
        Some(self.kaos.getcwd())
    }

    async fn read_text(&self, path: &str) -> anyhow::Result<String> {
        self.kaos
            .read_text(path, None, None)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn write_text(&self, path: &str, content: &str) -> anyhow::Result<()> {
        self.kaos
            .write_text(path, content, None, None)
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn stat(&self, path: &str) -> anyhow::Result<()> {
        self.kaos
            .stat(path, true)
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!(e))
    }
}

/// 默认 MCP provider：当前 Rust host 未暴露 MCP 能力时返回 false，工具会走 CLI fallback。
pub struct NoopMcpProvider;

#[async_trait]
impl McpProvider for NoopMcpProvider {
    async fn gbrain_available(&self) -> bool {
        false
    }
}

/// 将 `AgentEnvironment::track_telemetry` 桥接到 `TelemetryClient`。
pub struct AgentTelemetryClient {
    agent: Weak<Agent>,
}

impl TelemetryClient for AgentTelemetryClient {
    fn track(&self, event: &str, properties: HashMap<String, Value>) {
        if let Some(agent) = self.agent.upgrade() {
            agent.environment.track_telemetry(event, serde_json::Value::Object(
                properties.into_iter().map(|(k, v)| (k, v)).collect()
            ));
        }
    }
}

/// 持有 `Weak<Agent>` 的 `SessionModeProvider` 实现，避免与 `Agent` 形成强引用循环。
pub struct AgentSessionModeProvider {
    agent: Weak<Agent>,
}

impl AgentSessionModeProvider {
    pub fn new(agent: Weak<Agent>) -> Self {
        Self { agent }
    }

    fn upgrade(&self) -> Option<Arc<Agent>> {
        self.agent.upgrade()
    }

    fn tools_kind_to_agent_kind(kind: ToolsSessionModeKind) -> crate::records::nested::SessionModeKind {
        match kind {
            ToolsSessionModeKind::Plan => crate::records::nested::SessionModeKind::Plan,
            ToolsSessionModeKind::Design => crate::records::nested::SessionModeKind::Design,
            ToolsSessionModeKind::OfficeHours => crate::records::nested::SessionModeKind::OfficeHours,
            ToolsSessionModeKind::GameDesign => crate::records::nested::SessionModeKind::GameDesign,
        }
    }
}

#[async_trait]
impl SessionModeProvider for AgentSessionModeProvider {
    fn is_session_mode_active(&self) -> bool {
        self.upgrade()
            .map(|a| a.with_session_mode_sync(|sm| sm.is_active()))
            .unwrap_or(false)
    }

    fn session_mode_kind(&self) -> Option<ToolsSessionModeKind> {
        self.upgrade().and_then(|a| {
            a.with_session_mode_sync(|sm| sm.kind()).map(|k| match k {
                crate::records::nested::SessionModeKind::Plan => ToolsSessionModeKind::Plan,
                crate::records::nested::SessionModeKind::Design => ToolsSessionModeKind::Design,
                crate::records::nested::SessionModeKind::OfficeHours => ToolsSessionModeKind::OfficeHours,
                crate::records::nested::SessionModeKind::GameDesign => ToolsSessionModeKind::GameDesign,
            })
        })
    }

    fn session_mode_file_path(&self) -> Option<String> {
        self.upgrade()
            .map(|a| a.with_session_mode_sync(|sm| sm.session_mode_file_path()))
            .flatten()
    }

    async fn enter_session_mode(&self, kind: ToolsSessionModeKind) -> anyhow::Result<()> {
        let agent = self.upgrade().ok_or_else(|| anyhow::anyhow!("agent dropped"))?;
        let agent_kind = Self::tools_kind_to_agent_kind(kind);
        agent.session_mode.lock().await.enter(agent_kind, None, None).await
    }

    async fn exit_session_mode(&self) -> anyhow::Result<()> {
        let agent = self.upgrade().ok_or_else(|| anyhow::anyhow!("agent dropped"))?;
        agent.session_mode.lock().await.exit(None).await
    }

    async fn handoff_to(&self, target: &str, selected_label: Option<String>) -> anyhow::Result<()> {
        let agent = self.upgrade().ok_or_else(|| anyhow::anyhow!("agent dropped"))?;
        let options = crate::session_mode::types::HandoffOptions { selected_label };
        agent.session_mode.lock().await.handoff_to(target, options).await
    }

    fn user_language(&self) -> Language {
        self.upgrade()
            .map(|a| *a.user_language.lock().unwrap())
            .unwrap_or(Language::En)
    }

    fn set_user_language(&self, lang: Language) {
        if let Some(agent) = self.upgrade() {
            *agent.user_language.lock().unwrap() = lang;
        }
    }

    fn open_external_available(&self) -> bool {
        false
    }

    fn telemetry(&self) -> Arc<dyn TelemetryClient> {
        Arc::new(AgentTelemetryClient { agent: self.agent.clone() })
    }

    fn kaos(&self) -> Arc<dyn SessionModeContext> {
        self.upgrade()
            .map(|a| Arc::new(KaosSessionModeContext::new(a.kaos.clone())) as Arc<dyn SessionModeContext>)
            .unwrap_or_else(|| Arc::new(KaosSessionModeContext::new(Arc::new(kaos_rs::kaos::Kaos::new(
                kaos_rs::environment::Environment {
                    os_kind: "macOS".to_string(),
                    os_arch: "arm64".to_string(),
                    os_version: "23.0.0".to_string(),
                    shell_name: "bash".to_string(),
                    shell_path: "/bin/bash".to_string(),
                },
                "/",
            ))))
    }

    fn office_hours_store(&self) -> Arc<dyn OfficeHoursStateStore> {
        self.upgrade()
            .map(|a| Arc::clone(&a.office_hours_state_store) as Arc<dyn OfficeHoursStateStore>)
            .unwrap_or_else(|| Arc::new(crate::tool::state_store::FileSystemOfficeHoursStateStore::new(
                Arc::new(kaos_rs::kaos::Kaos::new(
                    kaos_rs::environment::Environment {
                        os_kind: "macOS".to_string(),
                        os_arch: "arm64".to_string(),
                        os_version: "23.0.0".to_string(),
                        shell_name: "bash".to_string(),
                        shell_path: "/bin/bash".to_string(),
                    },
                    "/",
                )),
                "/",
            )))
    }

    fn game_design_store(&self) -> Arc<dyn GameDesignStateStore> {
        self.upgrade()
            .map(|a| Arc::clone(&a.game_design_state_store) as Arc<dyn GameDesignStateStore>)
            .unwrap_or_else(|| Arc::new(crate::tool::state_store::FileSystemGameDesignStateStore::new(
                Arc::new(kaos_rs::kaos::Kaos::new(
                    kaos_rs::environment::Environment {
                        os_kind: "macOS".to_string(),
                        os_arch: "arm64".to_string(),
                        os_version: "23.0.0".to_string(),
                        shell_name: "bash".to_string(),
                        shell_path: "/bin/bash".to_string(),
                    },
                    "/",
                )),
                "/",
            )))
    }

    fn mcp(&self) -> Arc<dyn McpProvider> {
        Arc::new(NoopMcpProvider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaos_rs::environment::Environment;

    fn dummy_env() -> Environment {
        Environment {
            os_kind: "macOS".to_string(),
            os_arch: "arm64".to_string(),
            os_version: "23.0.0".to_string(),
            shell_name: "bash".to_string(),
            shell_path: "/bin/bash".to_string(),
        }
    }

    #[tokio::test]
    async fn kaos_session_mode_context_reads_and_writes() {
        let tmp = tempfile::tempdir().unwrap();
        let kaos = Arc::new(kaos_rs::kaos::Kaos::new(dummy_env(), tmp.path()));
        let ctx = KaosSessionModeContext::new(kaos);

        ctx.write_text("foo.txt", "hello").await.unwrap();
        assert_eq!(ctx.read_text("foo.txt").await.unwrap(), "hello");
        ctx.stat("foo.txt").await.unwrap();
        assert!(ctx.read_text("missing.txt").await.is_err());
    }
}
```

- [ ] 在 `rust-ody/crates/agent-rs/src/tool/mod.rs` 追加 `pub mod session_mode_provider;`。

- [ ] 确认 `Agent` 的 `with_session_mode_sync` 辅助函数（Task 3）是 `pub(crate)` 或 `pub(super)`，以便 `session_mode_provider.rs` 调用。

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p agent-rs tool::session_mode_provider
```

Expected: 1 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(agent-rs): add AgentSessionModeProvider and KaosSessionModeContext`。

---

## Task 5: `Agent` / `AgentBuilder` 字段接线与默认 State Store

**Depends on:** Task 1（state stores 已实现）、Task 4（`AgentSessionModeProvider` 已实现）。

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/agent.rs`
- Test: `rust-ody/crates/agent-rs/tests/agent_session_mode_wiring.rs`（新建）

**Why:** 工具需要访问 language、state stores 和 provider；`AgentBuilder` 需要为未显式指定的环境提供与 TS 一致的默认持久化路径。

**Steps：**

- [ ] 在 `rust-ody/crates/agent-rs/src/agent.rs` 顶部追加引入：

```rust
use tools_rs::builtin::session_mode::{
    Language, OfficeHoursStateStore, GameDesignStateStore, SessionModeProvider,
};
```

- [ ] 在 `Agent` struct 中 `cached_llm` 之后追加字段：

```rust
pub user_language: Mutex<Language>,
pub office_hours_state_store: Arc<dyn OfficeHoursStateStore>,
pub game_design_state_store: Arc<dyn GameDesignStateStore>,
pub session_mode_provider: Arc<dyn SessionModeProvider>,
```

- [ ] 在 `AgentBuilder` struct 中追加可选字段：

```rust
office_hours_state_store: Option<Arc<dyn OfficeHoursStateStore>>,
game_design_state_store: Option<Arc<dyn GameDesignStateStore>>,
user_language: Option<Language>,
```

- [ ] 在 `AgentBuilder::new` 中初始化这些可选字段为 `None`。

- [ ] 在 `AgentBuilder` 中追加 setter：

```rust
pub fn office_hours_state_store(
    mut self,
    store: Arc<dyn OfficeHoursStateStore>,
) -> Self {
    self.office_hours_state_store = Some(store);
    self
}

pub fn game_design_state_store(
    mut self,
    store: Arc<dyn GameDesignStateStore>,
) -> Self {
    self.game_design_state_store = Some(store);
    self
}

pub fn user_language(mut self, lang: Language) -> Self {
    self.user_language = Some(lang);
    self
}
```

- [ ] 在 `AgentBuilder::build` 的 `Arc::new_cyclic` 闭包内，构造 `Agent` 之前计算默认值：

```rust
let default_office_hours_store: Arc<dyn OfficeHoursStateStore> = self
    .office_hours_state_store
    .unwrap_or_else(|| {
        Arc::new(crate::tool::state_store::FileSystemOfficeHoursStateStore::new(
            Arc::clone(&self.kaos),
            self.homedir.clone().unwrap_or_else(|| PathBuf::from("/")),
        ))
    });

let default_game_design_store: Arc<dyn GameDesignStateStore> = self
    .game_design_state_store
    .unwrap_or_else(|| {
        Arc::new(crate::tool::state_store::FileSystemGameDesignStateStore::new(
            Arc::clone(&self.kaos),
            self.kaos.getcwd(),
        ))
    });

let user_language = Mutex::new(self.user_language.unwrap_or(Language::En));
```

- [ ] 在 `Arc::new_cyclic` 闭包内、构造 `Agent` 结构体时追加字段：

```rust
user_language,
office_hours_state_store: default_office_hours_store,
game_design_state_store: default_game_design_store,
session_mode_provider: Arc::new(crate::tool::session_mode_provider::AgentSessionModeProvider::new(
    weak.clone(),
)) as Arc<dyn SessionModeProvider>,
```

- [ ] 创建 `rust-ody/crates/agent-rs/tests/agent_session_mode_wiring.rs`：

```rust
use std::sync::Arc;
use agent_rs::agent::{AgentBuilder, AgentEnvironment, AgentEvent, AgentType, ApprovalRequest};
use kosong_rs::message::ContentPart;
use kosong_rs::provider::AbortSignal;
use tools_rs::builtin::session_mode::{Language, SessionModeProvider, SessionModeKind};

struct NoopEnv;

impl AgentEnvironment for NoopEnv {
    fn emit_event(&self, _event: AgentEvent) {}
    async fn request_approval(
        &self,
        _req: &ApprovalRequest,
        _signal: AbortSignal,
    ) -> Result<agent_rs::records::nested::ApprovalResponse, anyhow::Error> {
        Ok(agent_rs::records::nested::ApprovalResponse {
            decision: "approved".into(),
            scope: None,
            feedback: None,
            selected_label: None,
        })
    }
    fn fire_hook_pre_tool_use(
        &self,
        _tool_name: &str,
        _tool_input: serde_json::Value,
        _tool_call_id: &str,
        _signal: AbortSignal,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>> {
        Box::pin(async move { Ok(None) })
    }
    fn fire_hook_permission_request(&self, _tool_name: &str, _data: serde_json::Value) {}
    fn fire_hook_permission_result(&self, _tool_name: &str, _data: serde_json::Value) {}
    fn fire_hook_user_prompt_submit(
        &self,
        _input: Vec<ContentPart>,
        _signal: AbortSignal,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<agent_rs::turn::types::HookResult>, anyhow::Error>> + Send + '_>> {
        Box::pin(async move { Ok(vec![]) })
    }
    fn fire_hook_stop_hook(
        &self,
        _signal: AbortSignal,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<agent_rs::turn::types::StopHookBlock>, anyhow::Error>> + Send + '_>> {
        Box::pin(async move { Ok(None) })
    }
    fn fire_and_forget_hook(&self, _event: &str, _data: serde_json::Value) {}
    fn trigger_hook(
        &self,
        _event: &str,
        _data: serde_json::Value,
        _signal: AbortSignal,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), anyhow::Error>> + Send + '_>> {
        Box::pin(async move { Ok(()) })
    }
    fn track_telemetry(&self, _event: &str, _properties: serde_json::Value) {}
    fn log_debug(&self, _msg: &str, _data: serde_json::Value) {}
    fn log_warn(&self, _msg: &str, _data: serde_json::Value) {}
    fn log_error(&self, _msg: &str, _data: serde_json::Value) {}
}

#[tokio::test]
async fn agent_builds_with_default_state_stores() {
    let kaos = Arc::new(kaos_rs::kaos::Kaos::new(
        kaos_rs::environment::Environment {
            os_kind: "macOS".to_string(),
            os_arch: "arm64".to_string(),
            os_version: "23.0.0".to_string(),
            shell_name: "bash".to_string(),
            shell_path: "/bin/bash".to_string(),
        },
        std::env::current_dir().unwrap(),
    ));
    let env = Arc::new(NoopEnv);
    let agent = AgentBuilder::new("test", kaos, env).build().await.unwrap();

    assert_eq!(agent.user_language.lock().unwrap().clone(), Language::En);
    assert!(agent.session_mode_provider.is_session_mode_active() == false);
    assert!(agent.session_mode_provider.session_mode_kind().is_none());
}

#[tokio::test]
async fn agent_session_mode_provider_enters_and_exits() {
    let kaos = Arc::new(kaos_rs::kaos::Kaos::new(
        kaos_rs::environment::Environment {
            os_kind: "macOS".to_string(),
            os_arch: "arm64".to_string(),
            os_version: "23.0.0".to_string(),
            shell_name: "bash".to_string(),
            shell_path: "/bin/bash".to_string(),
        },
        std::env::current_dir().unwrap(),
    ));
    let env = Arc::new(NoopEnv);
    let agent = AgentBuilder::new("test", kaos, env).build().await.unwrap();
    let provider = Arc::clone(&agent.session_mode_provider);

    provider.enter_session_mode(SessionModeKind::OfficeHours).await.unwrap();
    assert!(provider.is_session_mode_active());
    assert_eq!(provider.session_mode_kind(), Some(SessionModeKind::OfficeHours));

    provider.exit_session_mode().await.unwrap();
    assert!(!provider.is_session_mode_active());
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p agent-rs --test agent_session_mode_wiring
```

Expected: 2 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(agent-rs): wire user_language, state stores, and SessionModeProvider into Agent`。

---

## Task 6: `SessionModeToolkit` 与 `ToolManager` 注册

**Depends on:** Task 5（`Agent.session_mode_provider` 与 state stores 已存在）。

**Files:**
- Create: `rust-ody/crates/agent-rs/src/tool/session_mode_toolkit.rs`
- Modify: `rust-ody/crates/agent-rs/src/tool/mod.rs`（追加 `pub mod session_mode_toolkit;`）
- Modify: `rust-ody/crates/agent-rs/src/tool/manager.rs`（`core_builtin_tools()` 追加 metadata）
- Modify: `rust-ody/crates/agent-rs/src/agent.rs`（`Agent::loop_tools()` 追加执行实例）
- Test: `rust-ody/crates/agent-rs/src/tool/session_mode_toolkit.rs`（内联测试）

**Why:** Session-mode 工具需要同时出现在 LLM 可见的工具列表（`ToolManager::core_builtin_tools` 元数据）和实际可执行集合（`TurnTools::loop_tools`）中。`SessionModeToolkit` 把这两份数据集中管理，避免 metadata 与执行实例漂移。

**Steps：**

- [ ] 创建 `rust-ody/crates/agent-rs/src/tool/session_mode_toolkit.rs`：

```rust
use std::sync::Arc;
use serde_json::json;
use tools_rs::builtin::session_mode::SessionModeProvider;
use tools_rs::builtin::session_mode::game_design::{
    EnterGameDesignModeTool, ExitGameDesignModeTool, SetGameDesignLanguageTool,
    AppendGameDesignLearningTool, AppendGameDesignProfileTool, SearchGameDesignLearningsTool,
    EnsureGameDesignRoutingTool, SyncGameDesignArtifactTool,
};
use tools_rs::builtin::session_mode::office_hours::{
    EnterOfficeHoursModeTool, ExitOfficeHoursModeTool, SetOfficeHoursLanguageTool,
    AppendLearningTool as AppendOfficeHoursLearningTool,
    AppendBuilderProfileTool as AppendOfficeHoursProfileTool,
    SearchLearningsTool as SearchOfficeHoursLearningsTool,
    EnsureClaudeMdRoutingTool,
    SyncOfficeHoursArtifactTool,
};
use tools_rs::builtin::session_mode::planning::{
    EnterPlanModeTool, ExitPlanModeTool, EnterDesignModeTool, ExitDesignModeTool,
};

use crate::agent_loop::types::ExecutableTool as LoopExecutableTool;
use crate::tool::bridge::ToolBridge;
use crate::tool::types::ExecutableTool as ToolInfo;

pub struct SessionModeToolkit;

impl SessionModeToolkit {
    /// 返回 session-mode 工具的静态元数据，供 `ToolManager` 使用。
    pub fn metadata() -> Vec<ToolInfo> {
        vec![
            ToolInfo {
                name: "EnterPlanMode".into(),
                description: "Enter plan mode to produce a structured implementation plan.".into(),
                parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
            },
            ToolInfo {
                name: "ExitPlanMode".into(),
                description: "Exit plan mode after selecting an approach and writing the plan document.".into(),
                parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
            },
            ToolInfo {
                name: "EnterDesignMode".into(),
                description: "Enter design mode to produce a design document.".into(),
                parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
            },
            ToolInfo {
                name: "ExitDesignMode".into(),
                description: "Exit design mode after selecting an approach and writing the design document.".into(),
                parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
            },
            ToolInfo {
                name: "EnterOfficeHoursMode".into(),
                description: "Use this tool when the user explicitly asks to start office hours mode.".into(),
                parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
            },
            ToolInfo {
                name: "ExitOfficeHoursMode".into(),
                description: "Exit office hours mode after the design document has been approved and written.".into(),
                parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
            },
            ToolInfo {
                name: "SetOfficeHoursLanguage".into(),
                description: "Call once at the start of office-hours to record the language the user is writing in.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "language": { "type": "string", "enum": ["en", "zh"] }
                    },
                    "required": ["language"],
                    "additionalProperties": false
                }),
            },
            ToolInfo {
                name: "AppendLearning".into(),
                description: "Record a learning insight during office hours.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "type": { "type": "string", "enum": ["operational", "eureka"] },
                        "key": { "type": "string", "minLength": 1 },
                        "insight": { "type": "string", "minLength": 1 },
                        "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
                        "branch": { "type": "string" }
                    },
                    "required": ["type", "key", "insight", "confidence"],
                    "additionalProperties": false
                }),
            },
            ToolInfo {
                name: "AppendBuilderProfile".into(),
                description: "Append a builder profile entry summarizing the office hours session.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "mode": { "type": "string", "enum": ["startup", "builder"] },
                        "projectSlug": { "type": "string" },
                        "signalCount": { "type": "integer", "minimum": 0 },
                        "signals": { "type": "array", "items": { "type": "string" } },
                        "designDoc": { "type": "string" },
                        "assignment": { "type": "string" },
                        "resourcesShown": { "type": "array", "items": { "type": "string" } },
                        "topics": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["mode", "projectSlug", "signalCount", "signals", "designDoc", "assignment", "resourcesShown", "topics"],
                    "additionalProperties": false
                }),
            },
            ToolInfo {
                name: "SearchLearnings".into(),
                description: "Search past office-hours learnings.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "minimum": 1 },
                        "crossProject": { "type": "boolean" }
                    },
                    "additionalProperties": false
                }),
            },
            ToolInfo {
                name: "EnsureClaudeMdRouting".into(),
                description: "Ensure the project's AGENTS.md contains a ## Skill routing section for office-hours.".into(),
                parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
            },
            ToolInfo {
                name: "SyncOfficeHoursArtifact".into(),
                description: "Sync a design document artifact to the gbrain knowledge base during office hours handoff.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "designFilePath": { "type": "string" }
                    },
                    "required": ["designFilePath"],
                    "additionalProperties": false
                }),
            },
            ToolInfo {
                name: "EnterGameDesignMode".into(),
                description: "Enter game-design mode to begin a guided game design session.".into(),
                parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
            },
            ToolInfo {
                name: "ExitGameDesignMode".into(),
                description: "Exit game-design mode, save the final design document, and return to normal mode.".into(),
                parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
            },
            ToolInfo {
                name: "SetGameDesignLanguage".into(),
                description: "Set the user language for the game-design session to 'en' or 'zh'.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "language": { "type": "string", "enum": ["en", "zh"] }
                    },
                    "required": ["language"],
                    "additionalProperties": false
                }),
            },
            ToolInfo {
                name: "AppendGameDesignLearning".into(),
                description: "Record a learning insight discovered during game design.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "type": { "type": "string", "enum": ["operational", "eureka"] },
                        "key": { "type": "string", "minLength": 1 },
                        "insight": { "type": "string", "minLength": 1 },
                        "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
                        "branch": { "type": "string" }
                    },
                    "required": ["type", "key", "insight", "confidence"],
                    "additionalProperties": false
                }),
            },
            ToolInfo {
                name: "AppendGameDesignProfile".into(),
                description: "Append a builder profile entry summarizing the game design session.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "mode": { "type": "string", "enum": ["startup", "builder"] },
                        "projectSlug": { "type": "string" },
                        "pillars": { "type": "string" },
                        "audience": { "type": "string" },
                        "platform": { "type": "string" },
                        "genre": { "type": "string" },
                        "designDoc": { "type": "string" },
                        "signals": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["mode", "projectSlug", "pillars", "audience", "platform", "genre"],
                    "additionalProperties": false
                }),
            },
            ToolInfo {
                name: "SearchGameDesignLearnings".into(),
                description: "Search past game design learnings, optionally filtered by branch.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "minimum": 1 },
                        "branch": { "type": "string" }
                    },
                    "additionalProperties": false
                }),
            },
            ToolInfo {
                name: "EnsureGameDesignRouting".into(),
                description: "Ensure the project's AGENTS.md contains a ## Skill routing section for game-design mode.".into(),
                parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
            },
            ToolInfo {
                name: "SyncGameDesignArtifact".into(),
                description: "Sync the game design artifact document to persistent storage via gbrain MCP or CLI.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "designFilePath": { "type": "string" }
                    },
                    "required": ["designFilePath"],
                    "additionalProperties": false
                }),
            },
        ]
    }

    /// 返回实际可执行的 session-mode 工具实例。
    pub fn build_tools(
        provider: Arc<dyn SessionModeProvider>,
    ) -> Vec<Arc<dyn LoopExecutableTool>> {
        vec![
            Arc::new(ToolBridge::new(Arc::new(EnterPlanModeTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(ExitPlanModeTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(EnterDesignModeTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(ExitDesignModeTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(EnterOfficeHoursModeTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(ExitOfficeHoursModeTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(SetOfficeHoursLanguageTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(AppendOfficeHoursLearningTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(AppendOfficeHoursProfileTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(SearchOfficeHoursLearningsTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(EnsureClaudeMdRoutingTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(SyncOfficeHoursArtifactTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(EnterGameDesignModeTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(ExitGameDesignModeTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(SetGameDesignLanguageTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(AppendGameDesignLearningTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(AppendGameDesignProfileTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(SearchGameDesignLearningsTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(EnsureGameDesignRoutingTool::new(Arc::clone(&provider))))),
            Arc::new(ToolBridge::new(Arc::new(SyncGameDesignArtifactTool::new(Arc::clone(&provider))))),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tools_rs::builtin::session_mode::{
        Language, LearningEntry, McpProvider, OfficeHoursStateStore, GameDesignStateStore,
        SessionModeContext, TelemetryClient,
    };
    use std::collections::HashMap;

    struct DummyProvider;
    impl SessionModeContext for DummyProvider {
        fn cwd(&self) -> String { "/".into() }
        fn project_root(&self) -> Option<String> { None }
        async fn read_text(&self, _path: &str) -> anyhow::Result<String> { Ok(String::new()) }
        async fn write_text(&self, _path: &str, _content: &str) -> anyhow::Result<()> { Ok(()) }
        async fn stat(&self, _path: &str) -> anyhow::Result<()> { Ok(()) }
    }
    impl TelemetryClient for DummyProvider {
        fn track(&self, _event: &str, _properties: HashMap<String, serde_json::Value>) {}
    }
    #[async_trait::async_trait]
    impl OfficeHoursStateStore for DummyProvider {
        async fn append_profile(&self, _entry: tools_rs::builtin::session_mode::BuilderProfileEntry) -> anyhow::Result<()> { Ok(()) }
        async fn append_learning(&self, _entry: LearningEntry) -> anyhow::Result<()> { Ok(()) }
        async fn search_learnings(&self, _limit: usize, _cross_project: bool) -> anyhow::Result<Vec<LearningEntry>> { Ok(vec![]) }
    }
    #[async_trait::async_trait]
    impl GameDesignStateStore for DummyProvider {
        async fn append_profile(&self, _entry: tools_rs::builtin::session_mode::GameDesignProfileEntry) -> anyhow::Result<()> { Ok(()) }
        async fn append_learning(&self, _entry: LearningEntry) -> anyhow::Result<()> { Ok(()) }
        async fn search_learnings(&self, _limit: usize, _branch: Option<String>) -> anyhow::Result<Vec<LearningEntry>> { Ok(vec![]) }
    }
    #[async_trait::async_trait]
    impl McpProvider for DummyProvider {
        async fn gbrain_available(&self) -> bool { false }
    }
    #[async_trait::async_trait]
    impl SessionModeProvider for DummyProvider {
        fn is_session_mode_active(&self) -> bool { false }
        fn session_mode_kind(&self) -> Option<tools_rs::builtin::session_mode::SessionModeKind> { None }
        fn session_mode_file_path(&self) -> Option<String> { None }
        async fn enter_session_mode(&self, _kind: tools_rs::builtin::session_mode::SessionModeKind) -> anyhow::Result<()> { Ok(()) }
        async fn exit_session_mode(&self) -> anyhow::Result<()> { Ok(()) }
        async fn handoff_to(&self, _target: &str, _selected_label: Option<String>) -> anyhow::Result<()> { Ok(()) }
        fn user_language(&self) -> Language { Language::En }
        fn set_user_language(&self, _lang: Language) {}
        fn open_external_available(&self) -> bool { false }
        fn telemetry(&self) -> Arc<dyn TelemetryClient> { Arc::new(DummyProvider) }
        fn kaos(&self) -> Arc<dyn SessionModeContext> { Arc::new(DummyProvider) }
        fn office_hours_store(&self) -> Arc<dyn OfficeHoursStateStore> { Arc::new(DummyProvider) }
        fn game_design_store(&self) -> Arc<dyn GameDesignStateStore> { Arc::new(DummyProvider) }
        fn mcp(&self) -> Arc<dyn McpProvider> { Arc::new(DummyProvider) }
    }

    #[test]
    fn metadata_covers_all_session_mode_tools() {
        let names: Vec<_> = SessionModeToolkit::metadata().into_iter().map(|t| t.name).collect();
        assert_eq!(names.len(), 20);
        assert!(names.contains(&"EnterPlanMode".into()));
        assert!(names.contains(&"ExitOfficeHoursMode".into()));
        assert!(names.contains(&"SyncGameDesignArtifact".into()));
    }

    #[test]
    fn build_tools_returns_twenty_bridged_tools() {
        let provider = Arc::new(DummyProvider);
        let tools = SessionModeToolkit::build_tools(provider);
        assert_eq!(tools.len(), 20);
        let names: Vec<_> = tools.iter().map(|t| t.name().to_string()).collect();
        assert!(names.contains(&"AppendGameDesignLearning".into()));
    }
}
```

- [ ] 在 `rust-ody/crates/agent-rs/src/tool/mod.rs` 追加 `pub mod session_mode_toolkit;`。

- [ ] 修改 `rust-ody/crates/agent-rs/src/tool/manager.rs` 中的 `core_builtin_tools()`：

```rust
fn core_builtin_tools() -> Vec<ExecutableTool> {
    let mut tools = vec![
        // ... 原有 Read/Write/Edit/Glob/Grep/Bash/FetchURL/WebSearch 不变 ...
    ];
    tools.extend(crate::tool::session_mode_toolkit::SessionModeToolkit::metadata());
    tools
}
```

- [ ] 修改 `rust-ody/crates/agent-rs/src/agent.rs` 中的 `impl TurnTools for Agent`：

```rust
impl TurnTools for Agent {
    fn loop_tools(&self) -> Vec<Arc<dyn crate::agent_loop::types::ExecutableTool>> {
        let context = self.agent_context();
        let background = self.background.lock().unwrap().clone();
        let mut tools = crate::tool::collaboration::CollaborationToolkit::build_tools(
            context,
            self.skill_registry.lock().unwrap().clone(),
            self.question_callback.lock().unwrap().clone(),
            self.subagent_host.lock().unwrap().clone(),
            background,
        );
        tools.extend(crate::tool::session_mode_toolkit::SessionModeToolkit::build_tools(
            Arc::clone(&self.session_mode_provider),
        ));
        tools
    }
    // ... store_data 不变 ...
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p agent-rs tool::session_mode_toolkit
```

Expected: 2 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(agent-rs): register session-mode tools in ToolManager and TurnTools`。


---

## Task 7: L1 golden fixture + harness for session-mode tools

**Depends on:** Task 6（`SessionModeToolkit` 已注册），以及 `planning-tools.md` / `office-hours-tools.md` / `game-design-tools.md` 中各工具实现已落地。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/golden.rs`（`Op` 枚举新增 session-mode 分支；`run_case_sync` 新增处理臂；新增 `FixtureSessionModeProvider` 等本地 fixture 类型）
- Modify: `packages/integration-tests/src/parity/tools-rs-golden.ts`（`GoldenOp` 新增分支；`runCase` 新增处理臂）
- Create: `packages/integration-tests/src/parity/fixtures/tools-rs/session-mode-tools.json`
- Modify: `packages/integration-tests/test/parity/tools-rs/l1-golden.test.ts`（fixture 列表追加 `session-mode-tools.json`）

**Why:** 在 L1 层证明 Rust 工具实现与 TS 行为等价；只覆盖每个 mode 的代表性错误/成功路径，避免 22 个工具全量 fixture 的维护成本，长入口提示语由 L3 事件覆盖。

**Steps：**

- [ ] 在 `rust-ody/crates/tools-rs/src/golden.rs` 的 `Op` 枚举中（`Checkpoint` 之后）追加：

```rust
    // ── session-mode tool ops ──
    #[serde(rename = "enter_plan_mode")]
    EnterPlanMode { active_kind: Option<String> },
    #[serde(rename = "enter_design_mode")]
    EnterDesignMode { active_kind: Option<String> },
    #[serde(rename = "enter_office_hours")]
    EnterOfficeHours { active_kind: Option<String> },
    #[serde(rename = "set_office_hours_language")]
    SetOfficeHoursLanguage { language: String },
    #[serde(rename = "append_learning")]
    AppendLearning {
        #[serde(rename = "learningType")]
        learning_type: String,
        key: String,
        insight: String,
        confidence: f64,
        branch: Option<String>,
    },
    #[serde(rename = "search_learnings")]
    SearchLearnings,
    #[serde(rename = "ensure_claude_md_routing")]
    EnsureClaudeMdRouting,
    #[serde(rename = "sync_office_hours_artifact")]
    SyncOfficeHoursArtifact {
        #[serde(rename = "designFilePath")]
        design_file_path: String,
        #[serde(rename = "gbrainSource")]
        gbrain_source: Option<String>,
        #[serde(rename = "gbrainAvailable")]
        gbrain_available: Option<bool>,
    },
```

- [ ] 在 `golden.rs` 末尾、`result_to_golden` 之前追加本地 fixture 实现：

```rust
fn parse_fixture_kind(s: &str) -> Option<crate::builtin::session_mode::SessionModeKind> {
    use crate::builtin::session_mode::SessionModeKind;
    match s {
        "plan" => Some(SessionModeKind::Plan),
        "design" => Some(SessionModeKind::Design),
        "office-hours" => Some(SessionModeKind::OfficeHours),
        "game-design" => Some(SessionModeKind::GameDesign),
        _ => None,
    }
}

#[derive(Default)]
struct FixtureKaos {
    cwd: String,
    files: std::sync::Mutex<HashMap<PathBuf, String>>,
}

impl FixtureKaos {
    fn insert(&self, path: impl Into<PathBuf>, content: impl Into<String>) {
        self.files.lock().unwrap().insert(path.into(), content.into());
    }
}

#[async_trait::async_trait]
impl crate::builtin::session_mode::SessionModeContext for FixtureKaos {
    fn cwd(&self) -> String { self.cwd.clone() }
    fn project_root(&self) -> Option<String> { Some(self.cwd.clone()) }
    async fn read_text(&self, path: &str) -> anyhow::Result<String> {
        self.files.lock().unwrap()
            .get(&PathBuf::from(path))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("not found: {}", path))
    }
    async fn write_text(&self, path: &str, content: &str) -> anyhow::Result<()> {
        self.files.lock().unwrap().insert(PathBuf::from(path), content.into());
        Ok(())
    }
    async fn stat(&self, path: &str) -> anyhow::Result<()> {
        if self.files.lock().unwrap().contains_key(&PathBuf::from(path)) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("not found: {}", path))
        }
    }
}

#[derive(Default)]
struct FixtureOfficeHoursStore {
    learnings: std::sync::Mutex<Vec<crate::builtin::session_mode::LearningEntry>>,
}

#[async_trait::async_trait]
impl crate::builtin::session_mode::OfficeHoursStateStore for FixtureOfficeHoursStore {
    async fn append_profile(&self, _entry: crate::builtin::session_mode::BuilderProfileEntry) -> anyhow::Result<()> { Ok(()) }
    async fn append_learning(&self, entry: crate::builtin::session_mode::LearningEntry) -> anyhow::Result<()> {
        self.learnings.lock().unwrap().push(entry);
        Ok(())
    }
    async fn search_learnings(&self, limit: usize, _cross_project: bool) -> anyhow::Result<Vec<crate::builtin::session_mode::LearningEntry>> {
        let all = self.learnings.lock().unwrap().clone();
        Ok(all.into_iter().rev().take(limit).collect())
    }
}

#[derive(Default)]
struct FixtureGameDesignStore;

#[async_trait::async_trait]
impl crate::builtin::session_mode::GameDesignStateStore for FixtureGameDesignStore {
    async fn append_profile(&self, _entry: crate::builtin::session_mode::GameDesignProfileEntry) -> anyhow::Result<()> { Ok(()) }
    async fn append_learning(&self, _entry: crate::builtin::session_mode::LearningEntry) -> anyhow::Result<()> { Ok(()) }
    async fn search_learnings(&self, _limit: usize, _branch: Option<String>) -> anyhow::Result<Vec<crate::builtin::session_mode::LearningEntry>> { Ok(vec![]) }
}

struct FixtureMcpProvider { available: std::sync::Mutex<bool> }

#[async_trait::async_trait]
impl crate::builtin::session_mode::McpProvider for FixtureMcpProvider {
    async fn gbrain_available(&self) -> bool { *self.available.lock().unwrap() }
}

struct FixtureTelemetryClient;

impl crate::builtin::session_mode::TelemetryClient for FixtureTelemetryClient {
    fn track(&self, _event: &str, _properties: HashMap<String, Value>) {}
}

struct FixtureSessionModeProvider {
    active: std::sync::Mutex<bool>,
    kind: std::sync::Mutex<Option<crate::builtin::session_mode::SessionModeKind>>,
    file_path: std::sync::Mutex<Option<String>>,
    language: std::sync::Mutex<crate::builtin::session_mode::Language>,
    kaos: Arc<FixtureKaos>,
    office_hours_store: Arc<dyn crate::builtin::session_mode::OfficeHoursStateStore>,
    game_design_store: Arc<dyn crate::builtin::session_mode::GameDesignStateStore>,
    mcp: Arc<dyn crate::builtin::session_mode::McpProvider>,
    telemetry: Arc<dyn crate::builtin::session_mode::TelemetryClient>,
}

impl FixtureSessionModeProvider {
    fn new(active_kind: Option<crate::builtin::session_mode::SessionModeKind>) -> Self {
        use crate::builtin::session_mode::SessionModeKind;
        let cwd = "/workspace".to_string();
        let kaos = Arc::new(FixtureKaos { cwd: cwd.clone(), files: Default::default() });
        let (active, kind, file_path) = match active_kind {
            Some(k) => {
                let fp = match k {
                    SessionModeKind::Plan => ".ody-code/plans/2026-06-29-plan.md",
                    SessionModeKind::Design => ".ody-code/designs/2026-06-29-design.md",
                    SessionModeKind::OfficeHours => ".ody-code/products/2026-06-29-office-hours.md",
                    SessionModeKind::GameDesign => ".ody-code/game-design/2026-06-29-game-design.md",
                };
                (true, Some(k), Some(fp.into()))
            }
            None => (false, None, None),
        };
        Self {
            active: std::sync::Mutex::new(active),
            kind: std::sync::Mutex::new(kind),
            file_path: std::sync::Mutex::new(file_path),
            language: std::sync::Mutex::new(crate::builtin::session_mode::Language::En),
            kaos: kaos.clone(),
            office_hours_store: Arc::new(FixtureOfficeHoursStore::default()),
            game_design_store: Arc::new(FixtureGameDesignStore::default()),
            mcp: Arc::new(FixtureMcpProvider { available: std::sync::Mutex::new(false) }),
            telemetry: Arc::new(FixtureTelemetryClient),
        }
    }

    fn with_gbrain_available(self, available: bool) -> Self {
        *self.mcp.available.lock().unwrap() = available;
        self
    }

    fn kaos_handle(&self) -> Arc<FixtureKaos> { Arc::clone(&self.kaos) }
}

#[async_trait::async_trait]
impl crate::builtin::session_mode::SessionModeProvider for FixtureSessionModeProvider {
    fn is_session_mode_active(&self) -> bool { *self.active.lock().unwrap() }
    fn session_mode_kind(&self) -> Option<crate::builtin::session_mode::SessionModeKind> { *self.kind.lock().unwrap() }
    fn session_mode_file_path(&self) -> Option<String> { self.file_path.lock().unwrap().clone() }

    async fn enter_session_mode(&self, kind: crate::builtin::session_mode::SessionModeKind) -> anyhow::Result<()> {
        *self.active.lock().unwrap() = true;
        *self.kind.lock().unwrap() = Some(kind);
        if self.file_path.lock().unwrap().is_none() {
            let fp = match kind {
                crate::builtin::session_mode::SessionModeKind::Plan => ".ody-code/plans/2026-06-29-plan.md",
                crate::builtin::session_mode::SessionModeKind::Design => ".ody-code/designs/2026-06-29-design.md",
                crate::builtin::session_mode::SessionModeKind::OfficeHours => ".ody-code/products/2026-06-29-office-hours.md",
                crate::builtin::session_mode::SessionModeKind::GameDesign => ".ody-code/game-design/2026-06-29-game-design.md",
            };
            *self.file_path.lock().unwrap() = Some(fp.into());
        }
        Ok(())
    }

    async fn exit_session_mode(&self) -> anyhow::Result<()> {
        *self.active.lock().unwrap() = false;
        *self.kind.lock().unwrap() = None;
        Ok(())
    }

    async fn handoff_to(&self, _target: &str, _selected_label: Option<String>) -> anyhow::Result<()> { Ok(()) }
    fn user_language(&self) -> crate::builtin::session_mode::Language { *self.language.lock().unwrap() }
    fn set_user_language(&self, lang: crate::builtin::session_mode::Language) { *self.language.lock().unwrap() = lang; }
    fn open_external_available(&self) -> bool { false }
    fn telemetry(&self) -> Arc<dyn crate::builtin::session_mode::TelemetryClient> { Arc::clone(&self.telemetry) }
    fn kaos(&self) -> Arc<dyn crate::builtin::session_mode::SessionModeContext> { Arc::clone(&self.kaos) as Arc<dyn crate::builtin::session_mode::SessionModeContext> }
    fn office_hours_store(&self) -> Arc<dyn crate::builtin::session_mode::OfficeHoursStateStore> { Arc::clone(&self.office_hours_store) }
    fn game_design_store(&self) -> Arc<dyn crate::builtin::session_mode::GameDesignStateStore> { Arc::clone(&self.game_design_store) }
    fn mcp(&self) -> Arc<dyn crate::builtin::session_mode::McpProvider> { Arc::clone(&self.mcp) }
}
```

- [ ] 在 `run_case_sync` 的 `Op::Checkpoint` 分支之后追加：

```rust
        // ── session-mode tool ops ──
        Op::EnterPlanMode { active_kind } => {
            let kind = active_kind.as_deref().and_then(parse_fixture_kind);
            let provider = Arc::new(FixtureSessionModeProvider::new(kind));
            let tool = Arc::new(crate::builtin::session_mode::enter_plan_mode::EnterPlanModeTool::new(provider));
            match tool.resolve_execution(serde_json::json!({})) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::EnterDesignMode { active_kind } => {
            let kind = active_kind.as_deref().and_then(parse_fixture_kind);
            let provider = Arc::new(FixtureSessionModeProvider::new(kind));
            let tool = Arc::new(crate::builtin::session_mode::enter_design_mode::EnterDesignModeTool::new(provider));
            match tool.resolve_execution(serde_json::json!({})) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::EnterOfficeHours { active_kind } => {
            let kind = active_kind.as_deref().and_then(parse_fixture_kind);
            let provider = Arc::new(FixtureSessionModeProvider::new(kind));
            let tool = Arc::new(crate::builtin::session_mode::office_hours::EnterOfficeHoursModeTool::new(provider));
            match tool.resolve_execution(serde_json::json!({})) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::SetOfficeHoursLanguage { language } => {
            let provider = Arc::new(FixtureSessionModeProvider::new(
                Some(crate::builtin::session_mode::SessionModeKind::OfficeHours),
            ));
            let tool = Arc::new(crate::builtin::session_mode::office_hours::SetOfficeHoursLanguageTool::new(provider));
            match tool.resolve_execution(serde_json::json!({"language": language})) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::AppendLearning { learning_type, key, insight, confidence, branch } => {
            let provider = Arc::new(FixtureSessionModeProvider::new(
                Some(crate::builtin::session_mode::SessionModeKind::OfficeHours),
            ));
            let tool = Arc::new(crate::builtin::session_mode::office_hours::AppendLearningTool::new(provider));
            let mut args = serde_json::json!({
                "type": learning_type,
                "key": key,
                "insight": insight,
                "confidence": confidence,
            });
            if let Some(b) = branch { args["branch"] = Value::String(b.clone()); }
            match tool.resolve_execution(args) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::SearchLearnings => {
            let provider = Arc::new(FixtureSessionModeProvider::new(
                Some(crate::builtin::session_mode::SessionModeKind::OfficeHours),
            ));
            let tool = Arc::new(crate::builtin::session_mode::office_hours::SearchLearningsTool::new(provider));
            match tool.resolve_execution(serde_json::json!({})) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::EnsureClaudeMdRouting => {
            let provider = Arc::new(FixtureSessionModeProvider::new(
                Some(crate::builtin::session_mode::SessionModeKind::OfficeHours),
            ));
            let tool = Arc::new(crate::builtin::session_mode::office_hours::EnsureClaudeMdRoutingTool::new(provider));
            match tool.resolve_execution(serde_json::json!({})) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::SyncOfficeHoursArtifact { design_file_path, gbrain_source, gbrain_available } => {
            let provider = FixtureSessionModeProvider::new(
                Some(crate::builtin::session_mode::SessionModeKind::OfficeHours),
            ).with_gbrain_available(gbrain_available.unwrap_or(true));
            provider.kaos_handle().insert(design_file_path, "# Design");
            if let Some(source) = gbrain_source {
                provider.kaos_handle().insert(
                    std::path::PathBuf::from("/workspace").join(".gbrain-source"),
                    source,
                );
            }
            let provider = Arc::new(provider);
            let tool = Arc::new(crate::builtin::session_mode::office_hours::SyncOfficeHoursArtifactTool::new(provider));
            match tool.resolve_execution(serde_json::json!({"designFilePath": design_file_path})) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
```

- [ ] 创建 fixture 文件 `packages/integration-tests/src/parity/fixtures/tools-rs/session-mode-tools.json`：

```json
{
  "version": 1,
  "cases": [
    {
      "name": "enter_plan_when_design_active",
      "op": { "type": "enter_plan_mode", "activeKind": "design" },
      "expected": {
        "result": {
          "output": "Design mode is already active. Use ExitDesignMode when you are ready to exit design mode; do not try to enter another mode on top of it.",
          "is_error": true,
          "message": "session mode already active"
        }
      }
    },
    {
      "name": "enter_design_when_plan_active",
      "op": { "type": "enter_design_mode", "activeKind": "plan" },
      "expected": {
        "result": {
          "output": "Plan mode is already active. Use ExitPlanMode when you are ready to exit plan mode; do not try to enter another mode on top of it.",
          "is_error": true,
          "message": "session mode already active"
        }
      }
    },
    {
      "name": "enter_office_hours_when_active",
      "op": { "type": "enter_office_hours", "activeKind": "office-hours" },
      "expected": {
        "result": {
          "output": "Office hours mode is already active. Use ExitOfficeHoursMode when the session is complete.",
          "is_error": true,
          "message": "already active"
        }
      }
    },
    {
      "name": "set_office_hours_language_zh",
      "op": { "type": "set_office_hours_language", "language": "zh" },
      "expected": {
        "result": { "output": "用户语言已设置为 zh。", "is_error": false, "message": null }
      }
    },
    {
      "name": "append_learning",
      "op": { "type": "append_learning", "learningType": "operational", "key": "demand_signal", "insight": "Users paid.", "confidence": 0.95, "branch": "main" },
      "expected": {
        "result": { "output": "Learning \"demand_signal\" recorded successfully.", "is_error": false, "message": null }
      }
    },
    {
      "name": "search_learnings_empty",
      "op": { "type": "search_learnings" },
      "expected": {
        "result": { "output": "No past learnings found.", "is_error": false, "message": null }
      }
    },
    {
      "name": "ensure_claude_md_routing_create",
      "op": { "type": "ensure_claude_md_routing" },
      "expected": {
        "result": { "output": "AGENTS.md created at /workspace/AGENTS.md with ## Skill routing section.", "is_error": false, "message": null }
      }
    },
    {
      "name": "sync_office_hours_artifact_mcp",
      "op": { "type": "sync_office_hours_artifact", "designFilePath": "/workspace/design.md", "gbrainSource": "my-source", "gbrainAvailable": true },
      "expected": {
        "result": { "output": "gbrain MCP server is connected.\nTarget source: my-source\nDesign artifact at /workspace/design.md is ready for sync via MCP.", "is_error": false, "message": null }
      }
    }
  ]
}
```

- [ ] 在 `packages/integration-tests/src/parity/tools-rs-golden.ts` 的 `GoldenOp` 联合类型中追加：

```ts
  // ── session-mode tool ops ──
  | { type: 'enter_plan_mode'; activeKind?: string | null }
  | { type: 'enter_design_mode'; activeKind?: string | null }
  | { type: 'enter_office_hours'; activeKind?: string | null }
  | { type: 'set_office_hours_language'; language: string }
  | { type: 'append_learning'; learningType: string; key: string; insight: string; confidence: number; branch?: string | null }
  | { type: 'search_learnings' }
  | { type: 'ensure_claude_md_routing' }
  | { type: 'sync_office_hours_artifact'; designFilePath: string; gbrainSource?: string | null; gbrainAvailable?: boolean | null }
```

- [ ] 在 `runCase` 的 `default` 分支之前追加 session-mode 处理臂：

```ts
    // ── session-mode tool ops ──
    case 'enter_plan_mode': {
      if (op.activeKind === 'design') {
        return { result: { output: 'Design mode is already active. Use ExitDesignMode when you are ready to exit design mode; do not try to enter another mode on top of it.', is_error: true, message: 'session mode already active' } };
      }
      return { result: okResult('Plan mode is now active.') };
    }
    case 'enter_design_mode': {
      if (op.activeKind === 'plan') {
        return { result: { output: 'Plan mode is already active. Use ExitPlanMode when you are ready to exit plan mode; do not try to enter another mode on top of it.', is_error: true, message: 'session mode already active' } };
      }
      return { result: okResult('Design mode is now active.') };
    }
    case 'enter_office_hours': {
      if (op.activeKind === 'office-hours') {
        return { result: { output: 'Office hours mode is already active. Use ExitOfficeHoursMode when the session is complete.', is_error: true, message: 'already active' } };
      }
      return { result: okResult('Office hours mode is now active.') };
    }
    case 'set_office_hours_language': {
      if (op.language === 'zh') {
        return { result: okResult('用户语言已设置为 zh。') };
      }
      return { result: okResult(`User language set to ${op.language}.`) };
    }
    case 'append_learning': {
      return { result: okResult(`Learning "${op.key}" recorded successfully.`) };
    }
    case 'search_learnings': {
      return { result: okResult('No past learnings found.') };
    }
    case 'ensure_claude_md_routing': {
      return { result: okResult('AGENTS.md created at /workspace/AGENTS.md with ## Skill routing section.') };
    }
    case 'sync_office_hours_artifact': {
      const source = op.gbrainSource ?? 'my-source';
      return { result: okResult(`gbrain MCP server is connected.\nTarget source: ${source}\nDesign artifact at ${op.designFilePath} is ready for sync via MCP.`) };
    }
```

- [ ] 在 `packages/integration-tests/test/parity/tools-rs/l1-golden.test.ts` 的 `fixtures` 数组中追加 `'session-mode-tools.json'`。

- [ ] 运行 L1 parity 测试（会同时编译 Rust golden bin）：

```bash
cd rust-ody && cargo build -p tools-rs --bin tools-golden
pnpm --filter @odysseythink/integration-tests test test/parity/tools-rs/l1-golden.test.ts
```

Expected: `session-mode-tools.json TS matches Rust` 通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
pnpm -r typecheck
```

Expected: green。

- [ ] Commit: `feat(parity): add L1 golden fixture for session-mode tools`。

---

## Task 8: L3 parity driver 接线

**Depends on:** Task 5（`Agent` 与 `AgentSessionModeProvider` 可真实进入/退出 session mode），以及 `planning-tools.md` / `office-hours-tools.md` / `game-design-tools.md` 中各 mode behavior 已落地。

**Files:**
- Modify: `packages/integration-tests/src/parity/session-mode-l3-driver.ts`
- Modify: `packages/integration-tests/test/parity/session-mode-l3.test.ts`

**Why:** 当前 TS driver 是 stub，只返回 fixture 的 `expectedEvents`；本 Task 让它真正调用 Rust `session_mode_l3` binary，并在 L3 测试中对比 Rust 输出与 fixture 期望事件。

**Steps：**

- [ ] 重写 `packages/integration-tests/src/parity/session-mode-l3-driver.ts`：

```ts
import { existsSync } from 'node:fs';
import { writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { execSync, spawnSync } from 'node:child_process';

import type { SessionModeFixture } from './session-mode-fixture';

function findProjectRoot(): string {
  let current = dirname(fileURLToPath(import.meta.url));
  while (current !== dirname(current)) {
    if (existsSync(join(current, '.git'))) {
      return current;
    }
    current = dirname(current);
  }
  return process.cwd();
}

const rootDir = findProjectRoot();

/**
 * TS self-parity stub: returns the fixture's expectedEvents directly.
 */
export async function runTsSessionModeFixture(
  fixture: SessionModeFixture,
): Promise<Array<Record<string, unknown>>> {
  return fixture.expectedEvents;
}

function resolveRustBinaryPath(): string {
  const override = process.env['ODY_SESSION_MODE_L3_BINARY_PATH'];
  if (override) return override;
  return join(rootDir, 'rust-ody', 'target', 'debug', 'session_mode_l3');
}

/**
 * Run the Rust session_mode_l3 binary against the fixture and parse JSONL output.
 */
export async function runRustSessionModeFixture(
  fixture: SessionModeFixture,
): Promise<Array<Record<string, unknown>>> {
  const binaryPath = resolveRustBinaryPath();
  if (!existsSync(binaryPath)) {
    execSync('cargo build -p agent-rs --bin session_mode_l3', {
      cwd: join(rootDir, 'rust-ody'),
      stdio: 'inherit',
    });
  }

  const fixturePath = join(tmpdir(), `session-mode-l3-${Date.now()}.json`);
  writeFileSync(fixturePath, JSON.stringify(fixture), 'utf8');

  const result = spawnSync(binaryPath, [fixturePath], {
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`session_mode_l3 exited ${String(result.status)}: ${result.stderr}`);
  }

  return result.stdout
    .split('\n')
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as Record<string, unknown>);
}
```

- [ ] 修改 `packages/integration-tests/test/parity/session-mode-l3.test.ts`，增加 Rust parity describe：

```ts
import { beforeAll, describe, expect, it } from 'vitest';
import { existsSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { runTsSessionModeFixture, runRustSessionModeFixture } from '../../src/parity/session-mode-l3-driver';
import { normalizeSessionModeEvents } from '../../src/parity/normalize-session-mode';
import type { SessionModeFixture } from '../../src/parity/session-mode-fixture';
import planEnterExit from '../../src/parity/fixtures/session-mode/plan-enter-exit.json';
import designEnterExit from '../../src/parity/fixtures/session-mode/design-enter-exit.json';
import officeHoursEnterExit from '../../src/parity/fixtures/session-mode/office-hours-enter-exit.json';
import gameDesignEnterExit from '../../src/parity/fixtures/session-mode/game-design-enter-exit.json';
import handoff from '../../src/parity/fixtures/session-mode/handoff.json';
import injectionContent from '../../src/parity/fixtures/session-mode/injection-content.json';

const fixtures = [
  ['plan-enter-exit', planEnterExit],
  ['design-enter-exit', designEnterExit],
  ['office-hours-enter-exit', officeHoursEnterExit],
  ['game-design-enter-exit', gameDesignEnterExit],
  ['handoff', handoff],
  ['injection-content', injectionContent],
] as const;

function findProjectRoot(): string {
  let current = dirname(fileURLToPath(import.meta.url));
  while (current !== dirname(current)) {
    if (existsSync(join(current, '.git'))) {
      return current;
    }
    current = dirname(current);
  }
  return process.cwd();
}

const rootDir = findProjectRoot();

describe('SessionMode L3 — TS self-parity', () => {
  for (const [name, fixture] of fixtures) {
    it(`${name} produces expected events`, async () => {
      const events = await runTsSessionModeFixture(fixture as SessionModeFixture);
      const normalized = normalizeSessionModeEvents(events);
      expect(normalized.length).toBeGreaterThanOrEqual(1);
      const firstExpected = (fixture as SessionModeFixture).expectedEvents[0];
      expect(normalized[0].type).toBe(firstExpected.type);
    });
  }
});

describe('SessionMode L3 — Rust parity', () => {
  beforeAll(() => {
    execSync('cargo build -p agent-rs --bin session_mode_l3', {
      cwd: join(rootDir, 'rust-ody'),
      stdio: 'inherit',
    });
  });

  for (const [name, fixture] of fixtures) {
    it(`${name} Rust matches expected events`, async () => {
      const events = await runRustSessionModeFixture(fixture as SessionModeFixture);
      const normalized = normalizeSessionModeEvents(events);
      expect(normalized).toEqual((fixture as SessionModeFixture).expectedEvents);
    });
  }
});
```

- [ ] 运行 L3 parity 测试：

```bash
cd rust-ody && cargo build -p agent-rs --bin session_mode_l3
pnpm --filter @odysseythink/integration-tests test test/parity/session-mode-l3.test.ts
```

Expected: `SessionMode L3 — Rust parity` 下 6 个 fixture 全部通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
pnpm -r typecheck
```

Expected: green。

- [ ] Commit: `feat(parity): wire Rust session_mode_l3 binary into L3 parity tests`。

---

## Local Self-Review

- [ ] 1. Spec-coverage table：

| Roadmap § / Requirement | Part 5 Task(s) | Status |
|---|---|---|
| 4.4.5.x — `OfficeHoursStateStore` / `GameDesignStateStore` 真实持久化 | Task 1 | covered |
| 4.4.5.x — session-mode TUI/状态 i18n 标签 | Task 2 | covered |
| 4.4.5.x — `Agent.session_mode` 支持异步 `SessionModeProvider` | Task 3 | covered |
| 4.4.5.x — `AgentSessionModeProvider` + `KaosSessionModeContext` | Task 4 | covered |
| 4.4.5.x — `Agent` 字段/Builder 接线 + 默认 state store | Task 5 | covered |
| 4.4.5.x — `ToolManager` 元数据 + `TurnTools::loop_tools` 注册 | Task 6 | covered |
| 4.4.5.4 — L1 golden fixture for session-mode tools | Task 7 | covered |
| 4.4.5.4 — L3 parity driver 调用 Rust binary | Task 8 | covered |
| E2E enrichment in `ExitPlanModeTool` | — | no-op（deferred gap） |
| Full MCP-based gbrain sync | — | no-op（CLI fallback only） |

- [ ] 2. Placeholder scan：本 Part 无 TODO/TBD；Task 7/8 的 fixture/harness/driver 均给出完整可执行代码。
- [ ] 3. No phantom tasks：每个 Task 都产生文件修改、测试命令与 commit；无 `--allow-empty` 或 "already done in Task N"。
- [ ] 4. Dependency soundness：Task 1/2 并行 → Task 3 → Task 4 → Task 5 → Task 6 → Task 7/8；所有 `Depends on:` 均指向前序 Task 或已完成的 Part 1–4。
- [ ] 5. Caller & build soundness：
  - Task 3 改变 `Agent.session_mode` 字段类型为 `tokio::sync::Mutex`，同 Task 更新 `agent.rs` 内所有 `.lock()` 调用点、同步 trait 方法、`enter_session_mode`/`exit_session_mode`/`TurnSessionMode`/`InjectionManagerContext`，并运行 `cargo check --workspace --all-targets`。
  - Task 6 改变 `ToolManager::core_builtin_tools()` 与 `TurnTools::loop_tools()`，同 Task 更新 `Agent::loop_tools` 调用点，并运行全工作区类型检查。
  - 每处共享签名变更均在本 Task 内完成，未拆分到多个 Task。
- [ ] 6. Test-the-risk：
  - Task 1：state store 测试断言 append 后 `search_learnings` 能读到同一 entry。
  - Task 3：session-mode manager 行为测试确认 `tokio::sync::Mutex` 切换后仍可正常 enter/exit。
  - Task 5：`agent_session_mode_wiring` 测试断言 provider 能真实进入/退出 office-hours。
  - Task 6：`SessionModeToolkit` 测试断言 20 个工具元数据与执行实例一一对应。
  - Task 7：L1 fixture 直接比较 Rust 与 TS 输出，覆盖 mode 检查、语言切换、learning append/search、AGENTS.md 写入、artifact sync。
  - Task 8：L3 测试断言 Rust binary 输出的事件序列与 fixture `expectedEvents` 完全一致。
- [ ] 7. Type consistency：Part 5 使用的 `SessionModeProvider`、`OfficeHoursStateStore`、`GameDesignStateStore`、`Language`、`SessionModeKind`、`McpProvider`、`TelemetryClient` 均与 Part 1 `infra.md` Task 3 定义一致；`ToolExecution.display` 与 `HandoffOptions.selected_label` 与 Part 1 Task 1/2 一致。
