//! Office-hours session-mode tools.
//!
//! Mirrors the upstream TypeScript office-hours builtins:
//! - Enter/Exit office-hours mode
//! - Append builder profile and learning entries
//! - Search past learnings
//! - Set the user language for the session
//! - Ensure `AGENTS.md` has an office-hours skill routing section
//! - Sync the design artifact to gbrain

use std::path::Path;
use std::sync::Arc;

use serde_json::Value;

use crate::builtin::session_mode::i18n::{subst, t, Msg};
use crate::builtin::session_mode::{
    BuilderProfileEntry, Language, LearningEntry, SessionModeKind, SessionModeProvider,
};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};

// ---------------------------------------------------------------------------
// Entry / exit messages
// ---------------------------------------------------------------------------

pub fn office_hours_entry_message(file_path: Option<&str>) -> String {
    let file_line = match file_path {
        Some(p) if !p.is_empty() => format!(
            "Design file: {}\nWrite the design doc to EXACTLY this path.",
            p
        ),
        _ => "No design file path is assigned yet. Invent your own filename under `.ody-code/office-hours/` (format: `YYYY-MM-DD-<topic>.md`). The host will normalize and deduplicate it on first write.".into(),
    };

    format!(
        "Office hours is now active. Your job is to act as a YC office hours partner.\nDo NOT write code. Produce only a design document.\n\n**Language:** Respond in the same language the user writes in — Chinese if they write Chinese, English if they write English.\n\n{}\n\nFollow the workflow below. Ask ONE question at a time via AskUserQuestion.\n\n1. **HARD GATE**: no implementation until the design is approved.\n2. **Voice**: builder-to-builder, concrete, no AI buzzwords.\n3. Gather context, then run the startup/builder diagnostic.\n4. Challenge premises and explore alternatives.\n5. Synthesize founder signals and append the builder profile.\n6. Write the design doc and exit office-hours mode.",
        file_line
    )
}

