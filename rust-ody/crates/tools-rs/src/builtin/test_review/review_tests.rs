use std::sync::Arc;

use kaos_rs::kaos::Kaos;
use regex::Regex;
use serde_json::Value;

use crate::builtin::test_review::{
    format_report, AdvancedSessionReviewResult, TestReviewError, TestReviewer,
};
use crate::builtin::{
    AbortSignal, BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};
use crate::policies::rule_match::literal_rule_pattern;
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;

const TEST_FILE_RE: &str = r"\.(test|spec)(\.[cm]?[jt]sx?)$";
const SOURCE_FILE_RE: &str = r"\.[cm]?[jt]sx?$";
const REVIEW_CONTENT_BUDGET_CHARS: usize = 300_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewEntryLabel {
    TestFile,
    ImplementationFile,
}

impl ReviewEntryLabel {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReviewEntryLabel::TestFile => "TEST FILE",
            ReviewEntryLabel::ImplementationFile => "IMPLEMENTATION FILE",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReviewEntry {
    pub label: ReviewEntryLabel,
    pub path: String,
}

pub fn build_review_entries(test_files: &[String], changed_files: &[String]) -> Vec<ReviewEntry> {
    let test_re = Regex::new(TEST_FILE_RE).unwrap();
    let source_re = Regex::new(SOURCE_FILE_RE).unwrap();
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |label: ReviewEntryLabel, path: String| {
        if seen.insert(path.clone()) {
            entries.push(ReviewEntry { label, path });
        }
    };
    for t in test_files {
        push(ReviewEntryLabel::TestFile, t.clone());
        let sibling = test_re.replace(t, "$2").to_string();
        if sibling != *t {
            push(ReviewEntryLabel::ImplementationFile, sibling);
        }
    }
    for f in changed_files {
        if !test_re.is_match(f) && source_re.is_match(f) {
            push(ReviewEntryLabel::ImplementationFile, f.clone());
        }
    }
    entries
}

fn parameters() -> Value {
    InputSchema::object(vec![(
        "projectRoot",
        InputSchema::string()
            .optional()
            .description("Optional project root; defaults to the agent workspace root."),
    )])
    .build()
}

pub struct ReviewTestsTool<R: TestReviewer> {
    kaos: Kaos,
    reviewer: Arc<R>,
}

impl<R: TestReviewer> ReviewTestsTool<R> {
    pub fn new(kaos: Kaos, reviewer: Arc<R>) -> Self {
        Self { kaos, reviewer }
    }
}

impl<R: TestReviewer + 'static> BuiltinTool for ReviewTestsTool<R> {
    fn name(&self) -> &str {
        "ReviewTests"
    }

    fn description(&self) -> &str {
        "Independently review the changed tests"
    }

    fn parameters(&self) -> Value {
        parameters()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let project_root = args
            .get("projectRoot")
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        let kaos = self.kaos.clone();
        let reviewer = Arc::clone(&self.reviewer);
        let approval_rule = literal_rule_pattern(self.name(), "*");
        Ok(ToolExecution {
            accesses: ToolAccesses::default(),
            description: "Independently review the changed tests".to_string(),
            approval_rule,
            matches_rule: Some(Box::new(|_| true)),
            display: None,
            execute: Box::new(move |ctx| {
                let kaos = kaos.clone();
                let reviewer = Arc::clone(&reviewer);
                let project_root = project_root.clone();
                Box::pin(async move { execution(kaos, reviewer, project_root, ctx).await })
            }),
        })
    }
}

