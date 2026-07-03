# Part 2: ody-host CoreHost 集成

**Goal:** 把 `agent-rs` 中已落地的 `Agent` 接入 `ody-host`：每个 session 持有一个 `Agent`，RPC 路由到 `Agent`/`TurnFlow`，事件经桥接后从 `EventSink` 发出，session 持久化扩展为支持 Rust 侧 resume。

**Architecture:** `CoreHost` 保留 `SessionManager`，但 `Session` 延迟构造并缓存 `Arc<Agent>`。新增 `HostAgentEnvironment` 实现 `agent_rs::agent::AgentEnvironment`，把 `agent-rs` 的 `AgentEvent` 映射为 `ody-host` 的 `AgentEvent`（补全 `sessionId`/`agentId`）。新增 `HostProviderResolver` 与 `HostLlmFactory` 把 host 配置和 `kosong_rs` provider 工厂桥接到 `AgentBuilder`。`setModel`/`setThinking`/`setPermission`/`enterPlan`/`exitPlan` 等 RPC 直接调用 `Agent` 的控制方法；`prompt`/`steer`/`cancel` 调用 `TurnFlow`。

**Tech Stack:** Rust（tokio / async-trait / serde_json / anyhow）。

> For executing workers: implement this plan task-by-task. Steps use - [ ] checkboxes for tracking.

---

## 任务总览与依赖

```
Task 1 — Cargo 依赖 + agent-rs ConfigState.provider 透传 apiKey/baseUrl
Task 2 — HostProviderResolver / HostLlmFactory / HostAgentEnvironment / 事件映射
Task 3 — SessionState 扩展 + Session 持 Agent + Agent 控制方法补全
Task 4 — CoreHost dispatch 路由改写 + 测试更新
Task 5 — 全树 typecheck + 既有测试回归
```

- Task 2 依赖 Task 1（需要 `agent-rs` 已作为依赖且 `ConfigState` 能正确创建带凭证的 provider）。
- Task 3 依赖 Task 2（需要 `HostAgentEnvironment` 与 resolver/factory 来构造 `Agent`）。
- Task 4 依赖 Task 3（需要 `Session::agent()` 与 `Agent` 控制方法）。
- Task 5 依赖 Task 4（最终验证）。

---

## Task 1: 添加 agent-rs 依赖并修复 ConfigState.provider 凭证透传

