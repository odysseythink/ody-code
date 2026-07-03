# 4.2.1 Part 1 — Rust `kosong-rs` utility 模块

**Scope:** 在 `rust-ody/crates/kosong-rs` 中实现 4.2.1 所需的四个纯函数模块：`tool_call_id`、`request_auth`、`capability_registry`、`catalog`；并补充 `ProviderType` 枚举。每个模块都以单元测试先行，最后以 `cargo test -p kosong-rs` 绿为门。

---

### Task 1: 新增 `ProviderType` enum

**Depends on:** 4.2.0 完成（`kosong-rs` 已存在 `Message`/`ModelCapability`/`ProviderRequestAuth`）

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/provider.rs:11`（在 `ThinkingEffort` 前插入 `ProviderType` enum）
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs:13`（新增 re-export）
- Test: `rust-ody/crates/kosong-rs/src/provider.rs:185`（追加到 `mod tests`）

**步骤：**

- [ ] 写失败测试：
  ```rust
  #[test]
  fn provider_type_serializes_to_kebab_case() {
      let v = serde_json::to_value(ProviderType::GoogleGenai).unwrap();
      assert_eq!(v, "google-genai");
      let round: ProviderType = serde_json::from_value(v).unwrap();
      assert_eq!(round, ProviderType::GoogleGenai);
  }
  ```

- [ ] 运行并确认失败：
  ```bash
  cargo test -p kosong-rs provider_type_serializes_to_kebab_case
  ```
  期望失败：`cannot find type ProviderType in this scope`。

- [ ] 实现 `ProviderType`：
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum ProviderType {
      Anthropic,
      #[serde(rename = "openai")]
      OpenAi,
      Kimi,
      #[serde(rename = "google-genai")]
      GoogleGenai,
      #[serde(rename = "openai_responses")]
      OpenAiResponses,
      Vertexai,
      Deepseek,
      Glm,
  }
  ```
  注意：TS 字符串为 `"openai"`、`"google-genai"`、`"openai_responses"`，Rust enum variant 用 `OpenAi`/`GoogleGenai`/`OpenAiResponses` 并通过 `#[serde(rename)]` 保持线格式一致。

- [ ] 在 `lib.rs` 追加 re-export：
  ```rust
  pub use provider::ProviderType;
  ```

- [ ] 运行并确认通过：
  ```bash
  cargo test -p kosong-rs provider_type_serializes_to_kebab_case
  cargo check -p kosong-rs
  ```

- [ ] Commit：`feat(kosong-rs): add ProviderType enum for catalog wiring`

---

### Task 2: 实现 `tool-call-id` 模块

**Depends on:** Task 1（不需要 `ProviderType`，但需要 `Message`/`ToolCall` 已存在）

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/tool_call_id.rs`
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs:14`（新增 `pub mod tool_call_id;` 与 re-export）

**步骤：**

