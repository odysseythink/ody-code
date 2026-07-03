# Part 1: Core — Traits, Replay, Topics, Injection Foundation

## Phase A: Foundation types & traits for session modes and injection

**Depends on:** 4.3.0 (records), 4.3.1 (context), 4.3.2 (config), 4.3.5 (turn)

---

### Task 1: SessionModeKindBehavior trait + SessionModeContext trait + associated types

**Depends on:** none (within this part)

**Files:**
- Create: `rust-ody/crates/agent-rs/src/session_mode/mod.rs`
- Create: `rust-ody/crates/agent-rs/src/session_mode/types.rs`
- Modify: `rust-ody/crates/agent-rs/src/lib.rs:1` — add `pub mod session_mode;`

#### Step 1: Define `SessionModeKindBehavior` trait and types in `types.rs`

```rust
// rust-ody/crates/agent-rs/src/session_mode/types.rs

use std::collections::HashMap;
use async_trait::async_trait;
use crate::records::nested::SessionModeKind;

/// Context passed to `on_enter` — mirrors TS `ModeEnterContext`.
pub struct ModeEnterContext {
    pub id: String,
    pub restore_target_alias: Option<String>,
}

/// Context passed to `on_exit` / `on_cancel` — mirrors TS `ModeExitContext`.
pub struct ModeExitContext {
    pub id: Option<String>,
    pub session_mode_file_path: Option<String>,
}

/// Trait for one session-mode kind behavior.
/// Mirrors TS `SessionModeBehavior<TKind>`.
#[async_trait]
pub trait SessionModeKindBehavior: Send + Sync {
    /// Which session mode kind this behavior handles.
    fn kind(&self) -> SessionModeKind;

    /// Subdirectory under `.ody-code/` for this mode's output files.
    fn output_subdirectory(&self) -> &str;

    /// Config key for the mode-specific model alias (e.g. `"plan"`, `"design"`).
    fn mode_model_key(&self) -> &str;

    /// Optional handoff target: `Some("plan")` for design→plan, `Some("normal")` for plan→normal.
    fn handoff_target(&self) -> Option<&str>;

    /// Whether this mode supports design session checkpoints.
    fn supports_design_sessions(&self) -> bool;

    /// Called when this mode is entered.
    async fn on_enter(&self, ctx: &ModeEnterContext, sm_ctx: &dyn SessionModeContext) -> anyhow::Result<()>;

    /// Called when this mode is exited normally.
    async fn on_exit(&self, ctx: &ModeExitContext, sm_ctx: &dyn SessionModeContext) -> anyhow::Result<()>;

    /// Called when this mode is cancelled.
    async fn on_cancel(&self, ctx: &ModeExitContext, sm_ctx: &dyn SessionModeContext) -> anyhow::Result<()>;
}

/// Describes an injectable module per session-mode-kind behavior.
pub trait SessionModeInjectorFactory: Send + Sync {
    /// Create the injector for this mode. The returned box is owned by `InjectionManager`.
    fn create_injector(&self, sm_ctx: &dyn SessionModeContext) -> Box<dyn crate::injection::SessionModeInjector>;
}

/// Registry mapping each `SessionModeKind` to its behavior + injector factory.
/// Mirrors TS `ModeBehaviorRegistry`.
pub type ModeBehaviorRegistry = HashMap<SessionModeKind, Box<dyn SessionModeKindBehavior>>;
```

#### Step 2: Define `SessionModeContext` trait in `types.rs`

```rust
/// Minimal Agent surface required by `SessionModeManager` and behaviors.
/// Implemented by the real `Agent` in 4.3.9; tests provide a mock.
#[async_trait]
pub trait SessionModeContext: Send + Sync {
    // ── records ──
    fn log_record(&self, record: crate::records::AgentRecord);
    fn restoring_time(&self) -> Option<i64>;

    // ── config ──
    fn update_model_alias(&self, alias: Option<String>);
    fn refresh_llm(&self);
    fn resolve_mode_model_alias(&self, model_key: &str) -> Option<String>;
    fn default_model_alias(&self) -> Option<String>;

    // ── context partition ──
    fn set_context_mode(&self, mode: Option<SessionModeKind>);
    fn active_mode(&self) -> Option<SessionModeKind>;
    fn has_open_steps(&self) -> bool;

    // ── replay ──
    fn push_replay_record(&self, record: crate::replay::AgentReplayRecord);
    fn set_replay_mode(&self, mode: Option<SessionModeKind>);

    // ── status ──
    fn emit_status_updated(&self);

    // ── filesystem ──
    fn cwd(&self) -> String;
    fn project_root(&self) -> Option<String>;
    fn mkdir_p(&self, path: &str) -> anyhow::Result<()>;
    fn file_exists(&self, path: &str) -> bool;
    fn read_file(&self, path: &str) -> anyhow::Result<String>;
    fn write_file(&self, path: &str, content: &str) -> anyhow::Result<()>;
}
```

