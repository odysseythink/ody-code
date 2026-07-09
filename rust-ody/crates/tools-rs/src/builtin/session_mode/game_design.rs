//! Game-design session-mode tools.
//!
//! Mirrors the upstream TypeScript game-design builtins:
//! - Enter/Exit game-design mode
//! - Append game-design profile and learning entries
//! - Search past game-design learnings
//! - Set the user language for the session
//! - Ensure `AGENTS.md` has a game-design skill routing section
//! - Sync the design artifact to gbrain

use std::path::Path;
use std::sync::Arc;

use serde_json::Value;

use crate::builtin::session_mode::i18n::{subst, t, Msg};
use crate::builtin::session_mode::{
    GameDesignProfileEntry, Language, LearningEntry, SessionModeKind, SessionModeProvider,
};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};

// ---------------------------------------------------------------------------
// Entry / exit messages
// ---------------------------------------------------------------------------

pub fn game_design_entry_message(file_path: Option<&str>) -> String {
    let path = file_path
        .filter(|p| !p.is_empty())
        .unwrap_or("(not yet assigned)");
    let companion_dir = path.strip_suffix(".md").unwrap_or(path);

    format!(
        "**Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.\n\ngame-design mode is now active. Your job is to act as a game design partner — guide the user through a complete game design process based on the 100 Principles of Game Design.\n\n## HARD GATES\n- Do NOT write code. Your output is a game design document.\n- Ask questions to clarify the vision, audience, and constraints.\n- Design file (write ONLY to this path): {}\n- You may create companion .md files in the {}/ subdirectory.\n\n## Available Game Design Skills\nUse the Skill tool to invoke specialized game design skills (game-design/*) for deep dives into specific areas: flow state, difficulty adjustment, puzzle design, player psychology, visual guidance, prototyping, team management, and more.\n\n## Core Workflow\n\nFollow these phases in order. Move forward only when the current phase has enough clarity to support the next one.\n\n### Phase 1: Concept Definition\n1. Define 3 design pillars — describe core play with action verbs and combine them into one sentence.\n2. Write a problem statement — specific focus + measurable result + clear expression. Use the 80/20 rule to focus on core features.\n3. Constraint triangle — fast, cheap, good: pick two. Cut scope before quality.\n\n### Phase 2: Core Loop Design\nThe core loop is the interesting behavior players want to repeat. Action → result → reaction → repeat. Describe the core action with verbs. It must be easy to understand, easy to operate, and give direct feedback. Warning: a broken core loop cannot be fixed by other elements.\n\n### Phase 3: Mechanics and Balance\nDifficulty design: three stages (intro / practice / flow), challenge slightly above current ability. Dynamic difficulty: adjust subtly and monitor consecutive failure / success rate / time spent. Quick balance method: test 2x or 0.5x extreme adjustments on core variables. Reward/punishment systems: lives / game over, stat decay, fixed / random rewards.\n\n### Phase 4: Levels and Experience\nChallenge types: memory-based (trial-and-error / pattern recognition) vs skill-based (physical / mental). Puzzle design: maintain flow, progressive hints, determinism, clarity. Pacing: human attention limit is 7-10 minutes; introduce something new every ~7 minutes. Environmental storytelling: tell story through graffiti, doors/windows, NPC dialogue, private spaces.\n\n### Phase 5: Visuals and Interaction\nVisual guidance: affordances (visual hints of interaction), attention capture (faces > motion > surprise), wayfinding. Fitts's Law: movement time = f(distance, target size); put common elements close and large. Hick's Law: decision time grows logarithmically with options; optimal is 3-6 options. Golden ratio: Φ=1.618 for UI layout / architecture / environmental art.\n\n### Phase 6: Player Psychology\nCognitive bias checklist: confirmation bias, availability bias, anchoring, framing effect. Decision design: triangulation (low-risk low-reward vs high-risk high-reward paths). Error handling: classify and address execution, lapse, slip, and mistake errors.\n\n### Phase 7: Prototyping and Testing\nPaper prototypes (UI / card / board game) and digital prototypes (feel / timing). Testing: first impression, black-box / white-box / stress testing. Loop: prototype → test → analyze → iterate.\n\n### Phase 8: Team Management\nShared vision, diversity paradox, process choice (waterfall vs agile), communication principles.\n\n## Output Conventions\n- Suggest concrete principles by name.\n- Give actionable next steps, not vague advice.\n- Use tables to compare options and trade-offs.\n- Tag decisions: [C:USER] for user-confirmed, [C:INFERRED] for inferred.\n- Include an ## Assumptions section.\n\n## Output File\n- Main document: {}\n- Companion files: {}/<topic>.md\n- Call SyncGameDesignArtifact when ready to persist.\n- Call ExitGameDesignMode when the design is complete.",
        path, companion_dir, path, companion_dir
    )
}

