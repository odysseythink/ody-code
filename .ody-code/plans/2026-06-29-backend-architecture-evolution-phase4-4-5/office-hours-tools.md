# Part 3 — Office-Hours 工具集（8 个内置工具）

**Goal:** 将 TypeScript 的 8 个 office-hours 会话模式工具迁移到 Rust 的 `tools-rs` 中，使其在 `agent-rs` 侧通过 `SessionModeProvider` trait 解耦，行为与 TS 实现等价。

**Architecture:** 所有 office-hours 工具集中在 `tools-rs/src/builtin/session_mode/office_hours.rs` 一个文件中，包含本地化字符串表、入口提示语、工具实现与测试。工具只依赖 Part 1 定义的 `SessionModeProvider` 及其卫星 trait（`OfficeHoursStateStore`、`McpProvider`、`SessionModeContext`、`TelemetryClient`），不直接引用 `agent-rs`。测试使用本 Part 自包含的 `MockSessionModeProvider`，保证 Part 2–4 之间互不依赖。

**Tech stack:** Rust (`tools-rs`), `serde_json`, `async-trait`, `chrono`（生成 ISO 时间戳）。

**Depends on:** `2026-06-29-backend-architecture-evolution-phase4-4/infra.md`（Task 3，trait 表面已存在）。

> For executing workers: implement this plan task-by-task（建议每个 Task 用一个新 subagent/Task，避免单会话退化）。步骤使用 - [ ] 复选框跟踪。

---

## File Structure

| File | Responsibility |
|---|---|
| `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs` | Office-hours 本地化字符串、`office_hours_entry_reminder`、8 个工具 struct、共享 mock provider、所有内联测试。 |
| `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs` | 暴露 `pub mod office_hours;`（Part 1 已预留，本 Part 填充实现）。 |
| `rust-ody/crates/agent-rs/src/tool/manager.rs` | 在 `core_builtin_tools()` / `loop_tools()` 中注册 8 个 office-hours 工具（Part 5 最终接线；本 Part 只确保工具自身可编译、可测试）。 |

---

## Dependency Overview

```
Task 1: 本地化字符串表 + office-hours 入口提示语
  │
  ├──► Task 2: EnterOfficeHoursModeTool
  ├──► Task 3: ExitOfficeHoursModeTool
  ├──► Task 4: SetOfficeHoursLanguageTool
  ├──► Task 5: AppendLearningTool
  ├──► Task 6: AppendBuilderProfileTool
  ├──► Task 7: SearchLearningsTool
  ├──► Task 8: EnsureClaudeMdRoutingTool
  └──► Task 9: SyncOfficeHoursArtifactTool
```

所有工具共享 Task 1 的字符串表与入口提示语；每个工具单独成 Task，保持 2–5 分钟级别的可提交粒度。Mock provider 在 Task 2 中引入，后续 Task 的测试直接复用。

---

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| TS `officeHoursEntryReminder` 是一份完整 workflow contract，Rust 端口需逐字对齐。 | 将 contract 文本作为 `const OFFICE_HOURS_ENTRY_REMINDER: &str` 完整嵌入 `office_hours.rs`，并添加断言检查关键 marker。 |
| `AppendLearningTool` / `AppendBuilderProfileTool` / `SearchLearningsTool` 依赖状态存储，Part 1 trait 只有方法签名。 | 测试使用本 Part 自包含的 `MockOfficeHoursStateStore`，记录所有 append 操作并支持查询。 |
| `SyncOfficeHoursArtifactTool` 需要 MCP gbrain 检测与 CLI fallback。 | `McpProvider::gbrain_available()` 提供检测能力；CLI fallback 使用 `std::process::Command` 调用 `gbrain`，与 TS `execFileSync` 等价。 |
| 本地化字符串需要与 TS `translations.ts` 保持一致。 | 端口 exact key/value；`SetOfficeHoursLanguageTool` 只接受 `en`/`zh`，与 TS `isSupportedLanguage` 一致。 |
| 8 个工具写在一个文件里会过长。 | 使用紧凑的 struct-per-tool 风格，公共逻辑（mode 检查、错误包装）抽取为 `require_office_hours!` 与 `wrap_error` 两个小宏/函数。 |

---

## Task 1: 本地化字符串表与 office-hours 入口提示语

**Depends on:** `infra.md` Task 3（`SessionModeProvider`、`Language`、`OfficeHoursStateStore` 等 trait 已定义）。

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs`（将 `pub mod office_hours;` 从 stub 变为真实模块）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`（内联 `#[cfg(test)]`）

**Why:** 后续 8 个工具都依赖同一套 `officeHours.*` 本地化字符串与入口提示语。先一次性端口这些静态内容，保证文案与 TS 完全一致，并在此 Task 中就通过测试锁定 key/value。

**Steps:**

- [ ] 创建 `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`，先写入本地化与入口提示语骨架：

