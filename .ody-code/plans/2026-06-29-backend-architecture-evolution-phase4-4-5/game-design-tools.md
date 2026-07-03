# Part 4 — Game-Design 工具集（8 个内置工具）

**Goal:** 将 TypeScript 的 8 个 game-design 会话模式工具迁移到 Rust 的 `tools-rs` 中，使其在 `agent-rs` 侧通过 `SessionModeProvider` trait 解耦，行为与 TS 实现等价。

**Architecture:** 所有 game-design 工具集中在 `tools-rs/src/builtin/session_mode/game_design.rs` 一个文件中，包含本地化字符串表、入口提示语、工具实现与测试。工具只依赖 Part 1 定义的 `SessionModeProvider` 及其卫星 trait（`GameDesignStateStore`、`McpProvider`、`SessionModeContext`、`TelemetryClient`），不直接引用 `agent-rs`。测试使用本 Part 自包含的 `MockSessionModeProvider`，保证 Part 2–4 之间互不依赖。

**Tech stack:** Rust (`tools-rs`), `serde_json`, `async-trait`, `chrono`（生成 ISO 时间戳）。

**Depends on:** `2026-06-29-backend-architecture-evolution-phase4-4/infra.md`（Task 3，trait 表面已存在）。

> For executing workers: implement this plan task-by-task（建议每个 Task 用一个新 subagent/Task，避免单会话退化）。步骤使用 - [ ] 复选框跟踪。

---

## File Structure

| File | Responsibility |
|---|---|
| `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs` | Game-design 本地化字符串、`game_design_entry_reminder`、8 个工具 struct、共享 mock provider、所有内联测试。 |
| `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs` | 暴露 `pub mod game_design;`（Part 1 已预留，本 Part 填充实现）。 |
| `rust-ody/crates/agent-rs/src/tool/manager.rs` | 在 `core_builtin_tools()` / `loop_tools()` 中注册 8 个 game-design 工具（Part 5 最终接线；本 Part 只确保工具自身可编译、可测试）。 |

---

## Dependency Overview

```
Task 1: 本地化字符串表 + game-design 入口提示语
  │
  ├──► Task 2: EnterGameDesignModeTool
  ├──► Task 3: ExitGameDesignModeTool
  ├──► Task 4: SetGameDesignLanguageTool
  ├──► Task 5: AppendGameDesignLearningTool
  ├──► Task 6: AppendGameDesignProfileTool
  ├──► Task 7: SearchGameDesignLearningsTool
  ├──► Task 8: EnsureGameDesignRoutingTool
  └──► Task 9: SyncGameDesignArtifactTool
```

所有工具共享 Task 1 的字符串表与入口提示语；每个工具单独成 Task，保持 2–5 分钟级别的可提交粒度。Mock provider 在 Task 2 中引入，后续 Task 的测试直接复用。

---

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| TS `gameDesignEntryReminder` 是一份完整 workflow contract，Rust 端口需逐字对齐。 | 将 contract 文本作为 `const GAME_DESIGN_ENTRY_REMINDER: &str` 完整嵌入 `game_design.rs`，并添加断言检查关键 marker。 |
| `AppendGameDesignLearningTool` / `SearchGameDesignLearningsTool` 依赖 `GameDesignStateStore`，Part 1 trait 只有方法签名。 | 测试使用本 Part 自包含的 `MockGameDesignStateStore`，记录所有 append 操作并支持按 `branch` 过滤。 |
| `AppendGameDesignProfileTool` 的字段（`pillars`、`audience`、`platform`、`genre`、`signals`）与 TS `GameDesignProfileEntry` 必须一一对应。 | 直接复用 Part 1 定义的 `GameDesignProfileEntry` struct，测试断言 JSON 化后的字段名与值。 |
| `SyncGameDesignArtifactTool` 需要 MCP gbrain 检测与 CLI fallback。 | `McpProvider::gbrain_available()` 提供检测能力；CLI fallback 使用本地 `GbrainCli` trait，生产环境用 `std::process::Command`，测试注入 mock。 |
| 本地化字符串需要与 TS `translations.ts` 保持一致。 | 端口 exact key/value；`SetGameDesignLanguageTool` 只接受 `en`/`zh`，与 TS 一致。 |

---

## Task 1: 本地化字符串表与 game-design 入口提示语

**Depends on:** `infra.md` Task 3（`SessionModeProvider`、`Language`、`GameDesignStateStore` 等 trait 已定义）。

**Files:**
- Create: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs`（将 `pub mod game_design;` 从 stub 变为真实模块）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`（内联 `#[cfg(test)]`）

**Why:** 后续 8 个工具都依赖同一套 `gameDesign.*` 本地化字符串与入口提示语。先一次性端口这些静态内容，保证文案与 TS 完全一致，并在此 Task 中就通过测试锁定 key/value。

**Steps:**

- [ ] 创建 `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`，先写入本地化与入口提示语骨架：

