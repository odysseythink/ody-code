use std::env;
use std::fs;

use kosong_rs::{generate, GoogleGenAIChatProvider, Message};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct Fixture {
    model: String,
    system_prompt: Option<String>,
    history: Vec<Message>,
    #[serde(default)]
    stream: bool,
}

#[derive(Debug, Serialize)]
struct GoldenResult {
    assistant_message: Option<Value>,
    error: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = env::args().nth(1).expect("fixture path required");
    let raw = fs::read_to_string(&path)?;
    let fixture: Fixture = serde_json::from_str(&raw)?;
    let provider = GoogleGenAIChatProvider::new(fixture.model).with_stream(fixture.stream);
    let result = generate(
        &provider,
        &fixture.system_prompt.unwrap_or_default(),
        &[],
        &fixture.history,
        None,
        None,
    )
    .await;
    let output = match result {
        Ok(r) => GoldenResult {
            assistant_message: Some(serde_json::to_value(&r.message)?),
            error: None,
        },
        Err(e) => GoldenResult {
            assistant_message: None,
            error: Some(format!("{e}")),
        },
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