- [ ] 写失败测试（先占位于 `tool_call_id.rs` 底部 `#[cfg(test)]`）：
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use crate::message::{ContentPart, Message, Role, ToolCall};

      #[test]
      fn sanitize_replaces_unsafe_chars() {
          assert_eq!(sanitize_tool_call_id("a|b/c", None), "a_b_c");
      }

      #[test]
      fn sanitize_truncates_to_max_length() {
          assert_eq!(sanitize_tool_call_id("abcdefghij", Some(5)), "abcde");
      }

      #[test]
      fn openai_responses_splits_pipe() {
          assert_eq!(sanitize_openai_responses_call_id("id|extra", Some(4)), "id_e");
      }

      #[test]
      fn normalize_renames_conflicting_ids() {
          let messages = vec![
              Message {
                  role: Role::Assistant,
                  name: None,
                  content: vec![],
                  tool_calls: vec![ToolCall {
                      call_type: "function".into(),
                      id: "a|b".into(),
                      name: "read".into(),
                      arguments: None,
                      extras: None,
                      stream_index: None,
                  }],
                  tool_call_id: None,
                  partial: None,
              },
              Message {
                  role: Role::Tool,
                  name: None,
                  content: vec![ContentPart::Text { text: "ok".into() }],
                  tool_calls: vec![],
                  tool_call_id: Some("a|b".into()),
                  partial: None,
              },
          ];
          let policy = ToolCallIdPolicy::new(|id| sanitize_tool_call_id(id, Some(3)), Some(3));
          let normalized = normalize_tool_call_ids_for_provider(&messages, &policy);
          assert_eq!(normalized[0].tool_calls[0].id, "a_b");
          assert_eq!(normalized[1].tool_call_id, Some("a_b".into()));
      }

      #[test]
      fn normalize_suffixes_duplicates() {
          let messages = vec![
              make_tool_call_msg("a|b"),
              make_tool_call_msg("a/b"),
          ];
          let policy = ToolCallIdPolicy::new(|id| sanitize_tool_call_id(id, Some(3)), Some(3));
          let normalized = normalize_tool_call_ids_for_provider(&messages, &policy);
          assert_eq!(normalized[0].tool_calls[0].id, "a_b");
          assert_eq!(normalized[1].tool_calls[0].id, "a_2");
      }

      fn make_tool_call_msg(id: &str) -> Message {
          Message {
              role: Role::Assistant,
              name: None,
              content: vec![],
              tool_calls: vec![ToolCall {
                  call_type: "function".into(),
                  id: id.into(),
                  name: "read".into(),
                  arguments: None,
                  extras: None,
                  stream_index: None,
              }],
              tool_call_id: None,
              partial: None,
          }
      }
  }
  ```

- [ ] 运行并确认失败：
  ```bash
  cargo test -p kosong-rs tool_call_id
  ```
  期望失败：`module tool_call_id not found` 或函数未找到。

- [ ] 实现 `src/tool_call_id.rs`：
  ```rust
  use crate::message::{Message, ToolCall};

  pub struct ToolCallIdPolicy {
      pub normalize: Box<dyn Fn(&str) -> String>,
      pub max_length: Option<usize>,
  }

  impl ToolCallIdPolicy {
      pub fn new(normalize: impl Fn(&str) -> String + 'static, max_length: Option<usize>) -> Self {
          Self {
              normalize: Box::new(normalize),
              max_length,
          }
      }
  }

  const EMPTY_TOOL_CALL_ID: &str = "tool_call";
  const TOOL_CALL_ID_SAFE_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-";

  pub fn sanitize_tool_call_id(id: &str, max_length: Option<usize>) -> String {
      let sanitized: String = id
          .chars()
          .map(|c| if TOOL_CALL_ID_SAFE_CHARS.contains(c) { c } else { '_' })
          .collect();
      match max_length {
          Some(n) => sanitized.chars().take(n).collect(),
          None => sanitized,
      }
  }

  pub fn sanitize_openai_responses_call_id(id: &str, max_length: Option<usize>) -> String {
      let call_id = id.split('|').next().unwrap_or(id);
      sanitize_tool_call_id(call_id, max_length)
  }

  pub fn normalize_tool_call_ids_for_provider(
      messages: &[Message],
      policy: &ToolCallIdPolicy,
  ) -> Vec<Message> {
      let raw_ids = collect_tool_call_ids(messages);
      if raw_ids.is_empty() {
          return messages.to_vec();
      }

      let mapped_ids = build_tool_call_id_map(&raw_ids, policy);
      let mut changed = false;
      let normalized: Vec<Message> = messages
          .iter()
          .map(|message| {
              let mut message_changed = false;
              let tool_calls: Vec<ToolCall> = message
                  .tool_calls
                  .iter()
                  .map(|tc| {
                      if let Some(mapped) = mapped_ids.get(&tc.id) {
                          if mapped != &tc.id {
                              message_changed = true;
                              return ToolCall { id: mapped.clone(), ..tc.clone() };
                          }
                      }
                      tc.clone()
                  })
                  .collect();

              let mapped_tool_call_id = message
                  .tool_call_id
                  .as_ref()
                  .and_then(|id| mapped_ids.get(id))
                  .cloned()
                  .or_else(|| message.tool_call_id.clone());
              if mapped_tool_call_id != message.tool_call_id {
                  message_changed = true;
              }

              if !message_changed {
                  return message.clone();
              }
              changed = true;
              Message {
                  tool_calls,
                  tool_call_id: mapped_tool_call_id,
                  ..message.clone()
              }
          })
          .collect();

      if changed { normalized } else { messages.to_vec() }
  }

  fn collect_tool_call_ids(messages: &[Message]) -> Vec<String> {
      let mut ids = Vec::new();
      let mut seen = std::collections::HashSet::new();
      for message in messages {
          for tc in &message.tool_calls {
              if seen.insert(tc.id.clone()) {
                  ids.push(tc.id.clone());
              }
          }
          if let Some(id) = &message.tool_call_id {
              if seen.insert(id.clone()) {
                  ids.push(id.clone());
              }
          }
      }
      ids
  }

  fn build_tool_call_id_map(
      raw_ids: &[String],
      policy: &ToolCallIdPolicy,
  ) -> std::collections::HashMap<String, String> {
      let mut mapped_ids = std::collections::HashMap::new();
      let mut used_ids = std::collections::HashSet::new();

      for raw_id in raw_ids {
          let normalized = (policy.normalize)(raw_id);
          if normalized == *raw_id && !normalized.is_empty() {
              mapped_ids.insert(raw_id.clone(), normalized.clone());
              used_ids.insert(normalized);
          }
      }

      for raw_id in raw_ids {
          if mapped_ids.contains_key(raw_id) {
              continue;
          }
          let normalized = (policy.normalize)(raw_id);
          let unique = make_unique_tool_call_id(normalized, &used_ids, policy.max_length);
          mapped_ids.insert(raw_id.clone(), unique.clone());
          used_ids.insert(unique);
      }

      mapped_ids
  }

  fn make_unique_tool_call_id(
      normalized: String,
      used_ids: &std::collections::HashSet<String>,
      max_length: Option<usize>,
  ) -> String {
      let base = if normalized.is_empty() { EMPTY_TOOL_CALL_ID } else { &normalized };
      let candidate = truncate_tool_call_id(base, max_length, "");
      if !used_ids.contains(&candidate) {
          return candidate;
      }
      for i in 2.. {
          let suffix = format!("_{}", i);
          let suffixed = truncate_tool_call_id(base, max_length, &suffix);
          if !used_ids.contains(&suffixed) {
              return suffixed;
          }
      }
      unreachable!()
  }

  fn truncate_tool_call_id(base: &str, max_length: Option<usize>, suffix: &str) -> String {
      match max_length {
          None => format!("{}{}", base, suffix),
          Some(n) => {
              let base_len = n.saturating_sub(suffix.len());
              if base_len == 0 {
                  panic!("Tool call id maxLength {} is too small for suffix {}.", n, suffix);
              }
              format!("{}{}", &base[..base_len], suffix)
          }
      }
  }
  ```

- [ ] 在 `lib.rs` 添加：
  ```rust
  pub mod tool_call_id;
  pub use tool_call_id::{sanitize_openai_responses_call_id, sanitize_tool_call_id, normalize_tool_call_ids_for_provider, ToolCallIdPolicy};
  ```

- [ ] 运行并确认通过：
  ```bash
  cargo test -p kosong-rs tool_call_id
  cargo check -p kosong-rs
  ```

- [ ] Commit：`feat(kosong-rs): implement tool-call-id normalization`

---

### Task 3: 实现 `request-auth` 模块

**Depends on:** Task 1（不需要，但 `ProviderRequestAuth` 已在 4.2.0 中定义）

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/request_auth.rs`
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs:15`（新增 `pub mod request_auth;` 与 re-export）
- Modify: `rust-ody/crates/kosong-rs/src/errors.rs:38`（为 `ChatProviderError` 补充 `MissingApiKey` 变体）

**步骤：**

- [ ] 搜索现有 `ChatProviderError` 匹配点：
  ```bash
  rg -n "ChatProviderError" rust-ody/crates/kosong-rs/src/
  ```
  当前只有 `errors.rs` 和 `generate.rs`。

- [ ] 写失败测试（先占位）：
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use crate::errors::ChatProviderError;
      use crate::provider::ProviderRequestAuth;

      #[test]
      fn require_api_key_returns_key() {
          let auth = ProviderRequestAuth { api_key: Some("sk".into()), headers: None };
          assert_eq!(require_provider_api_key("p", Some(&auth), None).unwrap(), "sk");
      }

      #[test]
      fn require_api_key_prefers_request_over_default() {
          let auth = ProviderRequestAuth { api_key: Some("req".into()), headers: None };
          assert_eq!(require_provider_api_key("p", Some(&auth), Some("def")).unwrap(), "req");
      }

      #[test]
      fn require_api_key_falls_back_to_default() {
          assert_eq!(require_provider_api_key("p", None, Some("def")).unwrap(), "def");
      }

      #[test]
      fn require_api_key_rejects_missing() {
          let err = require_provider_api_key("openai", None, None).unwrap_err();
          assert!(matches!(err, ChatProviderError::MissingApiKey(_)));
          assert!(err.to_string().contains("apiKey is required"));
      }

      #[test]
      fn merge_headers_combines_maps() {
          let mut default = HashMap::new();
          default.insert("a".into(), "1".into());
          let mut request = HashMap::new();
          request.insert("b".into(), "2".into());
          let merged = merge_request_headers(Some(&default), Some(&request));
          assert_eq!(merged.as_ref().unwrap()["a"], "1");
          assert_eq!(merged.as_ref().unwrap()["b"], "2");
      }

      #[test]
      fn merge_headers_request_overrides_default() {
          let mut default = HashMap::new();
          default.insert("a".into(), "1".into());
          let mut request = HashMap::new();
          request.insert("a".into(), "2".into());
          let merged = merge_request_headers(Some(&default), Some(&request));
          assert_eq!(merged.as_ref().unwrap()["a"], "2");
      }

      #[test]
      fn resolve_uses_factory_when_present() {
          let state = AuthBackedClientState::<String> {
              cached_client: Some("cached".into()),
              client_factory: Some(Box::new(|auth| format!("factory:{}", auth.api_key.as_deref().unwrap_or("")))),
          };
          let auth = ProviderRequestAuth { api_key: Some("k".into()), headers: None };
          let client = resolve_auth_backed_client(&state, Some(&auth), |_auth| "built".into());
          assert_eq!(client, "factory:k");
      }

      #[test]
      fn resolve_reuses_cached_when_no_auth() {
          let state = AuthBackedClientState::<String> {
              cached_client: Some("cached".into()),
              client_factory: None,
          };
          let client = resolve_auth_backed_client(&state, None, |_auth| panic!("should not build"));
          assert_eq!(client, "cached");
      }

      #[test]
      fn resolve_builds_when_auth_present() {
          let state = AuthBackedClientState::<String> {
              cached_client: Some("cached".into()),
              client_factory: None,
          };
          let auth = ProviderRequestAuth { api_key: Some("k".into()), headers: None };
          let client = resolve_auth_backed_client(&state, Some(&auth), |auth| format!("built:{}", auth.api_key.as_deref().unwrap_or("")));
          assert_eq!(client, "built:k");
      }
  }
  ```

