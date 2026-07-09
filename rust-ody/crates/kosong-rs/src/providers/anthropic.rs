use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
struct ClaudeVersion {
    family: String,
    major: u32,
    minor: Option<u32>,
}

const FALLBACK_MAX_TOKENS: i64 = 32_000;

fn ceiling_by_family_version() -> HashMap<String, i64> {
    [
        ("opus-4-7", 128_000),
        ("opus-4-6", 128_000),
        ("opus-4-5", 64_000),
        ("opus-4-1", 32_000),
        ("opus-4-0", 32_000),
        ("opus-4", 32_000),
        ("sonnet-4-6", 64_000),
        ("sonnet-4-5", 64_000),
        ("sonnet-4-0", 64_000),
        ("sonnet-4", 64_000),
        ("haiku-4-5", 64_000),
        ("haiku-4", 64_000),
        ("opus-3-5", 8_192),
        ("sonnet-3-5", 8_192),
        ("sonnet-3-7", 8_192),
        ("haiku-3-5", 8_192),
        ("opus-3", 4_096),
        ("sonnet-3", 4_096),
        ("haiku-3", 4_096),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), *v))
    .collect()
}

/// Check that a version number string is at most 2 digits (not part of a date like 20251001).
fn is_valid_version_component(s: &str) -> bool {
    s.len() <= 2
}

fn parse_claude_version(model: &str) -> Option<ClaudeVersion> {
    let normalized = model.to_lowercase();
    // Guard: non-Claude ids must not accidentally match opus/sonnet/haiku substrings.
    if !normalized.contains("claude") {
        return None;
    }

    // Family-first: "opus-4-7", "sonnet-4.6", "haiku-4-5-20251001"
    // Without lookahead, we match \d+ and validate length post-capture.
    let family_first = regex::Regex::new(r"(opus|sonnet|haiku)[-._](\d+)(?:[-._](\d+))?").unwrap();
    if let Some(caps) = family_first.captures(&normalized) {
        let major_str = &caps[2];
        if is_valid_version_component(major_str) {
            let minor = caps.get(3).and_then(|m| {
                let minor_str = m.as_str();
                if is_valid_version_component(minor_str) {
                    Some(minor_str.parse::<u32>().unwrap())
                } else {
                    None
                }
            });
            return Some(ClaudeVersion {
                family: caps[1].to_string(),
                major: caps[2].parse().unwrap(),
                minor,
            });
        }
    }

    // Version-first: "3-5-sonnet", "3.7.opus"
    let version_first = regex::Regex::new(r"(\d+)[-._](\d+)[-._](opus|sonnet|haiku)").unwrap();
    if let Some(caps) = version_first.captures(&normalized) {
        let major_str = &caps[1];
        let minor_str = &caps[2];
        if is_valid_version_component(major_str) && is_valid_version_component(minor_str) {
            return Some(ClaudeVersion {
                family: caps[3].to_string(),
                major: caps[1].parse().unwrap(),
                minor: Some(caps[2].parse().unwrap()),
            });
        }
    }

    // Bare family: "3-opus", "3.haiku"
    let bare = regex::Regex::new(r"(\d+)[-._](opus|sonnet|haiku)").unwrap();
    if let Some(caps) = bare.captures(&normalized) {
        let major_str = &caps[1];
        if is_valid_version_component(major_str) {
            return Some(ClaudeVersion {
                family: caps[2].to_string(),
                major: caps[1].parse().unwrap(),
                minor: None,
            });
        }
    }

    None
}

fn lookup_claude_ceiling(version: &ClaudeVersion) -> Option<i64> {
    let table = ceiling_by_family_version();
    if let Some(minor) = version.minor {
        if let Some(v) = table.get(&format!("{}-{}-{}", version.family, version.major, minor)) {
            return Some(*v);
        }
    }
    table
        .get(&format!("{}-{}", version.family, version.major))
        .copied()
}

pub fn resolve_default_max_tokens(model: &str, override_: Option<i64>) -> i64 {
    let parsed = parse_claude_version(model);
    let ceiling = parsed.as_ref().and_then(lookup_claude_ceiling);
    match ceiling {
        None => override_.unwrap_or(FALLBACK_MAX_TOKENS),
        Some(c) => override_.map(|o| o.min(c)).unwrap_or(c),
    }
}

// ── Thinking configuration ──────────────────────────────────────────────

use crate::provider::ThinkingEffort;