```rust
use std::sync::Arc;
use async_trait::async_trait;
use serde_json::Value;

use crate::builtin::session_mode::{
    BuilderProfileEntry, Language, LearningEntry, McpProvider, OfficeHoursStateStore,
    SessionModeContext, SessionModeKind, SessionModeProvider, TelemetryClient,
};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};

/// Office-hours 本地化字符串表，与 TS `translations.ts` 逐 key 对齐。
fn office_hours_t(key: &str, lang: Language) -> String {
    match (key, lang) {
        ("officeHours.entered", Language::En) => "Office hours mode is now active.".into(),
        ("officeHours.alreadyActive", Language::En) => "Office hours mode is already active. Use ExitOfficeHoursMode when the session is complete.".into(),
        ("officeHours.anotherModeActive", Language::En) => "Another session mode is already active. Exit it first before entering office hours mode.".into(),
        ("officeHours.failedToEnter", Language::En) => "Failed to enter office hours mode: {message}".into(),
        ("officeHours.sessionComplete", Language::En) => "Office hours session complete.".into(),
        ("officeHours.designDocSaved", Language::En) => "Design document saved to: {path}".into(),
        ("officeHours.appWillExit", Language::En) => "The application will now exit.".into(),
        ("officeHours.profileAppended", Language::En) => "Builder profile entry appended successfully. Session count will be updated for next tier computation.".into(),
        ("officeHours.learningRecorded", Language::En) => "Learning \"{key}\" recorded successfully.".into(),
        ("officeHours.noLearnings", Language::En) => "No past learnings found.".into(),
        ("officeHours.learningsHeader", Language::En) => "Found {count} learning(s):".into(),
        ("officeHours.learningTypeLabel", Language::En) => "Type".into(),
        ("officeHours.learningInsightLabel", Language::En) => "Insight".into(),
        ("officeHours.learningConfidenceLabel", Language::En) => "Confidence".into(),
        ("officeHours.learningDateLabel", Language::En) => "Date".into(),
        ("officeHours.learningBranchLabel", Language::En) => "Branch".into(),
        ("officeHours.modeNotActive", Language::En) => "Office hours mode is not active.".into(),
        ("officeHours.designFileNotFound", Language::En) => "Design file not found at {path}.".into(),
        ("officeHours.gbrainConnected", Language::En) => "gbrain MCP server is connected.".into(),
        ("officeHours.gbrainTargetSource", Language::En) => "Target source: {source}".into(),
        ("officeHours.gbrainNoSourcePin", Language::En) => "No .gbrain-source pin found.".into(),
        ("officeHours.gbrainReadyForSync", Language::En) => "Design artifact at {path} is ready for sync via MCP.".into(),
        ("officeHours.gbrainSynced", Language::En) => "Design artifact synced via gbrain CLI.".into(),
        ("officeHours.gbrainFile", Language::En) => "File: {path}".into(),
        ("officeHours.gbrainCliFailed", Language::En) => "gbrain CLI sync failed: {message}. Ensure the gbrain CLI is installed and configured.".into(),
        ("officeHours.agentsMdCreated", Language::En) => "AGENTS.md created at {path} with ## Skill routing section.".into(),
        ("officeHours.agentsMdUpdated", Language::En) => "Appended ## Skill routing section to AGENTS.md at {path}.".into(),
        ("officeHours.agentsMdAlreadyHasRouting", Language::En) => "AGENTS.md already has a ## Skill routing section — no changes needed.".into(),
        ("officeHours.failedToEnsureRouting", Language::En) => "Failed to ensure AGENTS.md routing: {message}".into(),
        ("officeHours.failedToSyncArtifact", Language::En) => "Failed to sync design artifact: {message}".into(),
        ("officeHours.languageSet", Language::En) => "User language set to {language}.".into(),

        ("officeHours.entered", Language::Zh) => "Office Hours 模式已激活。".into(),
        ("officeHours.alreadyActive", Language::Zh) => "Office Hours 模式已经处于激活状态。会话结束后请调用 ExitOfficeHoursMode。".into(),
        ("officeHours.anotherModeActive", Language::Zh) => "另一个会话模式已经激活。请先退出该模式再进入 Office Hours。".into(),
        ("officeHours.failedToEnter", Language::Zh) => "进入 Office Hours 模式失败：{message}".into(),
        ("officeHours.sessionComplete", Language::Zh) => "Office Hours 会话已结束。".into(),
        ("officeHours.designDocSaved", Language::Zh) => "设计文档已保存至：{path}".into(),
        ("officeHours.appWillExit", Language::Zh) => "应用即将退出。".into(),
        ("officeHours.profileAppended", Language::Zh) => "Builder 档案条目已追加成功。下次层级计算时将更新会话计数。".into(),
        ("officeHours.learningRecorded", Language::Zh) => "学习洞察 \"{key}\" 已记录成功。".into(),
        ("officeHours.noLearnings", Language::Zh) => "未找到过往学习洞察。".into(),
        ("officeHours.learningsHeader", Language::Zh) => "找到 {count} 条学习洞察：".into(),
        ("officeHours.learningTypeLabel", Language::Zh) => "类型".into(),
        ("officeHours.learningInsightLabel", Language::Zh) => "洞察".into(),
        ("officeHours.learningConfidenceLabel", Language::Zh) => "置信度".into(),
        ("officeHours.learningDateLabel", Language::Zh) => "日期".into(),
        ("officeHours.learningBranchLabel", Language::Zh) => "分支".into(),
        ("officeHours.modeNotActive", Language::Zh) => "Office Hours 模式未激活。".into(),
        ("officeHours.designFileNotFound", Language::Zh) => "在 {path} 未找到设计文件。".into(),
        ("officeHours.gbrainConnected", Language::Zh) => "gbrain MCP 服务器已连接。".into(),
        ("officeHours.gbrainTargetSource", Language::Zh) => "目标源：{source}".into(),
        ("officeHours.gbrainNoSourcePin", Language::Zh) => "未找到 .gbrain-source 固定文件。".into(),
        ("officeHours.gbrainReadyForSync", Language::Zh) => "{path} 处的设计制品已准备好通过 MCP 同步。".into(),
        ("officeHours.gbrainSynced", Language::Zh) => "设计制品已通过 gbrain CLI 同步。".into(),
        ("officeHours.gbrainFile", Language::Zh) => "文件：{path}".into(),
        ("officeHours.gbrainCliFailed", Language::Zh) => "gbrain CLI 同步失败：{message}。请确保 gbrain CLI 已安装并配置。".into(),
        ("officeHours.agentsMdCreated", Language::Zh) => "已在 {path} 创建 AGENTS.md，并添加 ## Skill routing 章节。".into(),
        ("officeHours.agentsMdUpdated", Language::Zh) => "已在 {path} 的 AGENTS.md 中追加 ## Skill routing 章节。".into(),
        ("officeHours.agentsMdAlreadyHasRouting", Language::Zh) => "AGENTS.md 已包含 ## Skill routing 章节，无需更改。".into(),
        ("officeHours.failedToEnsureRouting", Language::Zh) => "确保 AGENTS.md 路由失败：{message}".into(),
        ("officeHours.failedToSyncArtifact", Language::Zh) => "同步设计制品失败：{message}".into(),
        ("officeHours.languageSet", Language::Zh) => "用户语言已设置为 {language}。".into(),

        _ => key.into(),
    }
}

/// 将 `officeHours.*` key 的占位符 `{name}` 替换为实际值。
fn office_hours_t_replace(key: &str, lang: Language, replacements: &[(&str, &str)]) -> String {
    let mut s = office_hours_t(key, lang);
    for (k, v) in replacements {
        s = s.replace(&format!("{{{}}}", k), v);
    }
    s
}

/// 进入 office-hours 时返回的入口提示语，与 TS `officeHoursEntryReminder` 对齐。
pub fn office_hours_entry_reminder(design_file_path: Option<&str>) -> String {
    let path = design_file_path.unwrap_or("(not yet assigned)");
    [
        "**Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.",
        "",
        "Office hours is now active. Your job is to act as a YC office hours partner —",
        "a sharp, experienced builder who asks hard questions and pushes for clarity.",
        "",
        "## HARD GATES",
        "- Do NOT write code. Your ONLY output is a design document.",
        "- Ask ONE question at a time via AskUserQuestion.",
        &format!("- Design doc location: {}. When you are ready to write it, just call Write to a path under .ody-code/products/ (e.g. .ody-code/products/<YYYY-MM-DD>-<slug>.md). The host assigns/redirects to the canonical path automatically. You are ALREADY in a writing mode — do NOT call EnterDesignMode or EnterPlanMode (they will be rejected).", path),
        "",
        "Follow the workflow phases below. Begin with Phase 1: Context Gathering.",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn office_hours_t_returns_english_by_default() {
        assert_eq!(
            office_hours_t("officeHours.modeNotActive", Language::En),
            "Office hours mode is not active."
        );
    }

    #[test]
    fn office_hours_t_returns_chinese() {
        assert_eq!(
            office_hours_t("officeHours.modeNotActive", Language::Zh),
            "Office Hours 模式未激活。"
        );
    }

    #[test]
    fn office_hours_t_replace_substitutes_placeholders() {
        let s = office_hours_t_replace(
            "officeHours.learningRecorded",
            Language::En,
            &[("key", "demand_signal")],
        );
        assert_eq!(s, "Learning \"demand_signal\" recorded successfully.");
    }

    #[test]
    fn entry_reminder_contains_hard_gates_and_path() {
        let msg = office_hours_entry_reminder(Some(".ody-code/products/2026-06-29-foo.md"));
        assert!(msg.contains("Office hours is now active"));
        assert!(msg.contains("## HARD GATES"));
        assert!(msg.contains(".ody-code/products/2026-06-29-foo.md"));
        assert!(msg.contains("Do NOT call EnterDesignMode or EnterPlanMode"));
    }

    #[test]
    fn entry_reminder_handles_missing_path() {
        let msg = office_hours_entry_reminder(None);
        assert!(msg.contains("(not yet assigned)"));
    }
}
```

- [ ] 在 `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs` 中确认 `pub mod office_hours;` 已存在且指向真实文件（Part 1 已声明，本文件创建后自动生效）。

- [ ] 运行测试并确认通过：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::office_hours::tests
```

Expected: 5 个测试全部通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): port office-hours i18n strings and entry reminder`。

---

## Task 2: 实现 `EnterOfficeHoursModeTool` 并引入测试用 Mock Provider

**Depends on:** Task 1（本地化字符串表与入口提示语已就绪）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`（追加 `EnterOfficeHoursModeTool` 与 `#[cfg(test)]` mock providers）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`

**Why:** `EnterOfficeHoursModeTool` 是 office-hours 工作流的入口。本 Task 同时引入一套自包含的 mock provider（mock kaos、state store、MCP、telemetry），后续 7 个工具的测试直接复用，避免跨 Part 依赖 Part 2 的 mock。

**Steps：**

- [ ] 在 `office_hours.rs` 的 `#[cfg(test)] mod tests` 上方追加 mock providers（这些 mock 仅供本文件测试使用，但会被后续 Task 的测试引用）：