**Depends on:** Part 1 (`agent-rs.md` 已完成）

**Files:**
- Modify: `rust-ody/crates/ody-host/Cargo.toml:12-28`
- Modify: `rust-ody/crates/agent-rs/src/config/state.rs:131-140`
- Modify: `rust-ody/crates/agent-rs/src/config/state.rs:185`（`try_resolved_provider_config` 返回的 `ResolvedRuntimeProvider` 已包含凭证，无需改签名）
- Test: `rust-ody/crates/agent-rs/src/config/state.rs` 末尾新增 `#[cfg(test)]` 模块

**为什么需要这步**：`ody-host` 的 provider 配置包含 `api_key` / `base_url`，但 Part 1 的 `ConfigState.provider()` 调用 `kosong_rs::create_chat_provider` 时把这两项传成了 `None`，会导致真实 provider 认证失败。

### 步骤

- [ ] **运行并确认当前行为**（为测试建立基线）：

```bash
cd rust-ody && cargo test -p agent-rs --lib config::state
```

当前没有 `config::state` 测试模块，命令会报 `no test target matches`，这是正常的。

- [ ] **添加失败测试**：在 `rust-ody/crates/agent-rs/src/config/state.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::nested::AgentConfigUpdateData;

    struct Ctx(AgentConfigUpdateData);
    impl AgentConfigContext for Ctx {
        fn log_record(&mut self, _r: crate::records::AgentRecord) {}
        fn emit_status_updated(&self) {}
        fn initialize_builtin_tools(&self) {}
        fn get_cwd(&self) -> String { "/".into() }
        fn chdir(&self, _cwd: &str) {}
        fn default_model(&self) -> Option<String> { None }
        fn resolve_provider_config(&self, alias: &str) -> Option<ResolvedRuntimeProvider> {
            Some(ResolvedRuntimeProvider {
                provider_name: "openai".into(),
                provider: super::super::types::ProviderConfig {
                    r#type: kosong_rs::provider::ProviderType::OpenAi,
                    model: alias.into(),
                    api_key: Some("sk-test".into()),
                    base_url: Some("https://example.com/v1".into()),
                    default_headers: None,
                },
                model_capabilities: kosong_rs::provider::ModelCapability::unknown(),
            })
        }
        fn thinking_config(&self) -> Option<ThinkingConfig> { None }
        fn push_config_updated_replay(&self, _c: &AgentConfigUpdateData) {}
    }

    #[test]
    fn provider_uses_resolved_credentials() {
        let mut state = ConfigState::new(Ctx(AgentConfigUpdateData::default()));
        state.update(AgentConfigUpdateData {
            model_alias: Some("gpt-4o-mini".into()),
            ..Default::default()
        });
        let provider = state.provider();
        assert_eq!(provider.name(), "openai");
        // Kosong ChatProvider 不暴露 api_key/base_url，但通过请求 observable 无法在此单元测试验证；
        // 这里至少保证 provider 构造成功且 model 正确。
        assert_eq!(provider.model_name(), "gpt-4o-mini");
    }
}
```

- [ ] **运行并确认失败**：

```bash
cd rust-ody && cargo test -p agent-rs --lib config::state::tests::provider_uses_resolved_credentials
```

预期：测试可能编译失败，因为 `ConfigState::provider()` 目前未使用 `api_key` / `base_url`，但测试代码本身需要它们存在；如果已经能编译但 provider 构造使用了 `None`，则断言仍可能通过。关键是下一步修改后仍然通过，并且手动检查传参。

- [ ] **修改 `ConfigState.provider()`**：把 `rust-ody/crates/agent-rs/src/config/state.rs:131-140` 从：

```rust
    pub fn provider(&self) -> Box<dyn ChatProvider> {
        kosong_rs::create_chat_provider(kosong_rs::ProviderFactoryConfig {
            provider_id: self.resolved_provider_config().provider_name,
            model: self.model(),
            api_key: None,
            base_url: None,
            default_headers: None,
        })
        .expect("provider resolution already succeeded")
    }
```

改为：

```rust
    pub fn provider(&self) -> Box<dyn ChatProvider> {
        let resolved = self.resolved_provider_config();
        let provider_config = &resolved.provider;
        kosong_rs::create_chat_provider(kosong_rs::ProviderFactoryConfig {
            provider_id: resolved.provider_name,
            model: self.model(),
            api_key: provider_config.api_key.clone().filter(|k| !k.is_empty()),
            base_url: provider_config.base_url.clone(),
            default_headers: provider_config.default_headers.clone(),
        })
        .expect("provider resolution already succeeded")
    }
```

- [ ] **运行并确认通过**：

```bash
cd rust-ody && cargo test -p agent-rs --lib config::state::tests::provider_uses_resolved_credentials
```

- [ ] **修改 `ody-host/Cargo.toml` 添加依赖**：在 `[dependencies]` 段加入：

```toml
agent-rs = { path = "../agent-rs" }
```

位置在 `kaos-rs = { path = "../kaos-rs" }` 之后即可。

- [ ] **编译验证**：

```bash
cd rust-ody && cargo check -p ody-host
```

此时应只报 `agent-rs` 相关符号未使用，不应有编译错误。

- [ ] **提交**：

```bash
git add rust-ody/crates/ody-host/Cargo.toml rust-ody/crates/agent-rs/src/config/state.rs
git commit -m "chore(ody-host): add agent-rs dependency and forward provider credentials"
```

---

## Task 2: HostProviderResolver / HostLlmFactory / HostAgentEnvironment / 事件映射

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/ody-host/src/agent_bridge.rs`
- Modify: `rust-ody/crates/ody-host/src/lib.rs:1-10`（导出 `pub mod agent_bridge;`）
- Modify: `rust-ody/crates/ody-host/src/events.rs:1-157`（扩展 `AgentEvent` 枚举以覆盖 background/cron/tool-call 事件；字段名保持 TS 兼容）
- Modify: `rust-ody/crates/agent-rs/src/agent.rs:???`（本 Task 不修改，只使用 Part 1 已定义的 API）

**核心设计**：
- `HostProviderResolver` 把 host 的 `ProviderConfig` 和 model alias 解析为 `agent_rs::config::state::ResolvedRuntimeProvider`。
- `HostLlmFactory` 实现 `agent_rs::agent::LlmFactory`，直接用传入的 `Box<dyn ChatProvider>` 构造 `agent_rs::turn::kosong_llm::KosongLLM`。
- `HostAgentEnvironment` 实现 `agent_rs::agent::AgentEnvironment`，把 approval / hook / telemetry / log 通过 `EventSink` 转发；`emit_event` 时把无 session/agent 的 `agent_rs::turn::types::AgentEvent` 包装为带 `sessionId`/`agentId` 的 `ody_host::events::AgentEvent`。

### 步骤

- [ ] **写出失败测试**：在 `rust-ody/crates/ody-host/src/agent_bridge.rs` 中先写测试骨架（文件尚不存在，测试会随实现一起编译）。测试要点：
  1. `HostProviderResolver` 能解析 `"openai/gpt-4o-mini"`。
  2. `HostAgentEnvironment::emit_event` 把 `agent_rs::turn::types::AgentEvent::TurnStarted` 映射为 `ody_host::events::AgentEvent::TurnStarted` 并带有正确的 `session_id` / `agent_id`。

具体测试代码放在实现后的 `#[cfg(test)]` 模块中：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;
    use crate::events::{AgentEvent as HostEvent, EventSink, PromptOrigin as HostOrigin};
    use crate::error::RpcError;
    use std::sync::{Arc, Mutex};

    struct CollectSink(Arc<Mutex<Vec<HostEvent>>>);

    #[async_trait::async_trait]
    impl EventSink for CollectSink {
        async fn request(&self, _method: &str, _payload: Vec<u8>) -> Result<Vec<u8>, RpcError> {
            Ok(vec![])
        }
        fn emit(&self, event: HostEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn resolver_splits_model_alias() {
        let resolver = HostProviderResolver::new(ProviderConfig {
            provider_id: "kimi".into(),
            api_key: "ak".into(),
            base_url: None,
            default_model: Some("moonshot-v1".into()),
        });
        let resolved = resolver.resolve("openai/gpt-4o-mini").unwrap();
        assert_eq!(resolved.provider_name, "openai");
        assert_eq!(resolved.provider.model, "gpt-4o-mini");
        assert_eq!(resolved.provider.api_key, Some("ak".into()));
    }

    #[test]
    fn resolver_falls_back_to_host_provider() {
        let resolver = HostProviderResolver::new(ProviderConfig {
            provider_id: "openai".into(),
            api_key: "ak".into(),
            base_url: Some("https://example.com/v1".into()),
            default_model: Some("gpt-4o-mini".into()),
        });
        let resolved = resolver.resolve("gpt-4o").unwrap();
        assert_eq!(resolved.provider_name, "openai");
        assert_eq!(resolved.provider.model, "gpt-4o");
    }

    #[tokio::test]
    async fn environment_wraps_turn_started() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let env = HostAgentEnvironment {
            session_id: "sess-1".into(),
            agent_id: "main".into(),
            sink: Arc::new(CollectSink(Arc::clone(&events))),
        };
        env.emit_event(agent_rs::turn::types::AgentEvent::TurnStarted {
            turn_id: 7,
            origin: agent_rs::context::types::PromptOrigin::User,
        });
        let ev = events.lock().unwrap().pop().unwrap();
        match ev {
            HostEvent::TurnStarted { session_id, agent_id, turn_id, .. } => {
                assert_eq!(session_id, "sess-1");
                assert_eq!(agent_id, "main");
                assert_eq!(turn_id, 7);
            }
            _ => panic!("expected TurnStarted, got {:?}", ev),
        }
    }
}
```

- [ ] **运行并确认失败**：

```bash
cd rust-ody && cargo test -p ody-host --lib agent_bridge::tests
```

预期失败：`agent_bridge` 模块不存在。

- [ ] **实现 `rust-ody/crates/ody-host/src/agent_bridge.rs`**：

```rust
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use kosong_rs::provider::{AbortSignal, ChatProvider, ModelCapability};

use agent_rs::agent::{AgentEnvironment, LlmFactory, ProviderResolver};
use agent_rs::config::state::ResolvedRuntimeProvider;
use agent_rs::config::types::ProviderConfig as AgentProviderConfig;
use agent_rs::context::types::PromptOrigin as AgentPromptOrigin;
use agent_rs::permission::types::{ApprovalRequest as AgentApprovalRequest, ApprovalResponse as AgentApprovalResponse};
use agent_rs::skill::types::{SkillActivatedEvent, SkillPromptError};
use agent_rs::turn::kosong_llm::{KosongLLM, KosongLLMConfig};
use agent_rs::turn::types::{
    AgentEvent as AgentRsEvent, HookResult, StopHookBlock,
};

use crate::config::ProviderConfig as HostProviderConfig;
use crate::events::{
    AgentEvent as HostEvent, EventSink, PromptOrigin as HostPromptOrigin, TurnEndReason,
};
use crate::error::RpcError;

/// 把 host 配置解析为 agent-rs 需要的 ResolvedRuntimeProvider。
pub struct HostProviderResolver {
    host_provider: HostProviderConfig,
}

impl HostProviderResolver {
    pub fn new(host_provider: HostProviderConfig) -> Self {
        Self { host_provider }
    }

    fn parse_alias(&self, raw: &str) -> (String, String) {
        let (provider_id, model) = if let Some(idx) = raw.find('/') {
            (raw[..idx].to_string(), raw[idx + 1..].to_string())
        } else if let Some(idx) = raw.find(':') {
            (raw[..idx].to_string(), raw[idx + 1..].to_string())
        } else {
            (self.host_provider.provider_id.clone(), raw.to_string())
        };
        let model = if model.trim().is_empty() {
            self.host_provider.default_model.clone().unwrap_or_else(|| "gpt-4o-mini".into())
        } else {
            model
        };
        (provider_id, model)
    }

    fn provider_type_from_id(id: &str) -> Option<kosong_rs::provider::ProviderType> {
        serde_json::from_value(serde_json::Value::String(id.to_string())).ok()
    }
}

impl ProviderResolver for HostProviderResolver {
    fn default_model(&self) -> Option<String> {
        self.host_provider.default_model.clone()
    }

    fn resolve(&self, model_alias: &str) -> Option<ResolvedRuntimeProvider> {
        let (provider_id, model) = self.parse_alias(model_alias);
        let provider_type = Self::provider_type_from_id(&provider_id)?;
        let capability = kosong_rs::resolve_model_capability(&provider_id, &model)
            .unwrap_or_else(kosong_rs::provider::ModelCapability::unknown);
        Some(ResolvedRuntimeProvider {
            provider_name: provider_id,
            provider: AgentProviderConfig {
                r#type: provider_type,
                model,
                api_key: Some(self.host_provider.api_key.clone()).filter(|k| !k.is_empty()),
                base_url: self.host_provider.base_url.clone(),
                default_headers: None,
            },
            model_capabilities: capability,
        })
    }

    fn thinking_config(&self) -> Option<agent_rs::config::thinking::ThinkingConfig> {
        None
    }
}

/// 用 KosongLLM 包装 host 已经创建好的 ChatProvider。
pub struct HostLlmFactory;

impl LlmFactory for HostLlmFactory {
    fn create(
        &self,
        provider: Box<dyn ChatProvider>,
        model_name: String,
        system_prompt: String,
        capability: Option<ModelCapability>,
    ) -> Arc<dyn agent_rs::agent_loop::llm::Llm> {
        Arc::new(KosongLLM::new(KosongLLMConfig {
            provider,
            model_name,
            system_prompt,
            capability,
            completion_budget_config: None,
        }))
    }
}

/// 把 agent-rs 的事件/approval/hook/telemetry/log 桥接到 host 的 EventSink。
#[derive(Clone)]
pub struct HostAgentEnvironment {
    pub session_id: String,
    pub agent_id: String,
    pub sink: Arc<dyn EventSink>,
}

#[async_trait]
impl AgentEnvironment for HostAgentEnvironment {
    fn emit_event(&self, event: AgentRsEvent) {
        self.sink.emit(map_agent_event(self.session_id.clone(), self.agent_id.clone(), event));
    }

    async fn request_approval(
        &self,
        req: &AgentApprovalRequest,
        _signal: AbortSignal,
    ) -> Result<AgentApprovalResponse, anyhow::Error> {
        let payload = serde_json::to_vec(req)?;
        let bytes = self.sink.request("requestApproval", payload).await.map_err(|e| anyhow::anyhow!(e))?;
        let decision: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            serde_json::json!({ "decision": "cancelled" })
        });
        Ok(AgentApprovalResponse {
            decision: decision.get("decision").and_then(|v| v.as_str()).unwrap_or("cancelled").to_string(),
            scope: decision.get("scope").and_then(|v| v.as_str()).map(|s| s.to_string()),
            feedback: decision.get("feedback").and_then(|v| v.as_str()).map(|s| s.to_string()),
            selected_label: decision.get("selectedLabel").and_then(|v| v.as_str()).map(|s| s.to_string()),
        })
    }

    fn fire_hook_pre_tool_use(
        &self,
        tool_name: &str,
        tool_input: serde_json::Value,
        tool_call_id: &str,
        _signal: AbortSignal,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>, anyhow::Error>> + Send + '_>> {
        let sink = Arc::clone(&self.sink);
        let tool_name = tool_name.to_string();
        let tool_call_id = tool_call_id.to_string();
        Box::pin(async move {
            let payload = serde_json::json!({
                "toolName": tool_name,
                "toolInput": tool_input,
                "toolCallId": tool_call_id,
            });
            let _ = sink.request("fireHook.preToolUse", serde_json::to_vec(&payload)?).await;
            Ok(None)
        })
    }

    fn fire_hook_permission_request(&self, tool_name: &str, data: serde_json::Value) {
        let _ = self.sink.request("fireHook.permissionRequest", serde_json::to_vec(&serde_json::json!({
            "toolName": tool_name,
            "data": data,
        })));
    }

    fn fire_hook_permission_result(&self, tool_name: &str, data: serde_json::Value) {
        let _ = self.sink.request("fireHook.permissionResult", serde_json::to_vec(&serde_json::json!({
            "toolName": tool_name,
            "data": data,
        })));
    }

    fn fire_hook_user_prompt_submit(
        &self,
        input: Vec<kosong_rs::message::ContentPart>,
        _signal: AbortSignal,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HookResult>, anyhow::Error>> + Send + '_>> {
        let sink = Arc::clone(&self.sink);
        Box::pin(async move {
            let payload = serde_json::json!({ "input": input });
            let _ = sink.request("fireHook.userPromptSubmit", serde_json::to_vec(&payload)?).await;
            Ok(vec![])
        })
    }

    fn fire_hook_stop_hook(
        &self,
        _signal: AbortSignal,
    ) -> Pin<Box<dyn Future<Output = Result<Option<StopHookBlock>, anyhow::Error>> + Send + '_>> {
        let sink = Arc::clone(&self.sink);
        Box::pin(async move {
            let payload = serde_json::json!({});
            let _ = sink.request("fireHook.stop", serde_json::to_vec(&payload)?).await;
            Ok(None)
        })
    }

    fn fire_and_forget_hook(&self, event: &str, data: serde_json::Value) {
        let _ = self.sink.request("fireAndForgetHook", serde_json::to_vec(&serde_json::json!({
            "event": event,
            "data": data,
        })));
    }

    fn trigger_hook(
        &self,
        event: &str,
        data: serde_json::Value,
        _signal: AbortSignal,
    ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send + '_>> {
        let sink = Arc::clone(&self.sink);
        let event = event.to_string();
        Box::pin(async move {
            let payload = serde_json::json!({ "event": event, "data": data });
            let _ = sink.request("triggerHook", serde_json::to_vec(&payload)?).await;
            Ok(())
        })
    }

    fn track_telemetry(&self, event: &str, properties: serde_json::Value) {
        let _ = self.sink.request("telemetry.track", serde_json::to_vec(&serde_json::json!({
            "event": event,
            "properties": properties,
        })));
    }

    fn log_debug(&self, msg: &str, data: serde_json::Value) {
        tracing::debug!(target: "agent", message = msg, data = %data);
    }
    fn log_warn(&self, msg: &str, data: serde_json::Value) {
        tracing::warn!(target: "agent", message = msg, data = %data);
    }
    fn log_error(&self, msg: &str, data: serde_json::Value) {
        tracing::error!(target: "agent", message = msg, data = %data);
    }
}

