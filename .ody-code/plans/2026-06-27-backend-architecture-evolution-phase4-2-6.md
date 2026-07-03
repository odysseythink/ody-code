# 4.2.6 Google GenAI Provider Implementation Plan

**Goal:** 在 `kosong-rs` 中实现与 TS `packages/kosong/src/providers/google-genai.ts` 逐值等价的 `GoogleGenAIChatProvider`,并通过 L1 fixture 对照证明 Rust 输出与 TS 输出一致。

**Architecture:** `GoogleGenAIChatProvider` 作为 `kosong-rs` 的独立 provider 模块,直接调用 Google GenAI REST API(`generativelanguage.googleapis.com/v1beta`)或 Vertex AI REST API;流式响应解析为与 TS `@google/genai` SDK chunk 同形的 JSON 对象后,统一转换为 `StreamedMessagePart`。复用 4.2.0 的 `ChatProvider` trait、4.2.1 的 `capability_registry` 与 `request_auth`,不引入新的外部 SDK。

**Tech Stack:** Rust 2021 / tokio / reqwest / serde / async-trait;TypeScript / vitest(仅用于 TS 侧 fixture 参照);`httptest` 用于 Rust 侧 HTTP mock。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

新增/修改文件清单(按最终状态):

| 路径 | 责任 |
|---|---|
| `rust-ody/crates/kosong-rs/src/providers/mod.rs` | 暴露 `google_genai` 模块 |
| `rust-ody/crates/kosong-rs/src/providers/google_genai.rs` | Google GenAI provider 实现、转换函数、错误转换、单元测试 |
| `rust-ody/crates/kosong-rs/fixtures/google-genai/text_stream.json` | 流式文本 L1 fixture |
| `rust-ody/crates/kosong-rs/fixtures/google-genai/tool_call_stream.json` | 流式 tool-call L1 fixture |
| `rust-ody/crates/kosong-rs/fixtures/google-genai/non_stream.json` | 非流式响应 L1 fixture |
| `rust-ody/crates/kosong-rs/fixtures/google-genai/vertexai_config.json` | Vertex AI 构造参数 fixture |
| `rust-ody/crates/kosong-rs/src/bin/google_genai_golden.rs` | `kosong-google-genai-golden` 二进制入口 |
| `rust-ody/crates/kosong-rs/Cargo.toml` | 注册 golden binary |

---

## Dependency Overview

```text
Task 1: 模块骨架 + capability 单元测试
  │
  ▼
Task 2: message → Google contents 转换
  │
  ▼
Task 3: finish reason 归一化
  │
  ▼
Task 4: response chunk 解析(parts / usage / id)
  │
  ▼
Task 5: stream/non-stream 适配 + abort 兼容
  │
  ▼
Task 6: GoogleGenAIChatProvider.generate + 错误转换
  │
  ▼
Task 7: withThinking / withMaxCompletionTokens
  │
  ▼
Task 8: L1 fixtures + golden 对照
```

---

## Risks & Open Questions

| 风险 | 应对 |
|---|---|
| Rust 无官方 Google GenAI SDK,REST API 字段与 TS SDK 暴露的 chunk 字段可能有差异 | fixture 采用「SDK chunk 同形对象」,Rust 按同形解析;差异集中在 `extract_chunk_parts`,单点可控 |
| `function_response` 排序或 tool-call id 构造 `{name}_{id}` 与 TS 不对齐 | 作为 L1 硬门;不对齐则登记 `parity/known-gaps.md`,保留 TS 回调 |
| abort 语义:Google REST 不支持请求级 abort,只能靠手动 race | 在 `generate()` 启动与每个 chunk 边界检查 `AbortSignal`;L1 用 abort fixture 验证 |
| Vertex AI _auth 路径与 TS 不同(Rust 用 service account / ADC,无 `google-auth-library`) | 本阶段只验证 URL 构造与参数分支,真实 auth 留 4.2.7 / 4.5.0 决策 |

---

### Task 1: 添加 `google_genai` 模块骨架与 capability 单元测试

**Depends on:** 4.2.0(`kosong-rs` crate 与 `ChatProvider` trait 已落地),4.2.1(`capability_registry.rs` 已含 `get_google_genai_model_capability`)

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/providers/google_genai.rs:1-60`
- Modify: `rust-ody/crates/kosong-rs/src/providers/mod.rs:1-2`
- Test: `rust-ody/crates/kosong-rs/src/providers/google_genai.rs` 内 `#[cfg(test)]`

**实现步骤**:

- [ ] 修改 `rust-ody/crates/kosong-rs/src/providers/mod.rs`:
  ```rust
  pub mod anthropic;
  pub mod google_genai;
  ```

- [ ] 创建 `rust-ody/crates/kosong-rs/src/providers/google_genai.rs`,先写构造/访问器测试:
  ```rust
  use crate::capability_registry::get_google_genai_model_capability;
  use crate::provider::{ChatProvider, ModelCapability, ThinkingEffort};

  pub struct GoogleGenAIChatProvider {
      model: String,
      api_key: Option<String>,
      vertexai: bool,
      project: Option<String>,
      location: Option<String>,
      stream: bool,
      generation_kwargs: serde_json::Value,
  }

  impl GoogleGenAIChatProvider {
      pub fn new(model: impl Into<String>) -> Self {
          Self {
              model: model.into(),
              api_key: None,
              vertexai: false,
              project: None,
              location: None,
              stream: true,
              generation_kwargs: serde_json::Value::Object(Default::default()),
          }
      }

      pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
          let key = api_key.into();
          self.api_key = if key.is_empty() { None } else { Some(key) };
          self
      }

      pub fn with_vertexai(mut self, project: impl Into<String>, location: impl Into<String>) -> Self {
          self.vertexai = true;
          self.project = Some(project.into());
          self.location = Some(location.into());
          self
      }
  }

  #[async_trait::async_trait]
  impl ChatProvider for GoogleGenAIChatProvider {
      fn name(&self) -> &str { "google_genai" }
      fn model_name(&self) -> &str { &self.model }
      fn thinking_effort(&self) -> Option<ThinkingEffort> { None }
      fn get_capability(&self, model: Option<&str>) -> ModelCapability {
          get_google_genai_model_capability(model.unwrap_or(&self.model))
      }

      async fn generate(
          &self,
          _system_prompt: &str,
          _tools: &[crate::provider::Tool],
          _history: &[crate::message::Message],
          _options: Option<crate::provider::GenerateOptions>,
      ) -> Result<crate::generate::StreamedMessage, crate::errors::ChatProviderError> {
          unimplemented!("Task 6")
      }

      fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
          let mut clone = self.clone();
          clone.generation_kwargs = self.generation_kwargs.clone();
          Box::new(clone)
      }
  }

  impl Clone for GoogleGenAIChatProvider {
      fn clone(&self) -> Self {
          Self {
              model: self.model.clone(),
              api_key: self.api_key.clone(),
              vertexai: self.vertexai,
              project: self.project.clone(),
              location: self.location.clone(),
              stream: self.stream,
              generation_kwargs: self.generation_kwargs.clone(),
          }
      }
  }

  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn provider_name_and_model() {
          let p = GoogleGenAIChatProvider::new("gemini-2.0-flash").with_api_key("sk-test");
          assert_eq!(p.name(), "google_genai");
          assert_eq!(p.model_name(), "gemini-2.0-flash");
      }

      #[test]
      fn capability_for_gemini_flash() {
          let p = GoogleGenAIChatProvider::new("gemini-2.0-flash-exp");
          let cap = p.get_capability(None);
          assert!(cap.image_in);
          assert!(cap.video_in);
          assert!(cap.audio_in);
          assert!(cap.tool_use);
          assert!(!cap.thinking);
      }

      #[test]
      fn capability_for_gemini_thinking() {
          let p = GoogleGenAIChatProvider::new("gemini-2.5-pro-preview-05-06");
          let cap = p.get_capability(None);
          assert!(cap.thinking);
      }
  }
  ```

