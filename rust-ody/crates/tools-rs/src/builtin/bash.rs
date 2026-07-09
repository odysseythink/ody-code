use kaos_rs::kaos::Kaos;
use serde_json::Value;
use std::time::Duration;

use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult, ToolError,
    ToolExecution,
};
use crate::schema::InputSchema;
use crate::tool_accesses::ToolAccesses;
use crate::workspace::WorkspaceConfig;

const DEFAULT_TIMEOUT_SECONDS: u64 = 60;
const MAX_TIMEOUT_SECONDS: u64 = 600;
const KILL_GRACE_SECONDS: u64 = 5;
const MAX_OUTPUT_BYTES: usize = 1 * 1024 * 1024;
const POLL_INTERVAL_MS: u64 = 100;

fn bash_parameters() -> Value {
    InputSchema::object(vec![
        (
            "command",
            InputSchema::string().description("The command to execute."),
        ),
        (
            "timeout",
            InputSchema::integer()
                .min(1.0)
                .max(MAX_TIMEOUT_SECONDS as f64)
                .optional()
                .default(serde_json::json!(DEFAULT_TIMEOUT_SECONDS))
                .description("Optional timeout in seconds for the command to execute. Default 60s, max 600s."),
        ),
        (
            "env",
            InputSchema::record(serde_json::json!({"type": "string"}))
                .optional()
                .description("Optional environment variables to pass to the command."),
        ),
        (
            "description",
            InputSchema::string()
                .optional()
                .description("A short description for the command."),
        ),
        (
            "run_in_background",
            InputSchema::boolean()
                .optional()
                .default(serde_json::json!(false))
                .description("Whether to run the command as a background task."),
        ),
    ])
    .build()
}

pub struct BashTool {
    kaos: Kaos,
    #[allow(dead_code)]
    workspace: WorkspaceConfig,
}

impl BashTool {
    pub fn new(kaos: Kaos, workspace: WorkspaceConfig) -> Self {
        Self { kaos, workspace }
    }
}

impl BuiltinTool for BashTool {
    fn name(&self) -> &str {
        "Bash"
    }

    fn description(&self) -> &str {
        "Execute a bash command."
    }

    fn parameters(&self) -> Value {
        bash_parameters()
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let command = args
            .get("command")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::InvalidArgs("command is required".into()))?;

        let description = args
            .get("description")
            .and_then(Value::as_str)
            .map(|s| s.to_string());

        let kaos = self.kaos.clone();
        let args2 = args.clone();

        Ok(ToolExecution {
            accesses: ToolAccesses::all(),
            description: description.unwrap_or_else(|| format!("Bash: {}", command)),
            matches_rule: None,
            display: None,
            approval_rule: format!("bash:{}", command),
            execute: Box::new(move |ctx| {
                let kaos = kaos.clone();
                let args = args2.clone();
                Box::pin(async move { bash_execution(kaos, args, ctx).await })
            }),
        })
    }
}

#[derive(Debug, PartialEq)]
enum WaitResult {
    Exited(i32),
    Aborted,
}

