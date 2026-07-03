use std::env;
use std::fs;

use kosong_rs::{
    generate, AnthropicChatProvider, AnthropicOptions, GenerateOptions, Message,
    ProviderRequestAuth,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    system_prompt: Option<String>,
    tools: Option<Value>,
    history: Vec<Message>,
    options: Option<FixtureOptions>,
    provider_step: ProviderStep,
    #[allow(dead_code)]
    expect_error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureOptions {
    auth: Option<ProviderRequestAuth>,
    stream: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderStep {
    events: Option<Vec<Value>>,
    response: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoldenResult {
    generate_result: Option<GenerateResultOut>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateResultOut {
    id: Option<String>,
    message: Value,
    usage: Option<kosong_rs::TokenUsage>,
    finish_reason: Option<String>,
    raw_finish_reason: Option<String>,
}

fn events_to_sse_body(events: &[Value]) -> String {
    events
        .iter()
        .map(|e| {
            let event_type = e.get("type").and_then(|v| v.as_str()).unwrap_or("message");
            format!("event: {}\ndata: {}\n\n", event_type, e.to_string())
        })
        .collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = env::args().nth(1).expect("fixture path required");
    let input = fs::read_to_string(&path)?;
    let fixture: Fixture = serde_json::from_str(&input)?;

    let stream = fixture
        .options
        .as_ref()
        .and_then(|o| o.stream)
        .unwrap_or(true);

    let server = httptest::Server::run();
    if stream {
        let events = fixture
            .provider_step
            .events
            .as_ref()
            .expect("stream fixture requires events");
        server.expect(
            httptest::Expectation::matching(httptest::matchers::request::method_path(
                "POST",
                "/v1/messages",
            ))
            .respond_with(
                httptest::responders::status_code(200)
                    .body(events_to_sse_body(events))
                    .insert_header("Content-Type", "text/event-stream"),
            ),
        );
    } else {
        let response = fixture
            .provider_step
            .response
            .as_ref()
            .expect("non-stream fixture requires response");
        server.expect(
            httptest::Expectation::matching(httptest::matchers::request::method_path(
                "POST",
                "/v1/messages",
            ))
            .respond_with(
                httptest::responders::status_code(200)
                    .body(response.to_string())
                    .insert_header("Content-Type", "application/json"),
            ),
        );
    }
    let base_url = server.url_str("/");

    let provider = AnthropicChatProvider::new(AnthropicOptions {
        model: "claude-opus-4-7".into(),
        api_key: Some("sk-golden".into()),
        base_url: Some(base_url),
        default_max_tokens: Some(1_024),
        beta_features: Some(vec![]),
        default_headers: None,
        metadata: None,
        stream: Some(stream),
        adaptive_thinking: None,
    });

    let tools: Vec<kosong_rs::provider::Tool> = fixture
        .tools
        .and_then(|t| serde_json::from_value(t).ok())
        .unwrap_or_default();
    let options = fixture.options.map(|o| GenerateOptions {
        auth: o.auth,
        ..Default::default()
    });

    let output = match generate(
        &provider,
        &fixture.system_prompt.unwrap_or_default(),
        &tools,
        &fixture.history,
        None,
        options.as_ref(),
    )
    .await
    {
        Ok(r) => GoldenResult {
            generate_result: Some(GenerateResultOut {
                id: r.id,
                message: serde_json::to_value(&r.message)?,
                usage: r.usage,
                finish_reason: r.finish_reason.map(|fr| {
                    serde_json::to_string(&fr)
                        .unwrap()
                        .trim_matches('"')
                        .to_string()
                }),
                raw_finish_reason: r.raw_finish_reason,
            }),
            error: None,
        },
        Err(e) => GoldenResult {
            generate_result: None,
            error: Some(format!("{}", e)),
        },
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