fn mode_not_active_error(lang: Language) -> ExecutableToolResult {
    ExecutableToolResult::error_text(
        t(Msg::GameDesignModeNotActive, lang),
        "not in game-design mode".into(),
    )
}

fn another_mode_active_error(kind: SessionModeKind) -> String {
    let (name, exit_tool) = match kind {
        SessionModeKind::Plan => ("Plan", "ExitPlanMode"),
        SessionModeKind::Design => ("Design", "ExitDesignMode"),
        SessionModeKind::GameDesign => ("Game-design", "ExitGameDesignMode"),
        SessionModeKind::OfficeHours => ("Office-hours", "ExitOfficeHoursMode"),
    };
    format!(
        "{} mode is already active. Use {} when you are ready to exit {} mode; do not try to enter another mode on top of it.",
        name,
        exit_tool,
        name.to_lowercase()
    )
}

fn is_game_design_active(provider: &dyn SessionModeProvider) -> bool {
    provider.is_session_mode_active()
        && provider.session_mode_kind() == Some(SessionModeKind::GameDesign)
}

// ---------------------------------------------------------------------------
// EnterGameDesignMode
// ---------------------------------------------------------------------------

pub struct EnterGameDesignModeTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl EnterGameDesignModeTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for EnterGameDesignModeTool {
    fn name(&self) -> &str {
        "EnterGameDesignMode"
    }

    fn description(&self) -> &str {
        "Enter game-design mode. Produces a game design document, not code."
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
                        let active = provider
                            .session_mode_kind()
                            .unwrap_or(SessionModeKind::GameDesign);
                        return ExecutableToolResult::error_text(
                            another_mode_active_error(active),
                            "session mode already active".into(),
                        );
                    }

                    if let Err(e) = provider
                        .enter_session_mode(SessionModeKind::GameDesign)
                        .await
                    {
                        return ExecutableToolResult::error_text(
                            subst(
                                &t(Msg::GameDesignFailedToEnter, lang),
                                &[("message", &e.to_string())],
                            ),
                            "enter failed".into(),
                        );
                    }

                    provider.telemetry().track(
                        "game_design_enter_resolved",
                        std::collections::HashMap::from([(
                            "outcome".into(),
                            "auto_approved".into(),
                        )]),
                    );

                    let msg =
                        game_design_entry_message(provider.session_mode_file_path().as_deref());
                    ExecutableToolResult::ok_text(msg)
                })
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// ExitGameDesignMode
// ---------------------------------------------------------------------------

pub struct ExitGameDesignModeTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl ExitGameDesignModeTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for ExitGameDesignModeTool {
    fn name(&self) -> &str {
        "ExitGameDesignMode"
    }

