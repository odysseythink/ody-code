use std::env;
use std::fs;
use std::sync::Arc;

use futures_util::StreamExt;
use kosong_rs::{
    generate, ChatProviderError, GenerateOptions, Message, MockHttpClient,
    OpenAIResponsesChatProvider, OpenAIResponsesOptions, OpenAIResponsesStreamedMessage,
    ProviderRequestAuth, StreamedMessagePart, Tool,
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
    if fixture.response.stream {
        run_through_provider(fixture).await
    } else {
        run_direct_parser(fixture).await
    }
}

async fn run_through_provider(fixture: Fixture) -> Result<Message, ChatProviderError> {
    let response_bytes = build_response_bytes(&fixture.response);
    let client = Arc::new(MockHttpClient::new(fixture.response.status, response_bytes));
    let options = OpenAIResponsesOptions {
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
        max_output_tokens: None,
        default_headers: None,
        tool_message_conversion: None,
        http_client: Some(client),
    };
    let provider = OpenAIResponsesChatProvider::new(options);
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
        let body = serde_json::json!({
            "error": { "message": error.message, "type": "invalid_request_error" }
        });
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

async fn run_direct_parser(fixture: Fixture) -> Result<Message, ChatProviderError> {
    let body = fixture
        .response
        .body
        .as_ref()
        .expect("non-stream body required")
        .as_bytes()
        .to_vec();
    let stream = OpenAIResponsesStreamedMessage::from_bytes(body, false)?;
    let mut message = Message::assistant(vec![], vec![]);
    futures_util::pin_mut!(stream);
    while let Some(part) = stream.next().await {
        match part {
            StreamedMessagePart::Content(c) => message.content.push(c),
            StreamedMessagePart::ToolCall(mut tc) => {
                tc.stream_index = None;
                message.tool_calls.push(tc);
            }
            StreamedMessagePart::ToolCallPart(_) => {}
        }
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_stream_fixture() -> Fixture {
        serde_json::from_value(serde_json::json!({
            "systemPrompt": "",
            "history": [{"role":"user","content":[{"type":"text","text":"Hi"}],"toolCalls":[]}],
            "providerOptions": {"model": "gpt-4o-mini"},
            "response": {
                "status": 200,
                "stream": true,
                "body": "{\"type\":\"response.created\",\"response\":{\"id\":\"resp-test\"}}\n{\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n{\"type\":\"response.completed\",\"response\":{\"id\":\"resp-test\",\"status\":\"completed\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n"
            }
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn binary_runs_text_stream_fixture() {
        let fixture = text_stream_fixture();
        let message = run_fixture(fixture).await.unwrap();
        let text = kosong_rs::message::extract_text(&message, "");
        assert_eq!(text, "Hello");
    }

    #[tokio::test]
    async fn binary_reports_error_for_failed_response() {
        let fixture: Fixture = serde_json::from_value(serde_json::json!({
            "systemPrompt": "",
            "history": [{"role":"user","content":[{"type":"text","text":"Hi"}],"toolCalls":[]}],
            "providerOptions": {"model": "gpt-4o-mini"},
            "response": {
                "status": 401,
                "stream": true,
                "error": {"message": "Invalid auth"}
            },
            "expectError": true
        }))
        .unwrap();
        assert!(run_fixture(fixture).await.is_err());
    }
}
