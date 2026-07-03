# Part 1: Provider model & thinking configuration

本部分完成 Anthropic provider 的**与网络无关**的地基：依赖引入、Claude 版本解析、`max_tokens` ceiling、thinking 配置状态机，以及 provider 构造与 `ChatProvider` trait 基础实现。

---

### Task 1: 加入 reqwest 与 httptest 依赖

**Depends on:** none

**Files:**
- Modify: `rust-ody/crates/kosong-rs/Cargo.toml:8-20`

**实现步骤：**

- [ ] 编辑 `Cargo.toml`，在 `[dependencies]` 加入 `reqwest`，在 `[dev-dependencies]` 加入 `httptest`：

```toml
[dependencies]
anyhow = "1"
async-trait = "0.1"
futures-util = { version = "0.3", default-features = false, features = ["std"] }
regex = "1"
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = "1"
tokio = { workspace = true }

[dev-dependencies]
httptest = "0.16"
tokio-test = "0.4"
```

- [ ] 验证 Cargo 配置可解析：

```bash
cd rust-ody && cargo check -p kosong-rs
```

预期输出：编译成功（可能仅有 4.2.0 已存在代码的 warning，无 error）。

- [ ] Commit: `feat(kosong-rs): add reqwest and httptest deps for anthropic provider`

---

### Task 2: Claude 版本解析与 `resolve_default_max_tokens`

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/providers/anthropic.rs:1-130`
- Modify: `rust-ody/crates/kosong-rs/src/providers/mod.rs:1-3`
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs:11-13`

**实现步骤：**