```rust
use std::sync::Arc;
use async_trait::async_trait;
use serde_json::Value;

use crate::builtin::session_mode::{
    GameDesignProfileEntry, GameDesignStateStore, Language, LearningEntry, McpProvider,
    SessionModeContext, SessionModeKind, SessionModeProvider, TelemetryClient,
};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};

/// Game-design 本地化字符串表，与 TS `translations.ts` 逐 key 对齐。
fn game_design_t(key: &str, lang: Language) -> String {
    match (key, lang) {
        ("gameDesign.entered", Language::En) => "game-design mode is now active.".into(),
        ("gameDesign.alreadyActive", Language::En) => "game-design mode is already active. Use ExitGameDesignMode when the session is complete.".into(),
        ("gameDesign.anotherModeActive", Language::En) => "Another session mode is already active. Exit it first before entering game-design mode.".into(),
        ("gameDesign.failedToEnter", Language::En) => "Failed to enter game-design mode: {message}".into(),
        ("gameDesign.sessionComplete", Language::En) => "Game-design session complete.".into(),
        ("gameDesign.designDocSaved", Language::En) => "Design document saved to: {path}".into(),
        ("gameDesign.appWillExit", Language::En) => "The application will now exit.".into(),
        ("gameDesign.profileAppended", Language::En) => "Builder profile entry appended successfully.".into(),
        ("gameDesign.learningRecorded", Language::En) => "Learning \"{key}\" recorded successfully.".into(),
        ("gameDesign.noLearnings", Language::En) => "No past learnings found.".into(),
        ("gameDesign.learningsHeader", Language::En) => "Found {count} learning(s):".into(),
        ("gameDesign.learningTypeLabel", Language::En) => "Type".into(),
        ("gameDesign.learningInsightLabel", Language::En) => "Insight".into(),
        ("gameDesign.learningConfidenceLabel", Language::En) => "Confidence".into(),
        ("gameDesign.learningDateLabel", Language::En) => "Date".into(),
        ("gameDesign.learningBranchLabel", Language::En) => "Branch".into(),
        ("gameDesign.modeNotActive", Language::En) => "Game-design mode is not active.".into(),
        ("gameDesign.designFileNotFound", Language::En) => "Design file not found at {path}.".into(),
        ("gameDesign.gbrainConnected", Language::En) => "gbrain MCP server is connected.".into(),
        ("gameDesign.gbrainTargetSource", Language::En) => "Target source: {source}".into(),
        ("gameDesign.gbrainNoSourcePin", Language::En) => "No .gbrain-source pin found.".into(),
        ("gameDesign.gbrainReadyForSync", Language::En) => "Design artifact at {path} is ready for sync via MCP.".into(),
        ("gameDesign.gbrainSynced", Language::En) => "Design artifact synced via gbrain CLI.".into(),
        ("gameDesign.gbrainFile", Language::En) => "File: {path}".into(),
        ("gameDesign.gbrainCliFailed", Language::En) => "gbrain CLI sync failed: {message}. Ensure the gbrain CLI is installed and configured.".into(),
        ("gameDesign.agentsMdCreated", Language::En) => "AGENTS.md created at {path} with ## Skill routing section.".into(),
        ("gameDesign.agentsMdUpdated", Language::En) => "Appended ## Skill routing section to AGENTS.md at {path}.".into(),
        ("gameDesign.agentsMdAlreadyHasRouting", Language::En) => "AGENTS.md already has a ## Skill routing section — no changes needed.".into(),
        ("gameDesign.failedToEnsureRouting", Language::En) => "Failed to ensure AGENTS.md routing: {message}".into(),
        ("gameDesign.failedToSyncArtifact", Language::En) => "Failed to sync design artifact: {message}".into(),
        ("gameDesign.languageSet", Language::En) => "User language set to {language}.".into(),

        ("gameDesign.entered", Language::Zh) => "Game Design 模式已激活。".into(),
        ("gameDesign.alreadyActive", Language::Zh) => "Game Design 模式已经处于激活状态。会话结束后请调用 ExitGameDesignMode。".into(),
        ("gameDesign.anotherModeActive", Language::Zh) => "另一个会话模式已经激活。请先退出该模式再进入 Game Design。".into(),
        ("gameDesign.failedToEnter", Language::Zh) => "进入 Game Design 模式失败：{message}".into(),
        ("gameDesign.sessionComplete", Language::Zh) => "Game Design 会话已结束。".into(),
        ("gameDesign.designDocSaved", Language::Zh) => "设计文档已保存至：{path}".into(),
        ("gameDesign.appWillExit", Language::Zh) => "应用即将退出。".into(),
        ("gameDesign.profileAppended", Language::Zh) => "Builder 档案条目已追加成功。".into(),
        ("gameDesign.learningRecorded", Language::Zh) => "学习洞察 \"{key}\" 已记录成功。".into(),
        ("gameDesign.noLearnings", Language::Zh) => "未找到过往学习洞察。".into(),
        ("gameDesign.learningsHeader", Language::Zh) => "找到 {count} 条学习洞察：".into(),
        ("gameDesign.learningTypeLabel", Language::Zh) => "类型".into(),
        ("gameDesign.learningInsightLabel", Language::Zh) => "洞察".into(),
        ("gameDesign.learningConfidenceLabel", Language::Zh) => "置信度".into(),
        ("gameDesign.learningDateLabel", Language::Zh) => "日期".into(),
        ("gameDesign.learningBranchLabel", Language::Zh) => "分支".into(),
        ("gameDesign.modeNotActive", Language::Zh) => "Game Design 模式未激活。".into(),
        ("gameDesign.designFileNotFound", Language::Zh) => "在 {path} 未找到设计文件。".into(),
        ("gameDesign.gbrainConnected", Language::Zh) => "gbrain MCP 服务器已连接。".into(),
        ("gameDesign.gbrainTargetSource", Language::Zh) => "目标源：{source}".into(),
        ("gameDesign.gbrainNoSourcePin", Language::Zh) => "未找到 .gbrain-source 固定文件。".into(),
        ("gameDesign.gbrainReadyForSync", Language::Zh) => "{path} 处的设计制品已准备好通过 MCP 同步。".into(),
        ("gameDesign.gbrainSynced", Language::Zh) => "设计制品已通过 gbrain CLI 同步。".into(),
        ("gameDesign.gbrainFile", Language::Zh) => "文件：{path}".into(),
        ("gameDesign.gbrainCliFailed", Language::Zh) => "gbrain CLI 同步失败：{message}。请确保 gbrain CLI 已安装并配置。".into(),
        ("gameDesign.agentsMdCreated", Language::Zh) => "已在 {path} 创建 AGENTS.md，并添加 ## Skill routing 章节。".into(),
        ("gameDesign.agentsMdUpdated", Language::Zh) => "已在 {path} 的 AGENTS.md 中追加 ## Skill routing 章节。".into(),
        ("gameDesign.agentsMdAlreadyHasRouting", Language::Zh) => "AGENTS.md 已包含 ## Skill routing 章节，无需更改。".into(),
        ("gameDesign.failedToEnsureRouting", Language::Zh) => "确保 AGENTS.md 路由失败：{message}".into(),
        ("gameDesign.failedToSyncArtifact", Language::Zh) => "同步设计制品失败：{message}".into(),
        ("gameDesign.languageSet", Language::Zh) => "用户语言已设置为 {language}。".into(),

        _ => key.into(),
    }
}

/// 将 `gameDesign.*` key 的占位符 `{name}` 替换为实际值。
fn game_design_t_replace(key: &str, lang: Language, replacements: &[(&str, &str)]) -> String {
    let mut s = game_design_t(key, lang);
    for (k, v) in replacements {
        s = s.replace(&format!("{{{}}}", k), v);
    }
    s
}

/// 严格匹配 TS `SupportedLanguage = 'en' | 'zh'`。
fn parse_supported_language(s: &str) -> Option<Language> {
    match s {
        "en" => Some(Language::En),
        "zh" => Some(Language::Zh),
        _ => None,
    }
}

/// 进入 game-design 时返回的入口提示语，与 TS `gameDesignEntryReminder` 对齐。
pub fn game_design_entry_reminder(design_file_path: Option<&str>) -> String {
    let path = design_file_path.unwrap_or("(not yet assigned)");
    let companion_dir = path.strip_suffix(".md").unwrap_or(path);
    [
        "**Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.",
        "",
        "game-design mode is now active. Your job is to act as a game design partner —",
        "guide the user through a complete game design process based on the 100 Principles of Game Design.",
        "",
        "## HARD GATES",
        "- Do NOT write code. Your output is a game design document.",
        "- Ask questions to clarify the vision, audience, and constraints.",
        &format!("- Design file (write ONLY to this path): {}", path),
        &format!("- You may create companion .md files in the {}/ subdirectory.", companion_dir),
        "",
        "## Available Game Design Skills",
        "Use the Skill tool to invoke specialized game design skills (game-design/*) for",
        "deep dives into specific areas: flow state, difficulty adjustment, puzzle design,",
        "player psychology, visual guidance, prototyping, team management, and more.",
        "",
        "## Core Workflow (from skill.md)",
        "",
        "Follow these phases in order. Move forward only when the current phase has",
        "enough clarity to support the next one.",
        "",
        "### Phase 1: 概念定义",
        "1. 定义 3 根支柱 — 用动作动词描述核心玩法，组合成一句话。",
        "2. 写问题陈述 — 具体焦点 + 可量化结果 + 清晰表达。用 80/20 法则聚焦核心功能。",
        "3. 约束三角 — 快、便宜、好，只能选两个。砍范围 > 砍质量。",
        "",
        "### Phase 2: 核心循环设计",
        "核心循环 = 玩家愿意反复做的有趣行为。行动→结果→反应→重复。",
        "用动词描述核心动作。必须易懂、易操作、有直接反馈。",
        "警告：核心循环有缺陷 → 其他元素无法补救。",
        "",
        "### Phase 3: 机制与平衡",
        "难度设计：三阶段（入门/练习/心流），挑战略高于当前能力。",
        "动态难度：暗中调整，监控连续失败/成功率/耗时。",
        "快速平衡法：对核心变量做 2x 或 0.5x 极端调整测试。",
        "奖惩系统：生命/Game Over、属性衰退、固定/随机奖励。",
        "",
        "### Phase 4: 关卡与体验",
        "挑战分类：记忆型（试错/模式识别）vs 技能型（身体/心智能力）。",
        "谜题设计：保持心流、渐进提示、确定性、清晰性。",
        "节奏控制：人类注意力极限 7-10 分钟，每 ~7 分钟展示新元素。",
        "环境叙事：用涂鸦/门窗/NPC对话/私人空间讲故事。",
        "",
        "### Phase 5: 视觉与交互",
        "视觉引导：可供性（视觉暗示交互）、注意力捕获（面孔>运动>意外）、寻路。",
        "Fitts 定律：移动时间 = f(距离, 目标大小)，常用元素放近放大。",
        "Hick 定律：决策时间随选项数对数增长，最优 3-6 个选项。",
        "黄金比例：Φ=1.618，UI 布局/建筑比例/环境艺术。",
        "",
        "### Phase 6: 玩家心理",
        "认知偏差清单：确认偏差、可得性偏差、锚定效应、框架效应。",
        "决策设计：三角性（低风险低回报 vs 高风险高回报路径）。",
        "错误处理：运动控制/流程错误/遗漏错误/错误行动的分类与应对。",
        "",
        "### Phase 7: 原型与测试",
        "纸面原型（UI/卡牌/桌游）和数字原型（操作手感/时机）。",
        "测试：一次性测试（首次印象）、黑盒/白盒/压力测试。",
        "循环：原型→测试→分析→迭代。",
        "",
        "### Phase 8: 团队管理",
        "共享愿景、多样性悖论、流程选择（瀑布 vs 敏捷）、沟通原则。",
        "",
        "## Output Conventions",
        "- Suggest concrete principles by name.",
        "- Give actionable next steps, not vague advice.",
        "- Use tables to compare options and trade-offs.",
        "- Tag decisions: [C:USER] for user-confirmed, [C:INFERRED] for inferred.",
        "- Include an ## Assumptions section.",
        "",
        "## Output File",
        &format!("- Main document: {}", path),
        &format!("- Companion files: {}/<topic>.md", companion_dir),
        "- Call SyncGameDesignArtifact when ready to persist.",
        "- Call ExitGameDesignMode when the design is complete.",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_design_t_returns_english_by_default() {
        assert_eq!(
            game_design_t("gameDesign.modeNotActive", Language::En),
            "Game-design mode is not active."
        );
    }

    #[test]
    fn game_design_t_returns_chinese() {
        assert_eq!(
            game_design_t("gameDesign.modeNotActive", Language::Zh),
            "Game Design 模式未激活。"
        );
    }

    #[test]
    fn game_design_t_replace_substitutes_placeholders() {
        let s = game_design_t_replace(
            "gameDesign.learningRecorded",
            Language::En,
            &[("key", "flow_state")],
        );
        assert_eq!(s, "Learning \"flow_state\" recorded successfully.");
    }

    #[test]
    fn parse_supported_language_accepts_en_zh() {
        assert_eq!(parse_supported_language("en"), Some(Language::En));
        assert_eq!(parse_supported_language("zh"), Some(Language::Zh));
        assert_eq!(parse_supported_language("fr"), None);
    }

    #[test]
    fn entry_reminder_contains_hard_gates_and_path() {
        let msg = game_design_entry_reminder(Some(".ody-code/game-design/2026-06-29-foo.md"));
        assert!(msg.contains("game-design mode is now active"));
        assert!(msg.contains("## HARD GATES"));
        assert!(msg.contains(".ody-code/game-design/2026-06-29-foo.md"));
        assert!(msg.contains(".ody-code/game-design/2026-06-29-foo/"));
        assert!(msg.contains("Do NOT write code"));
    }

    #[test]
    fn entry_reminder_handles_missing_path() {
        let msg = game_design_entry_reminder(None);
        assert!(msg.contains("(not yet assigned)"));
    }
}
```

