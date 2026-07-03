use std::{env, fs, path::PathBuf};

use agent_rs::agent_loop::types::{ExecutableToolContext, ExecutableToolResult, UserToolExecutor};
use agent_rs::records::AgentRecord;
use agent_rs::tool::{ToolManager, ToolManagerContext, UserToolRegistration};
use serde_json::json;

struct NoopExecutor;
#[async_trait::async_trait]
impl UserToolExecutor for NoopExecutor {
    async fn execute_user_tool(
        &self,
        _name: &str,
        _args: serde_json::Value,
        _ctx: ExecutableToolContext,
    ) -> Result<ExecutableToolResult, anyhow::Error> {
        Ok(ExecutableToolResult::Success(
            agent_rs::records::nested::ExecutableToolSuccessResult {
                output: agent_rs::records::nested::ExecutableToolOutput::Text("noop".into()),
                is_error: None,
                stop_turn: None,
                message: None,
            },
        ))
    }
}

struct NoopCtx;

impl ToolManagerContext for NoopCtx {
    fn log_record(&mut self, _record: AgentRecord) {}
    fn emit_tool_list_updated(&mut self, _reason: &str, _server_name: Option<&str>) {}
    fn goal_mutation_tools_hidden(&self) -> bool {
        false
    }
    fn user_tool_executor(&self) -> std::sync::Arc<dyn UserToolExecutor> {
        std::sync::Arc::new(NoopExecutor)
    }
    fn mcp_tool_executor(&self) -> std::sync::Arc<dyn UserToolExecutor> {
        std::sync::Arc::new(NoopExecutor)
    }
}

fn main() {
    let mut mgr = ToolManager::new(NoopCtx);
    mgr.initialize_builtin_tools();
    mgr.register_user_tool(UserToolRegistration {
        name: "custom_user_tool".into(),
        description: "A user-registered tool for fixture generation.".into(),
        parameters: json!({"type": "object"}),
    });
    mgr.set_active_tools(&["Read".into(), "Grep".into(), "custom_user_tool".into()]);

    let infos = mgr.data();
    let out_dir = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap()
        .join("tests/fixtures");
    fs::create_dir_all(&out_dir).unwrap();
    fs::write(
        out_dir.join("tools-rust.json"),
        serde_json::to_string_pretty(&infos).unwrap(),
    )
    .unwrap();
}
