# Part 2 — ody-host config/routing

本部分把 `kosong-rs` 的 provider factory 接入 `ody-host`：扩展 `HostConfig` 以解析 `provider_id`，扩展会话状态以持久化 per-session provider，新增 `ChatProvider` → `LlmProvider` 的适配器，并让 `CoreHost` 的 `set_model`/`getConfig`/`getOdyConfig` 返回真实的 provider 与 capability 信息。

---

### Task 4: 扩展 `HostConfig`/`ProviderConfig` 解析 `provider_id`

**Depends on:** Part 1 Task 3（`kosong-rs::create_chat_provider` / `resolve_model_capability` 已可用）

**Files:**
- Modify: `rust-ody/crates/ody-host/src/config.rs`

实现步骤：

- [ ] 先写失败测试，断言 CLI 能解析 `--provider` 且配置文件能解析 `[provider].provider_id`：

```rust
#[cfg(test)]
mod provider_config_tests {
    use super::*;

    #[test]
    fn cli_provider_overrides_default() {
        let args = vec!["ody-host", "--provider", "anthropic", "--stdio"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert_eq!(config.provider.provider_id, "anthropic");
    }

    #[test]
    fn config_file_provider_is_parsed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ody.toml");
        std::fs::write(&path, "[provider]\nprovider_id = \"kimi\"\napi_key = \"sk\"\n").unwrap();
        let args = vec!["ody-host", "--config", path.to_str().unwrap(), "--stdio"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert_eq!(config.provider.provider_id, "kimi");
        assert_eq!(config.provider.api_key, "sk");
    }

    #[test]
    fn default_provider_is_openai() {
        let args = vec!["ody-host", "--stdio"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert_eq!(config.provider.provider_id, "openai");
    }
}
```

- [ ] 运行测试并确认失败（字段/参数未定义）：

```bash
cd rust-ody && cargo test -p ody-host config::provider_config_tests --lib
```

- [ ] 修改 `config.rs`：

在 `SharedArgs` 中新增 CLI 参数：

```rust
#[arg(long)]
provider: Option<String>,
```

在 `RawProvider` 中新增字段：

```rust
#[derive(Debug, Deserialize)]
struct RawProvider {
    provider_id: Option<String>,
    api_key: String,
    base_url: Option<String>,
    default_model: Option<String>,
}
```

修改 `HostConfig::from_cli` 中构造 `ProviderConfig` 的代码（当前 `provider_id: "openai".to_string()`），改为：

```rust
let provider = ProviderConfig {
    provider_id: active
        .provider
        .clone()
        .or_else(|| file.provider.as_ref().and_then(|p| p.provider_id.clone()))
        .unwrap_or_else(|| "openai".to_string()),
    api_key: file.provider.as_ref().map(|p| p.api_key.clone()).unwrap_or_default(),
    base_url: file.provider.as_ref().and_then(|p| p.base_url.clone()),
    default_model: Some(
        file.provider
            .as_ref()
            .and_then(|p| p.default_model.clone())
            .unwrap_or_else(|| "gpt-4o-mini".to_string()),
    ),
};
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p ody-host config::provider_config_tests --lib
```

预期：3 passed。

- [ ] Commit：`feat(ody-host): parse provider_id from CLI and config file`。

---

### Task 5: 扩展 `SessionState` 持久化 `provider_id`

**Depends on:** Task 4

**Files:**
- Modify: `rust-ody/crates/ody-host/src/session/store.rs`
- Modify: `rust-ody/crates/ody-host/src/session/manager.rs`
- Modify: `rust-ody/crates/ody-host/src/host.rs` 中所有读取/设置 provider_id 的位置

实现步骤：

- [ ] 在 `session/store.rs` 的 `SessionState` 新增字段：

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub provider_id: Option<String>,
```

- [ ] 在 `session/manager.rs` 的 `Session` 上新增方法：

```rust
pub async fn provider_id(&self) -> Option<String> {
    self.state.lock().await.provider_id.clone()
}