- [ ] 在 `rust-ody/crates/tools-rs/src/builtin/session_mode/mod.rs` 中确认 `pub mod game_design;` 已存在且指向真实文件（Part 1 已声明，本文件创建后自动生效）。

- [ ] 运行测试并确认通过：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::game_design::tests
```

Expected: 6 个测试全部通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): port game-design i18n strings and entry reminder`。

---

## Task 2: 实现 `EnterGameDesignModeTool` 并引入测试用 Mock Provider

**Depends on:** Task 1（本地化字符串表与入口提示语已就绪）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`（追加 `EnterGameDesignModeTool` 与 `#[cfg(test)]` mock providers）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`

**Why:** `EnterGameDesignModeTool` 是 game-design 工作流的入口。本 Task 同时引入一套自包含的 mock provider（mock kaos、state store、MCP、telemetry），后续 7 个工具的测试直接复用，避免跨 Part 依赖 Part 3 的 mock。

**Steps：**

- [ ] 在 `game_design.rs` 的 `#[cfg(test)] mod tests` 上方追加 mock providers（这些 mock 仅供本文件测试使用，但会被后续 Task 的测试引用）：

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
    pub struct MockOfficeHoursStore;

    #[async_trait]
    impl OfficeHoursStateStore for MockOfficeHoursStore {
        async fn append_profile(&self, _entry: BuilderProfileEntry) -> anyhow::Result<()> { Ok(()) }
        async fn append_learning(&self, _entry: LearningEntry) -> anyhow::Result<()> { Ok(()) }
        async fn search_learnings(
            &self,
            _limit: usize,
            _cross_project: bool,
        ) -> anyhow::Result<Vec<LearningEntry>> {
            Ok(vec![])
        }
    }

    #[derive(Default)]
    pub struct MockGameDesignStateStore {
        pub learnings: Mutex<Vec<LearningEntry>>,
        pub profiles: Mutex<Vec<GameDesignProfileEntry>>,
    }

    #[async_trait]
    impl GameDesignStateStore for MockGameDesignStateStore {
        async fn append_profile(&self, entry: GameDesignProfileEntry) -> anyhow::Result<()> {
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
            branch: Option<String>,
        ) -> anyhow::Result<Vec<LearningEntry>> {
            let all = self.learnings.lock().unwrap().clone();
            let filtered: Vec<_> = all
                .into_iter()
                .rev()
                .filter(|l| branch.as_ref().map_or(true, |b| l.branch.as_ref() == Some(b)))
                .take(limit)
                .collect();
            Ok(filtered)
        }
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
                game_design_store: Arc::new(MockGameDesignStateStore::default()),
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
            *s.file_path.lock().unwrap() = Some(".ody-code/game-design/2026-06-29-active.md".into());
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

        pub fn with_game_design_store(self, store: Arc<dyn GameDesignStateStore>) -> Self {
            self.game_design_store = store;
            self
        }

        pub fn with_kaos(self, kaos: Arc<MockKaos>) -> Self {
            self.kaos = kaos;
            self
        }

        pub fn with_mcp(self, mcp: Arc<dyn McpProvider>) -> Self {
            self.mcp = mcp;
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
                    SessionModeKind::Plan => ".ody-code/plans/2026-06-29-plan.md",
                    SessionModeKind::Design => ".ody-code/designs/2026-06-29-design.md",
                    SessionModeKind::OfficeHours => ".ody-code/products/2026-06-29-office-hours.md",
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

- [ ] 在同一文件 `game_design.rs` 中 mock 模块之后追加 `EnterGameDesignModeTool`：

```rust
pub struct EnterGameDesignModeTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl EnterGameDesignModeTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for EnterGameDesignModeTool {
    fn name(&self) -> &str { "EnterGameDesignMode" }

    fn description(&self) -> &str {
        "Enter game-design mode to begin a guided game design session based on the 100 Principles of Game Design framework. This mode restricts operations to producing a game design document under .ody-code/game-design/."
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
            description: "Requesting to enter game-design mode".into(),
            approval_rule: "EnterGameDesignMode".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                Box::pin(async move {
                    let lang = provider.user_language();
                    if provider.is_session_mode_active() {
                        if provider.session_mode_kind() == Some(SessionModeKind::GameDesign) {
                            return ExecutableToolResult::error_text(
                                game_design_t("gameDesign.alreadyActive", lang),
                                "already active".into(),
                            );
                        }
                        return ExecutableToolResult::error_text(
                            game_design_t("gameDesign.anotherModeActive", lang),
                            "another mode active".into(),
                        );
                    }

                    if let Err(e) = provider.enter_session_mode(SessionModeKind::GameDesign).await {
                        return ExecutableToolResult::error_text(
                            game_design_t_replace("gameDesign.failedToEnter", lang, &[("message", &e.to_string())]),
                            "enter failed".into(),
                        );
                    }

                    let msg = game_design_entry_reminder(provider.session_mode_file_path().as_deref());
                    ExecutableToolResult::ok_text(msg)
                })
            }),
        })
    }
}
```

- [ ] 在 `tests` 模块内追加 `EnterGameDesignModeTool` 的测试，并添加 `empty_context` 辅助函数：

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
    async fn enter_game_design_succeeds_when_inactive() {
        let provider = Arc::new(MockSessionModeProvider::inactive());
        let tool = EnterGameDesignModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(empty_context()).await;
        assert!(!result.is_error);
        let text = result.to_text();
        assert!(text.contains("game-design mode is now active"));
        assert!(text.contains("## HARD GATES"));
        assert!(text.contains("Do NOT write code"));
        assert_eq!(provider.entered.lock().unwrap().as_slice(), &[SessionModeKind::GameDesign]);
        assert!(provider.session_mode_file_path().is_some());
    }

    #[tokio::test]
    async fn enter_game_design_fails_when_already_active() {
        let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::GameDesign));
        let tool = EnterGameDesignModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(empty_context()).await;
        assert!(result.is_error);
        assert!(result.to_text().contains("game-design mode is already active"));
        assert!(provider.entered.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn enter_game_design_fails_when_another_mode_active() {
        let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::Plan));
        let tool = EnterGameDesignModeTool::new(provider.clone());
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
cd rust-ody && cargo test -p tools-rs builtin::session_mode::game_design
```

Expected: 9 个测试全部通过（Task 1 的 6 个 + 本 Task 的 3 个）。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): add EnterGameDesignModeTool with mock providers`。

---

## Task 3: 实现 `ExitGameDesignModeTool`

**Depends on:** Task 2（`MockSessionModeProvider` 与 `EnterGameDesignModeTool` 模式已就绪）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`（追加 `ExitGameDesignModeTool` 与测试）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`

**Why:** `ExitGameDesignModeTool` 结束 game-design 会话，需要读取当前 design doc 路径、调用 `exit_session_mode`，并返回包含保存路径的本地化告别消息。

**Steps：**

- [ ] 在 `game_design.rs` 中 `EnterGameDesignModeTool` 之后追加 `ExitGameDesignModeTool`：

```rust
pub struct ExitGameDesignModeTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl ExitGameDesignModeTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for ExitGameDesignModeTool {
    fn name(&self) -> &str { "ExitGameDesignMode" }

    fn description(&self) -> &str {
        "Exit game-design mode, save the final design document, and return to normal mode."
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
            description: "Requesting to exit game-design mode".into(),
            approval_rule: "ExitGameDesignMode".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                Box::pin(async move {
                    let lang = provider.user_language();
                    if !provider.is_session_mode_active() || provider.session_mode_kind() != Some(SessionModeKind::GameDesign) {
                        return ExecutableToolResult::error_text(
                            game_design_t("gameDesign.modeNotActive", lang),
                            "not active".into(),
                        );
                    }

                    let path = provider.session_mode_file_path();
                    if let Err(e) = provider.exit_session_mode().await {
                        return ExecutableToolResult::error_text(
                            format!("Failed to exit game-design mode: {}", e),
                            "exit failed".into(),
                        );
                    }

                    let mut parts = vec![game_design_t("gameDesign.sessionComplete", lang)];
                    if let Some(p) = path {
                        parts.push(game_design_t_replace("gameDesign.designDocSaved", lang, &[("path", &p)]));
                    }
                    parts.push(game_design_t("gameDesign.appWillExit", lang));

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
async fn exit_game_design_succeeds_when_active() {
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::GameDesign)
            .with_file_path(".ody-code/game-design/2026-06-29-foo.md"),
    );
    let tool = ExitGameDesignModeTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("Game-design session complete"));
    assert!(text.contains("Design document saved to: .ody-code/game-design/2026-06-29-foo.md"));
    assert!(text.contains("The application will now exit"));
    assert!(*provider.exited.lock().unwrap());
}

#[tokio::test]
async fn exit_game_design_fails_when_not_active() {
    let provider = Arc::new(MockSessionModeProvider::inactive());
    let tool = ExitGameDesignModeTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Game-design mode is not active"));
}

#[tokio::test]
async fn exit_game_design_fails_when_wrong_mode_active() {
    let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::OfficeHours));
    let tool = ExitGameDesignModeTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Game-design mode is not active"));
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::game_design
```

Expected: 12 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): add ExitGameDesignModeTool`。

---

## Task 4: 实现 `SetGameDesignLanguageTool`

**Depends on:** Task 2（`MockSessionModeProvider` 已就绪）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`（追加 `SetGameDesignLanguageTool` 与测试）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`

**Why:** 该工具在 game-design 会话开始时记录用户语言，后续所有本地化输出据此切换。需要严格校验输入只能是 `en` 或 `zh`，与 TS 一致。

**Steps：**

- [ ] 追加输入类型与 `SetGameDesignLanguageTool`：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetGameDesignLanguageInput {
    pub language: String,
}

pub struct SetGameDesignLanguageTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl SetGameDesignLanguageTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for SetGameDesignLanguageTool {
    fn name(&self) -> &str { "SetGameDesignLanguage" }

    fn description(&self) -> &str {
        "Set the user language for the game-design session to 'en' or 'zh'."
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
        let input: SetGameDesignLanguageInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let provider = Arc::clone(&self.provider);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Setting game-design language".into(),
            approval_rule: "SetGameDesignLanguage".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let input = input.clone();
                Box::pin(async move {
                    let current_lang = provider.user_language();
                    if !provider.is_session_mode_active() || provider.session_mode_kind() != Some(SessionModeKind::GameDesign) {
                        return ExecutableToolResult::error_text(
                            game_design_t("gameDesign.modeNotActive", current_lang),
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
                        game_design_t_replace("gameDesign.languageSet", lang, &[("language", &input.language)])
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
async fn set_language_succeeds_in_game_design() {
    let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::GameDesign));
    let tool = SetGameDesignLanguageTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({"language": "zh"})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    assert!(result.to_text().contains("用户语言已设置为 zh"));
    assert_eq!(provider.user_language(), Language::Zh);
}

#[tokio::test]
async fn set_language_fails_outside_game_design() {
    let provider = Arc::new(MockSessionModeProvider::inactive());
    let tool = SetGameDesignLanguageTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({"language": "en"})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Game-design mode is not active"));
}

#[tokio::test]
async fn set_language_rejects_unsupported_language() {
    let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::GameDesign));
    let tool = SetGameDesignLanguageTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({"language": "fr"})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Unsupported language: fr"));
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::game_design
```