fn map_origin(origin: AgentPromptOrigin) -> HostPromptOrigin {
    match origin {
        AgentPromptOrigin::User => HostPromptOrigin::User,
        AgentPromptOrigin::SkillActivation { activation_id, skill_name, .. } => {
            HostPromptOrigin::SkillActivation { activation_id, skill_name }
        }
        AgentPromptOrigin::Injection { variant } => HostPromptOrigin::Injection,
        AgentPromptOrigin::CompactionSummary => HostPromptOrigin::CompactionSummary,
        AgentPromptOrigin::SystemTrigger { name } => HostPromptOrigin::SystemTrigger,
        AgentPromptOrigin::BackgroundTask { task_id, status, notification_id } => {
            HostPromptOrigin::BackgroundTask { task_id, status, notification_id }
        }
        AgentPromptOrigin::CronJob { job_id, cron, recurring, coalesced_count, stale } => {
            HostPromptOrigin::CronJob { job_id, cron, recurring, coalesced_count, stale }
        }
        AgentPromptOrigin::CronMissed { count } => HostPromptOrigin::CronMissed { count },
        AgentPromptOrigin::HookResult { event, blocked } => HostPromptOrigin::HookResult { event, blocked },
    }
}

fn map_agent_event(session_id: String, agent_id: String, event: AgentRsEvent) -> HostEvent {
    use agent_rs::turn::types::AgentEvent::*;
    match event {
        TurnStarted { turn_id, origin } => HostEvent::TurnStarted {
            session_id,
            agent_id,
            turn_id,
            origin: map_origin(origin),
        },
        TurnEnded(te) => HostEvent::TurnEnded {
            session_id,
            agent_id,
            turn_id: te.turn_id,
            reason: match te.reason {
                agent_rs::turn::types::TurnEndedReason::Completed => TurnEndReason::Completed,
                agent_rs::turn::types::TurnEndedReason::Cancelled => TurnEndReason::Cancelled,
                agent_rs::turn::types::TurnEndedReason::Failed => TurnEndReason::Failed,
            },
            error: te.error.map(|e| e.message),
        },
        AssistantDelta { turn_id, delta } => HostEvent::AssistantDelta {
            session_id,
            agent_id,
            turn_id,
            delta,
        },
        ThinkingDelta { turn_id, delta } => HostEvent::ThinkingDelta {
            session_id,
            agent_id,
            turn_id,
            delta,
        },
        ToolCallStarted { turn_id, tool_call_id, name, args, description, display } => {
            HostEvent::ToolCall {
                session_id,
                agent_id,
                tool_name: name,
                args,
            }
        }
        ToolResult { turn_id, tool_call_id, output, is_error } => {
            let result = match output {
                agent_rs::records::nested::ExecutableToolOutput::Text(t) => serde_json::json!(t),
                agent_rs::records::nested::ExecutableToolOutput::Parts(parts) => serde_json::to_value(parts).unwrap_or_default(),
            };
            HostEvent::ToolResult {
                session_id,
                agent_id,
                tool_name: tool_call_id.clone(), // host ToolResult 目前没有 toolCallId 字段，先用 tool_call_id 占位
                result,
            }
        }
        AgentStatusUpdated { model, thinking_level, permission, context_tokens, max_context_tokens, session_mode, session_mode_file_path } => {
            HostEvent::AgentStatusUpdated {
                session_id,
                agent_id,
                model,
                thinking_level,
                permission,
                context_tokens,
                max_context_tokens,
                session_mode,
                session_mode_file_path,
            }
        }
        BackgroundTaskStarted { info } => HostEvent::BackgroundTaskStarted {
            session_id,
            agent_id,
            info: serde_json::to_value(info).unwrap_or_default(),
        },
        BackgroundTaskTerminated { info } => HostEvent::BackgroundTaskTerminated {
            session_id,
            agent_id,
            info: serde_json::to_value(info).unwrap_or_default(),
        },
        CronFired { origin, prompt } => HostEvent::CronFired {
            session_id,
            agent_id,
            origin: map_origin(origin),
            prompt,
        },
        _ => HostEvent::Status {
            session_id,
            agent_id,
            status: serde_json::to_string(&event).unwrap_or_else(|_| "agent-rs-event".into()),
        },
    }
}
```

- [ ] **扩展 `rust-ody/crates/ody-host/src/events.rs` 的 `AgentEvent`**：在现有变体之后追加：

```rust
    #[serde(rename = "thinking.delta")]
    ThinkingDelta {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "turnId")]
        turn_id: i64,
        delta: String,
    },
    #[serde(rename = "background.task.started")]
    BackgroundTaskStarted {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        info: serde_json::Value,
    },
    #[serde(rename = "background.task.terminated")]
    BackgroundTaskTerminated {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        info: serde_json::Value,
    },
    #[serde(rename = "cron.fired")]
    CronFired {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        origin: PromptOrigin,
        prompt: String,
    },