pub async fn set_provider_id(&self, provider_id: Option<String>) {
    self.state.lock().await.provider_id = provider_id;
}
```

并在 `create_with_id` 初始化 `SessionState` 时设置 `provider_id: None`。

- [ ] 在 `session/store.rs` 的 `state_json_roundtrip` 测试中增加 `provider_id` 断言：

```rust
#[test]
fn state_json_roundtrip_with_provider_id() {
    let dir = tempfile::tempdir().unwrap();
    let state = SessionState {
        title: None,
        last_prompt: None,
        custom: HashMap::new(),
        model: Some("gpt-4o".into()),
        thinking: None,
        permission: None,
        provider_id: Some("openai".into()),
    };
    write_state_json(dir.path(), &state).unwrap();
    let restored = read_state_json(dir.path()).unwrap().unwrap();
    assert_eq!(restored.provider_id, Some("openai".into()));
}
```

- [ ] 运行 `ody-host` 单元测试，确认新增测试通过且既有测试未因字段新增而失败（`SessionState` 新增 `Option<String>` 字段对现有构造是向后兼容的，编译会提示需补字段）：

```bash
cd rust-ody && cargo test -p ody-host session --lib
```

预期：所有 session 测试通过。

- [ ] Commit：`feat(ody-host): persist provider_id in SessionState`。

---

### Task 6: 实现 `ChatProviderLlmAdapter`

**Depends on:** Task 5（会话可存 provider_id，但适配器本身只依赖 kosong factory）

**Files:**
- Create: `rust-ody/crates/ody-host/src/llm/chat_provider_adapter.rs`
- Modify: `rust-ody/crates/ody-host/src/llm/mod.rs`

实现步骤：

- [ ] 在 `llm/chat_provider_adapter.rs` 写入失败测试，断言 mock ChatProvider 的 text 与 tool_call 能被适配成 `LlmProvider` 的 `ChatDelta`：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{ChatRequest, FinishReason, LlmProvider, Message, Role};
    use kosong_rs::{
        FinishReason as KosongFinishReason, MockProvider, StreamedMessagePart, Tool,
    };

    #[tokio::test]
    async fn adapter_forwards_text_and_tool_call() {
        let chat = MockProvider::new("mock", "m1").with_parts(vec![
            StreamedMessagePart::text("hello"),
            StreamedMessagePart::tool_call_part("read", "read_1", Some(r#"{"path":"/tmp"}"#)),
        ]);
        let provider = ChatProviderLlmAdapter::new(Box::new(chat));
        let request = ChatRequest {
            model: "m1".into(),
            messages: vec![Message { role: Role::User, content: "hi".into() }],
            tools: vec![],
            stream: true,
        };
        let mut deltas = Vec::new();
        let reason = provider
            .chat_stream(request, &mut |d| deltas.push(d.clone()))
            .await
            .unwrap();
        assert_eq!(deltas.len(), 2);
        assert_eq!(deltas[0].content.as_deref(), Some("hello"));
        assert_eq!(deltas[1].tool_call.as_ref().unwrap().name, "read");
        assert!(matches!(reason, FinishReason::Stop));
    }
}
```

> 若 `StreamedMessagePart::tool_call_part` 构造函数不存在，改为使用 `StreamedMessagePart::ToolCall { tool_call: ToolCall { ... } }`；先 `grep -n "pub fn tool_call_part" rust-ody/crates/kosong-rs/src/message.rs` 确认。

- [ ] 运行测试并确认失败：

```bash
cd rust-ody && cargo test -p ody-host llm::chat_provider_adapter --lib
```

- [ ] 写入适配器实现：