Expected: 15 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): add SetGameDesignLanguageTool`。

---

## Task 5: 实现 `AppendGameDesignLearningTool`

**Depends on:** Task 2（`MockSessionModeProvider` 与 `MockGameDesignStateStore` 已就绪）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`（追加 `AppendGameDesignLearningTool` 与测试）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`

**Why:** 该工具将学习洞察写入 `GameDesignStateStore`，是后续 Search 工具的数据来源。需要断言写入的 `LearningEntry` 包含正确的 `skill`、`source`、`ts` 与输入字段。

**Steps：**

- [ ] 追加输入类型与 `AppendGameDesignLearningTool`：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppendGameDesignLearningInput {
    #[serde(rename = "type")]
    pub type_: String,
    pub key: String,
    pub insight: String,
    pub confidence: f64,
    #[serde(default)]
    pub branch: Option<String>,
}

pub struct AppendGameDesignLearningTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl AppendGameDesignLearningTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for AppendGameDesignLearningTool {
    fn name(&self) -> &str { "AppendGameDesignLearning" }

    fn description(&self) -> &str {
        "Record a learning insight discovered during game design: type (operational/eureka), key, insight text, and confidence score."
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
        let input: AppendGameDesignLearningInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let provider = Arc::clone(&self.provider);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Appending game-design learning insight".into(),
            approval_rule: "AppendGameDesignLearning".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let input = input.clone();
                Box::pin(async move {
                    let lang = provider.user_language();
                    if !provider.is_session_mode_active() || provider.session_mode_kind() != Some(SessionModeKind::GameDesign) {
                        return ExecutableToolResult::error_text(
                            game_design_t("gameDesign.modeNotActive", lang),
                            "not active".into(),
                        );
                    }

                    let entry = LearningEntry {
                        ts: chrono::Utc::now().to_rfc3339(),
                        skill: "game-design".into(),
                        type_: input.type_,
                        key: input.key.clone(),
                        insight: input.insight,
                        confidence: input.confidence,
                        source: "observed".into(),
                        branch: input.branch,
                    };

                    if let Err(e) = provider.game_design_store().append_learning(entry).await {
                        return ExecutableToolResult::error_text(
                            format!("Failed to append learning: {}", e),
                            "append failed".into(),
                        );
                    }

                    ExecutableToolResult::ok_text(
                        game_design_t_replace("gameDesign.learningRecorded", lang, &[("key", &input.key)])
                    )
                })
            }),
        })
    }
}
```