- [ ] 运行测试并确认失败(因为 `unimplemented!` 与类型不匹配不影响构造测试):
  ```bash
  cd rust-ody && cargo test -p kosong-rs google_genai::tests
  ```
  预期失败:`unimplemented!("Task 6")` 处 panic 仅当调用 `generate`,构造测试应通过。若编译失败,先修复模块签名。

- [ ] 实现最小骨架(已在上文给出),运行测试:
  ```bash
  cd rust-ody && cargo test -p kosong-rs google_genai::tests
  ```
  预期:3 个测试通过。

- [ ] 提交:`git add rust-ody/crates/kosong-rs/src/providers/mod.rs rust-ody/crates/kosong-rs/src/providers/google_genai.rs && git commit -m "feat(kosong-rs): scaffold GoogleGenAI provider module"`

---

### Task 2: 实现 message → Google contents 转换

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/google_genai.rs:61-220`
- Test: `rust-ody/crates/kosong-rs/src/providers/google_genai.rs` 内 `#[cfg(test)]`

**实现步骤**:

- [ ] 先写转换测试(失败):
  ```rust
  #[cfg(test)]
  mod conversion_tests {
      use super::*;
      use crate::message::{ContentPart, Message, Role, ToolCall, UrlPayload};
      use crate::provider::Tool;

      #[test]
      fn user_text_message_to_content() {
          let msgs = vec![Message::user_text("hello")];
          let contents = messages_to_google_genai_contents(&msgs).unwrap();
          assert_eq!(contents.len(), 1);
          assert_eq!(contents[0].role, "user");
          assert_eq!(contents[0].parts, vec![GooglePart::text("hello")]);
      }

      #[test]
      fn assistant_tool_call_to_content() {
          let msg = Message {
              role: Role::Assistant,
              name: None,
              content: vec![],
              tool_calls: vec![ToolCall {
                  call_type: "function".into(),
                  id: "tc_1".into(),
                  name: "read".into(),
                  arguments: Some(r#"{"path":"/a"}"#.into()),
                  extras: None,
                  stream_index: None,
              }],
              tool_call_id: None,
              partial: None,
          };
          let contents = messages_to_google_genai_contents(&[msg]).unwrap();
          assert_eq!(contents[0].parts.len(), 1);
          assert!(matches!(
              &contents[0].parts[0],
              GooglePart::FunctionCall { name, args, .. }
              if name == "read" && args.get("path") == Some(&serde_json::Value::String("/a".into()))
          ));
      }

      #[test]
      fn tool_results_sorted_by_assistant_tool_call_order() {
          let assistant = Message {
              role: Role::Assistant,
              name: None,
              content: vec![],
              tool_calls: vec![
                  ToolCall { call_type: "function".into(), id: "id_b".into(), name: "b".into(), arguments: None, extras: None, stream_index: None },
                  ToolCall { call_type: "function".into(), id: "id_a".into(), name: "a".into(), arguments: None, extras: None, stream_index: None },
              ],
              tool_call_id: None,
              partial: None,
          };
          let tool_b = Message {
              role: Role::Tool,
              name: None,
              content: vec![ContentPart::Text { text: "out_b".into() }],
              tool_calls: vec![],
              tool_call_id: Some("id_b".into()),
              partial: None,
          };
          let tool_a = Message {
              role: Role::Tool,
              name: None,
              content: vec![ContentPart::Text { text: "out_a".into() }],
              tool_calls: vec![],
              tool_call_id: Some("id_a".into()),
              partial: None,
          };
          let contents = messages_to_google_genai_contents(&[assistant, tool_a, tool_b]).unwrap();
          assert_eq!(contents.len(), 2);
          assert_eq!(contents[1].role, "user");
          let names: Vec<_> = contents[1].parts.iter().filter_map(|p| match p {
              GooglePart::FunctionResponse { name, .. } => Some(name.as_str()),
              _ => None,
          }).collect();
          assert_eq!(names, vec!["b", "a"]);
      }

      #[test]
      fn media_url_to_inline_data() {
          let msg = Message {
              role: Role::User,
              name: None,
              content: vec![ContentPart::ImageUrl {
                  image_url: UrlPayload { url: "data:image/png;base64,ABC".into(), id: None },
              }],
              tool_calls: vec![],
              tool_call_id: None,
              partial: None,
          };
          let contents = messages_to_google_genai_contents(&[msg]).unwrap();
          assert!(matches!(
              &contents[0].parts[0],
              GooglePart::InlineData { mime_type, data } if mime_type == "image/png" && data == "ABC"
          ));
      }

      #[test]
      fn system_message_in_history_wrapped_as_user() {
          let msg = Message {
              role: Role::System,
              name: None,
              content: vec![ContentPart::Text { text: "sys".into() }],
              tool_calls: vec![],
              tool_call_id: None,
              partial: None,
          };
          let contents = messages_to_google_genai_contents(&[msg]).unwrap();
          assert_eq!(contents[0].role, "user");
          assert_eq!(contents[0].parts, vec![GooglePart::text("<system>sys</system>")]);
      }
  }
  ```

- [ ] 运行测试确认失败:
  ```bash
  cd rust-ody && cargo test -p kosong-rs google_genai::conversion_tests
  ```
  预期:编译失败(函数未定义)。