```rust
use kosong_rs::{
    AbortSignal, ChatProvider, ContentPart, GenerateOptions, Message as KosongMessage,
    Role as KosongRole, StreamedMessagePart, Tool as KosongTool,
};

use crate::llm::{ChatDelta, ChatRequest, FinishReason, LlmError, LlmProvider, ToolCallDelta};

pub struct ChatProviderLlmAdapter {
    inner: Box<dyn ChatProvider>,
}

impl ChatProviderLlmAdapter {
    pub fn new(inner: Box<dyn ChatProvider>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl LlmProvider for ChatProviderLlmAdapter {
    async fn chat_stream(
        &self,
        request: ChatRequest,
        on_delta: &mut (dyn FnMut(ChatDelta) + Send),
    ) -> Result<FinishReason, LlmError> {
        let system_prompt = ""; // ody-host 当前不在 ChatRequest 中携带 system prompt
        let tools: Vec<KosongTool> = request
            .tools
            .into_iter()
            .map(|t| KosongTool {
                name: t.name,
                description: t.description,
                parameters: t.parameters,
            })
            .collect();
        let history: Vec<KosongMessage> = request
            .messages
            .into_iter()
            .map(|m| KosongMessage {
                role: match m.role {
                    crate::llm::Role::System => KosongRole::System,
                    crate::llm::Role::User => KosongRole::User,
                    crate::llm::Role::Assistant => KosongRole::Assistant,
                },
                content: vec![ContentPart::Text { text: m.content }],
            })
            .collect();

        let stream = self
            .inner
            .generate(system_prompt, &tools, &history, Some(GenerateOptions::default()))
            .await
            .map_err(|e| LlmError::RequestFailed {
                source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
            })?;

        use futures_util::StreamExt;
        let mut parts = Vec::new();
        let mut stream = Box::pin(stream);
        while let Some(part) = stream.next().await {
            parts.push(part.clone());
            match part {
                StreamedMessagePart::Text { text } => {
                    on_delta(ChatDelta {
                        index: 0,
                        content: Some(text),
                        tool_call: None,
                    });
                }
                StreamedMessagePart::ToolCall(tool_call) => {
                    on_delta(ChatDelta {
                        index: 0,
                        content: None,
                        tool_call: Some(ToolCallDelta {
                            id: tool_call.id.unwrap_or_default(),
                            name: tool_call.name,
                            arguments: tool_call.arguments.unwrap_or(serde_json::Value::Null),
                        }),
                    });
                }
                _ => {}
            }
        }

        // finish_reason 取自最后一个非空 part，若流为空则默认 Stop
        let finish_reason = parts
            .iter()
            .rev()
            .find_map(|p| match p {
                StreamedMessagePart::Text { .. } => Some(FinishReason::Stop),
                StreamedMessagePart::ToolCall(_) => Some(FinishReason::ToolCalls),
                _ => None,
            })
            .unwrap_or(FinishReason::Stop);
        Ok(finish_reason)
    }
}
```

> 说明：`LlmError::RequestFailed` 当前接收 `reqwest::Error`；若类型不匹配，改为在 `llm/mod.rs` 新增 `LlmError::Provider { message: String }` 变体并在同 task 中更新 `OpenAiProvider` / `MockProvider` 的 match 分支（见下方共享签名处理）。

- [ ] 在 `llm/mod.rs` 新增 `pub mod chat_provider_adapter;`。

- [ ] 若新增 `LlmError::Provider` 变体，搜索并更新所有 match `LlmError` 的位置：

```bash
cd rust-ody && grep -rn "LlmError::" crates/ody-host/src/
```