    fn description(&self) -> &str {
        "Exit game-design mode after the design doc is complete."
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

                    if !is_game_design_active(&*provider) {
                        return mode_not_active_error(lang);
                    }

                    let path = provider.session_mode_file_path();

                    if let Err(e) = provider.exit_session_mode().await {
                        return ExecutableToolResult::error_text(
                            format!("Failed to exit game-design mode: {}", e),
                            "exit failed".into(),
                        );
                    }

                    provider.telemetry().track(
                        "game_design_exit_resolved",
                        std::collections::HashMap::from([(
                            "outcome".into(),
                            "auto_approved".into(),
                        )]),
                    );

                    let mut parts = vec![t(Msg::GameDesignSessionComplete, lang)];
                    if let Some(p) = &path {
                        if !p.is_empty() {
                            parts.push(subst(
                                &t(Msg::GameDesignDesignDocSaved, lang),
                                &[("path", p)],
                            ));
                        }
                    }
                    parts.push(t(Msg::GameDesignAppWillExit, lang));
                    ExecutableToolResult::ok_text(parts.join("\n"))
                })
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// AppendGameDesignProfile
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppendGameDesignProfileInput {
    pub mode: String,
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
    fn name(&self) -> &str {
        "AppendGameDesignProfile"
    }

    fn description(&self) -> &str {
        "Append a game-design profile entry to the game-design state store. Only available during game-design mode."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "mode": { "type": "string", "enum": ["startup", "builder"], "description": "Whether this is a full design startup or a builder session." },
                "projectSlug": { "type": "string", "description": "Project slug." },
                "pillars": { "type": "string", "description": "The 3 design pillars as a comma-separated string." },
                "audience": { "type": "string", "description": "Target audience description." },
                "platform": { "type": "string", "description": "Target platform(s)." },
                "genre": { "type": "string", "description": "Game genre." },
                "designDoc": { "type": "string", "description": "Path to the design document. Defaults to the current game-design file path." },
                "signals": { "type": "array", "items": { "type": "string" }, "description": "Design signals observed." }
            },
            "required": ["mode", "projectSlug", "pillars", "audience", "platform", "genre"],
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let input: AppendGameDesignProfileInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

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

                    if !is_game_design_active(&*provider) {
                        return mode_not_active_error(lang);
                    }

                    let design_doc = input
                        .design_doc
                        .or_else(|| provider.session_mode_file_path())
                        .unwrap_or_default();

                    let entry = GameDesignProfileEntry {
                        date: chrono::Utc::now().to_rfc3339(),
                        mode: input.mode,
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
                            "append profile failed".into(),
                        );
                    }

                    ExecutableToolResult::ok_text(t(Msg::GameDesignProfileAppended, lang))
                })
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// AppendGameDesignLearning
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
    fn name(&self) -> &str {
        "AppendGameDesignLearning"
    }

    fn description(&self) -> &str {
        "Append an operational or eureka learning insight to the game-design state store."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "type": { "type": "string", "enum": ["operational", "eureka"], "description": "Type of learning: operational (process/technique) or eureka (insight/discovery)." },
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
        let input: AppendGameDesignLearningInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

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

                    if !is_game_design_active(&*provider) {
                        return mode_not_active_error(lang);
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
                            "append learning failed".into(),
                        );
                    }

                    ExecutableToolResult::ok_text(subst(
                        &t(Msg::GameDesignLearningRecorded, lang),
                        &[("key", &input.key)],
                    ))
                })
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// SearchGameDesignLearnings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchGameDesignLearningsInput {
    #[serde(default = "default_search_limit")]
    pub limit: usize,
    #[serde(default)]
    pub branch: Option<String>,
}

