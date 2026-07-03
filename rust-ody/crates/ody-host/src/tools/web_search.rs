use async_trait::async_trait;
use std::sync::Arc;
use tools_rs::{
    builtin::{
        BuiltinTool, ExecutableToolOutput, ExecutableToolResult, ToolError as BuiltinToolError,
        ToolExecution,
    },
    result_builder::ToolResultBuilder,
    tool_accesses::ToolAccesses,
};

use super::providers::{WebSearchOptions, WebSearchProvider};
use super::{ApprovalClient, Tool, ToolError, ToolResult};

/// Description text matching TS web-search.md
const DESCRIPTION: &str = "Search the web for information. Use this when you need up-to-date information from the internet.\n\nEach result includes its title, URL, snippet, and—when available—a publication date. When `include_content` is enabled, the full page content—when available—is appended after the snippet.";

pub struct WebSearchTool {
    provider: Arc<dyn WebSearchProvider>,
}

impl WebSearchTool {
    pub fn new(provider: Arc<dyn WebSearchProvider>) -> Self {
        Self { provider }
    }
}

/// Maps a thrown search error to a categorised, human-readable message.
/// Mirrors TS `classifySearchError()` in `web-search.ts:135-158`.
fn classify_search_error(error: &(dyn std::error::Error + Send)) -> String {
    let msg = error.to_string();
    let lower = msg.to_lowercase();

    if lower.contains("abort") {
        return format!("Search cancelled: {msg}");
    }
    if lower.contains("timed out") || lower.contains("timeout") {
        return format!("Search timed out: {msg}");
    }
    if lower.contains("401") || lower.contains("unauthorized") || lower.contains("auth") {
        return format!("Search failed (authentication): {msg}");
    }
    if lower.contains("network") || lower.contains("fetch") {
        return format!("Search failed (network): {msg}");
    }
    format!("Search failed: {msg}")
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "WebSearch"
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn parameters(&self) -> serde_json::Value {
        tools_rs::schema::InputSchema::object(vec![
            (
                "query",
                tools_rs::schema::InputSchema::string()
                    .description("The query text to search for."),
            ),
            (
                "limit",
                tools_rs::schema::InputSchema::integer()
                    .min(1.0)
                    .max(20.0)
                    .default(serde_json::json!(5))
                    .description("The number of results to return."),
            ),
            (
                "include_content",
                tools_rs::schema::InputSchema::boolean()
                    .default(serde_json::json!(false))
                    .description("Whether to include the content of the web pages in the results."),
            ),
        ])
        .build()
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _approval: &dyn ApprovalClient,
    ) -> Result<ToolResult, ToolError> {
        let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::ExecutionFailed {
                message: "missing 'query' argument".into(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "missing query",
                )),
            }
        })?;

        let opts = WebSearchOptions {
            limit: args.get("limit").and_then(|v| v.as_u64()).map(|n| n as u32),
            include_content: args.get("include_content").and_then(|v| v.as_bool()),
            tool_call_id: None,
        };

        match self.provider.search(query, Some(opts)).await {
            Ok(results) => {
                if results.is_empty() {
                    let mut builder = ToolResultBuilder::new(None);
                    builder.write("No search results found.");
                    let tr = builder.ok(None);
                    return Ok(serde_json::to_value(tr).unwrap());
                }
                let mut output = String::new();
                let mut first = true;
                for r in &results {
                    if !first {
                        output.push_str("---\n\n");
                    }
                    first = false;
                    output.push_str(&format!("Title: {}\n", r.title));
                    if let Some(ref date) = r.date {
                        output.push_str(&format!("Date: {}\n", date));
                    }
                    output.push_str(&format!("URL: {}\n", r.url));
                    output.push_str(&format!("Snippet: {}\n\n", r.snippet));
                    if let Some(ref content) = r.content {
                        output.push_str(&format!("{}\n\n", content));
                    }
                }
                let mut builder = ToolResultBuilder::new(None);
                builder.write(&output);
                let tr = builder.ok(None);
                Ok(serde_json::to_value(tr).unwrap())
            }
            Err(e) => {
                let output = classify_search_error(&*e);
                Ok(serde_json::json!({
                    "output": output,
                    "isError": true,
                }))
            }
        }
    }
}