fn mode_not_active_error(lang: Language) -> ExecutableToolResult {
    ExecutableToolResult::error_text(
        t(Msg::OfficeHoursModeNotActive, lang),
        "not in office-hours mode".into(),
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

fn is_office_hours_active(provider: &dyn SessionModeProvider) -> bool {
    provider.is_session_mode_active()
        && provider.session_mode_kind() == Some(SessionModeKind::OfficeHours)
}

// ---------------------------------------------------------------------------
// EnterOfficeHoursMode
// ---------------------------------------------------------------------------

pub struct EnterOfficeHoursModeTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl EnterOfficeHoursModeTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for EnterOfficeHoursModeTool {
    fn name(&self) -> &str {
        "EnterOfficeHoursMode"
    }

    fn description(&self) -> &str {
        "Enter YC-style office-hours mode. Produces a design document, not code."
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
                    if provider.is_session_mode_active() {
                        let active = provider
                            .session_mode_kind()
                            .unwrap_or(SessionModeKind::OfficeHours);
                        return ExecutableToolResult::error_text(
                            another_mode_active_error(active),
                            "session mode already active".into(),
                        );
                    }

                    let lang = provider.user_language();

                    if let Err(e) = provider
                        .enter_session_mode(SessionModeKind::OfficeHours)
                        .await
                    {
                        return ExecutableToolResult::error_text(
                            subst(
                                &t(Msg::OfficeHoursFailedToEnter, lang),
                                &[("message", &e.to_string())],
                            ),
                            "enter failed".into(),
                        );
                    }

                    provider.telemetry().track(
                        "office_hours_enter_resolved",
                        std::collections::HashMap::from([(
                            "outcome".into(),
                            "auto_approved".into(),
                        )]),
                    );

                    let msg =
                        office_hours_entry_message(provider.session_mode_file_path().as_deref());
                    ExecutableToolResult::ok_text(msg)
                })
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// ExitOfficeHoursMode
// ---------------------------------------------------------------------------

pub struct ExitOfficeHoursModeTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl ExitOfficeHoursModeTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for ExitOfficeHoursModeTool {
    fn name(&self) -> &str {
        "ExitOfficeHoursMode"
    }

    fn description(&self) -> &str {
        "Exit office-hours mode after the design doc is complete."
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

                    if !is_office_hours_active(&*provider) {
                        return mode_not_active_error(lang);
                    }

                    let path = provider.session_mode_file_path();

                    if let Err(e) = provider.exit_session_mode().await {
                        return ExecutableToolResult::error_text(
                            format!("Failed to exit office hours mode: {}", e),
                            "exit failed".into(),
                        );
                    }

                    provider.telemetry().track(
                        "office_hours_exit_resolved",
                        std::collections::HashMap::from([(
                            "outcome".into(),
                            "auto_approved".into(),
                        )]),
                    );

                    let mut parts = vec![t(Msg::OfficeHoursSessionComplete, lang)];
                    if let Some(p) = &path {
                        if !p.is_empty() {
                            parts.push(subst(
                                &t(Msg::OfficeHoursDesignDocSaved, lang),
                                &[("path", p)],
                            ));
                        }
                    }
                    parts.push(t(Msg::OfficeHoursAppWillExit, lang));
                    ExecutableToolResult::ok_text(parts.join("\n"))
                })
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// AppendBuilderProfile
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppendBuilderProfileInput {
    pub mode: String,
    pub project_slug: String,
    pub signal_count: u64,
    pub signals: Vec<String>,
    #[serde(default)]
    pub design_doc: Option<String>,
    #[serde(default)]
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
    fn name(&self) -> &str {
        "AppendBuilderProfile"
    }

    fn description(&self) -> &str {
        "Append a builder profile entry to the office-hours state store. Only available during office-hours mode."
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
        let input: AppendBuilderProfileInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

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

                    if !is_office_hours_active(&*provider) {
                        return mode_not_active_error(lang);
                    }

                    let design_doc = input
                        .design_doc
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
                            "append profile failed".into(),
                        );
                    }

                    ExecutableToolResult::ok_text(t(Msg::OfficeHoursProfileAppended, lang))
                })
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// AppendLearning
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
    fn name(&self) -> &str {
        "AppendLearning"
    }

    fn description(&self) -> &str {
        "Append an operational or eureka learning insight to the office-hours state store."
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
        let input: AppendLearningInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

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

                    if !is_office_hours_active(&*provider) {
                        return mode_not_active_error(lang);
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
                            "append learning failed".into(),
                        );
                    }

                    ExecutableToolResult::ok_text(subst(
                        &t(Msg::OfficeHoursLearningRecorded, lang),
                        &[("key", &input.key)],
                    ))
                })
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// SearchLearnings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchLearningsInput {
    #[serde(default = "default_search_limit")]
    pub limit: usize,
    #[serde(default)]
    pub cross_project: Option<bool>,
}

fn default_search_limit() -> usize {
    10
}

pub struct SearchLearningsTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl SearchLearningsTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for SearchLearningsTool {
    fn name(&self) -> &str {
        "SearchLearnings"
    }

    fn description(&self) -> &str {
        "Search past office-hours learnings from the state store."
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
        let input: SearchLearningsInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

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

                    if !is_office_hours_active(&*provider) {
                        return mode_not_active_error(lang);
                    }

                    let cross_project = input.cross_project.unwrap_or(false);
                    let learnings = match provider
                        .office_hours_store()
                        .search_learnings(input.limit, cross_project)
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
                        return ExecutableToolResult::ok_text(t(Msg::OfficeHoursNoLearnings, lang));
                    }

                    let type_label = t(Msg::OfficeHoursLearningTypeLabel, lang);
                    let insight_label = t(Msg::OfficeHoursLearningInsightLabel, lang);
                    let confidence_label = t(Msg::OfficeHoursLearningConfidenceLabel, lang);
                    let date_label = t(Msg::OfficeHoursLearningDateLabel, lang);
                    let branch_label = t(Msg::OfficeHoursLearningBranchLabel, lang);

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
                            &t(Msg::OfficeHoursLearningsHeader, lang),
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
// SetOfficeHoursLanguage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
    fn name(&self) -> &str {
        "SetOfficeHoursLanguage"
    }

    fn description(&self) -> &str {
        "Set the user language for the current office-hours session. Use 'en' or 'zh'."
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
        let input: SetOfficeHoursLanguageInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

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
                    let lang = provider.user_language();

                    if !is_office_hours_active(&*provider) {
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
                        &t(Msg::OfficeHoursLanguageSet, lang),
                        &[("language", &input.language)],
                    ))
                })
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// EnsureClaudeMdRouting
// ---------------------------------------------------------------------------