- [ ] 实现转换函数(完整代码,放入 `google_genai.rs` 模块作用域):
  ```rust
  use serde::{Deserialize, Serialize};
  use crate::message::{ContentPart, Message, Role, ToolCall, UrlPayload};
  use crate::provider::Tool;

  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub(crate) enum GooglePart {
      Text { text: String },
      InlineData { mime_type: String, data: String },
      FileData { file_uri: String, mime_type: String },
      FunctionCall { name: String, args: serde_json::Value, thought_signature: Option<String> },
      FunctionResponse { name: String, response: serde_json::Value, parts: Vec<serde_json::Value> },
  }

  impl GooglePart {
      fn text(s: impl Into<String>) -> Self { Self::Text { text: s.into() } }
  }

  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub(crate) struct GoogleContent {
      pub role: String,
      pub parts: Vec<GooglePart>,
  }

  fn tool_to_google_genai(tool: &Tool) -> serde_json::Value {
      serde_json::json!({
          "functionDeclarations": [{
              "name": tool.name,
              "description": tool.description,
              "parameters": tool.parameters,
          }]
      })
  }

  fn convert_media_url(url: &str, fallback_mime: &str) -> GooglePart {
      if let Some(rest) = url.strip_prefix("data:") {
          let comma = rest.find(',').unwrap_or(rest.len());
          let meta = &rest[..comma];
          let data = &rest[comma.min(rest.len().saturating_sub(1)) + 1..];
          let mime = meta.split_once(':').and_then(|(_, m)| m.split_once(';').map(|(m, _)| m))
              .unwrap_or(fallback_mime);
          return GooglePart::InlineData { mime_type: mime.to_string(), data: data.to_string() };
      }
      let mime = std::path::Path::new(url)
          .extension()
          .and_then(|e| e.to_str())
          .map(|ext| match ext.to_lowercase().as_str() {
              "png" => "image/png",
              "jpg" | "jpeg" => "image/jpeg",
              "gif" => "image/gif",
              "webp" => "image/webp",
              "mp3" | "mpeg" => "audio/mpeg",
              "wav" => "audio/wav",
              "ogg" => "audio/ogg",
              "mp4" => "video/mp4",
              _ => fallback_mime,
          })
          .unwrap_or(fallback_mime);
      GooglePart::FileData { file_uri: url.to_string(), mime_type: mime.to_string() }
  }

  fn content_part_to_google(part: &ContentPart) -> Option<GooglePart> {
      Some(match part {
          ContentPart::Text { text } => GooglePart::text(text.clone()),
          ContentPart::Think { .. } => return None,
          ContentPart::ImageUrl { image_url } => convert_media_url(&image_url.url, "image/jpeg"),
          ContentPart::AudioUrl { audio_url } => convert_media_url(&audio_url.url, "audio/mpeg"),
          ContentPart::VideoUrl { video_url } => convert_media_url(&video_url.url, "video/mp4"),
      })
  }

  fn parse_tool_arguments(arguments: Option<&str>) -> Result<serde_json::Value, crate::errors::ChatProviderError> {
      match arguments {
          None | Some("") => Ok(serde_json::Value::Object(Default::default())),
          Some(s) => {
              let v: serde_json::Value = serde_json::from_str(s)
                  .map_err(|_| crate::errors::ChatProviderError::Other(format!("Tool call arguments must be valid JSON: {s}")))?;
              if !v.is_object() {
                  return Err(crate::errors::ChatProviderError::Other("Tool call arguments must be a JSON object.".into()));
              }
              Ok(v)
          }
      }
  }

  fn tool_call_to_google(tool_call: &ToolCall) -> Result<GooglePart, crate::errors::ChatProviderError> {
      let args = parse_tool_arguments(tool_call.arguments.as_deref())?;
      Ok(GooglePart::FunctionCall {
          name: tool_call.name.clone(),
          args,
          thought_signature: tool_call.extras.as_ref()
              .and_then(|e| e.get("thought_signature_b64"))
              .and_then(|v| v.as_str())
              .map(Into::into),
      })
  }

  fn tool_call_id_to_name(id: &str, tool_name_by_id: &std::collections::HashMap<String, String>) -> String {
      if let Some(name) = tool_name_by_id.get(id) { return name.clone(); }
      let re = regex::Regex::new(r"^(.+)_[^_]+$").unwrap();
      re.captures(id).and_then(|c| c.get(1)).map(|m| m.as_str().to_string()).unwrap_or_else(|| id.to_string())
  }

  fn tool_message_to_parts(message: &Message, tool_name_by_id: &std::collections::HashMap<String, String>)
      -> Result<Vec<GooglePart>, crate::errors::ChatProviderError> {
      let id = message.tool_call_id.as_deref()
          .ok_or_else(|| crate::errors::ChatProviderError::Other("Tool response is missing toolCallId.".into()))?;
      let mut text = String::new();
      let mut media = Vec::new();
      for part in &message.content {
          match part {
              ContentPart::Text { text: t } => text.push_str(t),
              other => if let Some(gp) = content_part_to_google(other) { media.push(gp); }
          }
      }
      let name = tool_call_id_to_name(id, tool_name_by_id);
      let mut parts = vec![GooglePart::FunctionResponse {
          name,
          response: serde_json::json!({ "output": text }),
          parts: vec![],
      }];
      parts.extend(media);
      Ok(parts)
  }

  pub(crate) fn messages_to_google_genai_contents(messages: &[Message]) -> Result<Vec<GoogleContent>, crate::errors::ChatProviderError> {
      let mut contents = Vec::new();
      let mut tool_name_by_id = std::collections::HashMap::new();
      let mut i = 0;
      while i < messages.len() {
          let msg = &messages[i];
          if msg.role == Role::System {
              let text: String = msg.content.iter().filter_map(|p| match p {
                  ContentPart::Text { text } => Some(text.as_str()),
                  _ => None,
              }).collect::<Vec<_>>().join("\n");
              if !text.is_empty() {
                  contents.push(GoogleContent { role: "user".into(), parts: vec![GooglePart::text(format!("<system>{text}</system>"))] });
              }
              i += 1;
              continue;
          }
          if msg.role == Role::Assistant && !msg.tool_calls.is_empty() {
              let mut parts = Vec::new();
              for part in &msg.content { if let Some(gp) = content_part_to_google(part) { parts.push(gp); } }
              for tc in &msg.tool_calls {
                  tool_name_by_id.insert(tc.id.clone(), tc.name.clone());
                  parts.push(tool_call_to_google(tc)?);
              }
              contents.push(GoogleContent { role: "model".into(), parts });

              let expected_ids: Vec<_> = msg.tool_calls.iter().map(|tc| tc.id.clone()).collect();
              let mut j = i + 1;
              let mut tool_messages = Vec::new();
              while j < messages.len() && messages[j].role == Role::Tool {
                  tool_messages.push(&messages[j]);
                  j += 1;
              }
              if !tool_messages.is_empty() {
                  let mut by_id = std::collections::HashMap::new();
                  let mut seen = std::collections::HashSet::new();
                  for tm in &tool_messages {
                      let id = tm.tool_call_id.as_deref().ok_or_else(|| crate::errors::ChatProviderError::Other("Tool response is missing toolCallId.".into()))?;
                      if !seen.insert(id) { return Err(crate::errors::ChatProviderError::Other(format!("Duplicate tool response for id: {id}"))); }
                      by_id.insert(id, *tm);
                  }
                  let mut sorted_parts = Vec::new();
                  for expected in &expected_ids {
                      let tm = by_id.remove(expected.as_str())
                          .ok_or_else(|| crate::errors::ChatProviderError::Other(format!("Missing tool responses for ids: {expected}")))?;
                      sorted_parts.extend(tool_message_to_parts(tm, &tool_name_by_id)?);
                  }
                  if !by_id.is_empty() {
                      return Err(crate::errors::ChatProviderError::Other(format!(
                          "Unexpected tool responses for ids: {:?}",
                          by_id.keys().collect::<Vec<_>>()
                      )));
                  }
                  contents.push(GoogleContent { role: "user".into(), parts: sorted_parts });
                  i = j;
                  continue;
              }
              i += 1;
              continue;
          }
          if msg.role == Role::Tool {
              contents.push(GoogleContent { role: "user".into(), parts: tool_message_to_parts(msg, &tool_name_by_id)? });
              i += 1;
              continue;
          }
          // user / assistant without tool calls
          let mut parts = Vec::new();
          for part in &msg.content { if let Some(gp) = content_part_to_google(part) { parts.push(gp); } }
          contents.push(GoogleContent { role: if msg.role == Role::Assistant { "model".into() } else { "user".into() }, parts });
          i += 1;
      }
      Ok(contents)
  }
  ```

- [ ] 运行测试:
  ```bash
  cd rust-ody && cargo test -p kosong-rs google_genai::conversion_tests
  ```
  预期:5 个测试通过。

- [ ] 运行全包编译检查:
  ```bash
  cd rust-ody && cargo check -p kosong-rs
  ```
  预期:无错误。

- [ ] 提交:`git add rust-ody/crates/kosong-rs/src/providers/google_genai.rs && git commit -m "feat(kosong-rs): GoogleGenAI message-to-contents conversion"`

---

### Task 3: 实现 finish reason 归一化

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/google_genai.rs:221-280`
- Test: `rust-ody/crates/kosong-rs/src/providers/google_genai.rs` 内 `#[cfg(test)]`

**实现步骤**:

- [ ] 先写测试:
  ```rust
  #[cfg(test)]
  mod finish_reason_tests {
      use super::*;
      use crate::provider::FinishReason;

      #[test]
      fn maps_stop_to_completed() {
          assert_eq!(normalize_google_genai_finish_reason(&serde_json::json!("STOP")).finish_reason, Some(FinishReason::Completed));
      }

      #[test]
      fn maps_max_tokens_to_truncated() {
          assert_eq!(normalize_google_genai_finish_reason(&serde_json::json!("MAX_TOKENS")).finish_reason, Some(FinishReason::Truncated));
      }

      #[test]
      fn maps_safety_to_filtered() {
          for raw in ["SAFETY", "RECITATION", "BLOCKLIST", "PROHIBITED_CONTENT", "SPII", "IMAGE_SAFETY"] {
              assert_eq!(normalize_google_genai_finish_reason(&serde_json::json!(raw)).finish_reason, Some(FinishReason::Filtered), "failed for {raw}");
          }
      }

      #[test]
      fn maps_other_to_other() {
          assert_eq!(normalize_google_genai_finish_reason(&serde_json::json!("OTHER")).finish_reason, Some(FinishReason::Other));
      }

      #[test]
      fn unspecified_returns_null() {
          let r = normalize_google_genai_finish_reason(&serde_json::json!("FINISH_REASON_UNSPECIFIED"));
          assert_eq!(r.finish_reason, None);
          assert_eq!(r.raw_finish_reason, None);
      }

      #[test]
      fn invalid_object_returns_null() {
          let r = normalize_google_genai_finish_reason(&serde_json::json!({"foo":1}));
          assert_eq!(r.finish_reason, None);
          assert_eq!(r.raw_finish_reason, None);
      }
  }
  ```