- [ ] 先写测试（在 `anthropic.rs` 末尾 `#[cfg(test)]` 内），覆盖 TS `resolveDefaultMaxTokens` 的核心用例：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_known_claude_4_caps() {
        assert_eq!(resolve_default_max_tokens("claude-opus-4-7", None), 128_000);
        assert_eq!(resolve_default_max_tokens("claude-opus-4-6", None), 128_000);
        assert_eq!(resolve_default_max_tokens("claude-opus-4-5-20251101", None), 64_000);
        assert_eq!(resolve_default_max_tokens("claude-opus-4-1-20250805", None), 32_000);
        assert_eq!(resolve_default_max_tokens("claude-sonnet-4-6", None), 64_000);
        assert_eq!(resolve_default_max_tokens("claude-haiku-4-5-20251001", None), 64_000);
    }

    #[test]
    fn resolve_legacy_claude_3_and_3_5() {
        assert_eq!(resolve_default_max_tokens("claude-3-opus-20240229", None), 4_096);
        assert_eq!(resolve_default_max_tokens("claude-3-5-sonnet-20240620", None), 8_192);
        assert_eq!(resolve_default_max_tokens("claude-3.5-sonnet", None), 8_192);
        assert_eq!(resolve_default_max_tokens("claude-sonnet-3-7", None), 8_192);
    }

    #[test]
    fn resolve_vendor_prefixed_and_suffixed() {
        assert_eq!(resolve_default_max_tokens("anthropic.claude-opus-4-7-v1:0", None), 128_000);
        assert_eq!(resolve_default_max_tokens("aws/claude-opus-4-7", None), 128_000);
        assert_eq!(resolve_default_max_tokens("openrouter/claude-opus-4-7", None), 128_000);
        assert_eq!(resolve_default_max_tokens("claude-opus-4-6-construct", None), 128_000);
        assert_eq!(resolve_default_max_tokens("anthropic.claude-3-5-sonnet-20240620-v1:0", None), 8_192);
    }

    #[test]
    fn resolve_override_unknown_model() {
        assert_eq!(resolve_default_max_tokens("unknown-model", Some(12_345)), 12_345);
    }

    #[test]
    fn resolve_override_lower_than_ceiling() {
        assert_eq!(resolve_default_max_tokens("claude-opus-4-7", Some(200)), 200);
    }

    #[test]
    fn resolve_override_clamped_to_ceiling() {
        assert_eq!(resolve_default_max_tokens("claude-opus-4-7", Some(999_999)), 128_000);
    }

    #[test]
    fn resolve_fallback_for_non_claude_ids() {
        // "vendor-opus-4-7-preview" has no 'claude' marker → must not get Claude ceiling.
        assert_eq!(resolve_default_max_tokens("vendor-opus-4-7-preview", None), 32_000);
        assert_eq!(resolve_default_max_tokens("vendor-opus-4-7-preview", Some(8_000)), 8_000);
    }

    #[test]
    fn resolve_case_insensitive() {
        assert_eq!(resolve_default_max_tokens("CLAUDE-OPUS-4-7", None), 128_000);
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::tests
```

预期失败：`resolve_default_max_tokens` 未定义。

- [ ] 实现 `providers/anthropic.rs` 的版本解析与 ceiling 表：

```rust
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

fn parse_claude_version(model: &str) -> Option<ClaudeVersion> {
    let normalized = model.to_lowercase();
    // Guard: non-Claude ids must not accidentally match opus/sonnet/haiku substrings.
    if !normalized.contains("claude") {
        return None;
    }

    // Family-first: "opus-4-7", "sonnet-4.6", "haiku-4-5-20251001"
    let family_first = regex::Regex::new(r"(opus|sonnet|haiku)[-._](\d{1,2})(?!\d)(?:[-._](\d{1,2})(?!\d))?").unwrap();
    if let Some(caps) = family_first.captures(&normalized) {
        return Some(ClaudeVersion {
            family: caps[1].to_string(),
            major: caps[2].parse().unwrap(),
            minor: caps.get(3).map(|m| m.as_str().parse().unwrap()),
        });
    }

    // Version-first: "3-5-sonnet", "3.7.opus"
    let version_first = regex::Regex::new(r"(\d{1,2})[-._](\d{1,2})[-._](opus|sonnet|haiku)").unwrap();
    if let Some(caps) = version_first.captures(&normalized) {
        return Some(ClaudeVersion {
            family: caps[3].to_string(),
            major: caps[1].parse().unwrap(),
            minor: Some(caps[2].parse().unwrap()),
        });
    }

    // Bare family: "3-opus", "3.haiku"
    let bare = regex::Regex::new(r"(\d{1,2})[-._](opus|sonnet|haiku)").unwrap();
    if let Some(caps) = bare.captures(&normalized) {
        return Some(ClaudeVersion {
            family: caps[2].to_string(),
            major: caps[1].parse().unwrap(),
            minor: None,
        });
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
    table.get(&format!("{}-{}", version.family, version.major)).copied()
}

pub fn resolve_default_max_tokens(model: &str, override_: Option<i64>) -> i64 {
    let parsed = parse_claude_version(model);
    let ceiling = parsed.as_ref().and_then(lookup_claude_ceiling);
    match ceiling {
        None => override_.unwrap_or(FALLBACK_MAX_TOKENS),
        Some(c) => override_.map(|o| o.min(c)).unwrap_or(c),
    }
}
```

- [ ] 创建 `providers/mod.rs` 并导出：

```rust
pub mod anthropic;
```

- [ ] 在 `lib.rs` 加入 `pub mod providers;`。

- [ ] 重新运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::tests
```

预期：全部通过。

- [ ] Commit: `feat(kosong-rs): anthropic claude version parsing and max_tokens ceiling`

---

### Task 3: Adaptive / budget thinking配置

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/anthropic.rs:131-300`

**实现步骤：**

- [ ] 先写测试，覆盖 TS `withThinking` / `clampEffort` / `supportsAdaptiveThinking` 的核心矩阵：

```rust
#[cfg(test)]
mod thinking_tests {
    use super::*;

    #[test]
    fn budget_tokens_for_effort() {
        assert_eq!(budget_tokens_for_effort(ThinkingEffort::Low), 1_024);
        assert_eq!(budget_tokens_for_effort(ThinkingEffort::Medium), 4_096);
        assert_eq!(budget_tokens_for_effort(ThinkingEffort::High), 32_000);
    }

    #[test]
    fn supports_adaptive_for_4_6_and_later() {
        assert!(supports_adaptive_thinking("claude-opus-4-6"));
        assert!(supports_adaptive_thinking("claude-sonnet-4-6"));
        assert!(supports_adaptive_thinking("claude-opus-4-7"));
        assert!(supports_adaptive_thinking("claude-opus-4-8"));
        assert!(!supports_adaptive_thinking("claude-opus-4-5"));
        assert!(!supports_adaptive_thinking("claude-sonnet-4-5"));
        assert!(!supports_adaptive_thinking("claude-3-5-sonnet"));
        // unversioned custom model without explicit adaptive flag
        assert!(!supports_adaptive_thinking("custom-model"));
    }

    #[test]
    fn clamp_effort_matrix() {
        // Opus 4.7: full range
        assert_eq!(clamp_effort(ThinkingEffort::Xhigh, "claude-opus-4-7", false), ThinkingEffort::Xhigh);
        assert_eq!(clamp_effort(ThinkingEffort::Max, "claude-opus-4-7", false), ThinkingEffort::Max);
        // Pre-4.7 adaptive: xhigh -> high, max passes
        assert_eq!(clamp_effort(ThinkingEffort::Xhigh, "claude-opus-4-6", true), ThinkingEffort::High);
        assert_eq!(clamp_effort(ThinkingEffort::Max, "claude-opus-4-6", true), ThinkingEffort::Max);
        // Non-adaptive budget: max -> high
        assert_eq!(clamp_effort(ThinkingEffort::Max, "claude-sonnet-4-5", false), ThinkingEffort::High);
        assert_eq!(clamp_effort(ThinkingEffort::Off, "claude-opus-4-7", false), ThinkingEffort::Off);
    }

    #[test]
    fn thinking_config_disabled() {
        let cfg = build_thinking_config(ThinkingEffort::Off, "claude-opus-4-6", None);
        assert_eq!(cfg, AnthropicThinkingConfig::Disabled);
    }

    #[test]
    fn thinking_config_adaptive_opus_47() {
        let (thinking, output) = build_thinking_config(ThinkingEffort::High, "claude-opus-4-7", None);
        assert_eq!(thinking, AnthropicThinkingConfig::Adaptive { display: "summarized".into() });
        assert_eq!(output, Some(AnthropicOutputConfig { effort: "high".into() }));
    }

    #[test]
    fn thinking_config_budget_pre_46() {
        let (thinking, output) = build_thinking_config(ThinkingEffort::High, "claude-sonnet-4-20250514", None);
        assert_eq!(thinking, AnthropicThinkingConfig::Enabled { budget_tokens: 32_000 });
        assert_eq!(output, None);
    }

    #[test]
    fn thinking_config_opus_45_with_effort_param() {
        let (thinking, output) = build_thinking_config(ThinkingEffort::Xhigh, "claude-opus-4-5", None);
        assert_eq!(thinking, AnthropicThinkingConfig::Enabled { budget_tokens: 32_000 });
        assert_eq!(output, Some(AnthropicOutputConfig { effort: "high".into() }));
    }

    #[test]
    fn explicit_adaptive_override_for_unversioned_model() {
        let (thinking, output) = build_thinking_config(ThinkingEffort::Max, "custom-model", Some(true));
        assert_eq!(thinking, AnthropicThinkingConfig::Adaptive { display: "summarized".into() });
        assert_eq!(output, Some(AnthropicOutputConfig { effort: "max".into() }));
    }

    #[test]
    fn explicit_non_adaptive_override_for_46() {
        let (thinking, output) = build_thinking_config(ThinkingEffort::High, "claude-opus-4-6", Some(false));
        assert_eq!(thinking, AnthropicThinkingConfig::Enabled { budget_tokens: 32_000 });
        assert_eq!(output, None);
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::thinking_tests
```

预期失败：函数未定义。

- [ ] 实现 thinking 配置辅助函数与类型：

```rust
use crate::provider::ThinkingEffort;

const ADAPTIVE_MIN_VERSION: (u32, u32) = (4, 6);
const INTERLEAVED_THINKING_BETA: &str = "interleaved-thinking-2025-05-14";

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AnthropicThinkingConfig {
    Disabled,
    Adaptive { display: String },
    Enabled { budget_tokens: i64 },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AnthropicOutputConfig {
    pub effort: String,
}

fn parse_version_pair(model: &str) -> Option<(u32, u32)> {
    let normalized = model.to_lowercase();
    let re = regex::Regex::new(r"(\d{1,2})[-._](\d{1,2})(?!\d)").unwrap();
    re.captures(&normalized)
        .map(|caps| (caps[1].parse().unwrap(), caps[2].parse().unwrap()))
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
        let effort_str = format!("{:?}", effective).to_lowercase();
        return (
            AnthropicThinkingConfig::Adaptive {
                display: "summarized".into(),
            },
            Some(AnthropicOutputConfig { effort: effort_str }),
        );
    }

    let thinking = AnthropicThinkingConfig::Enabled {
        budget_tokens: budget_tokens_for_effort(effective),
    };
    let output = if supports_effort_param(model, adaptive) {
        Some(AnthropicOutputConfig {
            effort: format!("{:?}", effective).to_lowercase(),
        })
    } else {
        None
    };
    (thinking, output)
}
```

注意：`format!("{:?}", ThinkingEffort::Xhigh)` 输出 `"Xhigh"`，但我们需要 `"xhigh"`。由于 `ThinkingEffort` 已 derive `Debug` 且 `Xhigh` 的 serde rename 为 `"xhigh"`，不能依赖 Debug。应显式映射：

```rust
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
```

把 `build_thinking_config` 中的 `format!("{:?}", effective).to_lowercase()` 替换为 `effort_str(effective)`。

- [ ] 重新运行测试，全部通过。

- [ ] Commit: `feat(kosong-rs): anthropic thinking effort configuration`

---

### Task 4: `AnthropicChatProvider` 构造与 `ChatProvider` trait 基础实现

**Depends on:** Task 3

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/anthropic.rs:301-430`

**实现步骤：**

- [ ] 先写测试：

```rust
#[cfg(test)]
mod provider_tests {
    use super::*;
    use crate::provider::{ModelCapability, ThinkingEffort};

    fn test_provider(model: &str) -> AnthropicChatProvider {
        AnthropicChatProvider::new(AnthropicOptions {
            model: model.into(),
            api_key: Some("sk-test".into()),
            base_url: None,
            default_max_tokens: Some(1_024),
            beta_features: None,
            default_headers: None,
            metadata: None,
            stream: false,
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
        assert!(!std::ptr::eq(&p, &*q));
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
        assert_eq!(p.max_tokens(), 8_192);
    }

    #[test]
    fn with_max_completion_tokens_preserves_existing_lower_cap() {
        let p = AnthropicChatProvider::new(AnthropicOptions {
            model: "claude-opus-4-7".into(),
            api_key: Some("sk-test".into()),
            base_url: None,
            default_max_tokens: Some(1_024),
            beta_features: None,
            default_headers: None,
            metadata: None,
            stream: false,
            adaptive_thinking: None,
        })
        .with_max_completion_tokens(128_000);
        assert_eq!(p.max_tokens(), 1_024);
    }

    #[test]
    fn thinking_effort_null_when_not_configured() {
        assert_eq!(test_provider("claude-opus-4-7").thinking_effort(), None);
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::provider_tests
```

- [ ] 实现 provider 构造与 trait：

```rust
use crate::capability_registry::get_anthropic_model_capability;
use crate::errors::ChatProviderError;
use crate::generate::StreamedMessage;
use crate::provider::{ChatProvider, GenerateOptions, ModelCapability, ProviderRequestAuth, ThinkingEffort, Tool};
use crate::message::Message;

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
    client: reqwest::Client,
}

impl AnthropicChatProvider {
    pub fn new(options: AnthropicOptions) -> Self {
        let api_key = options.api_key.filter(|k| !k.is_empty());
        let max_tokens = resolve_default_max_tokens(&options.model, options.default_max_tokens);
        let beta_features = options.beta_features.unwrap_or_else(|| {
            vec![INTERLEAVED_THINKING_BETA.to_string()]
        });
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
        };
        if let Some(t) = patch.thinking {
            clone.generation_kwargs.thinking = Some(t);
            if t == AnthropicThinkingConfig::Disabled {
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
                let effort = self.generation_kwargs.output_config.as_ref()
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

    async fn generate(
        &self,
        _system_prompt: &str,
        _tools: &[Tool],
        _history: &[Message],
        _options: Option<GenerateOptions>,
    ) -> Result<StreamedMessage, ChatProviderError> {
        // Stub: Part 2/3 实现完整请求与解析
        Ok(StreamedMessage::from_parts(vec![], None, None, None, None))
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
        if matches!(patch.thinking, Some(AnthropicThinkingConfig::Adaptive { .. }))
            || matches!(patch.thinking, Some(AnthropicThinkingConfig::Disabled))
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

    fn with_max_completion_tokens(&self, max_completion_tokens: i64) -> Option<Box<dyn ChatProvider>> {
        let requested_cap = resolve_default_max_tokens(&self.model, Some(max_completion_tokens));
        let existing_cap = self.generation_kwargs.max_tokens;
        // If defaultMaxTokens was explicitly provided, preserve it; otherwise clamp.
        let new_cap = if existing_cap == resolve_default_max_tokens(&self.model, None) {
            requested_cap
        } else {
            existing_cap.min(requested_cap)
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
}
```

注意：`clone_with_generation_kwargs` 需要比较 "是否显式提供了 default_max_tokens"，目前结构体中没有这个标记。需要在 `AnthropicChatProvider` 增加 `explicit_max_tokens: bool` 字段。在 `new()` 中 `explicit_max_tokens = options.default_max_tokens.is_some()`；在 `with_max_completion_tokens` 中使用该字段判断。

修正后：

```rust
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
        // ...
        Self {
            // ...
            explicit_max_tokens: options.default_max_tokens.is_some(),
            // ...
        }
    }
}

fn with_max_completion_tokens(...) {
    let new_cap = if self.explicit_max_tokens {
        existing_cap
    } else {
        requested_cap
    };
    // ...
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::provider_tests
```

预期：全部通过。

- [ ] 运行 `cargo test -p kosong-rs` 确认不破坏既有模块。

- [ ] Commit: `feat(kosong-rs): anthropic provider constructor and chat provider trait shell`

---

## Part 1 Self-Review

- [ ] 1. Spec-coverage table: 本部分覆盖 4.2.4.1（provider 构造）、4.2.4.4（thinking 配置）。
- [ ] 2. Placeholder scan: 无 TODO/TBD；`generate()` 为显式 stub，将在 Part 2/3 替换。
- [ ] 3. No phantom tasks: 每个 Task 均有代码、命令、预期结果、commit。
- [ ] 4. Dependency soundness: Task 1 → 2 → 3 → 4，无向后依赖。
- [ ] 5. Caller & build soundness: 新增 `providers` 模块由 `lib.rs` 暴露；未修改既有共享签名。
- [ ] 6. Test-the-risk: 测试覆盖版本解析边界、ceiling 查找回退、thinking 配置状态转换、provider 克隆不变性。
- [ ] 7. Type consistency: `ThinkingEffort`、`ModelCapability`、`ChatProvider` 均复用 4.2.0/4.2.1 定义；`AnthropicOptions` 为本地新类型，不影响外部签名。
