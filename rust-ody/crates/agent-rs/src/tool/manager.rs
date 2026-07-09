use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::Value as JsonValue;

use super::types::{
    BuiltinToolProvisionContext, BuiltinToolsProvider, ExecutableTool as ToolDefinition,
    McpServerRegistrationResult, McpToolCollision, ToolInfo, ToolSource, UserToolRegistration,
};
use crate::agent_loop::types::{
    ExecutableTool as LoopExecutableTool, UserToolExecutable, UserToolExecutor,
};
use crate::records::nested::ToolStoreUpdate;
use crate::records::AgentRecord;

/// Minimal Agent surface required by `ToolManager`.
pub trait ToolManagerContext: Send + Sync {
    fn log_record(&mut self, record: AgentRecord);
    fn emit_tool_list_updated(&mut self, reason: &str, server_name: Option<&str>);
    fn goal_mutation_tools_hidden(&self) -> bool;
    fn user_tool_executor(&self) -> Arc<dyn UserToolExecutor>;
    fn mcp_tool_executor(&self) -> Arc<dyn UserToolExecutor>;
}

struct McpToolEntry {
    tool: Arc<dyn LoopExecutableTool>,
    server_name: String,
}

pub struct ToolManager<C: ToolManagerContext> {
    context: C,
    builtin_tools: HashMap<String, Arc<dyn LoopExecutableTool>>,
    user_tools: HashMap<String, Arc<dyn LoopExecutableTool>>,
    mcp_tools: HashMap<String, McpToolEntry>,
    mcp_tools_by_server: HashMap<String, Vec<String>>,
    enabled_tools: HashSet<String>,
    mcp_access_patterns: Vec<String>,
    store: HashMap<String, JsonValue>,
}

impl<C: ToolManagerContext> ToolManager<C> {
    pub fn new(context: C) -> Self {
        Self {
            context,
            builtin_tools: HashMap::new(),
            user_tools: HashMap::new(),
            mcp_tools: HashMap::new(),
            mcp_tools_by_server: HashMap::new(),
            enabled_tools: HashSet::new(),
            mcp_access_patterns: Vec::new(),
            store: HashMap::new(),
        }
    }

    pub fn register_builtin(&mut self, tool: Arc<dyn LoopExecutableTool>) {
        self.builtin_tools.insert(tool.name().to_owned(), tool);
    }

    pub fn register_user_tool(&mut self, input: UserToolRegistration) {
        self.context.log_record(AgentRecord::ToolsRegisterUserTool {
            time: None,
            registration: input.clone(),
        });
        self.register_user_tool_without_log(&input);
    }

    pub(crate) fn register_user_tool_without_log(&mut self, input: &UserToolRegistration) {
        let info = ToolDefinition {
            name: input.name.clone(),
            description: input.description.clone(),
            parameters: input.parameters.clone(),
        };
        let tool: Arc<dyn LoopExecutableTool> = Arc::new(UserToolExecutable::new(
            info,
            self.context.user_tool_executor(),
        ));
        self.user_tools.insert(input.name.clone(), tool);
        self.enabled_tools.insert(input.name.clone());
    }

    pub fn unregister_user_tool(&mut self, name: &str) {
        self.context
            .log_record(AgentRecord::ToolsUnregisterUserTool {
                time: None,
                name: name.to_owned(),
            });
        self.unregister_user_tool_without_log(name);
    }

    pub(crate) fn unregister_user_tool_without_log(&mut self, name: &str) {
        self.user_tools.remove(name);
        self.enabled_tools.remove(name);
    }

    pub fn inherit_user_tools(&mut self, parent: &ToolManager<C>) {
        for tool in parent.user_tools.values() {
            if !parent.enabled_tools.contains(tool.name()) {
                continue;
            }
            self.register_user_tool(UserToolRegistration {
                name: tool.name().to_owned(),
                description: tool.description().to_owned(),
                parameters: tool.parameters(),
            });
        }
    }

    pub fn set_active_tools(&mut self, names: &[String]) {
        self.context.log_record(AgentRecord::ToolsSetActiveTools {
            time: None,
            names: names.to_vec(),
        });
        self.set_active_tools_without_log(names);
    }

    pub(crate) fn set_active_tools_without_log(&mut self, names: &[String]) {
        self.enabled_tools = names
            .iter()
            .filter(|n| !is_mcp_pattern(n))
            .cloned()
            .collect();
        self.mcp_access_patterns = names
            .iter()
            .filter(|n| is_mcp_pattern(n))
            .cloned()
            .collect();
    }

    pub fn is_tool_active(&self, name: &str) -> bool {
        self.enabled_tools.contains(name)
            && (self.builtin_tools.contains_key(name)
                || self.user_tools.contains_key(name)
                || self.mcp_tools.contains_key(name))
    }

