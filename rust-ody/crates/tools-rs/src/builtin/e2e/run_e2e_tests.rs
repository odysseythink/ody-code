use std::sync::{Arc, Mutex};

use kaos_rs::kaos::Kaos;
use serde_json::Value;

use crate::builtin::e2e::{
    AffectedTool, E2EConfig, E2EImpact, E2EResult, E2ETestRunner, E2ETestRunnerError,
};
use crate::builtin::{
    AbortSignal, BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};
use crate::policies::rule_match::literal_rule_pattern;
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;

fn parameters() -> Value {
    InputSchema::object(vec![
        (
            "toolId",
            InputSchema::string().optional().description(
                "Optional specific tool to test; if omitted, all affected tools are tested.",
            ),
        ),
        (
            "projectRoot",
            InputSchema::string()
                .optional()
                .description("Optional project root; defaults to the agent workspace root."),
        ),
    ])
    .build()
}

pub struct RunE2ETestsTool<R: E2ETestRunner> {
    kaos: Kaos,
    config: E2EConfig,
    runner: Arc<R>,
}

impl<R: E2ETestRunner> RunE2ETestsTool<R> {
    pub fn new(kaos: Kaos, config: E2EConfig, runner: Arc<R>) -> Self {
        Self {
            kaos,
            config,
            runner,
        }
    }
}

impl<R: E2ETestRunner + 'static> BuiltinTool for RunE2ETestsTool<R> {
    fn name(&self) -> &str {
        "RunE2ETests"
    }

    fn description(&self) -> &str {
        "Generate and run temporary end-to-end (E2E) tests for the current project."
    }

    fn parameters(&self) -> Value {
        parameters()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let tool_id = args
            .get("toolId")
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        let project_root = args
            .get("projectRoot")
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        let subject = tool_id.clone().unwrap_or_else(|| "*".to_string());
        let approval_rule = literal_rule_pattern(self.name(), &subject);
        let kaos = self.kaos.clone();
        let config = self.config.clone();
        let runner = Arc::clone(&self.runner);
        let desc = tool_id
            .as_ref()
            .map(|id| format!("Run E2E tests for {}", id))
            .unwrap_or_else(|| "Run E2E tests for affected tools".to_string());

        Ok(ToolExecution {
            accesses: ToolAccesses::default(),
            description: desc,
            approval_rule,
            matches_rule: Some(Box::new(move |rule_args| {
                crate::policies::rule_match::matches_glob_rule_subject(rule_args, &subject)
            })),
            display: None,
            execute: Box::new(move |ctx| {
                let kaos = kaos.clone();
                let config = config.clone();
                let runner = Arc::clone(&runner);
                let tool_id = tool_id.clone();
                let project_root = project_root.clone();
                Box::pin(async move {
                    execution(kaos, config, runner, tool_id, project_root, ctx).await
                })
            }),
        })
    }
}

async fn execution<R: E2ETestRunner>(
    kaos: Kaos,
    config: E2EConfig,
    runner: Arc<R>,
    tool_id: Option<String>,
    project_root: Option<String>,
    ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    if !config.enabled {
        return ExecutableToolResult::ok_text(
            "E2E testing is disabled in config.toml (e2e.enabled = false).".to_string(),
        );
    }

    let workspace_root = kaos.getcwd();
    let changed_files =
        match get_changed_files(&kaos, project_root.as_deref().unwrap_or(&workspace_root)).await {
            Some(files) => files,
            None => {
                return ExecutableToolResult::ok_text(
                    "Could not detect changed files; skipping E2E tests.".to_string(),
                )
            }
        };

    let root = project_root
        .or_else(|| derive_package_root(&changed_files))
        .unwrap_or(workspace_root);

    if let Err(e) = runner.detect_generator(&root).await {
        return ExecutableToolResult::ok_text(format!(
            "No E2E generator found for project at {}: {}.",
            root, e
        ));
    }

    let mut impact = match runner.analyze_impact(&changed_files, &config, &root).await {
        Ok(impact) => impact,
        Err(e) => {
            return ExecutableToolResult::ok_text(format!(
                "No E2E generator found for project at {}: {}.",
                root, e
            ))
        }
    };

    if let Some(id) = tool_id {
        impact.affected_tools.retain(|t| t.tool_id == id);
    }

    if impact.affected_tools.is_empty() {
        return ExecutableToolResult::ok_text(
            "No affected tools detected; skipping E2E tests.".to_string(),
        );
    }

    let mut test_files = Vec::new();
    for tool in &impact.affected_tools {
        match runner
            .generate_tests(tool, &changed_files, &root, &config.generated_test_dir)
            .await
        {
            Ok(files) => test_files.extend(files),
            Err(e) => {
                return ExecutableToolResult::ok_text(format!(
                    "E2E generator produced no test files: {}.",
                    e
                ))
            }
        }
    }

    if test_files.is_empty() {
        return ExecutableToolResult::ok_text("E2E generator produced no test files.".to_string());
    }

    if ctx.signal.aborted() {
        return ExecutableToolResult::error_text(
            "E2E tests cancelled.".to_string(),
            "cancelled".to_string(),
        );
    }

    match runner.run_e2e_tests(&test_files, &root, &ctx.signal).await {
        Ok(result) => {
            if ctx.signal.aborted() {
                return ExecutableToolResult::error_text(
                    "E2E tests cancelled.".to_string(),
                    "cancelled".to_string(),
                );
            }
            let is_error = result.is_error(&config.failure_policy);
            ExecutableToolResult {
                output: crate::builtin::ExecutableToolOutput::Text(result.summary.clone()),
                message: if is_error {
                    Some("Critical E2E tests failed.".to_string())
                } else {
                    None
                },
                is_error,
                stop_turn: Some(is_error),
            }
        }
        Err(E2ETestRunnerError::Cancelled) => ExecutableToolResult::error_text(
            "E2E tests cancelled.".to_string(),
            "cancelled".to_string(),
        ),
        Err(e) => ExecutableToolResult::ok_text(format!("E2E execution failed: {}", e)),
    }
}

