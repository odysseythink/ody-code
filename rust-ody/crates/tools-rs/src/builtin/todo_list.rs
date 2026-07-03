use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};
use crate::store::ToolStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TodoItem {
    title: String,
    status: TodoStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TodoStatus {
    Pending,
    #[serde(rename = "in_progress")]
    InProgress,
    Done,
}

const TODO_STORE_KEY: &str = "todo";
const WRITE_REMINDER: &str = "Ensure that you continue to use the todo list to track progress. Mark tasks done immediately after finishing them, and keep exactly one task in_progress when work is underway.";

pub struct TodoListTool {
    store: Arc<dyn ToolStore>,
}

impl TodoListTool {
    pub fn new(store: Arc<dyn ToolStore>) -> Self {
        Self { store }
    }
}

fn render_todo_list(items: &[TodoItem]) -> String {
    if items.is_empty() {
        return "Todo list is empty.".into();
    }
    let mut lines = vec!["Current todo list:".to_string()];
    for item in items {
        let marker = match item.status {
            TodoStatus::Pending => "[pending]",
            TodoStatus::InProgress => "[in_progress]",
            TodoStatus::Done => "[done]",
        };
        lines.push(format!("  {} {}", marker, item.title));
    }
    lines.join("\n")
}

impl BuiltinTool for TodoListTool {
    fn name(&self) -> &str {
        "TodoList"
    }
    fn description(&self) -> &str {
        "Maintain a structured TODO list. Omit todos to read, pass empty array to clear."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "title": { "type": "string" },
                            "status": { "type": "string", "enum": ["pending", "in_progress", "done"] }
                        },
                        "required": ["title", "status"]
                    },
                    "description": "Updated todo list. Omit to read, empty array to clear."
                }
            },
            "additionalProperties": false
        })
    }
    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let todos_arg: Option<Value> = args.get("todos").cloned();
        let is_query = todos_arg.is_none();
        let store = Arc::clone(&self.store);
        let description = if is_query {
            "Reading todo list".into()
        } else if todos_arg
            .as_ref()
            .and_then(|a| a.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(false)
        {
            "Clearing todo list".into()
        } else {
            "Updating todo list".into()
        };
        Ok(ToolExecution {
            accesses: Default::default(),
            description,
            approval_rule: "TodoList".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let store = Arc::clone(&store);
                let todos_arg_clone = todos_arg.clone();
                Box::pin(async move {
                    if let Some(todos_val) = todos_arg_clone {
                        let items: Vec<TodoItem> =
                            serde_json::from_value(todos_val).unwrap_or_default();
                        if items.is_empty() {
                            store.set(
                                TODO_STORE_KEY,
                                serde_json::to_value(Vec::<TodoItem>::new()).unwrap_or_default(),
                            );
                            ExecutableToolResult::ok_text("Todo list cleared.".into())
                        } else {
                            store.set(
                                TODO_STORE_KEY,
                                serde_json::to_value(&items).unwrap_or_default(),
                            );
                            let rendered = render_todo_list(&items);
                            ExecutableToolResult::ok_text(format!(
                                "Todo list updated.\n{}\n\n{}",
                                rendered, WRITE_REMINDER
                            ))
                        }
                    } else {
                        let items: Vec<TodoItem> = store
                            .get(TODO_STORE_KEY)
                            .and_then(|v| serde_json::from_value(v).ok())
                            .unwrap_or_default();
                        ExecutableToolResult::ok_text(render_todo_list(&items))
                    }
                })
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MockToolStore;
    use std::sync::Arc;

    #[test]
    fn reads_empty_todo_list() {
        let store = MockToolStore::new();
        let tool = TodoListTool::new(Arc::new(store));
        let args = serde_json::json!({});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("Todo list is empty"));
    }

    #[test]
    fn updates_todo_list() {
        let store = MockToolStore::new();
        let tool = TodoListTool::new(Arc::new(store));
        let args = serde_json::json!({
            "todos": [
                {"title": "Task 1", "status": "pending"},
                {"title": "Task 2", "status": "in_progress"}
            ]
        });
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        }));
        assert!(!result.is_error);
        let text = result.to_text();
        assert!(text.contains("[pending] Task 1"));
        assert!(text.contains("[in_progress] Task 2"));
    }

    #[test]
    fn clears_todo_list() {
        let store = MockToolStore::new();
        let tool = TodoListTool::new(Arc::new(store));
        // First add some items
        let args = serde_json::json!({
            "todos": [{"title": "Task 1", "status": "pending"}]
        });
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        }));
        // Then clear
        let args = serde_json::json!({"todos": []});
        let exec = tool.resolve_execution(args).unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("Todo list cleared"));
    }
}