    pub fn data(&self) -> Vec<ToolInfo> {
        let mut infos: Vec<ToolInfo> = self
            .builtin_tools
            .values()
            .map(|t| ToolInfo {
                name: t.name().to_owned(),
                description: t.description().to_owned(),
                active: self.enabled_tools.contains(t.name()),
                source: ToolSource::Builtin,
            })
            .chain(self.user_tools.values().map(|t| ToolInfo {
                name: t.name().to_owned(),
                description: t.description().to_owned(),
                active: self.enabled_tools.contains(t.name()),
                source: ToolSource::User,
            }))
            .chain(self.mcp_tools.values().map(|entry| ToolInfo {
                name: entry.tool.name().to_owned(),
                description: entry.tool.description().to_owned(),
                active: self.is_mcp_tool_enabled(&entry.tool.name()),
                source: ToolSource::Mcp,
            }))
            .collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos
    }

    pub fn store_data(&self) -> HashMap<String, JsonValue> {
        self.store.clone()
    }

    pub fn update_store(&mut self, key: &str, value: JsonValue) {
        self.context.log_record(AgentRecord::ToolsUpdateStore {
            time: None,
            update: ToolStoreUpdate {
                key: key.to_owned(),
                value: value.clone(),
            },
        });
        self.update_store_without_log(key, value);
    }

    pub(crate) fn update_store_without_log(&mut self, key: &str, value: JsonValue) {
        self.store.insert(key.to_owned(), value);
    }

    pub fn initialize_builtin_tools(&mut self) {
        for tool in core_builtin_tools() {
            self.register_builtin(Arc::new(tool));
        }
    }

    pub fn sync_builtins(
        &mut self,
        provider: &dyn BuiltinToolsProvider,
        ctx: BuiltinToolProvisionContext,
    ) {
        self.builtin_tools.clear();
        for tool in core_builtin_tools() {
            self.register_builtin(Arc::new(tool));
        }
        for tool in provider.provide(ctx) {
            let name = tool.name().to_owned();
            self.register_builtin(tool);
            self.enabled_tools.insert(name);
        }
    }

    pub fn loop_tools(&self) -> Vec<Arc<dyn LoopExecutableTool>> {
        let mut names: Vec<String> = self.enabled_tools.iter().cloned().collect();
        let mcp_names: Vec<String> = self
            .mcp_tools
            .keys()
            .filter(|name| self.is_mcp_tool_enabled(name))
            .cloned()
            .collect();
        names.extend(mcp_names);
        names.sort_unstable();
        names.dedup();

        if self.context.goal_mutation_tools_hidden() {
            names.retain(|name| name != "SetGoalBudget" && name != "UpdateGoal");
        }

        names
            .into_iter()
            .filter_map(|name| {
                self.user_tools
                    .get(&name)
                    .cloned()
                    .or_else(|| {
                        self.mcp_tools
                            .get(&name)
                            .map(|entry| Arc::clone(&entry.tool))
                    })
                    .or_else(|| self.builtin_tools.get(&name).cloned())
            })
            .collect()
    }

    pub fn register_mcp_server(
        &mut self,
        server_name: &str,
        tools: &[kosong_rs::provider::Tool],
        enabled_tools: Option<&HashSet<String>>,
    ) -> McpServerRegistrationResult {
        self.unregister_mcp_server(server_name);

        let mut registered: Vec<String> = Vec::new();
        let mut collisions: Vec<McpToolCollision> = Vec::new();
        let mut seen_in_this_call: HashMap<String, String> = HashMap::new();
        let executor = self.context.mcp_tool_executor();

        for tool in tools {
            if let Some(enabled) = enabled_tools {
                if !enabled.contains(&tool.name) {
                    continue;
                }
            }

            let qualified = qualify_mcp_tool_name(server_name, &tool.name);

            if let Some(first_name) = seen_in_this_call.get(&qualified) {
                collisions.push(McpToolCollision {
                    qualified: qualified.clone(),
                    tool_name: tool.name.clone(),
                    collides_with: super::types::McpCollisionTarget::SameServer {
                        tool_name: first_name.clone(),
                    },
                });
                continue;
            }

            if let Some(existing) = self.mcp_tools.get(&qualified) {
                collisions.push(McpToolCollision {
                    qualified: qualified.clone(),
                    tool_name: tool.name.clone(),
                    collides_with: super::types::McpCollisionTarget::OtherServer {
                        server_name: existing.server_name.clone(),
                    },
                });
                continue;
            }

            seen_in_this_call.insert(qualified.clone(), tool.name.clone());
            let wrapped: Arc<dyn LoopExecutableTool> = Arc::new(UserToolExecutable::new(
                ToolDefinition {
                    name: qualified.clone(),
                    description: tool.description.clone(),
                    parameters: tool.parameters.clone(),
                },
                Arc::clone(&executor),
            ));
            self.mcp_tools.insert(
                qualified.clone(),
                McpToolEntry {
                    tool: wrapped,
                    server_name: server_name.to_owned(),
                },
            );
            registered.push(qualified);
        }

        self.mcp_tools_by_server
            .insert(server_name.to_owned(), registered.clone());

        McpServerRegistrationResult {
            registered,
            collisions,
        }
    }