```rust
#[cfg(test)]
mod mock {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    pub struct MockKaos {
        files: Mutex<HashMap<PathBuf, String>>,
        cwd: String,
    }

    impl MockKaos {
        pub fn new(cwd: &str) -> Self {
            Self {
                files: Mutex::new(HashMap::new()),
                cwd: cwd.into(),
            }
        }
        pub fn insert(&self, path: impl Into<PathBuf>, content: impl Into<String>) {
            self.files.lock().unwrap().insert(path.into(), content.into());
        }
    }

    #[async_trait]
    impl SessionModeContext for MockKaos {
        fn cwd(&self) -> String { self.cwd.clone() }
        fn project_root(&self) -> Option<String> { Some(self.cwd.clone()) }
        async fn read_text(&self, path: &str) -> anyhow::Result<String> {
            self.files.lock().unwrap()
                .get(&PathBuf::from(path))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("file not found: {}", path))
        }
        async fn write_text(&self, path: &str, content: &str) -> anyhow::Result<()> {
            self.files.lock().unwrap().insert(PathBuf::from(path), content.into());
            Ok(())
        }
        async fn stat(&self, path: &str) -> anyhow::Result<()> {
            if self.files.lock().unwrap().contains_key(&PathBuf::from(path)) {
                Ok(())
            } else {
                Err(anyhow::anyhow!("file not found: {}", path))
            }
        }
    }

    #[derive(Default)]
    pub struct MockOfficeHoursStore {
        pub learnings: Mutex<Vec<LearningEntry>>,
        pub profiles: Mutex<Vec<BuilderProfileEntry>>,
    }

    #[async_trait]
    impl OfficeHoursStateStore for MockOfficeHoursStore {
        async fn append_profile(&self, entry: BuilderProfileEntry) -> anyhow::Result<()> {
            self.profiles.lock().unwrap().push(entry);
            Ok(())
        }
        async fn append_learning(&self, entry: LearningEntry) -> anyhow::Result<()> {
            self.learnings.lock().unwrap().push(entry);
            Ok(())
        }
        async fn search_learnings(
            &self,
            limit: usize,
            _cross_project: bool,
        ) -> anyhow::Result<Vec<LearningEntry>> {
            let all = self.learnings.lock().unwrap().clone();
            Ok(all.into_iter().rev().take(limit).collect())
        }
    }

    #[derive(Default)]
    pub struct MockGameDesignStore;

    #[async_trait]
    impl GameDesignStateStore for MockGameDesignStore {
        async fn append_profile(&self, _entry: crate::builtin::session_mode::GameDesignProfileEntry) -> anyhow::Result<()> { Ok(()) }
        async fn append_learning(&self, _entry: LearningEntry) -> anyhow::Result<()> { Ok(()) }
        async fn search_learnings(&self, _limit: usize, _branch: Option<String>) -> anyhow::Result<Vec<LearningEntry>> { Ok(vec![]) }
    }

    #[derive(Default)]
    pub struct MockMcpProvider {
        pub gbrain_available: Mutex<bool>,
    }

    #[async_trait]
    impl McpProvider for MockMcpProvider {
        async fn gbrain_available(&self) -> bool { *self.gbrain_available.lock().unwrap() }
    }

    #[derive(Default)]
    pub struct MockTelemetryClient {
        pub events: Mutex<Vec<(String, HashMap<String, Value>)>>,
    }

    impl TelemetryClient for MockTelemetryClient {
        fn track(&self, event: &str, properties: HashMap<String, Value>) {
            self.events.lock().unwrap().push((event.into(), properties));
        }
    }

    #[derive(Clone)]
    pub struct MockSessionModeProvider {
        active: Arc<Mutex<bool>>,
        kind: Arc<Mutex<Option<SessionModeKind>>>,
        file_path: Arc<Mutex<Option<String>>>,
        language: Arc<Mutex<Language>>,
        pub entered: Arc<Mutex<Vec<SessionModeKind>>>,
        pub exited: Arc<Mutex<bool>>,
        pub handoffs: Arc<Mutex<Vec<(String, Option<String>)>>>,
        kaos: Arc<MockKaos>,
        office_hours_store: Arc<dyn OfficeHoursStateStore>,
        game_design_store: Arc<dyn GameDesignStateStore>,
        mcp: Arc<dyn McpProvider>,
        telemetry: Arc<dyn TelemetryClient>,
    }

    impl Default for MockSessionModeProvider {
        fn default() -> Self {
            Self {
                active: Arc::new(Mutex::new(false)),
                kind: Arc::new(Mutex::new(None)),
                file_path: Arc::new(Mutex::new(None)),
                language: Arc::new(Mutex::new(Language::En)),
                entered: Arc::new(Mutex::new(vec![])),
                exited: Arc::new(Mutex::new(false)),
                handoffs: Arc::new(Mutex::new(vec![])),
                kaos: Arc::new(MockKaos::new("/workspace")),
                office_hours_store: Arc::new(MockOfficeHoursStore::default()),
                game_design_store: Arc::new(MockGameDesignStore::default()),
                mcp: Arc::new(MockMcpProvider::default()),
                telemetry: Arc::new(MockTelemetryClient::default()),
            }
        }
    }

    impl MockSessionModeProvider {
        pub fn inactive() -> Self { Self::default() }

        pub fn active(kind: SessionModeKind) -> Self {
            let s = Self::default();
            *s.active.lock().unwrap() = true;
            *s.kind.lock().unwrap() = Some(kind);
            *s.file_path.lock().unwrap() = Some(".ody-code/products/2026-06-29-active.md".into());
            s
        }

        pub fn with_file_path(self, path: impl Into<String>) -> Self {
            *self.file_path.lock().unwrap() = Some(path.into());
            self
        }

        pub fn with_language(self, lang: Language) -> Self {
            *self.language.lock().unwrap() = lang;
            self
        }

        pub fn with_office_hours_store(self, store: Arc<dyn OfficeHoursStateStore>) -> Self {
            self.office_hours_store = store;
            self
        }

        pub fn with_mcp(self, mcp: Arc<dyn McpProvider>) -> Self {
            self.mcp = mcp;
            self
        }

        pub fn with_kaos(self, kaos: Arc<MockKaos>) -> Self {
            self.kaos = kaos;
            self
        }

        pub fn mock_kaos(&self) -> Arc<MockKaos> {
            Arc::clone(&self.kaos)
        }
    }

    #[async_trait]
    impl SessionModeProvider for MockSessionModeProvider {
        fn is_session_mode_active(&self) -> bool { *self.active.lock().unwrap() }
        fn session_mode_kind(&self) -> Option<SessionModeKind> { *self.kind.lock().unwrap() }
        fn session_mode_file_path(&self) -> Option<String> { self.file_path.lock().unwrap().clone() }

        async fn enter_session_mode(&self, kind: SessionModeKind) -> anyhow::Result<()> {
            self.entered.lock().unwrap().push(kind);
            *self.active.lock().unwrap() = true;
            *self.kind.lock().unwrap() = Some(kind);
            if self.file_path.lock().unwrap().is_none() {
                let default_path = match kind {
                    SessionModeKind::OfficeHours => ".ody-code/products/2026-06-29-office-hours.md",
                    SessionModeKind::Plan => ".ody-code/plans/2026-06-29-plan.md",
                    SessionModeKind::Design => ".ody-code/designs/2026-06-29-design.md",
                    SessionModeKind::GameDesign => ".ody-code/game-design/2026-06-29-game-design.md",
                };
                *self.file_path.lock().unwrap() = Some(default_path.into());
            }
            Ok(())
        }

        async fn exit_session_mode(&self) -> anyhow::Result<()> {
            *self.active.lock().unwrap() = false;
            *self.kind.lock().unwrap() = None;
            *self.exited.lock().unwrap() = true;
            Ok(())
        }

        async fn handoff_to(&self, target: &str, selected_label: Option<String>) -> anyhow::Result<()> {
            self.handoffs.lock().unwrap().push((target.into(), selected_label));
            Ok(())
        }

        fn user_language(&self) -> Language { *self.language.lock().unwrap() }
        fn set_user_language(&self, lang: Language) { *self.language.lock().unwrap() = lang; }
        fn open_external_available(&self) -> bool { false }
        fn telemetry(&self) -> Arc<dyn TelemetryClient> { Arc::clone(&self.telemetry) }
        fn kaos(&self) -> Arc<dyn SessionModeContext> { Arc::clone(&self.kaos) as Arc<dyn SessionModeContext> }
        fn office_hours_store(&self) -> Arc<dyn OfficeHoursStateStore> { Arc::clone(&self.office_hours_store) }
        fn game_design_store(&self) -> Arc<dyn GameDesignStateStore> { Arc::clone(&self.game_design_store) }
        fn mcp(&self) -> Arc<dyn McpProvider> { Arc::clone(&self.mcp) }
    }
}
```

- [ ] 在同一文件 `office_hours.rs` 中 mock 模块之后追加 `EnterOfficeHoursModeTool`：

```rust
pub struct EnterOfficeHoursModeTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl EnterOfficeHoursModeTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for EnterOfficeHoursModeTool {
    fn name(&self) -> &str { "EnterOfficeHoursMode" }

    fn description(&self) -> &str {
        "Use this tool when the user explicitly asks to start office hours mode. Office hours mode provides structured YC-style startup/builder diagnostic workflow. It should only be used as the very first action in a session — once active, it locks the session into the diagnostic flow and exits after producing a design document."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, _args: Value) -> Result<ToolExecution, ToolError> {
        let provider = Arc::clone(&self.provider);
        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Requesting to enter office hours mode".into(),
            approval_rule: "EnterOfficeHoursMode".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                Box::pin(async move {
                    let lang = provider.user_language();
                    if provider.is_session_mode_active() {
                        if provider.session_mode_kind() == Some(SessionModeKind::OfficeHours) {
                            return ExecutableToolResult::error_text(
                                office_hours_t("officeHours.alreadyActive", lang),
                                "already active".into(),
                            );
                        }
                        return ExecutableToolResult::error_text(
                            office_hours_t("officeHours.anotherModeActive", lang),
                            "another mode active".into(),
                        );
                    }

                    if let Err(e) = provider.enter_session_mode(SessionModeKind::OfficeHours).await {
                        return ExecutableToolResult::error_text(
                            office_hours_t_replace("officeHours.failedToEnter", lang, &[("message", &e.to_string())]),
                            "enter failed".into(),
                        );
                    }

                    let msg = office_hours_entry_reminder(provider.session_mode_file_path().as_deref());
                    ExecutableToolResult::ok_text(msg)
                })
            }),
        })
    }
}
```

- [ ] 在 `tests` 模块内追加 `EnterOfficeHoursModeTool` 的测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use super::mock::MockSessionModeProvider;

    fn empty_context() -> ExecutableToolContext {
        ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
        }
    }

    #[tokio::test]
    async fn enter_office_hours_succeeds_when_inactive() {
        let provider = Arc::new(MockSessionModeProvider::inactive());
        let tool = EnterOfficeHoursModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(empty_context()).await;
        assert!(!result.is_error);
        let text = result.to_text();
        assert!(text.contains("Office hours is now active"));
        assert!(text.contains("## HARD GATES"));
        assert_eq!(provider.entered.lock().unwrap().as_slice(), &[SessionModeKind::OfficeHours]);
        assert!(provider.session_mode_file_path().is_some());
    }

    #[tokio::test]
    async fn enter_office_hours_fails_when_already_active() {
        let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::OfficeHours));
        let tool = EnterOfficeHoursModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(empty_context()).await;
        assert!(result.is_error);
        assert!(result.to_text().contains("already active"));
        assert!(provider.entered.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn enter_office_hours_fails_when_another_mode_active() {
        let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::Design));
        let tool = EnterOfficeHoursModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(empty_context()).await;
        assert!(result.is_error);
        let text = result.to_text();
        assert!(text.contains("Another session mode"));
        assert!(text.contains("Exit it first"));
        assert!(provider.entered.lock().unwrap().is_empty());
    }
}
```

- [ ] 运行测试并确认通过：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::office_hours
```

