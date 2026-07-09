use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use kaos_rs::kaos::Kaos;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::args_validator::{compile_tool_args_validator, validate_tool_args};
use crate::builtin::background::{self, BackgroundTaskInfoData};
use crate::builtin::collaboration::skill::{SkillTool, SkillToolOptions};
use crate::builtin::collaboration::{SkillActivationOrigin, SkillError, SkillInfo, SkillProvider};
use crate::builtin::cron::{self, CronManager as _};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult,
};
use crate::file_type::{
    detect_file_type, sniff_image_dimensions, sniff_media_from_magic, FileKind,
};
use crate::policies::path_access::{
    assert_path_allowed, canonicalize_path, normalize_user_path, resolve_path_access,
    PathAccessOperation, PathClass, WorkspaceAccessPolicy, WorkspaceGuardMode,
};
use crate::policies::path_glob_match::PermissionPathMatchOptions;
use crate::policies::rule_match::{
    escape_rule_subject_literal, literal_rule_pattern, matches_glob_rule_subject,
    matches_path_rule_subject,
};
use crate::policies::sensitive::is_sensitive_file;
use crate::result_builder::ToolResultBuilder;
use crate::rg_locator::{find_existing_rg, RgResolution, RgResolutionSource};
use crate::tool_accesses::ToolAccesses;
use crate::tool_accesses::ToolResourceAccess;
use crate::workspace::WorkspaceConfig;