impl BuiltinTool for WebSearchTool {
    fn name(&self) -> &str {
        Tool::name(self)
    }
    fn description(&self) -> &str {
        Tool::description(self)
    }
    fn parameters(&self) -> serde_json::Value {
        Tool::parameters(self)
    }

    fn resolve_execution(
        &self,
        args: serde_json::Value,
    ) -> Result<ToolExecution, BuiltinToolError> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| BuiltinToolError::InvalidArgs("missing 'query' argument".into()))?
            .to_string();
        let opts = WebSearchOptions {
            limit: args.get("limit").and_then(|v| v.as_u64()).map(|n| n as u32),
            include_content: args.get("include_content").and_then(|v| v.as_bool()),
            tool_call_id: None,
        };
        let provider = Arc::clone(&self.provider);

        Ok(ToolExecution {
            accesses: ToolAccesses::none(),
            description: format!("Searching the web for '{}'", query),
            approval_rule: "WebSearch".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx| {
                let provider = Arc::clone(&provider);
                let query = query.clone();
                let opts = opts.clone();
                Box::pin(async move {
                    match provider.search(&query, Some(opts)).await {
                        Ok(results) => {
                            if results.is_empty() {
                                return ExecutableToolResult {
                                    output: ExecutableToolOutput::Text(
                                        "No search results found.".into(),
                                    ),
                                    message: None,
                                    is_error: false,
                                    stop_turn: None,
                                };
                            }
                            let mut output = String::new();
                            let mut first = true;
                            for r in &results {
                                if !first {
                                    output.push_str("---\n\n");
                                }
                                first = false;
                                output.push_str(&format!("Title: {}\n", r.title));
                                if let Some(ref date) = r.date {
                                    output.push_str(&format!("Date: {}\n", date));
                                }
                                output.push_str(&format!("URL: {}\n", r.url));
                                output.push_str(&format!("Snippet: {}\n\n", r.snippet));
                                if let Some(ref content) = r.content {
                                    output.push_str(&format!("{}\n\n", content));
                                }
                            }
                            ExecutableToolResult {
                                output: ExecutableToolOutput::Text(output),
                                message: None,
                                is_error: false,
                                stop_turn: None,
                            }
                        }
                        Err(e) => {
                            let output = classify_search_error(&*e);
                            ExecutableToolResult {
                                output: ExecutableToolOutput::Text(output),
                                message: Some("Search failed".into()),
                                is_error: true,
                                stop_turn: None,
                            }
                        }
                    }
                })
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::providers::WebSearchResult;
    use super::super::{ApprovalDecision, ApprovalRequest, ApprovalResponse};
    use super::*;
    use std::sync::Mutex;

    struct MockWebSearchProvider {
        results: Mutex<Option<Result<Vec<WebSearchResult>, Box<dyn std::error::Error + Send>>>>,
    }

    impl MockWebSearchProvider {
        fn new(results: Result<Vec<WebSearchResult>, Box<dyn std::error::Error + Send>>) -> Self {
            Self {
                results: Mutex::new(Some(results)),
            }
        }
    }

    #[async_trait]
    impl WebSearchProvider for MockWebSearchProvider {
        async fn search(
            &self,
            _query: &str,
            _options: Option<WebSearchOptions>,
        ) -> Result<Vec<WebSearchResult>, Box<dyn std::error::Error + Send>> {
            self.results.lock().unwrap().take().unwrap()
        }
    }

    struct NoopApproval;
    #[async_trait]
    impl ApprovalClient for NoopApproval {
        async fn request(&self, _req: ApprovalRequest) -> Result<ApprovalResponse, ToolError> {
            Ok(ApprovalResponse {
                decision: ApprovalDecision::Approved,
            })
        }
    }

    fn make_tool(
        results: Result<Vec<WebSearchResult>, Box<dyn std::error::Error + Send>>,
    ) -> WebSearchTool {
        WebSearchTool::new(Arc::new(MockWebSearchProvider::new(results)))
    }

    #[tokio::test]
    async fn web_search_formats_single_result() {
        let tool = make_tool(Ok(vec![WebSearchResult {
            title: "Test".into(),
            url: "https://example.com".into(),
            snippet: "A snippet".into(),
            date: Some("2024-01-01".into()),
            content: None,
        }]));
        let result = tool
            .execute(serde_json::json!({"query": "test"}), &NoopApproval)
            .await
            .unwrap();
        let r: serde_json::Value = result;
        assert_eq!(r["isError"], false);
        let output = r["output"].as_str().unwrap();
        assert!(output.contains("Title: Test"));
        assert!(output.contains("Date: 2024-01-01"));
        assert!(output.contains("URL: https://example.com"));
        assert!(output.contains("Snippet: A snippet"));
    }

    #[tokio::test]
    async fn web_search_formats_multiple_results_with_separator() {
        let tool = make_tool(Ok(vec![
            WebSearchResult {
                title: "A".into(),
                url: "a".into(),
                snippet: "s".into(),
                date: None,
                content: None,
            },
            WebSearchResult {
                title: "B".into(),
                url: "b".into(),
                snippet: "s".into(),
                date: None,
                content: None,
            },
        ]));
        let result = tool
            .execute(serde_json::json!({"query": "test"}), &NoopApproval)
            .await
            .unwrap();
        let r: serde_json::Value = result;
        let output = r["output"].as_str().unwrap();
        assert!(output.contains("---"));
        assert!(output.contains("Title: A"));
        assert!(output.contains("Title: B"));
    }

    #[tokio::test]
    async fn web_search_empty_results_shows_message() {
        let tool = make_tool(Ok(vec![]));
        let result = tool
            .execute(serde_json::json!({"query": "test"}), &NoopApproval)
            .await
            .unwrap();
        let r: serde_json::Value = result;
        assert_eq!(r["isError"], false);
        assert!(r["output"]
            .as_str()
            .unwrap()
            .contains("No search results found"));
    }

    #[tokio::test]
    async fn web_search_includes_content_when_present() {
        let tool = make_tool(Ok(vec![WebSearchResult {
            title: "T".into(),
            url: "u".into(),
            snippet: "s".into(),
            date: None,
            content: Some("Full content here".into()),
        }]));
        let result = tool
            .execute(serde_json::json!({"query": "test"}), &NoopApproval)
            .await
            .unwrap();
        let r: serde_json::Value = result;
        assert!(r["output"].as_str().unwrap().contains("Full content here"));
    }

    #[tokio::test]
    async fn web_search_http_401_classified_as_auth() {
        let err: Box<dyn std::error::Error + Send> = Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "HTTP 401 Unauthorized",
        ));
        let tool = make_tool(Err(err));
        let result = tool
            .execute(serde_json::json!({"query": "test"}), &NoopApproval)
            .await
            .unwrap();
        let r: serde_json::Value = result;
        assert_eq!(r["isError"], true);
        assert!(r["output"].as_str().unwrap().contains("authentication"));
    }

    #[tokio::test]
    async fn web_search_network_error_classified() {
        let err: Box<dyn std::error::Error + Send> = Box::new(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "network down",
        ));
        let tool = make_tool(Err(err));
        let result = tool
            .execute(serde_json::json!({"query": "test"}), &NoopApproval)
            .await
            .unwrap();
        let r: serde_json::Value = result;
        assert_eq!(r["isError"], true);
        assert!(r["output"]
            .as_str()
            .unwrap()
            .contains("Search failed (network)"));
    }

    #[tokio::test]
    async fn web_search_generic_error() {
        let err: Box<dyn std::error::Error + Send> = Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "something broke",
        ));
        let tool = make_tool(Err(err));
        let result = tool
            .execute(serde_json::json!({"query": "test"}), &NoopApproval)
            .await
            .unwrap();
        let r: serde_json::Value = result;
        assert_eq!(r["isError"], true);
        assert!(r["output"].as_str().unwrap().starts_with("Search failed:"));
    }
}