Expected: 8 个测试全部通过（Task 1 的 5 个 + 本 Task 的 3 个）。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): add EnterOfficeHoursModeTool with mock providers`。

---

## Task 3: 实现 `ExitOfficeHoursModeTool`

**Depends on:** Task 2（`MockSessionModeProvider` 与 `EnterOfficeHoursModeTool` 模式已就绪）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`（追加 `ExitOfficeHoursModeTool` 与测试）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`

**Why:** `ExitOfficeHoursModeTool` 结束 office-hours 会话，需要读取当前 design doc 路径、调用 `exit_session_mode`，并返回包含保存路径的本地化告别消息。

**Steps：**

- [ ] 在 `office_hours.rs` 中 `EnterOfficeHoursModeTool` 之后追加 `ExitOfficeHoursModeTool`：

```rust
pub struct ExitOfficeHoursModeTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl ExitOfficeHoursModeTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for ExitOfficeHoursModeTool {
    fn name(&self) -> &str { "ExitOfficeHoursMode" }

    fn description(&self) -> &str {
        "Exit office hours mode after the design document has been approved and written. This ends the office hours session, flushes telemetry and profile data, and shuts down the application."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, _args: Value) -> Result<ToolExecution, ToolError> {
        let provider = Arc::clone(&self.provider);
        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Requesting to exit office hours mode".into(),
            approval_rule: "ExitOfficeHoursMode".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                Box::pin(async move {
                    let lang = provider.user_language();
                    if !provider.is_session_mode_active() || provider.session_mode_kind() != Some(SessionModeKind::OfficeHours) {
                        return ExecutableToolResult::error_text(
                            office_hours_t("officeHours.modeNotActive", lang),
                            "not active".into(),
                        );
                    }

                    let path = provider.session_mode_file_path();
                    if let Err(e) = provider.exit_session_mode().await {
                        return ExecutableToolResult::error_text(
                            format!("Failed to exit office hours mode: {}", e),
                            "exit failed".into(),
                        );
                    }

                    let mut parts = vec![office_hours_t("officeHours.sessionComplete", lang)];
                    if let Some(p) = path {
                        parts.push(office_hours_t_replace("officeHours.designDocSaved", lang, &[("path", &p)]));
                    }
                    parts.push(office_hours_t("officeHours.appWillExit", lang));

                    ExecutableToolResult::ok_text(parts.join("\n"))
                })
            }),
        })
    }
}
```

- [ ] 在 `tests` 模块内追加测试：

```rust
#[tokio::test]
async fn exit_office_hours_succeeds_when_active() {
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::OfficeHours)
            .with_file_path(".ody-code/products/2026-06-29-foo.md"),
    );
    let tool = ExitOfficeHoursModeTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("Office hours session complete"));
    assert!(text.contains("Design document saved to: .ody-code/products/2026-06-29-foo.md"));
    assert!(text.contains("The application will now exit"));
    assert!(!*provider.exited.lock().unwrap());
}

#[tokio::test]
async fn exit_office_hours_fails_when_not_active() {
    let provider = Arc::new(MockSessionModeProvider::inactive());
    let tool = ExitOfficeHoursModeTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Office hours mode is not active"));
}

#[tokio::test]
async fn exit_office_hours_fails_when_wrong_mode_active() {
    let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::Plan));
    let tool = ExitOfficeHoursModeTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Office hours mode is not active"));
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::office_hours
```

Expected: 11 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): add ExitOfficeHoursModeTool`。

---

## Task 4: 实现 `SetOfficeHoursLanguageTool`

**Depends on:** Task 2（`MockSessionModeProvider` 已就绪）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`（追加 `SetOfficeHoursLanguageTool`、语言校验辅助函数与测试）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`

**Why:** 该工具在 office-hours 会话开始时记录用户语言，后续所有本地化输出据此切换。需要严格校验输入只能是 `en` 或 `zh`，与 TS `isSupportedLanguage` 一致。

**Steps：**

- [ ] 在 `office_hours.rs` 的 i18n 函数附近追加语言校验辅助函数：

```rust
/// 严格匹配 TS `SupportedLanguage = 'en' | 'zh'`。
fn parse_supported_language(s: &str) -> Option<Language> {
    match s {
        "en" => Some(Language::En),
        "zh" => Some(Language::Zh),
        _ => None,
    }
}
```

- [ ] 追加 `SetOfficeHoursLanguageInput` 与 `SetOfficeHoursLanguageTool`：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetOfficeHoursLanguageInput {
    pub language: String,
}

pub struct SetOfficeHoursLanguageTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl SetOfficeHoursLanguageTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for SetOfficeHoursLanguageTool {
    fn name(&self) -> &str { "SetOfficeHoursLanguage" }

    fn description(&self) -> &str {
        "Call once at the start of office-hours to record the language the user is writing in. This localizes tool outputs and TUI labels."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "language": {
                    "type": "string",
                    "enum": ["en", "zh"],
                    "description": "Language must be 'en' or 'zh'"
                }
            },
            "required": ["language"],
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let input: SetOfficeHoursLanguageInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let provider = Arc::clone(&self.provider);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Setting office hours user language".into(),
            approval_rule: "SetOfficeHoursLanguage".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let input = input.clone();
                Box::pin(async move {
                    let current_lang = provider.user_language();
                    if !provider.is_session_mode_active() || provider.session_mode_kind() != Some(SessionModeKind::OfficeHours) {
                        return ExecutableToolResult::error_text(
                            office_hours_t("officeHours.modeNotActive", current_lang),
                            "not active".into(),
                        );
                    }

                    let Some(lang) = parse_supported_language(&input.language) else {
                        return ExecutableToolResult::error_text(
                            format!("Unsupported language: {}", input.language),
                            "unsupported language".into(),
                        );
                    };

                    provider.set_user_language(lang);
                    ExecutableToolResult::ok_text(
                        office_hours_t_replace("officeHours.languageSet", lang, &[("language", &input.language)])
                    )
                })
            }),
        })
    }
}
```

- [ ] 在 `tests` 模块追加测试：

```rust
#[tokio::test]
async fn set_language_succeeds_in_office_hours() {
    let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::OfficeHours));
    let tool = SetOfficeHoursLanguageTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({"language": "zh"})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    assert!(result.to_text().contains("用户语言已设置为 zh"));
    assert_eq!(provider.user_language(), Language::Zh);
}

#[tokio::test]
async fn set_language_fails_outside_office_hours() {
    let provider = Arc::new(MockSessionModeProvider::inactive());
    let tool = SetOfficeHoursLanguageTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({"language": "en"})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Office hours mode is not active"));
}

#[tokio::test]
async fn set_language_rejects_unsupported_language() {
    let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::OfficeHours));
    let tool = SetOfficeHoursLanguageTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({"language": "fr"})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Unsupported language: fr"));
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::office_hours
```

Expected: 14 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): add SetOfficeHoursLanguageTool`。

---

## Task 5: 实现 `AppendLearningTool`

**Depends on:** Task 2（`MockSessionModeProvider` 与 `MockOfficeHoursStore` 已就绪）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`（追加 `AppendLearningTool` 与测试）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`

**Why:** 该工具将学习洞察写入 `OfficeHoursStateStore`，是 Phase 6 Handoff 资源推荐的数据来源。需要断言写入的条目包含正确的 `skill`、`source`、`ts` 与输入字段。

**Steps：**

- [ ] 追加输入类型与 `AppendLearningTool`：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppendLearningInput {
    #[serde(rename = "type")]
    pub type_: String,
    pub key: String,
    pub insight: String,
    pub confidence: f64,
    #[serde(default)]
    pub branch: Option<String>,
}

pub struct AppendLearningTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl AppendLearningTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for AppendLearningTool {
    fn name(&self) -> &str { "AppendLearning" }