const ROUTING_SECTION: &str = "\n## Skill routing\n\n- **office-hours**: YC office hours diagnostic workflow. Activates when the user explicitly requests office hours or asks for startup/builder diagnostic help.\n\nTo invoke, ask the agent to start office hours.\n";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnsureClaudeMdRoutingInput {}

pub struct EnsureClaudeMdRoutingTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl EnsureClaudeMdRoutingTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for EnsureClaudeMdRoutingTool {
    fn name(&self) -> &str {
        "EnsureClaudeMdRouting"
    }

    fn description(&self) -> &str {
        "Ensure AGENTS.md has a skill routing section for office hours."
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

                    if !is_office_hours_active(&*provider) {
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
                                    &t(Msg::OfficeHoursFailedToEnsureRouting, lang),
                                    &[("message", &e.to_string())],
                                ),
                                "create failed".into(),
                            );
                        }
                        return ExecutableToolResult::ok_text(subst(
                            &t(Msg::OfficeHoursAgentsMdCreated, lang),
                            &[("path", &path_str)],
                        ));
                    }

                    if content.contains("## Skill routing") {
                        return ExecutableToolResult::ok_text(t(
                            Msg::OfficeHoursAgentsMdAlreadyHasRouting,
                            lang,
                        ));
                    }

                    let updated = format!("{}\n{}", content.trim_end(), ROUTING_SECTION);
                    if let Err(e) = provider.kaos().write_text(&path_str, &updated).await {
                        return ExecutableToolResult::error_text(
                            subst(
                                &t(Msg::OfficeHoursFailedToEnsureRouting, lang),
                                &[("message", &e.to_string())],
                            ),
                            "update failed".into(),
                        );
                    }

                    ExecutableToolResult::ok_text(subst(
                        &t(Msg::OfficeHoursAgentsMdUpdated, lang),
                        &[("path", &path_str)],
                    ))
                })
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// SyncOfficeHoursArtifact
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOfficeHoursArtifactInput {
    pub design_file_path: String,
}

pub struct SyncOfficeHoursArtifactTool {
    provider: Arc<dyn SessionModeProvider>,
}

impl SyncOfficeHoursArtifactTool {
    pub fn new(provider: Arc<dyn SessionModeProvider>) -> Self {
        Self { provider }
    }
}

impl BuiltinTool for SyncOfficeHoursArtifactTool {
    fn name(&self) -> &str {
        "SyncOfficeHoursArtifact"
    }