- [ ] 运行并确认失败：
  ```bash
  cargo test -p kosong-rs request_auth
  ```
  期望失败：模块/函数未找到，或 `MissingApiKey` 未找到。

- [ ] 更新 `errors.rs`：
  ```rust
  #[derive(Debug, Clone, thiserror::Error)]
  #[error("{provider}: apiKey is required. Provide it via the constructor options, the provider's API-key environment variable, options.auth.apiKey on each request, or an OAuth login.")]
  pub struct APIMissingApiKeyError { pub provider: String }

  #[derive(Debug, Error)]
  pub enum ChatProviderError {
      #[error("API connection error")]
      Connection(APIConnectionError),
      #[error("API timeout error")]
      Timeout(APITimeoutError),
      #[error(transparent)]
      Status(APIStatusError),
      #[error(transparent)]
      ContextOverflow(APIContextOverflowError),
      #[error(transparent)]
      Empty(APIEmptyResponseError),
      #[error("The operation was aborted.")]
      Aborted(AbortError),
      #[error(transparent)]
      MissingApiKey(APIMissingApiKeyError),
  }
  ```

- [ ] 更新 `generate.rs` 中的 `is_retryable_generate_error`，让 `MissingApiKey` 自然落入非 retryable（保留 `_ => false` 即可，因为 match 已经 exhaustive）。若编译器提示 non-exhaustive，则补一条：
  ```rust
  ChatProviderError::MissingApiKey(_) => false,
  ```