```

- [ ] **修改 `rust-ody/crates/ody-host/src/lib.rs`**：加入 `pub mod agent_bridge;`。

- [ ] **运行并确认通过**：

```bash
cd rust-ody && cargo test -p ody-host --lib agent_bridge::tests
```

- [ ] **提交**：

```bash
git add rust-ody/crates/ody-host/src/agent_bridge.rs rust-ody/crates/ody-host/src/events.rs rust-ody/crates/ody-host/src/lib.rs
git commit -m "feat(ody-host): bridge agent-rs events, approval, hooks and provider resolution"
```

---

## Task 3: Session 持 Agent + Agent 控制方法补全

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/agent.rs:???`（补全 `turn()`、`update_config()`、`set_permission_mode()`、`enter_session_mode()`、`exit_session_mode()`、`permission_data()` 等公开方法）
- Modify: `rust-ody/crates/ody-host/src/session/store.rs:11-27`（`SessionState` 新增字段）
- Modify: `rust-ody/crates/ody-host/src/session/manager.rs:20-168`（`SessionManager` 持 `kaos`/`event_sink`/`provider_config`；`Session` 延迟构造 `Agent`）
- Modify: `rust-ody/crates/ody-host/src/host.rs:15-23`（`CoreHost::new` 把 `Box<dyn EventSink>` 改为 `Arc<dyn EventSink>`，并传入 `SessionManager`）
- Modify: `rust-ody/crates/ody-host/src/main.rs:36-43`（调用处把 `Box` 转成 `Arc`）
- Test: `rust-ody/crates/ody-host/src/session/manager.rs:171-208` 中新增 session 能返回 agent 的测试

**核心设计**：
- `Session` 增加 `agent: tokio::sync::Mutex<Option<Arc<agent_rs::agent::Agent>>>`，第一次调用 `agent()` 时用 `AgentBuilder` 构造，之后复用。
- `SessionManager` 增加构造 Agent 所需的 `kaos`、`event_sink`、`provider_config`。
- `Agent` 补全控制方法，让 `CoreHost` 可以通过 `session.agent().await.update_config(...)` 等方式操作。
- `EventSink` 在 `CoreHost` 中统一用 `Arc<dyn EventSink>`，避免 `Box` 与 `Arc` 之间的所有权转换问题。

### 步骤

- [ ] **补全 `rust-ody/crates/agent-rs/src/agent.rs` 的 `impl Agent`**：在 `impl Agent` 块中追加以下方法（位置在 `pub fn id(&self)` 之后即可）：

```rust
    pub fn turn(&self) -> &TurnFlow {
        &self.turn
    }

    pub fn update_config(&self, update: AgentConfigUpdateData) {
        self.config.lock().unwrap().update(update);
        self.refresh_llm();
    }

    pub fn set_permission_mode(&self, mode: PermissionMode) {
        self.permission.lock().unwrap().set_mode(mode);
    }

    pub fn permission_data(&self) -> PermissionData {
        self.permission.lock().unwrap().data()
    }

    pub async fn enter_session_mode(&self, kind: SessionModeKind, id: Option<String>) -> anyhow::Result<()> {
        self.session_mode.lock().unwrap().enter(kind, id, None).await
    }

    pub async fn exit_session_mode(&self) -> anyhow::Result<()> {
        self.session_mode.lock().unwrap().exit(None).await
    }

    pub fn config_data(&self) -> AgentConfigData {
        self.config.lock().unwrap().data()
    }
```

> 注意：`TurnFlow`、`AgentConfigUpdateData`、`PermissionMode`、`SessionModeKind`、`AgentConfigData`、`PermissionData` 均需已在 `agent.rs` 顶部 `use` 中引入。Part 1 的实现已引入大部分，缺的在 Task 4 编译错误时补上即可。

- [ ] **修改 `rust-ody/crates/ody-host/src/session/store.rs:11-27`**：把 `SessionState` 改为：

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SessionState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_prompt: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub custom: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_records_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_state: Option<serde_json::Value>,
}
```

- [ ] **修改 `rust-ody/crates/ody-host/src/session/manager.rs`**：

把 `SessionManager` 和 `Session` 改为：

```rust
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::agent_bridge::{HostAgentEnvironment, HostLlmFactory, HostProviderResolver};
use crate::config::ProviderConfig as HostProviderConfig;
use crate::events::EventSink;
use crate::session::store::{IndexEntry, SessionError, SessionState, SessionStoreAdapter, SessionSummary};

fn new_id() -> String {
    Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string()
}

#[derive(Debug, Default, Clone)]
pub struct SessionFilter { ... } // 保持不变

