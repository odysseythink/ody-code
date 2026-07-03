use std::sync::{Arc, Mutex};

use agent_rs::agent_loop::types::{ExecutableToolContext, ExecutableToolResult, UserToolExecutor};
use agent_rs::records::AgentRecord;
use agent_rs::tool::{ToolManager, ToolManagerContext, ToolSource, UserToolRegistration};
use serde_json::json;

#[derive(Debug, Default)]
struct MockCtx {
    records: Mutex<Vec<AgentRecord>>,
    events: Mutex<Vec<(String, Option<String>)>>,
    hide_goal: bool,
}

struct MockUserToolExecutor;

#[async_trait::async_trait]
impl UserToolExecutor for MockUserToolExecutor {
    async fn execute_user_tool(
        &self,
        _name: &str,
        _args: serde_json::Value,
        _ctx: ExecutableToolContext,
    ) -> Result<ExecutableToolResult, anyhow::Error> {
        Ok(ExecutableToolResult::Success(
            agent_rs::records::nested::ExecutableToolSuccessResult {
                output: agent_rs::records::nested::ExecutableToolOutput::Text("ok".into()),
                is_error: None,
                stop_turn: None,
                message: None,
            },
        ))
    }
}

impl ToolManagerContext for MockCtx {
    fn log_record(&mut self, record: AgentRecord) {
        self.records.lock().unwrap().push(record);
    }

    fn emit_tool_list_updated(&mut self, reason: &str, server_name: Option<&str>) {
        self.events
            .lock()
            .unwrap()
            .push((reason.to_string(), server_name.map(|s| s.to_string())));
    }

    fn goal_mutation_tools_hidden(&self) -> bool {
        self.hide_goal
    }

    fn user_tool_executor(&self) -> Arc<dyn UserToolExecutor> {
        Arc::new(MockUserToolExecutor)
    }

    fn mcp_tool_executor(&self) -> Arc<dyn UserToolExecutor> {
        Arc::new(MockUserToolExecutor)
    }
}

#[test]
fn register_user_tool_logs_and_enables() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.register_user_tool(UserToolRegistration {
        name: "my_tool".into(),
        description: "does a thing".into(),
        parameters: json!({"type": "object"}),
    });

    assert!(mgr.is_tool_active("my_tool"));
    let ctx = mgr.into_inner();
    let records = ctx.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    match &records[0] {
        AgentRecord::ToolsRegisterUserTool { registration, .. } => {
            assert_eq!(registration.name, "my_tool");
        }
        _ => panic!("expected tools.register_user_tool record"),
    }
}

#[test]
fn unregister_user_tool_logs_and_removes() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.register_user_tool(UserToolRegistration {
        name: "my_tool".into(),
        description: "".into(),
        parameters: json!({}),
    });
    mgr.unregister_user_tool("my_tool");

    assert!(!mgr.is_tool_active("my_tool"));
    let ctx = mgr.into_inner();
    assert_eq!(ctx.records.lock().unwrap().len(), 2);
}

#[test]
fn set_active_tools_splits_exact_and_mcp_patterns() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.set_active_tools(&[
        "Read".into(),
        "my_tool".into(),
        "mcp__*".into(),
        "mcp__github__*".into(),
    ]);

    assert!(mgr.is_tool_active("Read") == false); // builtin not registered yet
    let data = mgr.data();
    assert!(data.iter().all(|i| i.source != ToolSource::Mcp));
    let ctx = mgr.into_inner();
    let records = ctx.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    match &records[0] {
        AgentRecord::ToolsSetActiveTools { names, .. } => {
            assert_eq!(names.len(), 4);
        }
        _ => panic!("expected tools.set_active_tools record"),
    }
}

#[test]
fn update_store_logs_and_retains_value() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.update_store("foo", json!({"bar": 1}));
    assert_eq!(mgr.store_data().get("foo").unwrap(), &json!({"bar": 1}));
    let ctx = mgr.into_inner();
    assert_eq!(ctx.records.lock().unwrap().len(), 1);
}

#[test]
fn initialize_builtin_tools_populates_core_tools() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.initialize_builtin_tools();
    let data = mgr.data();
    let names: Vec<_> = data.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"Read"));
    assert!(names.contains(&"Write"));
    assert!(names.contains(&"Edit"));
    assert!(names.contains(&"Glob"));
    assert!(names.contains(&"Grep"));
    assert!(names.contains(&"Bash"));
}

