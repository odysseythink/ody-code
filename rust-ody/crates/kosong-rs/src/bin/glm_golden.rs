use std::env;
use std::fs;
use std::sync::Arc;

use kosong_rs::providers::glm::{GLMChatProvider, GLMOptions};
use kosong_rs::{
    generate, ChatProviderError, GenerateOptions, Message, MockHttpClient, ProviderRequestAuth,
    Tool,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    system_prompt: Option<String>,
    #[serde(default)]
    tools: Vec<Tool>,
    history: Vec<Message>,
    #[serde(default)]
    options: FixtureOptions,
    provider_options: ProviderOptionsFixture,
    response: ResponseFixture,
    #[serde(default)]
    #[allow(dead_code)]
    expect_error: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct FixtureOptions {
    auth: Option<ProviderRequestAuth>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderOptionsFixture {
    model: String,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    max_tokens: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseFixture {
    status: u16,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    error: Option<ErrorFixture>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ErrorFixture {
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoldenResult {
    assistant_message: Option<Value>,
    error: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = env::args().nth(1).expect("fixture path required");
    let fixture = load_fixture(&path)?;
    let result = run_fixture(fixture).await;
    let output = GoldenResult {
        assistant_message: result
            .as_ref()
            .ok()
            .map(|m| serde_json::to_value(m).unwrap()),
        error: result.as_ref().err().map(|e| format!("{}", e)),
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn load_fixture(path: &str) -> anyhow::Result<Fixture> {
    let input = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&input)?)
}

async fn run_fixture(fixture: Fixture) -> Result<Message, ChatProviderError> {
    let response_bytes = build_response_bytes(&fixture.response);
    let client = Arc::new(MockHttpClient::new(fixture.response.status, response_bytes));
    let options = GLMOptions {
        api_key: Some(
            fixture
                .provider_options
                .api_key
                .unwrap_or_else(|| "sk-test".into()),
        ),
        base_url: Some(
            fixture
                .provider_options
                .base_url
                .unwrap_or_else(|| "http://mock".into()),
        ),
        model: fixture.provider_options.model,
        stream: Some(fixture.provider_options.stream),
        max_tokens: fixture.provider_options.max_tokens,
        default_headers: None,
        http_client: Some(client),
    };
    let provider = GLMChatProvider::new(options);
    let gen_options = fixture.options.auth.map(|auth| GenerateOptions {
        auth: Some(auth),
        ..Default::default()
    });
    let result = generate(
        &provider,
        &fixture.system_prompt.unwrap_or_default(),
        &fixture.tools,
        &fixture.history,
        None,
        gen_options.as_ref(),
    )
    .await?;
    Ok(result.message)
}

fn build_response_bytes(response: &ResponseFixture) -> Vec<u8> {
    if let Some(error) = &response.error {
        let body = serde_json::json!({"error": { "message": error.message, "type": "invalid_request_error" }});
        return serde_json::to_vec(&body).unwrap();
    }
    let body = response
        .body
        .as_ref()
        .expect("success response must have body");
    if response.stream {
        body.as_bytes().to_vec()
    } else {
        let value: Value = serde_json::from_str(body).expect("non-stream body must be valid JSON");
        serde_json::to_vec(&value).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_stream_fixture() -> Fixture {
        let body = "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"}}]}\n\n\
            data: {\"id\":\"2\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"}}]}\n\n\
            data: {\"id\":\"3\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
            data: [DONE]\n\n";
        serde_json::from_value(serde_json::json!({
            "systemPrompt": "",
            "history": [{"role": "user", "content": [{"type": "text", "text": "Hi"}], "toolCalls": []}],
            "providerOptions": {"model": "glm-4-flash", "stream": true},
            "response": {"status": 200, "stream": true, "body": body}
        }))
        .unwrap()
    }

    fn thinking_stream_fixture() -> Fixture {
        let body = "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"reasoning_content\":\"Let me think\"}}]}\n\n\
            data: {\"id\":\"2\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\" about this\"}}]}\n\n\
            data: {\"id\":\"3\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"}}]}\n\n\
            data: {\"id\":\"4\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
            data: [DONE]\n\n";
        serde_json::from_value(serde_json::json!({
            "systemPrompt": "",
            "history": [{"role": "user", "content": [{"type": "text", "text": "Hi"}], "toolCalls": []}],
            "providerOptions": {"model": "glm-4-flash", "stream": true},
            "response": {"status": 200, "stream": true, "body": body}
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn binary_runs_text_stream_fixture() {
        let fixture = text_stream_fixture();
        let message = run_fixture(fixture).await.unwrap();
        let text = kosong_rs::message::extract_text(&message, "");
        assert_eq!(text, "Hello world");
    }

    #[tokio::test]
    async fn binary_runs_thinking_stream_fixture() {
        let fixture = thinking_stream_fixture();
        let message = run_fixture(fixture).await.unwrap();
        assert_eq!(kosong_rs::message::extract_text(&message, ""), "Hello");
        let think_parts: Vec<_> = message
            .content
            .iter()
            .filter_map(|p| match p {
                kosong_rs::message::ContentPart::Think { think, .. } => Some(think.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(think_parts.concat(), "Let me think about this");
    }

    #[tokio::test]
    async fn binary_reports_error_for_failed_response() {
        let fixture: Fixture = serde_json::from_value(serde_json::json!({
            "systemPrompt": "",
            "history": [{"role": "user", "content": [{"type": "text", "text": "Hi"}], "toolCalls": []}],
            "providerOptions": {"model": "glm-4-flash", "stream": true},
            "response": {"status": 401, "error": {"message": "Invalid auth"}},
            "expectError": true
        }))
        .unwrap();
        assert!(run_fixture(fixture).await.is_err());
    }
}