- [ ] 实现 `src/request_auth.rs`：
  ```rust
  use std::collections::HashMap;

  use crate::errors::{APIMissingApiKeyError, ChatProviderError};
  use crate::provider::ProviderRequestAuth;

  pub fn require_provider_api_key(
      provider_name: &str,
      auth: Option<&ProviderRequestAuth>,
      default_api_key: Option<&str>,
  ) -> Result<String, ChatProviderError> {
      let api_key = auth
          .as_ref()
          .and_then(|a| a.api_key.clone())
          .or_else(|| default_api_key.map(|s| s.to_string()));
      match api_key {
          Some(key) if !key.is_empty() => Ok(key),
          _ => Err(ChatProviderError::MissingApiKey(APIMissingApiKeyError {
              provider: provider_name.to_string(),
          })),
      }
  }

  pub fn merge_request_headers(
      default_headers: Option<&HashMap<String, String>>,
      request_headers: Option<&HashMap<String, String>>,
  ) -> Option<HashMap<String, String>> {
      let mut merged = HashMap::new();
      if let Some(default) = default_headers {
          merged.extend(default.iter().map(|(k, v)| (k.clone(), v.clone())));
      }
      if let Some(request) = request_headers {
          merged.extend(request.iter().map(|(k, v)| (k.clone(), v.clone())));
      }
      if merged.is_empty() { None } else { Some(merged) }
  }

  pub struct AuthBackedClientState<TClient: Clone> {
      pub cached_client: Option<TClient>,
      pub client_factory: Option<Box<dyn Fn(&ProviderRequestAuth) -> TClient>>,
  }

  pub fn resolve_auth_backed_client<TClient: Clone>(
      state: &AuthBackedClientState<TClient>,
      auth: Option<&ProviderRequestAuth>,
      build: impl FnOnce(Option<&ProviderRequestAuth>) -> TClient,
  ) -> TClient {
      if let Some(factory) = &state.client_factory {
          return factory(auth.unwrap_or(&ProviderRequestAuth { api_key: None, headers: None }));
      }
      if auth.is_none() {
          if let Some(cached) = &state.cached_client {
              return cached.clone();
          }
      }
      build(auth)
  }
  ```

- [ ] 在 `lib.rs` 添加：
  ```rust
  pub mod request_auth;
  pub use request_auth::{require_provider_api_key, merge_request_headers, resolve_auth_backed_client, AuthBackedClientState};
  ```

- [ ] 运行并确认通过：
  ```bash
  cargo test -p kosong-rs request_auth
  cargo check -p kosong-rs
  ```

- [ ] Commit：`feat(kosong-rs): implement request-auth helpers with MissingApiKey error`

---

### Task 4: 实现 `capability-registry` 模块

**Depends on:** Task 1（`ModelCapability` 已在 4.2.0 中定义）

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/capability_registry.rs`
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs:16`（新增 `pub mod capability_registry;` 与 re-export）