    fn description(&self) -> &str {
        "Record a learning insight during office hours. Use this to persist operational observations or eureka moments for future sessions. Learnings are searchable across sessions for Phase 6 (Handoff) resource selection. Only available while office hours mode is active."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "type": {
                    "type": "string",
                    "enum": ["operational", "eureka"],
                    "description": "Type of learning: operational (process/technique) or eureka (insight/discovery)."
                },
                "key": { "type": "string", "minLength": 1, "description": "Short unique key to identify this learning." },
                "insight": { "type": "string", "minLength": 1, "description": "The learning insight text." },
                "confidence": { "type": "number", "minimum": 0, "maximum": 1, "description": "Confidence score between 0 and 1." },
                "branch": { "type": "string", "description": "Optional git branch identifier for context." }
            },
            "required": ["type", "key", "insight", "confidence"],
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let input: AppendLearningInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let provider = Arc::clone(&self.provider);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Appending learning insight".into(),
            approval_rule: "AppendLearning".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let input = input.clone();
                Box::pin(async move {
                    let lang = provider.user_language();
                    if !provider.is_session_mode_active() || provider.session_mode_kind() != Some(SessionModeKind::OfficeHours) {
                        return ExecutableToolResult::error_text(
                            office_hours_t("officeHours.modeNotActive", lang),
                            "not active".into(),
                        );
                    }

                    let entry = LearningEntry {
                        ts: chrono::Utc::now().to_rfc3339(),
                        skill: "office-hours".into(),
                        type_: input.type_,
                        key: input.key.clone(),
                        insight: input.insight,
                        confidence: input.confidence,
                        source: "observed".into(),
                        branch: input.branch,
                    };

                    if let Err(e) = provider.office_hours_store().append_learning(entry).await {
                        return ExecutableToolResult::error_text(
                            format!("Failed to append learning: {}", e),
                            "append failed".into(),
                        );
                    }

                    ExecutableToolResult::ok_text(
                        office_hours_t_replace("officeHours.learningRecorded", lang, &[("key", &input.key)])
                    )
                })
            }),
        })
    }
}
```

- [ ] 在 `tests` 模块追加测试。为验证错误路径，内联一个总是返回 Err 的 store：

```rust
struct FailingLearningStore;
#[async_trait]
impl OfficeHoursStateStore for FailingLearningStore {
    async fn append_profile(&self, _entry: BuilderProfileEntry) -> anyhow::Result<()> { Ok(()) }
    async fn append_learning(&self, _entry: LearningEntry) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("disk full"))
    }
    async fn search_learnings(&self, _limit: usize, _cross_project: bool) -> anyhow::Result<Vec<LearningEntry>> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn append_learning_records_entry() {
    let store = Arc::new(super::mock::MockOfficeHoursStore::default());
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::OfficeHours)
            .with_office_hours_store(store.clone()),
    );
    let tool = AppendLearningTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({
        "type": "eureka",
        "key": "demand_signal",
        "insight": "Users stated demand is softer than observed usage.",
        "confidence": 0.85,
        "branch": "main"
    })).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    assert!(result.to_text().contains("demand_signal"));

    let learnings = store.learnings.lock().unwrap();
    assert_eq!(learnings.len(), 1);
    assert_eq!(learnings[0].key, "demand_signal");
    assert_eq!(learnings[0].skill, "office-hours");
    assert_eq!(learnings[0].source, "observed");
    assert_eq!(learnings[0].type_, "eureka");
    assert_eq!(learnings[0].branch.as_deref(), Some("main"));
    assert!(learnings[0].ts.len() >= 20); // ISO-8601 sanity
}

#[tokio::test]
async fn append_learning_fails_when_not_active() {
    let provider = Arc::new(MockSessionModeProvider::inactive());
    let tool = AppendLearningTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({
        "type": "operational", "key": "x", "insight": "y", "confidence": 0.5
    })).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Office hours mode is not active"));
}

#[tokio::test]
async fn append_learning_propagates_store_error() {
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::OfficeHours)
            .with_office_hours_store(Arc::new(FailingLearningStore)),
    );
    let tool = AppendLearningTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({
        "type": "operational", "key": "x", "insight": "y", "confidence": 0.5
    })).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Failed to append learning: disk full"));
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::office_hours
```

Expected: 17 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green（若 `chrono` 未在 `tools-rs/Cargo.toml` 中声明，请在 Task 1 或本 Task 中添加 `chrono = { version = "0.4", features = ["serde"] }`）。

- [ ] Commit: `feat(tools-rs): add AppendLearningTool`。

---

## Task 6: 实现 `AppendBuilderProfileTool`

**Depends on:** Task 2（mock provider 与 state store 已就绪）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`（追加 `AppendBuilderProfileTool` 与测试）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`

**Why:** Phase 4.5 Founder Signal Synthesis 完成后必须调用该工具，将 founder signals 持久化到 builder profile，用于后续 tier 计算与资源推荐。`designDoc` 参数可选，缺省时回退到当前 session-mode 文件路径。

**Steps：**

- [ ] 追加输入类型与 `AppendBuilderProfileTool`：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppendBuilderProfileInput {
    pub mode: String,
    pub project_slug: String,
    pub signal_count: u64,
    pub signals: Vec<String>,
    pub design_doc: Option<String>,
    pub assignment: Option<String>,
    pub resources_shown: Vec<String>,
    pub topics: Vec<String>,
}

pub struct AppendBuilderProfileTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl AppendBuilderProfileTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for AppendBuilderProfileTool {
    fn name(&self) -> &str { "AppendBuilderProfile" }

    fn description(&self) -> &str {
        "Append a builder profile entry after completing Phase 4.5 (Founder Signal Synthesis). This persists the session profile data to the local office-hours state store for tier computation and resource selection. Only available while office hours mode is active."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "mode": { "type": "string", "enum": ["startup", "builder"], "description": "Whether this is a startup or builder session." },
                "projectSlug": { "type": "string", "description": "Project slug derived from the project name or working directory." },
                "signalCount": { "type": "integer", "minimum": 0, "description": "Number of founder signals observed during Phase 4.5 synthesis." },
                "signals": { "type": "array", "items": { "type": "string" }, "description": "List of founder signal names observed." },
                "designDoc": { "type": "string", "description": "Path to the design document produced during Phase 5. Defaults to the current office-hours design file path if omitted." },
                "assignment": { "type": "string", "description": "The assignment text from the design document. Defaults to empty if omitted." },
                "resourcesShown": { "type": "array", "items": { "type": "string" }, "description": "URLs of resources shown to the user during this session." },
                "topics": { "type": "array", "items": { "type": "string" }, "description": "Topics or categories covered in the session." }
            },
            "required": ["mode", "projectSlug", "signalCount", "signals", "resourcesShown", "topics"],
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let input: AppendBuilderProfileInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let provider = Arc::clone(&self.provider);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Appending builder profile entry".into(),
            approval_rule: "AppendBuilderProfile".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let input = input.clone();
                Box::pin(async move {
                    let lang = provider.user_language();
                    if !provider.is_session_mode_active() || provider.session_mode_kind() != Some(SessionModeKind::OfficeHours) {
                        return ExecutableToolResult::error_text(
                            office_hours_t("officeHours.modeNotActive", lang),
                            "not active".into(),
                        );
                    }

                    let design_doc = input.design_doc
                        .or_else(|| provider.session_mode_file_path())
                        .unwrap_or_default();

                    let entry = BuilderProfileEntry {
                        date: chrono::Utc::now().to_rfc3339(),
                        mode: input.mode,
                        project_slug: input.project_slug,
                        signal_count: input.signal_count,
                        signals: input.signals,
                        design_doc,
                        assignment: input.assignment.unwrap_or_default(),
                        resources_shown: input.resources_shown,
                        topics: input.topics,
                    };

                    if let Err(e) = provider.office_hours_store().append_profile(entry).await {
                        return ExecutableToolResult::error_text(
                            format!("Failed to append builder profile entry: {}", e),
                            "append failed".into(),
                        );
                    }

                    ExecutableToolResult::ok_text(office_hours_t("officeHours.profileAppended", lang))
                })
            }),
        })
    }
}
```

- [ ] 在 `tests` 模块追加测试。为验证 store 错误路径，内联一个总是返回 Err 的 store：

```rust
struct FailingProfileStore;
#[async_trait]
impl OfficeHoursStateStore for FailingProfileStore {
    async fn append_profile(&self, _entry: BuilderProfileEntry) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("disk full"))
    }
    async fn append_learning(&self, _entry: LearningEntry) -> anyhow::Result<()> { Ok(()) }
    async fn search_learnings(&self, _limit: usize, _cross_project: bool) -> anyhow::Result<Vec<LearningEntry>> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn append_profile_uses_explicit_design_doc() {
    let store = Arc::new(super::mock::MockOfficeHoursStore::default());
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::OfficeHours)
            .with_office_hours_store(store.clone()),
    );
    let tool = AppendBuilderProfileTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({
        "mode": "startup",
        "projectSlug": "acme-corp",
        "signalCount": 4,
        "signals": ["demand_observed", "named_users", "agency", "taste"],
        "designDoc": ".ody-code/products/acme.md",
        "assignment": "Build a payment widget",
        "resourcesShown": ["https://example.com/resource"],
        "topics": ["payments", "b2b"]
    })).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);

    let profiles = store.profiles.lock().unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].mode, "startup");
    assert_eq!(profiles[0].project_slug, "acme-corp");
    assert_eq!(profiles[0].signal_count, 4);
    assert_eq!(profiles[0].design_doc, ".ody-code/products/acme.md");
    assert_eq!(profiles[0].assignment, "Build a payment widget");
    assert_eq!(profiles[0].signals.len(), 4);
}

#[tokio::test]
async fn append_profile_falls_back_to_session_mode_file_path() {
    let store = Arc::new(super::mock::MockOfficeHoursStore::default());
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::OfficeHours)
            .with_file_path(".ody-code/products/fallback.md")
            .with_office_hours_store(store.clone()),
    );
    let tool = AppendBuilderProfileTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({
        "mode": "builder",
        "projectSlug": "widget",
        "signalCount": 0,
        "signals": [],
        "resourcesShown": [],
        "topics": []
    })).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);

    let profiles = store.profiles.lock().unwrap();
    assert_eq!(profiles[0].design_doc, ".ody-code/products/fallback.md");
    assert_eq!(profiles[0].assignment, "");
}

#[tokio::test]
async fn append_profile_fails_when_not_active() {
    let provider = Arc::new(MockSessionModeProvider::inactive());
    let tool = AppendBuilderProfileTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({
        "mode": "startup", "projectSlug": "x", "signalCount": 0, "signals": [],
        "resourcesShown": [], "topics": []
    })).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Office hours mode is not active"));
}

#[tokio::test]
async fn append_profile_propagates_store_error() {
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::OfficeHours)
            .with_office_hours_store(Arc::new(FailingProfileStore)),
    );
    let tool = AppendBuilderProfileTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({
        "mode": "startup", "projectSlug": "x", "signalCount": 0, "signals": [],
        "resourcesShown": [], "topics": []
    })).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Failed to append builder profile entry: disk full"));
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::office_hours
```