async fn execution<R: TestReviewer>(
    kaos: Kaos,
    reviewer: Arc<R>,
    project_root: Option<String>,
    ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    let root = project_root.unwrap_or_else(|| kaos.getcwd());
    let changed_files = match get_changed_files(&kaos, &root).await {
        Some(files) => files,
        None => {
            return ExecutableToolResult::ok_text(
                "No changed files detected; nothing to review.".to_string(),
            )
        }
    };

    let test_re = Regex::new(TEST_FILE_RE).unwrap();
    let test_files: Vec<String> = changed_files
        .iter()
        .filter(|f| test_re.is_match(f))
        .cloned()
        .collect();
    if test_files.is_empty() {
        return ExecutableToolResult::ok_text(
            "No changed test files detected; nothing to review.".to_string(),
        );
    }

    let entries = build_review_entries(&test_files, &changed_files);
    let review_content = build_review_content(&kaos, &root, &entries).await;
    if review_content.trim().is_empty() {
        return ExecutableToolResult::ok_text(
            "Changed test files could not be read; nothing to review.".to_string(),
        );
    }

    if ctx.signal.aborted() {
        return ExecutableToolResult::error_text(
            "Test review cancelled.".to_string(),
            "cancelled".to_string(),
        );
    }

    let alias = "kimi-for-coding";
    match reviewer
        .review_tests(&review_content, alias, &ctx.signal)
        .await
    {
        Ok(result) => {
            if ctx.signal.aborted() {
                return ExecutableToolResult::error_text(
                    "Test review cancelled.".to_string(),
                    "cancelled".to_string(),
                );
            }
            if !result.ok {
                let note = result.note.unwrap_or_else(|| "unknown error".to_string());
                return ExecutableToolResult::ok_text(format!(
                    "Test review could not run: {} (reviewer: {}).",
                    note, alias
                ));
            }
            ExecutableToolResult::ok_text(format_report(&result, alias, &test_files))
        }
        Err(TestReviewError::Cancelled) | Err(TestReviewError::GenerationFailed(_))
            if ctx.signal.aborted() =>
        {
            ExecutableToolResult::error_text(
                "Test review cancelled.".to_string(),
                "cancelled".to_string(),
            )
        }
        Err(e) => ExecutableToolResult::ok_text(format!(
            "Test review could not run: {} (reviewer: {}).",
            e, alias
        )),
    }
}

async fn build_review_content(kaos: &Kaos, root: &str, entries: &[ReviewEntry]) -> String {
    let k = kaos.with_cwd(root);
    let mut sections = Vec::new();
    let mut total = 0usize;
    let mut omitted = 0usize;
    for entry in entries {
        if total >= REVIEW_CONTENT_BUDGET_CHARS {
            omitted += 1;
            continue;
        }
        let content = match k.read_text(&entry.path, None, None).await {
            Ok(c) => c,
            Err(_) => continue,
        };
        let section = format!(
            "===== {}: {} =====\n\n{}\n",
            entry.label.as_str(),
            entry.path,
            content
        );
        total += section.len();
        sections.push(section);
    }
    if omitted > 0 {
        sections.push(format!(
            "===== [truncated: {} file(s) omitted to fit the review budget] =====\n",
            omitted
        ));
    }
    sections.join("\n")
}

async fn get_changed_files(kaos: &Kaos, root: &str) -> Option<Vec<String>> {
    let k = kaos.with_cwd(root);
    let proc = k
        .exec(&["git", "status", "--short", "--no-renames"])
        .await
        .ok()?;
    proc.wait().await;
    let stdout = proc.stdout().await;
    let text = String::from_utf8_lossy(&stdout);
    Some(parse_git_status_short(&text))
}