**步骤：**

- [ ] 写失败测试：
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use crate::provider::ModelCapability;

      #[test]
      fn openai_reasoning_model() {
          let cap = get_openai_legacy_model_capability("o3-mini");
          assert!(cap.thinking);
          assert!(cap.tool_use);
          assert!(!cap.image_in);
          assert_eq!(cap.max_output_tokens, 16_384);
      }

      #[test]
      fn openai_vision_tool_model() {
          let cap = get_openai_legacy_model_capability("gpt-4o-2024-05-13");
          assert!(cap.image_in);
          assert!(cap.tool_use);
          assert!(!cap.thinking);
      }

      #[test]
      fn unknown_model_returns_unknown() {
          let cap = get_openai_legacy_model_capability("unknown");
          assert!(cap.is_unknown());
      }

      #[test]
      fn anthropic_claude_4_thinks() {
          let cap = get_anthropic_model_capability("claude-opus-4-20250514");
          assert!(cap.thinking);
          assert!(cap.image_in);
          assert_eq!(cap.max_output_tokens, 8_192);
      }

      #[test]
      fn google_gemini_multimodal() {
          let cap = get_google_genai_model_capability("gemini-2.0-flash-exp");
          assert!(cap.image_in);
          assert!(cap.video_in);
          assert!(cap.audio_in);
          assert!(cap.tool_use);
      }

      #[test]
      fn google_gemini_thinking() {
          let cap = get_google_genai_model_capability("gemini-2.5-pro-preview-05-06");
          assert!(cap.thinking);
      }

      #[test]
      fn developer_role_models() {
          assert!(uses_openai_responses_developer_role("gpt-4.1"));
          assert!(uses_openai_responses_developer_role("o3-mini"));
          assert!(!uses_openai_responses_developer_role("gpt-4o"));
      }
  }
  ```

- [ ] 运行并确认失败：
  ```bash
  cargo test -p kosong-rs capability_registry
  ```

- [ ] 实现 `src/capability_registry.rs`：
  ```rust
  use crate::provider::ModelCapability;

  const OPENAI_RESPONSES_DEVELOPER_ROLE_MODELS: &[&str] = &[
      "gpt-4.1",
      "gpt-4.1-mini",
      "gpt-4.1-nano",
      "gpt-5-codex",
      "o1",
      "o1-mini",
      "o1-pro",
      "o3",
      "o3-mini",
      "o3-pro",
      "o4-mini",
  ];

  const OPENAI_VISION_TOOL_PREFIXES: &[&str] = &["gpt-4o", "gpt-4-turbo", "gpt-4.1", "gpt-4.5"];
  const CLAUDE_3_PREFIXES: &[&str] = &["claude-3-", "claude-3.5-", "claude-3.7-"];
  const CLAUDE_4_PREFIXES: &[&str] = &["claude-opus-4", "claude-sonnet-4", "claude-haiku-4"];
  const GEMINI_CATALOGUED_PREFIXES: &[&str] = &[
      "gemini-1.5-pro",
      "gemini-1.5-flash",
      "gemini-2.0-flash",
      "gemini-2.0-pro",
      "gemini-2.5-pro",
      "gemini-2.5-flash",
  ];

  fn openai_reasoning_capability() -> ModelCapability {
      ModelCapability {
          image_in: false,
          video_in: false,
          audio_in: false,
          thinking: true,
          tool_use: true,
          max_context_tokens: 0,
          max_output_tokens: 16_384,
      }
  }

  fn openai_vision_tool_capability() -> ModelCapability {
      ModelCapability {
          image_in: true,
          video_in: false,
          audio_in: false,
          thinking: false,
          tool_use: true,
          max_context_tokens: 0,
          max_output_tokens: 16_384,
      }
  }

  fn openai_text_tool_capability() -> ModelCapability {
      ModelCapability {
          image_in: false,
          video_in: false,
          audio_in: false,
          thinking: false,
          tool_use: true,
          max_context_tokens: 0,
          max_output_tokens: 16_384,
      }
  }

  fn anthropic_vision_tool_capability() -> ModelCapability {
      ModelCapability {
          image_in: true,
          video_in: false,
          audio_in: false,
          thinking: false,
          tool_use: true,
          max_context_tokens: 0,
          max_output_tokens: 8_192,
      }
  }

  fn anthropic_thinking_vision_tool_capability() -> ModelCapability {
      ModelCapability {
          image_in: true,
          video_in: false,
          audio_in: false,
          thinking: true,
          tool_use: true,
          max_context_tokens: 0,
          max_output_tokens: 8_192,
      }
  }

  fn gemini_multimodal_tool_capability() -> ModelCapability {
      ModelCapability {
          image_in: true,
          video_in: true,
          audio_in: true,
          thinking: false,
          tool_use: true,
          max_context_tokens: 0,
          max_output_tokens: 8_192,
      }
  }

  fn gemini_thinking_multimodal_tool_capability() -> ModelCapability {
      ModelCapability {
          image_in: true,
          video_in: true,
          audio_in: true,
          thinking: true,
          tool_use: true,
          max_context_tokens: 0,
          max_output_tokens: 8_192,
      }
  }

  fn normalize_model_name(model_name: &str) -> String {
      model_name.to_lowercase()
  }

  fn has_prefix(model_name: &str, prefixes: &[&str]) -> bool {
      prefixes.iter().any(|p| model_name.starts_with(p))
  }

  fn is_openai_reasoning_model(model_name: &str) -> bool {
      regex::Regex::new(r"^o\d").unwrap().is_match(model_name)
  }

  fn capability_from_catalog(
      model_name: &str,
      catalog: &[(fn(&str) -> bool, ModelCapability)],
  ) -> ModelCapability {
      let normalized = normalize_model_name(model_name);
      for (matches, cap) in catalog {
          if matches(&normalized) {
              return *cap;
          }
      }
      ModelCapability::unknown()
  }

  pub fn get_openai_legacy_model_capability(model_name: &str) -> ModelCapability {
      let catalog: &[(fn(&str) -> bool, ModelCapability)] = &[
          (is_openai_reasoning_model, openai_reasoning_capability()),
          (|name| has_prefix(name, OPENAI_VISION_TOOL_PREFIXES), openai_vision_tool_capability()),
          (|name| name.starts_with("gpt-3.5-turbo"), openai_text_tool_capability()),
      ];
      capability_from_catalog(model_name, catalog)
  }

  pub fn get_openai_responses_model_capability(model_name: &str) -> ModelCapability {
      let catalog: &[(fn(&str) -> bool, ModelCapability)] = &[
          (is_openai_reasoning_model, openai_reasoning_capability()),
          (|name| has_prefix(name, OPENAI_VISION_TOOL_PREFIXES), openai_vision_tool_capability()),
      ];
      capability_from_catalog(model_name, catalog)
  }

  pub fn get_anthropic_model_capability(model_name: &str) -> ModelCapability {
      let catalog: &[(fn(&str) -> bool, ModelCapability)] = &[
          (|name| has_prefix(name, CLAUDE_3_PREFIXES), anthropic_vision_tool_capability()),
          (|name| has_prefix(name, CLAUDE_4_PREFIXES), anthropic_thinking_vision_tool_capability()),
      ];
      capability_from_catalog(model_name, catalog)
  }

  pub fn get_google_genai_model_capability(model_name: &str) -> ModelCapability {
      let normalized = normalize_model_name(model_name);
      if !normalized.starts_with("gemini-") {
          return ModelCapability::unknown();
      }
      if !has_prefix(&normalized, GEMINI_CATALOGUED_PREFIXES) {
          return ModelCapability::unknown();
      }
      if normalized.starts_with("gemini-2.5-") || normalized.contains("thinking") {
          gemini_thinking_multimodal_tool_capability()
      } else {
          gemini_multimodal_tool_capability()
      }
  }

  pub fn uses_openai_responses_developer_role(model_name: &str) -> bool {
      let normalized = normalize_model_name(model_name);
      OPENAI_RESPONSES_DEVELOPER_ROLE_MODELS.iter().any(|m| {
          *m == normalized || normalized.starts_with(&format!("{}-", m))
      })
  }
  ```

- [ ] 在 `lib.rs` 添加：
  ```rust
  pub mod capability_registry;
  pub use capability_registry::{get_openai_legacy_model_capability, get_openai_responses_model_capability, get_anthropic_model_capability, get_google_genai_model_capability, uses_openai_responses_developer_role};
  ```

- [ ] 运行并确认通过：
  ```bash
  cargo test -p kosong-rs capability_registry
  cargo check -p kosong-rs
  ```

- [ ] Commit：`feat(kosong-rs): implement capability registry`

---

### Task 5: 实现 `catalog` 模块

**Depends on:** Task 1（`ProviderType`）

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/catalog.rs`
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs:17`（新增 `pub mod catalog;` 与 re-export）

**步骤：**

- [ ] 写失败测试：
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn infer_wire_type_uses_explicit_type() {
          let entry = CatalogProviderEntry {
              id: None,
              name: None,
              api: None,
              env: None,
              npm: None,
              r#type: Some(ProviderType::Deepseek),
              models: None,
          };
          assert_eq!(infer_wire_type(&entry), Some(ProviderType::Deepseek));
      }

      #[test]
      fn infer_wire_type_from_id() {
          let entry = provider_entry_with_id("claude-3-5-sonnet");
          assert_eq!(infer_wire_type(&entry), Some(ProviderType::Anthropic));
      }

      #[test]
      fn infer_wire_type_unknown() {
          let entry = provider_entry_with_id("foo");
          assert_eq!(infer_wire_type(&entry), None);
      }

      #[test]
      fn catalog_base_url_strips_v1_for_anthropic() {
          let entry = CatalogProviderEntry {
              api: Some("https://api.anthropic.com/v1".into()),
              ..Default::default()
          };
          assert_eq!(catalog_base_url(&entry, ProviderType::Anthropic), Some("https://api.anthropic.com".into()));
      }

      #[test]
      fn catalog_model_to_capability_skips_embedding() {
          let model = CatalogModelEntry {
              id: Some("text-embedding-3".into()),
              name: None,
              family: Some("embedding".into()),
              limit: Some(Limit { context: Some(8192), output: Some(1536) }),
              tool_call: None,
              reasoning: None,
              interleaved: None,
              modalities: Some(Modalities { input: Some(vec!["text".into()]), output: Some(vec!["text".into()]) }),
          };
          assert!(catalog_model_to_capability(&model).is_none());
      }

      #[test]
      fn catalog_model_to_capability_parses_modalities() {
          let model = CatalogModelEntry {
              id: Some("gpt-4o".into()),
              name: None,
              family: None,
              limit: Some(Limit { context: Some(128_000), output: Some(16_384) }),
              tool_call: Some(true),
              reasoning: None,
              interleaved: None,
              modalities: Some(Modalities { input: Some(vec!["text".into(), "image".into()]), output: Some(vec!["text".into()]) }),
          };
          let cap = catalog_model_to_capability(&model).unwrap();
          assert_eq!(cap.id, "gpt-4o");
          assert!(cap.capability.image_in);
          assert!(cap.capability.tool_use);
          assert_eq!(cap.capability.max_context_tokens, 128_000);
          assert_eq!(cap.capability.max_output_tokens, 16_384);
      }

      fn provider_entry_with_id(id: &str) -> CatalogProviderEntry {
          CatalogProviderEntry {
              id: Some(id.into()),
              name: None,
              api: None,
              env: None,
              npm: None,
              r#type: None,
              models: None,
          }
      }
  }
  ```