    pub fn unregister_mcp_server(&mut self, server_name: &str) -> bool {
        let Some(existing) = self.mcp_tools_by_server.remove(server_name) else {
            return false;
        };
        for qualified in existing {
            self.mcp_tools.remove(&qualified);
        }
        true
    }

    pub fn into_inner(self) -> C {
        self.context
    }

    fn is_mcp_tool_enabled(&self, name: &str) -> bool {
        self.mcp_access_patterns
            .iter()
            .any(|pattern| matches_mcp_pattern(name, pattern))
    }
}

fn is_mcp_pattern(name: &str) -> bool {
    name.starts_with("mcp__")
}

fn qualify_mcp_tool_name(server_name: &str, tool_name: &str) -> String {
    format!("mcp__{}__{}", server_name, tool_name)
}

fn matches_mcp_pattern(name: &str, pattern: &str) -> bool {
    if pattern == name {
        return true;
    }
    if pattern.ends_with('*') && name.starts_with(&pattern[..pattern.len() - 1]) {
        return true;
    }
    false
}

fn core_builtin_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "Read".into(),
            description: "Read a text file from the local filesystem.".into(),
            parameters: json_schema_object(&["path"]),
        },
        ToolDefinition {
            name: "Write".into(),
            description: "Write or overwrite a text file.".into(),
            parameters: json_schema_object(&["path", "content"]),
        },
        ToolDefinition {
            name: "Edit".into(),
            description: "Apply a targeted edit to a text file.".into(),
            parameters: json_schema_object(&["path", "old_string", "new_string"]),
        },
        ToolDefinition {
            name: "Glob".into(),
            description: "Find files matching a glob pattern.".into(),
            parameters: json_schema_object(&["pattern"]),
        },
        ToolDefinition {
            name: "Grep".into(),
            description: "Search file contents with a regex.".into(),
            parameters: json_schema_object(&["pattern", "path"]),
        },
        ToolDefinition {
            name: "Bash".into(),
            description: "Execute a shell command.".into(),
            parameters: json_schema_object(&["command"]),
        },
        ToolDefinition {
            name: "FetchURL".into(),
            description:
                "Fetch content from a URL. Returns the main text content extracted from the page."
                    .into(),
            parameters: json_schema_object(&["url"]),
        },
        ToolDefinition {
            name: "WebSearch".into(),
            description: "Search the web for information.".into(),
            parameters: json_schema_object(&["query"]),
        },
    ]
}