const ADAPTIVE_MIN_VERSION: (u32, u32) = (4, 6);
const INTERLEAVED_THINKING_BETA: &str = "interleaved-thinking-2025-05-14";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AnthropicThinkingConfig {
    Disabled,
    Adaptive { display: String },
    Enabled { budget_tokens: i64 },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnthropicOutputConfig {
    pub effort: String,
}

fn supports_adaptive_thinking(model: &str) -> bool {
    let version = parse_claude_version(model);
    match version {
        Some(v) if v.minor.is_some() => {
            let minor = v.minor.unwrap();
            v.major > ADAPTIVE_MIN_VERSION.0
                || (v.major == ADAPTIVE_MIN_VERSION.0 && minor >= ADAPTIVE_MIN_VERSION.1)
        }
        _ => false,
    }
}

fn is_opus_47(model: &str) -> bool {
    parse_claude_version(model)
        .map(|v| v.family == "opus" && v.major == 4 && v.minor == Some(7))
        .unwrap_or(false)
}

fn supports_effort_param(model: &str, adaptive: bool) -> bool {
    if adaptive {
        return true;
    }
    let normalized = model.to_lowercase();
    normalized.contains("opus-4-5") || normalized.contains("opus-4.5")
}

fn clamp_effort(effort: ThinkingEffort, model: &str, adaptive: bool) -> ThinkingEffort {
    match effort {
        ThinkingEffort::Off => ThinkingEffort::Off,
        ThinkingEffort::Xhigh if !is_opus_47(model) => ThinkingEffort::High,
        ThinkingEffort::Max if !adaptive => ThinkingEffort::High,
        other => other,
    }
}

fn budget_tokens_for_effort(effort: ThinkingEffort) -> i64 {
    match effort {
        ThinkingEffort::Low => 1_024,
        ThinkingEffort::Medium => 4_096,
        ThinkingEffort::High => 32_000,
        _ => panic!("Unsupported budget-based thinking effort: {:?}", effort),
    }
}

fn effort_str(effort: ThinkingEffort) -> String {
    match effort {
        ThinkingEffort::Off => "off",
        ThinkingEffort::Low => "low",
        ThinkingEffort::Medium => "medium",
        ThinkingEffort::High => "high",
        ThinkingEffort::Xhigh => "xhigh",
        ThinkingEffort::Max => "max",
    }
    .to_string()
}

pub fn build_thinking_config(
    effort: ThinkingEffort,
    model: &str,
    adaptive_override: Option<bool>,
) -> (AnthropicThinkingConfig, Option<AnthropicOutputConfig>) {
    let adaptive = adaptive_override.unwrap_or_else(|| supports_adaptive_thinking(model));

    if effort == ThinkingEffort::Off {
        return (AnthropicThinkingConfig::Disabled, None);
    }

    let effective = clamp_effort(effort, model, adaptive);
    if effective == ThinkingEffort::Off {
        return (AnthropicThinkingConfig::Disabled, None);
    }

    if adaptive {
        let effort_str_val = effort_str(effective);
        return (
            AnthropicThinkingConfig::Adaptive {
                display: "summarized".into(),
            },
            Some(AnthropicOutputConfig {
                effort: effort_str_val,
            }),
        );
    }

    let thinking = AnthropicThinkingConfig::Enabled {
        budget_tokens: budget_tokens_for_effort(effective),
    };
    let output = if supports_effort_param(model, adaptive) {
        Some(AnthropicOutputConfig {
            effort: effort_str(effective),
        })
    } else {
        None
    };
    (thinking, output)
}

// ── Provider struct & ChatProvider trait ───────────────────────────────

use crate::capability_registry::get_anthropic_model_capability;
use crate::errors::ChatProviderError;
use crate::generate::StreamedMessage;
use crate::message::Message;
use crate::message::{ContentPart, StreamedMessagePart, ToolCall};
use crate::provider::{ChatProvider, FinishReason, GenerateOptions, ModelCapability, Tool};

pub struct AnthropicOptions {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: String,
    pub default_max_tokens: Option<i64>,
    pub beta_features: Option<Vec<String>>,
    pub default_headers: Option<HashMap<String, String>>,
    pub metadata: Option<HashMap<String, String>>,
    pub stream: Option<bool>,
    pub adaptive_thinking: Option<bool>,
}

#[derive(Clone)]
pub struct AnthropicGenerationKwargs {
    pub max_tokens: i64,
    pub temperature: Option<f64>,
    pub top_k: Option<i64>,
    pub top_p: Option<f64>,
    pub thinking: Option<AnthropicThinkingConfig>,
    pub output_config: Option<AnthropicOutputConfig>,
    pub beta_features: Vec<String>,
}

pub struct AnthropicChatProvider {
    model: String,
    stream: bool,
    api_key: Option<String>,
    base_url: Option<String>,
    default_headers: Option<HashMap<String, String>>,
    metadata: Option<HashMap<String, String>>,
    generation_kwargs: AnthropicGenerationKwargs,
    adaptive_thinking: Option<bool>,
    explicit_max_tokens: bool,
    client: reqwest::Client,
}

impl AnthropicChatProvider {
    pub fn new(options: AnthropicOptions) -> Self {
        let api_key = options.api_key.filter(|k| !k.is_empty());
        let max_tokens = resolve_default_max_tokens(&options.model, options.default_max_tokens);
        let beta_features = options
            .beta_features
            .unwrap_or_else(|| vec![INTERLEAVED_THINKING_BETA.to_string()]);
        Self {
            model: options.model,
            stream: options.stream.unwrap_or(true),
            api_key,
            base_url: options.base_url,
            default_headers: options.default_headers,
            metadata: options.metadata,
            generation_kwargs: AnthropicGenerationKwargs {
                max_tokens,
                temperature: None,
                top_k: None,
                top_p: None,
                thinking: None,
                output_config: None,
                beta_features,
            },
            adaptive_thinking: options.adaptive_thinking,
            explicit_max_tokens: options.default_max_tokens.is_some(),
            client: reqwest::Client::new(),
        }
    }

    pub fn max_tokens(&self) -> i64 {
        self.generation_kwargs.max_tokens
    }

    fn clone_with_generation_kwargs(&self, patch: AnthropicGenerationKwargsPatch) -> Self {
        let mut clone = Self {
            model: self.model.clone(),
            stream: self.stream,
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            default_headers: self.default_headers.clone(),
            metadata: self.metadata.clone(),
            generation_kwargs: self.generation_kwargs.clone(),
            adaptive_thinking: self.adaptive_thinking,
            client: self.client.clone(),
            explicit_max_tokens: self.explicit_max_tokens,
        };
        if let Some(t) = patch.thinking {
            clone.generation_kwargs.thinking = Some(t);
            if clone.generation_kwargs.thinking == Some(AnthropicThinkingConfig::Disabled) {
                clone.generation_kwargs.output_config = None;
            }
        }
        if patch.output_config_set {
            clone.generation_kwargs.output_config = patch.output_config;
        }
        if let Some(m) = patch.max_tokens {
            clone.generation_kwargs.max_tokens = m;
        }
        if let Some(b) = patch.beta_features {
            clone.generation_kwargs.beta_features = b;
        }
        clone
    }
}

#[derive(Default)]
struct AnthropicGenerationKwargsPatch {
    thinking: Option<AnthropicThinkingConfig>,
    output_config: Option<AnthropicOutputConfig>,
    output_config_set: bool,
    max_tokens: Option<i64>,
    beta_features: Option<Vec<String>>,
}

#[async_trait::async_trait]
impl ChatProvider for AnthropicChatProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn thinking_effort(&self) -> Option<ThinkingEffort> {
        match self.generation_kwargs.thinking.as_ref()? {
            AnthropicThinkingConfig::Disabled => Some(ThinkingEffort::Off),
            AnthropicThinkingConfig::Adaptive { .. } => {
                let effort = self
                    .generation_kwargs
                    .output_config
                    .as_ref()
                    .map(|o| o.effort.as_str())
                    .unwrap_or("high");
                match effort {
                    "low" => Some(ThinkingEffort::Low),
                    "medium" => Some(ThinkingEffort::Medium),
                    "high" => Some(ThinkingEffort::High),
                    "xhigh" => Some(ThinkingEffort::Xhigh),
                    "max" => Some(ThinkingEffort::Max),
                    _ => None,
                }
            }
            AnthropicThinkingConfig::Enabled { budget_tokens } => {
                if *budget_tokens <= 1_024 {
                    Some(ThinkingEffort::Low)
                } else if *budget_tokens <= 4_096 {
                    Some(ThinkingEffort::Medium)
                } else {
                    Some(ThinkingEffort::High)
                }
            }
        }
    }

    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        let (thinking, output) = build_thinking_config(effort, &self.model, self.adaptive_thinking);
        let mut patch = AnthropicGenerationKwargsPatch {
            thinking: Some(thinking),
            output_config_set: true,
            ..Default::default()
        };
        patch.output_config = output;

        // Adaptive thinking removes interleaved-thinking beta; disabled removes it entirely.
        if matches!(
            patch.thinking,
            Some(AnthropicThinkingConfig::Adaptive { .. })
        ) || matches!(patch.thinking, Some(AnthropicThinkingConfig::Disabled))
        {
            patch.beta_features = Some(
                self.generation_kwargs
                    .beta_features
                    .iter()
                    .cloned()
                    .filter(|b| b != INTERLEAVED_THINKING_BETA)
                    .collect(),
            );
        }

        Box::new(self.clone_with_generation_kwargs(patch))
    }

    fn with_max_completion_tokens(
        &self,
        max_completion_tokens: i64,
    ) -> Option<Box<dyn ChatProvider>> {
        let requested_cap = resolve_default_max_tokens(&self.model, Some(max_completion_tokens));
        let existing_cap = self.generation_kwargs.max_tokens;
        // If defaultMaxTokens was explicitly provided, preserve it; otherwise clamp.
        let new_cap = if self.explicit_max_tokens {
            existing_cap
        } else {
            requested_cap
        };
        let patch = AnthropicGenerationKwargsPatch {
            max_tokens: Some(new_cap),
            ..Default::default()
        };
        Some(Box::new(self.clone_with_generation_kwargs(patch)))
    }

    fn get_capability(&self, model: Option<&str>) -> ModelCapability {
        get_anthropic_model_capability(model.unwrap_or(&self.model))
    }

    async fn generate(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        history: &[Message],
        options: Option<GenerateOptions>,
    ) -> Result<StreamedMessage, ChatProviderError> {
        let auth = options.as_ref().and_then(|o| o.auth.as_ref());
        let stream = self.stream;
        let create_params = self.build_create_params(system_prompt, tools, history, stream)?;
        let headers = self.build_extra_headers(auth)?;

        let url = format!(
            "{}v1/messages",
            self.base_url
                .as_deref()
                .unwrap_or("https://api.anthropic.com/")
        );

        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .map_err(|_| ChatProviderError::Connection(crate::errors::APIConnectionError))?;

        let mut req = client.post(&url).json(&create_params);
        for (k, v) in headers {
            req = req.header(k, v);
        }

        let response = req.send().await.map_err(convert_anthropic_request_error)?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(convert_anthropic_api_error(status.as_u16(), &body, None));
        }

        if stream {
            let body = response
                .text()
                .await
                .map_err(convert_anthropic_request_error)?;
            let events = parse_sse_body(&body);
            anthropic_events_to_streamed_message(events)
        } else {
            let message_response: AnthropicMessageResponse =
                response.json().await.map_err(|_| {
                    ChatProviderError::Status(crate::errors::APIStatusError {
                        status_code: 500,
                        message: "Failed to parse Anthropic response JSON".into(),
                        request_id: None,
                    })
                })?;
            parse_non_stream_response(message_response)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_known_claude_4_caps() {
        assert_eq!(resolve_default_max_tokens("claude-opus-4-7", None), 128_000);
        assert_eq!(resolve_default_max_tokens("claude-opus-4-6", None), 128_000);
        assert_eq!(
            resolve_default_max_tokens("claude-opus-4-5-20251101", None),
            64_000
        );
        assert_eq!(
            resolve_default_max_tokens("claude-opus-4-1-20250805", None),
            32_000
        );
        assert_eq!(
            resolve_default_max_tokens("claude-sonnet-4-6", None),
            64_000
        );
        assert_eq!(
            resolve_default_max_tokens("claude-haiku-4-5-20251001", None),
            64_000
        );
    }

    #[test]
    fn resolve_legacy_claude_3_and_3_5() {
        assert_eq!(
            resolve_default_max_tokens("claude-3-opus-20240229", None),
            4_096
        );
        assert_eq!(
            resolve_default_max_tokens("claude-3-5-sonnet-20240620", None),
            8_192
        );
        assert_eq!(resolve_default_max_tokens("claude-3.5-sonnet", None), 8_192);
        assert_eq!(resolve_default_max_tokens("claude-sonnet-3-7", None), 8_192);
    }

    #[test]
    fn resolve_vendor_prefixed_and_suffixed() {
        assert_eq!(
            resolve_default_max_tokens("anthropic.claude-opus-4-7-v1:0", None),
            128_000
        );
        assert_eq!(
            resolve_default_max_tokens("aws/claude-opus-4-7", None),
            128_000
        );
        assert_eq!(
            resolve_default_max_tokens("openrouter/claude-opus-4-7", None),
            128_000
        );
        assert_eq!(
            resolve_default_max_tokens("claude-opus-4-6-construct", None),
            128_000
        );
        assert_eq!(
            resolve_default_max_tokens("anthropic.claude-3-5-sonnet-20240620-v1:0", None),
            8_192
        );
    }

    #[test]
    fn resolve_override_unknown_model() {
        assert_eq!(
            resolve_default_max_tokens("unknown-model", Some(12_345)),
            12_345
        );
    }

    #[test]
    fn resolve_override_lower_than_ceiling() {
        assert_eq!(
            resolve_default_max_tokens("claude-opus-4-7", Some(200)),
            200
        );
    }

    #[test]
    fn resolve_override_clamped_to_ceiling() {
        assert_eq!(
            resolve_default_max_tokens("claude-opus-4-7", Some(999_999)),
            128_000
        );
    }

    #[test]
    fn resolve_fallback_for_non_claude_ids() {
        assert_eq!(
            resolve_default_max_tokens("vendor-opus-4-7-preview", None),
            32_000
        );
        assert_eq!(
            resolve_default_max_tokens("vendor-opus-4-7-preview", Some(8_000)),
            8_000
        );
    }

    #[test]
    fn resolve_case_insensitive() {
        assert_eq!(resolve_default_max_tokens("CLAUDE-OPUS-4-7", None), 128_000);
    }
}

