# Part 1 — kosong-rs provider factory

本部分在 `kosong-rs` 内新增 `provider_factory` 模块，把 4.2.0–4.2.6 落地的所有 ChatProvider 统一到一个工厂入口，并暴露 capability 解析 helper。

---

### Task 1: 定义 `ProviderFactoryConfig` 与工厂错误类型

**Depends on:** none

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/provider_factory.rs`
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs:15` 附近新增 `pub mod provider_factory;`

实现步骤：

- [ ] 在 `provider_factory.rs` 顶部写入失败测试，断言 `ProviderFactoryError::UnknownProvider` 的 Display 包含 provider id：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_provider_error_includes_id() {
        let err = ProviderFactoryError::UnknownProvider("weird".into());
        assert!(err.to_string().contains("weird"));
    }
}
```

- [ ] 运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs provider_factory::tests::unknown_provider_error_includes_id --lib
```

预期失败：`error: cannot find module provider_factory` 或 `module not found`。

- [ ] 写入最小实现：

```rust
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderFactoryConfig {
    pub provider_id: String,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub default_headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProviderFactoryError {
    UnknownProvider(String),
    MissingModel,
    MissingApiKey { provider: String },
}

impl std::fmt::Display for ProviderFactoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderFactoryError::UnknownProvider(id) => write!(f, "unknown provider: {id}"),
            ProviderFactoryError::MissingModel => write!(f, "model is required"),
            ProviderFactoryError::MissingApiKey { provider } => {
                write!(f, "apiKey is required for provider: {provider}")
            }
        }
    }
}

impl std::error::Error for ProviderFactoryError {}

impl ProviderFactoryConfig {
    pub fn require_api_key(&self) -> Result<String, ProviderFactoryError> {
        match self.api_key.as_ref().filter(|k| !k.is_empty()) {
            Some(key) => Ok(key.clone()),
            None => Err(ProviderFactoryError::MissingApiKey {
                provider: self.provider_id.clone(),
            }),
        }
    }
}
```

- [ ] 在 `lib.rs` 新增 `pub mod provider_factory;`。

- [ ] 运行测试并确认通过：

```bash
cd rust-ody && cargo test -p kosong-rs provider_factory::tests::unknown_provider_error_includes_id --lib
```

预期：1 passed。

- [ ] Commit：`feat(kosong-rs): add ProviderFactoryConfig and factory error types`。

---

### Task 2: 实现 `create_chat_provider` 全 provider 分支

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/provider_factory.rs`

实现步骤：

- [ ] 在 `provider_factory.rs` 底部新增失败测试，断言工厂能构造 mock 与 openai provider 并读取其 `name()`/`model_name()`：

```rust
#[cfg(test)]
mod create_tests {
    use super::*;

    #[tokio::test]
    async fn factory_creates_mock_provider() {
        let provider = create_chat_provider(ProviderFactoryConfig {
            provider_id: "mock".into(),
            model: "m1".into(),
            api_key: None,
            base_url: None,
            default_headers: None,
        })
        .unwrap();
        assert_eq!(provider.name(), "mock");
        assert_eq!(provider.model_name(), "m1");
    }

    #[tokio::test]
    async fn factory_creates_openai_provider() {
        let provider = create_chat_provider(ProviderFactoryConfig {
            provider_id: "openai".into(),
            model: "gpt-4o-mini".into(),
            api_key: Some("sk-test".into()),
            base_url: Some("https://example.com/v1".into()),
            default_headers: None,
        })
        .unwrap();
        assert_eq!(provider.name(), "openai");
        assert_eq!(provider.model_name(), "gpt-4o-mini");
    }