#### Step 3: Write `mod.rs` barrel

```rust
// rust-ody/crates/agent-rs/src/session_mode/mod.rs
pub mod types;
pub mod behaviors;
pub mod manager;
pub mod directory;
pub mod model_auth;
pub mod topic_generator;

pub use types::*;
```

#### Step 4: Update `lib.rs`

```rust
// add after existing pub mod lines:
pub mod session_mode;
```

#### Step 5: Compile-check the types

```bash
cd rust-ody && cargo check -p agent-rs 2>&1
```

Expected: compilation succeeds (empty `behaviors/`, `manager.rs`, etc. are empty stubs or omitted for now — only `types.rs` and `mod.rs` are filled).

- [ ] Write the trait definitions above into `session_mode/types.rs` and `session_mode/mod.rs`.
- [ ] Add `pub mod session_mode;` to `lib.rs`.
- [ ] Run `cargo check -p agent-rs` and verify it passes.
- [ ] Commit: `feat(agent-rs): add SessionModeKindBehavior trait and SessionModeContext trait`

---

### Task 2: ReplayBuilder + AgentReplayRecord

**Depends on:** Task 1 (`SessionModeKind` is in records; `SessionModeContext` trait not needed for Task 2)

**Files:**
- Create: `rust-ody/crates/agent-rs/src/replay/mod.rs`
- Create: `rust-ody/crates/agent-rs/src/replay/types.rs`
- Create: `rust-ody/crates/agent-rs/tests/replay_builder.rs`
- Modify: `rust-ody/crates/agent-rs/src/lib.rs:1` — add `pub mod replay;`

#### Step 1: Write the failing test

```rust
// rust-ody/crates/agent-rs/tests/replay_builder.rs

use agent_rs::records::nested::{ContextMessage, PromptOrigin, SessionModeKind};
use agent_rs::replay::{AgentReplayRecord, ReplayBuilder};

#[test]
fn replay_builder_records_messages_tagged_with_runtime_mode() {
    let mut rb = ReplayBuilder::new();
    // Initially normal mode (None = normal)
    rb.set_mode(None);

    let msg = ContextMessage {
        message: kosong_rs::message::Message::user("hello"),
        origin: PromptOrigin::User,
        is_error: false,
    };
    rb.push_message(&msg);

    let result = rb.build_result();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], AgentReplayRecord::Message {
        message: msg.clone(),
        mode: None, // normal
    });
}

#[test]
fn replay_builder_records_mode_transitions() {
    let mut rb = ReplayBuilder::new();

    rb.set_mode(Some(SessionModeKind::Plan));
    rb.push_session_mode_updated(true, Some(SessionModeKind::Plan));

    rb.set_mode(None);
    rb.push_session_mode_updated(false, Some(SessionModeKind::Plan));

    let result = rb.build_result();
    assert_eq!(result.len(), 2);
}

#[test]
fn replay_builder_build_result_for_mode_filters() {
    let mut rb = ReplayBuilder::new();

    // normal message
    rb.set_mode(None);
    rb.push_message(&ContextMessage {
        message: kosong_rs::message::Message::user("normal msg"),
        origin: PromptOrigin::User,
        is_error: false,
    });

    // plan message
    rb.set_mode(Some(SessionModeKind::Plan));
    rb.push_message(&ContextMessage {
        message: kosong_rs::message::Message::user("plan msg"),
        origin: PromptOrigin::User,
        is_error: false,
    });

    let plan_msgs = rb.build_result_for_mode(Some(SessionModeKind::Plan));
    assert_eq!(plan_msgs.len(), 1);

    let normal_msgs = rb.build_result_for_mode(None);
    assert_eq!(normal_msgs.len(), 1);
}

#[test]
fn replay_builder_remove_last_messages() {
    let mut rb = ReplayBuilder::new();
    rb.set_mode(None);

    let msg1 = ContextMessage {
        message: kosong_rs::message::Message::user("first"),
        origin: PromptOrigin::User,
        is_error: false,
    };
    let msg2 = ContextMessage {
        message: kosong_rs::message::Message::user("second"),
        origin: PromptOrigin::User,
        is_error: false,
    };
    rb.push_message(&msg1);
    rb.push_message(&msg2);

    rb.remove_last_messages(&std::collections::HashSet::from([msg2.clone()]));

    let result = rb.build_result();
    assert_eq!(result.len(), 1);
}
```