- [ ] 运行测试确认失败:
  ```bash
  cd rust-ody && cargo test -p kosong-rs google_genai::finish_reason_tests
  ```

- [ ] 实现函数:
  ```rust
  #[derive(Debug, Clone, PartialEq)]
  pub(crate) struct NormalizedFinishReason {
      pub finish_reason: Option<crate::provider::FinishReason>,
      pub raw_finish_reason: Option<String>,
  }

  pub(crate) fn normalize_google_genai_finish_reason(raw: &serde_json::Value) -> NormalizedFinishReason {
      let raw_string = match raw {
          serde_json::Value::String(s) => s.to_uppercase(),
          serde_json::Value::Number(n) => n.to_string().to_uppercase(),
          serde_json::Value::Bool(b) => b.to_string().to_uppercase(),
          _ => return NormalizedFinishReason { finish_reason: None, raw_finish_reason: None },
      };
      if raw_string.is_empty() || raw_string == "FINISH_REASON_UNSPECIFIED" {
          return NormalizedFinishReason { finish_reason: None, raw_finish_reason: None };
      }
      use crate::provider::FinishReason;
      let finish_reason = match raw_string.as_str() {
          "STOP" => Some(FinishReason::Completed),
          "MAX_TOKENS" => Some(FinishReason::Truncated),
          "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" | "IMAGE_SAFETY" => Some(FinishReason::Filtered),
          _ => Some(FinishReason::Other),
      };
      NormalizedFinishReason { finish_reason, raw_finish_reason: Some(raw_string) }
  }
  ```

- [ ] 运行测试:
  ```bash
  cd rust-ody && cargo test -p kosong-rs google_genai::finish_reason_tests
  ```
  预期:6 个测试通过。

- [ ] 提交。

---

### Task 4: 实现 response chunk 解析(parts / usage / id)

**Depends on:** Task 3

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/google_genai.rs:281-420`
- Test: `rust-ody/crates/kosong-rs/src/providers/google_genai.rs` 内 `#[cfg(test)]`

**实现步骤**:

- [ ] 先写测试,覆盖 text/thought/function_call/usage/id:
  ```rust
  #[cfg(test)]
  mod chunk_tests {
      use super::*;
      use crate::message::{StreamedMessagePart, ToolCall};
      use crate::usage::TokenUsage;

      fn chunk(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
          value.as_object().unwrap().clone()
      }

      #[test]
      fn extracts_text_part() {
          let parts = extract_chunk_parts(&chunk(serde_json::json!({
              "candidates": [{"content": {"parts": [{"text": "hello"}]}}]
          })));
          assert_eq!(parts, vec![StreamedMessagePart::text("hello")]);
      }

      #[test]
      fn extracts_thought_part() {
          let parts = extract_chunk_parts(&chunk(serde_json::json!({
              "candidates": [{"content": {"parts": [{"text": "think", "thought": true}]}}]
          })));
          assert_eq!(parts, vec![StreamedMessagePart::think("think")]);
      }

      #[test]
      fn extracts_function_call() {
          let parts = extract_chunk_parts(&chunk(serde_json::json!({
              "candidates": [{"content": {"parts": [{
                  "functionCall": {"name": "read", "args": {"path": "/a"}, "id": "abc"}
              }]}}]
          })));
          assert_eq!(parts.len(), 1);
          assert!(matches!(
              &parts[0],
              StreamedMessagePart::ToolCall(ToolCall { id, name, arguments, .. })
              if id == "read_abc" && name == "read" && arguments.as_deref() == Some(r#"{"path":"/a"}"#)
          ));
      }

      #[test]
      fn extracts_usage() {
          let usage = extract_usage(&chunk(serde_json::json!({
              "usageMetadata": {
                  "promptTokenCount": 100,
                  "cachedContentTokenCount": 30,
                  "candidatesTokenCount": 20
              }
          })));
          assert_eq!(usage, Some(TokenUsage {
              input_other: 70,
              output: 20,
              input_cache_read: 30,
              input_cache_creation: 0,
          }));
      }

      #[test]
      fn extracts_response_id() {
          assert_eq!(
              extract_id(&chunk(serde_json::json!({"responseId": "resp_1"}))),
              Some("resp_1".into())
          );
      }
  }
  ```

- [ ] 运行确认失败:
  ```bash
  cd rust-ody && cargo test -p kosong-rs google_genai::chunk_tests
  ```

- [ ] 实现解析函数:
  ```rust
  use crate::message::{StreamedMessagePart, ToolCall};
  use crate::usage::TokenUsage;
  use serde_json::Map;

  pub(crate) fn extract_id(response: &Map<String, serde_json::Value>) -> Option<String> {
      response.get("responseId").and_then(|v| v.as_str()).map(Into::into)
  }

  pub(crate) fn extract_usage(response: &Map<String, serde_json::Value>) -> Option<TokenUsage> {
      let meta = response.get("usageMetadata")?.as_object()?;
      let prompt = meta.get("promptTokenCount").and_then(|v| v.as_u64()).unwrap_or(0) as i64;
      let cached = meta.get("cachedContentTokenCount").and_then(|v| v.as_u64()).unwrap_or(0) as i64;
      let output = meta.get("candidatesTokenCount").and_then(|v| v.as_u64()).unwrap_or(0) as i64;
      Some(TokenUsage {
          input_other: (prompt - cached).max(0),
          output,
          input_cache_read: cached,
          input_cache_creation: 0,
      })
  }

  pub(crate) fn extract_chunk_parts(response: &Map<String, serde_json::Value>) -> Vec<StreamedMessagePart> {
      let mut out = Vec::new();
      let candidates = match response.get("candidates").and_then(|v| v.as_array()) {
          Some(c) => c,
          None => return out,
      };
      for candidate in candidates {
          let content = match candidate.get("content").and_then(|v| v.as_object()) {
              Some(c) => c,
              None => continue,
          };
          let parts = match content.get("parts").and_then(|v| v.as_array()) {
              Some(p) => p,
              None => continue,
          };
          for part in parts {
              let obj = match part.as_object() {
                  Some(o) => o,
                  None => continue,
              };
              if obj.get("thought").and_then(|v| v.as_bool()) == Some(true) {
                  if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                      out.push(StreamedMessagePart::think(text));
                  }
              } else if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                  out.push(StreamedMessagePart::text(text));
              } else if let Some(fc) = obj.get("functionCall").or_else(|| obj.get("function_call")).and_then(|v| v.as_object()) {
                  let name = fc.get("name").and_then(|v| v.as_str()).unwrap_or("");
                  if name.is_empty() { continue; }
                  let id = fc.get("id").and_then(|v| v.as_str()).unwrap_or("");
                  let args = fc.get("args").cloned().unwrap_or(serde_json::Value::Object(Default::default()));
                  let thought_sig = obj.get("thoughtSignature").or_else(|| obj.get("thought_signature")).and_then(|v| v.as_str());
                  let extras = thought_sig.map(|s| serde_json::json!({"thought_signature_b64": s}));
                  out.push(StreamedMessagePart::ToolCall(ToolCall {
                      call_type: "function".into(),
                      id: format!("{name}_{id}"),
                      name: name.into(),
                      arguments: Some(args.to_string()),
                      extras,
                      stream_index: None,
                  }));
              }
          }
      }
      out
  }
  ```

- [ ] 运行测试:
  ```bash
  cd rust-ody && cargo test -p kosong-rs google_genai::chunk_tests
  ```

- [ ] 提交。

---

### Task 5: 实现 stream/non-stream 适配与 abort 兼容

**Depends on:** Task 4

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/google_genai.rs:421-560`
- Test: `rust-ody/crates/kosong-rs/src/providers/google_genai.rs` 内 `#[cfg(test)]`

**实现步骤**:

- [ ] 先写测试:
  ```rust
  #[cfg(test)]
  mod streamed_message_tests {
      use super::*;
      use crate::provider::FinishReason;
      use crate::usage::TokenUsage;
      use futures_util::StreamExt;

      #[tokio::test]
      async fn collects_text_from_stream() {
          let chunks = vec![
              serde_json::json!({"candidates": [{"content": {"parts": [{"text": "hello"}]}}]}),
              serde_json::json!({"candidates": [{"content": {"parts": [{"text": " world"}]}}]}),
          ];
          let msg = GoogleGenAIStreamedMessage::from_chunks(chunks, None);
          let parts: Vec<_> = msg.collect().await;
          assert_eq!(parts, vec![StreamedMessagePart::text("hello"), StreamedMessagePart::text(" world")]);
          assert_eq!(msg.id(), None);
      }

      #[tokio::test]
      async fn collects_from_non_stream_response() {
          let resp = serde_json::json!({
              "responseId": "r1",
              "candidates": [{"content": {"parts": [{"text": "hi"}]}, "finishReason": "STOP"}],
              "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 1}
          });
          let msg = GoogleGenAIStreamedMessage::from_response(resp, None);
          let parts: Vec<_> = msg.collect().await;
          assert_eq!(parts, vec![StreamedMessagePart::text("hi")]);
          assert_eq!(msg.id(), Some("r1".into()));
          assert_eq!(msg.usage(), Some(TokenUsage { input_other: 5, output: 1, input_cache_read: 0, input_cache_creation: 0 }));
          assert_eq!(msg.finish_reason(), Some(FinishReason::Completed));
      }

      #[tokio::test]
      async fn abort_mid_stream_throws() {
          let signal = AbortSignal::new();
          let signal_clone = signal.clone();
          let chunks = vec![
              serde_json::json!({"candidates": [{"content": {"parts": [{"text": "a"}]}}]}),
              serde_json::json!({"candidates": [{"content": {"parts": [{"text": "b"}]}}]}),
          ];
          let mut msg = GoogleGenAIStreamedMessage::from_chunks(chunks, Some(signal));
          signal_clone.abort();
          let err = msg.next().await.unwrap_err();
          assert!(format!("{err}").contains("aborted") || format!("{err}").contains("Aborted"));
      }
  }
  ```

- [ ] 运行确认失败:
  ```bash
  cd rust-ody && cargo test -p kosong-rs google_genai::streamed_message_tests
  ```

- [ ] 实现 `GoogleGenAIStreamedMessage`。因 `StreamedMessage` 类型在 `crate::generate` 中通过 `from_parts` 构造,且需要跨异步迭代持有 `id`/`usage`/`finish_reason`,最简方式是预先收集所有 `StreamedMessagePart` 并在构造时计算聚合字段:
  ```rust
  use crate::generate::StreamedMessage;
  use crate::provider::{AbortSignal, FinishReason};
  use crate::message::StreamedMessagePart;

  pub(crate) struct GoogleGenAIStreamedMessage;

  impl GoogleGenAIStreamedMessage {
      pub fn from_chunks<I>(chunks: I, signal: Option<AbortSignal>) -> StreamedMessage
      where I: IntoIterator<Item = serde_json::Value> {
          let mut id: Option<String> = None;
          let mut usage: Option<TokenUsage> = None;
          let mut finish_reason: Option<FinishReason> = None;
          let mut raw_finish_reason: Option<String> = None;
          let mut parts = Vec::new();

          for chunk in chunks {
              if let Some(sig) = signal.as_ref() {
                  if sig.is_aborted() {
                      break;
                  }
              }
              if let Some(obj) = chunk.as_object() {
                  if let Some(new_id) = extract_id(obj) { id = Some(new_id); }
                  if let Some(new_usage) = extract_usage(obj) { usage = Some(new_usage); }
                  let normalized = obj.get("finishReason").or_else(|| obj.get("finish_reason"))
                      .map(normalize_google_genai_finish_reason)
                      .unwrap_or(NormalizedFinishReason { finish_reason: None, raw_finish_reason: None });
                  if normalized.finish_reason.is_some() || normalized.raw_finish_reason.is_some() {
                      finish_reason = normalized.finish_reason;
                      raw_finish_reason = normalized.raw_finish_reason;
                  }
                  parts.extend(extract_chunk_parts(obj));
              }
          }

          StreamedMessage::from_parts(parts, id, usage, finish_reason, raw_finish_reason)
      }

      pub fn from_response(response: serde_json::Value, signal: Option<AbortSignal>) -> StreamedMessage {
          Self::from_chunks(std::iter::once(response), signal)
      }
  }
  ```
  **注意**:上述实现为了测试方便预收集。真实 `generate()` 中流的解析见 Task 6;若需要真正的异步流,可用 `stream::unfold` 包装 chunk 迭代器,但 L1  fixture 与测试均使用预收集语义,与 TS `GoogleGenAIStreamedMessage` 暴露的 async iterator 等价。

- [ ] 运行测试:
  ```bash
  cd rust-ody && cargo test -p kosong-rs google_genai::streamed_message_tests
  ```

- [ ] 提交。

---

### Task 6: 实现 `GoogleGenAIChatProvider.generate` 与错误转换

**Depends on:** Task 2, Task 5

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/google_genai.rs:561-720`
- Test: `rust-ody/crates/kosong-rs/src/providers/google_genai.rs` 内 `#[cfg(test)]`

**实现步骤**:

- [ ] 先写 HTTP 级测试(失败)。使用 `httptest` mock 服务器,构造一个 SSE 流返回两个 text chunk:
  ```rust
  #[cfg(test)]
  mod generate_tests {
      use super::*;
      use futures_util::StreamExt;

      #[tokio::test]
      async fn streams_text_over_rest_sse() {
          let server = httptest::ServerBuilder::new()
              .bind_addr("127.0.0.1:0".parse().unwrap())
              .run()
              .unwrap();
          server.expect(
              httptest::Expectation::matching(httptest::matchers::request::method_path("POST", "/v1beta/models/gemini-test:streamGenerateContent"))
                  .respond_with(httptest::responders::status_code(200).body(
                      "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}]}\n\n\
                       data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" world\"}]},\"finishReason\":\"STOP\"}]}\n\n",
                  )),
          );

          let provider = GoogleGenAIChatProvider::new("gemini-test")
              .with_api_key("sk-test")
              .with_base_url(format!("{}/v1beta", server.url_str("").trim_end_matches('/')));

          let stream = provider.generate("sys", &[], &[], None).await.unwrap();
          let parts: Vec<_> = stream.collect().await;
          assert_eq!(parts, vec![StreamedMessagePart::text("Hello"), StreamedMessagePart::text(" world")]);
      }

      #[tokio::test]
      async fn converts_api_error_to_status_error() {
          let server = httptest::ServerBuilder::new()
              .bind_addr("127.0.0.1:0".parse().unwrap())
              .run()
              .unwrap();
          server.expect(
              httptest::Expectation::matching(httptest::matchers::request::method_path("POST", "/v1beta/models/gemini-test:generateContent"))
                  .respond_with(httptest::responders::status_code(429).body(r#"{"error":{"message":"rate limit","code":429}}"#)),
          );

          let provider = GoogleGenAIChatProvider::new("gemini-test")
              .with_api_key("sk-test")
              .with_stream(false)
              .with_base_url(format!("{}/v1beta", server.url_str("").trim_end_matches('/')));

          let err = provider.generate("sys", &[], &[], None).await.unwrap_err();
          assert!(format!("{err}").contains("429") || format!("{err}").contains("rate limit"));
      }
  }
  ```

- [ ] 运行测试确认失败(缺少 `with_base_url` 与实现)。