    #[tokio::test]
    async fn factory_rejects_unknown_provider() {
        let err = create_chat_provider(ProviderFactoryConfig {
            provider_id: "weird".into(),
            model: "x".into(),
            api_key: None,
            base_url: None,
            default_headers: None,
        })
        .unwrap_err();
        assert!(matches!(err, ProviderFactoryError::UnknownProvider(id) if id == "weird"));
    }
}
```

- [ ] 运行测试并确认失败（函数未定义）：

```bash
cd rust-ody && cargo test -p kosong-rs provider_factory::create_tests --lib
```

- [ ] 写入 `create_chat_provider` 实现（放在 `ProviderFactoryConfig` 同一文件，不重复导入）：

```rust
use crate::{
    AnthropicChatProvider, AnthropicOptions, DeepSeekChatProvider, DeepSeekOptions,
    GLMChatProvider, GLMOptions, GoogleGenAIChatProvider, KimiChatProvider, KimiOptions,
    MockProvider, OpenAILegacyChatProvider, OpenAILegacyOptions, OpenAIResponsesChatProvider,
    OpenAIResponsesOptions,
};
use crate::provider::ChatProvider;

pub fn create_chat_provider(
    config: ProviderFactoryConfig,
) -> Result<Box<dyn ChatProvider>, ProviderFactoryError> {
    if config.model.is_empty() {
        return Err(ProviderFactoryError::MissingModel);
    }

    match config.provider_id.as_str() {
        "mock" => Ok(Box::new(MockProvider::new("mock", config.model))),
        "openai" => Ok(Box::new(OpenAILegacyChatProvider::new(OpenAILegacyOptions {
            api_key: config.api_key,
            base_url: config.base_url,
            model: config.model,
            stream: Some(true),
            max_tokens: None,
            reasoning_key: None,
            default_headers: config.default_headers,
            tool_message_conversion: None,
            http_client: None,
        }))),
        "openai_responses" => Ok(Box::new(OpenAIResponsesChatProvider::new(
            OpenAIResponsesOptions {
                api_key: config.api_key,
                base_url: config.base_url,
                model: config.model,
                max_output_tokens: None,
                default_headers: config.default_headers,
                tool_message_conversion: None,
                http_client: None,
            },
        ))),
        "kimi" => Ok(Box::new(KimiChatProvider::new(KimiOptions {
            api_key: config.api_key,
            base_url: config.base_url,
            model: config.model,
            stream: Some(true),
            default_headers: config.default_headers,
            generation_kwargs: None,
            http_client: None,
            reasoning_key: None,
        }))),
        "anthropic" => Ok(Box::new(AnthropicChatProvider::new(AnthropicOptions {
            api_key: config.api_key,
            base_url: config.base_url,
            model: config.model,
            default_max_tokens: None,
            beta_features: None,
            default_headers: config.default_headers,
            metadata: None,
            stream: Some(true),
            adaptive_thinking: None,
        }))),
        "deepseek" => Ok(Box::new(DeepSeekChatProvider::new(DeepSeekOptions {
            api_key: config.api_key,
            base_url: config.base_url,
            model: config.model,
            stream: Some(true),
            max_tokens: None,
            reasoning_key: None,
            http_client: None,
            default_headers: config.default_headers,
            tool_message_conversion: None,
        }))),
        "glm" => Ok(Box::new(GLMChatProvider::new(GLMOptions {
            api_key: config.api_key,
            base_url: config.base_url,
            model: config.model,
            stream: Some(true),
            max_tokens: None,
            default_headers: config.default_headers,
            http_client: None,
        }))),
        "google-genai" => {
            let mut provider = GoogleGenAIChatProvider::new(config.model)
                .with_stream(true);
            if let Some(key) = config.api_key {
                provider = provider.with_api_key(key);
            }
            if let Some(url) = config.base_url {
                provider = provider.with_base_url(url);
            }
            Ok(Box::new(provider))
        }
        "vertexai" => {
            let mut provider = GoogleGenAIChatProvider::new(config.model)
                .with_stream(true)
                .with_vertexai("", "");
            if let Some(key) = config.api_key {
                provider = provider.with_api_key(key);
            }
            if let Some(url) = config.base_url {
                provider = provider.with_base_url(url);
            }
            Ok(Box::new(provider))
        }
        other => Err(ProviderFactoryError::UnknownProvider(other.into())),
    }
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs provider_factory::create_tests --lib
```

预期：3 passed。

- [ ] Commit：`feat(kosong-rs): implement create_chat_provider for all providers`。

---

### Task 3: 实现 `resolve_model_capability` 与工厂单元测试

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/provider_factory.rs`
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs:44` 附近按需调整 re-export

实现步骤：

- [ ] 在 `provider_factory.rs` 新增失败测试，断言已知模型 capability 与 unknown 模型均返回对象：

```rust
#[cfg(test)]
mod capability_tests {
    use super::*;
    use crate::provider::ModelCapability;

    #[test]
    fn resolve_openai_known_model() {
        let cap = resolve_model_capability("openai", "gpt-4o").unwrap();
        assert!(cap.image_in);
        assert!(cap.tool_use);
        assert_eq!(cap.max_context_tokens, 128_000);
    }

    #[test]
    fn resolve_unknown_model_returns_unknown() {
        let cap = resolve_model_capability("openai", "not-a-real-model").unwrap();
        assert!(cap.is_unknown());
    }

    #[test]
    fn resolve_unsupported_provider_returns_unknown() {
        let cap = resolve_model_capability("weird", "m1").unwrap();
        assert!(cap.is_unknown());
    }
}
```

- [ ] 运行测试并确认失败（`resolve_model_capability` 未定义）：

```bash
cd rust-ody && cargo test -p kosong-rs provider_factory::capability_tests --lib
```

- [ ] 写入实现：

```rust
use crate::capability_registry::{
    get_anthropic_model_capability, get_deepseek_model_capability, get_glm_model_capability,
    get_google_genai_model_capability, get_kimi_model_capability, get_openai_legacy_model_capability,
    get_openai_responses_model_capability,
};
use crate::provider::ModelCapability;

pub fn resolve_model_capability(provider_id: &str, model: &str) -> Option<ModelCapability> {
    match provider_id {
        "openai" => get_openai_legacy_model_capability(model),
        "openai_responses" => get_openai_responses_model_capability(model),
        "kimi" => get_kimi_model_capability(model),
        "anthropic" => get_anthropic_model_capability(model),
        "deepseek" => get_deepseek_model_capability(model),
        "glm" => get_glm_model_capability(model),
        "google-genai" | "vertexai" => get_google_genai_model_capability(model),
        _ => None,
    }
}
```

- [ ] 测试运行：

```bash
cd rust-ody && cargo test -p kosong-rs provider_factory::capability_tests --lib
```

预期：3 passed。若 `gpt-4o` 不在 capability registry 中导致断言失败，则把测试模型改为 `gpt-4o-mini` 或 registry 中已存在的条目（先 `grep -n "gpt-4" rust-ody/crates/kosong-rs/src/capability_registry.rs` 确认）。

- [ ] 在 `lib.rs` 新增 re-export：`pub use provider_factory::{create_chat_provider, resolve_model_capability, ProviderFactoryConfig, ProviderFactoryError};`。

- [ ] 全 `kosong-rs` 编译检查：

```bash
cd rust-ody && cargo check -p kosong-rs
```

预期：无错误。

- [ ] Commit：`feat(kosong-rs): add resolve_model_capability helper and re-exports`。

---

## Part 1 Self-Review

- [ ] 1. Spec-coverage: 4.2.7.1（kosong-rs provider factory）已覆盖。
- [ ] 2. Placeholder scan: 本部分无 TODO/TBD；每个分支均给出完整构造代码。
- [ ] 3. No phantom tasks: 每个 task 均产生可编译/可测试的代码变更。
- [ ] 4. Dependency soundness: Task 2 依赖 Task 1 的类型；Task 3 依赖 Task 2 的 `create_chat_provider` 未使用但依赖已定义符号（`resolve_model_capability` 只使用 capability_registry，无向后依赖）。
- [ ] 5. Caller & build soundness: Task 3 修改 `lib.rs` re-export，并在同 task 内跑 `cargo check -p kosong-rs`。
- [ ] 6. Test-the-risk: 工厂分支选择是状态风险，测试覆盖 mock/openai/unknown；capability 解析测试覆盖 known/unknown/unsupported provider。
- [ ] 7. Type consistency: `ProviderFactoryConfig` 字段名与后续 Part 2 使用的 `provider_id`/`model` 一致；`resolve_model_capability` 返回 `Option<ModelCapability>` 与 `ody-host` `get_agent_config` 使用的 `UNKNOWN_CAPABILITY` 回退语义一致。