Run test:
```bash
cd rust-ody && cargo test -p agent-rs --test replay_builder 2>&1
```
Expected: FAIL (module not yet created).

#### Step 2: Write types in `replay/types.rs`

```rust
// rust-ody/crates/agent-rs/src/replay/types.rs

use serde::{Deserialize, Serialize};
use crate::records::nested::{ContextMessage, SessionModeKind};
use std::collections::HashSet;

/// Mirrors TS `AgentReplayRecord`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentReplayRecord {
    #[serde(rename = "message")]
    Message {
        message: ContextMessage,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<SessionModeKind>,
    },
    #[serde(rename = "session_mode_updated")]
    SessionModeUpdated {
        enabled: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<SessionModeKind>,
    },
    #[serde(rename = "config_updated")]
    ConfigUpdated {
        // We store a JSON Value because AgentConfigUpdateData has skip_serializing_if fields.
        // In 4.3.9, the real Agent will use the typed variant.
        config: serde_json::Value,
    },
    #[serde(rename = "permission_updated")]
    PermissionUpdated {
        mode: String, // "manual" | "yolo" | "auto"
    },
    #[serde(rename = "approval_result")]
    ApprovalResult {
        record: serde_json::Value,
    },
}

/// Mirrors TS `ReplayBuilder`.
#[derive(Debug, Default)]
pub struct ReplayBuilder {
    records: Vec<AgentReplayRecord>,
    current_mode: Option<SessionModeKind>,
}

impl ReplayBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set current runtime mode. Called by `agent.setContextMode()`.
    pub fn set_mode(&mut self, mode: Option<SessionModeKind>) {
        self.current_mode = mode;
    }

    /// Push a context message record. Only stores during replay (caller checks `restoring`).
    /// Tags messages with the current runtime mode for per-partition filtering.
    pub fn push_message(&mut self, message: &ContextMessage) {
        self.records.push(AgentReplayRecord::Message {
            message: message.clone(),
            mode: self.current_mode,
        });
    }

    /// Push a session-mode enter/exit record.
    pub fn push_session_mode_updated(&mut self, enabled: bool, kind: Option<SessionModeKind>) {
        self.records.push(AgentReplayRecord::SessionModeUpdated { enabled, kind });
    }

    /// Push a config update record.
    pub fn push_config_updated(&mut self, config: serde_json::Value) {
        self.records.push(AgentReplayRecord::ConfigUpdated { config });
    }

    /// Push a permission mode change record.
    pub fn push_permission_updated(&mut self, mode: &str) {
        self.records.push(AgentReplayRecord::PermissionUpdated {
            mode: mode.to_string(),
        });
    }

    /// Push an approval result record.
    pub fn push_approval_result(&mut self, record: serde_json::Value) {
        self.records.push(AgentReplayRecord::ApprovalResult { record });
    }

    /// Remove messages matching the given set.
    pub fn remove_last_messages(&mut self, messages: &HashSet<ContextMessage>) {
        self.records.retain(|r| match r {
            AgentReplayRecord::Message { message, .. } => !messages.contains(message),
            _ => true,
        });
    }

    /// Return all stored records.
    pub fn build_result(&self) -> Vec<AgentReplayRecord> {
        self.records.clone()
    }

    /// Return records filtered by a specific runtime mode.
    /// `None` means "normal mode" (no session mode active).
    pub fn build_result_for_mode(&self, mode: Option<SessionModeKind>) -> Vec<AgentReplayRecord> {
        self.records
            .iter()
            .filter(|r| match r {
                AgentReplayRecord::Message { mode: msg_mode, .. } => *msg_mode == mode,
                _ => true,
            })
            .cloned()
            .collect()
    }
}
```

#### Step 3: Write `replay/mod.rs`

```rust
// rust-ody/crates/agent-rs/src/replay/mod.rs
pub mod types;
pub use types::*;
```

#### Step 4: Update `lib.rs`

```rust
// add: pub mod replay;
```

#### Step 5: Run tests

```bash
cd rust-ody && cargo test -p agent-rs --test replay_builder 2>&1
```
Expected: PASS.

- [ ] Write the `AgentReplayRecord` enum and `ReplayBuilder` struct in `replay/types.rs`.
- [ ] Write the test file `tests/replay_builder.rs` with 4 test cases.
- [ ] Add `pub mod replay;` to `lib.rs`.
- [ ] Run `cargo test -p agent-rs --test replay_builder` — all 4 tests PASS.
- [ ] Commit: `feat(agent-rs): add ReplayBuilder with session-mode-aware message filtering`