- [ ] 在 `tests` 模块追加测试（注意 `chrono` 可能未加入 `tools-rs/Cargo.toml`，若缺失则在下一步添加）：

```rust
#[tokio::test]
async fn append_learning_succeeds_in_game_design() {
    let store = Arc::new(mock::MockGameDesignStateStore::default());
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::GameDesign)
            .with_game_design_store(store.clone()),
    );
    let tool = AppendGameDesignLearningTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({
        "type": "eureka",
        "key": "flow_state",
        "insight": "Core loop drives flow.",
        "confidence": 0.95,
        "branch": "feat/game-loop"
    })).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    assert!(result.to_text().contains("Learning \"flow_state\" recorded successfully."));

    let learnings = store.learnings.lock().unwrap();
    assert_eq!(learnings.len(), 1);
    assert_eq!(learnings[0].skill, "game-design");
    assert_eq!(learnings[0].type_, "eureka");
    assert_eq!(learnings[0].key, "flow_state");
    assert_eq!(learnings[0].source, "observed");
    assert_eq!(learnings[0].branch.as_deref(), Some("feat/game-loop"));
}

#[tokio::test]
async fn append_learning_fails_outside_game_design() {
    let provider = Arc::new(MockSessionModeProvider::inactive());
    let tool = AppendGameDesignLearningTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({
        "type": "operational",
        "key": "x",
        "insight": "y",
        "confidence": 0.5
    })).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Game-design mode is not active"));
}
```

- [ ] 确认 `rust-ody/crates/tools-rs/Cargo.toml` 已包含 `chrono`；若未包含，追加：

```toml
chrono = { version = "0.4", features = ["clock"] }
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::game_design
```

Expected: 17 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): add AppendGameDesignLearningTool`。

---

## Task 6: 实现 `AppendGameDesignProfileTool`

**Depends on:** Task 2（`MockSessionModeProvider` 与 `MockGameDesignStateStore` 已就绪）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`（追加 `AppendGameDesignProfileTool` 与测试）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`

**Why:** 该工具将本次 game-design 会话的摘要（pillars、audience、platform、genre、signals、designDoc）写入 `GameDesignStateStore`，用于后续 tier 计算与资源推荐。

**Steps：**

- [ ] 追加输入类型与 `AppendGameDesignProfileTool`：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppendGameDesignProfileInput {
    #[serde(rename = "mode")]
    pub mode_: String,
    pub project_slug: String,
    pub pillars: String,
    pub audience: String,
    pub platform: String,
    pub genre: String,
    #[serde(default)]
    pub design_doc: Option<String>,
    #[serde(default)]
    pub signals: Vec<String>,
}

pub struct AppendGameDesignProfileTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl AppendGameDesignProfileTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for AppendGameDesignProfileTool {
    fn name(&self) -> &str { "AppendGameDesignProfile" }

    fn description(&self) -> &str {
        "Append a builder profile entry summarizing the game design session: pillars, audience, platform, genre, and design doc path."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "mode": {
                    "type": "string",
                    "enum": ["startup", "builder"],
                    "description": "Whether this is a full design startup or a builder session."
                },
                "projectSlug": { "type": "string", "description": "Project slug." },
                "pillars": { "type": "string", "description": "The 3 design pillars as a comma-separated string." },
                "audience": { "type": "string", "description": "Target audience description." },
                "platform": { "type": "string", "description": "Target platform(s)." },
                "genre": { "type": "string", "description": "Game genre." },
                "designDoc": { "type": "string", "description": "Path to the design document. Defaults to the current game-design file path." },
                "signals": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Design signals observed."
                }
            },
            "required": ["mode", "projectSlug", "pillars", "audience", "platform", "genre"],
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let input: AppendGameDesignProfileInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let provider = Arc::clone(&self.provider);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Appending game-design profile entry".into(),
            approval_rule: "AppendGameDesignProfile".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let input = input.clone();
                Box::pin(async move {
                    let lang = provider.user_language();
                    if !provider.is_session_mode_active() || provider.session_mode_kind() != Some(SessionModeKind::GameDesign) {
                        return ExecutableToolResult::error_text(
                            game_design_t("gameDesign.modeNotActive", lang),
                            "not active".into(),
                        );
                    }

                    let design_doc = input.design_doc
                        .or_else(|| provider.session_mode_file_path())
                        .unwrap_or_default();

                    let entry = GameDesignProfileEntry {
                        date: chrono::Utc::now().to_rfc3339(),
                        mode: input.mode_,
                        project_slug: input.project_slug,
                        pillars: input.pillars,
                        audience: input.audience,
                        platform: input.platform,
                        genre: input.genre,
                        signals: input.signals,
                        design_doc,
                    };

                    if let Err(e) = provider.game_design_store().append_profile(entry).await {
                        return ExecutableToolResult::error_text(
                            format!("Failed to append game-design profile entry: {}", e),
                            "append failed".into(),
                        );
                    }

                    ExecutableToolResult::ok_text(game_design_t("gameDesign.profileAppended", lang))
                })
            }),
        })
    }
}
```

- [ ] 在 `tests` 模块追加测试：