必须更新的文件：
- `crates/ody-host/src/llm/openai.rs`：把 `map_err(|e| LlmError::RequestFailed { source: e })` 保持原样（若未改签名则不变）；若新增 `Provider` 变体则按需加入 match。
- `crates/ody-host/src/host.rs`：所有 `.map_err(|e| e.to_string())` 继续工作，因为 `Display` 已实现。

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p ody-host llm::chat_provider_adapter --lib
```

预期：1 passed。

- [ ] Commit：`feat(ody-host): add ChatProviderLlmAdapter`。

---

### Task 7: 实现 `ody-host` 侧 provider factory 封装

**Depends on:** Task 6

**Files:**
- Create: `rust-ody/crates/ody-host/src/provider_factory.rs`

实现步骤：

- [ ] 写入测试，断言根据 `ProviderConfig` 能构造出对应 provider：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_openai_provider() {
        let config = ProviderConfig {
            provider_id: "openai".into(),
            api_key: "sk".into(),
            base_url: Some("https://example.com/v1".into()),
            default_model: Some("gpt-4o-mini".into()),
        };
        let provider = create_host_provider(&config).unwrap();
        assert_eq!(provider.name(), "openai");
        assert_eq!(provider.model_name(), "gpt-4o-mini");
    }

    #[test]
    fn rejects_unknown_provider() {
        let config = ProviderConfig {
            provider_id: "weird".into(),
            api_key: "x".into(),
            base_url: None,
            default_model: Some("m".into()),
        };
        let err = create_host_provider(&config).unwrap_err();
        assert!(err.to_string().contains("weird"));
    }
}
```

- [ ] 写入实现：

```rust
use kosong_rs::{create_chat_provider, ProviderFactoryConfig};

use crate::config::ProviderConfig;
use crate::error::HostError;

pub fn create_host_provider(config: &ProviderConfig) -> Result<Box<dyn kosong_rs::ChatProvider>, HostError> {
    create_chat_provider(ProviderFactoryConfig {
        provider_id: config.provider_id.clone(),
        model: config.default_model.clone().unwrap_or_else(|| "gpt-4o-mini".into()),
        api_key: Some(config.api_key.clone()).filter(|k| !k.is_empty()),
        base_url: config.base_url.clone(),
        default_headers: None,
    })
    .map_err(|e| HostError::config_invalid(e.to_string()))
}
```

- [ ] 在 `ody-host/src/lib.rs`（若存在）或 `main.rs` 中按需 `pub mod provider_factory;`；`ody-host` 当前没有 `lib.rs`，入口是 `main.rs`，因此直接由 `main.rs` 引用 `crate::provider_factory::create_host_provider`。

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p ody-host provider_factory --lib
```

预期：2 passed。

- [ ] Commit：`feat(ody-host): add host-side provider factory wrapper`。

---

### Task 8: `CoreHost` `set_model` 路由与 `getConfig`/`getOdyConfig` provider 信息

**Depends on:** Task 5、Task 7

**Files:**
- Modify: `rust-ody/crates/ody-host/src/host.rs`

实现步骤：

- [ ] 先写失败测试，覆盖 `set_model` 的 provider/model 拆分与 `getConfig` 返回字段：

```rust
#[cfg(test)]
mod provider_routing_tests {
    use super::*;
    use crate::config::{HostConfig, LogLevel, ProviderConfig, TransportMode};
    use crate::events::AgentEvent;
    use crate::llm::{ChatDelta, LlmProvider};
    use std::sync::{Arc, Mutex};

    struct EchoProvider;
    #[async_trait::async_trait]
    impl LlmProvider for EchoProvider {
        async fn chat_stream(
            &self,
            _request: crate::llm::ChatRequest,
            on_delta: &mut (dyn FnMut(ChatDelta) + Send),
        ) -> Result<crate::llm::FinishReason, crate::llm::LlmError> {
            on_delta(ChatDelta { index: 0, content: Some("ok".into()), tool_call: None });
            Ok(crate::llm::FinishReason::Stop)
        }
    }

    fn make_host_with_config(config: HostConfig) -> CoreHost {
        CoreHost::new(config, Box::new(crate::host::tests::MockSink(Arc::new(Mutex::new(Vec::new())))), Box::new(EchoProvider)).unwrap()
    }