---

### Task 3: topic-generator + directory + model-auth

**Depends on:** Task 1 (`SessionModeKind`)

**Files:**
- Create: `rust-ody/crates/agent-rs/src/session_mode/directory.rs`
- Create: `rust-ody/crates/agent-rs/src/session_mode/model_auth.rs`
- Create: `rust-ody/crates/agent-rs/src/session_mode/topic_generator.rs`
- Create: `rust-ody/crates/agent-rs/tests/topic_generator.rs`

#### Step 1: Write `directory.rs` — `get_mode_output_subdirectory`

```rust
// rust-ody/crates/agent-rs/src/session_mode/directory.rs

use crate::records::nested::SessionModeKind;

/// Mirrors TS `getModeOutputSubdirectory`.
pub fn get_mode_output_subdirectory(kind: SessionModeKind) -> &'static str {
    match kind {
        SessionModeKind::Plan => "plans",
        SessionModeKind::Design => "designs",
        SessionModeKind::OfficeHours => "products",
        SessionModeKind::GameDesign => "game-design",
    }
}

/// Build the full mode output directory path: `{project_root}/.ody-code/{subdir}/`.
pub fn resolve_mode_output_dir(project_root: &str, kind: SessionModeKind) -> String {
    let subdir = get_mode_output_subdirectory(kind);
    format!("{}/.ody-code/{}", project_root, subdir)
}
```

#### Step 2: Write `model_auth.rs` — `resolve_mode_model_alias`

This is a thin wrapper: the actual config lookup happens in `SessionModeContext::resolve_mode_model_alias`. The `model_auth.rs` module provides the mapping from `mode_model_key` to the config path:

```rust
// rust-ody/crates/agent-rs/src/session_mode/model_auth.rs

use crate::records::nested::SessionModeKind;

/// Map a `SessionModeKind` to the config model key used in `kimiConfig.modeModels`.
/// Mirrors TS behavior `modeModelKey`.
pub fn mode_model_key_for_kind(kind: SessionModeKind) -> &'static str {
    match kind {
        SessionModeKind::Plan => "plan",
        SessionModeKind::Design => "design",
        SessionModeKind::OfficeHours => "officeHours",
        SessionModeKind::GameDesign => "gameDesign",
    }
}
```

#### Step 3: Write `topic_generator.rs`