Expected: 21 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): add AppendBuilderProfileTool`。

---

## Task 7: 实现 `SearchLearningsTool`

**Depends on:** Task 2（mock provider 与 `MockOfficeHoursStore` 已就绪）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`（追加 `SearchLearningsTool` 与测试）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`

**Why:** Phase 6 Handoff 使用该工具检索过往学习洞察以推荐资源。输出格式（含 Type / Insight / Confidence / Date / Branch 标签）必须与 TS 完全一致，否则 L3 parity 会失败。

**Steps：**

- [ ] 追加输入类型与 `SearchLearningsTool`：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchLearningsInput {
    #[serde(default = "default_search_limit")]
    pub limit: usize,
    pub cross_project: Option<bool>,
}

fn default_search_limit() -> usize { 10 }

pub struct SearchLearningsTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl SearchLearningsTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for SearchLearningsTool {
    fn name(&self) -> &str { "SearchLearnings" }

    fn description(&self) -> &str {
        "Search past learnings from office hours sessions. Use this to find relevant prior insights for Phase 6 (Handoff) resource selection and follow-up recommendations. Only available while office hours mode is active."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "minimum": 1, "default": 10, "description": "Maximum number of learnings to return." },
                "crossProject": { "type": "boolean", "description": "Whether to search across all projects (true) or only the current project (false)." }
            },
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let input: SearchLearningsInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let provider = Arc::clone(&self.provider);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Searching past learnings".into(),
            approval_rule: "SearchLearnings".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let input = input.clone();
                Box::pin(async move {
                    let lang = provider.user_language();
                    if !provider.is_session_mode_active() || provider.session_mode_kind() != Some(SessionModeKind::OfficeHours) {
                        return ExecutableToolResult::error_text(
                            office_hours_t("officeHours.modeNotActive", lang),
                            "not active".into(),
                        );
                    }

                    let limit = input.limit;
                    let cross_project = input.cross_project.unwrap_or(false);
                    let learnings = match provider.office_hours_store().search_learnings(limit, cross_project).await {
                        Ok(v) => v,
                        Err(e) => return ExecutableToolResult::error_text(
                            format!("Failed to search learnings: {}", e),
                            "search failed".into(),
                        ),
                    };

                    if learnings.is_empty() {
                        return ExecutableToolResult::ok_text(office_hours_t("officeHours.noLearnings", lang));
                    }

                    let formatted: Vec<String> = learnings.iter().enumerate().map(|(i, l)| {
                        let mut lines = vec![
                            format!(
                                "[{}] {}: {}: {}",
                                i + 1,
                                office_hours_t("officeHours.learningTypeLabel", lang),
                                l.type_.to_uppercase(),
                                l.key
                            ),
                            format!("    {}: {}", office_hours_t("officeHours.learningInsightLabel", lang), l.insight),
                            format!("    {}: {}", office_hours_t("officeHours.learningConfidenceLabel", lang), l.confidence),
                            format!("    {}: {}", office_hours_t("officeHours.learningDateLabel", lang), l.ts),
                        ];
                        if let Some(branch) = &l.branch {
                            lines.push(format!(
                                "    {}: {}",
                                office_hours_t("officeHours.learningBranchLabel", lang),
                                branch
                            ));
                        }
                        lines.join("\n")
                    }).collect();

                    let header = office_hours_t_replace("officeHours.learningsHeader", lang, &[("count", &learnings.len().to_string())]);
                    ExecutableToolResult::ok_text(format!("{}\n\n{}", header, formatted.join("\n\n")))
                })
            }),
        })
    }
}
```

- [ ] 在 `tests` 模块追加测试。内联一个总是返回 Err 的 store 用于错误路径：

```rust
struct FailingSearchStore;
#[async_trait]
impl OfficeHoursStateStore for FailingSearchStore {
    async fn append_profile(&self, _entry: BuilderProfileEntry) -> anyhow::Result<()> { Ok(()) }
    async fn append_learning(&self, _entry: LearningEntry) -> anyhow::Result<()> { Ok(()) }
    async fn search_learnings(&self, _limit: usize, _cross_project: bool) -> anyhow::Result<Vec<LearningEntry>> {
        Err(anyhow::anyhow!("timeout"))
    }
}

#[tokio::test]
async fn search_learnings_returns_formatted_results() {
    let store = Arc::new(super::mock::MockOfficeHoursStore::default());
    store.learnings.lock().unwrap().push(LearningEntry {
        ts: "2026-06-29T12:00:00Z".into(),
        skill: "office-hours".into(),
        type_: "eureka".into(),
        key: "demand_gap".into(),
        insight: "Observed usage is lower than stated demand.".into(),
        confidence: 0.9,
        source: "observed".into(),
        branch: Some("main".into()),
    });
    store.learnings.lock().unwrap().push(LearningEntry {
        ts: "2026-06-29T11:00:00Z".into(),
        skill: "office-hours".into(),
        type_: "operational".into(),
        key: "question_economy".into(),
        insight: "Ask only load-bearing questions.".into(),
        confidence: 0.75,
        source: "observed".into(),
        branch: None,
    });

    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::OfficeHours)
            .with_office_hours_store(store.clone()),
    );
    let tool = SearchLearningsTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({"limit": 10})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("Found 2 learning(s):"));
    assert!(text.contains("[1] Type: EUREKA: demand_gap"));
    assert!(text.contains("Insight: Observed usage is lower than stated demand."));
    assert!(text.contains("Confidence: 0.9"));
    assert!(text.contains("Branch: main"));
    assert!(text.contains("[2] Type: OPERATIONAL: question_economy"));
    assert!(!text.contains("Branch:")); // second entry has no branch
}

#[tokio::test]
async fn search_learnings_returns_empty_message() {
    let store = Arc::new(super::mock::MockOfficeHoursStore::default());
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::OfficeHours)
            .with_office_hours_store(store),
    );
    let tool = SearchLearningsTool::new(provider);
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    assert_eq!(result.to_text(), "No past learnings found.");
}

#[tokio::test]
async fn search_learnings_fails_when_not_active() {
    let provider = Arc::new(MockSessionModeProvider::inactive());
    let tool = SearchLearningsTool::new(provider);
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Office hours mode is not active"));
}

#[tokio::test]
async fn search_learnings_propagates_store_error() {
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::OfficeHours)
            .with_office_hours_store(Arc::new(FailingSearchStore)),
    );
    let tool = SearchLearningsTool::new(provider);
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Failed to search learnings: timeout"));
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::office_hours
```

Expected: 25 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): add SearchLearningsTool`。

---

## Task 8: 实现 `EnsureClaudeMdRoutingTool`

**Depends on:** Task 2（`MockSessionModeProvider` 与 `MockKaos` 已就绪）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`（追加 `EnsureClaudeMdRoutingTool` 与测试）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`

**Why:** 该工具确保项目根目录 `AGENTS.md` 包含 office-hours 的 `## Skill routing` 章节，是 office-hours 入口引导的一部分。需要覆盖「文件不存在则创建」「存在但缺少章节则追加」「已有章节则跳过」三种状态。

**Steps：**

- [ ] 在 `office_hours.rs` 中工具区追加常量与工具：