fn default_search_limit() -> usize {
    10
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
    fn name(&self) -> &str {
        "SearchGameDesignLearnings"
    }

    fn description(&self) -> &str {
        "Search past game-design learnings from the state store."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "minimum": 1, "default": 10, "description": "Maximum number of learnings to return." },
                "branch": { "type": "string", "description": "Optional git branch identifier to filter by." }
            },
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let input: SearchGameDesignLearningsInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

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

                    if !is_game_design_active(&*provider) {
                        return mode_not_active_error(lang);
                    }

                    let learnings = match provider
                        .game_design_store()
                        .search_learnings(input.limit, input.branch.clone())
                        .await
                    {
                        Ok(v) => v,
                        Err(e) => {
                            return ExecutableToolResult::error_text(
                                format!("Failed to search learnings: {}", e),
                                "search learnings failed".into(),
                            );
                        }
                    };

                    if learnings.is_empty() {
                        return ExecutableToolResult::ok_text(t(Msg::GameDesignNoLearnings, lang));
                    }

                    let type_label = t(Msg::GameDesignLearningTypeLabel, lang);
                    let insight_label = t(Msg::GameDesignLearningInsightLabel, lang);
                    let confidence_label = t(Msg::GameDesignLearningConfidenceLabel, lang);
                    let date_label = t(Msg::GameDesignLearningDateLabel, lang);
                    let branch_label = t(Msg::GameDesignLearningBranchLabel, lang);

                    let formatted = learnings
                        .iter()
                        .enumerate()
                        .map(|(i, l)| {
                            let mut lines = vec![format!(
                                "[{}] {}: {} | KEY: {}",
                                i + 1,
                                type_label.to_uppercase(),
                                l.type_.to_uppercase(),
                                l.key
                            )];
                            lines.push(format!("    {}: {}", insight_label, l.insight));
                            lines.push(format!("    {}: {}", confidence_label, l.confidence));
                            lines.push(format!("    {}: {}", date_label, l.ts));
                            if let Some(branch) = &l.branch {
                                lines.push(format!("    {}: {}", branch_label, branch));
                            }
                            lines.join("\n")
                        })
                        .collect::<Vec<_>>()
                        .join("\n\n");

                    ExecutableToolResult::ok_text(format!(
                        "{}\n\n{}",
                        subst(
                            &t(Msg::GameDesignLearningsHeader, lang),
                            &[("count", &learnings.len().to_string())],
                        ),
                        formatted
                    ))
                })
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// SetGameDesignLanguage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
    fn name(&self) -> &str {
        "SetGameDesignLanguage"
    }

    fn description(&self) -> &str {
        "Set the user language for the current game-design session. Use 'en' or 'zh'."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "language": { "type": "string", "description": "Language code: 'en' or 'zh'." }
            },
            "required": ["language"],
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let input: SetGameDesignLanguageInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

        let provider = Arc::clone(&self.provider);
        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Setting game-design user language".into(),
            approval_rule: "SetGameDesignLanguage".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let input = input.clone();
                Box::pin(async move {
                    let lang = provider.user_language();

                    if !is_game_design_active(&*provider) {
                        return mode_not_active_error(lang);
                    }

                    let new_lang = match Language::from_str(&input.language) {
                        Some(l) => l,
                        None => {
                            return ExecutableToolResult::error_text(
                                format!("Unsupported language: {}", input.language),
                                "unsupported language".into(),
                            );
                        }
                    };

                    provider.set_user_language(new_lang);
                    ExecutableToolResult::ok_text(subst(
                        &t(Msg::GameDesignLanguageSet, lang),
                        &[("language", &input.language)],
                    ))
                })
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// EnsureGameDesignRouting
// ---------------------------------------------------------------------------

const ROUTING_SECTION: &str = "\n## Skill routing\n\n- **game-design**: Game design workflow based on the 100 Principles of Game Design. Activates via --game-design or when the user requests game design help.\n\nTo invoke, ask the agent to start game-design mode.\n";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnsureGameDesignRoutingInput {}

pub struct EnsureGameDesignRoutingTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl EnsureGameDesignRoutingTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for EnsureGameDesignRoutingTool {
    fn name(&self) -> &str {
        "EnsureGameDesignRouting"
    }

    fn description(&self) -> &str {
        "Ensure AGENTS.md has a skill routing section for game-design."
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

                    if !is_game_design_active(&*provider) {
                        return mode_not_active_error(lang);
                    }

                    let cwd = provider.kaos().cwd();
                    let path = Path::new(&cwd).join("AGENTS.md");
                    let path_str = path.to_string_lossy().to_string();

                    let (content, exists) = match provider.kaos().read_text(&path_str).await {
                        Ok(c) => (c, true),
                        Err(_) => (String::new(), false),
                    };

                    if !exists {
                        let new_content = ROUTING_SECTION.trim_start();
                        if let Err(e) = provider.kaos().write_text(&path_str, new_content).await {
                            return ExecutableToolResult::error_text(
                                subst(
                                    &t(Msg::GameDesignFailedToEnsureRouting, lang),
                                    &[("message", &e.to_string())],
                                ),
                                "create failed".into(),
                            );
                        }
                        return ExecutableToolResult::ok_text(subst(
                            &t(Msg::GameDesignAgentsMdCreated, lang),
                            &[("path", &path_str)],
                        ));
                    }

                    if content.contains("## Skill routing") {
                        return ExecutableToolResult::ok_text(t(
                            Msg::GameDesignAgentsMdAlreadyHasRouting,
                            lang,
                        ));
                    }

                    let updated = format!("{}\n{}", content.trim_end(), ROUTING_SECTION);
                    if let Err(e) = provider.kaos().write_text(&path_str, &updated).await {
                        return ExecutableToolResult::error_text(
                            subst(
                                &t(Msg::GameDesignFailedToEnsureRouting, lang),
                                &[("message", &e.to_string())],
                            ),
                            "update failed".into(),
                        );
                    }

                    ExecutableToolResult::ok_text(subst(
                        &t(Msg::GameDesignAgentsMdUpdated, lang),
                        &[("path", &path_str)],
                    ))
                })
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// SyncGameDesignArtifact
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncGameDesignArtifactInput {
    pub design_file_path: String,
}

pub struct SyncGameDesignArtifactTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl SyncGameDesignArtifactTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for SyncGameDesignArtifactTool {
    fn name(&self) -> &str {
        "SyncGameDesignArtifact"
    }

    fn description(&self) -> &str {
        "Sync the game-design design document artifact to gbrain."
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
        let input: SyncGameDesignArtifactInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

        let provider = Arc::clone(&self.provider);
        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Syncing game-design artifact to gbrain".into(),
            approval_rule: "SyncGameDesignArtifact".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let input = input.clone();
                Box::pin(async move {
                    let lang = provider.user_language();

                    if !is_game_design_active(&*provider) {
                        return mode_not_active_error(lang);
                    }

                    let project_root = provider
                        .kaos()
                        .project_root()
                        .unwrap_or_else(|| provider.kaos().cwd());
                    let pin_path = Path::new(&project_root)
                        .join(".gbrain-source")
                        .to_string_lossy()
                        .to_string();

                    // Verify design file exists.
                    if let Err(_) = provider.kaos().stat(&input.design_file_path).await {
                        return ExecutableToolResult::error_text(
                            subst(
                                &t(Msg::GameDesignDesignFileNotFound, lang),
                                &[("path", &input.design_file_path)],
                            ),
                            "design file not found".into(),
                        );
                    }

                    // Read optional .gbrain-source pin.
                    let gbrain_source = match provider.kaos().read_text(&pin_path).await {
                        Ok(s) => {
                            let s = s.trim();
                            if s.is_empty() {
                                None
                            } else {
                                Some(s.to_string())
                            }
                        }
                        Err(_) => None,
                    };

                    // Prefer MCP-based sync when available.
                    if provider.mcp().gbrain_available().await {
                        let mut lines = vec![t(Msg::GameDesignGbrainConnected, lang)];
                        if let Some(source) = &gbrain_source {
                            lines.push(subst(
                                &t(Msg::GameDesignGbrainTargetSource, lang),
                                &[("source", source)],
                            ));
                        } else {
                            lines.push(t(Msg::GameDesignGbrainNoSourcePin, lang));
                        }
                        lines.push(subst(
                            &t(Msg::GameDesignGbrainReadyForSync, lang),
                            &[("path", &input.design_file_path)],
                        ));
                        return ExecutableToolResult::ok_text(lines.join("\n"));
                    }

                    // Fall back to the gbrain CLI.
                    let mut cli_args = vec!["artifact".to_string(), "add".to_string()];
                    if let Some(source) = &gbrain_source {
                        cli_args.push("--source".into());
                        cli_args.push(source.clone());
                    }
                    cli_args.push(input.design_file_path.clone());

                    match tokio::process::Command::new("gbrain")
                        .args(&cli_args)
                        .current_dir(&project_root)
                        .output()
                        .await
                    {
                        Ok(output) if output.status.success() => {
                            let mut lines = vec![t(Msg::GameDesignGbrainSynced, lang)];
                            if let Some(source) = &gbrain_source {
                                lines.push(subst(
                                    &t(Msg::GameDesignGbrainTargetSource, lang),
                                    &[("source", source)],
                                ));
                            }
                            lines.push(subst(
                                &t(Msg::GameDesignGbrainFile, lang),
                                &[("path", &input.design_file_path)],
                            ));
                            ExecutableToolResult::ok_text(lines.join("\n"))
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            ExecutableToolResult::error_text(
                                subst(
                                    &t(Msg::GameDesignGbrainCliFailed, lang),
                                    &[("message", &stderr)],
                                ),
                                "gbrain cli failed".into(),
                            )
                        }
                        Err(e) => ExecutableToolResult::error_text(
                            subst(
                                &t(Msg::GameDesignFailedToSyncArtifact, lang),
                                &[("message", &e.to_string())],
                            ),
                            "gbrain cli failed".into(),
                        ),
                    }
                })
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::session_mode::stores::{
        InMemoryGameDesignStateStore, InMemoryOfficeHoursStateStore,
    };
    use crate::builtin::session_mode::tests::{
        MockKaosContext, MockMcpProvider, MockSessionModeProvider, MockTelemetryClient,
    };
    use crate::builtin::session_mode::GameDesignStateStore;

    fn provider_with_store(
        store: Arc<InMemoryGameDesignStateStore>,
    ) -> Arc<MockSessionModeProvider> {
        Arc::new(MockSessionModeProvider {
            active: std::sync::Mutex::new(false),
            kind: std::sync::Mutex::new(None),
            file_path: std::sync::Mutex::new(None),
            entered: std::sync::Mutex::new(Vec::new()),
            exited: std::sync::Mutex::new(false),
            handed_off_to: std::sync::Mutex::new(Vec::new()),
            kaos: Arc::new(MockKaosContext::new()),
            telemetry: Arc::new(MockTelemetryClient::new()),
            office_hours_store: Arc::new(InMemoryOfficeHoursStateStore::new()),
            game_design_store: store,
            mcp: Arc::new(MockMcpProvider),
        })
    }

    #[tokio::test]
    async fn enter_game_design_mode_succeeds_when_inactive() {
        let provider = Arc::new(MockSessionModeProvider::inactive());
        let tool = EnterGameDesignModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error);
        assert!(result.to_text().contains("game-design mode is now active"));
        assert!(provider
            .entered
            .lock()
            .unwrap()
            .contains(&SessionModeKind::GameDesign));
    }