    #[tokio::test]
    async fn set_model_with_provider_prefix_updates_both() {
        let config = HostConfig {
            home_dir: tempfile::tempdir().unwrap().path().to_path_buf(),
            config_path: None,
            transport: TransportMode::Stdio,
            log_level: LogLevel::Info,
            provider: ProviderConfig {
                provider_id: "openai".into(),
                api_key: "".into(),
                base_url: None,
                default_model: Some("gpt-4o-mini".into()),
            },
            mock_provider: false,
        };
        let host = make_host_with_config(config);
        let work_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
        let session = host.dispatch("createSession", serde_json::json!({"workDir": work_dir})).await.unwrap();

        let result = host
            .dispatch("setModel", serde_json::json!({"sessionId": session["id"], "agentId": "main", "model": "anthropic/claude-sonnet-4"}))
            .await
            .unwrap();
        assert_eq!(result["model"], "claude-sonnet-4");
        assert_eq!(result["providerName"], "anthropic");

        let cfg = host
            .dispatch("getConfig", serde_json::json!({"sessionId": session["id"], "agentId": "main"}))
            .await
            .unwrap();
        assert_eq!(cfg["modelAlias"], "claude-sonnet-4");
        assert_eq!(cfg["provider"]["id"], "anthropic");
    }
}
```

> 注意：测试需要复用 `MockSink`；`MockSink` 当前定义在 `host.rs` 的 `#[cfg(test)] mod tests` 中，若无法跨模块访问，则在 `provider_routing_tests` 内重新定义一个最小 `MockSink`。

- [ ] 运行测试并确认失败：

```bash
cd rust-ody && cargo test -p ody-host host::provider_routing_tests --lib
```

- [ ] 实现 `set_model` 拆分逻辑。在 `host.rs` 的 `set_model` 方法中替换当前仅解析 `model` 的代码：

```rust
async fn set_model(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
    let (session_id, _agent_id) = self.require_session_agent(&payload)?;
    let raw = payload
        .get("model")
        .and_then(|v| v.as_str())
        .ok_or("missing model")?
        .to_string();

    let (provider_id, model) = parse_model_alias(&raw);
    let resolved_provider_id = provider_id.unwrap_or_else(|| self.config.provider.provider_id.clone());

    let session = self.session_manager.get(session_id).await.map_err(|e| e.to_string())?;
    session.set_model(Some(model.clone())).await;
    session.set_provider_id(Some(resolved_provider_id.clone())).await;
    session.persist_state().await.map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "model": model,
        "providerName": resolved_provider_id,
    }))
}
```

新增 helper：

```rust
fn parse_model_alias(raw: &str) -> (Option<String>, String) {
    if let Some(idx) = raw.find('/') {
        return (Some(raw[..idx].to_string()), raw[idx + 1..].to_string());
    }
    if let Some(idx) = raw.find(':') {
        return (Some(raw[..idx].to_string()), raw[idx + 1..].to_string());
    }
    (None, raw.to_string())
}
```

- [ ] 更新 `get_agent_config`：

```rust
async fn get_agent_config(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
    let (session_id, _agent_id) = self.require_session_agent(&payload)?;
    let session = self.session_manager.get(session_id).await.map_err(|e| e.to_string())?;
    let model = session.model().await;
    let thinking = session.thinking().await.unwrap_or_else(|| "off".to_string());
    let default_model = self.config.provider.default_model.clone().unwrap_or_else(|| "gpt-4o-mini".to_string());
    let model_alias = model.clone().unwrap_or_else(|| default_model.clone());
    let provider_id = session
        .provider_id()
        .await
        .unwrap_or_else(|| self.config.provider.provider_id.clone());

    let capability = kosong_rs::resolve_model_capability(&provider_id, &model_alias)
        .unwrap_or_else(kosong_rs::ModelCapability::unknown);

    Ok(serde_json::json!({
        "cwd": session.work_dir,
        "provider": {
            "id": provider_id,
            "model": model_alias.clone(),
        },
        "modelAlias": model_alias,
        "modelCapabilities": {
            "image_in": capability.image_in,
            "video_in": capability.video_in,
            "audio_in": capability.audio_in,
            "thinking": capability.thinking,
            "tool_use": capability.tool_use,
            "max_context_tokens": capability.max_context_tokens,
            "max_output_tokens": capability.max_output_tokens,
        },
        "thinkingLevel": thinking,
        "systemPrompt": "",
    }))
}
```

