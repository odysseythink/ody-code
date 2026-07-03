use async_trait::async_trait;
use std::sync::Arc;
use tools_rs::{
    builtin::{
        BuiltinTool, ExecutableToolOutput, ExecutableToolResult, ToolError as BuiltinToolError,
        ToolExecution,
    },
    result_builder::ToolResultBuilder,
    schema::InputSchema,
    tool_accesses::ToolAccesses,
};

use super::providers::{UrlFetchKind, UrlFetcher};
use super::{ApprovalClient, Tool, ToolError, ToolResult};

/// Description text matching TS fetch-url.md
const DESCRIPTION: &str = "Fetch content from a URL. Returns the main text content extracted from the page. Use this when you need to read a specific web page.\n\nOnly public `http`/`https` URLs are supported. Requests to private, loopback, or link-local addresses are refused, and responses larger than 10 MiB are rejected.";

pub struct FetchURLTool {
    fetcher: Arc<dyn UrlFetcher>,
}

impl FetchURLTool {
    pub fn new(fetcher: Arc<dyn UrlFetcher>) -> Self {
        Self { fetcher }
    }
}

#[async_trait]
impl Tool for FetchURLTool {
    fn name(&self) -> &str {
        "FetchURL"
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn parameters(&self) -> serde_json::Value {
        InputSchema::object(vec![(
            "url",
            InputSchema::string().description("The URL to fetch content from."),
        )])
        .build()
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _approval: &dyn ApprovalClient,
    ) -> Result<ToolResult, ToolError> {
        let url =
            args.get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::ExecutionFailed {
                    message: "missing 'url' argument".into(),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "missing url",
                    )),
                })?;

        match self.fetcher.fetch(url, None).await {
            Ok(result) => {
                if result.content.is_empty() {
                    return Ok(serde_json::json!({
                        "output": "The response body is empty.",
                        "isError": false,
                    }));
                }
                let mut builder = ToolResultBuilder::new(None);
                builder.write(&result.content);
                let message = match result.kind {
                    UrlFetchKind::Passthrough => {
                        "The returned content is the full response body, returned verbatim."
                    }
                    UrlFetchKind::Extracted => {
                        "The returned content is the main text extracted from the page."
                    }
                };
                let tr = builder.ok(Some(message.to_string()));
                Ok(serde_json::to_value(tr).unwrap())
            }
            Err(e) => {
                // TS branches on `instanceof HttpFetchError` — mirror with status check
                let is_http_error = e.status > 0 && e.status < 600;
                let output = if is_http_error {
                    format!("Failed to fetch URL. Status: {}. {}", e.status, e.message)
                } else {
                    format!(
                        "Failed to fetch URL due to network error: {}. {}",
                        url, e.message
                    )
                };
                Ok(serde_json::json!({
                    "output": output,
                    "isError": true,
                }))
            }
        }
    }
}