pub struct SessionManager {
    store: SessionStoreAdapter,
    active: RwLock<HashMap<String, Arc<Session>>>,
    kaos: Arc<kaos_rs::kaos::Kaos>,
    event_sink: Arc<dyn EventSink>,
    provider_config: HostProviderConfig,
}

#[derive(Debug)]
pub struct Session {
    pub id: String,
    pub work_dir: std::path::PathBuf,
    pub dir: std::path::PathBuf,
    state: tokio::sync::Mutex<SessionState>,
    agent: tokio::sync::Mutex<Option<Arc<agent_rs::agent::Agent>>>,
}

impl Session {
    // 原有 getter/setter 保持不变

    pub async fn agent(
        &self,
        kaos: Arc<kaos_rs::kaos::Kaos>,
        event_sink: Arc<dyn EventSink>,
        provider_config: &HostProviderConfig,
    ) -> Result<Arc<agent_rs::agent::Agent>, SessionError> {
        let mut cached = self.agent.lock().await;
        if let Some(agent) = cached.as_ref() {
            return Ok(Arc::clone(agent));
        }
        let env = Arc::new(HostAgentEnvironment {
            session_id: self.id.clone(),
            agent_id: "main".into(),
            sink: event_sink,
        });
        let resolver = Arc::new(HostProviderResolver::new(provider_config.clone()));
        let records_path = self.dir.join("wire.jsonl");
        let agent = agent_rs::agent::AgentBuilder::new("main", kaos, env)
            .homedir(self.dir.clone())
            .provider_resolver(resolver)
            .llm_factory(Arc::new(HostLlmFactory))
            .build()
            .await
            .map_err(|e| SessionError::Io { source: std::io::Error::new(std::io::ErrorKind::Other, e), path: records_path })?;

        // 把 session state 中缓存的 model/thinking/permission 同步到 agent
        let state = self.state.lock().await;
        if let Some(model) = state.model.clone() {
            let update = agent_rs::config::types::AgentConfigUpdateData {
                model_alias: Some(model),
                ..Default::default()
            };
            agent.update_config(update);
        }
        if let Some(level) = state.thinking.clone() {
            let update = agent_rs::config::types::AgentConfigUpdateData {
                thinking_level: Some(level),
                ..Default::default()
            };
            agent.update_config(update);
        }
        if let Some(mode_str) = state.permission.clone() {
            let mode = match mode_str.as_str() {
                "yolo" => agent_rs::records::nested::PermissionMode::Yolo,
                "auto" => agent_rs::records::nested::PermissionMode::Auto,
                _ => agent_rs::records::nested::PermissionMode::Manual,
            };
            agent.set_permission_mode(mode);
        }
        drop(state);

        *cached = Some(Arc::clone(&agent));
        Ok(agent)
    }

    pub async fn persist_state(&self) -> Result<(), SessionError> {
        let mut state = self.state.lock().await.clone();
        state.agent_records_path = Some(self.dir.join("wire.jsonl"));
        crate::session::store::write_state_json(&self.dir, &state)
            .map_err(|e| SessionError::Io { source: e, path: self.dir.clone() })
    }
}

impl SessionManager {
    pub fn new(
        store: SessionStoreAdapter,
        kaos: Arc<kaos_rs::kaos::Kaos>,
        event_sink: Arc<dyn EventSink>,
        provider_config: HostProviderConfig,
    ) -> Self {
        Self { store, active: RwLock::new(HashMap::new()), kaos, event_sink, provider_config }
    }

    pub async fn create_with_id(... ) -> Result<SessionSummary, SessionError> {
        // ... 创建目录、写入 state.json、append index 保持不变 ...
        let summary = self.store.summary_from_dir(id.to_string(), &dir, &normalized)?;
        let session = Arc::new(Session {
            id: id.to_string(),
            work_dir: normalized,
            dir: dir.clone(),
            state: tokio::sync::Mutex::new(state),
            agent: tokio::sync::Mutex::new(None),
        });
        // 预创建 agent，使 createSession 后 prompt 立即可用
        let _ = session.agent(Arc::clone(&self.kaos), Arc::clone(&self.event_sink), &self.provider_config).await?;
        self.active.write().await.insert(id.to_string(), session);
        Ok(summary)
    }

    pub async fn get(&self, id: String) -> Result<Arc<Session>, SessionError> {
        {
            let active = self.active.read().await;
            if let Some(s) = active.get(&id) {
                return Ok(Arc::clone(s));
            }
        }
        let index = self.store.read_index()?;
        let entry = index.get(&id).cloned().ok_or_else(|| SessionError::NotFound { session_id: id.clone() })?;
        if !entry.session_dir.exists() {
            return Err(SessionError::NotFound { session_id: id });
        }
        let state = crate::session::store::read_state_json(&entry.session_dir)
            .map_err(|e| SessionError::Io { source: e, path: entry.session_dir.clone() })?
            .unwrap_or_default();
        let session = Arc::new(Session {
            id: id.clone(),
            work_dir: entry.work_dir.clone(),
            dir: entry.session_dir.clone(),
            state: tokio::sync::Mutex::new(state),
            agent: tokio::sync::Mutex::new(None),
        });
        self.active.write().await.insert(id, Arc::clone(&session));
        Ok(session)
    }

    // close 保持不变
}
```

- [ ] **修改 `rust-ody/crates/ody-host/src/host.rs`**：
  - `CoreHost` 中 `sink: Box<dyn EventSink>` 改为 `sink: Arc<dyn EventSink>`。
  - `CoreHost::new` 签名改为接收 `Arc<dyn EventSink>`。
  - `CoreHost::new` 内部创建 `SessionManager` 时传入 `Arc::clone(&sink)`、`Arc::clone(&kaos)`、`config.provider.clone()`。
  - 删除 `tool_registry`、`provider`、`turn_counter` 字段（后续不再使用；若编译器报 unused，保留为 `_` 或删除）。

变更后 `CoreHost` 大致：

```rust
pub struct CoreHost {
    pub config: HostConfig,
    pub session_manager: SessionManager,
    sink: Arc<dyn EventSink>,
    kaos: Arc<Kaos>,
}

impl CoreHost {
    pub fn new(
        config: HostConfig,
        sink: Arc<dyn EventSink>,
        _provider: Box<dyn LlmProvider>, // 暂时保留参数以兼容 main.rs，后续移除
    ) -> Result<Self, crate::error::HostError> {
        let store = SessionStoreAdapter::new(config.home_dir.clone());
        let env = detect_environment_from_node();
        let kaos = Arc::new(Kaos::new(env, &config.home_dir));
        let session_manager = SessionManager::new(
            store,
            Arc::clone(&kaos),
            Arc::clone(&sink),
            config.provider.clone(),
        );
        Ok(Self { config, session_manager, sink, kaos })
    }
    ...
}
```

- [ ] **修改 `rust-ody/crates/ody-host/src/main.rs:36-43`**：把 `event_sink` 包成 `Arc`：

```rust
    let (server, event_sink) = build_transport(config.transport.clone()).await?;
    let event_sink: Arc<dyn ody_host::events::EventSink> = Arc::from(event_sink);
    let provider: Box<dyn ody_host::llm::LlmProvider> = if config.mock_provider { ... } else { ... };
    let host = Arc::new(CoreHost::new(config, Arc::clone(&event_sink), provider)?);