- [ ] 更新 `get_ody_config`：

```rust
fn get_ody_config(&self) -> serde_json::Value {
    serde_json::json!({
        "providers": [{
            "id": self.config.provider.provider_id,
            "apiKey": self.config.provider.api_key,
            "baseUrl": self.config.provider.base_url,
            "defaultModel": self.config.provider.default_model,
        }],
        "homeDir": self.config.home_dir,
    })
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p ody-host host::provider_routing_tests --lib
```

预期：1 passed。

- [ ] Commit：`feat(ody-host): route setModel by provider prefix and return provider info in getConfig`。

---

### Task 9: `main.rs` 接入工厂并做全树 typecheck

**Depends on:** Task 7、Task 8

**Files:**
- Modify: `rust-ody/crates/ody-host/src/main.rs`

实现步骤：

- [ ] 修改 `main.rs`，用 `create_host_provider` 与 `ChatProviderLlmAdapter` 替换硬编码的 `OpenAiProvider`：

```rust
use ody_host::config::{HostConfig, LogLevel};
use ody_host::error::HostError;
use ody_host::host::CoreHost;
use ody_host::llm::chat_provider_adapter::ChatProviderLlmAdapter;
use ody_host::llm::mock::MockProvider;
use ody_host::provider_factory::create_host_provider;
use ody_host::transport::{build_transport, RpcRouter};

// ...

let provider: Box<dyn ody_host::llm::LlmProvider> = if config.mock_provider {
    Box::new(MockProvider::new())
} else {
    let chat_provider = create_host_provider(&config.provider).map_err(|e| e.to_string())?;
    Box::new(ChatProviderLlmAdapter::new(chat_provider))
};
```

- [ ] 运行 `ody-host` 完整单元测试：

```bash
cd rust-ody && cargo test -p ody-host
```

预期：全部通过。

- [ ] 全 Rust workspace typecheck：

```bash
cd rust-ody && cargo check --workspace
```

预期：无编译错误。

- [ ] Commit：`feat(ody-host): wire provider factory into main.rs`。

---

## Part 2 Self-Review

- [ ] 1. Spec-coverage: 4.2.7.2（CoreHost provider 可切换）与 4.2.7.3（getConfig/getOdyConfig provider 信息）已覆盖。
- [ ] 2. Placeholder scan: 无 TODO/TBD；每个函数均给出完整实现。
- [ ] 3. No phantom tasks: 每个 task 均产生可验证变更（代码 + 测试 + commit）。
- [ ] 4. Dependency soundness: Task 5 依赖 Task 4；Task 6 依赖 kosong factory（Part 1）；Task 7 依赖 Task 6；Task 8 依赖 Task 5/7；Task 9 依赖 Task 7/8。无向后依赖。
- [ ] 5. Caller & build soundness: Task 6 若新增 `LlmError` 变体，同 task 内搜索并更新所有 `LlmError` match；Task 9 以 `cargo check --workspace` 收尾。`SessionState` 字段变更在 Task 5 同 task 内更新所有构造位置（store/manager tests）。
- [ ] 6. Test-the-risk: `set_model` 的状态变更（session model + provider_id）由 `provider_routing_tests` 行为断言；`HostConfig` 解析由 `provider_config_tests` 覆盖；`SessionState` 持久化由 `state_json_roundtrip_with_provider_id` 覆盖。
- [ ] 7. Type consistency: `get_agent_config` 返回 JSON 字段 `provider.id`/`model`/`modelAlias`/`modelCapabilities`/`thinkingLevel` 与 TS `AgentConfigData` 字段名一致；`get_ody_config` 返回的 `providers[0].id/apiKey/baseUrl/defaultModel` 与 TS `OdyConfig` provider 序列化一致。
