use std::collections::HashMap;
use std::env;
use std::fs;

use anyhow::{bail, Context, Result};
use kosong_rs::catalog::{CatalogModelEntry, CatalogProviderEntry};
use kosong_rs::provider::{ProviderRequestAuth, ProviderType};
use kosong_rs::tool_call_id::{
    normalize_tool_call_ids_for_provider, sanitize_openai_responses_call_id, sanitize_tool_call_id,
    ToolCallIdPolicy,
};
use kosong_rs::{capability_registry, catalog, request_auth};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const TOOL_CALL_ID_MAX_LENGTH: usize = 64;

#[derive(Debug, Deserialize)]
struct Fixture {
    operations: Vec<Operation>,
}

#[derive(Debug, Deserialize)]
struct Operation {
    operation: String,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    input: Value,
}

#[derive(Debug, Serialize)]
struct GoldenResult {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct GoldenOperation {
    operation: String,
    results: Vec<GoldenResult>,
}

#[derive(Debug, Serialize)]
struct GoldenOutput {
    operations: Vec<GoldenOperation>,
}

fn main() -> Result<()> {
    let path = env::args().nth(1).context("fixture path required")?;
    let input = fs::read_to_string(&path)?;
    let fixture: Fixture = serde_json::from_str(&input)?;

    let mut operations = Vec::new();
    for op in fixture.operations {
        let mut results = Vec::new();
        for case in op.cases {
            match run_case(&op.operation, &case.input) {
                Ok(output) => results.push(GoldenResult {
                    name: case.name,
                    output: Some(output),
                    error: None,
                }),
                Err(e) => {
                    results.push(GoldenResult {
                        name: case.name,
                        output: None,
                        error: Some(format!("{}", e)),
                    });
                }
            }
        }
        operations.push(GoldenOperation {
            operation: op.operation,
            results,
        });
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&GoldenOutput { operations })?
    );
    Ok(())
}

fn run_case(operation: &str, input: &Value) -> Result<Value> {
    match operation {
        "sanitizeToolCallId" => {
            let id = input["id"].as_str().context("id must be a string")?;
            let max_length = input
                .get("maxLength")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            Ok(Value::String(sanitize_tool_call_id(id, max_length)))
        }
        "sanitizeOpenAIResponsesCallId" => {
            let id = input["id"].as_str().context("id must be a string")?;
            let max_length = input
                .get("maxLength")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            Ok(Value::String(sanitize_openai_responses_call_id(
                id, max_length,
            )))
        }
        "normalizeToolCallIdsForProvider" => {
            let messages: Vec<kosong_rs::message::Message> =
                serde_json::from_value(input["messages"].clone())
                    .context("messages must be an array of Message")?;
            let provider: ProviderType = serde_json::from_value(input["provider"].clone())
                .context("provider must be a ProviderType string")?;
            let policy = tool_call_id_policy_for_provider(provider);
            let normalized = normalize_tool_call_ids_for_provider(&messages, &policy);
            Ok(serde_json::to_value(&normalized)?)
        }
        "requireProviderApiKey" => {
            let provider_name = input["providerName"]
                .as_str()
                .context("providerName must be a string")?;
            let auth: Option<ProviderRequestAuth> = input
                .get("auth")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            let default_api_key = input.get("defaultApiKey").and_then(|v| v.as_str());
            let key = request_auth::require_provider_api_key(
                provider_name,
                auth.as_ref(),
                default_api_key,
            )?;
            Ok(Value::String(key))
        }
        "mergeRequestHeaders" => {
            let default_headers: Option<HashMap<String, String>> = input
                .get("defaultHeaders")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            let request_headers: Option<HashMap<String, String>> = input
                .get("requestHeaders")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            let merged = request_auth::merge_request_headers(
                default_headers.as_ref(),
                request_headers.as_ref(),
            );
            Ok(serde_json::to_value(&merged)?)
        }
        "getOpenAILegacyModelCapability" => {
            let model_name = input["modelName"]
                .as_str()
                .context("modelName must be a string")?;
            let cap = capability_registry::get_openai_legacy_model_capability(model_name);
            Ok(serde_json::to_value(&cap)?)
        }
        "getOpenAIResponsesModelCapability" => {
            let model_name = input["modelName"]
                .as_str()
                .context("modelName must be a string")?;
            let cap = capability_registry::get_openai_responses_model_capability(model_name);
            Ok(serde_json::to_value(&cap)?)
        }
        "getAnthropicModelCapability" => {
            let model_name = input["modelName"]
                .as_str()
                .context("modelName must be a string")?;
            let cap = capability_registry::get_anthropic_model_capability(model_name);
            Ok(serde_json::to_value(&cap)?)
        }
        "getGoogleGenAIModelCapability" => {
            let model_name = input["modelName"]
                .as_str()
                .context("modelName must be a string")?;
            let cap = capability_registry::get_google_genai_model_capability(model_name);
            Ok(serde_json::to_value(&cap)?)
        }
        "usesOpenAIResponsesDeveloperRole" => {
            let model_name = input["modelName"]
                .as_str()
                .context("modelName must be a string")?;
            Ok(Value::Bool(
                capability_registry::uses_openai_responses_developer_role(model_name),
            ))
        }
        "inferWireType" => {
            let entry: CatalogProviderEntry = serde_json::from_value(input["entry"].clone())
                .context("entry must be a CatalogProviderEntry")?;
            let wire = catalog::infer_wire_type(&entry);
            Ok(serde_json::to_value(&wire)?)
        }
        "catalogBaseUrl" => {
            let entry: CatalogProviderEntry = serde_json::from_value(input["entry"].clone())
                .context("entry must be a CatalogProviderEntry")?;
            let wire: ProviderType = serde_json::from_value(input["wire"].clone())
                .context("wire must be a ProviderType string")?;
            let url = catalog::catalog_base_url(&entry, wire);
            Ok(serde_json::to_value(&url)?)
        }
        "catalogModelToCapability" => {
            let model: CatalogModelEntry = serde_json::from_value(input["model"].clone())
                .context("model must be a CatalogModelEntry")?;
            let result = catalog::catalog_model_to_capability(&model);
            Ok(serde_json::to_value(&result)?)
        }
        "catalogProviderModels" => {
            let entry: CatalogProviderEntry = serde_json::from_value(input["entry"].clone())
                .context("entry must be a CatalogProviderEntry")?;
            let models = catalog::catalog_provider_models(&entry);
            Ok(serde_json::to_value(&models)?)
        }
        _ => bail!("unknown operation: {}", operation),
    }
}

fn tool_call_id_policy_for_provider(provider: ProviderType) -> ToolCallIdPolicy {
    match provider {
        ProviderType::OpenAiResponses => ToolCallIdPolicy::new(
            |id| sanitize_openai_responses_call_id(id, Some(TOOL_CALL_ID_MAX_LENGTH)),
            Some(TOOL_CALL_ID_MAX_LENGTH),
        ),
        _ => ToolCallIdPolicy::new(
            |id| sanitize_tool_call_id(id, Some(TOOL_CALL_ID_MAX_LENGTH)),
            Some(TOOL_CALL_ID_MAX_LENGTH),
        ),
    }
}
