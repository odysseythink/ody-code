use std::env;
use std::fs;

use kosong_rs::{
    generate, GenerateOptions, Message, MockProvider, StreamedMessagePart, TokenUsage,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    system_prompt: Option<String>,
    tools: Option<Value>,
    history: Vec<Message>,
    options: Option<GenerateOptionsFixture>,
    provider_step: ProviderStep,
    #[allow(dead_code)]
    expect_error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateOptionsFixture {
    auth: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderStep {
    id: Option<String>,
    parts: Vec<StreamedMessagePart>,
    usage: Option<TokenUsage>,
    finish_reason: Option<kosong_rs::provider::FinishReason>,
    raw_finish_reason: Option<String>,
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
    let input = fs::read_to_string(&path)?;
    let fixture: Fixture = serde_json::from_str(&input)?;

    let mut provider = MockProvider::new("mock", "m1").with_parts(fixture.provider_step.parts);
    if let Some(id) = fixture.provider_step.id {
        provider = provider.with_id(id);
    }
    if let Some(usage) = fixture.provider_step.usage {
        provider = provider.with_usage(usage);
    }
    if let Some(finish_reason) = fixture.provider_step.finish_reason {
        provider = provider.with_finish_reason(finish_reason);
    }
    if let Some(raw) = fixture.provider_step.raw_finish_reason {
        provider = provider.with_raw_finish_reason(raw);
    }

    let system_prompt = fixture.system_prompt.unwrap_or_default();
    let tools: Vec<kosong_rs::provider::Tool> = fixture
        .tools
        .and_then(|t| serde_json::from_value(t).ok())
        .unwrap_or_default();
    let options = fixture.options.map(|o| GenerateOptions {
        auth: o.auth.and_then(|a| serde_json::from_value(a).ok()),
        ..Default::default()
    });

    let result = generate(
        &provider,
        &system_prompt,
        &tools,
        &fixture.history,
        None,
        options.as_ref(),
    )
    .await;

    let output = match result {
        Ok(r) => GoldenResult {
            assistant_message: Some(serde_json::to_value(&r.message)?),
            error: None,
        },
        Err(e) => GoldenResult {
            assistant_message: None,
            error: Some(format!("{}", e)),
        },
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