fn json_schema_object(required: &[&str]) -> JsonValue {
    serde_json::json!({
        "type": "object",
        "required": required,
        "properties": {}
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_loop::types::{ExecutableTool as LoopExecutableTool, UserToolExecutor};
    use serde_json::json;
    use std::sync::Arc;

    struct NoopExecutor;
    #[async_trait::async_trait]
    impl UserToolExecutor for NoopExecutor {
        async fn execute_user_tool(
            &self,
            _name: &str,
            _args: JsonValue,
            _ctx: crate::agent_loop::types::ExecutableToolContext,
        ) -> Result<crate::agent_loop::types::ExecutableToolResult, anyhow::Error> {
            Ok(crate::agent_loop::types::ExecutableToolResult::Success(
                crate::records::nested::ExecutableToolSuccessResult {
                    output: crate::records::nested::ExecutableToolOutput::Text("ok".into()),
                    is_error: None,
                    stop_turn: None,
                    message: None,
                },
            ))
        }
    }

    struct HideGoalsCtx;
    impl ToolManagerContext for HideGoalsCtx {
        fn log_record(&mut self, _r: AgentRecord) {}
        fn emit_tool_list_updated(&mut self, _reason: &str, _server_name: Option<&str>) {}
        fn goal_mutation_tools_hidden(&self) -> bool {
            true
        }
        fn user_tool_executor(&self) -> Arc<dyn UserToolExecutor> {
            Arc::new(NoopExecutor)
        }
        fn mcp_tool_executor(&self) -> Arc<dyn UserToolExecutor> {
            Arc::new(NoopExecutor)
        }
    }

    struct ShowGoalsCtx;
    impl ToolManagerContext for ShowGoalsCtx {
        fn log_record(&mut self, _r: AgentRecord) {}
        fn emit_tool_list_updated(&mut self, _reason: &str, _server_name: Option<&str>) {}
        fn goal_mutation_tools_hidden(&self) -> bool {
            false
        }
        fn user_tool_executor(&self) -> Arc<dyn UserToolExecutor> {
            Arc::new(NoopExecutor)
        }
        fn mcp_tool_executor(&self) -> Arc<dyn UserToolExecutor> {
            Arc::new(NoopExecutor)
        }
    }

    fn stub_tool(name: &str) -> Arc<dyn LoopExecutableTool> {
        Arc::new(crate::tool::types::ExecutableTool {
            name: name.into(),
            description: format!("{} tool", name),
            parameters: json!({"type": "object"}),
        })
    }

    #[test]
    fn loop_tools_sorts_and_hides_goal_mutation_tools() {
        let mut mgr = ToolManager::new(HideGoalsCtx);
        mgr.register_builtin(stub_tool("Zeta"));
        mgr.register_builtin(stub_tool("Alpha"));
        mgr.register_builtin(stub_tool("SetGoalBudget"));
        mgr.register_builtin(stub_tool("UpdateGoal"));
        mgr.set_active_tools(&[
            "Zeta".into(),
            "Alpha".into(),
            "SetGoalBudget".into(),
            "UpdateGoal".into(),
        ]);

        let names: Vec<String> = mgr
            .loop_tools()
            .iter()
            .map(|t| t.name().to_string())
            .collect();
        assert_eq!(names, vec!["Alpha", "Zeta"]);
    }

    #[test]
    fn loop_tools_shows_goal_mutation_tools_when_not_hidden() {
        let mut mgr = ToolManager::new(ShowGoalsCtx);
        mgr.register_builtin(stub_tool("SetGoalBudget"));
        mgr.set_active_tools(&["SetGoalBudget".into()]);
        assert_eq!(mgr.loop_tools().len(), 1);
    }

    #[test]
    fn sync_builtins_replaces_builtins_and_preserves_user_tools() {
        struct Ctx;
        impl ToolManagerContext for Ctx {
            fn log_record(&mut self, _r: AgentRecord) {}
            fn emit_tool_list_updated(&mut self, _reason: &str, _server_name: Option<&str>) {}
            fn goal_mutation_tools_hidden(&self) -> bool {
                false
            }
            fn user_tool_executor(&self) -> Arc<dyn UserToolExecutor> {
                Arc::new(NoopExecutor)
            }
            fn mcp_tool_executor(&self) -> Arc<dyn UserToolExecutor> {
                Arc::new(NoopExecutor)
            }
        }

        struct TestProvider;
        impl BuiltinToolsProvider for TestProvider {
            fn provide(
                &self,
                _ctx: BuiltinToolProvisionContext,
            ) -> Vec<Arc<dyn LoopExecutableTool>> {
                vec![
                    Arc::new(crate::tool::types::ExecutableTool {
                        name: "Beta".into(),
                        description: "second".into(),
                        parameters: json!({"type":"object"}),
                    }),
                    Arc::new(crate::tool::types::ExecutableTool {
                        name: "Alpha".into(),
                        description: "first".into(),
                        parameters: json!({"type":"object"}),
                    }),
                ]
            }
        }

        let mut mgr = ToolManager::new(Ctx);
        mgr.initialize_builtin_tools();
        mgr.register_user_tool(UserToolRegistration {
            name: "User".into(),
            description: "user tool".into(),
            parameters: json!({"type":"object"}),
        });
        mgr.sync_builtins(
            &TestProvider,
            BuiltinToolProvisionContext {
                agent_type: crate::agent::AgentType::Main,
                model_capabilities: kosong_rs::provider::ModelCapability::unknown(),
                homedir: None,
                goal_command_enabled: false,
                rpc_open_external: false,
                rpc_request_question: false,
                background_available: false,
                cron_available: false,
                has_invocable_skills: false,
                subagent_host_available: false,
                web_searcher_available: false,
                url_fetcher_available: false,
            },
        );
        mgr.set_active_tools(&["Alpha".into(), "User".into()]);

        let names: Vec<String> = mgr
            .loop_tools()
            .into_iter()
            .map(|t| t.name().to_string())
            .collect();
        assert_eq!(names, vec!["Alpha", "User"]);
        assert!(mgr
            .data()
            .iter()
            .any(|info| info.name == "Beta" && !info.active));
        assert!(mgr
            .data()
            .iter()
            .any(|info| info.name == "User" && info.active));
    }
}