```rust
#[tokio::test]
async fn append_profile_succeeds_in_game_design() {
    let store = Arc::new(mock::MockGameDesignStateStore::default());
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::GameDesign)
            .with_file_path(".ody-code/game-design/2026-06-29-foo.md")
            .with_game_design_store(store.clone()),
    );
    let tool = AppendGameDesignProfileTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({
        "mode": "builder",
        "projectSlug": "lunar-lander",
        "pillars": "explore, survive, upgrade",
        "audience": "casual mobile players",
        "platform": "iOS, Android",
        "genre": "arcade",
        "signals": ["flow_state", "difficulty_curve"]
    })).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    assert!(result.to_text().contains("Builder profile entry appended successfully."));

    let profiles = store.profiles.lock().unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].mode, "builder");
    assert_eq!(profiles[0].project_slug, "lunar-lander");
    assert_eq!(profiles[0].pillars, "explore, survive, upgrade");
    assert_eq!(profiles[0].design_doc, ".ody-code/game-design/2026-06-29-foo.md");
    assert_eq!(profiles[0].signals, vec!["flow_state", "difficulty_curve"]);
}

#[tokio::test]
async fn append_profile_uses_explicit_design_doc() {
    let store = Arc::new(mock::MockGameDesignStateStore::default());
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::GameDesign)
            .with_game_design_store(store.clone()),
    );
    let tool = AppendGameDesignProfileTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({
        "mode": "startup",
        "projectSlug": "x",
        "pillars": "a, b, c",
        "audience": "y",
        "platform": "z",
        "genre": "rpg",
        "designDoc": "/custom/design.md"
    })).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    assert_eq!(store.profiles.lock().unwrap()[0].design_doc, "/custom/design.md");
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::game_design
```

Expected: 19 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): add AppendGameDesignProfileTool`。

---

## Task 7: 实现 `SearchGameDesignLearningsTool`

**Depends on:** Task 2（`MockSessionModeProvider` 与 `MockGameDesignStateStore` 已就绪）以及 Task 5（`MockGameDesignStateStore::search_learnings` 已验证可写入）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`（追加 `SearchGameDesignLearningsTool` 与测试）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`

**Why:** 该工具查询 `GameDesignStateStore`，支持按 `branch` 过滤，是 Phase 6 Handoff 资源推荐的关键。需要断言返回格式与 TS 一致（包括 `Type`、`Insight`、`Confidence`、`Branch` 标签）。

**Steps：**

- [ ] 追加输入类型与 `SearchGameDesignLearningsTool`：

```rust
fn default_limit() -> usize { 10 }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchGameDesignLearningsInput {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub branch: Option<String>,
}

pub struct SearchGameDesignLearningsTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl SearchGameDesignLearningsTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for SearchGameDesignLearningsTool {
    fn name(&self) -> &str { "SearchGameDesignLearnings" }

    fn description(&self) -> &str {
        "Search past game design learnings, optionally filtered by branch. Returns the most recent entries."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "default": 10,
                    "description": "Maximum number of learnings to return."
                },
                "branch": {
                    "type": "string",
                    "description": "Optional git branch to filter by."
                }
            },
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let input: SearchGameDesignLearningsInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let provider = Arc::clone(&self.provider);

        Ok(ToolExecution {
            accesses: Default::default(),
    description: "Searching past game-design learnings".into(),
            approval_rule: "SearchGameDesignLearnings".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let input = input.clone();
                Box::pin(async move {
                    let lang = provider.user_language();
                    if !provider.is_session_mode_active() || provider.session_mode_kind() != Some(SessionModeKind::GameDesign) {
                        return ExecutableToolResult::error_text(
                            game_design_t("gameDesign.modeNotActive", lang),
                            "not active".into(),
                        );
                    }

                    let learnings = match provider.game_design_store().search_learnings(input.limit, input.branch.clone()).await {
                        Ok(v) => v,
                        Err(e) => return ExecutableToolResult::error_text(
                            format!("Failed to search learnings: {}", e),
                            "search failed".into(),
                        ),
                    };

                    if learnings.is_empty() {
                        return ExecutableToolResult::ok_text(game_design_t("gameDesign.noLearnings", lang));
                    }

                    let formatted = learnings.iter().enumerate().map(|(i, l)| {
                        let mut lines = vec![
                            format!(
                                "[{}] {}: {}: {}",
                                i + 1,
                                game_design_t("gameDesign.learningTypeLabel", lang),
                                l.type_.to_uppercase(),
                                l.key
                            ),
                            format!("    {}: {}", game_design_t("gameDesign.learningInsightLabel", lang), l.insight),
                            format!(
                                "    {}: {}",
                                game_design_t("gameDesign.learningConfidenceLabel", lang),
                                l.confidence
                            ),
                        ];
                        if let Some(b) = &l.branch {
                            lines.push(format!(
                                "    {}: {}",
                                game_design_t("gameDesign.learningBranchLabel", lang),
                                b
                            ));
                        }
                        lines.join("\n")
                    }).collect::<Vec<_>>().join("\n\n");

                    ExecutableToolResult::ok_text(
                        game_design_t_replace("gameDesign.learningsHeader", lang, &[("count", &learnings.len().to_string())])
                            + "\n\n"
                            + &formatted
                    )
                })
            }),
        })
    }
}
```

- [ ] 在 `tests` 模块追加测试（依赖 `MockGameDesignStateStore` 的 `search_learnings` 过滤逻辑）：

```rust
#[tokio::test]
async fn search_learnings_returns_formatted_results() {
    let store = Arc::new(mock::MockGameDesignStateStore::default());
    store.learnings.lock().unwrap().push(LearningEntry {
        ts: "2026-06-29T00:00:00Z".into(),
        skill: "game-design".into(),
        type_: "eureka".into(),
        key: "flow_state".into(),
        insight: "Core loop drives flow.".into(),
        confidence: 0.95,
        source: "observed".into(),
        branch: Some("feat/game-loop".into()),
    });
    store.learnings.lock().unwrap().push(LearningEntry {
        ts: "2026-06-29T01:00:00Z".into(),
        skill: "game-design".into(),
        type_: "operational".into(),
        key: "ui_guidance".into(),
        insight: "Use affordances.".into(),
        confidence: 0.8,
        source: "observed".into(),
        branch: Some("feat/ui".into()),
    });
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::GameDesign)
            .with_game_design_store(store.clone()),
    );
    let tool = SearchGameDesignLearningsTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({"limit": 10})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("Found 2 learning(s):"));
    assert!(text.contains("[1] Type: OPERATIONAL: ui_guidance"));
    assert!(text.contains("[2] Type: EUREKA: flow_state"));
    assert!(text.contains("Branch: feat/game-loop"));
}

#[tokio::test]
async fn search_learnings_filters_by_branch() {
    let store = Arc::new(mock::MockGameDesignStateStore::default());
    store.learnings.lock().unwrap().push(LearningEntry {
        ts: "2026-06-29T00:00:00Z".into(),
        skill: "game-design".into(),
        type_: "eureka".into(),
        key: "flow_state".into(),
        insight: "Core loop drives flow.".into(),
        confidence: 0.95,
        source: "observed".into(),
        branch: Some("feat/game-loop".into()),
    });
    store.learnings.lock().unwrap().push(LearningEntry {
        ts: "2026-06-29T01:00:00Z".into(),
        skill: "game-design".into(),
        type_: "operational".into(),
        key: "ui_guidance".into(),
        insight: "Use affordances.".into(),
        confidence: 0.8,
        source: "observed".into(),
        branch: Some("feat/ui".into()),
    });
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::GameDesign)
            .with_game_design_store(store.clone()),
    );
    let tool = SearchGameDesignLearningsTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({"branch": "feat/ui"})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    let text = result.to_text();
    assert!(text.contains("Found 1 learning(s):"));
    assert!(text.contains("ui_guidance"));
    assert!(!text.contains("flow_state"));
}