#[derive(Debug, Deserialize)]
pub struct FixtureFile {
    #[allow(dead_code)]
    pub version: u32,
    pub cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
pub struct Case {
    pub name: String,
    pub op: Op,
    #[allow(dead_code)]
    pub expected: Value,
}

pub type FileSet = HashMap<String, Vec<u8>>;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Op {
    CanonicalizePath {
        path: String,
        cwd: String,
        #[serde(rename = "pathClass")]
        path_class: PathClass,
    },
    IsWithinDirectory {
        candidate: String,
        base: String,
        #[serde(rename = "pathClass")]
        path_class: PathClass,
    },
    NormalizeUserPath {
        path: String,
        #[serde(rename = "pathClass")]
        path_class: PathClass,
    },
    ResolvePathAccess {
        path: String,
        cwd: String,
        #[serde(rename = "workspaceDir")]
        workspace_dir: String,
        #[serde(rename = "additionalDirs")]
        additional_dirs: Vec<String>,
        operation: PathAccessOperation,
        #[serde(rename = "pathClass")]
        path_class: PathClass,
        #[serde(rename = "homeDir")]
        home_dir: Option<String>,
    },
    AssertPathAllowed {
        path: String,
        cwd: String,
        #[serde(rename = "workspaceDir")]
        workspace_dir: String,
        #[serde(rename = "additionalDirs")]
        additional_dirs: Vec<String>,
        mode: PathAccessOperation,
        #[serde(rename = "pathClass")]
        path_class: PathClass,
    },
    IsSensitiveFile {
        path: String,
    },
    LiteralRulePattern {
        #[serde(rename = "toolName")]
        tool_name: String,
        subject: String,
    },
    EscapeRuleSubjectLiteral {
        subject: String,
    },
    MatchesGlobRuleSubject {
        #[serde(rename = "ruleArgs")]
        rule_args: String,
        subject: String,
    },
    MatchesPathRuleSubject {
        #[serde(rename = "ruleArgs")]
        rule_args: String,
        subject: String,
        cwd: Option<String>,
        #[serde(rename = "pathClass")]
        path_class: PathClass,
    },
    ValidateArgs {
        schema: Value,
        args: Value,
    },
    AccessConflict {
        left: Vec<ToolResourceAccess>,
        right: Vec<ToolResourceAccess>,
    },
    BuildResult {
        writes: Vec<String>,
        #[serde(rename = "maxLineLength")]
        max_line_length: usize,
        #[serde(rename = "asError", default)]
        as_error: bool,
    },
    SniffMediaFromMagic {
        header: Vec<u8>,
    },
    DetectFileType {
        path: String,
        header: Option<Vec<u8>>,
    },
    SniffImageDimensions {
        header: Vec<u8>,
    },
    DetectTarget {
        arch: String,
        platform: String,
    },
    FindExistingRg {
        #[serde(rename = "pathEnv")]
        path_env: Vec<String>,
        #[serde(rename = "shareDir")]
        share_dir: String,
        #[serde(default)]
        files: FileSet,
    },
    ListDirectory {
        path: String,
        #[serde(default)]
        files: FileSet,
    },
    ReadText {
        path: String,
        #[serde(default)]
        line_offset: Option<i64>,
        #[serde(rename = "nLines", default)]
        n_lines: Option<i64>,
        #[serde(default)]
        files: FileSet,
    },
    WriteFile {
        path: String,
        content: String,
        #[serde(default)]
        mode: Option<String>,
        #[serde(default)]
        files: FileSet,
    },
    EditFile {
        path: String,
        old_string: String,
        new_string: String,
        #[serde(rename = "replaceAll", default)]
        replace_all: bool,
        #[serde(default)]
        files: FileSet,
    },
    GlobSearch {
        pattern: String,
        #[serde(default)]
        path: Option<String>,
        #[serde(rename = "includeDirs", default = "default_include_dirs")]
        include_dirs: bool,
        #[serde(default)]
        files: FileSet,
    },
    GrepSearch {
        pattern: String,
        #[serde(default)]
        path: Option<String>,
        #[serde(rename = "outputMode", default)]
        output_mode: Option<String>,
        #[serde(default)]
        files: FileSet,
    },
    ReadMedia {
        path: String,
        #[serde(default)]
        files: FileSet,
    },
    BashExec {
        command: String,
        #[serde(default)]
        timeout: Option<u64>,
        #[serde(default)]
        files: FileSet,
    },
    // ── background & cron tool ops ──
    #[serde(rename = "task_list")]
    TaskList {
        #[serde(default = "default_true")]
        active_only: Option<bool>,
        #[serde(default = "default_20")]
        limit: Option<usize>,
        #[serde(default)]
        tasks: Vec<TaskInfoDataFixture>,
    },
    #[serde(rename = "task_output")]
    TaskOutput {
        task_id: String,
        #[serde(default)]
        block: Option<bool>,
        #[serde(default)]
        timeout: Option<u64>,
        #[serde(default)]
        tasks: Vec<TaskInfoDataFixture>,
    },
    #[serde(rename = "task_stop")]
    TaskStop {
        task_id: String,
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        tasks: Vec<TaskInfoDataFixture>,
    },
    #[serde(rename = "cron_create")]
    CronCreate {
        cron: String,
        prompt: String,
        #[serde(default = "default_true")]
        recurring: Option<bool>,
        #[serde(default)]
        existing_tasks: Vec<CronTaskFixture>,
    },
    #[serde(rename = "cron_list")]
    CronList {
        #[serde(default)]
        tasks: Vec<CronTaskFixture>,
    },
    #[serde(rename = "cron_delete")]
    CronDelete {
        id: String,
        #[serde(default)]
        tasks: Vec<CronTaskFixture>,
    },
    #[serde(rename = "skill_call")]
    SkillCall {
        name: String,
        #[serde(default)]
        args: Option<String>,
        #[serde(default)]
        query_depth: Option<u32>,
        #[serde(default)]
        session_mode: Option<String>,
        skills: Vec<SkillFixture>,
    },
    #[serde(rename = "ask_user")]
    AskUser {
        questions: Vec<crate::builtin::collaboration::QuestionItem>,
        #[serde(default)]
        background: Option<bool>,
        #[serde(default)]
        provider_response: Option<String>,
        #[serde(default)]
        answers: Option<HashMap<String, serde_json::Value>>,
        #[serde(default)]
        method: Option<String>,
        #[serde(default)]
        registrar_response: Option<String>,
        #[serde(default)]
        task_id: Option<String>,
    },
    #[serde(rename = "agent_call")]
    AgentCall {
        prompt: String,
        description: String,
        #[serde(default)]
        subagent_type: Option<String>,
        #[serde(default)]
        resume: Option<String>,
        #[serde(default)]
        run_in_background: Option<bool>,
        #[serde(default)]
        timeout: Option<u64>,
        #[serde(default)]
        host_response: Option<String>,
        #[serde(default)]
        result: Option<String>,
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        profile_name: Option<String>,
        #[serde(default)]
        registrar_response: Option<String>,
        #[serde(default)]
        task_id: Option<String>,
    },
    // ── goal & state tool ops ──
    #[serde(rename = "create_goal")]
    CreateGoal {
        #[serde(rename = "storeGoal", default)]
        store_goal: Option<GoalFixture>,
        args: Value,
    },
    #[serde(rename = "get_goal")]
    GetGoal {
        #[serde(rename = "storeGoal", default)]
        store_goal: Option<GoalFixture>,
    },
    #[serde(rename = "set_goal_budget")]
    SetGoalBudget {
        #[serde(rename = "storeGoal", default)]
        store_goal: Option<GoalFixture>,
        args: Value,
    },
    #[serde(rename = "update_goal")]
    UpdateGoal {
        #[serde(rename = "storeGoal", default)]
        store_goal: Option<GoalFixture>,
        args: Value,
    },
    #[serde(rename = "todo_list")]
    TodoList {
        #[serde(rename = "storeTodos", default)]
        store_todos: Vec<TodoFixtureItem>,
        args: Value,
    },
    #[serde(rename = "checkpoint")]
    Checkpoint {
        #[serde(default = "default_true")]
        enabled: Option<bool>,
        #[serde(default)]
        reason: Option<String>,
    },
    #[serde(rename = "harvest_ody_markers")]
    HarvestOdyMarkers {
        #[serde(default)]
        files: FileSet,
        args: Value,
    },
    #[serde(rename = "save_idea_report")]
    SaveIdeaReport {
        #[serde(default)]
        files: FileSet,
        #[serde(default, rename = "existingReports")]
        existing_reports: Vec<String>,
        args: Value,
    },
    #[serde(rename = "show_design_mockup")]
    ShowDesignMockup {
        #[serde(default)]
        files: FileSet,
        args: Value,
    },
    #[serde(rename = "review_tests")]
    ReviewTests {
        #[serde(default)]
        files: FileSet,
        #[serde(default, rename = "reviewResult")]
        review_result: Option<crate::builtin::test_review::AdvancedSessionReviewResult>,
        args: Value,
    },
    #[serde(rename = "run_e2e_tests")]
    RunE2ETests {
        #[serde(default)]
        files: FileSet,
        #[serde(default, rename = "e2eResult")]
        e2e_result: Option<crate::builtin::e2e::E2EResult>,
        args: Value,
    },
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TaskInfoDataFixture {
    #[serde(rename = "taskId")]
    pub task_id: String,
    pub description: String,
    pub status: String,
    #[serde(rename = "startedAt")]
    pub started_at: u64,
    #[serde(rename = "endedAt")]
    pub ended_at: Option<u64>,
    #[serde(rename = "stopReason")]
    pub stop_reason: Option<String>,
    #[serde(rename = "terminalNotificationSuppressed")]
    pub terminal_notification_suppressed: Option<bool>,
    #[serde(rename = "outputSnapshot")]
    pub output_snapshot: Option<TaskOutputSnapshotFixture>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TaskOutputSnapshotFixture {
    #[serde(rename = "outputPath")]
    pub output_path: Option<String>,
    #[serde(rename = "outputSizeBytes")]
    pub output_size_bytes: u64,
    #[serde(rename = "previewBytes")]
    pub preview_bytes: usize,
    pub truncated: bool,
    #[serde(rename = "fullOutputAvailable")]
    pub full_output_available: bool,
    pub preview: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CronTaskFixture {
    pub id: Option<String>,
    pub cron: String,
    pub prompt: String,
    pub recurring: bool,
    #[serde(rename = "createdAt")]
    pub created_at: Option<u64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SkillFixture {
    pub name: String,
    #[serde(rename = "skill_type")]
    pub skill_type: Option<String>,
    #[serde(rename = "disable_model_invocation")]
    pub disable_model_invocation: Option<bool>,
    #[serde(rename = "hidden_in_modes")]
    pub hidden_in_modes: Option<Vec<String>>,
    pub content: String,
    pub path: String,
    pub source: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalFixture {
    pub goal_id: String,
    pub objective: String,
    #[serde(default)]
    pub completion_criterion: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub started_by: String,
    #[serde(default)]
    pub updated_by: String,
    #[serde(default)]
    pub turns_used: u64,
    #[serde(default)]
    pub tokens_used: u64,
    #[serde(default)]
    pub wall_clock_ms: u64,
    #[serde(default)]
    pub terminal_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TodoFixtureItem {
    pub title: String,
    pub status: String,
}

fn goal_fixture_to_snapshot(g: &GoalFixture) -> crate::builtin::goal::GoalSnapshot {
    use crate::builtin::goal::{GoalActor, GoalBudgetReport, GoalSnapshot, GoalStatus};
    GoalSnapshot {
        goal_id: g.goal_id.clone(),
        objective: g.objective.clone(),
        completion_criterion: g.completion_criterion.clone(),
        status: match g.status.as_str() {
            "active" => GoalStatus::Active,
            "paused" => GoalStatus::Paused,
            "blocked" => GoalStatus::Blocked,
            _ => GoalStatus::Active,
        },
        created_at: g.created_at.clone(),
        updated_at: g.updated_at.clone(),
        started_by: match g.started_by.as_str() {
            "model" => GoalActor::Model,
            "runtime" => GoalActor::Runtime,
            "system" => GoalActor::System,
            _ => GoalActor::User,
        },
        updated_by: match g.updated_by.as_str() {
            "model" => GoalActor::Model,
            "runtime" => GoalActor::Runtime,
            "system" => GoalActor::System,
            _ => GoalActor::User,
        },
        turns_used: g.turns_used,
        tokens_used: g.tokens_used,
        wall_clock_ms: g.wall_clock_ms,
        budget: GoalBudgetReport {
            token_budget: None,
            turn_budget: None,
            wall_clock_budget_ms: None,
            remaining_tokens: None,
            remaining_turns: None,
            remaining_wall_clock_ms: None,
            token_budget_reached: false,
            turn_budget_reached: false,
            wall_clock_budget_reached: false,
            over_budget: false,
        },
        terminal_reason: g.terminal_reason.clone(),
    }
}

fn run_tool_exec(exec: crate::builtin::ToolExecution) -> CaseResult {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
        turn_id: "0".into(),
        tool_call_id: "call_golden".into(),
        signal: crate::builtin::AbortSignal::new(),
        metadata: None,
    }));
    CaseResult::ok(result_to_golden(&result))
}

fn default_true() -> Option<bool> {
    Some(true)
}
fn default_20() -> Option<usize> {
    Some(20)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl CaseResult {
    pub fn ok(value: Value) -> Self {
        Self {
            result: Some(value),
            error: None,
        }
    }
    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            result: None,
            error: Some(msg.into()),
        }
    }
}

fn kind_to_str(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Text => "text",
        FileKind::Image => "image",
        FileKind::Video => "video",
        FileKind::Unknown => "unknown",
    }
}

fn default_include_dirs() -> bool {
    true
}

fn file_type_to_value(ft: crate::file_type::FileType) -> Value {
    serde_json::json!({ "kind": kind_to_str(ft.kind), "mimeType": ft.mime_type })
}

fn run_case_sync(case: &Case, temp_dir: Option<&std::path::Path>) -> CaseResult {
    match &case.op {
        Op::CanonicalizePath {
            path,
            cwd,
            path_class,
        } => match canonicalize_path(path, cwd, *path_class) {
            Ok(v) => CaseResult::ok(Value::String(v)),
            Err(e) => CaseResult::err(format!("{:?}", e.code)),
        },
        Op::IsWithinDirectory {
            candidate,
            base,
            path_class,
        } => CaseResult::ok(Value::Bool(
            crate::policies::path_access::is_within_directory(candidate, base, *path_class),
        )),
        Op::NormalizeUserPath { path, path_class } => {
            CaseResult::ok(Value::String(normalize_user_path(path, *path_class)))
        }
        Op::ResolvePathAccess {
            path,
            cwd,
            workspace_dir,
            additional_dirs,
            operation,
            path_class,
            home_dir,
        } => {
            let config = WorkspaceConfig {
                workspace_dir: workspace_dir.clone(),
                additional_dirs: additional_dirs.clone(),
            };
            let policy = WorkspaceAccessPolicy {
                guard_mode: WorkspaceGuardMode::AbsoluteOutsideAllowed,
                check_sensitive: true,
            };
            match resolve_path_access(
                path,
                cwd,
                &config,
                crate::policies::path_access::ResolvePathAccessOptions {
                    operation: *operation,
                    policy: Some(policy),
                    path_class: Some(*path_class),
                    home_dir: home_dir.clone(),
                },
            ) {
                Ok(a) => CaseResult::ok(serde_json::to_value(a).unwrap()),
                Err(e) => CaseResult::err(format!("{:?}", e.code)),
            }
        }
        Op::AssertPathAllowed {
            path,
            cwd,
            workspace_dir,
            additional_dirs,
            mode,
            path_class,
        } => {
            let config = WorkspaceConfig {
                workspace_dir: workspace_dir.clone(),
                additional_dirs: additional_dirs.clone(),
            };
            match assert_path_allowed(
                path,
                cwd,
                &config,
                crate::policies::path_access::AssertPathOptions {
                    mode: *mode,
                    check_sensitive: Some(true),
                    path_class: Some(*path_class),
                },
            ) {
                Ok(v) => CaseResult::ok(Value::String(v)),
                Err(e) => CaseResult::err(format!("{:?}", e.code)),
            }
        }
        Op::IsSensitiveFile { path } => CaseResult::ok(Value::Bool(is_sensitive_file(path))),
        Op::LiteralRulePattern { tool_name, subject } => {
            CaseResult::ok(Value::String(literal_rule_pattern(tool_name, subject)))
        }
        Op::EscapeRuleSubjectLiteral { subject } => {
            CaseResult::ok(Value::String(escape_rule_subject_literal(subject)))
        }
        Op::MatchesGlobRuleSubject { rule_args, subject } => {
            CaseResult::ok(Value::Bool(matches_glob_rule_subject(rule_args, subject)))
        }
        Op::MatchesPathRuleSubject {
            rule_args,
            subject,
            cwd,
            path_class,
        } => {
            let opts = PermissionPathMatchOptions {
                cwd: cwd.clone(),
                path_class: Some(*path_class),
                home_dir: None,
                case_insensitive_paths: Some(true),
            };
            CaseResult::ok(Value::Bool(matches_path_rule_subject(
                rule_args,
                subject,
                Some(&opts),
            )))
        }
        Op::ValidateArgs { schema, args } => match compile_tool_args_validator(schema) {
            Ok(v) => match validate_tool_args(&v, args) {
                None => CaseResult::ok(Value::Null),
                Some(msg) => CaseResult::err(msg),
            },
            Err(e) => CaseResult::err(e.to_string()),
        },
        Op::AccessConflict { left, right } => {
            let a = ToolAccesses(left.clone());
            let b = ToolAccesses(right.clone());
            CaseResult::ok(Value::Bool(ToolAccesses::conflict(&a, &b)))
        }
        Op::BuildResult {
            writes,
            max_line_length,
            as_error,
        } => {
            let mut builder = ToolResultBuilder::new(Some(*max_line_length));
            for text in writes {
                builder.write(text);
            }
            let result = if *as_error {
                builder.error("it broke".into())
            } else {
                builder.ok(Some("ok".into()))
            };
            CaseResult::ok(serde_json::to_value(result).unwrap())
        }
        Op::SniffMediaFromMagic { header } => match sniff_media_from_magic(header) {
            Some(ft) => CaseResult::ok(file_type_to_value(ft)),
            None => CaseResult::err(String::from("no media magic")),
        },
        Op::DetectFileType { path, header } => {
            let h = header.as_deref();
            CaseResult::ok(file_type_to_value(detect_file_type(path, h)))
        }
        Op::SniffImageDimensions { header } => match sniff_image_dimensions(header) {
            Some(d) => CaseResult::ok(serde_json::to_value(d).unwrap()),
            None => CaseResult::err(String::from("no dimensions")),
        },
        Op::DetectTarget { arch, platform } => {
            match crate::rg_locator::detect_target_for(arch, platform) {
                Some(target) => CaseResult::ok(Value::String(target)),
                None => CaseResult::err(String::from("unsupported platform")),
            }
        }
        Op::FindExistingRg {
            path_env,
            share_dir,
            files,
        } => {
            let dir = temp_dir.expect("find_existing_rg requires tempdir");
            let rg_name = if cfg!(windows) { "rg.exe" } else { "rg" };
            let mut paths: Vec<PathBuf> = path_env
                .iter()
                .map(|p| dir.join(p.trim_start_matches('/')))
                .collect();
            let share = dir.join(share_dir.trim_start_matches('/'));
            std::fs::create_dir_all(&share.join("bin")).unwrap();

            // Create share/bin/rg if fixtures specify it
            if let Some(data) = files.get(&format!("{}/bin/{}", share_dir, rg_name)) {
                let target = share.join("bin").join(rg_name);
                std::fs::write(&target, data).unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))
                        .unwrap();
                }
            }
            // Create PATH entries and make them executable
            for (rel, data) in files {
                if rel.starts_with(share_dir) {
                    continue;
                }
                let target = dir.join(rel.trim_start_matches('/'));
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(&target, data).unwrap();
                if rel.ends_with(rg_name) {
                    paths.push(target.parent().unwrap().to_path_buf());
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))
                            .unwrap();
                    }
                }
            }
            let path_var = paths
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(if cfg!(windows) { ";" } else { ":" });
            let old = std::env::var("PATH").ok();
            std::env::set_var("PATH", &path_var);
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(find_existing_rg(&share, Some(&path_var)));
            if let Some(old) = old {
                std::env::set_var("PATH", old);
            } else {
                std::env::remove_var("PATH");
            }
            match result {
                Some(RgResolution { path, source }) => {
                    let source_str = match source {
                        RgResolutionSource::SystemPath => "system-path",
                        RgResolutionSource::Vendor => "vendor",
                        RgResolutionSource::ShareBinCached => "share-bin-cached",
                        RgResolutionSource::ShareBinDownloaded => "share-bin-downloaded",
                    };
                    CaseResult::ok(serde_json::json!({
                        "path": path.to_string_lossy().to_string(),
                        "source": source_str,
                    }))
                }
                None => CaseResult::err(String::from("rg not found")),
            }
        }
        Op::ListDirectory { path, files } => {
            use kaos_rs::kaos::Kaos;
            let dir = temp_dir.expect("list_directory requires tempdir");
            for (rel, data) in files {
                let target = dir.join(rel.trim_start_matches('/'));
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(&target, data).unwrap();
            }
            let rt = tokio::runtime::Runtime::new().unwrap();
            let env = kaos_rs::environment::Environment {
                os_kind: "macOS".to_string(),
                os_arch: "arm64".to_string(),
                os_version: "23.0.0".to_string(),
                shell_name: "bash".to_string(),
                shell_path: "/bin/bash".to_string(),
            };
            let kaos = Kaos::new(env, dir);
            let listing = rt
                .block_on(crate::list_directory::list_directory(&kaos, Some(path)))
                .unwrap();
            CaseResult::ok(Value::String(listing))
        }
        // ── core tool ops ──
        Op::ReadText {
            path,
            line_offset,
            n_lines,
            files: _,
        } => {
            use kaos_rs::kaos::Kaos;
            let dir = temp_dir.expect("read_text requires tempdir");
            let rt = tokio::runtime::Runtime::new().unwrap();
            let env = dummy_env();
            let kaos = Kaos::new(env, dir);
            let ws = WorkspaceConfig::new(dir.to_string_lossy().to_string());
            let tool = crate::builtin::read::ReadTool::new(kaos, ws);
            let mut args = serde_json::json!({ "path": path });
            if let Some(lo) = line_offset {
                args["line_offset"] = Value::from(*lo);
            }
            if let Some(n) = n_lines {
                args["n_lines"] = Value::from(*n);
            }
            match tool.resolve_execution(args) {
                Ok(exec) => {
                    let result = rt.block_on((exec.execute)(ExecutableToolContext {
                        turn_id: "1".into(),
                        tool_call_id: "call_1".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    }));
                    CaseResult::ok(result_to_value(result))
                }
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::WriteFile {
            path,
            content,
            mode,
            files: _,
        } => {
            use kaos_rs::kaos::Kaos;
            let dir = temp_dir.expect("write_file requires tempdir");
            let rt = tokio::runtime::Runtime::new().unwrap();
            let env = dummy_env();
            let kaos = Kaos::new(env, dir);
            let ws = WorkspaceConfig::new(dir.to_string_lossy().to_string());
            let tool = crate::builtin::write::WriteTool::new(kaos, ws);
            let mut args = serde_json::json!({ "path": path, "content": content });
            if let Some(m) = mode {
                args["mode"] = Value::String(m.clone());
            }
            match tool.resolve_execution(args) {
                Ok(exec) => {
                    let result = rt.block_on((exec.execute)(ExecutableToolContext {
                        turn_id: "1".into(),
                        tool_call_id: "call_1".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    }));
                    CaseResult::ok(result_to_value(result))
                }
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::EditFile {
            path,
            old_string,
            new_string,
            replace_all,
            files: _,
        } => {
            use kaos_rs::kaos::Kaos;
            let dir = temp_dir.expect("edit_file requires tempdir");
            let rt = tokio::runtime::Runtime::new().unwrap();
            let env = dummy_env();
            let kaos = Kaos::new(env, dir);
            let ws = WorkspaceConfig::new(dir.to_string_lossy().to_string());
            let tool = crate::builtin::edit::EditTool::new(kaos, ws);
            let mut args = serde_json::json!({
                "path": path,
                "old_string": old_string,
                "new_string": new_string,
            });
            if *replace_all {
                args["replace_all"] = Value::Bool(true);
            }
            match tool.resolve_execution(args) {
                Ok(exec) => {
                    let result = rt.block_on((exec.execute)(ExecutableToolContext {
                        turn_id: "1".into(),
                        tool_call_id: "call_1".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    }));
                    CaseResult::ok(result_to_value(result))
                }
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::GlobSearch {
            pattern,
            path: glob_path,
            include_dirs,
            files: _,
        } => {
            use kaos_rs::kaos::Kaos;
            let dir = temp_dir.expect("glob_search requires tempdir");
            let rt = tokio::runtime::Runtime::new().unwrap();
            let env = dummy_env();
            let kaos = Kaos::new(env, dir);
            let ws = WorkspaceConfig::new(dir.to_string_lossy().to_string());
            let tool = crate::builtin::glob::GlobTool::new(kaos, ws);
            let mut args = serde_json::json!({
                "pattern": pattern,
                "include_dirs": include_dirs,
            });
            if let Some(p) = glob_path {
                args["path"] = Value::String(p.clone());
            }
            match tool.resolve_execution(args) {
                Ok(exec) => {
                    let result = rt.block_on((exec.execute)(ExecutableToolContext {
                        turn_id: "1".into(),
                        tool_call_id: "call_1".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    }));
                    // Sort output lines by name for deterministic parity with TS
                    let mut value = result_to_value(result);
                    if let Some(output) = value.get("output").and_then(Value::as_str) {
                        let mut lines: Vec<&str> = output.lines().collect();
                        if !lines.is_empty()
                            && !lines[0].starts_with('[')
                            && lines[0] != "No matches found"
                        {
                            lines.sort();
                            value["output"] = Value::String(lines.join("\n"));
                        }
                    }
                    CaseResult::ok(value)
                }
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::GrepSearch {
            pattern,
            path: grep_path,
            output_mode,
            files: _,
        } => {
            use kaos_rs::kaos::Kaos;
            let dir = temp_dir.expect("grep_search requires tempdir");
            let rt = tokio::runtime::Runtime::new().unwrap();
            let env = dummy_env();
            let kaos = Kaos::new(env, dir);
            let ws = WorkspaceConfig::new(dir.to_string_lossy().to_string());
            let tool = crate::builtin::grep::GrepTool::new(kaos, ws);
            let mut args = serde_json::json!({ "pattern": pattern });
            if let Some(p) = grep_path {
                args["path"] = Value::String(p.clone());
            }
            if let Some(mode) = output_mode {
                args["output_mode"] = Value::String(mode.clone());
            }
            match tool.resolve_execution(args) {
                Ok(exec) => {
                    let result = rt.block_on((exec.execute)(ExecutableToolContext {
                        turn_id: "1".into(),
                        tool_call_id: "call_1".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    }));
                    CaseResult::ok(result_to_value(result))
                }
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::ReadMedia { path, files: _ } => {
            use kaos_rs::kaos::Kaos;
            let dir = temp_dir.expect("read_media requires tempdir");
            let rt = tokio::runtime::Runtime::new().unwrap();
            let env = dummy_env();
            let kaos = Kaos::new(env, dir);
            let ws = WorkspaceConfig::new(dir.to_string_lossy().to_string());
            let tool = crate::builtin::media::ReadMediaFileTool::new(kaos, ws);
            let args = serde_json::json!({ "file_path": path });
            match tool.resolve_execution(args) {
                Ok(exec) => {
                    let result = rt.block_on((exec.execute)(ExecutableToolContext {
                        turn_id: "1".into(),
                        tool_call_id: "call_1".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    }));
                    CaseResult::ok(result_to_value(result))
                }
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::BashExec {
            command,
            timeout,
            files: _,
        } => {
            use kaos_rs::kaos::Kaos;
            let dir = temp_dir.expect("bash_exec requires tempdir");
            let rt = tokio::runtime::Runtime::new().unwrap();
            let env = dummy_env();
            let kaos = Kaos::new(env, dir);
            let ws = WorkspaceConfig::new(dir.to_string_lossy().to_string());
            let tool = crate::builtin::bash::BashTool::new(kaos, ws);
            let mut args = serde_json::json!({ "command": command });
            if let Some(t) = timeout {
                args["timeout"] = Value::from(*t);
            }
            match tool.resolve_execution(args) {
                Ok(exec) => {
                    let result = rt.block_on((exec.execute)(ExecutableToolContext {
                        turn_id: "1".into(),
                        tool_call_id: "call_1".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    }));
                    CaseResult::ok(result_to_value(result))
                }
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        // ── background & cron tool ops ──
        Op::TaskList {
            active_only,
            limit,
            tasks,
        } => {
            use std::sync::Arc;
            let mgr = Arc::new(background::MockBackgroundManager::new());
            for t in tasks {
                mgr.add_task(BackgroundTaskInfoData {
                    task_id: t.task_id.clone(),
                    description: t.description.clone(),
                    status: parse_status(&t.status),
                    started_at: t.started_at,
                    ended_at: t.ended_at,
                    stop_reason: t.stop_reason.clone(),
                    terminal_notification_suppressed: t
                        .terminal_notification_suppressed
                        .unwrap_or(false),
                });
            }
            let tool = background::task_list::TaskListTool::new(mgr);
            match tool.resolve_execution(serde_json::json!({
                "active_only": active_only.unwrap_or(true),
                "limit": limit.unwrap_or(20),
            })) {
                Ok(exec) => {
                    let ctx = ExecutableToolContext {
                        turn_id: "1".into(),
                        tool_call_id: "call_1".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    };
                    let result = tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on((exec.execute)(ctx));
                    CaseResult::ok(result_to_golden(&result))
                }
                Err(e) => CaseResult::err(format!("{:?}", e)),
            }
        }
        Op::TaskOutput {
            task_id,
            block,
            timeout,
            tasks,
        } => {
            use std::sync::Arc;
            let mgr = Arc::new(background::MockBackgroundManager::new());
            for t in tasks {
                mgr.add_task(BackgroundTaskInfoData {
                    task_id: t.task_id.clone(),
                    description: t.description.clone(),
                    status: parse_status(&t.status),
                    started_at: t.started_at,
                    ended_at: t.ended_at,
                    stop_reason: t.stop_reason.clone(),
                    terminal_notification_suppressed: t
                        .terminal_notification_suppressed
                        .unwrap_or(false),
                });
                if let Some(ref snap) = t.output_snapshot {
                    mgr.set_output_snapshot(
                        &t.task_id,
                        background::BackgroundTaskOutputSnapshot {
                            output_path: snap.output_path.clone(),
                            output_size_bytes: snap.output_size_bytes,
                            preview_bytes: snap.preview_bytes,
                            truncated: snap.truncated,
                            full_output_available: snap.full_output_available,
                            preview: snap.preview.clone(),
                        },
                    );
                }
            }
            let tool = background::task_output::TaskOutputTool::new(mgr);
            match tool.resolve_execution(serde_json::json!({
                "task_id": task_id,
                "block": block.unwrap_or(false),
                "timeout": timeout.unwrap_or(30),
            })) {
                Ok(exec) => {
                    let ctx = ExecutableToolContext {
                        turn_id: "1".into(),
                        tool_call_id: "call_1".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    };
                    let result = tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on((exec.execute)(ctx));
                    CaseResult::ok(result_to_golden(&result))
                }
                Err(e) => CaseResult::err(format!("{:?}", e)),
            }
        }
        Op::TaskStop {
            task_id,
            reason,
            tasks,
        } => {
            use std::sync::Arc;
            let mgr = Arc::new(background::MockBackgroundManager::new());
            for t in tasks {
                mgr.add_task(BackgroundTaskInfoData {
                    task_id: t.task_id.clone(),
                    description: t.description.clone(),
                    status: parse_status(&t.status),
                    started_at: t.started_at,
                    ended_at: t.ended_at,
                    stop_reason: t.stop_reason.clone(),
                    terminal_notification_suppressed: t
                        .terminal_notification_suppressed
                        .unwrap_or(false),
                });
            }
            let tool = background::task_stop::TaskStopTool::new(mgr);
            match tool.resolve_execution(serde_json::json!({
                "task_id": task_id,
                "reason": reason,
            })) {
                Ok(exec) => {
                    let ctx = ExecutableToolContext {
                        turn_id: "1".into(),
                        tool_call_id: "call_1".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    };
                    let result = tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on((exec.execute)(ctx));
                    CaseResult::ok(result_to_golden(&result))
                }
                Err(e) => CaseResult::err(format!("{:?}", e)),
            }
        }
        Op::CronCreate {
            cron,
            prompt,
            recurring,
            existing_tasks,
        } => {
            use std::sync::Arc;
            let now = 1700000000000u64;
            let mgr = Arc::new(cron::MockCronManager::new(Some(now)));
            for t in existing_tasks {
                mgr.add_task(cron::SessionCronTaskInit {
                    cron: t.cron.clone(),
                    prompt: t.prompt.clone(),
                    recurring: t.recurring,
                });
            }
            let tool = cron::cron_create::CronCreateTool::new(mgr);
            match tool.resolve_execution(serde_json::json!({
                "cron": cron,
                "prompt": prompt,
                "recurring": recurring.unwrap_or(true),
            })) {
                Ok(exec) => {
                    let ctx = ExecutableToolContext {
                        turn_id: "1".into(),
                        tool_call_id: "call_1".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    };
                    let result = tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on((exec.execute)(ctx));
                    CaseResult::ok(result_to_golden(&result))
                }
                Err(e) => CaseResult::err(format!("{:?}", e)),
            }
        }
        Op::CronList { tasks } => {
            use std::sync::Arc;
            let now = 1700000000000u64;
            let mgr = Arc::new(cron::MockCronManager::new(Some(now)));
            for t in tasks {
                mgr.add_task(cron::SessionCronTaskInit {
                    cron: t.cron.clone(),
                    prompt: t.prompt.clone(),
                    recurring: t.recurring,
                });
            }
            let tool = cron::cron_list::CronListTool::new(mgr);
            match tool.resolve_execution(serde_json::json!({})) {
                Ok(exec) => {
                    let ctx = ExecutableToolContext {
                        turn_id: "1".into(),
                        tool_call_id: "call_1".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    };
                    let result = tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on((exec.execute)(ctx));
                    CaseResult::ok(result_to_golden(&result))
                }
                Err(e) => CaseResult::err(format!("{:?}", e)),
            }
        }
        Op::CronDelete { id, tasks } => {
            use std::sync::Arc;
            let now = 1700000000000u64;
            let mgr = Arc::new(cron::MockCronManager::new(Some(now)));
            for t in tasks {
                mgr.add_task(cron::SessionCronTaskInit {
                    cron: t.cron.clone(),
                    prompt: t.prompt.clone(),
                    recurring: t.recurring,
                });
            }
            let tool = cron::cron_delete::CronDeleteTool::new(mgr);
            match tool.resolve_execution(serde_json::json!({ "id": id })) {
                Ok(exec) => {
                    let ctx = ExecutableToolContext {
                        turn_id: "1".into(),
                        tool_call_id: "call_1".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    };
                    let result = tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on((exec.execute)(ctx));
                    CaseResult::ok(result_to_golden(&result))
                }
                Err(e) => CaseResult::err(format!("{:?}", e)),
            }
        }
        Op::SkillCall {
            name,
            args,
            query_depth,
            session_mode,
            skills,
        } => {
            use std::sync::Arc;
            let provider = Arc::new(FixtureSkillProvider::new(skills.clone()));
            let mut options = SkillToolOptions::default();
            if let Some(d) = query_depth {
                options.query_depth = Some(*d);
            }
            if let Some(m) = session_mode {
                options.session_mode = Some(m.clone());
            }
            let tool = SkillTool::new(provider, options);
            let mut tool_args = serde_json::json!({ "skill": name });
            if let Some(a) = args {
                tool_args["args"] = serde_json::Value::String(a.clone());
            }
            match tool.resolve_execution(tool_args) {
                Ok(exec) => {
                    let ctx = ExecutableToolContext {
                        turn_id: "1".into(),
                        tool_call_id: "call_1".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    };
                    let result = tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on((exec.execute)(ctx));
                    CaseResult::ok(result_to_golden(&result))
                }
                Err(e) => CaseResult::err(format!("{:?}", e)),
            }
        }
        Op::AskUser {
            questions,
            background,
            provider_response,
            answers,
            method,
            registrar_response,
            task_id,
        } => {
            use crate::builtin::collaboration::{
                AskUserQuestionOptions, AskUserQuestionTool, QuestionAnswers, QuestionError,
                QuestionResult,
            };

            let result = match provider_response.as_deref() {
                Some("unsupported") => Err(QuestionError::NotImplemented),
                Some("dismissed") => Ok(QuestionResult::Dismissed),
                _ => {
                    let answers = answers.clone().unwrap_or_default();
                    Ok(QuestionResult::Answers(QuestionAnswers {
                        answers,
                        method: method.clone(),
                    }))
                }
            };
            let provider = std::sync::Arc::new(FixtureQuestionProvider { response: result });
            let registrar = std::sync::Arc::new(FixtureBackgroundRegistrar {
                next_id: task_id
                    .clone()
                    .unwrap_or_else(|| "question-00000001".into()),
                fail: registrar_response.as_deref() == Some("fail"),
            });
            let background_enabled = *background == Some(true);
            let tool = AskUserQuestionTool::new(
                provider,
                registrar,
                AskUserQuestionOptions {
                    background_ask_enabled: background_enabled,
                },
            );
            match tool.resolve_execution(serde_json::json!({
                "questions": questions,
                "background": background.unwrap_or(false),
            })) {
                Ok(exec) => {
                    let ctx = ExecutableToolContext {
                        turn_id: "7".into(),
                        tool_call_id: "call_q".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    };
                    let result = tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on((exec.execute)(ctx));
                    CaseResult::ok(result_to_golden(&result))
                }
                Err(e) => CaseResult::err(format!("{:?}", e)),
            }
        }
        Op::AgentCall {
            prompt,
            description,
            subagent_type,
            resume,
            run_in_background,
            timeout,
            host_response,
            result,
            error,
            agent_id,
            profile_name,
            registrar_response,
            task_id,
        } => {
            use crate::builtin::collaboration::{AgentTool, AgentToolOptions};
            use std::sync::Arc;
            let host = Arc::new(FixtureSubagentHost {
                behavior: host_response.clone().unwrap_or_else(|| "success".into()),
                result: result.clone(),
                error: error.clone(),
                agent_id: agent_id.clone(),
            });
            let registrar: Option<Arc<dyn crate::builtin::collaboration::BackgroundRegistrar>> =
                run_in_background.unwrap_or(false).then(|| {
                    Arc::new(FixtureAgentBackgroundRegistrar {
                        next_id: task_id.clone().unwrap_or_else(|| "agent-00000001".into()),
                        fail: registrar_response.as_deref() == Some("fail"),
                    })
                        as Arc<dyn crate::builtin::collaboration::BackgroundRegistrar>
                });
            let tool = AgentTool::new(host, registrar, AgentToolOptions::default());
            let mut args = serde_json::json!({
                "prompt": prompt,
                "description": description,
                "run_in_background": run_in_background.unwrap_or(false),
            });
            if let Some(st) = subagent_type {
                args["subagent_type"] = Value::String(st.clone());
            }
            if let Some(r) = resume {
                args["resume"] = Value::String(r.clone());
            }
            if let Some(t) = timeout {
                args["timeout"] = Value::from(*t);
            }
            match tool.resolve_execution(args) {
                Ok(exec) => {
                    let ctx = ExecutableToolContext {
                        turn_id: "1".into(),
                        tool_call_id: "call_a".into(),
                        signal: crate::builtin::AbortSignal::new(),
                        metadata: None,
                    };
                    let result = tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on((exec.execute)(ctx));
                    CaseResult::ok(result_to_golden(&result))
                }
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        // ── goal & state tool ops ──
        Op::CreateGoal { store_goal, args } => {
            use crate::builtin::goal::MockGoalStore;
            use std::sync::Arc;
            let snapshot = store_goal.as_ref().map(|g| goal_fixture_to_snapshot(g));
            let mock = Arc::new(MockGoalStore::new(snapshot));
            let tool = crate::builtin::goal::create_goal::CreateGoalTool::new(mock);
            match tool.resolve_execution(args.clone()) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::GetGoal { store_goal } => {
            use crate::builtin::goal::MockGoalStore;
            use std::sync::Arc;
            let snapshot = store_goal.as_ref().map(|g| goal_fixture_to_snapshot(g));
            let mock = Arc::new(MockGoalStore::new(snapshot));
            let tool = crate::builtin::goal::get_goal::GetGoalTool::new(mock);
            match tool.resolve_execution(serde_json::json!({})) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::SetGoalBudget { store_goal, args } => {
            use crate::builtin::goal::MockGoalStore;
            use std::sync::Arc;
            let snapshot = store_goal.as_ref().map(|g| goal_fixture_to_snapshot(g));
            let mock = Arc::new(MockGoalStore::new(snapshot));
            let tool = crate::builtin::goal::set_goal_budget::SetGoalBudgetTool::new(mock);
            match tool.resolve_execution(args.clone()) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::UpdateGoal { store_goal, args } => {
            use crate::builtin::goal::MockGoalStore;
            use std::sync::Arc;
            let snapshot = store_goal.as_ref().map(|g| goal_fixture_to_snapshot(g));
            let mock = Arc::new(MockGoalStore::new(snapshot));
            let tool = crate::builtin::goal::update_goal::UpdateGoalTool::new(mock, None);
            match tool.resolve_execution(args.clone()) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::TodoList { store_todos, args } => {
            use crate::store::{MockToolStore, ToolStore};
            use std::sync::Arc;
            let mock = Arc::new(MockToolStore::new());
            if !store_todos.is_empty() {
                let items: Vec<serde_json::Value> = store_todos
                    .iter()
                    .map(|t| serde_json::json!({"title": t.title, "status": t.status}))
                    .collect();
                mock.set("todo", serde_json::to_value(&items).unwrap_or_default());
            }
            let tool = crate::builtin::todo_list::TodoListTool::new(mock);
            match tool.resolve_execution(args.clone()) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::Checkpoint { enabled: _, reason } => {
            use crate::builtin::checkpoint::MockCheckpointCoordinator;
            use std::sync::Arc;
            let coord = Arc::new(MockCheckpointCoordinator::new());
            let tool = crate::builtin::checkpoint::CheckpointTool::new(coord);
            let args = if let Some(r) = reason {
                serde_json::json!({"reason": r})
            } else {
                serde_json::json!({})
            };
            match tool.resolve_execution(args) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::HarvestOdyMarkers { files, args } => {
            let dir = temp_dir.expect("harvest_ody_markers requires tempdir");
            setup_files(dir, files).unwrap();
            let workspace = WorkspaceConfig::new(dir.to_string_lossy().to_string());
            let kaos = Kaos::new(dummy_env(), dir);
            let grep = crate::builtin::grep::GrepTool::new(kaos.clone(), workspace.clone());
            let tool = crate::builtin::quality::HarvestOdyMarkersTool::new(
                kaos,
                workspace,
                grep,
                Arc::new(crate::builtin::quality::NoopTelemetryClient),
            );
            match tool.resolve_execution(args.clone()) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::SaveIdeaReport {
            files,
            existing_reports: _,
            args,
        } => {
            let dir = temp_dir.expect("save_idea_report requires tempdir");
            setup_files(dir, files).unwrap();
            let workspace = WorkspaceConfig::new(dir.to_string_lossy().to_string());
            let kaos = Kaos::new(dummy_env(), dir);
            let now: chrono::DateTime<chrono::Utc> = "2026-01-02T00:00:00Z".parse().unwrap();
            let ctx = crate::builtin::idea::MockIdeaReportContext::new(true, now);
            let tool = crate::builtin::idea::SaveIdeaReportTool::new(kaos, workspace, ctx);
            match tool.resolve_execution(args.clone()) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::ShowDesignMockup { files, args } => {
            let dir = temp_dir.expect("show_design_mockup requires tempdir");
            setup_files(dir, files).unwrap();
            let kaos = Kaos::new(dummy_env(), dir);
            let design_path = format!("{}/design.md", dir.to_string_lossy());
            let host: Arc<dyn crate::builtin::visual::DesignMockupHost> =
                Arc::new(crate::builtin::visual::MockDesignMockupHost::new(
                    true,
                    Some(design_path),
                    Ok(crate::builtin::visual::OpenExternalResult {
                        opened: true,
                        error: None,
                    }),
                ));
            let tool = crate::builtin::visual::ShowDesignMockupTool::new(kaos, host);
            match tool.resolve_execution(args.clone()) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::ReviewTests {
            files,
            review_result,
            args,
        } => {
            let dir = temp_dir.expect("review_tests requires tempdir");
            setup_files(dir, files).unwrap();
            if let Err(e) = std::process::Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(dir)
                .status()
            {
                return CaseResult::err(format!("git init failed: {}", e));
            }
            let workspace = WorkspaceConfig::new(dir.to_string_lossy().to_string());
            let kaos = Kaos::new(dummy_env(), dir);
            let result = review_result.clone().unwrap_or_else(|| {
                crate::builtin::test_review::AdvancedSessionReviewResult {
                    audit_level: crate::builtin::test_review::AuditLevel::Standard,
                    findings: vec![],
                    mutation_probes: None,
                    ok: true,
                    note: None,
                }
            });
            let reviewer = Arc::new(GoldenMockReviewer { result });
            let tool = crate::builtin::test_review::ReviewTestsTool::new(kaos, reviewer);
            match tool.resolve_execution(args.clone()) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
        Op::RunE2ETests {
            files,
            e2e_result,
            args,
        } => {
            let dir = temp_dir.expect("run_e2e_tests requires tempdir");
            setup_files(dir, files).unwrap();
            if let Err(e) = std::process::Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(dir)
                .status()
            {
                return CaseResult::err(format!("git init failed: {}", e));
            }
            let kaos = Kaos::new(dummy_env(), dir);
            let result = e2e_result
                .clone()
                .unwrap_or_else(|| crate::builtin::e2e::E2EResult {
                    passed: 0,
                    failed: 0,
                    skipped: 0,
                    summary: "ok".to_string(),
                    test_files: vec![],
                });
            let runner = Arc::new(GoldenMockRunner { result });
            let config = crate::builtin::e2e::E2EConfig::default();
            let tool = crate::builtin::e2e::RunE2ETestsTool::new(kaos, config, runner);
            match tool.resolve_execution(args.clone()) {
                Ok(exec) => run_tool_exec(exec),
                Err(e) => CaseResult::err(e.to_string()),
            }
        }
    }
}

fn needs_tempdir(op: &Op) -> bool {
    matches!(
        op,
        Op::FindExistingRg { .. }
            | Op::ListDirectory { .. }
            | Op::ReadText { .. }
            | Op::WriteFile { .. }
            | Op::EditFile { .. }
            | Op::GlobSearch { .. }
            | Op::GrepSearch { .. }
            | Op::ReadMedia { .. }
            | Op::BashExec { .. }
            | Op::HarvestOdyMarkers { .. }
            | Op::SaveIdeaReport { .. }
            | Op::ShowDesignMockup { .. }
            | Op::ReviewTests { .. }
            | Op::RunE2ETests { .. }
    )
}

fn files_for_op(op: &Op) -> FileSet {
    match op {
        Op::FindExistingRg { files, .. } => files.clone(),
        Op::ListDirectory { files, .. } => files.clone(),
        Op::ReadText { files, .. } => files.clone(),
        Op::WriteFile { files, .. } => files.clone(),
        Op::EditFile { files, .. } => files.clone(),
        Op::GlobSearch { files, .. } => files.clone(),
        Op::GrepSearch { files, .. } => files.clone(),
        Op::ReadMedia { files, .. } => files.clone(),
        Op::BashExec { files, .. } => files.clone(),
        Op::HarvestOdyMarkers { files, .. } => files.clone(),
        Op::SaveIdeaReport { files, .. } => files.clone(),
        Op::ShowDesignMockup { files, .. } => files.clone(),
        Op::ReviewTests { files, .. } => files.clone(),
        Op::RunE2ETests { files, .. } => files.clone(),
        _ => FileSet::new(),
    }
}

fn setup_files(dir: &std::path::Path, files: &FileSet) -> std::io::Result<()> {
    for (rel, data) in files {
        let target = dir.join(rel.trim_start_matches('/'));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, data)?;
    }
    Ok(())
}

fn parse_status(s: &str) -> background::BackgroundTaskStatus {
    match s {
        "running" => background::BackgroundTaskStatus::Running,
        "completed" => background::BackgroundTaskStatus::Completed,
        "failed" => background::BackgroundTaskStatus::Failed,
        "timed_out" => background::BackgroundTaskStatus::TimedOut,
        "killed" => background::BackgroundTaskStatus::Killed,
        "lost" => background::BackgroundTaskStatus::Lost,
        _ => background::BackgroundTaskStatus::Running,
    }
}

struct GoldenMockReviewer {
    result: crate::builtin::test_review::AdvancedSessionReviewResult,
}

#[async_trait::async_trait]
impl crate::builtin::test_review::TestReviewer for GoldenMockReviewer {
    async fn review_tests(
        &self,
        _content: &str,
        _reviewer_alias: &str,
        _signal: &crate::builtin::AbortSignal,
    ) -> Result<
        crate::builtin::test_review::AdvancedSessionReviewResult,
        crate::builtin::test_review::TestReviewError,
    > {
        Ok(self.result.clone())
    }
}

struct GoldenMockRunner {
    result: crate::builtin::e2e::E2EResult,
}

#[async_trait::async_trait]
impl crate::builtin::e2e::E2ETestRunner for GoldenMockRunner {
    async fn detect_generator(
        &self,
        _root: &str,
    ) -> Result<(), crate::builtin::e2e::E2ETestRunnerError> {
        Ok(())
    }

    async fn analyze_impact(
        &self,
        _changed_files: &[String],
        _config: &crate::builtin::e2e::E2EConfig,
        _root: &str,
    ) -> Result<crate::builtin::e2e::E2EImpact, crate::builtin::e2e::E2ETestRunnerError> {
        Ok(crate::builtin::e2e::E2EImpact {
            affected_tools: vec![crate::builtin::e2e::AffectedTool {
                tool_id: "Read".to_string(),
                reason: "golden fixture".to_string(),
            }],
        })
    }

    async fn generate_tests(
        &self,
        _tool: &crate::builtin::e2e::AffectedTool,
        _changed_files: &[String],
        _root: &str,
        _dir: &str,
    ) -> Result<Vec<String>, crate::builtin::e2e::E2ETestRunnerError> {
        Ok(vec!["generated.test.ts".to_string()])
    }

    async fn run_e2e_tests(
        &self,
        _test_files: &[String],
        _root: &str,
        _signal: &crate::builtin::AbortSignal,
    ) -> Result<crate::builtin::e2e::E2EResult, crate::builtin::e2e::E2ETestRunnerError> {
        Ok(self.result.clone())
    }
}

fn result_to_golden(r: &ExecutableToolResult) -> serde_json::Value {
    serde_json::json!({
        "output": r.to_text(),
        "is_error": r.is_error,
        "message": r.message,
    })
}

fn dummy_env() -> kaos_rs::environment::Environment {
    kaos_rs::environment::Environment {
        os_kind: "macOS".to_string(),
        os_arch: "arm64".to_string(),
        os_version: "23.0.0".to_string(),
        shell_name: "bash".to_string(),
        shell_path: "/bin/bash".to_string(),
    }
}

struct FixtureSkillProvider {
    skills: Vec<SkillFixture>,
    reminders: std::sync::Mutex<Vec<(String, SkillActivationOrigin)>>,
    activations: std::sync::Mutex<Vec<SkillActivationOrigin>>,
}

impl FixtureSkillProvider {
    fn new(skills: Vec<SkillFixture>) -> Self {
        Self {
            skills,
            reminders: std::sync::Mutex::new(Vec::new()),
            activations: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl SkillProvider for FixtureSkillProvider {
    fn get_skill(&self, name: &str) -> Option<SkillInfo> {
        self.skills
            .iter()
            .find(|s| s.name == name)
            .map(|s| SkillInfo {
                name: s.name.clone(),
                skill_type: s.skill_type.clone(),
                disable_model_invocation: s.disable_model_invocation,
                hidden_in_modes: s.hidden_in_modes.clone(),
                content: s.content.clone(),
                path: s.path.clone(),
                source: s.source.clone(),
            })
    }

    fn record_activation(&self, origin: SkillActivationOrigin) -> Result<(), SkillError> {
        self.activations.lock().unwrap().push(origin);
        Ok(())
    }

    fn render_skill_prompt(&self, skill: &SkillInfo, args: &str) -> String {
        format!("{} rendered with {}", skill.content, args)
    }

    fn current_session_mode(&self) -> Option<String> {
        None
    }

    fn append_system_reminder(
        &self,
        content: String,
        origin: SkillActivationOrigin,
    ) -> Result<(), SkillError> {
        self.reminders.lock().unwrap().push((content, origin));
        Ok(())
    }
}

struct FixtureQuestionProvider {
    response: Result<
        crate::builtin::collaboration::QuestionResult,
        crate::builtin::collaboration::QuestionError,
    >,
}

#[async_trait::async_trait]
impl crate::builtin::collaboration::QuestionProvider for FixtureQuestionProvider {
    async fn request_question(
        &self,
        _req: crate::builtin::collaboration::QuestionRequest,
        _signal: &crate::builtin::AbortSignal,
    ) -> Result<
        crate::builtin::collaboration::QuestionResult,
        crate::builtin::collaboration::QuestionError,
    > {
        match &self.response {
            Ok(r) => Ok(r.clone()),
            Err(e) => Err(match e {
                crate::builtin::collaboration::QuestionError::NotImplemented => {
                    crate::builtin::collaboration::QuestionError::NotImplemented
                }
                crate::builtin::collaboration::QuestionError::Aborted => {
                    crate::builtin::collaboration::QuestionError::Aborted
                }
                crate::builtin::collaboration::QuestionError::Other(_) => {
                    crate::builtin::collaboration::QuestionError::Other(anyhow::anyhow!(
                        "fixture error"
                    ))
                }
            }),
        }
    }
}

struct FixtureBackgroundRegistrar {
    next_id: String,
    fail: bool,
}

#[async_trait::async_trait]
impl crate::builtin::collaboration::BackgroundRegistrar for FixtureBackgroundRegistrar {
    async fn register_question_task(
        &self,
        _description: String,
        _run: crate::builtin::collaboration::QuestionRunFn,
        _options: crate::builtin::collaboration::QuestionTaskOptions,
    ) -> Result<String, crate::builtin::collaboration::BackgroundError> {
        if self.fail {
            return Err(crate::builtin::collaboration::BackgroundError::Message(
                "registrar down".into(),
            ));
        }
        Ok(self.next_id.clone())
    }
    async fn register_agent_task(
        &self,
        _completion: crate::builtin::collaboration::AgentCompletion,
        _description: String,
        _options: crate::builtin::collaboration::AgentTaskOptions,
    ) -> Result<String, crate::builtin::collaboration::BackgroundError> {
        unimplemented!()
    }
}

struct FixtureSubagentHost {
    behavior: String,
    result: Option<String>,
    error: Option<String>,
    agent_id: Option<String>,
}

#[async_trait::async_trait]
impl crate::builtin::collaboration::SubagentHost for FixtureSubagentHost {
    async fn spawn(
        &self,
        profile: &str,
        options: crate::builtin::collaboration::SubagentOptions,
    ) -> Result<
        crate::builtin::collaboration::SubagentHandle,
        crate::builtin::collaboration::SubagentError,
    > {
        self.make_handle(profile, options.signal)
    }
    async fn resume(
        &self,
        _agent_id: &str,
        options: crate::builtin::collaboration::SubagentOptions,
    ) -> Result<
        crate::builtin::collaboration::SubagentHandle,
        crate::builtin::collaboration::SubagentError,
    > {
        self.make_handle("subagent", options.signal)
    }
    fn get_profile_name(&self, _agent_id: &str) -> Option<String> {
        None
    }
    fn background_task_timeout_ms(&self) -> u64 {
        600_000
    }
    fn cancel_all(&self, _reason: &str) {}
}

impl FixtureSubagentHost {
    fn make_handle(
        &self,
        profile: &str,
        signal: crate::builtin::AbortSignal,
    ) -> Result<
        crate::builtin::collaboration::SubagentHandle,
        crate::builtin::collaboration::SubagentError,
    > {
        let profile = profile.to_string();
        if self.behavior == "fail" {
            let err_msg = self.error.clone().unwrap_or_else(|| "boom".into());
            return Err(crate::builtin::collaboration::SubagentError::Message(
                err_msg,
            ));
        }
        let agent_id = self.agent_id.clone().unwrap_or_else(|| {
            if self.behavior == "timeout" {
                "agent-timeout".into()
            } else {
                "agent-123".into()
            }
        });
        if self.behavior == "timeout" {
            return Ok(crate::builtin::collaboration::SubagentHandle {
                agent_id,
                profile_name: profile,
                completion: Box::pin(async move {
                    tokio::select! {
                        _ = futures::future::pending::<Result<crate::builtin::collaboration::SubagentResult, crate::builtin::collaboration::SubagentError>>() => unreachable!(),
                        _ = wait_golden_abort(signal) => {
                            Err(crate::builtin::collaboration::SubagentError::Message("aborted".into()))
                        }
                    }
                }),
            });
        }
        let result = self.result.clone().unwrap_or_else(|| "Done".into());
        Ok(crate::builtin::collaboration::SubagentHandle {
            agent_id,
            profile_name: profile,
            completion: Box::pin(async move {
                Ok(crate::builtin::collaboration::SubagentResult {
                    result,
                    usage: None,
                })
            }),
        })
    }
}

async fn wait_golden_abort(signal: crate::builtin::AbortSignal) {
    while !signal.aborted() {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

struct FixtureAgentBackgroundRegistrar {
    next_id: String,
    fail: bool,
}

#[async_trait::async_trait]
impl crate::builtin::collaboration::BackgroundRegistrar for FixtureAgentBackgroundRegistrar {
    async fn register_question_task(
        &self,
        _description: String,
        _run: crate::builtin::collaboration::QuestionRunFn,
        _options: crate::builtin::collaboration::QuestionTaskOptions,
    ) -> Result<String, crate::builtin::collaboration::BackgroundError> {
        unimplemented!()
    }
    async fn register_agent_task(
        &self,
        _completion: crate::builtin::collaboration::AgentCompletion,
        _description: String,
        _options: crate::builtin::collaboration::AgentTaskOptions,
    ) -> Result<String, crate::builtin::collaboration::BackgroundError> {
        if self.fail {
            return Err(crate::builtin::collaboration::BackgroundError::Message(
                "registrar down".into(),
            ));
        }
        Ok(self.next_id.clone())
    }
}

fn result_to_value(result: ExecutableToolResult) -> Value {
    let output = match result.output {
        ExecutableToolOutput::Text(s) => Value::String(s),
        ExecutableToolOutput::Parts(parts) => Value::Array(parts),
    };
    let mut obj = serde_json::Map::new();
    obj.insert("output".to_string(), output);
    obj.insert("isError".to_string(), Value::Bool(result.is_error));
    if let Some(msg) = result.message {
        obj.insert("message".to_string(), Value::String(msg));
    }
    Value::Object(obj)
}

pub fn run_fixture_file(path: &str) -> HashMap<String, CaseResult> {
    let content = std::fs::read_to_string(path).expect("read fixture");
    let fixture: FixtureFile = serde_json::from_str(&content).expect("parse fixture");
    let mut all_files = FileSet::new();
    for case in &fixture.cases {
        for (k, v) in files_for_op(&case.op) {
            all_files.insert(k, v);
        }
    }
    let temp_dir = if all_files.is_empty() {
        None
    } else {
        let dir = std::env::temp_dir().join(format!("tools-rs-golden-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        setup_files(&dir, &all_files).unwrap();
        Some(dir)
    };
    let mut out = HashMap::new();
    for case in &fixture.cases {
        let td = needs_tempdir(&case.op)
            .then_some(temp_dir.as_deref())
            .flatten();
        out.insert(case.name.clone(), run_case_sync(case, td));
    }
    out
}
