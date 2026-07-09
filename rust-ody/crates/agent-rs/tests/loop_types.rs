use agent_rs::agent_loop::tool_access::ToolAccesses;
use agent_rs::agent_loop::types::{
    ExecutableTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult,
    ExecutableToolSuccessResult, LoopHooks, RunnableToolExecution, ToolExecution,
};
use kosong_rs::provider::AbortSignal;
use serde_json::json;

// 一个必须保留的工具：名字含 "read_file" 但不应被当成内置 Read
struct EchoTool;

#[async_trait::async_trait]
impl ExecutableTool for EchoTool {
    async fn resolve_execution(
        &self,
        input: serde_json::Value,
    ) -> Result<ToolExecution, anyhow::Error> {
        Ok(ToolExecution::Runnable(RunnableToolExecution {
            is_error: None,
            approval_rule: "auto".to_string(),
            matches_rule: None,
            execute: Box::new(move |ctx: ExecutableToolContext| {
                let input = input.clone();
                Box::pin(async move {
                    Ok(ExecutableToolResult::Success(ExecutableToolSuccessResult {
                        output: ExecutableToolOutput::Text(format!(
                            "echo:{}:{}",
                            ctx.tool_call_id,
                            input["x"].as_str().unwrap_or("?")
                        )),
                        message: None,
                        stop_turn: None,
                        is_error: None,
                    }))
                })
            }),
            accesses: Some(ToolAccesses::none()),
            description: Some("echo".to_string()),
            display: None,
            stop_batch_after_this: None,
        }))
    }

    fn name(&self) -> &str {
        "echo"
    }
    fn description(&self) -> &str {
        "echo tool"
    }
    fn parameters(&self) -> serde_json::Value {
        json!({"type":"object"})
    }
}

#[tokio::test]
async fn executable_tool_runs_and_returns_text_output() {
    let tool = EchoTool;
    let exec = tool.resolve_execution(json!({"x":"hi"})).await.unwrap();
    match exec {
        ToolExecution::Runnable(r) => {
            let ctx = ExecutableToolContext {
                turn_id: "t1".into(),
                tool_call_id: "tc1".into(),
                metadata: None,
                signal: AbortSignal::new(),
                on_update: None,
            };
            let result = (r.execute)(ctx).await.unwrap();
            let text = match result {
                ExecutableToolResult::Success(s) => match s.output {
                    ExecutableToolOutput::Text(t) => t,
                    _ => panic!("expected text"),
                },
                ExecutableToolResult::Error(_) => panic!("expected success"),
            };
            assert_eq!(text, "echo:tc1:hi");
        }
        _ => panic!("expected runnable"),
    }
}

#[test]
fn loop_hooks_default_all_none() {
    let hooks = LoopHooks::default();
    assert!(hooks.before_step.is_none());
    assert!(hooks.after_step.is_none());
    assert!(hooks.prepare_tool_execution.is_none());
    assert!(hooks.authorize_tool_execution.is_none());
    assert!(hooks.finalize_tool_result.is_none());
    assert!(hooks.should_continue_after_stop.is_none());
}