fn parse_git_status_short(status: &str) -> Vec<String> {
    status
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.len() < 4 {
                return None;
            }
            let path = &trimmed[3..];
            if path.is_empty() {
                return None;
            }
            Some(path.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_review_entries_pairs_test_with_sibling_impl() {
        let test_files = vec![
            "src/foo.test.ts".to_string(),
            "src/bar.spec.tsx".to_string(),
        ];
        let changed = vec![
            "src/foo.test.ts".to_string(),
            "src/foo.ts".to_string(),
            "src/bar.spec.tsx".to_string(),
            "src/bar.tsx".to_string(),
            "src/util.ts".to_string(),
        ];
        let entries = build_review_entries(&test_files, &changed);
        let labels: Vec<_> = entries
            .iter()
            .map(|e| (e.label.as_str(), e.path.as_str()))
            .collect();
        assert_eq!(
            labels,
            vec![
                ("TEST FILE", "src/foo.test.ts"),
                ("IMPLEMENTATION FILE", "src/foo.ts"),
                ("TEST FILE", "src/bar.spec.tsx"),
                ("IMPLEMENTATION FILE", "src/bar.tsx"),
                ("IMPLEMENTATION FILE", "src/util.ts"),
            ]
        );
    }

    #[test]
    fn build_review_entries_dedupes_sibling_impl_when_already_in_changed() {
        let test_files = vec!["a.test.ts".to_string()];
        let changed = vec!["a.test.ts".to_string(), "a.ts".to_string()];
        let entries = build_review_entries(&test_files, &changed);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].path, "a.ts");
    }

    use crate::builtin::test_review::{
        AdvancedSessionReviewResult, AuditLevel, Confidence, MutationProbe, ReviewFinding,
        Severity, TestReviewError, TestReviewer,
    };
    use crate::builtin::{BuiltinTool, ExecutableToolContext};
    use kaos_rs::environment::Environment;
    use kaos_rs::kaos::Kaos;
    use tempfile::TempDir;

    #[derive(Clone)]
    struct FixtureReviewer {
        result: AdvancedSessionReviewResult,
    }

    #[async_trait::async_trait]
    impl TestReviewer for FixtureReviewer {
        async fn review_tests(
            &self,
            content: &str,
            _alias: &str,
            _signal: &AbortSignal,
        ) -> Result<AdvancedSessionReviewResult, TestReviewError> {
            assert!(content.contains("===== TEST FILE: add.test.ts ====="));
            Ok(self.result.clone())
        }
    }

    #[tokio::test]
    async fn review_tests_tool_runs_end_to_end() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap().to_string();

        // Initialize a git repo so git status works.
        std::process::Command::new("git")
            .args(["init", "--quiet", &root])
            .status()
            .unwrap();
        std::fs::write(
            tmp.path().join("add.test.ts"),
            "test('adds', () => expect(add(1,2)).toBe(3));",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("add.ts"),
            "export const add = (a,b) => a+b;",
        )
        .unwrap();

        let reviewer = Arc::new(FixtureReviewer {
            result: AdvancedSessionReviewResult {
                audit_level: AuditLevel::Standard,
                findings: vec![ReviewFinding {
                    severity: Severity::High,
                    confidence: Some(Confidence::Certain),
                    title: "Tautology".to_string(),
                    detail: "assertion re-states implementation".to_string(),
                    location: Some("add.test.ts:1".to_string()),
                    suggested_fix: None,
                }],
                mutation_probes: Some(vec![MutationProbe {
                    location: "add.ts:1".to_string(),
                    mutation: "return a - b;".to_string(),
                    expected_catch: "adds".to_string(),
                }]),
                ok: true,
                note: None,
            },
        });
        let env = kaos_rs::environment::Environment {
            os_kind: "macOS".to_string(),
            os_arch: "arm64".to_string(),
            os_version: "23.0.0".to_string(),
            shell_name: "bash".to_string(),
            shell_path: "/bin/bash".to_string(),
        };
        let kaos = Kaos::new(env, root.clone());
        let tool = ReviewTestsTool::new(kaos, reviewer);
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let ctx = ExecutableToolContext {
            turn_id: "1".to_string(),
            tool_call_id: "call_1".to_string(),
            signal: AbortSignal::new(),
            metadata: None,
        };
        let result = (exec.execute)(ctx).await;
        let text = result.to_text();
        assert!(text.contains("Tautology"));
        assert!(text.contains("add.test.ts"));
        assert!(text.contains("return a - b;"));
        assert!(text.contains("ESCALATE"));
    }
}
