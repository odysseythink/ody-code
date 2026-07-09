use std::sync::Arc;

use kaos_rs::environment::Environment;
use kaos_rs::kaos::Kaos;
use serde_json::Value;

use crate::builtin::grep::GrepTool;
use crate::builtin::quality::{parse_ody_marker, render_debt_ledger, NoopTelemetryClient};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};
use crate::policies::path_access::{
    assert_path_allowed, AssertPathOptions, PathAccessOperation, PathClass,
};
use crate::policies::rule_match::literal_rule_pattern;
use crate::tool_accesses::ToolAccesses;
use crate::workspace::WorkspaceConfig;

const MAX_MARKERS: i64 = 200;
const TOOL_DESCRIPTION: &str = "Scan the codebase for `// ody:` / `# ody:` simplification-debt markers and return a Chinese-first ledger report.";

pub struct HarvestOdyMarkersTool {
    kaos: Kaos,
    workspace: WorkspaceConfig,
    #[allow(dead_code)]
    grep: GrepTool,
    telemetry: Arc<dyn crate::builtin::quality::TelemetryClient>,
}

impl HarvestOdyMarkersTool {
    pub fn new(
        kaos: Kaos,
        workspace: WorkspaceConfig,
        grep: GrepTool,
        telemetry: Arc<dyn crate::builtin::quality::TelemetryClient>,
    ) -> Self {
        Self {
            kaos,
            workspace,
            grep,
            telemetry,
        }
    }
}

impl BuiltinTool for HarvestOdyMarkersTool {
    fn name(&self) -> &str {
        "HarvestOdyMarkers"
    }

    fn description(&self) -> &str {
        TOOL_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Optional subdirectory or file to scan."
                }
            },
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let path_arg = args
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let scan_path = if path_arg.is_empty() {
            self.workspace.workspace_dir.clone()
        } else {
            path_arg
        };

        let path_class = match self.kaos.path_class() {
            "win32" => PathClass::Win32,
            _ => PathClass::Posix,
        };

        let safe_path = assert_path_allowed(
            &scan_path,
            &self.kaos.getcwd(),
            &self.workspace,
            AssertPathOptions {
                mode: PathAccessOperation::Search,
                check_sensitive: Some(false),
                path_class: Some(path_class),
            },
        )?;

        let approval_rule = literal_rule_pattern(self.name(), &safe_path);
        let kaos = self.kaos.clone();
        let workspace = self.workspace.clone();
        let telemetry = Arc::clone(&self.telemetry);
        let safe_path2 = safe_path.clone();

        Ok(ToolExecution {
            accesses: ToolAccesses::search_tree(&safe_path),
            description: "Harvesting ody: simplification debt markers".into(),
            approval_rule,
            matches_rule: None,
            display: None,
            execute: Box::new(move |ctx| {
                let safe_path = safe_path2.clone();
                let workspace_dir = workspace.workspace_dir.clone();
                let grep = GrepTool::new(kaos.clone(), workspace.clone());
                let telemetry = Arc::clone(&telemetry);
                Box::pin(async move {
                    run_harvest(safe_path, workspace_dir, grep, telemetry, ctx).await
                })
            }),
        })
    }
}

async fn run_harvest(
    safe_path: String,
    workspace_dir: String,
    grep: GrepTool,
    telemetry: Arc<dyn crate::builtin::quality::TelemetryClient>,
    ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    if ctx.signal.aborted() {
        return ExecutableToolResult::error_text("Aborted".into(), "Aborted".into());
    }

    let grep_args = serde_json::json!({
        "pattern": "(#|//) ?ody:",
        "path": safe_path,
        "output_mode": "content",
        "-n": true,
        "head_limit": MAX_MARKERS,
        "include_ignored": false,
    });

    let exec = match grep.resolve_execution(grep_args) {
        Ok(e) => e,
        Err(e) => {
            telemetry.track(
                "debt_ledger_failed",
                serde_json::json!({ "error": e.to_string() }),
            );
            return ExecutableToolResult::error_text(
                format!("债务台账扫描失败：{}", e),
                "Grep setup failed".into(),
            );
        }
    };

    let grep_result = (exec.execute)(ctx).await;

    if grep_result.is_error {
        let err_text = grep_result.to_text();
        telemetry.track(
            "debt_ledger_failed",
            serde_json::json!({ "error": err_text }),
        );
        return ExecutableToolResult::error_text(
            format!("债务台账扫描失败：{}", err_text),
            "Grep failed".into(),
        );
    }

    let output_text = grep_result.to_text();
    let raw_lines: Vec<&str> = output_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    let mut markers: Vec<crate::builtin::quality::DebtLedgerMarker> =
        raw_lines.into_iter().filter_map(parse_ody_marker).collect();
    for m in &mut markers {
        m.file = relativize_to_workspace(&m.file, &workspace_dir);
    }

    let truncated = output_text.contains("Results truncated");
    let markdown = render_debt_ledger(&markers, truncated);
    let rot_count = markers.iter().filter(|m| m.rot).count();

    telemetry.track(
        "debt_ledger_harvested",
        serde_json::json!({
            "marker_count": markers.len(),
            "rot_risk_count": rot_count,
        }),
    );

    let payload = serde_json::json!({
        "markdown": markdown,
        "markerCount": markers.len(),
        "rotRiskCount": rot_count,
        "truncated": truncated,
    });

    ExecutableToolResult::ok_text(payload.to_string())
}

fn relativize_to_workspace(file: &str, workspace_dir: &str) -> String {
    if workspace_dir.is_empty() {
        return file.to_string();
    }
    if file == workspace_dir {
        return ".".to_string();
    }
    let prefix = format!("{}/", workspace_dir);
    if let Some(rel) = file.strip_prefix(&prefix) {
        rel.to_string()
    } else {
        file.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn harvests_markers_from_temp_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let ws = WorkspaceConfig::new(tmp.path().to_string_lossy().to_string());

        // Write a file with two markers.
        std::fs::write(
            tmp.path().join("a.rs"),
            "fn main() {\n  // ody: hardcoded timeout, use config\n  // ody: missing test\n}\n",
        )
        .unwrap();

        let grep = GrepTool::new(kaos.clone(), ws.clone());
        let tool = HarvestOdyMarkersTool::new(kaos, ws, grep, Arc::new(NoopTelemetryClient));
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;

        assert!(!result.is_error, "unexpected error: {}", result.to_text());
        let text = result.to_text();
        assert!(
            text.contains("hardcoded timeout"),
            "missing ceiling: {}",
            text
        );
        assert!(
            text.contains("missing test"),
            "missing second marker: {}",
            text
        );
        assert!(text.contains("2 个标记"), "missing count: {}", text);
    }

    #[tokio::test]
    async fn empty_tree_returns_clean_ledger() {
        let tmp = tempfile::tempdir().unwrap();
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let ws = WorkspaceConfig::new(tmp.path().to_string_lossy().to_string());
        let grep = GrepTool::new(kaos.clone(), ws.clone());
        let tool = HarvestOdyMarkersTool::new(kaos, ws, grep, Arc::new(NoopTelemetryClient));
        let exec = tool.resolve_execution(serde_json::json!({})).unwrap();
        let result = (exec.execute)(ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await;

        assert!(!result.is_error);
        assert!(result.to_text().contains("台账干净"));
    }
}