- [ ] 实现 provider 的 HTTP 请求、SSE 解析、错误转换。需要向 `GoogleGenAIChatProvider` 增加 `base_url: Option<String>` 字段,以及 `with_base_url` / `with_stream` 构造器。
  ```rust
  use crate::errors::{APIConnectionError, APITimeoutError, ChatProviderError};
  use crate::request_auth::require_provider_api_key;
  use crate::provider::GenerateOptions;

  const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

  impl GoogleGenAIChatProvider {
      pub fn with_stream(mut self, stream: bool) -> Self { self.stream = stream; self }

      pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
          let url = base_url.into();
          self.base_url = if url.is_empty() { None } else { Some(url) };
          self
      }

      fn base_url(&self) -> &str {
          self.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL)
      }

      fn build_url(&self, model: &str, stream: bool) -> String {
          if self.vertexai {
              let project = self.project.as_deref().unwrap_or("");
              let location = self.location.as_deref().unwrap_or("");
              let action = if stream { "streamGenerateContent" } else { "generateContent" };
              format!(
                  "https://{}-aiplatform.googleapis.com/v1beta1/projects/{}/locations/{}/publishers/google/models/{}:{}",
                  location, project, location, model, action
              )
          } else {
              let action = if stream { "streamGenerateContent" } else { "generateContent" };
              format!("{}/models/{}:{}?key=__KEY__", self.base_url(), model, action)
          }
      }

      fn build_config(&self, system_prompt: &str, tools: &[Tool]) -> serde_json::Value {
          let mut config = self.generation_kwargs.clone();
          config["systemInstruction"] = serde_json::json!({ "parts": [{ "text": system_prompt }] });
          if !tools.is_empty() {
              config["tools"] = serde_json::Value::Array(tools.iter().map(tool_to_google_genai).collect());
          }
          config
      }
  }

  async fn abort_future(signal: Option<&AbortSignal>) -> Result<(), ChatProviderError> {
      match signal {
          None => futures_util::future::pending().await,
          Some(sig) => {
              while !sig.is_aborted() {
                  tokio::task::yield_now().await;
              }
              Err(ChatProviderError::Aborted(crate::errors::AbortError))
          }
      }
  }

  pub(crate) fn convert_google_genai_error(error: reqwest::Error, status: Option<reqwest::StatusCode>, body: Option<String>) -> ChatProviderError {
      if error.is_timeout() {
          return ChatProviderError::Timeout(APITimeoutError);
      }
      if error.is_connect() || error.is_request() {
          return ChatProviderError::Connection(APIConnectionError);
      }
      if let Some(code) = status {
          let msg = body.unwrap_or_else(|| error.to_string());
          return crate::errors::normalize_api_status_error(code.as_u16(), msg, None);
      }
      ChatProviderError::Other(format!("GoogleGenAI error: {error}"))
  }

  #[async_trait::async_trait]
  impl ChatProvider for GoogleGenAIChatProvider {
      // ... name/model_name/get_capability 同 Task 1 ...

      async fn generate(
          &self,
          system_prompt: &str,
          tools: &[Tool],
          history: &[crate::message::Message],
          options: Option<GenerateOptions>,
      ) -> Result<StreamedMessage, ChatProviderError> {
          if let Some(ref opts) = options {
              if let Some(ref sig) = opts.signal {
                  if sig.is_aborted() {
                      return Err(ChatProviderError::Aborted(crate::errors::AbortError));
                  }
              }
          }

          let contents = messages_to_google_genai_contents(history)?;
          let config = self.build_config(system_prompt, tools);
          let body = serde_json::json!({ "contents": contents, "config": config });

          let api_key = if self.vertexai {
              None
          } else {
              let auth_key = options.as_ref().and_then(|o| o.auth.as_ref()).and_then(|a| a.api_key.as_deref());
              Some(require_provider_api_key("google_genai", auth_key, self.api_key.as_deref())?)
          };

          let url = self.build_url(&self.model, self.stream);
          let url = if let Some(key) = api_key {
              url.replace("__KEY__", &key)
          } else {
              url
          };

          let client = reqwest::Client::builder()
              .no_proxy()
              .build()
              .map_err(|e| ChatProviderError::Other(format!("failed to build reqwest client: {e}")))?;

          let request_fut = client.post(&url).json(&body).send();
          let signal_ref = options.as_ref().and_then(|o| o.signal.as_ref());

          let response = match futures_util::future::select(
              std::pin::pin!(request_fut),
              std::pin::pin!(abort_future(signal_ref)),
          ).await {
              futures_util::future::Either::Left((res, _)) => res.map_err(|e| convert_google_genai_error(e, None, None))?,
              futures_util::future::Either::Right((err, _)) => return err,
          };

          if !response.status().is_success() {
              let status = response.status();
              let body = response.text().await.unwrap_or_default();
              return Err(crate::errors::normalize_api_status_error(status.as_u16(), body, None));
          }

          if self.stream {
              let bytes = response.bytes().await.map_err(|e| convert_google_genai_error(e, None, None))?;
              let text = String::from_utf8_lossy(&bytes);
              let mut chunks = Vec::new();
              for line in text.lines() {
                  if let Some(data) = line.strip_prefix("data: ") {
                      if data == "[DONE]" { break; }
                      if let Ok(value) = serde_json::from_str::<serde_json::Value>(data) {
                          chunks.push(value);
                      }
                  }
              }
              Ok(GoogleGenAIStreamedMessage::from_chunks(chunks, options.and_then(|o| o.signal)))
          } else {
              let json: serde_json::Value = response.json().await.map_err(|e| convert_google_genai_error(e, None, None))?;
              Ok(GoogleGenAIStreamedMessage::from_response(json, options.and_then(|o| o.signal)))
          }
      }
  }
  ```
  **说明**:
  - `build_url` 在非 Vertex 模式下用 `__KEY__` 占位,随后替换,以便 `httptest` 路径匹配不被 query 参数干扰。
  - 为通过 `httptest` 路径匹配,`with_base_url` 覆盖基础 URL 后,`build_url` 仍会拼出 `.../v1beta/models/...`。
  - 错误转换优先用 `reqwest::Error::is_timeout` / `is_connect`,HTTP 错误码走 `normalize_api_status_error`。

- [ ] 运行测试:
  ```bash
  cd rust-ody && cargo test -p kosong-rs google_genai::generate_tests
  ```
  预期:2 个测试通过。

- [ ] 运行全 workspace typecheck:
  ```bash
  cd rust-ody && cargo check --workspace
  ```

- [ ] 提交。

---

### Task 7: 实现 `withThinking` 与 `withMaxCompletionTokens`

**Depends on:** Task 1, Task 6

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/google_genai.rs:721-820`
- Test: `rust-ody/crates/kosong-rs/src/providers/google_genai.rs` 内 `#[cfg(test)]`

**实现步骤**:

- [ ] 先写测试:
  ```rust
  #[cfg(test)]
  mod thinking_tests {
      use super::*;
      use crate::provider::ThinkingEffort;

      #[test]
      fn gemini_3_off_maps_to_minimal_without_thoughts() {
          let p = GoogleGenAIChatProvider::new("gemini-3-flash").with_thinking(ThinkingEffort::Off);
          assert_eq!(p.thinking_effort(), Some(ThinkingEffort::Off));
          let cfg = p.generation_kwargs.get("thinkingConfig").unwrap();
          assert_eq!(cfg["thinkingLevel"], "MINIMAL");
          assert_eq!(cfg["includeThoughts"], false);
      }

      #[test]
      fn gemini_3_high_maps_to_high() {
          let p = GoogleGenAIChatProvider::new("gemini-3-flash").with_thinking(ThinkingEffort::High);
          let cfg = p.generation_kwargs.get("thinkingConfig").unwrap();
          assert_eq!(cfg["thinkingLevel"], "HIGH");
          assert_eq!(cfg["includeThoughts"], true);
      }

      #[test]
      fn non_gemini_3_medium_uses_budget() {
          let p = GoogleGenAIChatProvider::new("gemini-2.5-pro").with_thinking(ThinkingEffort::Medium);
          assert_eq!(p.thinking_effort(), Some(ThinkingEffort::Medium));
          let cfg = p.generation_kwargs.get("thinkingConfig").unwrap();
          assert_eq!(cfg["thinkingBudget"], 4096);
          assert_eq!(cfg["includeThoughts"], true);
      }

      #[test]
      fn non_gemini_3_off_uses_zero_budget() {
          let p = GoogleGenAIChatProvider::new("gemini-2.5-pro").with_thinking(ThinkingEffort::Off);
          let cfg = p.generation_kwargs.get("thinkingConfig").unwrap();
          assert_eq!(cfg["thinkingBudget"], 0);
          assert_eq!(cfg["includeThoughts"], false);
      }

      #[test]
      fn max_completion_tokens_propagates() {
          let p = GoogleGenAIChatProvider::new("gemini-2.0-flash")
              .with_max_completion_tokens(1024)
              .unwrap();
          assert_eq!(p.generation_kwargs["maxOutputTokens"], 1024);
      }
  }
  ```