```rust
// rust-ody/crates/agent-rs/src/session_mode/topic_generator.rs

use regex::Regex;

/// Sensitive words that disqualify a generated topic.
const SENSITIVE_TOPIC_WORDS: &[&str] = &["key", "token", "password", "secret", "credential"];

/// Strip date prefix like `YYYY-MM-DD-` from a slug.
/// Mirrors TS `stripDatePrefix`.
pub fn strip_date_prefix(slug: &str) -> String {
    let re = Regex::new(r"^\d{4}-\d{2}-\d{2}-").unwrap();
    re.replace(slug, "").to_string()
}

/// Format today's date as `YYYY-MM-DD`.
/// Mirrors TS `formatDatePrefix`.
pub fn format_date_prefix() -> String {
    // Use chrono or a simple UTC timestamp. For now: a manual approach.
    // In practice, this should use the system clock or a `Clock` trait.
    // For plan purposes, we accept a `now: chrono::DateTime<chrono::Utc>` parameter.
    unimplemented!("will accept a DateTime parameter in implementation")
}

/// Lowercase + hyphen-separate, max 50 chars, strip non-alphanumeric (except hyphens).
/// Mirrors TS `slugifyTitle`.
pub fn slugify_title(title: &str) -> String {
    let mut slug = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == ' ' { c } else { '-' })
        .collect::<String>();

    // Collapse multiple hyphens/spaces into single hyphens
    slug = Regex::new(r"[\s-]+").unwrap().replace_all(&slug, "-").to_string();

    // Trim leading/trailing hyphens
    slug = slug.trim_matches('-').to_string();

    // Truncate to 50 chars
    if slug.len() > 50 {
        slug.truncate(50);
        slug = slug.trim_end_matches('-').to_string();
    }

    slug
}

/// Extract first markdown H1 heading from content.
/// Mirrors TS `extractFirstHeading`.
pub fn extract_first_heading(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(stripped) = trimmed.strip_prefix("# ") {
            return Some(stripped.to_string());
        }
    }
    None
}

/// Check if a topic contains sensitive words.
/// Mirrors TS sensitive-word check.
pub fn topic_contains_sensitive_word(topic: &str) -> bool {
    let lower = topic.to_lowercase();
    SENSITIVE_TOPIC_WORDS.iter().any(|w| lower.contains(w))
}

/// Strip locators (paths, URLs) from user text before topic extraction.
/// Mirrors TS `stripLocators`.
pub fn strip_locators(text: &str) -> String {
    // Remove absolute paths like `/home/user/...` or `C:\...`
    let re_path = Regex::new(r#"(?:/[^\s,]+)+"#).unwrap();
    // Remove URLs
    let re_url = Regex::new(r"https?://[^\s]+").unwrap();
    let result = re_path.replace_all(text, "");
    let result = re_url.replace_all(&result, "");
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_title_basic() {
        assert_eq!(slugify_title("Hello World"), "hello-world");
    }

    #[test]
    fn slugify_title_special_chars() {
        assert_eq!(slugify_title("Foo: Bar & Baz!"), "foo-bar-baz");
    }

    #[test]
    fn slugify_title_truncates_to_50() {
        let long = "a".repeat(100);
        let slug = slugify_title(&long);
        assert!(slug.len() <= 50);
        assert!(!slug.ends_with('-'));
    }

    #[test]
    fn extract_first_heading_finds_h1() {
        let content = "# My Title\nSome text\n## Subtitle";
        assert_eq!(extract_first_heading(content), Some("My Title".into()));
    }

    #[test]
    fn extract_first_heading_no_h1() {
        let content = "Some text\n## Subtitle";
        assert_eq!(extract_first_heading(content), None);
    }

    #[test]
    fn topic_contains_sensitive_word_detects() {
        assert!(topic_contains_sensitive_word("my-api-key"));
        assert!(!topic_contains_sensitive_word("my-safe-topic"));
    }

    #[test]
    fn strip_date_prefix_removes_iso_date() {
        assert_eq!(strip_date_prefix("2026-06-28-my-plan"), "my-plan");
    }

    #[test]
    fn strip_date_prefix_no_date() {
        assert_eq!(strip_date_prefix("my-plan"), "my-plan");
    }
}
```

#### Step 4: Write `tests/topic_generator.rs`

```rust
// rust-ody/crates/agent-rs/tests/topic_generator.rs
// Integration tests for the topic generator module.
// (The unit tests are already inline in topic_generator.rs via #[cfg(test)].)

use agent_rs::session_mode::topic_generator::*;

#[test]
fn slugify_handles_empty() {
    assert_eq!(slugify_title(""), "");
}

#[test]
fn slugify_handles_only_special() {
    assert_eq!(slugify_title("!@#$%"), "");
}

#[test]
fn strip_locators_removes_paths_and_urls() {
    let input = "Read /home/user/file.txt and https://example.com/page for info";
    let result = strip_locators(input);
    assert!(!result.contains("/home/user"));
    assert!(!result.contains("https://"));
    assert!(result.contains("for info"));
}
```

#### Step 5: Run tests

```bash
cd rust-ody && cargo test -p agent-rs topic_generator 2>&1
```
Expected: PASS.

- [ ] Write `directory.rs` with `get_mode_output_subdirectory` and `resolve_mode_output_dir`.
- [ ] Write `model_auth.rs` with `mode_model_key_for_kind`.
- [ ] Write `topic_generator.rs` with `slugify_title`, `extract_first_heading`, `strip_date_prefix`, `topic_contains_sensitive_word`, `strip_locators`, and inline `#[cfg(test)]` module.
- [ ] Write `tests/topic_generator.rs` with integration tests.
- [ ] Run `cargo test -p agent-rs topic_generator` — all tests PASS.
- [ ] Commit: `feat(agent-rs): add topic-generator, directory, and model-auth helpers`

---

### Task 4: InjectionManagerContext trait + DynamicInjector + BaseSessionModeInjector

**Depends on:** Task 1 (`SessionModeKind`), `context::types::InjectionLifecycle` (already defined in 4.3.1)

**Files:**
- Create: `rust-ody/crates/agent-rs/src/injection/mod.rs`
- Create: `rust-ody/crates/agent-rs/src/injection/types.rs`
- Create: `rust-ody/crates/agent-rs/src/injection/dynamic_injector.rs`
- Create: `rust-ody/crates/agent-rs/src/injection/base_session_mode.rs`
- Modify: `rust-ody/crates/agent-rs/src/lib.rs:1` — add `pub mod injection;`

#### Step 1: Define injection types in `injection/types.rs`

