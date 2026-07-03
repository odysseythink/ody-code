use std::sync::Arc;

use crate::agent_loop::types::{
    ExecutableTool, ExecutableToolContext as LoopContext, RunnableToolExecution,
    ToolExecution as LoopToolExecution,
};
use crate::records::nested::{
    ExecutableToolErrorResult, ExecutableToolOutput as AgentOutput,
    ExecutableToolResult as AgentResult, ExecutableToolSuccessResult,
};
use tools_rs::builtin::{
    BuiltinTool, ExecutableToolContext as ToolsContext, ExecutableToolResult as ToolsResult,
};

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn convert_output(o: tools_rs::builtin::ExecutableToolOutput) -> AgentOutput {
    match o {
        tools_rs::builtin::ExecutableToolOutput::Text(s) => AgentOutput::Text(s),
        tools_rs::builtin::ExecutableToolOutput::Parts(parts) => AgentOutput::Parts(
            parts
                .into_iter()
                .map(|v| {
                    serde_json::from_value(v).unwrap_or_else(|_| {
                        kosong_rs::message::ContentPart::Text {
                            text: String::new(),
                        }
                    })
                })
                .collect(),
        ),
    }
}

fn convert_tool_accesses(
    a: tools_rs::tool_accesses::ToolAccesses,
) -> crate::agent_loop::tool_access::ToolAccesses {
    crate::agent_loop::tool_access::ToolAccesses(
        a.0.into_iter()
            .map(|ra| match ra {
                tools_rs::tool_accesses::ToolResourceAccess::File {
                    operation,
                    path,
                    recursive,
                } => crate::agent_loop::tool_access::ToolResourceAccess::File {
                    operation,
                    path,
                    recursive,
                },
                tools_rs::tool_accesses::ToolResourceAccess::All => {
                    crate::agent_loop::tool_access::ToolResourceAccess::All
                }
            })
            .collect(),
    )
}

impl From<ToolsResult> for AgentResult {
    fn from(r: ToolsResult) -> Self {
        let output = convert_output(r.output);
        if r.is_error {
            AgentResult::Error(ExecutableToolErrorResult {
                output,
                is_error: true,
                stop_turn: None,
                message: r.message,
            })
        } else {
            AgentResult::Success(ExecutableToolSuccessResult {
                output,
                is_error: None,
                stop_turn: None,
                message: r.message,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// ToolBridge
// ---------------------------------------------------------------------------

pub struct ToolBridge {
    inner: Arc<dyn BuiltinTool>,
}

impl ToolBridge {
    pub fn new(inner: Arc<dyn BuiltinTool>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl ExecutableTool for ToolBridge {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn description(&self) -> &str {
        self.inner.description()
    }
    fn parameters(&self) -> serde_json::Value {
        self.inner.parameters()
    }
    async fn resolve_execution(
        &self,
        input: serde_json::Value,
    ) -> Result<LoopToolExecution, anyhow::Error> {
        let tools_exec = self.inner.resolve_execution(input)?;
        let execute = tools_exec.execute;
        let matches_rule = tools_exec.matches_rule;
        Ok(LoopToolExecution::Runnable(RunnableToolExecution {
            is_error: None,
            accesses: Some(convert_tool_accesses(tools_exec.accesses)),
            display: tools_exec.display,
            description: Some(tools_exec.description),
            stop_batch_after_this: None,
            approval_rule: tools_exec.approval_rule,
            matches_rule,
            execute: Box::new(move |loop_ctx: LoopContext| {
                let tools_ctx = ToolsContext {
                    turn_id: loop_ctx.turn_id,
                    tool_call_id: loop_ctx.tool_call_id,
                    signal: tools_rs::builtin::AbortSignal::from_inner(
                        loop_ctx.signal.inner().clone(),
                    ),
                    metadata: loop_ctx.metadata,
                };
                let fut = execute(tools_ctx);
                Box::pin(async move {
                    let result: ToolsResult = fut.await;
                    Ok(result.into())
                })
            }),
        }))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tools_rs::builtin::{
        AbortSignal, ExecutableToolContext, ExecutableToolResult, ToolExecution,
    };
    use tools_rs::tool_accesses::ToolAccesses;

    struct EchoTool;
    impl BuiltinTool for EchoTool {
        fn name(&self) -> &str {
            "Echo"
        }
        fn description(&self) -> &str {
            "echo"
        }
        fn parameters(&self) -> serde_json::Value {
            json!({"type":"object"})
        }
        fn resolve_execution(
            &self,
            args: serde_json::Value,
        ) -> Result<ToolExecution, tools_rs::builtin::ToolError> {
            Ok(ToolExecution {
                accesses: ToolAccesses::none(),
                description: "echo".into(),
                approval_rule: "Echo".into(),
                matches_rule: None,
                display: None,
                execute: Box::new(move |ctx| {
                    let msg = format!(
                        "turn={} call={} arg={}",
                        ctx.turn_id,
                        ctx.tool_call_id,
                        args["msg"].as_str().unwrap_or("")
                    );
                    Box::pin(async move { ExecutableToolResult::ok_text(msg) })
                }),
            })
        }
    }

    #[tokio::test]
    async fn bridge_forwards_turn_and_call_id() {
        let bridge = ToolBridge::new(Arc::new(EchoTool));
        let exec = bridge.resolve_execution(json!({"msg":"hi"})).await.unwrap();
        match exec {
            LoopToolExecution::Runnable(r) => {
                let ctx = LoopContext {
                    turn_id: "3".into(),
                    tool_call_id: "call_1".into(),
                    metadata: None,
                    signal: kosong_rs::provider::AbortSignal::new(),
                    on_update: None,
                };
                let result = (r.execute)(ctx).await.unwrap();
                let text = match result {
                    crate::records::nested::ExecutableToolResult::Success(s) => s.output.to_text(),
                    crate::records::nested::ExecutableToolResult::Error(e) => {
                        panic!("unexpected error: {:?}", e)
                    }
                };
                assert_eq!(text, "turn=3 call=call_1 arg=hi");
            }
            _ => panic!("expected Runnable"),
        }
    }

    struct DisplayTool;
    impl BuiltinTool for DisplayTool {
        fn name(&self) -> &str {
            "Display"
        }
        fn description(&self) -> &str {
            "display"
        }
        fn parameters(&self) -> serde_json::Value {
            json!({"type":"object"})
        }
        fn resolve_execution(
            &self,
            _args: serde_json::Value,
        ) -> Result<ToolExecution, tools_rs::builtin::ToolError> {
            Ok(ToolExecution {
                accesses: ToolAccesses::none(),
                description: "display".into(),
                approval_rule: "Display".into(),
                matches_rule: None,
                display: Some(json!({"kind":"plan_review","plan":"x"})),
                execute: Box::new(|_ctx| {
                    Box::pin(async { ExecutableToolResult::ok_text("ok".into()) })
                }),
            })
        }
    }

    #[tokio::test]
    async fn bridge_forwards_display_from_tool_execution() {
        let bridge = ToolBridge::new(Arc::new(DisplayTool));
        let exec = bridge.resolve_execution(json!({})).await.unwrap();
        match exec {
            LoopToolExecution::Runnable(r) => {
                assert_eq!(r.display, Some(json!({"kind":"plan_review","plan":"x"})));
            }
            _ => panic!("expected Runnable"),
        }
    }
}