impl BuiltinTool for FetchURLTool {
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
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| BuiltinToolError::InvalidArgs("missing 'url' argument".into()))?
            .to_string();
        let fetcher = Arc::clone(&self.fetcher);

        Ok(ToolExecution {
            accesses: ToolAccesses::none(),
            description: format!("Fetching {}", url),
            approval_rule: "FetchURL".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx| {
                let fetcher = Arc::clone(&fetcher);
                let url = url.clone();
                Box::pin(async move {
                    match fetcher.fetch(&url, None).await {
                        Ok(result) => {
                            if result.content.is_empty() {
                                return ExecutableToolResult {
                                    output: ExecutableToolOutput::Text(
                                        "The response body is empty.".into(),
                                    ),
                                    message: None,
                                    is_error: false,
                                    stop_turn: None,
                                };
                            }
                            let message = match result.kind {
                                UrlFetchKind::Passthrough =>
                                    "The returned content is the full response body, returned verbatim.",
                                UrlFetchKind::Extracted =>
                                    "The returned content is the main text extracted from the page.",
                            };
                            ExecutableToolResult {
                                output: ExecutableToolOutput::Text(result.content),
                                message: Some(message.into()),
                                is_error: false,
                                stop_turn: None,
                            }
                        }
                        Err(e) => {
                            let is_http_error = e.status > 0 && e.status < 600;
                            let output = if is_http_error {
                                format!("Failed to fetch URL. Status: {}. {}", e.status, e.message)
                            } else {
                                format!(
                                    "Failed to fetch URL due to network error: {}. {}",
                                    url, e.message
                                )
                            };
                            ExecutableToolResult {
                                output: ExecutableToolOutput::Text(output),
                                message: Some("Fetch failed".into()),
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
    use super::super::providers::{HttpFetchError, UrlFetchResult};
    use super::super::{ApprovalDecision, ApprovalRequest, ApprovalResponse};
    use super::*;
    use std::sync::Mutex;

    struct MockUrlFetcher {
        result: Mutex<Option<Result<UrlFetchResult, HttpFetchError>>>,
    }

    impl MockUrlFetcher {
        fn new(result: Result<UrlFetchResult, HttpFetchError>) -> Self {
            Self {
                result: Mutex::new(Some(result)),
            }
        }
    }

    #[async_trait]
    impl UrlFetcher for MockUrlFetcher {
        async fn fetch(
            &self,
            _url: &str,
            _tool_call_id: Option<&str>,
        ) -> Result<UrlFetchResult, HttpFetchError> {
            self.result.lock().unwrap().take().unwrap()
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

    fn make_tool(result: Result<UrlFetchResult, HttpFetchError>) -> FetchURLTool {
        FetchURLTool::new(Arc::new(MockUrlFetcher::new(result)))
    }

    #[tokio::test]
    async fn fetch_url_passthrough_returns_content_with_message() {
        let tool = make_tool(Ok(UrlFetchResult {
            content: "plain text".into(),
            kind: UrlFetchKind::Passthrough,
        }));
        let result = tool
            .execute(
                serde_json::json!({"url": "https://example.com"}),
                &NoopApproval,
            )
            .await
            .unwrap();
        let r: serde_json::Value = result;
        assert_eq!(r["isError"], false);
        assert!(r["output"].as_str().unwrap().contains("plain text"));
        assert!(r["message"].as_str().unwrap().contains("verbatim"));
    }

    #[tokio::test]
    async fn fetch_url_extracted_returns_content_with_message() {
        let tool = make_tool(Ok(UrlFetchResult {
            content: "# Title\n\narticle".into(),
            kind: UrlFetchKind::Extracted,
        }));
        let result = tool
            .execute(
                serde_json::json!({"url": "https://example.com"}),
                &NoopApproval,
            )
            .await
            .unwrap();
        let r: serde_json::Value = result;
        assert_eq!(r["isError"], false);
        assert!(r["message"].as_str().unwrap().contains("extracted"));
    }

    #[tokio::test]
    async fn fetch_url_empty_body_returns_message() {
        let tool = make_tool(Ok(UrlFetchResult {
            content: "".into(),
            kind: UrlFetchKind::Passthrough,
        }));
        let result = tool
            .execute(
                serde_json::json!({"url": "https://example.com"}),
                &NoopApproval,
            )
            .await
            .unwrap();
        let r: serde_json::Value = result;
        assert_eq!(r["isError"], false);
        assert!(r["output"].as_str().unwrap().contains("empty"));
    }

    #[tokio::test]
    async fn fetch_url_http_error_returns_status_in_output() {
        let tool = make_tool(Err(HttpFetchError::new(404, "Not Found")));
        let result = tool
            .execute(
                serde_json::json!({"url": "https://example.com"}),
                &NoopApproval,
            )
            .await
            .unwrap();
        let r: serde_json::Value = result;
        assert_eq!(r["isError"], true);
        let output = r["output"].as_str().unwrap();
        assert!(output.contains("Status: 404"));
        assert!(output.contains("Not Found"));
    }

    #[tokio::test]
    async fn fetch_url_network_error_mentions_url() {
        let tool = make_tool(Err(HttpFetchError::new(0, "connection refused")));
        let result = tool
            .execute(
                serde_json::json!({"url": "https://bad.example"}),
                &NoopApproval,
            )
            .await
            .unwrap();
        let r: serde_json::Value = result;
        assert_eq!(r["isError"], true);
        let output = r["output"].as_str().unwrap();
        assert!(output.contains("network error"));
        assert!(output.contains("https://bad.example"));
    }
}