#[cfg(test)]
mod thinking_tests {
    use super::*;

    #[test]
    fn budget_tokens_for_effort_values() {
        assert_eq!(budget_tokens_for_effort(ThinkingEffort::Low), 1_024);
        assert_eq!(budget_tokens_for_effort(ThinkingEffort::Medium), 4_096);
        assert_eq!(budget_tokens_for_effort(ThinkingEffort::High), 32_000);
    }

    #[test]
    fn adaptive_for_4_6_and_later() {
        assert!(supports_adaptive_thinking("claude-opus-4-6"));
        assert!(supports_adaptive_thinking("claude-sonnet-4-6"));
        assert!(supports_adaptive_thinking("claude-opus-4-7"));
        assert!(!supports_adaptive_thinking("claude-opus-4-5"));
        assert!(!supports_adaptive_thinking("claude-sonnet-4-5"));
        assert!(!supports_adaptive_thinking("claude-3-5-sonnet"));
        assert!(!supports_adaptive_thinking("custom-model"));
    }

    #[test]
    fn clamp_effort_matrix() {
        // Opus 4.7: full range (xhigh passes through since it's opus-4-7;
        // max passes through since opus-4-7 is adaptive)
        assert_eq!(
            clamp_effort(ThinkingEffort::Xhigh, "claude-opus-4-7", false),
            ThinkingEffort::Xhigh
        );
        assert_eq!(
            clamp_effort(ThinkingEffort::Max, "claude-opus-4-7", true),
            ThinkingEffort::Max
        );
        // Pre-4.7 adaptive: xhigh -> high, max passes
        assert_eq!(
            clamp_effort(ThinkingEffort::Xhigh, "claude-opus-4-6", true),
            ThinkingEffort::High
        );
        assert_eq!(
            clamp_effort(ThinkingEffort::Max, "claude-opus-4-6", true),
            ThinkingEffort::Max
        );
        // Non-adaptive budget: max -> high
        assert_eq!(
            clamp_effort(ThinkingEffort::Max, "claude-sonnet-4-5", false),
            ThinkingEffort::High
        );
        assert_eq!(
            clamp_effort(ThinkingEffort::Off, "claude-opus-4-7", false),
            ThinkingEffort::Off
        );
    }

    #[test]
    fn thinking_config_disabled() {
        let cfg = build_thinking_config(ThinkingEffort::Off, "claude-opus-4-6", None);
        assert_eq!(cfg, (AnthropicThinkingConfig::Disabled, None));
    }

    #[test]
    fn thinking_config_adaptive_opus_47() {
        let (thinking, output) =
            build_thinking_config(ThinkingEffort::High, "claude-opus-4-7", None);
        assert_eq!(
            thinking,
            AnthropicThinkingConfig::Adaptive {
                display: "summarized".into()
            }
        );
        assert_eq!(
            output,
            Some(AnthropicOutputConfig {
                effort: "high".into()
            })
        );
    }

    #[test]
    fn thinking_config_budget_pre_46() {
        let (thinking, output) =
            build_thinking_config(ThinkingEffort::High, "claude-sonnet-4-20250514", None);
        assert_eq!(
            thinking,
            AnthropicThinkingConfig::Enabled {
                budget_tokens: 32_000
            }
        );
        assert_eq!(output, None);
    }

    #[test]
    fn thinking_config_opus_45_with_effort_param() {
        let (thinking, output) =
            build_thinking_config(ThinkingEffort::Xhigh, "claude-opus-4-5", None);
        assert_eq!(
            thinking,
            AnthropicThinkingConfig::Enabled {
                budget_tokens: 32_000
            }
        );
        assert_eq!(
            output,
            Some(AnthropicOutputConfig {
                effort: "high".into()
            })
        );
    }

    #[test]
    fn explicit_adaptive_override_for_unversioned_model() {
        let (thinking, output) =
            build_thinking_config(ThinkingEffort::Max, "custom-model", Some(true));
        assert_eq!(
            thinking,
            AnthropicThinkingConfig::Adaptive {
                display: "summarized".into()
            }
        );
        assert_eq!(
            output,
            Some(AnthropicOutputConfig {
                effort: "max".into()
            })
        );
    }

    #[test]
    fn explicit_non_adaptive_override_for_46() {
        let (thinking, output) =
            build_thinking_config(ThinkingEffort::High, "claude-opus-4-6", Some(false));
        assert_eq!(
            thinking,
            AnthropicThinkingConfig::Enabled {
                budget_tokens: 32_000
            }
        );
        assert_eq!(output, None);
    }
}

#[cfg(test)]
mod provider_tests {
    use super::*;
    use crate::provider::ThinkingEffort;

    fn test_provider(model: &str) -> AnthropicChatProvider {
        AnthropicChatProvider::new(AnthropicOptions {
            model: model.into(),
            api_key: Some("sk-test".into()),
            base_url: None,
            default_max_tokens: Some(1_024),
            beta_features: None,
            default_headers: None,
            metadata: None,
            stream: None,
            adaptive_thinking: None,
        })
    }

    #[test]
    fn name_and_model() {
        let p = test_provider("claude-opus-4-7");
        assert_eq!(p.name(), "anthropic");
        assert_eq!(p.model_name(), "claude-opus-4-7");
    }

    #[test]
    fn get_capability_claude_4_thinks() {
        let cap = test_provider("claude-opus-4-7").get_capability(None);
        assert!(cap.thinking);
        assert!(cap.image_in);
        assert!(cap.tool_use);
    }

    #[test]
    fn get_capability_for_specific_model() {
        let cap = test_provider("claude-opus-4-7").get_capability(Some("claude-3-5-sonnet"));
        assert!(!cap.thinking);
        assert!(cap.image_in);
    }

    #[test]
    fn with_thinking_returns_new_instance() {
        let p = test_provider("claude-opus-4-7");
        let q = p.with_thinking(ThinkingEffort::High);
        assert_eq!(q.thinking_effort(), Some(ThinkingEffort::High));
    }

    #[test]
    fn with_thinking_off_then_on() {
        let p = test_provider("claude-opus-4-7")
            .with_thinking(ThinkingEffort::High)
            .with_thinking(ThinkingEffort::Off);
        assert_eq!(p.thinking_effort(), Some(ThinkingEffort::Off));
    }

    #[test]
    fn with_max_completion_tokens_lowers_cap() {
        let p = test_provider("claude-opus-4-7").with_max_completion_tokens(8_192);
        assert_eq!(p.unwrap().thinking_effort(), None); // no thinking configured
    }

    #[test]
    fn thinking_effort_null_when_not_configured() {
        assert_eq!(test_provider("claude-opus-4-7").thinking_effort(), None);
    }
}

// ── Request content block types & conversion ───────────────────────────

use crate::message::Role;

const CACHEABLE_BLOCK_TYPES: &[&str] = &[
    "text",
    "image",
    "document",
    "search_result",
    "tool_use",
    "tool_result",
    "server_tool_use",
    "web_search_tool_result",
];

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CacheControl {
    pub r#type: String,
}