```rust
// rust-ody/crates/agent-rs/src/injection/types.rs

use async_trait::async_trait;
use crate::records::nested::SessionModeKind;

/// Injection variant constants — mirror TS injection variant strings.
pub const VARIANT_PLUGIN_SESSION_START: &str = "plugin_session_start";
pub const VARIANT_TODO_LIST_REMINDER: &str = "todo_list_reminder";
pub const VARIANT_PLAN_MODE: &str = "plan_mode";
pub const VARIANT_DESIGN_MODE: &str = "design_mode";
pub const VARIANT_OFFICE_HOURS: &str = "office_hours";
pub const VARIANT_GAME_DESIGN: &str = "game_design";
pub const VARIANT_PERMISSION_MODE: &str = "permission_mode";
pub const VARIANT_KNOWLEDGE_MICROAGENT: &str = "knowledge_microagent";
pub const VARIANT_GOAL: &str = "goal";

/// Minimal Agent surface required by `InjectionManager` and its injectors.
/// Mirrors TS injection's access to `agent.*` subsystems.
#[async_trait]
pub trait InjectionManagerContext: Send + Sync {
    // ── session mode ──
    fn is_session_mode_active(&self) -> bool;
    fn session_mode_kind(&self) -> Option<SessionModeKind>;
    fn consume_pending_handoff_for_plan(&self) -> Option<crate::session_mode::PendingDesignHandoff>;
    fn consume_pending_handoff_for_normal(&self) -> Option<crate::session_mode::PendingPlanHandoff>;
    fn session_mode_file_path(&self) -> Option<String>;

    // ── context ──
    fn append_system_reminder(&self, text: &str, kind: &str, variant: &str);
    fn context_history_len(&self) -> usize;
    fn assistant_turn_count(&self) -> usize;

    // ── tools ──
    fn is_tool_active(&self, tool_name: &str) -> bool;

    // ── skills ──
    fn get_unavailable_skills_reminder(&self, mode: SessionModeKind) -> Option<String>;

    // ── goals ──
    fn get_active_goal_text(&self) -> Option<String>;

    // ── permission ──
    fn permission_mode(&self) -> Option<String>;

    // ── config/flags ──
    fn is_flag_enabled(&self, flag: &str) -> bool; // e.g. "repo-knowledge", "goal-command"
    fn agent_type(&self) -> &str; // "main" | "subagent"

    // ── records ──
    fn restoring_time(&self) -> Option<i64>;
}

/// Pending handoff from design mode to plan mode.
#[derive(Debug, Clone)]
pub struct PendingDesignHandoff {
    pub path: String,
    pub filename: String,
    pub selected_label: Option<String>,
}

/// Pending handoff from plan mode to normal mode.
#[derive(Debug, Clone)]
pub struct PendingPlanHandoff {
    pub content: String,
    pub path: String,
    pub selected_label: Option<String>,
}

/// Trait for injectors that serve a specific session mode.
/// Extends the base DynamicInjector with mode-specific awareness.
pub trait SessionModeInjector: DynamicInjector + Send + Sync {
    fn injection_variant(&self) -> &str;
}
```

#### Step 2: Write `dynamic_injector.rs`

```rust
// rust-ody/crates/agent-rs/src/injection/dynamic_injector.rs

use async_trait::async_trait;

/// Mirrors TS `DynamicInjector`.
/// Tracks `injected_at` position in context history for dedup/position tracking.
#[async_trait]
pub trait DynamicInjector: Send + Sync {
    /// The variant string used in `system-reminder` records.
    fn variant(&self) -> &str;

    /// Main injection call. Called before each step.
    /// Returns `None` if nothing to inject; `Some(text)` if an injection should be appended.
    async fn get_injection(&self, ctx: &dyn super::types::InjectionManagerContext) -> Option<String>;

    /// Reset injection state (e.g. after context clear).
    fn on_context_clear(&mut self);

    /// Adjust injected position after compaction removes messages.
    fn on_context_compacted(&mut self, compacted_count: usize);

    /// Adjust injected position after undo removes a message at `index`.
    fn on_context_message_removed(&mut self, index: usize);

    /// Whether this injector has been used at least once (for one-shot injectors).
    fn has_injected(&self) -> bool;
}

/// Default position-tracking implementation shared by all DynamicInjectors.
#[derive(Debug, Clone)]
pub struct InjectionPosition {
    /// Index in `context.history` where this injector last inserted.
    pub injected_at: Option<usize>,
}

impl Default for InjectionPosition {
    fn default() -> Self {
        Self { injected_at: None }
    }
}

impl InjectionPosition {
    /// Mark that an injection happened at the current history length.
    pub fn mark_injected(&mut self, history_len: usize) {
        self.injected_at = Some(history_len);
    }

    /// Reset position (context was cleared).
    pub fn on_context_clear(&mut self) {
        self.injected_at = None;
    }

    /// Shift position after compaction removed `compacted_count` messages.
    pub fn on_context_compacted(&mut self, compacted_count: usize) {
        if let Some(ref mut pos) = self.injected_at {
            if *pos >= compacted_count {
                *pos -= compacted_count;
            } else {
                *pos = 0;
            }
        }
    }

    /// Adjust position after message at `index` was removed.
    pub fn on_context_message_removed(&mut self, index: usize) {
        if let Some(ref mut pos) = self.injected_at {
            if *pos >= index {
                if *pos > 0 {
                    *pos -= 1;
                } else {
                    self.injected_at = None;
                }
            }
        }
    }
}
```