#[tokio::test]
async fn search_learnings_returns_none_when_empty() {
    let store = Arc::new(mock::MockGameDesignStateStore::default());
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::GameDesign)
            .with_game_design_store(store.clone()),
    );
    let tool = SearchGameDesignLearningsTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    assert_eq!(result.to_text(), "No past learnings found.");
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::game_design
```

Expected: 22 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): add SearchGameDesignLearningsTool`。

---

## Task 8: 实现 `EnsureGameDesignRoutingTool`

**Depends on:** Task 2（`MockSessionModeProvider` 与 `MockKaos` 已就绪）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`（追加 `EnsureGameDesignRoutingTool` 与测试）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`

**Why:** 该工具确保项目根目录的 `AGENTS.md` 包含针对 game-design 的 `## Skill routing` 章节。需要覆盖文件不存在、已存在但无 routing、已有 routing 三种状态，且文案与 TS 一致。

**Steps：**

- [ ] 在 `game_design.rs` 的工具实现区追加常量与 `EnsureGameDesignRoutingTool`：

```rust
const GAME_DESIGN_ROUTING_SECTION: &str = r#"## Skill routing

- **game-design**: Game design workflow based on the 100 Principles of Game Design. Activates via --game-design or when the user requests game design help.

To invoke, ask the agent to start game-design mode.
"#;

pub struct EnsureGameDesignRoutingTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl EnsureGameDesignRoutingTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for EnsureGameDesignRoutingTool {
    fn name(&self) -> &str { "EnsureGameDesignRouting" }

    fn description(&self) -> &str {
        "Ensure the project's AGENTS.md contains a ## Skill routing section for game-design mode. Creates or updates AGENTS.md as needed."
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
            description: "Ensuring AGENTS.md has skill routing section for game-design".into(),
            approval_rule: "EnsureGameDesignRouting".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                Box::pin(async move {
                    let lang = provider.user_language();
                    if !provider.is_session_mode_active() || provider.session_mode_kind() != Some(SessionModeKind::GameDesign) {
                        return ExecutableToolResult::error_text(
                            game_design_t("gameDesign.modeNotActive", lang),
                            "not active".into(),
                        );
                    }

                    let cwd = provider.kaos().cwd();
                    let agents_md_path = std::path::PathBuf::from(&cwd).join("AGENTS.md");
                    let agents_md_path_str = agents_md_path.to_string_lossy().to_string();

                    let (content, exists) = match provider.kaos().read_text(&agents_md_path_str).await {
                        Ok(c) => (c, true),
                        Err(_) => (String::new(), false),
                    };

                    if !exists {
                        if let Err(e) = provider.kaos().write_text(&agents_md_path_str, GAME_DESIGN_ROUTING_SECTION).await {
                            return ExecutableToolResult::error_text(
                                game_design_t_replace("gameDesign.failedToEnsureRouting", lang, &[("message", &e.to_string())]),
                                "write failed".into(),
                            );
                        }
                        return ExecutableToolResult::ok_text(
                            game_design_t_replace("gameDesign.agentsMdCreated", lang, &[("path", &agents_md_path_str)])
                        );
                    }

                    if content.contains("## Skill routing") {
                        return ExecutableToolResult::ok_text(
                            game_design_t("gameDesign.agentsMdAlreadyHasRouting", lang)
                        );
                    }

                    let updated = content.trim_end().to_string() + "\n" + GAME_DESIGN_ROUTING_SECTION;
                    if let Err(e) = provider.kaos().write_text(&agents_md_path_str, &updated).await {
                        return ExecutableToolResult::error_text(
                            game_design_t_replace("gameDesign.failedToEnsureRouting", lang, &[("message", &e.to_string())]),
                            "write failed".into(),
                        );
                    }
                    ExecutableToolResult::ok_text(
                        game_design_t_replace("gameDesign.agentsMdUpdated", lang, &[("path", &agents_md_path_str)])
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
async fn ensure_routing_creates_agents_md_when_missing() {
    let kaos = Arc::new(mock::MockKaos::new("/workspace"));
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::GameDesign)
            .with_kaos(kaos.clone()),
    );
    let tool = EnsureGameDesignRoutingTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    assert!(result.to_text().contains("AGENTS.md created at /workspace/AGENTS.md"));
    let content = kaos.read_text("/workspace/AGENTS.md").await.unwrap();
    assert!(content.contains("## Skill routing"));
    assert!(content.contains("game-design"));
}

#[tokio::test]
async fn ensure_routing_appends_when_no_routing_section() {
    let kaos = Arc::new(mock::MockKaos::new("/workspace"));
    kaos.insert("/workspace/AGENTS.md", "# Agent Guide\n\nSome rules.\n");
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::GameDesign)
            .with_kaos(kaos.clone()),
    );
    let tool = EnsureGameDesignRoutingTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    assert!(result.to_text().contains("Appended ## Skill routing section to AGENTS.md"));
    let content = kaos.read_text("/workspace/AGENTS.md").await.unwrap();
    assert!(content.contains("Some rules."));
    assert!(content.contains("## Skill routing"));
}

#[tokio::test]
async fn ensure_routing_noop_when_routing_exists() {
    let kaos = Arc::new(mock::MockKaos::new("/workspace"));
    kaos.insert("/workspace/AGENTS.md", "# Agent Guide\n\n## Skill routing\n\nexisting");
    let provider = Arc::new(
        MockSessionModeProvider::active(SessionModeKind::GameDesign)
            .with_kaos(kaos.clone()),
    );
    let tool = EnsureGameDesignRoutingTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(!result.is_error);
    assert!(result.to_text().contains("AGENTS.md already has a ## Skill routing section"));
}

#[tokio::test]
async fn ensure_routing_fails_when_not_active() {
    let provider = Arc::new(MockSessionModeProvider::inactive());
    let tool = EnsureGameDesignRoutingTool::new(provider.clone());
    let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Game-design mode is not active"));
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::game_design
```

Expected: 26 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): add EnsureGameDesignRoutingTool`。

---

## Task 9: 实现 `SyncGameDesignArtifactTool`

**Depends on:** Task 2（`MockSessionModeProvider`、`MockKaos`、`MockMcpProvider` 已就绪）。

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`（追加本地 `GbrainCli` trait、`RealGbrainCli`、`SyncGameDesignArtifactTool` 与测试）
- Test: `rust-ody/crates/tools-rs/src/builtin/session_mode/game_design.rs`

**Why:** 该工具在 Phase 6 Handoff 将 game-design 设计文档同步到 gbrain。它需要同时支持 MCP 检测路径与 gbrain CLI fallback。为让 CLI fallback 可单元测试，引入本文件局部的 `GbrainCli` 抽象，生产环境使用真实 `std::process::Command`，测试注入 mock runner。

**Steps：**

- [ ] 在 `game_design.rs` 追加 `GbrainCli` trait 与真实实现：

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

- [ ] 追加输入类型与 `SyncGameDesignArtifactTool`：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncGameDesignArtifactInput {
    pub design_file_path: String,
}

pub struct SyncGameDesignArtifactTool {
    provider: Arc<dyn SessionModeProvider>,
    gbrain_cli: Arc<dyn GbrainCli>,
}

impl SyncGameDesignArtifactTool {
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

impl BuiltinTool for SyncGameDesignArtifactTool {
    fn name(&self) -> &str { "SyncGameDesignArtifact" }