fn cache_control_ephemeral() -> CacheControl {
    CacheControl {
        r#type: "ephemeral".into(),
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ImageSource {
    Base64 { data: String, media_type: String },
    Url { url: String },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AnthropicToolResultContent {
    Text { text: String },
    Image { source: ImageSource },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AnthropicContentBlock {
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    Image {
        source: ImageSource,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    ToolResult {
        tool_use_id: String,
        content: Vec<AnthropicToolResultContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnthropicToolParam {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnthropicMessageParam {
    pub role: String,
    pub content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnthropicCreateParams {
    pub model: String,
    pub messages: Vec<AnthropicMessageParam>,
    pub max_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Vec<AnthropicContentBlock>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<AnthropicToolParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<AnthropicThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<AnthropicOutputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    pub stream: bool,
}

const SUPPORTED_B64_MEDIA_TYPES: &[&str] = &["image/png", "image/jpeg", "image/gif", "image/webp"];

pub fn convert_tool(tool: &Tool) -> AnthropicToolParam {
    AnthropicToolParam {
        name: tool.name.clone(),
        description: tool.description.clone(),
        input_schema: tool.parameters.clone(),
        cache_control: None,
    }
}

pub fn image_url_part_to_anthropic(url: &str) -> Result<AnthropicContentBlock, ChatProviderError> {
    if let Some(rest) = url.strip_prefix("data:") {
        let parts: Vec<&str> = rest.split(";base64,").collect();
        if parts.len() != 2 {
            return Err(ChatProviderError::Status(crate::errors::APIStatusError {
                status_code: 400,
                message: format!("Invalid data URL for image: {}", url),
                request_id: None,
            }));
        }
        let media_type = parts[0];
        let data = parts[1];
        if !SUPPORTED_B64_MEDIA_TYPES.contains(&media_type) {
            return Err(ChatProviderError::Status(crate::errors::APIStatusError {
                status_code: 400,
                message: format!("Unsupported media type for base64 image: {}", media_type),
                request_id: None,
            }));
        }
        Ok(AnthropicContentBlock::Image {
            source: ImageSource::Base64 {
                data: data.into(),
                media_type: media_type.into(),
            },
            cache_control: None,
        })
    } else {
        Ok(AnthropicContentBlock::Image {
            source: ImageSource::Url { url: url.into() },
            cache_control: None,
        })
    }
}

fn tool_result_to_block(
    tool_call_id: &str,
    content: &[ContentPart],
) -> Result<AnthropicContentBlock, ChatProviderError> {
    let mut blocks = Vec::new();
    for part in content {
        match part {
            ContentPart::Text { text } if !text.is_empty() => {
                blocks.push(AnthropicToolResultContent::Text { text: text.clone() });
            }
            ContentPart::ImageUrl { image_url } => {
                let block = image_url_part_to_anthropic(&image_url.url)?;
                if let AnthropicContentBlock::Image { source, .. } = block {
                    blocks.push(AnthropicToolResultContent::Image { source });
                }
            }
            _ => {}
        }
    }
    Ok(AnthropicContentBlock::ToolResult {
        tool_use_id: tool_call_id.into(),
        content: blocks,
        cache_control: None,
    })
}

fn should_preserve_unsigned_thinking(model: &str) -> bool {
    parse_claude_version(model).is_none()
}

pub fn convert_message(
    message: &Message,
    model: &str,
) -> Result<AnthropicMessageParam, ChatProviderError> {
    match message.role {
        Role::System => {
            let text = message
                .content
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            Ok(AnthropicMessageParam {
                role: "user".into(),
                content: vec![AnthropicContentBlock::Text {
                    text: format!("<system>{}</system>", text),
                    cache_control: None,
                }],
            })
        }
        Role::Tool => {
            let id = message.tool_call_id.as_deref().ok_or_else(|| {
                ChatProviderError::Status(crate::errors::APIStatusError {
                    status_code: 400,
                    message: "Tool message missing `toolCallId`.".into(),
                    request_id: None,
                })
            })?;
            Ok(AnthropicMessageParam {
                role: "user".into(),
                content: vec![tool_result_to_block(id, &message.content)?],
            })
        }
        Role::User | Role::Assistant => {
            let mut blocks = Vec::new();
            for part in &message.content {
                match part {
                    ContentPart::Text { text } => blocks.push(AnthropicContentBlock::Text {
                        text: text.clone(),
                        cache_control: None,
                    }),
                    ContentPart::ImageUrl { image_url } => {
                        blocks.push(image_url_part_to_anthropic(&image_url.url)?);
                    }
                    ContentPart::Think { think, encrypted } => {
                        if encrypted.is_some() {
                            blocks.push(AnthropicContentBlock::Thinking {
                                thinking: think.clone(),
                                signature: encrypted.clone(),
                            });
                        } else if !think.is_empty() && should_preserve_unsigned_thinking(model) {
                            blocks.push(AnthropicContentBlock::Thinking {
                                thinking: think.clone(),
                                signature: None,
                            });
                        }
                    }
                    _ => {}
                }
            }
            for tc in &message.tool_calls {
                let input = match tc.arguments.as_deref() {
                    Some(args) => {
                        let parsed: serde_json::Value =
                            serde_json::from_str(args).map_err(|_| {
                                ChatProviderError::Status(crate::errors::APIStatusError {
                                    status_code: 400,
                                    message: "Tool call arguments must be valid JSON.".into(),
                                    request_id: None,
                                })
                            })?;
                        if !parsed.is_object() {
                            return Err(ChatProviderError::Status(crate::errors::APIStatusError {
                                status_code: 400,
                                message: "Tool call arguments must be a JSON object.".into(),
                                request_id: None,
                            }));
                        }
                        parsed
                    }
                    None => serde_json::Value::Object(serde_json::Map::new()),
                };
                blocks.push(AnthropicContentBlock::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input,
                    cache_control: None,
                });
            }
            Ok(AnthropicMessageParam {
                role: match message.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    _ => unreachable!(),
                }
                .into(),
                content: blocks,
            })
        }
    }
}

// ── Message merge & cache control helpers ────────────────────────────────

fn is_tool_result_only(message: &AnthropicMessageParam) -> bool {
    if message.role != "user" {
        return false;
    }
    !message.content.is_empty()
        && message
            .content
            .iter()
            .all(|b| matches!(b, AnthropicContentBlock::ToolResult { .. }))
}

pub fn inject_cache_control_on_last_block(messages: &mut [AnthropicMessageParam]) {
    let last = match messages.last_mut() {
        Some(l) => l,
        None => return,
    };
    let block = match last.content.last_mut() {
        Some(b) => b,
        None => return,
    };
    let type_name = match block {
        AnthropicContentBlock::Text { .. } => "text",
        AnthropicContentBlock::Image { .. } => "image",
        AnthropicContentBlock::ToolUse { .. } => "tool_use",
        AnthropicContentBlock::ToolResult { .. } => "tool_result",
        _ => return,
    };
    if CACHEABLE_BLOCK_TYPES.contains(&type_name) {
        let cc = cache_control_ephemeral();
        match block {
            AnthropicContentBlock::Text { cache_control, .. }
            | AnthropicContentBlock::Image { cache_control, .. }
            | AnthropicContentBlock::ToolUse { cache_control, .. }
            | AnthropicContentBlock::ToolResult { cache_control, .. } => {
                *cache_control = Some(cc);
            }
            _ => {}
        }
    }
}

pub fn merge_parallel_tool_results(
    messages: &[Message],
    model: &str,
) -> Result<Vec<AnthropicMessageParam>, ChatProviderError> {
    let mut out: Vec<AnthropicMessageParam> = Vec::new();
    for msg in messages {
        let converted = convert_message(msg, model)?;
        if let Some(last) = out.last_mut() {
            if is_tool_result_only(last) && is_tool_result_only(&converted) {
                last.content.extend(converted.content);
                continue;
            }
        }
        out.push(converted);
    }
    Ok(out)
}

pub fn build_system_param(system_prompt: &str) -> Vec<AnthropicContentBlock> {
    let trimmed = system_prompt.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    vec![AnthropicContentBlock::Text {
        text: trimmed.into(),
        cache_control: Some(cache_control_ephemeral()),
    }]
}

// ── Request builder methods on AnthropicChatProvider ─────────────────────

use crate::request_auth::require_provider_api_key;
use crate::tool_call_id::{
    normalize_tool_call_ids_for_provider, sanitize_tool_call_id, ToolCallIdPolicy,
};

impl AnthropicChatProvider {
    pub(crate) fn resolve_api_key(
        &self,
        auth: Option<&ProviderRequestAuth>,
    ) -> Result<String, ChatProviderError> {
        require_provider_api_key("anthropic", auth, self.api_key.as_deref())
    }

    pub(crate) fn build_extra_headers(
        &self,
        auth: Option<&ProviderRequestAuth>,
    ) -> Result<HashMap<String, String>, ChatProviderError> {
        let mut headers = HashMap::new();
        headers.insert("content-type".into(), "application/json".into());
        headers.insert("anthropic-version".into(), "2023-06-01".into());
        headers.insert("x-api-key".into(), self.resolve_api_key(auth)?);

        let betas = &self.generation_kwargs.beta_features;
        if !betas.is_empty() {
            headers.insert("anthropic-beta".into(), betas.join(","));
        }

        if let Some(default) = &self.default_headers {
            for (k, v) in default {
                headers.insert(k.clone(), v.clone());
            }
        }
        if let Some(request_headers) = auth.and_then(|a| a.headers.as_ref()) {
            for (k, v) in request_headers {
                headers.insert(k.clone(), v.clone());
            }
        }
        Ok(headers)
    }

    pub(crate) fn build_create_params(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        history: &[Message],
        stream: bool,
    ) -> Result<AnthropicCreateParams, ChatProviderError> {
        let normalized = normalize_tool_call_ids_for_provider(
            history,
            &ToolCallIdPolicy::new(|id| sanitize_tool_call_id(id, Some(64)), Some(64)),
        );
        let mut messages = merge_parallel_tool_results(&normalized, &self.model)?;
        inject_cache_control_on_last_block(&mut messages);

        let mut anthropic_tools: Vec<AnthropicToolParam> = tools.iter().map(convert_tool).collect();
        if let Some(last) = anthropic_tools.last_mut() {
            last.cache_control = Some(cache_control_ephemeral());
        }

        let system = {
            let blocks = build_system_param(system_prompt);
            if blocks.is_empty() {
                None
            } else {
                Some(blocks)
            }
        };

        Ok(AnthropicCreateParams {
            model: self.model.clone(),
            messages,
            max_tokens: self.generation_kwargs.max_tokens,
            system,
            tools: if anthropic_tools.is_empty() {
                None
            } else {
                Some(anthropic_tools)
            },
            temperature: self.generation_kwargs.temperature,
            top_k: self.generation_kwargs.top_k,
            top_p: self.generation_kwargs.top_p,
            thinking: self.generation_kwargs.thinking.clone(),
            output_config: self.generation_kwargs.output_config.clone(),
            metadata: self.metadata.clone(),
            stream,
        })
    }
}

use crate::provider::ProviderRequestAuth;

// ── Request tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod request_tests {
    use super::*;
    use crate::message::{ContentPart, Message, Role, ToolCall};

    #[test]
    fn convert_tool_uses_input_schema() {
        let tool = Tool {
            name: "read".into(),
            description: "read a file".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        };
        let at = convert_tool(&tool);
        assert_eq!(at.name, "read");
        assert_eq!(at.description, "read a file");
        assert_eq!(at.input_schema, tool.parameters);
        assert!(at.cache_control.is_none());
    }

    #[test]
    fn convert_system_message_wraps_in_user_system_tag() {
        let msg = Message {
            role: Role::System,
            name: None,
            content: vec![ContentPart::Text {
                text: "be helpful".into(),
            }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let out = convert_message(&msg, "claude-opus-4-7").unwrap();
        assert_eq!(out.role, "user");
        assert_eq!(out.content.len(), 1);
        match &out.content[0] {
            AnthropicContentBlock::Text { text, .. } => {
                assert_eq!(text, "<system>be helpful</system>")
            }
            _ => panic!("expected text block"),
        }
    }

    #[test]
    fn convert_tool_message_to_tool_result() {
        let msg = Message {
            role: Role::Tool,
            name: None,
            content: vec![ContentPart::Text { text: "42".into() }],
            tool_calls: vec![],
            tool_call_id: Some("tc_1".into()),
            partial: None,
        };
        let out = convert_message(&msg, "claude-opus-4-7").unwrap();
        assert_eq!(out.role, "user");
        match &out.content[0] {
            AnthropicContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => {
                assert_eq!(tool_use_id, "tc_1");
                assert_eq!(content.len(), 1);
                match &content[0] {
                    AnthropicToolResultContent::Text { text } => assert_eq!(text, "42"),
                    _ => panic!("expected text"),
                }
            }
            _ => panic!("expected tool_result"),
        }
    }

    #[test]
    fn convert_tool_message_requires_tool_call_id() {
        let msg = Message {
            role: Role::Tool,
            name: None,
            content: vec![ContentPart::Text { text: "x".into() }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let err = convert_message(&msg, "claude-opus-4-7").unwrap_err();
        assert!(err.to_string().contains("toolCallId"));
    }

    #[test]
    fn convert_image_url_data_to_base64_source() {
        let url = "data:image/png;base64,abcd";
        let block = image_url_part_to_anthropic(url).unwrap();
        match block {
            AnthropicContentBlock::Image {
                source:
                    ImageSource::Base64 {
                        data, media_type, ..
                    },
                ..
            } => {
                assert_eq!(data, "abcd");
                assert_eq!(media_type, "image/png");
            }
            _ => panic!("expected base64 image"),
        }
    }

    #[test]
    fn convert_image_url_remote_to_url_source() {
        let block = image_url_part_to_anthropic("https://example.com/x.png").unwrap();
        match block {
            AnthropicContentBlock::Image {
                source: ImageSource::Url { url },
                ..
            } => {
                assert_eq!(url, "https://example.com/x.png");
            }
            _ => panic!("expected url image"),
        }
    }

    #[test]
    fn convert_image_rejects_unsupported_media_type() {
        let err = image_url_part_to_anthropic("data:image/bmp;base64,abcd").unwrap_err();
        assert!(err.to_string().contains("Unsupported media type"));
    }

    #[test]
    fn convert_signed_thinking_preserved() {
        let msg = Message {
            role: Role::Assistant,
            name: None,
            content: vec![ContentPart::Think {
                think: "reason".into(),
                encrypted: Some("sig".into()),
            }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let out = convert_message(&msg, "claude-opus-4-7").unwrap();
        match &out.content[0] {
            AnthropicContentBlock::Thinking {
                thinking,
                signature,
            } => {
                assert_eq!(thinking, "reason");
                assert_eq!(signature.as_deref(), Some("sig"));
            }
            _ => panic!("expected thinking block"),
        }
    }

    #[test]
    fn convert_unsigned_thinking_dropped_for_claude() {
        let msg = Message {
            role: Role::Assistant,
            name: None,
            content: vec![ContentPart::Think {
                think: "reason".into(),
                encrypted: None,
            }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let out = convert_message(&msg, "claude-opus-4-7").unwrap();
        assert!(out.content.is_empty());
    }

    #[test]
    fn convert_unsigned_thinking_preserved_for_non_claude_alias() {
        let msg = Message {
            role: Role::Assistant,
            name: None,
            content: vec![ContentPart::Think {
                think: "reason".into(),
                encrypted: None,
            }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let out = convert_message(&msg, "custom-compatible-model").unwrap();
        match &out.content[0] {
            AnthropicContentBlock::Thinking {
                thinking,
                signature,
            } => {
                assert_eq!(thinking, "reason");
                assert!(signature.is_none());
            }
            _ => panic!("expected thinking block"),
        }
    }

    #[test]
    fn convert_tool_call_to_tool_use() {
        let tc = ToolCall {
            call_type: "function".into(),
            id: "tc_1".into(),
            name: "read".into(),
            arguments: Some("{\"path\":\"/etc/passwd\"}".into()),
            extras: None,
            stream_index: None,
        };
        let msg = Message {
            role: Role::Assistant,
            name: None,
            content: vec![],
            tool_calls: vec![tc],
            tool_call_id: None,
            partial: None,
        };
        let out = convert_message(&msg, "claude-opus-4-7").unwrap();
        match &out.content[0] {
            AnthropicContentBlock::ToolUse {
                id, name, input, ..
            } => {
                assert_eq!(id, "tc_1");
                assert_eq!(name, "read");
                assert_eq!(input, &serde_json::json!({"path": "/etc/passwd"}));
            }
            _ => panic!("expected tool_use"),
        }
    }

    #[test]
    fn convert_tool_call_rejects_non_object_arguments() {
        let tc = ToolCall {
            call_type: "function".into(),
            id: "tc_1".into(),
            name: "read".into(),
            arguments: Some("\"not-an-object\"".into()),
            extras: None,
            stream_index: None,
        };
        let msg = Message::assistant(vec![], vec![tc]);
        let err = convert_message(&msg, "claude-opus-4-7").unwrap_err();
        assert!(err.to_string().contains("JSON object"));
    }
}

#[cfg(test)]
mod message_build_tests {
    use super::*;
    use crate::message::{ContentPart, Message, Role};

    fn tool_result_msg(id: &str, text: &str) -> Message {
        Message {
            role: Role::Tool,
            name: None,
            content: vec![ContentPart::Text { text: text.into() }],
            tool_calls: vec![],
            tool_call_id: Some(id.into()),
            partial: None,
        }
    }

    #[test]
    fn merge_consecutive_tool_result_only_messages() {
        let msgs = vec![tool_result_msg("a", "1"), tool_result_msg("b", "2")];
        let out = merge_parallel_tool_results(&msgs, "claude-opus-4-7").unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "user");
        assert_eq!(out[0].content.len(), 2);
        match (&out[0].content[0], &out[0].content[1]) {
            (
                AnthropicContentBlock::ToolResult { tool_use_id: a, .. },
                AnthropicContentBlock::ToolResult { tool_use_id: b, .. },
            ) => {
                assert_eq!(a, "a");
                assert_eq!(b, "b");
            }
            _ => panic!("expected two tool_result blocks"),
        }
    }

    #[test]
    fn do_not_merge_tool_result_with_interleaved_user_message() {
        let msgs = vec![
            tool_result_msg("a", "1"),
            Message::user_text("ok?"),
            tool_result_msg("b", "2"),
        ];
        let out = merge_parallel_tool_results(&msgs, "claude-opus-4-7").unwrap();
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn inject_cache_control_on_last_text_block() {
        let msgs = vec![Message::user_text("hello")];
        let mut converted = merge_parallel_tool_results(&msgs, "claude-opus-4-7").unwrap();
        inject_cache_control_on_last_block(&mut converted);
        match &converted[0].content[0] {
            AnthropicContentBlock::Text { cache_control, .. } => {
                assert_eq!(cache_control.as_ref().unwrap().r#type, "ephemeral");
            }
            _ => panic!("expected text with cache_control"),
        }
    }

    #[test]
    fn inject_cache_control_on_last_tool_result_after_merge() {
        let msgs = vec![tool_result_msg("a", "1"), tool_result_msg("b", "2")];
        let mut converted = merge_parallel_tool_results(&msgs, "claude-opus-4-7").unwrap();
        inject_cache_control_on_last_block(&mut converted);
        match converted[0].content.last().unwrap() {
            AnthropicContentBlock::ToolResult { cache_control, .. } => {
                assert_eq!(cache_control.as_ref().unwrap().r#type, "ephemeral");
            }
            _ => panic!("expected cache_control on last tool_result"),
        }
    }

    #[test]
    fn system_prompt_becomes_system_param_with_cache_control() {
        let system = build_system_param("be helpful");
        assert_eq!(system.len(), 1);
        match &system[0] {
            AnthropicContentBlock::Text {
                text,
                cache_control,
            } => {
                assert_eq!(text, "be helpful");
                assert_eq!(cache_control.as_ref().unwrap().r#type, "ephemeral");
            }
            _ => panic!("expected system text block"),
        }
    }

    #[test]
    fn empty_system_prompt_returns_none() {
        assert!(build_system_param("").is_empty());
        assert!(build_system_param("   ").is_empty());
    }
}

#[cfg(test)]
mod params_tests {
    use super::*;
    use crate::provider::{ProviderRequestAuth, ThinkingEffort};

    fn provider(model: &str) -> AnthropicChatProvider {
        AnthropicChatProvider::new(AnthropicOptions {
            model: model.into(),
            api_key: Some("sk-test".into()),
            base_url: None,
            default_max_tokens: None,
            beta_features: None,
            default_headers: None,
            metadata: None,
            stream: Some(false),
            adaptive_thinking: None,
        })
    }

    #[test]
    fn build_create_params_serializes_stream_false() {
        let p = provider("claude-opus-4-7");
        let params = p
            .build_create_params("be helpful", &[], &[Message::user_text("hi")], false)
            .unwrap();
        let v = serde_json::to_value(&params).unwrap();
        assert_eq!(v["model"], "claude-opus-4-7");
        assert_eq!(v["stream"], false);
        assert_eq!(v["maxTokens"], 128_000);
        assert!(v["system"].is_array());
        assert_eq!(v["messages"][0]["role"], "user");
    }

    #[test]
    fn build_create_params_includes_thinking_and_output_config() {
        // Build thinking manually via build_thinking_config, then create params
        let p = AnthropicChatProvider::new(AnthropicOptions {
            model: "claude-opus-4-7".into(),
            api_key: Some("sk-test".into()),
            base_url: None,
            default_max_tokens: None,
            beta_features: None,
            default_headers: None,
            metadata: None,
            stream: Some(false),
            adaptive_thinking: None,
        });
        let (thinking, output) =
            build_thinking_config(ThinkingEffort::High, "claude-opus-4-7", None);
        let patch = AnthropicGenerationKwargsPatch {
            thinking: Some(thinking),
            output_config: output,
            output_config_set: true,
            beta_features: Some(
                p.generation_kwargs
                    .beta_features
                    .iter()
                    .cloned()
                    .filter(|b| b != INTERLEAVED_THINKING_BETA)
                    .collect(),
            ),
            ..Default::default()
        };
        let anthropic_p = p.clone_with_generation_kwargs(patch);
        let params = anthropic_p
            .build_create_params("", &[], &[Message::user_text("hi")], false)
            .unwrap();
        let v = serde_json::to_value(&params).unwrap();
        assert_eq!(v["thinking"]["type"], "adaptive");
        assert_eq!(v["thinking"]["display"], "summarized");
        assert_eq!(v["outputConfig"]["effort"], "high");
    }

    #[test]
    fn build_create_params_injects_tool_cache_control() {
        let tool = Tool {
            name: "read".into(),
            description: "read".into(),
            parameters: serde_json::json!({}),
        };
        let p = provider("claude-opus-4-7");
        let params = p
            .build_create_params("", &[tool], &[Message::user_text("hi")], false)
            .unwrap();
        let v = serde_json::to_value(&params).unwrap();
        assert_eq!(v["tools"][0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn build_extra_headers_contains_beta_and_version() {
        let p = provider("claude-opus-4-7");
        let headers = p
            .build_extra_headers(Some(&ProviderRequestAuth {
                api_key: None,
                headers: None,
            }))
            .unwrap();
        assert!(headers.contains_key("x-api-key"));
        assert!(headers.contains_key("anthropic-version"));
        assert!(headers.contains_key("anthropic-beta"));
        assert!(headers.contains_key("content-type"));
    }

    #[test]
    fn request_headers_merge_auth_headers() {
        let p = provider("claude-opus-4-7");
        let mut req_headers = HashMap::new();
        req_headers.insert("x-custom".into(), "v".into());
        let headers = p
            .build_extra_headers(Some(&ProviderRequestAuth {
                api_key: None,
                headers: Some(req_headers),
            }))
            .unwrap();
        assert_eq!(headers.get("x-custom").unwrap(), "v");
    }

    #[test]
    fn resolve_api_key_prefers_request_auth() {
        let p = AnthropicChatProvider::new(AnthropicOptions {
            model: "claude-opus-4-7".into(),
            api_key: Some("default".into()),
            base_url: None,
            default_max_tokens: None,
            beta_features: None,
            default_headers: None,
            metadata: None,
            stream: Some(false),
            adaptive_thinking: None,
        });
        let auth = crate::provider::ProviderRequestAuth {
            api_key: Some("request".into()),
            headers: None,
        };
        assert_eq!(p.resolve_api_key(Some(&auth)).unwrap(), "request");
    }
}

// ── Response types & parsing ────────────────────────────────────────────

use crate::errors::{APIConnectionError, APITimeoutError};
use crate::usage::TokenUsage;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnthropicUsage {
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    #[serde(default)]
    pub cache_read_input_tokens: i64,
    #[serde(default)]
    pub cache_creation_input_tokens: i64,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AnthropicResponseContentBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
        signature: Option<String>,
    },
    RedactedThinking {
        data: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnthropicMessageResponse {
    pub id: String,
    pub stop_reason: Option<String>,
    pub usage: AnthropicUsage,
    pub content: Vec<AnthropicResponseContentBlock>,
}

fn normalize_stop_reason(raw: Option<&str>) -> (Option<FinishReason>, Option<String>) {
    let raw = match raw {
        Some(r) => r,
        None => return (None, None),
    };
    let finish = match raw {
        "end_turn" | "stop_sequence" => FinishReason::Completed,
        "max_tokens" => FinishReason::Truncated,
        "tool_use" => FinishReason::ToolCalls,
        "pause_turn" => FinishReason::Paused,
        "refusal" => FinishReason::Filtered,
        _ => FinishReason::Other,
    };
    (Some(finish), Some(raw.into()))
}

fn parse_non_stream_response(
    response: AnthropicMessageResponse,
) -> Result<StreamedMessage, ChatProviderError> {
    let mut parts = Vec::new();
    for block in response.content {
        match block {
            AnthropicResponseContentBlock::Text { text } => {
                parts.push(StreamedMessagePart::text(text));
            }
            AnthropicResponseContentBlock::Thinking {
                thinking,
                signature,
            } => {
                parts.push(StreamedMessagePart::Content(ContentPart::Think {
                    think: thinking,
                    encrypted: signature,
                }));
            }
            AnthropicResponseContentBlock::RedactedThinking { data } => {
                parts.push(StreamedMessagePart::Content(ContentPart::Think {
                    think: String::new(),
                    encrypted: Some(data),
                }));
            }
            AnthropicResponseContentBlock::ToolUse { id, name, input } => {
                parts.push(StreamedMessagePart::tool_call(
                    id,
                    name,
                    Some(&input.to_string()),
                ));
            }
        }
    }

    let usage = TokenUsage {
        input_other: response.usage.input_tokens,
        output: response.usage.output_tokens,
        input_cache_read: response.usage.cache_read_input_tokens,
        input_cache_creation: response.usage.cache_creation_input_tokens,
    };

    let (finish_reason, raw_finish_reason) = normalize_stop_reason(response.stop_reason.as_deref());

    Ok(StreamedMessage::from_parts(
        parts,
        Some(response.id),
        Some(usage),
        finish_reason,
        raw_finish_reason,
    ))
}

// ── SSE event types & parser ────────────────────────────────────────────

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AnthropicSseEvent {
    MessageStart {
        message: AnthropicSseMessageStart,
    },
    ContentBlockStart {
        index: usize,
        content_block: AnthropicSseContentBlock,
    },
    ContentBlockDelta {
        index: usize,
        delta: AnthropicSseDelta,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        delta: AnthropicSseMessageDelta,
        usage: Option<AnthropicUsage>,
    },
    MessageStop,
    #[serde(rename = "error")]
    Error {
        error: serde_json::Value,
    },
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnthropicSseMessageStart {
    pub id: String,
    pub usage: AnthropicUsage,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnthropicSseContentBlock {
    pub r#type: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub input: Option<serde_json::Value>,
    #[serde(default)]
    pub signature: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AnthropicSseDelta {
    TextDelta { text: String },
    ThinkingDelta { thinking: String },
    InputJsonDelta { partial_json: String },
    SignatureDelta { signature: String },
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct AnthropicSseMessageDelta {
    #[serde(default)]
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSseEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
}

fn parse_sse_body(body: &str) -> Vec<ParsedSseEvent> {
    let mut events = Vec::new();
    let mut current_event: Option<String> = None;
    let mut current_data = String::new();

    for raw_line in body.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.is_empty() {
            if let Some(et) = current_event.take() {
                if !current_data.is_empty() {
                    if let Ok(payload) = serde_json::from_str(&current_data) {
                        events.push(ParsedSseEvent {
                            event_type: et,
                            payload,
                        });
                    }
                }
                current_data.clear();
            }
            continue;
        }
        if line.starts_with(':') {
            continue;
        }
        if let Some(et) = line.strip_prefix("event: ") {
            current_event = Some(et.to_string());
        } else if let Some(data) = line.strip_prefix("data: ") {
            if !current_data.is_empty() {
                current_data.push('\n');
            }
            current_data.push_str(data);
        }
    }

    if let Some(et) = current_event.take() {
        if !current_data.is_empty() {
            if let Ok(payload) = serde_json::from_str(&current_data) {
                events.push(ParsedSseEvent {
                    event_type: et,
                    payload,
                });
            }
        }
    }

    events
}

// ── Streaming adapter ───────────────────────────────────────────────────

#[derive(Debug, Default)]
struct AnthropicStreamState {
    id: Option<String>,
    usage: TokenUsage,
    finish_reason: Option<FinishReason>,
    raw_finish_reason: Option<String>,
}

pub fn anthropic_events_to_streamed_message(
    events: Vec<ParsedSseEvent>,
) -> Result<StreamedMessage, ChatProviderError> {
    let mut state = AnthropicStreamState::default();
    let mut parts = Vec::new();

    for event in events {
        let sse: AnthropicSseEvent = serde_json::from_value(event.payload).map_err(|e| {
            ChatProviderError::Status(crate::errors::APIStatusError {
                status_code: 500,
                message: format!("Invalid Anthropic SSE event: {}", e),
                request_id: None,
            })
        })?;
        match sse {
            AnthropicSseEvent::MessageStart { message } => {
                state.id = Some(message.id);
                state.usage.input_other = message.usage.input_tokens;
                state.usage.output = message.usage.output_tokens;
                state.usage.input_cache_read = message.usage.cache_read_input_tokens;
                state.usage.input_cache_creation = message.usage.cache_creation_input_tokens;
            }
            AnthropicSseEvent::ContentBlockStart {
                index,
                content_block,
            } => match content_block.r#type.as_str() {
                "text" => {
                    if let Some(text) = content_block.text {
                        parts.push(StreamedMessagePart::text(text));
                    }
                }
                "thinking" => {
                    parts.push(StreamedMessagePart::Content(ContentPart::Think {
                        think: content_block.thinking.unwrap_or_default(),
                        encrypted: None,
                    }));
                }
                "redacted_thinking" => {
                    parts.push(StreamedMessagePart::Content(ContentPart::Think {
                        think: String::new(),
                        encrypted: content_block.data,
                    }));
                }
                "tool_use" => {
                    let idx = content_block.id.as_deref().unwrap_or("");
                    let name = content_block.name.as_deref().unwrap_or("");
                    let mut tc = ToolCall {
                        call_type: "function".into(),
                        id: idx.into(),
                        name: name.into(),
                        arguments: Some(String::new()),
                        extras: None,
                        stream_index: None,
                    };
                    // Use the numeric index (not id) for parallel tool call routing
                    tc.stream_index = Some(serde_json::json!(index));
                    parts.push(StreamedMessagePart::ToolCall(tc));
                }
                _ => {}
            },
            AnthropicSseEvent::ContentBlockDelta { index, delta } => match delta {
                AnthropicSseDelta::TextDelta { text } => {
                    parts.push(StreamedMessagePart::text(text));
                }
                AnthropicSseDelta::ThinkingDelta { thinking } => {
                    parts.push(StreamedMessagePart::Content(ContentPart::Think {
                        think: thinking,
                        encrypted: None,
                    }));
                }
                AnthropicSseDelta::InputJsonDelta { partial_json } => {
                    parts.push(StreamedMessagePart::ToolCallPart(
                        crate::message::ToolCallPart {
                            part_type: "tool_call_part".to_string(),
                            arguments_part: Some(partial_json),
                            index: Some(serde_json::json!(index)),
                        },
                    ));
                }
                AnthropicSseDelta::SignatureDelta { signature } => {
                    parts.push(StreamedMessagePart::Content(ContentPart::Think {
                        think: String::new(),
                        encrypted: Some(signature),
                    }));
                }
            },
            AnthropicSseEvent::ContentBlockStop { .. } => {}
            AnthropicSseEvent::MessageDelta { delta, usage } => {
                if let Some(u) = usage {
                    state.usage.input_other = u.input_tokens;
                    state.usage.output = u.output_tokens;
                    state.usage.input_cache_read = u.cache_read_input_tokens;
                    state.usage.input_cache_creation = u.cache_creation_input_tokens;
                }
                if let Some(raw) = delta.stop_reason {
                    let (fr, raw_str) = normalize_stop_reason(Some(&raw));
                    state.finish_reason = fr;
                    state.raw_finish_reason = raw_str;
                }
            }
            AnthropicSseEvent::MessageStop => {}
            AnthropicSseEvent::Error { error } => {
                return Err(ChatProviderError::Status(crate::errors::APIStatusError {
                    status_code: 500,
                    message: format!("Anthropic SSE error event: {}", error),
                    request_id: None,
                }));
            }
        }
    }

    let usage = if state.usage.input_other == 0
        && state.usage.output == 0
        && state.usage.input_cache_read == 0
        && state.usage.input_cache_creation == 0
    {
        None
    } else {
        Some(state.usage)
    };

    Ok(StreamedMessage::from_parts(
        parts,
        state.id,
        usage,
        state.finish_reason,
        state.raw_finish_reason,
    ))
}

// ── Error mapping ───────────────────────────────────────────────────────

pub fn convert_anthropic_request_error(error: reqwest::Error) -> ChatProviderError {
    if error.is_timeout() {
        ChatProviderError::Timeout(APITimeoutError)
    } else {
        ChatProviderError::Connection(APIConnectionError)
    }
}

pub fn convert_anthropic_api_error(
    status: u16,
    body: &str,
    fallback_request_id: Option<String>,
) -> ChatProviderError {
    let request_id = fallback_request_id.or_else(|| {
        serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| {
                v.get("request_id")
                    .and_then(|r| r.as_str())
                    .map(|s| s.to_string())
            })
    });
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message").or(Some(e)))
                .and_then(|m| m.as_str().map(|s| s.to_string()))
        })
        .unwrap_or_else(|| body.into());
    normalize_api_status_error(status, message, request_id)
}

use crate::errors::normalize_api_status_error;

// ── Response tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod sse_tests {
    use super::*;

    #[test]
    fn parse_sse_text_event() {
        let body = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n";
        let events = parse_sse_body(body);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "content_block_delta");
    }

    #[test]
    fn parse_sse_skips_comments_and_empty_lines() {
        let body = ":comment\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
        let events = parse_sse_body(body);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "message_stop");
    }

    #[test]
    fn parse_sse_multiple_events() {
        let body = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
        let events = parse_sse_body(body);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn deserialize_message_start_event() {
        let json = serde_json::json!({
            "type": "message_start",
            "message": {
                "id": "msg_1",
                "usage": { "input_tokens": 10, "output_tokens": 0 }
            }
        });
        let evt: AnthropicSseEvent = serde_json::from_value(json).unwrap();
        match evt {
            AnthropicSseEvent::MessageStart { message } => {
                assert_eq!(message.id, "msg_1");
                assert_eq!(message.usage.input_tokens, 10);
            }
            _ => panic!("expected message_start"),
        }
    }

    #[test]
    fn deserialize_content_block_start_tool_use() {
        let json = serde_json::json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": { "type": "tool_use", "id": "tc_1", "name": "read", "input": {} }
        });
        let evt: AnthropicSseEvent = serde_json::from_value(json).unwrap();
        match evt {
            AnthropicSseEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                assert_eq!(index, 0);
                assert_eq!(content_block.r#type, "tool_use");
            }
            _ => panic!("expected content_block_start"),
        }
    }

    #[test]
    fn deserialize_input_json_delta() {
        let json = serde_json::json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": { "type": "input_json_delta", "partial_json": "{\"a\":1" }
        });
        let evt: AnthropicSseEvent = serde_json::from_value(json).unwrap();
        match evt {
            AnthropicSseEvent::ContentBlockDelta { delta, .. } => match delta {
                AnthropicSseDelta::InputJsonDelta { partial_json } => {
                    assert_eq!(partial_json, "{\"a\":1")
                }
                _ => panic!("expected input_json_delta"),
            },
            _ => panic!("expected content_block_delta"),
        }
    }
}

#[cfg(test)]
mod stream_adapter_tests {
    use super::*;

    fn text_event(text: &str) -> ParsedSseEvent {
        ParsedSseEvent {
            event_type: "content_block_delta".into(),
            payload: serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "text_delta", "text": text }
            }),
        }
    }

    fn make_msg_start() -> ParsedSseEvent {
        ParsedSseEvent {
            event_type: "message_start".into(),
            payload: serde_json::json!({
                "type":"message_start",
                "message":{
                    "id":"msg_1",
                    "usage":{"input_tokens":10,"output_tokens":0}
                }
            }),
        }
    }

    fn make_msg_delta(stop_reason: &str) -> ParsedSseEvent {
        ParsedSseEvent {
            event_type: "message_delta".into(),
            payload: serde_json::json!({
                "type":"message_delta",
                "delta":{"stop_reason": stop_reason},
                "usage":{"output_tokens":2}
            }),
        }
    }

    fn make_msg_stop() -> ParsedSseEvent {
        ParsedSseEvent {
            event_type: "message_stop".into(),
            payload: serde_json::json!({"type":"message_stop"}),
        }
    }

    #[tokio::test]
    async fn text_events_yield_merged_text() {
        let events = vec![
            make_msg_start(),
            text_event("Hello"),
            text_event(" world"),
            make_msg_delta("end_turn"),
            make_msg_stop(),
        ];
        let stream = anthropic_events_to_streamed_message(events).unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(stream).await;
        // generate loop merges consecutive text, but the adapter returns raw parts
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], StreamedMessagePart::text("Hello"));
        assert_eq!(parts[1], StreamedMessagePart::text(" world"));
    }

    #[tokio::test]
    async fn thinking_and_signature_events() {
        let events = vec![
            ParsedSseEvent {
                event_type: "content_block_start".into(),
                payload: serde_json::json!({
                    "type":"content_block_start",
                    "index":0,
                    "content_block":{"type":"thinking","thinking":"step1"}
                }),
            },
            ParsedSseEvent {
                event_type: "content_block_delta".into(),
                payload: serde_json::json!({
                    "type":"content_block_delta",
                    "index":0,
                    "delta":{"type":"thinking_delta","thinking":" step2"}
                }),
            },
            ParsedSseEvent {
                event_type: "content_block_delta".into(),
                payload: serde_json::json!({
                    "type":"content_block_delta",
                    "index":0,
                    "delta":{"type":"signature_delta","signature":"sig"}
                }),
            },
        ];
        let stream = anthropic_events_to_streamed_message(events).unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(stream).await;
        assert_eq!(parts.len(), 3);
        assert_eq!(
            parts[0],
            StreamedMessagePart::Content(ContentPart::Think {
                think: "step1".into(),
                encrypted: None
            })
        );
        assert_eq!(
            parts[1],
            StreamedMessagePart::Content(ContentPart::Think {
                think: " step2".into(),
                encrypted: None
            })
        );
        assert_eq!(
            parts[2],
            StreamedMessagePart::Content(ContentPart::Think {
                think: "".into(),
                encrypted: Some("sig".into())
            })
        );
    }

    #[tokio::test]
    async fn tool_use_start_and_input_json_deltas() {
        let events = vec![
            ParsedSseEvent {
                event_type: "content_block_start".into(),
                payload: serde_json::json!({
                    "type":"content_block_start",
                    "index":0,
                    "content_block":{"type":"tool_use","id":"tc_1","name":"read","input":{}}
                }),
            },
            ParsedSseEvent {
                event_type: "content_block_delta".into(),
                payload: serde_json::json!({
                    "type":"content_block_delta",
                    "index":0,
                    "delta":{"type":"input_json_delta","partial_json":"{\"path\":\""}
                }),
            },
            ParsedSseEvent {
                event_type: "content_block_delta".into(),
                payload: serde_json::json!({
                    "type":"content_block_delta",
                    "index":0,
                    "delta":{"type":"input_json_delta","partial_json":"/etc/passwd\"}"}
                }),
            },
        ];
        let stream = anthropic_events_to_streamed_message(events).unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(stream).await;
        assert_eq!(parts.len(), 3);
        // ToolCall part
        match &parts[0] {
            StreamedMessagePart::ToolCall(tc) => {
                assert_eq!(tc.name, "read");
                assert_eq!(tc.arguments.as_deref(), Some(""));
            }
            _ => panic!("expected tool call"),
        }
        match &parts[1] {
            StreamedMessagePart::ToolCallPart(p) => {
                assert_eq!(p.arguments_part.as_deref(), Some("{\"path\":\""));
            }
            _ => panic!("expected tool_call_part"),
        }
    }

    #[tokio::test]
    async fn redacted_thinking_event() {
        let events = vec![ParsedSseEvent {
            event_type: "content_block_start".into(),
            payload: serde_json::json!({
                "type":"content_block_start",
                "index":0,
                "content_block":{"type":"redacted_thinking","data":"secret"}
            }),
        }];
        let stream = anthropic_events_to_streamed_message(events).unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(stream).await;
        assert_eq!(
            parts[0],
            StreamedMessagePart::Content(ContentPart::Think {
                think: "".into(),
                encrypted: Some("secret".into())
            })
        );
    }

    #[test]
    fn message_delta_updates_usage_and_stop_reason() {
        let events = vec![
            ParsedSseEvent {
                event_type: "message_start".into(),
                payload: serde_json::json!({
                    "type":"message_start",
                    "message":{"id":"msg_1","usage":{"input_tokens":10,"output_tokens":0,"cache_read_input_tokens":2,"cache_creation_input_tokens":1}}
                }),
            },
            text_event("x"),
            ParsedSseEvent {
                event_type: "message_delta".into(),
                payload: serde_json::json!({
                    "type":"message_delta",
                    "delta":{"stop_reason":"max_tokens"},
                    "usage":{"output_tokens":5,"cache_read_input_tokens":3,"cache_creation_input_tokens":2,"input_tokens":11}
                }),
            },
        ];
        let stream = anthropic_events_to_streamed_message(events).unwrap();
        assert_eq!(stream.id(), Some("msg_1".into()));
        assert_eq!(
            stream.usage(),
            Some(TokenUsage {
                input_other: 11,
                output: 5,
                input_cache_read: 3,
                input_cache_creation: 2,
            })
        );
        assert_eq!(stream.finish_reason(), Some(FinishReason::Truncated));
        assert_eq!(stream.raw_finish_reason(), Some("max_tokens".into()));
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;

    #[test]
    fn normalize_anthropic_api_error_extracts_request_id() {
        let body = serde_json::json!({
            "error": { "type": "rate_limit_error", "message": "rate limited" },
            "request_id": "req_123"
        });
        let err = convert_anthropic_api_error(429, &body.to_string(), None);
        match err {
            ChatProviderError::Status(crate::errors::APIStatusError {
                status_code,
                message,
                request_id,
            }) => {
                assert_eq!(status_code, 429);
                assert!(message.contains("rate limited"));
                assert_eq!(request_id, Some("req_123".into()));
            }
            _ => panic!("expected status error"),
        }
    }
}