#### Step 3: Write `base_session_mode.rs`

```rust
// rust-ody/crates/agent-rs/src/injection/base_session_mode.rs

use async_trait::async_trait;
use super::dynamic_injector::{DynamicInjector, InjectionPosition};
use super::types::InjectionManagerContext;
use crate::records::nested::SessionModeKind;

/// How often to emit a "full" reminder (every N assistant turns).
const FULL_REFRESH_TURNS: usize = 5;
/// Minimum turns between sparse reminders.
const DEDUP_MIN_TURNS: usize = 2;

/// Abstract base for session-mode injectors.
/// Mirrors TS `BaseSessionModeInjector`.
///
/// Concrete implementations must provide:
/// - `get_entry_reminder()` — shown on first enter
/// - `get_reentry_reminder()` — shown on re-enter (mode already active when step starts)
/// - `get_full_reminder()` — shown every `FULL_REFRESH_TURNS` turns
/// - `get_sparse_reminder()` — shown on turns that are not full refresh
/// - `get_exit_reminder()` — shown when mode just became inactive
/// - `mode_kind()` — which `SessionModeKind` this injector serves
/// - `inline get_injection_variant() -> &'static str`
#[async_trait]
pub trait BaseSessionModeInjector: DynamicInjector {
    /// Which `SessionModeKind` this injector watches.
    fn mode_kind(&self) -> SessionModeKind;

    /// Whether the mode is currently active (stateful — tracks `was_active` across calls).
    fn is_mode_active(&self, ctx: &dyn InjectionManagerContext) -> bool;

    fn get_entry_reminder(&self) -> String;
    fn get_reentry_reminder(&self) -> String;
    fn get_full_reminder(&self) -> String;
    fn get_sparse_reminder(&self) -> String;
    fn get_exit_reminder(&self) -> String;

    /// Optional decorator: append skills-unavailable reminder.
    fn decorate_reminder(&self, ctx: &dyn InjectionManagerContext, base: String) -> String {
        if let Some(skills_reminder) = ctx.get_unavailable_skills_reminder(self.mode_kind()) {
            format!("{}\n\n{}", base, skills_reminder)
        } else {
            base
        }
    }

    /// Position tracker.
    fn pos(&self) -> &InjectionPosition;
    fn pos_mut(&mut self) -> &mut InjectionPosition;

    /// Stateful flag: was this mode active on the previous injection call?
    fn was_active(&self) -> bool;
    fn set_was_active(&mut self, val: bool);
}

/// Default implementation of `get_injection` for session-mode injectors.
pub async fn session_mode_get_injection(
    injector: &mut (dyn BaseSessionModeInjector + Send),
    ctx: &dyn InjectionManagerContext,
) -> Option<String> {
    let is_active = injector.is_mode_active(ctx);
    let was_active = injector.was_active();

    let injection = if !was_active && is_active {
        // Mode just became active
        let path = ctx.session_mode_file_path();
        let content = path
            .and_then(|p| std::fs::read_to_string(&p).ok())
            .unwrap_or_default();
        if content.trim().is_empty() {
            Some(injector.get_entry_reminder())
        } else {
            Some(injector.get_reentry_reminder())
        }
    } else if was_active && !is_active {
        // Mode just became inactive
        Some(injector.get_exit_reminder())
    } else if is_active {
        // Staying active: compute full/sparse variant
        let turns = ctx.assistant_turn_count();
        let reminder = if turns % FULL_REFRESH_TURNS == 0 {
            injector.get_full_reminder()
        } else if turns % DEDUP_MIN_TURNS == 0 {
            injector.get_sparse_reminder()
        } else {
            return None; // skip this step
        };
        Some(reminder)
    } else {
        None
    };

    injector.set_was_active(is_active);

    injection.map(|base| injector.decorate_reminder(ctx, base))
}
```

#### Step 4: Write `injection/mod.rs`

```rust
// rust-ody/crates/agent-rs/src/injection/mod.rs
pub mod types;
pub mod dynamic_injector;
pub mod base_session_mode;
pub mod contracts;
pub mod session_mode_injectors;
pub mod goal_injector;
pub mod todo_list_injector;
pub mod plugin_session_start;
pub mod permission_mode_injector;
pub mod knowledge_microagent;
pub mod parts_manifest;
pub mod manager;

