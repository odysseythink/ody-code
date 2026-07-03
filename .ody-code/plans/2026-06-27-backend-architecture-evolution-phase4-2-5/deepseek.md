# Part 3: DeepSeek provider

本部分实现 `DeepSeekChatProvider`。它是 `OpenAILegacyChatProvider` 的薄封装：复用已有的 OpenAI-Compatible 请求/解析逻辑，只替换 capability 查询、默认 base URL 与 API key 解析（显式传空字符串防止回退到 `OPENAI_API_KEY`）。

---

## File Structure

```
rust-ody/crates/kosong-rs/
  src/
    providers/
      mod.rs                       # 导出 deepseek 模块
      deepseek.rs                  # DeepSeekChatProvider + 测试
    lib.rs                         # 导出 DeepSeekChatProvider / DeepSeekOptions
```

---

## Dependency Overview (Part 3 内部)

```
Task 3.1 DeepSeekChatProvider 构造、capability 与 with_* 委托
  │
  └─ Task 3.2 generate() 委托端到端与空 API key 行为
```

- Task 3.1 是 Task 3.2 的前置（先生成可工作的 provider 壳，再验证完整 generate）。
- Part 3 仅依赖 Part 1 Task 1（`get_deepseek_model_capability`）和已有的 `OpenAILegacyChatProvider`。

---

## Risks

| 风险 | 缓解 |
|---|---|
| DeepSeek 空 api_key 意外回退到 `OPENAI_API_KEY` | 构造时显式传入 `Some("")`，让 delegate 的 `.filter(|k| !k.is_empty())` 直接失败，不触发 env fallback；Task 3.2 有断言 |
| `with_thinking` / `with_max_completion_tokens` 返回 trait object 后丢失 DeepSeek 包装 | 在 `with_*` 中重新包回 `DeepSeekChatProvider` 再 box |

---

### Task 3.1: DeepSeekChatProvider 构造、capability 与 with_* 委托

**Depends on:** `shared.md` Task 1（`get_deepseek_model_capability`）

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/providers/deepseek.rs:1-120`
- Modify: `rust-ody/crates/kosong-rs/src/providers/mod.rs:1-4`（追加 `pub mod deepseek;`）
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs:1-45`（追加 re-export）

**实现步骤：**

- [ ] 先写测试，覆盖构造、name/model、capability、with_thinking/with_max_completion_tokens：

```rust
#[cfg(test)]
mod provider_shell_tests {
    use super::*;
    use crate::provider::ThinkingEffort;

    fn provider(model: &str) -> DeepSeekChatProvider {
        DeepSeekChatProvider::new(DeepSeekOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: model.into(),
            stream: None,
            max_tokens: None,
            reasoning_key: None,
            http_client: None,
            default_headers: None,
            tool_message_conversion: None,
        })
    }

    #[test]
    fn name_is_deepseek() {
        assert_eq!(provider("deepseek-chat").name(), "deepseek");
    }

    #[test]
    fn model_name_matches_constructor() {
        assert_eq!(provider("deepseek-chat").model_name(), "deepseek-chat");
    }

    #[test]
    fn reasoner_thinks_but_no_tools() {
        let cap = provider("deepseek-reasoner").get_capability(None);
        assert!(cap.thinking);
        assert!(!cap.tool_use);
    }

    #[test]
    fn chat_uses_tools_but_no_thinking() {
        let cap = provider("deepseek-chat").get_capability(None);
        assert!(!cap.thinking);
        assert!(cap.tool_use);
    }

    #[test]
    fn v4_thinks_and_uses_tools_with_large_context() {
        let cap = provider("deepseek-v4-0320").get_capability(None);
        assert!(cap.thinking);
        assert!(cap.tool_use);
        assert_eq!(cap.max_context_tokens, 1_000_000);
        assert_eq!(cap.max_output_tokens, 384_000);
    }

    #[test]
    fn with_thinking_sets_reasoning_effort() {
        let boxed = provider("deepseek-chat").with_thinking(ThinkingEffort::High);
        assert_eq!(boxed.thinking_effort(), Some(ThinkingEffort::High));
    }

    #[test]
    fn with_max_completion_tokens_returns_provider() {
        let boxed = provider("deepseek-chat").with_max_completion_tokens(512);
        assert!(boxed.is_some());
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs providers::deepseek::provider_shell_tests
```

预期失败：`DeepSeekChatProvider` / `DeepSeekOptions` 未定义。

- [ ] 实现 `DeepSeekChatProvider` 构造与 trait 壳：