pub fn derive_package_root(changed_files: &[String]) -> Option<String> {
    if changed_files.is_empty() {
        return None;
    }
    let parts: Vec<Vec<&str>> = changed_files
        .iter()
        .map(|s| s.split('/').collect())
        .collect();
    let mut common = parts[0].clone();
    for p in &parts[1..] {
        let mut i = 0;
        while i < common.len() && i < p.len() && common[i] == p[i] {
            i += 1;
        }
        common.truncate(i);
    }
    if common.is_empty() {
        None
    } else {
        Some(common.join("/"))
    }
}

pub fn filter_by_tool_id(impact: E2EImpact, tool_id: Option<&str>) -> Vec<AffectedTool> {
    match tool_id {
        Some(id) => impact
            .affected_tools
            .into_iter()
            .filter(|t| t.tool_id == id)
            .collect(),
        None => impact.affected_tools,
    }
}

async fn get_changed_files(kaos: &Kaos, project_root: &str) -> Option<Vec<String>> {
    let k = kaos.with_cwd(project_root);
    let proc = k
        .exec(&["git", "status", "--short", "--no-renames"])
        .await
        .ok()?;
    proc.wait().await;
    let stdout = proc.stdout().await;
    let text = String::from_utf8_lossy(&stdout);
    Some(
        text.lines()
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
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::{BuiltinTool, ExecutableToolContext};

    #[test]
    fn derive_package_root_prefers_deepest_common_parent() {
        assert_eq!(
            derive_package_root(&[
                "packages/a/src/foo.ts".to_string(),
                "packages/a/test/bar.ts".to_string(),
            ]),
            Some("packages/a".to_string())
        );
    }

    #[test]
    fn derive_package_root_returns_none_for_empty_list() {
        assert_eq!(derive_package_root(&[]), None);
    }

    #[test]
    fn affected_tools_filter_by_tool_id() {
        let impact = E2EImpact {
            affected_tools: vec![
                AffectedTool {
                    tool_id: "Read".to_string(),
                    reason: "r".to_string(),
                },
                AffectedTool {
                    tool_id: "Write".to_string(),
                    reason: "w".to_string(),
                },
            ],
        };
        let filtered = filter_by_tool_id(impact, Some("Read"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].tool_id, "Read");
    }

    #[derive(Clone)]
    struct FixtureRunner {
        calls: Arc<Mutex<Vec<Vec<String>>>>,
        result: E2EResult,
    }

    #[async_trait::async_trait]
    impl E2ETestRunner for FixtureRunner {
        async fn detect_generator(&self, _root: &str) -> Result<(), E2ETestRunnerError> {
            Ok(())
        }

        async fn analyze_impact(
            &self,
            changed_files: &[String],
            _config: &E2EConfig,
            _root: &str,
        ) -> Result<E2EImpact, E2ETestRunnerError> {
            Ok(E2EImpact {
                affected_tools: vec![AffectedTool {
                    tool_id: "Read".to_string(),
                    reason: format!("changed {:?}", changed_files),
                }],
            })
        }

        async fn generate_tests(
            &self,
            _tool: &AffectedTool,
            _changed_files: &[String],
            _root: &str,
            _dir: &str,
        ) -> Result<Vec<String>, E2ETestRunnerError> {
            Ok(vec!["generated.test.ts".to_string()])
        }

        async fn run_e2e_tests(
            &self,
            test_files: &[String],
            _root: &str,
            _signal: &AbortSignal,
        ) -> Result<E2EResult, E2ETestRunnerError> {
            self.calls.lock().unwrap().push(test_files.to_vec());
            Ok(self.result.clone())
        }
    }

    #[tokio::test]
    async fn run_e2e_tests_tool_runs_with_mock_runner() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap().to_string();
        std::process::Command::new("git")
            .args(["init", "--quiet", &root])
            .status()
            .unwrap();
        std::fs::write(tmp.path().join("foo.ts"), "// changed").unwrap();

        let result = E2EResult {
            passed: 3,
            failed: 0,
            skipped: 0,
            summary: "3 passed, 0 failed".to_string(),
            test_files: vec!["generated.test.ts".to_string()],
        };
        let runner = Arc::new(FixtureRunner {
            calls: Arc::new(Mutex::new(Vec::new())),
            result,
        });
        let env = kaos_rs::environment::Environment {
            os_kind: "macOS".to_string(),
            os_arch: "arm64".to_string(),
            os_version: "23.0.0".to_string(),
            shell_name: "bash".to_string(),
            shell_path: "/bin/bash".to_string(),
        };
        let kaos = Kaos::new(env, root.clone());
        let config = E2EConfig::default();
        let tool = RunE2ETestsTool::new(kaos, config, runner);
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let ctx = ExecutableToolContext {
            turn_id: "1".to_string(),
            tool_call_id: "call_1".to_string(),
            signal: AbortSignal::new(),
            metadata: None,
        };
        let out = (exec.execute)(ctx).await;
        assert!(!out.is_error);
        assert_eq!(out.to_text(), "3 passed, 0 failed");
    }
}