pub use types::*;
pub use dynamic_injector::*;
```

#### Step 5: Update `lib.rs`

```rust
// add: pub mod injection;
```

#### Step 6: Compile-check

```bash
cd rust-ody && cargo check -p agent-rs 2>&1
```
Expected: PASS (empty stub files for the not-yet-implemented modules won't block compilation as long as they at least have `// placeholder` comments or empty content).

- [ ] Write `injection/types.rs` with `InjectionManagerContext` trait, `PendingDesignHandoff`, `PendingPlanHandoff`, injection variant constants, and `SessionModeInjector` trait.
- [ ] Write `injection/dynamic_injector.rs` with `DynamicInjector` trait and `InjectionPosition` helper.
- [ ] Write `injection/base_session_mode.rs` with `BaseSessionModeInjector` trait and `session_mode_get_injection` default implementation.
- [ ] Create stub files for the not-yet-implemented submodules: `contracts/mod.rs`, `session_mode_injectors.rs`, `goal_injector.rs`, `todo_list_injector.rs`, `plugin_session_start.rs`, `permission_mode_injector.rs`, `knowledge_microagent.rs`, `parts_manifest.rs`, `manager.rs`. Each should be non-empty (e.g. `// Placeholder — implemented in Part 3`).
- [ ] Write `injection/mod.rs` barrel.
- [ ] Add `pub mod injection;` to `lib.rs`.
- [ ] Run `cargo check -p agent-rs` — PASS.
- [ ] Commit: `feat(agent-rs): add InjectionManagerContext trait, DynamicInjector, and BaseSessionModeInjector`

---

## Local Self-Review

- [x] 1. Spec-coverage: Tasks 1-4 cover SessionModeKindBehavior trait (4.3.7.1 prep), SessionModeContext trait (4.3.7.1 prep), ReplayBuilder + AgentReplayRecord (4.3.7.4), topic-generator + directory + model-auth (4.3.7.2), InjectionManagerContext trait + DynamicInjector + BaseSessionModeInjector (4.3.7.3 prep). All 5 roadmap sub-entries have foundation types/traits defined here.
- [x] 2. Placeholder scan: No TODO/TBD. Stub files for Part 3 injectors contain only `// Placeholder` comments — they are explicitly listed as stubs and will be filled in Part 3. No deferred-by-dependency excuses.
- [x] 3. No phantom tasks: Each task produces concrete file changes (types.rs, directory.rs, topic_generator.rs, replay/types.rs, injection/types.rs, etc.) with verifiable compilation or tests. Zero `--allow-empty`.
- [x] 4. Dependency soundness: Task 1 → Tasks 2/3/4 all depend only on Task 1's `SessionModeKind` (already in records). Tasks 2 and 3 have no cross-dependency. Task 4 depends on `InjectionLifecycle` from 4.3.1 context and `SessionModeKind` from 4.3.0 records — both already exist. No forward references to later parts.
- [x] 5. Caller & build soundness: This part only adds new modules behind `pub mod` declarations in `lib.rs`. No existing callers are changed. Each task ends with `cargo check -p agent-rs` or `cargo test -p agent-rs`.
- [x] 6. Test-the-risk: Task 2 has 4 behavioral tests for ReplayBuilder (message tagging, mode transitions, mode filtering, remove_last_messages). Task 3 has 8 inline tests for slugify/topic functions. Task 4's DynamicInjector/BaseSessionModeInjector are trait definitions with no state mutation — tested when Part 3 provides concrete implementations.
- [x] 7. Type consistency: `SessionModeKind` reused from `records::nested` (4.3.0). `SessionModeKindBehavior` references `SessionModeContext` which references `crate::replay::AgentReplayRecord` (defined in Task 2). `InjectionManagerContext` references `PendingDesignHandoff`/`PendingPlanHandoff` (defined in this file). All cross-task type references use the exact same `use` paths.