```rust
const OFFICE_HOURS_ROUTING_SECTION: &str = r#"
## Skill routing

- **office-hours**: YC office hours diagnostic workflow. Activates when the user explicitly requests office hours or asks for startup/builder diagnostic help.

To invoke, ask the agent to start office hours.
"#;

pub struct EnsureClaudeMdRoutingTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl EnsureClaudeMdRoutingTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for EnsureClaudeMdRoutingTool {
    fn name(&self) -> &str { "EnsureClaudeMdRouting" }

    fn description(&self) -> &str {
        "Ensure AGENTS.md exists in the project root with a ## Skill routing section for office hours mode. If the file is missing, create it with the section. If it exists without the section, append it. If it already has the section, do nothing. Only available while office hours mode is active."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, _args: Value) -> Result<ToolExecution, ToolError> {
        let provider = Arc::clone(&self.provider);
        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Ensuring AGENTS.md has skill routing section for office hours".into(),
            approval_rule: "EnsureClaudeMdRouting".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                Box::pin(async move {
                    let lang = provider.user_language();
                    if !provider.is_session_mode_active() || provider.session_mode_kind() != Some(SessionModeKind::OfficeHours) {
                        return ExecutableToolResult::error_text(
                            office_hours_t("officeHours.modeNotActive", lang),
                            "not active".into(),
                        );
                    }

                    let cwd = provider.kaos().cwd();
                    let path = std::path::PathBuf::from(&cwd).join("AGENTS.md");
                    let path_str = path.to_string_lossy().to_string();

                    let (content, file_exists) = match provider.kaos().read_text(&path_str).await {
                        Ok(c) => (c, true),
                        Err(_) => (String::new(), false),
                    };

                    let result = if !file_exists {
                        match provider.kaos().write_text(&path_str, OFFICE_HOURS_ROUTING_SECTION.trim_start()).await {
                            Ok(()) => ExecutableToolResult::ok_text(
                                office_hours_t_replace("officeHours.agentsMdCreated", lang, &[("path", &path_str)])
                            ),
                            Err(e) => ExecutableToolResult::error_text(
                                office_hours_t_replace("officeHours.failedToEnsureRouting", lang, &[("message", &e.to_string())]),
                                "write failed".into(),
                            ),
                        }
                    } else if content.contains("## Skill routing") {
                        ExecutableToolResult::ok_text(office_hours_t("officeHours.agentsMdAlreadyHasRouting", lang))
                    } else {
                        let updated = content.trim_end().to_string() + OFFICE_HOURS_ROUTING_SECTION;
                        match provider.kaos().write_text(&path_str, &updated).await {
                            Ok(()) => ExecutableToolResult::ok_text(
                                office_hours_t_replace("officeHours.agentsMdUpdated", lang, &[("path", &path_str)])
                            ),
                            Err(e) => ExecutableToolResult::error_text(
                                office_hours_t_replace("officeHours.failedToEnsureRouting", lang, &[("message", &e.to_string())]),
                                "write failed".into(),
                            ),
                        }
                    };

                    result
                })
            }),
        })
    }
}
```

- [ ] 在 `tests` 模块追加测试。通过 Task 2 已添加的 `provider.mock_kaos()` 直接读写 mock 文件：

```rust
#[tokio::test]
async fn ensure_routing_creates_agents_md_when_missing() {
    let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::OfficeHours));
    let tool = EnsureClaudeMdRoutingTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    assert!(result.to_text().contains("AGENTS.md created"));

    let content = provider.mock_kaos().files.lock().unwrap()
        .get(&std::path::PathBuf::from("/workspace/AGENTS.md")).cloned().unwrap();
    assert!(content.contains("## Skill routing"));
    assert!(content.contains("office-hours"));
}

#[tokio::test]
async fn ensure_routing_appends_when_missing_section() {
    let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::OfficeHours));
    provider.mock_kaos().insert("/workspace/AGENTS.md", "# Existing guide\n\nSome rules.\n");
    let tool = EnsureClaudeMdRoutingTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    assert!(result.to_text().contains("Appended ## Skill routing section"));

    let content = provider.mock_kaos().files.lock().unwrap()
        .get(&std::path::PathBuf::from("/workspace/AGENTS.md")).cloned().unwrap();
    assert!(content.starts_with("# Existing guide"));
    assert!(content.contains("## Skill routing"));
    assert!(content.contains("office-hours"));
}

#[tokio::test]
async fn ensure_routing_noop_when_already_has_section() {
    let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::OfficeHours));
    provider.mock_kaos().insert("/workspace/AGENTS.md", "# Existing guide\n\n## Skill routing\n\nAlready present.\n");
    let tool = EnsureClaudeMdRoutingTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    assert!(result.to_text().contains("already has a ## Skill routing section"));

    let content = provider.mock_kaos().files.lock().unwrap()
        .get(&std::path::PathBuf::from("/workspace/AGENTS.md")).cloned().unwrap();
    assert!(!content.contains("YC office hours diagnostic workflow"));
}

#[tokio::test]
async fn ensure_routing_fails_when_not_active() {
    let provider = Arc::new(MockSessionModeProvider::inactive());
    let tool = EnsureClaudeMdRoutingTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Office hours mode is not active"));
}
```

- [ ] 确认 Task 2 的 `MockSessionModeProvider` 已实现 `#[derive(Clone)]`、`kaos: Arc<MockKaos>` 字段、`with_kaos(...)` 与 `mock_kaos()` 方法（已在 Task 2 编辑中完成）。

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::office_hours
```

Expected: 29 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): add EnsureClaudeMdRoutingTool`。

---

## Task 9: 实现 `SyncOfficeHoursArtifactTool`

**Depends on:** Task 2（`MockSessionModeProvider`、`MockKaos`、`MockMcpProvider` 已就绪）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`（追加 `GbrainCli` trait、`SyncOfficeHoursArtifactTool` 与测试）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/office_hours.rs`

**Why:** 该工具在 Phase 6 Handoff 将设计文档同步到 gbrain。它需要同时支持 MCP 检测路径与 gbrain CLI fallback。为让 CLI fallback 可单元测试，引入一个轻量的 `GbrainCli` 抽象，生产环境使用真实 `std::process::Command`，测试注入 mock runner。

**Steps：**

- [ ] 在 `office_hours.rs` 追加 `GbrainCli` trait与真实实现：

```rust
#[async_trait]
pub trait GbrainCli: Send + Sync {
    async fn run(&self, args: &[String], cwd: &str) -> anyhow::Result<String>;
}

pub struct RealGbrainCli;

#[async_trait]
impl GbrainCli for RealGbrainCli {
    async fn run(&self, args: &[String], cwd: &str) -> anyhow::Result<String> {
        let output = tokio::task::spawn_blocking({
            let args = args.to_vec();
            let cwd = cwd.to_string();
            move || {
                std::process::Command::new("gbrain")
                    .args(&args)
                    .current_dir(&cwd)
                    .timeout(std::time::Duration::from_secs(30))
                    .output()
            }
        }).await.map_err(|e| anyhow::anyhow!("gbrain CLI spawn failed: {}", e))??;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(anyhow::anyhow!("{}", String::from_utf8_lossy(&output.stderr)))
        }
    }
}
```

- [ ] 追加输入类型与 `SyncOfficeHoursArtifactTool`：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOfficeHoursArtifactInput {
    pub design_file_path: String,
}

pub struct SyncOfficeHoursArtifactTool {
    provider: Arc<dyn SessionModeProvider>,
    gbrain_cli: Arc<dyn GbrainCli>,
}

impl SyncOfficeHoursArtifactTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self {
            provider,
            gbrain_cli: Arc::new(RealGbrainCli),
        }
    }

    #[cfg(test)]
    pub fn new_with_runner(provider: Arc<dyn SessionModeProvider>, gbrain_cli: Arc<dyn GbrainCli>) -> Self {
        Self { provider, gbrain_cli }
    }
}

impl BuiltinTool for SyncOfficeHoursArtifactTool {
    fn name(&self) -> &str { "SyncOfficeHoursArtifact" }

    fn description(&self) -> &str {
        "Sync a design document artifact to the gbrain knowledge base during office hours handoff (Phase 6). Checks for a .gbrain-source pin in the project root to determine the target source, then attempts to sync via the gbrain MCP tool (if available) or falls back to the gbrain CLI. Only available while office hours mode is active."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "designFilePath": { "type": "string", "description": "Absolute path to the design document artifact to sync." }
            },
            "required": ["designFilePath"],
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let input: SyncOfficeHoursArtifactInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let provider = Arc::clone(&self.provider);
        let gbrain_cli = Arc::clone(&self.gbrain_cli);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Syncing design artifact to gbrain".into(),
            approval_rule: "SyncOfficeHoursArtifact".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let gbrain_cli = Arc::clone(&gbrain_cli);
                let input = input.clone();
                Box::pin(async move {
                    let lang = provider.user_language();
                    if !provider.is_session_mode_active() || provider.session_mode_kind() != Some(SessionModeKind::OfficeHours) {
                        return ExecutableToolResult::error_text(
                            office_hours_t("officeHours.modeNotActive", lang),
                            "not active".into(),
                        );
                    }

                    let project_root = provider.kaos().project_root().unwrap_or_else(|| provider.kaos().cwd());
                    let gbrain_pin_path = std::path::PathBuf::from(&project_root).join(".gbrain-source").to_string_lossy().to_string();

                    let gbrain_source = match provider.kaos().read_text(&gbrain_pin_path).await {
                        Ok(s) => {
                            let trimmed = s.trim();
                            if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
                        }
                        Err(_) => None,
                    };

                    if let Err(_) = provider.kaos().stat(&input.design_file_path).await {
                        return ExecutableToolResult::error_text(
                            office_hours_t_replace("officeHours.designFileNotFound", lang, &[("path", &input.design_file_path)]),
                            "design file not found".into(),
                        );
                    }

                    if provider.mcp().gbrain_available().await {
                        let mut parts = vec![office_hours_t("officeHours.gbrainConnected", lang)];
                        if let Some(source) = &gbrain_source {
                            parts.push(office_hours_t_replace("officeHours.gbrainTargetSource", lang, &[("source", source)]));
                        } else {
                            parts.push(office_hours_t("officeHours.gbrainNoSourcePin", lang));
                        }
                        parts.push(office_hours_t_replace("officeHours.gbrainReadyForSync", lang, &[("path", &input.design_file_path)]));
                        return ExecutableToolResult::ok_text(parts.join("\n"));
                    }

                    let mut cli_args = vec!["artifact".into(), "add".into()];
                    if let Some(source) = &gbrain_source {
                        cli_args.push("--source".into());
                        cli_args.push(source.clone());
                    }
                    cli_args.push(input.design_file_path.clone());

                    match gbrain_cli.run(&cli_args, &project_root).await {
                        Ok(_) => {
                            let mut parts = vec![office_hours_t("officeHours.gbrainSynced", lang)];
                            if let Some(source) = &gbrain_source {
                                parts.push(office_hours_t_replace("officeHours.gbrainTargetSource", lang, &[("source", source)]));
                            }
                            parts.push(office_hours_t_replace("officeHours.gbrainFile", lang, &[("path", &input.design_file_path)]));
                            ExecutableToolResult::ok_text(parts.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n"))
                        }
                        Err(e) => ExecutableToolResult::error_text(
                            office_hours_t_replace("officeHours.gbrainCliFailed", lang, &[("message", &e.to_string())]),
                            "cli failed".into(),
                        ),
                    }
                })
            }),
        })
    }
}
```

- [ ] 在 `tests` 模块追加 mock CLI 与测试：

```rust
#[derive(Clone)]
struct MockGbrainCli {
    result: std::sync::Mutex<Result<String, String>>,
    captured: std::sync::Mutex<Vec<(Vec<String>, String)>>,
}