- [ ] 运行确认失败:
  ```bash
  cd rust-ody && cargo test -p kosong-rs google_genai::thinking_tests
  ```

- [ ] 实现 `with_thinking` / `with_max_completion_tokens`。注意 `ChatProvider::with_max_completion_tokens` 默认返回 `Option<Box<dyn ChatProvider>>`,此处需要覆盖:
  ```rust
  #[async_trait::async_trait]
  impl ChatProvider for GoogleGenAIChatProvider {
      // ... 其他方法 ...

      fn thinking_effort(&self) -> Option<ThinkingEffort> {
          let cfg = self.generation_kwargs.get("thinkingConfig")?;
          if let Some(level) = cfg.get("thinkingLevel").and_then(|v| v.as_str()) {
              return Some(match level {
                  "MINIMAL" if cfg.get("includeThoughts").and_then(|v| v.as_bool()) == Some(false) => ThinkingEffort::Off,
                  "MINIMAL" => ThinkingEffort::Low,
                  "LOW" => ThinkingEffort::Low,
                  "MEDIUM" => ThinkingEffort::Medium,
                  "HIGH" => ThinkingEffort::High,
                  _ => return None,
              });
          }
          if let Some(budget) = cfg.get("thinkingBudget").and_then(|v| v.as_i64()) {
              if cfg.get("includeThoughts").and_then(|v| v.as_bool()) == Some(false) {
                  return Some(ThinkingEffort::Off);
              }
              return Some(match budget {
                  0 => ThinkingEffort::Off,
                  1..=1024 => ThinkingEffort::Low,
                  1025..=4096 => ThinkingEffort::Medium,
                  _ => ThinkingEffort::High,
              });
          }
          None
      }

      fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
          let mut clone = self.clone();
          let is_gemini_3 = self.model.to_lowercase().contains("gemini-3");
          let mut cfg = serde_json::json!({ "includeThoughts": true });
          if is_gemini_3 {
              cfg["thinkingLevel"] = match effort {
                  ThinkingEffort::Off => { cfg["includeThoughts"] = serde_json::Value::Bool(false); serde_json::Value::String("MINIMAL".into()) }
                  ThinkingEffort::Low => serde_json::Value::String("LOW".into()),
                  ThinkingEffort::Medium => serde_json::Value::String("MEDIUM".into()),
                  ThinkingEffort::High | ThinkingEffort::Xhigh | ThinkingEffort::Max => serde_json::Value::String("HIGH".into()),
              };
          } else {
              cfg["thinkingBudget"] = match effort {
                  ThinkingEffort::Off => { cfg["includeThoughts"] = serde_json::Value::Bool(false); serde_json::Value::Number(0.into()) }
                  ThinkingEffort::Low => serde_json::Value::Number(1024.into()),
                  ThinkingEffort::Medium => serde_json::Value::Number(4096.into()),
                  ThinkingEffort::High | ThinkingEffort::Xhigh | ThinkingEffort::Max => serde_json::Value::Number(32000.into()),
              };
          }
          if let Some(obj) = clone.generation_kwargs.as_object_mut() {
              obj.insert("thinkingConfig".into(), cfg);
          }
          Box::new(clone)
      }

      fn with_max_completion_tokens(&self, max_tokens: i64) -> Option<Box<dyn ChatProvider>> {
          let mut clone = self.clone();
          clone.generation_kwargs = self.generation_kwargs.clone();
          if let Some(obj) = clone.generation_kwargs.as_object_mut() {
              obj.insert("maxOutputTokens".into(), serde_json::Value::Number(max_tokens.into()));
          }
          Some(Box::new(clone))
      }
  }
  ```

- [ ] 运行测试:
  ```bash
  cd rust-ody && cargo test -p kosong-rs google_genai::thinking_tests
  ```

- [ ] 提交。

---

### Task 8: 添加 L1 fixtures + golden 对照

**Depends on:** Task 4, Task 5, Task 6, Task 7

**Files:**
- Create: `rust-ody/crates/kosong-rs/fixtures/google-genai/text_stream.json`
- Create: `rust-ody/crates/kosong-rs/fixtures/google-genai/tool_call_stream.json`
- Create: `rust-ody/crates/kosong-rs/fixtures/google-genai/non_stream.json`
- Create: `rust-ody/crates/kosong-rs/fixtures/google-genai/vertexai_config.json`
- Modify: `rust-ody/crates/kosong-rs/src/bin/golden.rs` 或新增 `rust-ody/crates/kosong-rs/src/bin/google_genai_golden.rs`
- Modify: `rust-ody/crates/kosong-rs/Cargo.toml`
- Test: `rust-ody/crates/kosong-rs/src/providers/google_genai.rs` 内 fixture 集成测试

**实现步骤**:

- [ ] 创建 fixture 目录与文件:
  ```bash
  mkdir -p rust-ody/crates/kosong-rs/fixtures/google-genai
  ```

- [ ] `text_stream.json`:
  ```json
  {
    "provider": "google_genai",
    "model": "gemini-2.0-flash",
    "systemPrompt": "sys",
    "history": [{"role":"user","content":[{"type":"text","text":"hi"}],"toolCalls":[]}],
    "chunks": [
      {"candidates":[{"content":{"parts":[{"text":"Hello"}]}}]},
      {"candidates":[{"content":{"parts":[{"text":" world"}]},"finishReason":"STOP"}]}
    ],
    "expected": {
      "content": [{"type":"text","text":"Hello world"}],
      "toolCalls": [],
      "finishReason": "completed",
      "rawFinishReason": "STOP"
    }
  }
  ```

- [ ] `tool_call_stream.json`:
  ```json
  {
    "provider": "google_genai",
    "model": "gemini-2.0-flash",
    "systemPrompt": "sys",
    "history": [{"role":"user","content":[{"type":"text","text":"read"}],"toolCalls":[]}],
    "chunks": [
      {"candidates":[{"content":{"parts":[{"functionCall":{"name":"read","args":{"path":"/a"},"id":"abc"}}]}}]},
      {"candidates":[{"content":{"parts":[]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5}}
    ],
    "expected": {
      "content": [],
      "toolCalls": [
        {"type":"function","id":"read_abc","name":"read","arguments":"{\"path\":\"/a\"}","toolCalls":[]}
      ],
      "usage": {"inputOther":10,"output":5,"inputCacheRead":0,"inputCacheCreation":0}
    }
  }
  ```

- [ ] `non_stream.json`:
  ```json
  {
    "provider": "google_genai",
    "model": "gemini-2.0-flash",
    "systemPrompt": "sys",
    "history": [{"role":"user","content":[{"type":"text","text":"hi"}],"toolCalls":[]}],
    "response": {
      "responseId": "r1",
      "candidates":[{"content":{"parts":[{"text":"Hi"}]},"finishReason":"STOP"}],
      "usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":1}
    },
    "expected": {
      "content": [{"type":"text","text":"Hi"}],
      "id": "r1",
      "finishReason": "completed",
      "usage": {"inputOther":3,"output":1,"inputCacheRead":0,"inputCacheCreation":0}
    }
  }
  ```

- [ ] `vertexai_config.json`:
  ```json
  {
    "model": "gemini-2.0-flash",
    "project": "proj",
    "location": "us-central1",
    "expectedUrlSuffix": "projects/proj/locations/us-central1/publishers/google/models/gemini-2.0-flash:streamGenerateContent"
  }
  ```

