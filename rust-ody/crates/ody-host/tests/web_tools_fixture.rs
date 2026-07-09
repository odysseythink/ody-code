use async_trait::async_trait;
use ody_host::tools::fetch_url::FetchURLTool;
use ody_host::tools::providers::{
    HttpFetchError, UrlFetchKind, UrlFetchResult, UrlFetcher, WebSearchOptions, WebSearchProvider,
    WebSearchResult,
};
use ody_host::tools::web_search::WebSearchTool;
use ody_host::tools::{
    ApprovalClient, ApprovalDecision, ApprovalRequest, ApprovalResponse, Tool, ToolError,
};
use serde::Deserialize;
use std::sync::Arc;

// ── Fixture types ──

#[derive(Debug, Deserialize)]
struct FixtureFile {
    #[allow(dead_code)]
    version: u32,
    cases: Vec<WebCase>,
}

#[derive(Debug, Deserialize)]
struct WebCase {
    name: String,
    op: WebOp,
    #[serde(rename = "providerResult")]
    provider_result: ProviderResult,
    expected: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WebOp {
    #[serde(rename = "fetch_url")]
    FetchUrl { url: String },
    #[serde(rename = "web_search")]
    WebSearch { query: String },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ProviderResult {
    FetchOk { ok: FetchOkResult },
    FetchErr { err: FetchErrPayload },
    SearchOk { ok: Vec<WebSearchResult> },
    SearchErr { err: SearchErrPayload },
}

#[derive(Debug, Deserialize)]
struct FetchOkResult {
    content: String,
    kind: String,
}

#[derive(Debug, Deserialize)]
struct FetchErrPayload {
    status: u16,
    message: String,
}

#[derive(Debug, Deserialize)]
struct SearchErrPayload {
    message: String,
}

// ── Mock providers ──

struct FixtureFetchProvider {
    result: Result<UrlFetchResult, HttpFetchError>,
}

#[async_trait]
impl UrlFetcher for FixtureFetchProvider {
    async fn fetch(
        &self,
        _url: &str,
        _tool_call_id: Option<&str>,
    ) -> Result<UrlFetchResult, HttpFetchError> {
        match &self.result {
            Ok(r) => Ok(UrlFetchResult {
                content: r.content.clone(),
                kind: r.kind,
            }),
            Err(e) => Err(HttpFetchError::new(e.status, e.message.clone())),
        }
    }
}

use std::sync::Mutex;

struct FixtureSearchProvider {
    results: Mutex<Option<Result<Vec<WebSearchResult>, String>>>,
}

#[async_trait]
impl WebSearchProvider for FixtureSearchProvider {
    async fn search(
        &self,
        _query: &str,
        _options: Option<WebSearchOptions>,
    ) -> Result<Vec<WebSearchResult>, Box<dyn std::error::Error + Send>> {
        match self.results.lock().unwrap().take().unwrap() {
            Ok(r) => Ok(r),
            Err(e) => Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))),
        }
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

fn run_fixture(path: &str) {
    let content = std::fs::read_to_string(path).expect("read fixture");
    let fixture: FixtureFile = serde_json::from_str(&content).expect("parse fixture");

    let rt = tokio::runtime::Runtime::new().unwrap();

    for case in &fixture.cases {
        let result = rt.block_on(async {
            match (&case.op, &case.provider_result) {
                (WebOp::FetchUrl { url }, ProviderResult::FetchOk { ok }) => {
                    let kind = match ok.kind.as_str() {
                        "extracted" => UrlFetchKind::Extracted,
                        _ => UrlFetchKind::Passthrough,
                    };
                    let provider = FixtureFetchProvider {
                        result: Ok(UrlFetchResult {
                            content: ok.content.clone(),
                            kind,
                        }),
                    };
                    let tool = FetchURLTool::new(Arc::new(provider));
                    tool.execute(serde_json::json!({"url": url}), &NoopApproval)
                        .await
                        .unwrap()
                }
                (WebOp::FetchUrl { url }, ProviderResult::FetchErr { err }) => {
                    let provider = FixtureFetchProvider {
                        result: Err(HttpFetchError::new(err.status, err.message.clone())),
                    };
                    let tool = FetchURLTool::new(Arc::new(provider));
                    tool.execute(serde_json::json!({"url": url}), &NoopApproval)
                        .await
                        .unwrap()
                }
                (WebOp::WebSearch { query }, ProviderResult::SearchOk { ok }) => {
                    let provider = FixtureSearchProvider {
                        results: Mutex::new(Some(Ok(ok.clone()))),
                    };
                    let tool = WebSearchTool::new(Arc::new(provider));
                    tool.execute(serde_json::json!({"query": query}), &NoopApproval)
                        .await
                        .unwrap()
                }
                (WebOp::WebSearch { query }, ProviderResult::SearchErr { err }) => {
                    let provider = FixtureSearchProvider {
                        results: Mutex::new(Some(Err(err.message.clone()))),
                    };
                    let tool = WebSearchTool::new(Arc::new(provider));
                    tool.execute(serde_json::json!({"query": query}), &NoopApproval)
                        .await
                        .unwrap()
                }
                _ => panic!("mismatched op/provider pair in case {}", case.name),
            }
        });

        let expected = &case.expected;
        if expected.get("output").and_then(|v| v.as_str())
            != result.get("output").and_then(|v| v.as_str())
        {
            eprintln!("FAIL {}: output mismatch", case.name);
            eprintln!(
                "  expected: {}",
                serde_json::to_string_pretty(expected).unwrap()
            );
            eprintln!(
                "  got:      {}",
                serde_json::to_string_pretty(&result).unwrap()
            );
            panic!("fixture case '{}' failed", case.name);
        }
        if expected.get("isError") != result.get("isError") {
            eprintln!("FAIL {}: isError mismatch", case.name);
            panic!("fixture case '{}' failed", case.name);
        }
        // If expected has a message, check it
        if let Some(expected_msg) = expected.get("message").and_then(|v| v.as_str()) {
            let got_msg = result.get("message").and_then(|v| v.as_str()).unwrap_or("");
            if expected_msg != got_msg {
                eprintln!("FAIL {}: message mismatch", case.name);
                eprintln!("  expected: {expected_msg}");
                eprintln!("  got:      {got_msg}");
                panic!("fixture case '{}' failed", case.name);
            }
        }
    }
}

#[test]
fn fetch_url_fixtures_pass() {
    run_fixture("tests/fixtures/web/fetch-url.json");
}

#[test]
fn web_search_fixtures_pass() {
    run_fixture("tests/fixtures/web/web-search.json");
}