```rust
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::capability_registry;
use crate::generate::StreamedMessage;
use crate::http_client::HttpClient;
use crate::message::Message;
use crate::openai_common::ToolMessageConversion;
use crate::openai_legacy::{OpenAILegacyChatProvider, OpenAILegacyOptions};
use crate::provider::{ChatProvider, GenerateOptions, ModelCapability, ThinkingEffort, Tool};
use crate::ChatProviderError;

#[derive(Clone)]
pub struct DeepSeekOptions {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: String,
    pub stream: Option<bool>,
    pub max_tokens: Option<i64>,
    pub reasoning_key: Option<String>,
    pub http_client: Option<Arc<dyn HttpClient>>,
    pub default_headers: Option<HashMap<String, String>>,
    pub tool_message_conversion: Option<ToolMessageConversion>,
}

pub struct DeepSeekChatProvider {
    delegate: Arc<dyn ChatProvider>,
}

impl std::fmt::Debug for DeepSeekChatProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeepSeekChatProvider")
            .field("name", &self.name())
            .field("model", &self.model_name())
            .finish_non_exhaustive()
    }
}

impl Clone for DeepSeekChatProvider {
    fn clone(&self) -> Self {
        Self {
            delegate: Arc::clone(&self.delegate),
        }
    }
}

impl DeepSeekChatProvider {
    pub fn new(options: DeepSeekOptions) -> Self {
        let api_key = options
            .api_key
            .or_else(|| std::env::var("DEEPSEEK_API_KEY").ok())
            .unwrap_or_default();
        // Explicitly pass an empty string when no key is available so the
        // underlying OpenAILegacyChatProvider does not fall back to OPENAI_API_KEY.
        let resolved_api_key = if api_key.is_empty() { "".into() } else { api_key };
        let base_url = options
            .base_url
            .unwrap_or_else(|| "https://api.deepseek.com/v1".into());

        let delegate = OpenAILegacyChatProvider::new(OpenAILegacyOptions {
            api_key: Some(resolved_api_key),
            base_url: Some(base_url),
            model: options.model,
            stream: options.stream,
            max_tokens: options.max_tokens,
            reasoning_key: options.reasoning_key,
            default_headers: options.default_headers,
            tool_message_conversion: options.tool_message_conversion,
            http_client: options.http_client,
        });

        Self {
            delegate: Arc::new(delegate),
        }
    }
}

#[async_trait]
impl ChatProvider for DeepSeekChatProvider {
    fn name(&self) -> &str { "deepseek" }
    fn model_name(&self) -> &str { self.delegate.model_name() }
    fn thinking_effort(&self) -> Option<ThinkingEffort> { self.delegate.thinking_effort() }
    fn get_capability(&self, model: Option<&str>) -> ModelCapability {
        capability_registry::get_deepseek_model_capability(model.unwrap_or(self.model_name()))
    }

    async fn generate(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        history: &[Message],
        options: Option<GenerateOptions>,
    ) -> Result<StreamedMessage, ChatProviderError> {
        self.delegate.generate(system_prompt, tools, history, options).await
    }

    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        let new_delegate = self.delegate.with_thinking(effort);
        Box::new(Self {
            delegate: Arc::new(new_delegate),
        })
    }

    fn with_max_completion_tokens(&self, max_completion_tokens: i64) -> Option<Box<dyn ChatProvider>> {
        let new_delegate = self.delegate.with_max_completion_tokens(max_completion_tokens)?;
        Some(Box::new(Self {
            delegate: Arc::new(new_delegate),
        }))
    }
}
```

- [ ] 修改 `providers/mod.rs`：

```rust
pub mod anthropic;
pub mod deepseek;
pub mod google_genai;
pub mod kimi;
```

- [ ] 修改 `lib.rs`：

```rust
pub use providers::deepseek::{DeepSeekChatProvider, DeepSeekOptions};
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs providers::deepseek::provider_shell_tests
```

预期：全部通过。

- [ ] Commit: `feat(kosong-rs): add DeepSeekChatProvider shell and capability`

---

### Task 3.2: generate() 委托端到端与空 API key 行为

**Depends on:** Task 3.1

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/deepseek.rs:120-260`（追加测试）
- Test: `rust-ody/crates/kosong-rs/src/providers/deepseek.rs` 内 `#[cfg(test)]` 模块

**实现步骤：**

- [ ] 先写测试，覆盖流式 generate 委托与空 API key 不回退：