```

> `Arc::from(Box<dyn Trait>)` 在 Rust 标准库中存在（`impl<T: ?Sized> From<Box<T>> for Arc<T>`）。如果编译器报错，改用 `Arc::new(event_sink)` 并把 `build_transport` 返回类型改为 `Arc<dyn EventSink>`（涉及 transport 模块，超出本 Task；优先用 `Arc::from`）。

- [ ] **更新 `rust-ody/crates/ody-host/src/session/manager.rs` 测试**：`SessionManager::new` 签名变了，所有测试都要更新。例如：

```rust
fn make_manager() -> SessionManager {
    let tmp = tempfile::tempdir().unwrap();
    let kaos = Arc::new(kaos_rs::kaos::Kaos::new(kaos_rs::environment::detect_environment_from_node(), tmp.path()));
    let sink: Arc<dyn EventSink> = Arc::new(NoopSink);
    SessionManager::new(
        SessionStoreAdapter::new(tmp.path().to_path_buf()),
        kaos,
        sink,
        crate::config::ProviderConfig {
            provider_id: "mock".into(),
            api_key: "".into(),
            base_url: None,
            default_model: Some("mock".into()),
        },
    )
}

struct NoopSink;
#[async_trait::async_trait]
impl EventSink for NoopSink {
    async fn request(&self, _method: &str, _payload: Vec<u8>) -> Result<Vec<u8>, RpcError> { Ok(vec![]) }
    fn emit(&self, _event: AgentEvent) {}
}
```

然后新增测试：

```rust
#[tokio::test]
async fn session_returns_agent_after_creation() {
    let manager = make_manager();
    let summary = manager.create(Path::new("/tmp/wd"), Some("t")).await.unwrap();
    let session = manager.get(summary.id).await.unwrap();
    let agent = session.agent(
        Arc::clone(&manager.kaos),
        Arc::clone(&manager.event_sink),
        &manager.provider_config,
    ).await.unwrap();
    assert_eq!(agent.agent_type(), "main");
}
```

- [ ] **运行并确认通过**：

```bash
cd rust-ody && cargo test -p ody-host --lib session::manager
```

- [ ] **提交**：

```bash
git add rust-ody/crates/agent-rs/src/agent.rs rust-ody/crates/ody-host/src/session/store.rs rust-ody/crates/ody-host/src/session/manager.rs rust-ody/crates/ody-host/src/host.rs rust-ody/crates/ody-host/src/main.rs
git commit -m "feat(ody-host): attach Agent to Session and expose control methods"
```

---

## Task 4: CoreHost dispatch 路由改写

**Depends on:** Task 3

**Files:**
- Modify: `rust-ody/crates/ody-host/src/host.rs:15-529`（重写 prompt/steer/cancel/setModel/setThinking/setPermission/getConfig/setConfig/getPermission/getContext/getPlan/resume_session；删除旧的 tool_registry/provider/turn_counter 字段与 chat 方法中的旧实现）
- Modify: `rust-ody/crates/ody-host/src/host.rs:609-999`（更新测试中的 `MockSink`、`make_host_with_events` 以及各测试断言）

**核心设计**：
- `prompt` / `steer`：提取 `input` → 调用 `session.agent().await.turn().prompt/steer(...)` → `wait_for_current_turn(None)` → 返回 `{ ok: true, turnId }`。
- `cancel`：调用 `agent.turn().cancel(turn_id, reason)`。
- `setModel` / `setThinking`：生成 `AgentConfigUpdateData` 调用 `agent.update_config(...)`，并同步回 `SessionState` 后持久化。
- `setPermission`：字符串映射为 `PermissionMode` 后调用 `agent.set_permission_mode(...)`。
- `getConfig` / `getPermission` / `getContext` / `getPlan`：从 `Agent` 直接读取。
- `resume_session`：构造/获取 Agent 后调用 `agent.resume().await`，并把 `ReplayResult` 写回 `SessionState`。
- 旧的 `chat` 方法保留但内部直接转发到 `prompt`，避免既有调用方（测试）需要大改。

### 步骤

- [ ] **先改 `CoreHost` 字段与构造**：确保 `CoreHost` 只剩：

```rust
pub struct CoreHost {
    pub config: HostConfig,
    pub session_manager: SessionManager,
    sink: Arc<dyn EventSink>,
    kaos: Arc<Kaos>,
}
```

并删除 `allocate_turn_id`（由 `TurnFlow` 自己分配 turn id）。

- [ ] **重写核心 dispatch 方法**：把 `rust-ody/crates/ody-host/src/host.rs` 中 `prompt` 到 `get_plan` 的实现替换为：

```rust
    async fn prompt(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let parts = extract_input_parts(&payload).ok_or("missing or empty prompt input")?;
        let session = self.session_manager.get(session_id.clone()).await.map_err(|e| e.to_string())?;
        let agent = session.agent(
            Arc::clone(&self.session_manager.kaos),
            Arc::clone(&self.session_manager.event_sink),
            &self.session_manager.provider_config,
        ).await.map_err(|e| e.to_string())?;

        let turn_id = agent.turn().prompt(parts, agent_rs::context::types::PromptOrigin::User);
        match turn_id {
            Some(id) => {
                let result = agent.turn().wait_for_current_turn(None).await.map_err(|e| e.to_string())?;
                let finish_reason = match result.event.reason {
                    agent_rs::turn::types::TurnEndedReason::Completed => "stop",
                    agent_rs::turn::types::TurnEndedReason::Cancelled => "cancelled",
                    agent_rs::turn::types::TurnEndedReason::Failed => "error",
                };
                Ok(serde_json::json!({
                    "ok": true,
                    "turnId": id,
                    "finishReason": finish_reason,
                    "error": result.event.error.map(|e| e.message),
                }))
            }
            None => Ok(serde_json::json!({ "ok": true, "turnId": serde_json::Value::Null })),
        }
    }

    async fn steer(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let parts = extract_input_parts(&payload).ok_or("missing or empty prompt input")?;
        let session = self.session_manager.get(session_id.clone()).await.map_err(|e| e.to_string())?;
        let agent = session.agent(
            Arc::clone(&self.session_manager.kaos),
            Arc::clone(&self.session_manager.event_sink),
            &self.session_manager.provider_config,
        ).await.map_err(|e| e.to_string())?;

        let turn_id = agent.turn().steer(parts, agent_rs::context::types::PromptOrigin::User);
        match turn_id {
            Some(id) => {
                let _ = agent.turn().wait_for_current_turn(None).await.map_err(|e| e.to_string())?;
                Ok(serde_json::json!({ "ok": true, "turnId": id }))
            }
            None => Ok(serde_json::json!({ "ok": true, "turnId": serde_json::Value::Null })),
        }
    }

    async fn cancel(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let turn_id = payload.get("turnId").and_then(|v| v.as_i64());
        let reason = payload.get("reason").and_then(|v| v.as_str()).map(|s| s.to_string());
        let session = self.session_manager.get(session_id).await.map_err(|e| e.to_string())?;
        let agent = session.agent(
            Arc::clone(&self.session_manager.kaos),
            Arc::clone(&self.session_manager.event_sink),
            &self.session_manager.provider_config,
        ).await.map_err(|e| e.to_string())?;
        agent.turn().cancel(turn_id, reason);
        Ok(serde_json::json!({ "ok": true }))
    }

    async fn set_model(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let raw = payload.get("model").and_then(|v| v.as_str()).ok_or("missing model")?.to_string();
        let session = self.session_manager.get(session_id.clone()).await.map_err(|e| e.to_string())?;
        let agent = session.agent(
            Arc::clone(&self.session_manager.kaos),
            Arc::clone(&self.session_manager.event_sink),
            &self.session_manager.provider_config,
        ).await.map_err(|e| e.to_string())?;

        let (_, model) = parse_model_alias(&raw);
        agent.update_config(agent_rs::config::types::AgentConfigUpdateData {
            model_alias: Some(model.clone()),
            ..Default::default()
        });
        session.set_model(Some(model.clone())).await;
        session.persist_state().await.map_err(|e| e.to_string())?;

        Ok(serde_json::json!({ "model": model, "providerName": self.session_manager.provider_config.provider_id }))
    }

    async fn set_thinking(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let level = payload.get("level").and_then(|v| v.as_str()).ok_or("missing level")?.to_string();
        let session = self.session_manager.get(session_id.clone()).await.map_err(|e| e.to_string())?;
        let agent = session.agent(
            Arc::clone(&self.session_manager.kaos),
            Arc::clone(&self.session_manager.event_sink),
            &self.session_manager.provider_config,
        ).await.map_err(|e| e.to_string())?;

        agent.update_config(agent_rs::config::types::AgentConfigUpdateData {
            thinking_level: Some(level.clone()),
            ..Default::default()
        });
        session.set_thinking(Some(level)).await;
        session.persist_state().await.map_err(|e| e.to_string())?;
        Ok(serde_json::json!({}))
    }

    async fn set_permission(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let mode_str = payload.get("mode").and_then(|v| v.as_str()).ok_or("missing mode")?.to_string();
        let session = self.session_manager.get(session_id.clone()).await.map_err(|e| e.to_string())?;
        let agent = session.agent(
            Arc::clone(&self.session_manager.kaos),
            Arc::clone(&self.session_manager.event_sink),
            &self.session_manager.provider_config,
        ).await.map_err(|e| e.to_string())?;

        let mode = match mode_str.as_str() {
            "yolo" => agent_rs::records::nested::PermissionMode::Yolo,
            "auto" => agent_rs::records::nested::PermissionMode::Auto,
            _ => agent_rs::records::nested::PermissionMode::Manual,
        };
        agent.set_permission_mode(mode);
        session.set_permission(Some(mode_str)).await;
        session.persist_state().await.map_err(|e| e.to_string())?;
        Ok(serde_json::json!({}))
    }

    async fn get_agent_config(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let session = self.session_manager.get(session_id).await.map_err(|e| e.to_string())?;
        let agent = session.agent(
            Arc::clone(&self.session_manager.kaos),
            Arc::clone(&self.session_manager.event_sink),
            &self.session_manager.provider_config,
        ).await.map_err(|e| e.to_string())?;
        let data = agent.config_data();
        Ok(serde_json::json!({
            "cwd": data.cwd,
            "provider": data.provider.map(|p| serde_json::json!({
                "id": p.r#type,
                "model": p.model,
            })),
            "modelAlias": data.model_alias,
            "modelCapabilities": {
                "image_in": data.model_capabilities.image_in,
                "video_in": data.model_capabilities.video_in,
                "audio_in": data.model_capabilities.audio_in,
                "thinking": data.model_capabilities.thinking,
                "tool_use": data.model_capabilities.tool_use,
                "max_context_tokens": data.model_capabilities.max_context_tokens,
                "max_output_tokens": data.model_capabilities.max_output_tokens,
            },
            "thinkingLevel": data.thinking_level,
            "systemPrompt": data.system_prompt,
        }))
    }

    async fn set_agent_config(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let session = self.session_manager.get(session_id.clone()).await.map_err(|e| e.to_string())?;
        let agent = session.agent(
            Arc::clone(&self.session_manager.kaos),
            Arc::clone(&self.session_manager.event_sink),
            &self.session_manager.provider_config,
        ).await.map_err(|e| e.to_string())?;

        let update = agent_rs::config::types::AgentConfigUpdateData {
            cwd: payload.get("cwd").and_then(|v| v.as_str()).map(|s| s.to_string()),
            model_alias: payload.get("modelAlias").and_then(|v| v.as_str()).map(|s| s.to_string()),
            profile_name: payload.get("profileName").and_then(|v| v.as_str()).map(|s| s.to_string()),
            thinking_level: payload.get("thinkingLevel").and_then(|v| v.as_str()).map(|s| s.to_string()),
            system_prompt: payload.get("systemPrompt").and_then(|v| v.as_str()).map(|s| s.to_string()),
        };
        agent.update_config(update);
        session.persist_state().await.map_err(|e| e.to_string())?;
        self.get_agent_config(payload).await
    }

    async fn get_permission(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let session = self.session_manager.get(session_id).await.map_err(|e| e.to_string())?;
        let agent = session.agent(
            Arc::clone(&self.session_manager.kaos),
            Arc::clone(&self.session_manager.event_sink),
            &self.session_manager.provider_config,
        ).await.map_err(|e| e.to_string())?;
        let data = agent.permission_data();
        Ok(serde_json::json!({
            "mode": format!("{:?}", data.mode).to_lowercase(),
            "rules": data.rules,
        }))
    }

    fn get_context(&self) -> serde_json::Value {
        serde_json::json!({
            "history": [],
            "tokenCount": 0,
        })
    }

    async fn get_context_for_session(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let session = self.session_manager.get(session_id).await.map_err(|e| e.to_string())?;
        let agent = session.agent(
            Arc::clone(&self.session_manager.kaos),
            Arc::clone(&self.session_manager.event_sink),
            &self.session_manager.provider_config,
        ).await.map_err(|e| e.to_string())?;
        let history: Vec<serde_json::Value> = agent.context().history()
            .into_iter()
            .map(|m| serde_json::to_value(m).unwrap_or_default())
            .collect();
        Ok(serde_json::json!({
            "history": history,
            "tokenCount": agent.context().token_count(),
        }))
    }

    async fn enter_plan(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let session = self.session_manager.get(session_id.clone()).await.map_err(|e| e.to_string())?;
        let agent = session.agent(
            Arc::clone(&self.session_manager.kaos),
            Arc::clone(&self.session_manager.event_sink),
            &self.session_manager.provider_config,
        ).await.map_err(|e| e.to_string())?;
        let id = payload.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
        agent.enter_session_mode(agent_rs::records::nested::SessionModeKind::Plan, id).await.map_err(|e| e.to_string())?;
        session.persist_state().await.map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "ok": true }))
    }

    async fn exit_plan(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        let session = self.session_manager.get(session_id.clone()).await.map_err(|e| e.to_string())?;
        let agent = session.agent(
            Arc::clone(&self.session_manager.kaos),
            Arc::clone(&self.session_manager.event_sink),
            &self.session_manager.provider_config,
        ).await.map_err(|e| e.to_string())?;
        agent.exit_session_mode().await.map_err(|e| e.to_string())?;
        session.persist_state().await.map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "ok": true }))
    }

    fn get_plan(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|e| e.to_string())?;
        let (session_id, _agent_id) = self.require_session_agent(&payload)?;
        runtime.block_on(async move {
            let session = self.session_manager.get(session_id).await.map_err(|e| e.to_string())?;
            let agent = session.agent(
                Arc::clone(&self.session_manager.kaos),
                Arc::clone(&self.session_manager.event_sink),
                &self.session_manager.provider_config,
            ).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "active": agent.session_mode().is_active(),
                "kind": agent.session_mode().kind(),
                "filePath": agent.session_mode().file_path(),
            }))
        })
    }

    async fn resume_session(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let id = payload.get("sessionId").and_then(|v| v.as_str()).ok_or("missing session id")?;
        let session = self.session_manager.get(id.to_string()).await.map_err(|e| e.to_string())?;
        let agent = session.agent(
            Arc::clone(&self.session_manager.kaos),
            Arc::clone(&self.session_manager.event_sink),
            &self.session_manager.provider_config,
        ).await.map_err(|e| e.to_string())?;
        let result = agent.resume().await.map_err(|e| e.to_string())?;
        {
            let mut state = session.state.lock().await;
            state.resume_state = Some(serde_json::json!({
                "recordsReplayed": result.records_replayed,
                "warning": result.warning,
            }));
        }
        session.persist_state().await.map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "id": session.id,
            "workDir": session.work_dir,
            "resumeState": state.resume_state,
        }))
    }