async fn bash_execution(
    kaos: Kaos,
    args: Value,
    ctx: ExecutableToolContext,
) -> ExecutableToolResult {
    // Reject background mode
    if args
        .get("run_in_background")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return ExecutableToolResult::error_text(
            "Background BashTool execution is not yet supported in the Rust host.".into(),
            "Not supported".into(),
        );
    }

    let command = match args
        .get("command")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        Some(c) => c,
        None => {
            return ExecutableToolResult::error_text(
                "command is required".into(),
                "command is required".into(),
            );
        }
    };

    let timeout_secs = args
        .get("timeout")
        .and_then(Value::as_u64)
        .map(|t| {
            if t == 0 {
                DEFAULT_TIMEOUT_SECONDS
            } else if t > MAX_TIMEOUT_SECONDS {
                MAX_TIMEOUT_SECONDS
            } else {
                t
            }
        })
        .unwrap_or(DEFAULT_TIMEOUT_SECONDS);

    // Build extra env from the env object
    let extra_env: Vec<(String, String)> =
        if let Some(env_obj) = args.get("env").and_then(Value::as_object) {
            env_obj
                .iter()
                .map(|(k, v)| {
                    let val = match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (k.clone(), val)
                })
                .collect()
        } else {
            Vec::new()
        };

    let env_refs: Vec<(&str, &str)> = extra_env
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let shell = kaos.env().shell_path.clone();
    let shell_args = [shell.as_str(), "-c", command];

    let proc = match kaos.exec_with_env(&shell_args, &env_refs).await {
        Ok(p) => p,
        Err(e) => {
            return ExecutableToolResult::error_text(
                format!("{}", e),
                format!("Failed to spawn command: {}", e),
            );
        }
    };

    let wait_result = tokio::time::timeout(Duration::from_secs(timeout_secs), async {
        loop {
            if ctx.signal.aborted() {
                return WaitResult::Aborted;
            }
            if let Some(code) = proc.exit_code() {
                return WaitResult::Exited(code);
            }
            tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
        }
    })
    .await;

    let _final_exit = match wait_result {
        Ok(WaitResult::Exited(code)) => Some(code),
        Ok(WaitResult::Aborted) | Err(_) => {
            // Kill the process: SIGTERM, wait grace, then SIGKILL
            let _ = proc.kill(None).await;
            // Wait up to KILL_GRACE_SECONDS for it to die
            let grace_result =
                tokio::time::timeout(Duration::from_secs(KILL_GRACE_SECONDS), async {
                    loop {
                        if proc.exit_code().is_some() {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                })
                .await;
            if grace_result.is_err() {
                let _ = proc.kill(Some("SIGKILL")).await;
                // Wait a bit for SIGKILL to take effect
                tokio::time::timeout(Duration::from_secs(2), async {
                    loop {
                        if proc.exit_code().is_some() {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                })
                .await
                .ok();
            }
            None
        }
    };

    let stdout = proc.stdout().await;
    let stderr = proc.stderr().await;

    // Combine stdout and stderr
    let mut combined: Vec<u8> = Vec::new();
    combined.extend_from_slice(&stdout);
    if !stderr.is_empty() {
        if !combined.is_empty() {
            combined.extend_from_slice(b"\n");
        }
        combined.extend_from_slice(&stderr);
    }

    // Truncate at MAX_OUTPUT_BYTES
    let truncated = if combined.len() > MAX_OUTPUT_BYTES {
        let mut t = combined[..MAX_OUTPUT_BYTES].to_vec();
        t.extend_from_slice(b"\n... (output truncated at 1 MiB)");
        t
    } else {
        combined
    };

    let output_text = String::from_utf8_lossy(&truncated).to_string();

    match wait_result {
        Err(_) => {
            // Timed out
            ExecutableToolResult::error_text(
                output_text,
                format!("Command timed out after {} seconds", timeout_secs),
            )
        }
        Ok(WaitResult::Aborted) => ExecutableToolResult::error_text(output_text, "Aborted".into()),
        Ok(WaitResult::Exited(code)) => {
            if code == 0 {
                ExecutableToolResult::ok_text(output_text)
            } else {
                let mut message = format!("Command failed with exit code: {}", code);
                // Append tail of stderr if available and not already in output
                if !stderr.is_empty() {
                    let stderr_str = String::from_utf8_lossy(&stderr);
                    let tail = if stderr_str.len() > 200 {
                        &stderr_str[stderr_str.len() - 200..]
                    } else {
                        &stderr_str
                    };
                    // Only append if informative
                    let trimmed = tail.trim();
                    if !trimmed.is_empty() && trimmed.len() < 200 {
                        message.push_str(": ");
                        message.push_str(trimmed);
                    }
                }
                ExecutableToolResult {
                    output: ExecutableToolOutput::Text(output_text),
                    message: Some(message),
                    is_error: true,
                    stop_turn: None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::WorkspaceConfig;
    use kaos_rs::environment::Environment;
    use serde_json::json;

    fn dummy_env() -> Environment {
        Environment {
            os_kind: "macOS".to_string(),
            os_arch: "arm64".to_string(),
            os_version: "23.0.0".to_string(),
            shell_name: "bash".to_string(),
            shell_path: "/bin/bash".to_string(),
        }
    }

    fn workspace(tmp: &std::path::Path) -> WorkspaceConfig {
        WorkspaceConfig::new(tmp.to_string_lossy().to_string())
    }

    async fn run_bash(tmp: &tempfile::TempDir, args: Value) -> ExecutableToolResult {
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let tool = BashTool::new(kaos, workspace(tmp.path()));
        let exec = tool.resolve_execution(args).unwrap();
        (exec.execute)(ExecutableToolContext {
            turn_id: "".into(),
            tool_call_id: "".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        })
        .await
    }

    #[tokio::test]
    async fn echo_returns_stdout_and_zero_exit() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_bash(&tmp, json!({"command": "echo hello world"})).await;
        assert!(
            !result.is_error,
            "expected success, got: {:?}",
            result.message
        );
        let output = match &result.output {
            ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text output"),
        };
        assert!(output.contains("hello world"));
    }

    #[tokio::test]
    async fn non_zero_exit_is_marked_error() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_bash(&tmp, json!({"command": "exit 42"})).await;
        assert!(result.is_error, "expected error for non-zero exit");
        let message = result.message.unwrap_or_default();
        assert!(
            message.contains("42"),
            "message should contain exit code 42, got: {}",
            message
        );
    }

    #[tokio::test]
    async fn timeout_kills_long_sleep() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_bash(&tmp, json!({"command": "sleep 30", "timeout": 1})).await;
        assert!(result.is_error, "expected error for timeout");
        let message = result.message.unwrap_or_default();
        assert!(
            message.contains("timed out"),
            "message should mention 'timed out', got: {}",
            message
        );
    }

    #[tokio::test]
    async fn env_vars_are_visible_to_command() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_bash(
            &tmp,
            json!({"command": "echo $ODY_TEST_VAR", "env": {"ODY_TEST_VAR": "secret"}}),
        )
        .await;
        assert!(
            !result.is_error,
            "expected success, got: {:?}",
            result.message
        );
        let output = match &result.output {
            ExecutableToolOutput::Text(s) => s,
            _ => panic!("expected text output"),
        };
        assert!(
            output.contains("secret"),
            "output should contain 'secret', got: {}",
            output
        );
    }
}