```rust
#[cfg(test)]
mod generate_tests {
    use super::*;
    use crate::http_client::MockHttpClient;
    use crate::message::StreamedMessagePart;
    use crate::provider::{GenerateOptions, ProviderRequestAuth};
    use futures_util::StreamExt;
    use std::sync::Arc;

    #[tokio::test]
    async fn generate_delegates_to_openai_legacy_stream() {
        let sse = b"data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\"}}]}\n\ndata: [DONE]\n\n".to_vec();
        let http_client: Arc<dyn HttpClient> = Arc::new(MockHttpClient::new(200, sse));
        let provider = DeepSeekChatProvider::new(DeepSeekOptions {
            api_key: Some("sk".into()),
            base_url: Some("http://mock".into()),
            model: "deepseek-chat".into(),
            stream: Some(true),
            max_tokens: None,
            reasoning_key: None,
            http_client: Some(http_client),
            default_headers: None,
            tool_message_conversion: None,
        });
        let stream = provider.generate("", &[], &[], None).await.unwrap();
        let parts: Vec<_> = stream.collect().await;
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], StreamedMessagePart::text("hello"));
    }

    #[tokio::test]
    async fn empty_api_key_does_not_fallback_to_openai_key() {
        // Even if OPENAI_API_KEY is set in the environment, an explicit empty
        // api_key should make the delegate fail with MissingApiKey.
        let http_client: Arc<dyn HttpClient> = Arc::new(MockHttpClient::new(200, vec![]));
        let provider = DeepSeekChatProvider::new(DeepSeekOptions {
            api_key: Some("".into()),
            base_url: Some("http://mock".into()),
            model: "deepseek-chat".into(),
            stream: Some(true),
            max_tokens: None,
            reasoning_key: None,
            http_client: Some(http_client),
            default_headers: None,
            tool_message_conversion: None,
        });
        let auth = ProviderRequestAuth {
            api_key: Some("".into()),
            headers: None,
        };
        let options = GenerateOptions {
            auth: Some(auth),
            ..Default::default()
        };
        let err = provider
            .generate("", &[], &[], Some(options))
            .await
            .unwrap_err();
        assert!(matches!(err, ChatProviderError::MissingApiKey(_)));
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs providers::deepseek::generate_tests
```

预期失败：`generate_tests` 模块未定义，或两个断言在实现正确前无法通过。实际上 Task 3.1 的 `generate` 已经是委托实现，所以这些测试应当直接通过；失败只可能来自模块尚未写入。写入后应通过。

- [ ] 将实现代码保持为 Task 3.1 的委托版本，无需额外改动（generate 与 with_* 已在 Task 3.1 完成）。

- [ ] 运行整个 crate 测试确认没有破坏：

```bash
cd rust-ody && cargo test -p kosong-rs
```

预期：全部通过（新增 DeepSeek 测试 + 既有测试）。

- [ ] Commit: `feat(kosong-rs): verify DeepSeek generate delegation and api-key isolation`

---

## Part 3 Self-Review

- [ ] 1. Spec-coverage table:
  | 路线图/TS 行为 | Task | 状态 |
  |---|---|---|
  | DeepSeekChatProvider 构造（api_key / base_url / model / stream / max_tokens） | 3.1 | covered |
  | name / model_name | 3.1 | covered |
  | getCapability（reasoner / chat / v4） | 3.1 | covered |
  | `with_thinking` 委托给 OpenAILegacyChatProvider | 3.1 | covered |
  | `with_max_completion_tokens` 委托 | 3.1 | covered |
  | `generate` 委托给 OpenAILegacyChatProvider | 3.2 | covered |
  | 空 api_key 不回退到 `OPENAI_API_KEY` | 3.2 | covered |
- [ ] 2. Placeholder scan: 无 `TODO`/`TBD`；所有代码均为可直接运行的完整实现。
- [ ] 3. No phantom tasks: 两个 Task 均有代码、测试、命令、commit。
- [ ] 4. Dependency soundness: Task 3.1 依赖 Part 1 Task 1；Task 3.2 依赖 Task 3.1。未引用后序 Task 的符号。
- [ ] 5. Caller & build soundness: 本 Part 未修改共享签名；以 `cargo test -p kosong-rs` 验证整个 crate。
- [ ] 6. Test-the-risk: capability 边界、API key 不回退、generate 委托均通过行为断言验证；常量与实现一致。
- [ ] 7. Type一致性: `DeepSeekOptions` 字段名与 `OpenAILegacyOptions` 对齐；`DeepSeekChatProvider` 实现 `ChatProvider` trait，返回类型与 trait 一致。