    fn description(&self) -> &str {
        "Sync the game design artifact document to persistent storage via gbrain MCP or CLI."
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
        let input: SyncGameDesignArtifactInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let provider = Arc::clone(&self.provider);
        let gbrain_cli = Arc::clone(&self.gbrain_cli);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Syncing game-design artifact".into(),
            approval_rule: "SyncGameDesignArtifact".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let gbrain_cli = Arc::clone(&self.gbrain_cli);
                let input = input.clone();
                Box::pin(async move {
                    let lang = provider.user_language();
                    if !provider.is_session_mode_active() || provider.session_mode_kind() != Some(SessionModeKind::GameDesign) {
                        return ExecutableToolResult::error_text(
                            game_design_t("gameDesign.modeNotActive", lang),
                            "not active".into(),
                        );
                    }

                    let project_root = provider.kaos().project_root().unwrap_or_else(|| provider.kaos().cwd());
                    let gbrain_pin_path = std::path::PathBuf::from(&project_root)
                        .join(".gbrain-source")
                        .to_string_lossy()
                        .to_string();

                    let gbrain_source = match provider.kaos().read_text(&gbrain_pin_path).await {
                        Ok(s) => {
                            let trimmed = s.trim();
                            if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
                        }
                        Err(_) => None,
                    };

                    if let Err(_) = provider.kaos().stat(&input.design_file_path).await {
                        return ExecutableToolResult::error_text(
                            game_design_t_replace("gameDesign.designFileNotFound", lang, &[("path", &input.design_file_path)]),
                            "design file not found".into(),
                        );
                    }

                    if provider.mcp().gbrain_available().await {
                        let mut parts = vec![game_design_t("gameDesign.gbrainConnected", lang)];
                        if let Some(source) = &gbrain_source {
                            parts.push(game_design_t_replace("gameDesign.gbrainTargetSource", lang, &[("source", source)]));
                        } else {
                            parts.push(game_design_t("gameDesign.gbrainNoSourcePin", lang));
                        }
                        parts.push(game_design_t_replace("gameDesign.gbrainReadyForSync", lang, &[("path", &input.design_file_path)]));
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
                            let mut parts = vec![game_design_t("gameDesign.gbrainSynced", lang)];
                            if let Some(source) = &gbrain_source {
                                parts.push(game_design_t_replace("gameDesign.gbrainTargetSource", lang, &[("source", source)]));
                            }
                            parts.push(game_design_t_replace("gameDesign.gbrainFile", lang, &[("path", &input.design_file_path)]));
                            ExecutableToolResult::ok_text(parts.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n"))
                        }
                        Err(e) => ExecutableToolResult::error_text(
                            game_design_t_replace("gameDesign.gbrainCliFailed", lang, &[("message", &e.to_string())]),
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
        MockSessionModeProvider::active(SessionModeKind::GameDesign)
            .with_mcp(mcp)
            .with_kaos(kaos)
            .with_file_path("/workspace/design.md"),
    );
    let tool = SyncGameDesignArtifactTool::new_with_runner(provider, Arc::new(MockGbrainCli::success()));
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
        MockSessionModeProvider::active(SessionModeKind::GameDesign)
            .with_mcp(mcp)
            .with_kaos(kaos)
            .with_file_path("/workspace/design.md"),
    );
    let cli = Arc::new(MockGbrainCli::success());
    let tool = SyncGameDesignArtifactTool::new_with_runner(provider.clone(), cli.clone());
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
        MockSessionModeProvider::active(SessionModeKind::GameDesign)
            .with_mcp(mcp)
            .with_kaos(kaos)
            .with_file_path("/workspace/design.md"),
    );
    let tool = SyncGameDesignArtifactTool::new_with_runner(provider, Arc::new(MockGbrainCli::failure("command not found")));
    let exec = tool.resolve_execution(serde_json::json!({"designFilePath": "/workspace/design.md"})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("gbrain CLI sync failed: command not found"));
}

#[tokio::test]
async fn sync_artifact_design_file_not_found() {
    let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::GameDesign));
    let tool = SyncGameDesignArtifactTool::new(provider);
    let exec = tool.resolve_execution(serde_json::json!({"designFilePath": "/workspace/missing.md"})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Design file not found at /workspace/missing.md"));
}

#[tokio::test]
async fn sync_artifact_fails_when_not_active() {
    let provider = Arc::new(MockSessionModeProvider::inactive());
    let tool = SyncGameDesignArtifactTool::new(provider);
    let exec = tool.resolve_execution(serde_json::json!({"designFilePath": "/workspace/design.md"})).unwrap();
    let result = (exec.execute)(empty_context()).await;
    assert!(result.is_error);
    assert!(result.to_text().contains("Game-design mode is not active"));
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p tools-rs builtin::session_mode::game_design
```

Expected: 31 个测试通过。

- [ ] 运行全工作区类型检查：

```bash
cd rust-ody && cargo check --workspace --all-targets
```

Expected: green。

- [ ] Commit: `feat(tools-rs): add SyncGameDesignArtifactTool`。

---

## Local Self-Review

- [ ] 1. Spec-coverage table：

| Roadmap § / Requirement | Task(s) | Status |
|---|---|---|
| 4.4.5.3 game-design 工具集（8 个工具） | Task 2–9 | covered |
| `EnterGameDesignModeTool` | Task 2 | covered |
| `ExitGameDesignModeTool` | Task 3 | covered |
| `SetGameDesignLanguageTool` | Task 4 | covered |
| `AppendGameDesignLearningTool` | Task 5 | covered |
| `AppendGameDesignProfileTool` | Task 6 | covered |
| `SearchGameDesignLearningsTool` | Task 7 | covered |
| `EnsureGameDesignRoutingTool` | Task 8 | covered |
| `SyncGameDesignArtifactTool`（MCP + CLI fallback） | Task 9 | covered |
| game-design 本地化字符串表 | Task 1 | covered |
| game-design 入口提示语 | Task 1 | covered |

- [ ] 2. Placeholder scan：无 TODO/TBD；本 Part 的 `GbrainCli` trait 与 `RealGbrainCli` 均为完整实现；`MockSessionModeProvider` 的 `Clone`/`with_kaos`/`with_game_design_store` 已在 Task 2 一并补齐。
- [ ] 3. No phantom tasks：每个 Task 都产生真实代码、内联测试与 commit；不存在 `--allow-empty` 或 "already done in Task N"。
- [ ] 4. Dependency soundness：所有 `Depends on:` 均指向 Task 1 或 Task 2（本 Part 内更早的任务）或 `infra.md` Task 3（Part 1 已完成）。
- [ ] 5. Caller & build soundness：本 Part 未修改 Part 1 的共享签名；每个 Task 末尾均运行 `cargo check --workspace --all-targets`。
- [ ] 6. Test-the-risk：
  - `EnterGameDesignModeTool`：断言 `provider.entered` 包含 `GameDesign`。
  - `ExitGameDesignModeTool`：断言 `provider.exited` 为 true，且输出包含保存路径。
  - `SetGameDesignLanguageTool`：断言 `provider.user_language()` 变为 `Zh`。
  - `AppendGameDesignLearningTool` / `AppendGameDesignProfileTool`：断言 mock store 中记录了正确的字段值。
  - `SearchGameDesignLearningsTool`：断言返回格式、按 `branch` 过滤、空结果路径均正确。
  - `EnsureGameDesignRoutingTool`：断言 mock kaos 中的 `AGENTS.md` 内容按状态创建/追加/不变。
  - `SyncGameDesignArtifactTool`：断言 mock CLI runner 捕获到正确的参数与 cwd，且 MCP 可用路径跳过 CLI。
- [ ] 7. Type consistency：`Language`、`SessionModeKind`、`SessionModeProvider`、`GameDesignStateStore`、`McpProvider`、`SessionModeContext` 均使用 Part 1 定义的签名；`LearningEntry`/`GameDesignProfileEntry` 的字段名与 `#[serde(rename_all = "camelCase")]` 与 TS 一致。
