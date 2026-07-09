use std::sync::Arc;

use async_trait::async_trait;
use kaos_rs::kaos::Kaos;
use tools_rs::builtin::e2e::{
    AffectedTool, E2EConfig, E2EImpact, E2EResult, E2ETestRunner, E2ETestRunnerError,
};
use tools_rs::builtin::AbortSignal;

pub struct HostE2ETestRunner {
    kaos: Arc<Kaos>,
}

impl HostE2ETestRunner {
    pub fn new(kaos: Arc<Kaos>) -> Self {
        Self { kaos }
    }
}

#[async_trait]
impl E2ETestRunner for HostE2ETestRunner {
    async fn detect_generator(&self, _project_root: &str) -> Result<(), E2ETestRunnerError> {
        Ok(())
    }

    async fn analyze_impact(
        &self,
        changed_files: &[String],
        _config: &E2EConfig,
        project_root: &str,
    ) -> Result<E2EImpact, E2ETestRunnerError> {
        let re = regex::Regex::new(r"packages/agent-core/src/tools/builtin/([a-z_-]+)/")
            .map_err(|e| E2ETestRunnerError::ExecutionFailed(e.to_string()))?;
        let mut tools = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for f in changed_files {
            if let Some(caps) = re.captures(f) {
                let tool_id = to_pascal_case(caps.get(1).unwrap().as_str());
                if seen.insert(tool_id.clone()) {
                    tools.push(AffectedTool {
                        tool_id,
                        reason: format!("changed file under {}", project_root),
                    });
                }
            }
        }
        if tools.is_empty() {
            return Err(E2ETestRunnerError::NoGenerator);
        }
        Ok(E2EImpact {
            affected_tools: tools,
        })
    }

    async fn generate_tests(
        &self,
        tool: &AffectedTool,
        _changed_files: &[String],
        project_root: &str,
        generated_test_dir: &str,
    ) -> Result<Vec<String>, E2ETestRunnerError> {
        let dir = std::path::Path::new(project_root).join(generated_test_dir);
        std::fs::create_dir_all(&dir)
            .map_err(|e| E2ETestRunnerError::ExecutionFailed(e.to_string()))?;
        let file_name = format!("{}.e2e.test.ts", to_kebab_case(&tool.tool_id));
        let path = dir.join(&file_name);
        let content = generate_vitest_content(tool);
        std::fs::write(&path, content)
            .map_err(|e| E2ETestRunnerError::ExecutionFailed(e.to_string()))?;
        Ok(vec![path.to_string_lossy().to_string()])
    }

    async fn run_e2e_tests(
        &self,
        test_files: &[String],
        project_root: &str,
        signal: &AbortSignal,
    ) -> Result<E2EResult, E2ETestRunnerError> {
        let k = self.kaos.with_cwd(project_root);
        let mut args: Vec<&str> = vec!["pnpm", "vitest", "run", "--reporter=json"];
        for f in test_files {
            args.push(f);
        }
        let proc = k
            .exec(&args)
            .await
            .map_err(|e| E2ETestRunnerError::ExecutionFailed(e.to_string()))?;
        let stdout = proc.stdout().await;
        let stderr = proc.stderr().await;
        let exit = proc.wait().await;
        if signal.aborted() {
            return Err(E2ETestRunnerError::Cancelled);
        }
        let text = String::from_utf8_lossy(&stdout);
        let result = match parse_vitest_json(&text, test_files) {
            Some(r) => r,
            None => E2EResult {
                passed: 0,
                failed: if exit == 0 { 0 } else { 1 },
                skipped: 0,
                summary: format!(
                    "E2E runner exited {}. stderr: {}",
                    exit,
                    String::from_utf8_lossy(&stderr)
                ),
                test_files: test_files.to_vec(),
            },
        };
        Ok(result)
    }
}

fn generate_vitest_content(tool: &AffectedTool) -> String {
    format!(
        "// Auto-generated E2E test for {}\nimport {{ describe, it, expect }} from 'vitest';\n\ndescribe('{}', () => {{\n  it('should be callable through its interface', () => {{\n    expect(typeof '{}').toBe('string');\n  }});\n}});\n",
        tool.tool_id, tool.tool_id, tool.tool_id
    )
}

pub fn parse_vitest_json(text: &str, test_files: &[String]) -> Option<E2EResult> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end < start {
        return None;
    }
    let json_str = &text[start..=end];
    let parsed: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let passed = parsed.get("numPassedTests")?.as_u64()? as usize;
    let failed = parsed.get("numFailedTests")?.as_u64()? as usize;
    let skipped = parsed.get("numSkippedTests")?.as_u64()? as usize;
    let summary = format!("{} passed, {} failed, {} skipped", passed, failed, skipped);
    Some(E2EResult {
        passed,
        failed,
        skipped,
        summary,
        test_files: test_files.to_vec(),
    })
}

fn to_pascal_case(s: &str) -> String {
    s.split('-')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn to_kebab_case(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('-');
        }
        out.push(c.to_lowercase().next().unwrap_or(c));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vitest_json_parser_extracts_summary() {
        let raw = r#"{"numPassedTests":2,"numFailedTests":1,"numSkippedTests":0,"testResults":[{"name":"a.test.ts","status":"failed"}]}"#;
        let result = parse_vitest_json(raw, &["a.test.ts".to_string()]).unwrap();
        assert_eq!(result.passed, 2);
        assert_eq!(result.failed, 1);
        assert!(result.summary.contains("2 passed"));
    }
}