    fn description(&self) -> &str {
        "Sync the office-hours design document artifact to gbrain."
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
        let input: SyncOfficeHoursArtifactInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;

        let provider = Arc::clone(&self.provider);
        Ok(ToolExecution {
            accesses: Default::default(),
            description: "Syncing design artifact to gbrain".into(),
            approval_rule: "SyncOfficeHoursArtifact".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let provider = Arc::clone(&provider);
                let input = input.clone();
                Box::pin(async move {
                    let lang = provider.user_language();

                    if !is_office_hours_active(&*provider) {
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
                                &t(Msg::OfficeHoursDesignFileNotFound, lang),
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
                        let mut lines = vec![t(Msg::OfficeHoursGbrainConnected, lang)];
                        if let Some(source) = &gbrain_source {
                            lines.push(subst(
                                &t(Msg::OfficeHoursGbrainTargetSource, lang),
                                &[("source", source)],
                            ));
                        } else {
                            lines.push(t(Msg::OfficeHoursGbrainNoSourcePin, lang));
                        }
                        lines.push(subst(
                            &t(Msg::OfficeHoursGbrainReadyForSync, lang),
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
                            let mut lines = vec![t(Msg::OfficeHoursGbrainSynced, lang)];
                            if let Some(source) = &gbrain_source {
                                lines.push(subst(
                                    &t(Msg::OfficeHoursGbrainTargetSource, lang),
                                    &[("source", source)],
                                ));
                            }
                            lines.push(subst(
                                &t(Msg::OfficeHoursGbrainFile, lang),
                                &[("path", &input.design_file_path)],
                            ));
                            ExecutableToolResult::ok_text(lines.join("\n"))
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            ExecutableToolResult::error_text(
                                subst(
                                    &t(Msg::OfficeHoursGbrainCliFailed, lang),
                                    &[("message", &stderr)],
                                ),
                                "gbrain cli failed".into(),
                            )
                        }
                        Err(e) => ExecutableToolResult::error_text(
                            subst(
                                &t(Msg::OfficeHoursFailedToSyncArtifact, lang),
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

    fn provider_with_store(
        store: Arc<InMemoryOfficeHoursStateStore>,
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
            office_hours_store: store,
            game_design_store: Arc::new(InMemoryGameDesignStateStore::new()),
            mcp: Arc::new(MockMcpProvider),
        })
    }

    #[tokio::test]
    async fn enter_office_hours_mode_succeeds_when_inactive() {
        let provider = Arc::new(MockSessionModeProvider::inactive());
        let tool = EnterOfficeHoursModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error);
        assert!(result.to_text().contains("Office hours is now active"));
        assert!(provider
            .entered
            .lock()
            .unwrap()
            .contains(&SessionModeKind::OfficeHours));
    }

    #[tokio::test]
    async fn enter_office_hours_mode_fails_when_plan_active() {
        let provider = Arc::new(MockSessionModeProvider::active(SessionModeKind::Plan));
        let tool = EnterOfficeHoursModeTool::new(provider.clone());
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
    async fn exit_office_hours_mode_succeeds_when_active() {
        let provider = Arc::new(MockSessionModeProvider::active(
            SessionModeKind::OfficeHours,
        ));
        *provider.file_path.lock().unwrap() = Some("/office-hours/design.md".into());
        let tool = ExitOfficeHoursModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error);
        assert!(result.to_text().contains("Office hours session complete."));
        assert!(provider.exited.lock().unwrap().clone());
    }

    #[tokio::test]
    async fn exit_office_hours_mode_fails_when_inactive() {
        let provider = Arc::new(MockSessionModeProvider::inactive());
        let tool = ExitOfficeHoursModeTool::new(provider.clone());
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(result.is_error);
        assert!(result
            .to_text()
            .contains("Office hours mode is not active."));
    }

    #[tokio::test]
    async fn append_builder_profile_stores_entry() {
        let store = Arc::new(InMemoryOfficeHoursStateStore::new());
        let provider = provider_with_store(store.clone());
        *provider.active.lock().unwrap() = true;
        *provider.kind.lock().unwrap() = Some(SessionModeKind::OfficeHours);
        *provider.file_path.lock().unwrap() = Some("/office-hours/design.md".into());
        let tool = AppendBuilderProfileTool::new(provider.clone());
        let exec = tool
            .resolve_execution(serde_json::json!({
                "mode": "startup",
                "projectSlug": "acme",
                "signalCount": 3,
                "signals": ["demand_stated", "named_users"],
                "resourcesShown": ["https://example.com"],
                "topics": ["b2b", "saas"]
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

        let profiles = store.profiles();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].project_slug, "acme");
        assert_eq!(profiles[0].mode, "startup");
        assert_eq!(profiles[0].design_doc, "/office-hours/design.md");
    }

    #[tokio::test]
    async fn append_learning_and_search() {
        let store = Arc::new(InMemoryOfficeHoursStateStore::new());
        let provider = provider_with_store(store.clone());
        *provider.active.lock().unwrap() = true;
        *provider.kind.lock().unwrap() = Some(SessionModeKind::OfficeHours);
        let tool = AppendLearningTool::new(provider.clone());
        let exec = tool
            .resolve_execution(serde_json::json!({
                "type": "eureka",
                "key": "market-gap",
                "insight": "There is a clear gap in the market.",
                "confidence": 0.9
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

        let search = SearchLearningsTool::new(provider.clone());
        let exec = search.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "2".into(),
            tool_call_id: "call_2".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error);
        assert!(result.to_text().contains("market-gap"));
    }

    #[tokio::test]
    async fn set_language_changes_provider_language() {
        let provider = Arc::new(MockSessionModeProvider::active(
            SessionModeKind::OfficeHours,
        ));
        let tool = SetOfficeHoursLanguageTool::new(provider.clone());
        let exec = tool
            .resolve_execution(serde_json::json!({ "language": "zh-CN" }))
            .unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn ensure_routing_creates_agents_md() {
        let provider = Arc::new(MockSessionModeProvider::active(
            SessionModeKind::OfficeHours,
        ));
        let tool = EnsureClaudeMdRoutingTool::new(provider.clone());
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
}