- [ ] 运行并确认失败：
  ```bash
  cargo test -p kosong-rs catalog
  ```

- [ ] 实现 `src/catalog.rs`：
  ```rust
  use serde::{Deserialize, Serialize};

  use crate::provider::{ModelCapability, ProviderType};

  #[derive(Debug, Clone, Default, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct Limit {
      pub context: Option<i64>,
      pub output: Option<i64>,
  }

  #[derive(Debug, Clone, Default, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct Modalities {
      pub input: Option<Vec<String>>,
      pub output: Option<Vec<String>>,
  }

  #[derive(Debug, Clone, Default, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct CatalogModelEntry {
      pub id: Option<String>,
      pub name: Option<String>,
      pub family: Option<String>,
      pub limit: Option<Limit>,
      #[serde(rename = "tool_call")]
      pub tool_call: Option<bool>,
      pub reasoning: Option<bool>,
      pub interleaved: Option<serde_json::Value>,
      pub modalities: Option<Modalities>,
  }

  #[derive(Debug, Clone, Default, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct CatalogProviderEntry {
      pub id: Option<String>,
      pub name: Option<String>,
      pub api: Option<String>,
      pub env: Option<Vec<String>>,
      pub npm: Option<String>,
      #[serde(rename = "type")]
      pub r#type: Option<ProviderType>,
      pub models: Option<std::collections::HashMap<String, CatalogModelEntry>>,
  }

  pub type Catalog = std::collections::HashMap<String, CatalogProviderEntry>;

  #[derive(Debug, Clone, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct CatalogModel {
      pub id: String,
      #[serde(skip_serializing_if = "Option::is_none")]
      pub name: Option<String>,
      #[serde(skip_serializing_if = "Option::is_none")]
      pub max_output_size: Option<i64>,
      #[serde(skip_serializing_if = "Option::is_none")]
      pub reasoning_key: Option<String>,
      pub capability: ModelCapability,
  }

  fn has_embedding_marker(value: Option<&str>) -> bool {
      let Some(v) = value else { return false };
      let lower = v.to_lowercase();
      lower.contains("embedding")
          || regex::Regex::new(r"(?:^|[-_/])embed(?:$|[-_/])")
              .unwrap()
              .is_match(&lower)
  }

  fn is_usable_chat_model(model: &CatalogModelEntry) -> bool {
      if let Some(output) = &model.modalities {
          if let Some(out) = &output.output {
              if !out.contains(&"text".to_string()) {
                  return false;
              }
          }
      }
      !has_embedding_marker(model.family.as_deref())
          && !has_embedding_marker(model.id.as_deref())
          && !has_embedding_marker(model.name.as_deref())
  }

  pub fn infer_wire_type(entry: &CatalogProviderEntry) -> Option<ProviderType> {
      if let Some(t) = entry.r#type {
          return Some(t);
      }
      let npm = entry.npm.as_deref().unwrap_or("").to_lowercase();
      let id = entry.id.as_deref().unwrap_or("").to_lowercase();
      if npm.contains("anthropic") || id.contains("anthropic") || id.contains("claude") {
          return Some(ProviderType::Anthropic);
      }
      if id.contains("vertex") {
          return Some(ProviderType::Vertexai);
      }
      if npm.contains("google") || id.contains("google") || id.contains("gemini") {
          return Some(ProviderType::GoogleGenai);
      }
      if npm.contains("openai") || id.contains("openai") {
          return Some(ProviderType::OpenAi);
      }
      if id.contains("deepseek") {
          return Some(ProviderType::Deepseek);
      }
      if id.contains("glm") || id.contains("zhipu") || id.contains("bigmodel") {
          return Some(ProviderType::Glm);
      }
      None
  }

  pub fn catalog_base_url(entry: &CatalogProviderEntry, wire: ProviderType) -> Option<String> {
      let api = entry.api.as_deref()?;
      if api.is_empty() {
          return None;
      }
      if wire == ProviderType::Anthropic {
          return Some(api.replace(regex::Regex::new(r"/v1/?$").unwrap(), ""));
      }
      Some(api.to_string())
  }

  pub fn catalog_model_to_capability(model: &CatalogModelEntry) -> Option<CatalogModel> {
      let id = model.id.as_deref()?;
      if id.is_empty() {
          return None;
      }
      let context = model.limit.as_ref().and_then(|l| l.context)?;
      if context <= 0 {
          return None;
      }
      if !is_usable_chat_model(model) {
          return None;
      }
      let inputs = model
          .modalities
          .as_ref()
          .and_then(|m| m.input.clone())
          .unwrap_or_default();
      let output = model.limit.as_ref().and_then(|l| l.output);
      Some(CatalogModel {
          id: id.to_string(),
          name: model.name.clone().filter(|n| !n.is_empty()),
          max_output_size: output.filter(|n| *n > 0),
          reasoning_key: catalog_reasoning_key(model.interleaved.as_ref()),
          capability: ModelCapability {
              image_in: inputs.contains(&"image".to_string()),
              video_in: inputs.contains(&"video".to_string()),
              audio_in: inputs.contains(&"audio".to_string()),
              thinking: model.reasoning.unwrap_or(false),
              tool_use: model.tool_call.unwrap_or(true),
              max_context_tokens: context,
              max_output_tokens: output.unwrap_or(0),
          },
      })
  }

  fn catalog_reasoning_key(interleaved: Option<&serde_json::Value>) -> Option<String> {
      match interleaved {
          Some(v) if v.is_boolean() && v.as_bool() == Some(true) => Some("reasoning_content".into()),
          Some(v) if v.is_object() => v
              .get("field")
              .and_then(|f| f.as_str())
              .map(|s| s.trim().to_string())
              .filter(|s| !s.is_empty()),
          _ => None,
      }
  }

  pub fn catalog_provider_models(entry: &CatalogProviderEntry) -> Vec<CatalogModel> {
      entry
          .models
          .iter()
          .flat_map(|m| m.values())
          .filter_map(|m| catalog_model_to_capability(m))
          .collect()
  }
  ```