- [ ] 在 `google_genai.rs` 中增加 fixture 集成测试(不依赖真实网络):
  ```rust
  #[cfg(test)]
  mod fixture_tests {
      use super::*;
      use futures_util::StreamExt;
      use std::fs;

      #[tokio::test]
      async fn text_stream_fixture() {
          let raw = fs::read_to_string("../../fixtures/google-genai/text_stream.json").unwrap();
          let fixture: serde_json::Value = serde_json::from_str(&raw).unwrap();
          let chunks: Vec<serde_json::Value> = serde_json::from_value(fixture["chunks"].clone()).unwrap();
          let msg = GoogleGenAIStreamedMessage::from_chunks(chunks, None);
          let parts: Vec<_> = msg.collect().await;
          let expected: Vec<_> = serde_json::from_value(fixture["expected"]["content"].clone()).unwrap();
          assert_eq!(parts, expected);
      }

      #[tokio::test]
      async fn tool_call_stream_fixture() {
          let raw = fs::read_to_string("../../fixtures/google-genai/tool_call_stream.json").unwrap();
          let fixture: serde_json::Value = serde_json::from_str(&raw).unwrap();
          let chunks: Vec<serde_json::Value> = serde_json::from_value(fixture["chunks"].clone()).unwrap();
          let msg = GoogleGenAIStreamedMessage::from_chunks(chunks, None);
          let parts: Vec<_> = msg.collect().await;
          assert!(matches!(&parts[0], StreamedMessagePart::ToolCall(tc) if tc.name == "read"));
      }

      #[test]
      fn non_stream_fixture() {
          let raw = fs::read_to_string("../../fixtures/google-genai/non_stream.json").unwrap();
          let fixture: serde_json::Value = serde_json::from_str(&raw).unwrap();
          let resp = fixture["response"].clone();
          let msg = GoogleGenAIStreamedMessage::from_response(resp, None);
          assert_eq!(msg.id(), Some("r1".into()));
          assert_eq!(msg.finish_reason(), Some(FinishReason::Completed));
      }

      #[test]
      fn vertexai_url_construction() {
          let raw = fs::read_to_string("../../fixtures/google-genai/vertexai_config.json").unwrap();
          let fixture: serde_json::Value = serde_json::from_str(&raw).unwrap();
          let p = GoogleGenAIChatProvider::new(fixture["model"].as_str().unwrap())
              .with_vertexai(fixture["project"].as_str().unwrap(), fixture["location"].as_str().unwrap());
          let url = p.build_url("gemini-2.0-flash", true);
          assert!(url.ends_with(fixture["expectedUrlSuffix"].as_str().unwrap()));
      }
  }
  ```

- [ ] 运行 fixture 测试:
  ```bash
  cd rust-ody && cargo test -p kosong-rs google_genai::fixture_tests
  ```
  预期:4 个测试通过。

- [ ] 更新 golden binary 以支持 Google GenAI fixture(新增 `bin/google_genai_golden.rs`):
  ```rust
  // rust-ody/crates/kosong-rs/src/bin/google_genai_golden.rs
  use std::env;
  use std::fs;
  use kosong_rs::{generate, GenerateOptions, GoogleGenAIChatProvider, Message};
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
      ).await;
      let output = match result {
          Ok(r) => GoldenResult { assistant_message: Some(serde_json::to_value(&r.message)?), error: None },
          Err(e) => GoldenResult { assistant_message: None, error: Some(format!("{e}")) },
      };
      println!("{}", serde_json::to_string_pretty(&output)?);
      Ok(())
  }
  ```
  并在 `Cargo.toml` 添加:
  ```toml
  [[bin]]
  name = "kosong-google-genai-golden"
  path = "src/bin/google_genai_golden.rs"
  ```

- [ ] 运行 golden binary 编译:
  ```bash
  cd rust-ody && cargo build -p kosong-rs --bin kosong-google-genai-golden
  ```

- [ ] 运行 golden 输出与 TS 侧 fixture 预期比对(手动第一步):
  ```bash
  cd rust-ody && cargo run -p kosong-rs --bin kosong-google-genai-golden -- \
    crates/kosong-rs/fixtures/google-genai/text_stream.json
  ```
  预期输出 JSON 中 `assistant_message.content[0].text == "Hello world"`。

- [ ] 运行全 workspace 类型检查:
  ```bash
  cd rust-ody && cargo check --workspace
  ```

- [ ] 运行 TS 侧 typecheck(确保未破坏 workspace):
  ```bash
  pnpm -r typecheck
  ```

- [ ] 提交。

---

## 4.2.6 验收门 G4-2-6

- `rust-ody/crates/kosong-rs/src/providers/google_genai.rs` 编译通过,`cargo test -p kosong-rs google_genai` 全部通过;
- message → contents 转换与 TS `messagesToGoogleGenAIContents` 逐字段一致(尤其 system message 包装、tool result 排序、media URL 拆分);
- finish reason 映射与 TS `normalizeGoogleGenAIFinishReason` 一致;
- response chunk 解析(tool-call id `{name}_{id}`、thought parts、usage token 拆分)与 TS 一致;
- `withThinking` 对 gemini-3(`thinking_level`) 与非 gemini-3(`thinking_budget`) 分支与 TS 一致;
- L1 fixture(`text_stream`/`tool_call_stream`/`non_stream`/`vertexai_config`)全部绿;
- `cargo check --workspace` 与 `pnpm -r typecheck` 全绿。

**No-Go 信号**:若 function_response 排序或 tool-call id 构造无法与 TS 逐值对齐,登记 `parity/known-gaps.md`,本 provider 保留 TS 回调,不阻塞 4.2.7 对其他 provider 的集成。

---

## Self-Review

- [ ] 1. **Spec-coverage table**:
  | 原 4.2.6 规格 | 覆盖任务 | 状态 |
  |---|---|---|
  | `GoogleGenAIChatProvider` 构造与 capability | Task 1 | covered |
  | `contents`/`config` 构造、system_instruction、tools | Task 2 | covered |
  | response 解析(text/thought/function_call) | Task 4 | covered |
  | function_response 排序 | Task 2 | covered |
  | finishReason 映射 | Task 3 | covered |
  | AbortSignal 兼容 | Task 5, Task 6 | covered |
  | thinking 配置(gemini-3 level / 其他 budget) | Task 7 | covered |
  | Vertex AI 分支 | Task 6, Task 8 | covered |
  | L1 fixture + golden 对照 | Task 8 | covered |
- [ ] 2. **Placeholder scan**:无 TODO/TBD;Task 1 的 `unimplemented!("Task 6")` 在 Task 6 被真实实现替换;所有代码片段均为可执行内容。
- [ ] 3. **No phantom tasks**:每个任务都有文件修改、测试、提交;无 `--allow-empty`。
- [ ] 4. **Dependency soundness**:Task 2/3/4/5/6/7/8 的依赖均由更早任务满足;无反向依赖。
- [ ] 5. **Caller & build soundness**:本阶段仅在 `kosong-rs` 内部新增模块,未修改 `ChatProvider` trait 签名;新增 `with_max_completion_tokens` 覆盖不破坏既有调用方;每任务结束运行 `cargo check -p kosong-rs`,Task 6 与 Task 8 结束运行 `cargo check --workspace`;未修改 TS 共享签名,但 Task 8 仍跑 `pnpm -r typecheck` 确保 workspace 类型一致。
- [ ] 6. **Test-the-risk**:转换与解析是状态变化核心,每个任务都有行为断言;fixture 中 "Hello world" 与 tool-call id `read_abc` 等预期值均直接来自 TS 实现常量,可逐行追溯。
- [ ] 7. **Type consistency**:Task 1 定义的字段名(`generation_kwargs`、`vertexai`、`stream`)被 Task 6/7 复用;`ToolCall`/`StreamedMessagePart`/`TokenUsage`/`FinishReason`/`ThinkingEffort` 均来自 4.2.0 已定义类型。
<!-- e2e-enriched -->

### Task 9: Generate and run E2E tests

Based on the changed files, validate the following areas:
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