```

- [ ] **更新 dispatch 路由表**：在 `dispatch` 方法中：
  - 把 `"getContext"` 路由到 `self.get_context_for_session(payload).await`（`fn get_context(&self)` 保留给无 session 的旧调用）。
  - 新增 `"enterPlan"` → `Ok(self.enter_plan(payload).await.map_err(|e| e.to_string())?)`。
  - 新增 `"exitPlan"` → `Ok(self.exit_plan(payload).await.map_err(|e| e.to_string())?)`。
  - `"resumeSession"` 改为调用上面新的 `resume_session`。
  - `"chat"` 改为直接 `self.prompt(payload).await`（可保留 `chat` 方法名或直接 inline）。

- [ ] **更新测试中的 `MockSink`**：把 `request` 返回值改为 agent-rs 兼容的 JSON：

```rust
    #[async_trait::async_trait]
    impl EventSink for MockSink {
        async fn request(&self, _method: &str, _payload: Vec<u8>) -> Result<Vec<u8>, RpcError> {
            let resp = agent_rs::permission::types::ApprovalResponse {
                decision: "approved".into(),
                scope: None,
                feedback: None,
                selected_label: None,
            };
            Ok(serde_json::to_vec(&resp).unwrap())
        }
        fn emit(&self, event: AgentEvent) { ... }
    }