- [ ] 在 `lib.rs` 添加：
  ```rust
  pub mod catalog;
  pub use catalog::{catalog_base_url, catalog_model_to_capability, catalog_provider_models, infer_wire_type, Catalog, CatalogModel, CatalogModelEntry, CatalogProviderEntry};
  ```

- [ ] 运行并确认通过：
  ```bash
  cargo test -p kosong-rs catalog
  cargo test -p kosong-rs
  cargo check -p kosong-rs
  ```

- [ ] Commit：`feat(kosong-rs): implement catalog parser and wire type inference`

---

## Part 1 Local Self-Review

- [ ] 1. Spec-coverage：Task 2 覆盖 4.2.1.1；Task 3 覆盖 4.2.1.2；Task 4 覆盖 4.2.1.3；Task 5 覆盖 4.2.1.4；Task 1 是 Task 5 的类型前置。
- [ ] 2. Placeholder scan：所有代码块完整，无 `TODO`/`TBD`。
- [ ] 3. No phantom tasks：每个 Task 都有明确的文件改动、测试命令、commit。
- [ ] 4. Dependency soundness：Task 5 依赖 Task 1；其余 Rust 模块只依赖 4.2.0 已有类型。
- [ ] 5. Caller & build soundness：Task 3 新增 `ChatProviderError::MissingApiKey` 共享签名，同一任务内更新 `errors.rs` 与 `generate.rs` 的 match；每任务以 `cargo check -p kosong-rs` 结束。
- [ ] 6. Test-the-risk：纯函数模块，每个 Task 的单元测试覆盖边界（空 key、冲突 id、embedding 过滤、v1 剥离）。
- [ ] 7. Type一致性：Rust `ProviderType` 通过 serde rename 与 TS 字符串对齐；`ModelCapability` 字段名与 TS camelCase 一致。