impl MockGbrainCli {
    fn success() -> Self {
        Self { result: std::sync::Mutex::new(Ok("ok".into())), captured: std::sync::Mutex::new(vec![]) }
    }
    fn failure(message: &str) -> Self {
        Self { result: std::sync::Mutex::new(Err(message.into())), captured: std::sync::Mutex::new(vec![]) }
    }
}

#[async_trait]
impl GbrainCli for MockGbrainCli {
    async fn run(&self, args: &[String], cwd: &str) -> anyhow::Result<String> {
        self.captured.lock().unwrap().push((args.to_vec(), cwd.into()));
        match self.result.lock().unwrap().clone() {
            Ok(s) => Ok(s),
            Err(e) => Err(anyhow::anyhow!("{}", e)),
        }
    }
}

#[tokio::test]
async fn sync_artifact_mcp_available() {
    let mcp = Arc::new(super::mock::MockMcpProvider { gbrain_available: std::sync::Mutex::new(true) });
    let kaos = Arc::new(super::mock::MockKaos::new("/workspace"));
    kaos.insert("/workspace/design.md", "# Design");
    kaos.insert("/workspace/.gbrain-source", "my-source");
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::OfficeHours)
            .with_mcp(mcp)
            .with_kaos(kaos)
            .with_file_path("/workspace/design.md"),
    );
    let tool = SyncOfficeHoursArtifactTool::new_with_runner(provider, Arc::new(MockGbrainCli::success()));
    let exec = tool.resolve_execution(serde_json::json!({"designFilePath": "/workspace/design.md"})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("gbrain MCP server is connected"));
    assert!(text.contains("Target source: my-source"));
    assert!(text.contains("ready for sync via MCP"));
}

#[tokio::test]
async fn sync_artifact_cli_success() {
    let mcp = Arc::new(super::mock::MockMcpProvider { gbrain_available: std::sync::Mutex::new(false) });
    let kaos = Arc::new(super::mock::MockKaos::new("/workspace"));
    kaos.insert("/workspace/design.md", "# Design");
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::OfficeHours)
            .with_mcp(mcp)
            .with_kaos(kaos)
            .with_file_path("/workspace/design.md"),
    );
    let cli = Arc::new(MockGbrainCli::success());
    let tool = SyncOfficeHoursArtifactTool::new_with_runner(provider.clone(), cli.clone());
    let exec = tool.resolve_execution(serde_json::json!({"designFilePath": "/workspace/design.md"})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    assert!(result.to_text().contains("Design artifact synced via gbrain CLI"));
    assert!(result.to_text().contains("File: /workspace/design.md"));
    let captured = cli.captured.lock().unwrap();
    assert_eq!(captured[0].0, vec!["artifact", "add", "/workspace/design.md"]);
    assert_eq!(captured[0].1, "/workspace");
}

#[tokio::test]
async fn sync_artifact_cli_failure() {
    let mcp = Arc::new(super::mock::MockMcpProvider { gbrain_available: std::sync::Mutex::new(false) });
    let kaos = Arc::new(super::mock::MockKaos::new("/workspace"));
    kaos.insert("/workspace/design.md", "# Design");
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::OfficeHours)
            .with_mcp(mcp)
            .with_kaos(kaos)
            .with_file_path("/workspace/design.md"),
    );
    let tool = SyncOfficeHoursArtifactTool::new_with_runner(provider, Arc::new(MockGbrainCli::failure("command not found")));
    let exec = tool.resolve_execution(serde_json::json!({"designFilePath": "/workspace/design.md"})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("gbrain CLI sync failed: command not found"));
}

#[tokio::test]
async fn sync_artifact_design_file_not_found() {
    let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::OfficeHours));
    let tool = SyncOfficeHoursArtifactTool::new(provider);
    let exec = tool.resolve_execution(serde_json::json!({"designFilePath": "/workspace/missing.md"})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Design file not found at /workspace/missing.md"));
}

#[tokio::test]
async fn sync_artifact_fails_when_not_active() {
    let provider = Arc::new(MockSessionModeProvider::inactive());
    let tool = SyncOfficeHoursArtifactTool::new(provider);
    let exec = tool.resolve_execution(serde_json::json!({"designFilePath": "/workspace/design.md"})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Office hours mode is not active"));
}
```

- [ ] 确认 Task 2 的 `MockSessionModeProvider` 已实现 `#[derive(Clone)]`、`kaos: Arc<MockKaos>` 字段、`with_kaos(...)` 方法（已在 Task 2 编辑中完成）。

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::office_hours
```

Expected: 34 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): add SyncOfficeHoursArtifactTool`。

---

## Local Self-Review

- [ ] 1. Spec-coverage table：

| Roadmap § / Requirement | Task(s) | Status |
|---|---|---|
| 4.4.5.2 office-hours 工具集（8 个工具） | Task 2–9 | covered |
| `EnterOfficeHoursModeTool` | Task 2 | covered |
| `ExitOfficeHoursModeTool` | Task 3 | covered |
| `SetOfficeHoursLanguageTool` | Task 4 | covered |
| `AppendLearningTool` | Task 5 | covered |
| `AppendBuilderProfileTool` | Task 6 | covered |
| `SearchLearningsTool` | Task 7 | covered |
| `EnsureClaudeMdRoutingTool` | Task 8 | covered |
| `SyncOfficeHoursArtifactTool`（MCP + CLI fallback） | Task 9 | covered |
| office-hours 本地化字符串表 | Task 1 | covered |
| office-hours 入口提示语 | Task 1 | covered |

- [ ] 2. Placeholder scan：无 TODO/TBD；`GbrainCli` trait 与 `RealGbrainCli` 均为完整实现；`MockSessionModeProvider` 的 `Clone`/`with_kaos`/`mock_kaos` 已在 Task 2 一并补齐。
- [ ] 3. No phantom tasks：每个 Task 都产生真实代码、内联测试与 commit；不存在 `--allow-empty` 或 "already done in Task N"。
- [ ] 4. Dependency soundness：所有 `Depends on:` 均指向 Task 1 或 Task 2（本 Part 内更早的任务）或 `infra.md` Task 3（Part 1 已完成）。
- [ ] 5. Caller & build soundness：本 Part 未修改 Part 1 的共享签名；每个 Task 末尾均运行 `cargo check --workspace --all-targets`。
- [ ] 6. Test-the-risk：
  - `EnterOfficeHoursModeTool`：断言 `provider.entered` 包含 `OfficeHours`。
  - `ExitOfficeHoursModeTool`：断言 `provider.exited` 为 true，且输出包含保存路径。
  - `SetOfficeHoursLanguageTool`：断言 `provider.user_language()` 变为 `Zh`。
  - `AppendLearningTool` / `AppendBuilderProfileTool`：断言 mock store 中记录了正确的字段值。
  - `SearchLearningsTool`：断言返回格式与输入学习条目一一对应。
  - `EnsureClaudeMdRoutingTool`：断言 mock kaos 中的 `AGENTS.md` 内容按状态创建/追加/不变。
  - `SyncOfficeHoursArtifactTool`：断言 mock CLI runner 捕获到正确的参数与 cwd。
- [ ] 7. Type consistency：`Language`、`SessionModeKind`、`SessionModeProvider`、`OfficeHoursStateStore`、`McpProvider`、`SessionModeContext` 均使用 Part 1 定义的签名；`LearningEntry`/`BuilderProfileEntry` 的字段名与 `#[serde(rename_all = "camelCase")]` 与 TS 一致。