    #[tokio::test]
    async fn enter_game_design_mode_fails_when_plan_active() {
        let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::Plan));
        let tool = EnterGameDesignModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(result.is_error);
        assert!(result.to_text().contains("Plan mode is already active"));
        assert!(result.to_text().contains("ExitPlanMode"));
    }

    #[tokio::test]
    async fn exit_game_design_mode_succeeds_when_active() {
        let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::GameDesign));
        *provider.file_path.lock().unwrap() = Some("/game-design/design.md".into());
        let tool = ExitGameDesignModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error);
        assert!(result.to_text().contains("Game-design session complete."));
        assert!(provider.exited.lock().unwrap().clone());
    }

    #[tokio::test]
    async fn exit_game_design_mode_fails_when_inactive() {
        let provider = Arc::new(MockSessionModeProvider::inactive());
        let tool = ExitGameDesignModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(result.is_error);
        assert!(result.to_text().contains("Game-design mode is not active."));
    }

    #[tokio::test]
    async fn append_game_design_profile_stores_entry() {
        let store = Arc::new(InMemoryGameDesignStateStore::new());
        let provider = provider_with_store(store.clone());
        *provider.active.lock().unwrap() = true;
        *provider.kind.lock().unwrap() = Some(SessionModeKind::GameDesign);
        *provider.file_path.lock().unwrap() = Some("/game-design/design.md".into());
        let tool = AppendGameDesignProfileTool::new(provider.clone());
        let exec = tool
            .resolve_execution(serde_json::json!({
                "mode": "startup",
                "projectSlug": "roguelike",
                "pillars": "tactical, procedural, narrative",
                "audience": "core roguelike players",
                "platform": "PC",
                "genre": "Roguelike",
                "signals": ["strong_core_loop"]
            }))
            .unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error);
        assert_eq!(store.profiles().len(), 1);
        let entry = &store.profiles()[0];
        assert_eq!(entry.project_slug, "roguelike");
        assert_eq!(entry.pillars, "tactical, procedural, narrative");
        assert_eq!(entry.genre, "Roguelike");
        assert_eq!(entry.design_doc, "/game-design/design.md");
    }

    #[tokio::test]
    async fn append_game_design_learning_stores_entry() {
        let store = Arc::new(InMemoryGameDesignStateStore::new());
        let provider = provider_with_store(store.clone());
        *provider.active.lock().unwrap() = true;
        *provider.kind.lock().unwrap() = Some(SessionModeKind::GameDesign);
        let tool = AppendGameDesignLearningTool::new(provider.clone());
        let exec = tool
            .resolve_execution(serde_json::json!({
                "type": "eureka",
                "key": "core-loop-clarity",
                "insight": "A clear core loop matters more than extra features.",
                "confidence": 0.9,
                "branch": "main"
            }))
            .unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error);
        assert_eq!(store.learnings().len(), 1);
        let entry = &store.learnings()[0];
        assert_eq!(entry.skill, "game-design");
        assert_eq!(entry.key, "core-loop-clarity");
        assert_eq!(entry.type_, "eureka");
        assert_eq!(entry.branch.as_deref(), Some("main"));
    }

    #[tokio::test]
    async fn search_game_design_learnings_returns_results() {
        let store = Arc::new(InMemoryGameDesignStateStore::new());
        store
            .append_learning(LearningEntry {
                ts: "2024-01-01T00:00:00Z".into(),
                skill: "game-design".into(),
                type_: "operational".into(),
                key: "first".into(),
                insight: "First insight".into(),
                confidence: 0.8,
                source: "observed".into(),
                branch: None,
            })
            .await
            .unwrap();
        store
            .append_learning(LearningEntry {
                ts: "2024-01-02T00:00:00Z".into(),
                skill: "game-design".into(),
                type_: "eureka".into(),
                key: "second".into(),
                insight: "Second insight".into(),
                confidence: 0.9,
                source: "observed".into(),
                branch: None,
            })
            .await
            .unwrap();

        let provider = provider_with_store(store.clone());
        *provider.active.lock().unwrap() = true;
        *provider.kind.lock().unwrap() = Some(SessionModeKind::GameDesign);
        let tool = SearchGameDesignLearningsTool::new(provider.clone());
        let exec = tool
            .resolve_execution(serde_json::json!({ "limit": 1 }))
            .unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error);
        let text = result.to_text();
        assert!(text.contains("Found 1 learning(s):"));
        assert!(text.contains("KEY: second"));
        assert!(!text.contains("KEY: first"));
    }

    #[tokio::test]
    async fn set_game_design_language_updates_language() {
        let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::GameDesign));
        let tool = SetGameDesignLanguageTool::new(provider.clone());
        let exec = tool
            .resolve_execution(serde_json::json!({ "language": "zh" }))
            .unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error);
        assert!(result.to_text().contains("User language set to zh."));
    }

    #[tokio::test]
    async fn ensure_game_design_routing_creates_agents_md() {
        let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::GameDesign));
        let tool = EnsureGameDesignRoutingTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error);
        assert!(result.to_text().contains("AGENTS.md created at"));
    }

    #[tokio::test]
    async fn sync_game_design_artifact_reports_connected_when_mcp_available() {
        struct AlwaysAvailableMcp;
        #[async_trait::async_trait]
        impl crate::builtin::session_mode::McpProvider for AlwaysAvailableMcp {
            async fn gbrain_available(&self) -> bool {
                true
            }
        }

        let mut provider = MockSessionModeProvider::active(SessionModeKind::GameDesign);
        provider.mcp = Arc::new(AlwaysAvailableMcp);
        let provider = Arc::new(provider);
        let tool = SyncGameDesignArtifactTool::new(provider.clone());
        let exec = tool
            .resolve_execution(serde_json::json!({ "designFilePath": "/game-design/design.md" }))
            .unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error);
        assert!(result.to_text().contains("gbrain MCP server is connected."));
    }
}