#[test]
fn loop_tools_sorted_and_includes_active_builtin() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.initialize_builtin_tools();
    mgr.set_active_tools(&["Write".into(), "Read".into(), "Grep".into()]);
    let tools = mgr.loop_tools();
    let names: Vec<_> = tools.iter().map(|t| t.name()).collect();
    assert_eq!(names, vec!["Grep", "Read", "Write"]);
}

#[test]
fn loop_tools_hides_goal_mutation_tools_when_no_goal() {
    let mut ctx = MockCtx::default();
    ctx.hide_goal = true;
    let mut mgr = ToolManager::new(ctx);
    mgr.register_user_tool(UserToolRegistration {
        name: "SetGoalBudget".into(),
        description: "".into(),
        parameters: json!({}),
    });
    mgr.register_user_tool(UserToolRegistration {
        name: "UpdateGoal".into(),
        description: "".into(),
        parameters: json!({}),
    });
    mgr.register_user_tool(UserToolRegistration {
        name: "Read".into(),
        description: "".into(),
        parameters: json!({}),
    });
    mgr.set_active_tools(&["SetGoalBudget".into(), "UpdateGoal".into(), "Read".into()]);
    let names: Vec<_> = mgr
        .loop_tools()
        .iter()
        .map(|t| t.name().to_string())
        .collect();
    assert_eq!(names, vec!["Read"]);
}

use kosong_rs::provider::Tool as KosongTool;

#[test]
fn register_mcp_server_qualifies_names_and_respects_enabled_filter() {
    let mut mgr = ToolManager::new(MockCtx::default());
    let result = mgr.register_mcp_server(
        "github",
        &[
            KosongTool {
                name: "list_repos".into(),
                description: "".into(),
                parameters: json!({}),
            },
            KosongTool {
                name: "create_issue".into(),
                description: "".into(),
                parameters: json!({}),
            },
        ],
        Some(&{
            let mut set = std::collections::HashSet::new();
            set.insert("list_repos".into());
            set
        }),
    );

    assert_eq!(result.registered, vec!["mcp__github__list_repos"]);
    assert!(result.collisions.is_empty());

    mgr.set_active_tools(&["mcp__github__*".into()]);
    let tools = mgr.loop_tools();
    let names: Vec<_> = tools.iter().map(|t| t.name()).collect();
    assert_eq!(names, vec!["mcp__github__list_repos"]);
}

#[test]
fn mcp_same_server_collisions_detected() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.register_mcp_server(
        "github",
        &[KosongTool {
            name: "list".into(),
            description: "".into(),
            parameters: json!({}),
        }],
        None,
    );

    let result = mgr.register_mcp_server(
        "github",
        &[
            KosongTool {
                name: "list".into(),
                description: "".into(),
                parameters: json!({}),
            },
            KosongTool {
                name: "list".into(),
                description: "".into(),
                parameters: json!({}),
            },
        ],
        None,
    );

    // register_mcp_server unregisters the old server first, so the first
    // duplicate is registered successfully and only the second collides.
    assert_eq!(result.collisions.len(), 1);
    assert!(result.collisions.iter().all(|c| matches!(
        c.collides_with,
        agent_rs::tool::McpCollisionTarget::SameServer { .. }
    )));
}

#[test]
fn mcp_other_server_collision_detected_with_matching_qualified_name() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.register_mcp_server(
        "github",
        &[KosongTool {
            name: "_list".into(),
            description: "".into(),
            parameters: json!({}),
        }],
        None,
    );

    // Use a synthetic server name that produces the same qualified name.
    let result = mgr.register_mcp_server(
        "github_",
        &[KosongTool {
            name: "list".into(),
            description: "".into(),
            parameters: json!({}),
        }],
        None,
    );

    assert!(result.collisions.iter().any(|c| matches!(
        c.collides_with,
        agent_rs::tool::McpCollisionTarget::OtherServer { .. }
    )));
}

#[test]
fn unregister_mcp_server_removes_tools() {
    let mut mgr = ToolManager::new(MockCtx::default());
    mgr.register_mcp_server(
        "github",
        &[KosongTool {
            name: "list".into(),
            description: "".into(),
            parameters: json!({}),
        }],
        None,
    );
    mgr.set_active_tools(&["mcp__github__*".into()]);
    assert_eq!(mgr.loop_tools().len(), 1);

    mgr.unregister_mcp_server("github");
    assert_eq!(mgr.loop_tools().len(), 0);
}