```

同时删除 `ApprovalDecision` / `ApprovalResponse` 的 host tools 导入（测试不再使用它们）。

- [ ] **更新 `make_host_with_events`**：由于 `CoreHost::new` 现在接收 `Arc<dyn EventSink>` 并忽略 provider：

```rust
    fn make_host_with_events() -> (CoreHost, Arc<Mutex<Vec<AgentEvent>>>) {
        let config = HostConfig { ... };
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink: Arc<dyn EventSink> = Arc::new(MockSink(Arc::clone(&events)));
        let host = CoreHost::new(config, Arc::clone(&sink), Box::new(MockProvider)).unwrap();
        (host, events)
    }
```

- [ ] **更新测试断言**：
  - `chat_returns_content` 改名为 `chat_routes_to_prompt_and_returns_ok`，断言 `result["ok"] == true`。
  - `prompt_emits_turn_events_and_returns_ok` 不再断言 `result["content"]`，只断言 `result["ok"] == true` 且事件流包含 `turn.started` / `turn.ended`。
  - `set_model_updates_session_model`：在 `setModel` 后继续调用 `getConfig`，断言 `modelAlias` 已变。
  - `set_permission_updates_session_permission`：断言 `getPermission` 返回新 mode。

- [ ] **运行并确认通过**：

```bash
cd rust-ody && cargo test -p ody-host --lib host::tests
```

由于 Agent 构造和 TurnFlow 运行依赖较多，第一次运行可能会有编译错误；按错误提示补充 `use` 语句即可（例如 `agent_rs::config::types`、`agent_rs::records::nested`、`agent_rs::context::types`）。

- [ ] **提交**：

```bash
git add rust-ody/crates/ody-host/src/host.rs
git commit -m "feat(ody-host): route AgentAPI RPC through Agent and TurnFlow"
```

---

## Task 5: 全树 typecheck 与既有测试回归

**Depends on:** Task 4

**Files：** 无新增；验证命令覆盖 `rust-ody` 整个 workspace。

**为什么需要这步**：Task 1-4 修改了跨 crate 的共享签名（`ConfigState.provider`、`SessionState`、`SessionManager::new`、`CoreHost::new`、`AgentEvent` 枚举、`EventSink` 所有权），必须在全 workspace 范围内确认没有遗漏的调用方，尤其是测试文件。

### 步骤

- [ ] **搜索所有 `CoreHost::new` 调用点**：

```bash
cd rust-ody && rg -n "CoreHost::new" crates/
```

预期只有两个命中：`crates/ody-host/src/main.rs` 和 `crates/ody-host/src/host.rs` 的测试。若还有其他（例如 bench、example），必须同步改为 `Arc<dyn EventSink>`。

- [ ] **搜索所有 `SessionManager::new` 调用点**：

```bash
cd rust-ody && rg -n "SessionManager::new" crates/
```

预期命中：`crates/ody-host/src/host.rs` 和 `crates/ody-host/src/session/manager.rs` 的测试。全部更新为新签名。

- [ ] **搜索所有 `AgentEvent::ToolResult` / `ToolCall` / `ApprovalResponse` 的 match**：

```bash
cd rust-ody && rg -n "AgentEvent::ToolResult|AgentEvent::ToolCall|ApprovalResponse" crates/ody-host/
```

确认测试中的 match 覆盖了新增/改动的变体与字段。

- [ ] **运行全 workspace check**：

```bash
cd rust-ody && cargo check --workspace --tests
```

预期输出最后为 `Finished dev [unoptimized + debuginfo] target(s) in ...`，无 error。

- [ ] **运行 ody-host 与 agent-rs 全部单元测试**：

```bash
cd rust-ody && cargo test -p agent-rs --lib && cargo test -p ody-host --lib
```

预期全部通过。若有个别测试因 MockProvider 输出格式变化而失败，按实际输出调整断言（例如 `echo: hello` 取代 `ok`）。

- [ ] **手动验证 stdio 启动**：

```bash
cd rust-ody && cargo run --bin ody-host -- --home /tmp/ody-host-smoke --mock-provider --stdio <<'EOF'
{"jsonrpc":"2.0","id":1,"method":"getCoreInfo","params":{}}
EOF
```

预期在 stderr 看到 `ody-host ready`，stdout 返回包含 `"name":"ody-host"` 的 JSON-RPC 响应。

- [ ] **提交**：

```bash
git add -A rust-ody/crates/ody-host rust-ody/crates/agent-rs
git commit -m "test(ody-host): whole-tree typecheck and test regression for Agent integration"
```

---

## 本 Part 自检清单

- [ ] 1. Spec-coverage: Task 1-5 覆盖了 roadmap 4.3.9.2（AgentAPI RPC 路由）、4.3.9.3（resume 路径）以及 4.3.9.1 中 `useProfile` 之外的所有 Agent 组装点。
- [ ] 2. Placeholder scan: 无 TODO/TBD；所有 trait 方法、RPC 路由、事件映射均有具体实现。
- [ ] 3. No phantom tasks: 每个 task 都产生文件变更或验证命令；无 `--allow-empty`。
- [ ] 4. Dependency soundness: Task 2→1, Task 3→2, Task 4→3, Task 5→4，无反向依赖。
- [ ] 5. Caller & build soundness: Task 5 明确搜索 `CoreHost::new`、`SessionManager::new`、`AgentEvent` 等共享签名并跑 `cargo check --workspace --tests`。
- [ ] 6. Test-the-risk: Task 1 验证 provider 凭证透传；Task 2 验证 resolver 与环境桥接；Task 3 验证 session 能返回 agent；Task 4 验证 prompt/steer 事件流与配置变更。
- [ ] 7. Type consistency: `HostAgentEnvironment` 使用 Part 1 定义的 `AgentEnvironment` trait；`Session::agent()` 返回 `Arc<agent_rs::agent::Agent>`；`CoreHost` dispatch 使用 `Agent` 控制方法名与 Part 1/3 一致。
