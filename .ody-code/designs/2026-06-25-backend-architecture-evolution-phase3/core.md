# Part 1 — Rust Host Core

> Scope: session runtime、persistence adapter、最小 LLM provider、内置 tool。  
> Corresponds to index: [Architecture & Data Flow](../2026-06-25-backend-architecture-evolution-phase3.md)

---

## 1. Component Overview

Rust host core 是 `ody-host` 的"业务层"，负责：
1. 解析启动配置（`HostConfig`）。
2. 管理会话生命周期（`SessionManager`）。
3. 按现有 `SessionStore` 格式读写磁盘（`SessionStoreAdapter`）。
4. 调用 LLM provider 并流式返回 assistant 事件（`LlmProvider`）。
5. 执行内置 tool 并走 approval 反向 RPC（`ToolRegistry` + `BashTool`）。
6. 将内部状态变化转换为 `AgentEvent`，通过 transport 层推送给 TUI。

---

## 2. Typed Interfaces

### 2.1 HostConfig

```rust
struct HostConfig {
    home_dir: PathBuf,               // 默认 ~/.ody [C:INFERRED]
    config_path: Option<PathBuf>,    // --config 覆盖 [C:USER]
    transport: TransportMode,        // stdio | socket { path | addr } [C:USER]
    log_level: LogLevel,             // 默认 info [C:INFERRED]
    provider: ProviderConfig,        // 见 2.4 [C:INFERRED]
}

enum TransportMode {
    Stdio,
    UnixSocket { path: PathBuf },
    TcpSocket { host: String, port: u16 },
}

enum LogLevel { Debug, Info, Warn, Error }
```

### 2.2 CoreHost（顶层聚合根）

```rust
struct CoreHost {
    config: HostConfig,
    sessions: SessionManager,
    tools: ToolRegistry,
    provider: Box<dyn LlmProvider>,
    event_sink: Box<dyn EventSink>,   // 将 AgentEvent 序列化后交给 transport
}

impl CoreHost {
    // contract: 根据 config 初始化所有子系统，返回可运行的 host
    fn new(config: HostConfig, event_sink: Box<dyn EventSink>) -> Result<Self, HostError>;

    // contract: 处理一条 CoreAPI method + 已反序列化的 payload，返回序列化后的返回值
    async fn dispatch(&self, method: &str, payload: JsonValue) -> Result<JsonValue, RpcError>;
}
```

### 2.3 SessionManager + Session

```rust
struct SessionManager {
    store: SessionStoreAdapter,
    active: RwLock<HashMap<SessionId, Arc<Session>>>,
}

impl SessionManager {
    fn new(store: SessionStoreAdapter) -> Self;

    async fn create(&self, work_dir: &Path, title: Option<&str>)
        -> Result<SessionSummary, SessionError>;

    async fn list(&self, filter: SessionFilter)
        -> Result<Vec<SessionSummary>, SessionError>;

    async fn get(&self, id: SessionId)
        -> Result<Arc<Session>, SessionError>;

    async fn close(&self, id: SessionId)
        -> Result<(), SessionError>;
}

struct Session {
    id: SessionId,
    work_dir: PathBuf,
    dir: PathBuf,                    // home_dir/sessions/{workDirKey}/{id}
    state: Mutex<SessionState>,
}

struct SessionState {
    title: Option<String>,
    last_prompt: Option<String>,
    custom: HashMap<String, JsonValue>,
    // wire log 不写；原型只读/写 state.json [C:USER]
}
```

### 2.4 LlmProvider

```rust
trait LlmProvider: Send + Sync {
    // contract: 发送 messages，通过 callback 流式返回 delta；返回 finish reason
    async fn chat_stream(
        &self,
        request: ChatRequest,
        on_delta: &mut dyn FnMut(ChatDelta),
    ) -> Result<FinishReason, LlmError>;
}

struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    tools: Vec<ToolDefinition>,      // 原型只注册 bash [C:INFERRED]
    stream: bool,                    // true
}

struct Message {
    role: Role,                      // system | user | assistant
    content: String,
}

struct ChatDelta {
    index: usize,                    // choice index，固定 0
    content: Option<String>,         // 文本增量
    tool_call: Option<ToolCallDelta>,// 工具调用增量（原型 bash 为一次性）
}

enum FinishReason { Stop, ToolCalls, Length, ContentFilter, Other }
```

### 2.5 Tool Registry + BashTool

```rust
trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> ToolDefinition;

    // contract: 执行 tool；若需要 approval，先通过 event_sink 发 requestApproval
    async fn execute(
        &self,
        session: Arc<Session>,
        tool_call_id: &str,
        args: JsonValue,
        approval: &dyn ApprovalClient,
    ) -> Result<ToolResult, ToolError>;
}

struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    fn with_builtin() -> Self;        // 注册 BashTool [C:INFERRED]
    fn get(&self, name: &str) -> Option<&dyn Tool>;
}

struct BashTool;

impl Tool for BashTool {
    fn name(&self) -> &str { "bash" }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "bash",
            description: "Execute a shell command after user approval.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "timeout_ms": { "type": "integer", "default": 30000 }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(
        &self,
        session: Arc<Session>,
        tool_call_id: &str,
        args: JsonValue,
        approval: &dyn ApprovalClient,
    ) -> Result<ToolResult, ToolError> {
        let command = args["command"].as_str().ok_or(ToolError::InvalidArgs)?;
        let req = ApprovalRequest {
            tool_call_id: tool_call_id.to_string(),
            tool_name: "bash".to_string(),
            action: format!("Execute: {}", command),
            display: ToolInputDisplay { command: command.to_string() },
        };
        let resp = approval.request(req).await?;
        if resp.decision != ApprovalDecision::Approved {
            return Ok(ToolResult {
                output: "User declined.".to_string(),
                is_error: false,
            });
        }
        execute_shell(command, session.work_dir()).await
    }
}
```

### 2.6 EventSink

```rust
trait EventSink: Send + Sync {
    // contract: 将 AgentEvent 序列化为 JSON 并通过 transport 发送给 TUI
    fn emit(&self, event: AgentEvent);
}

// 在 prompt/toll 流程中，emit 是同步触发；实际 transport 发送是异步的。
```

---

## 3. Algorithms

### 3.1 `CoreHost::dispatch` — CoreAPI 请求分发

> Note: 这是 RPC wrapper 解析后的业务分发。transport 层收到的原始 bytes 是 `{ method: string, args: [payload] }`（见 transport.md §2.1/§3.1）。

```
INPUT: method: string, payload: JsonValue
OUTPUT: response_value: JsonValue or RpcError

1. MATCH method:
   - "getCoreInfo":
       return json({ version, capabilities: ["chat", "bash"] })
   - "createSession":
       req = deserialize::<CreateSessionPayload>(payload)
       summary = self.sessions.create(&req.workDir, req.title.as_deref()).await?
       return json(summary)
   - "listSessions":
       req = deserialize::<ListSessionsPayload>(payload)
       list = self.sessions.list(SessionFilter::from(req)).await?
       return json(list)
   - "closeSession":
       req = deserialize::<CloseSessionPayload>(payload)
       self.sessions.close(req.sessionId).await?
       return json(null)
   - "prompt":
       req = deserialize::<PromptPayload>(payload)
       session = self.sessions.get(req.sessionId).await?
       spawn_background(self.handle_prompt(session, req.agentId, req.input))
       return json(null)              // prompt 异步，结果通过 emitEvent
   - else:
       return RpcError::MethodNotImplemented
2. On any deserialization or business error, return RpcError with code/message.
```

### 3.2 `SessionManager::create` — 创建会话并落盘

```
INPUT: work_dir: Path, title: Option<string>
OUTPUT: SessionSummary

1. session_id = generate_uuid_v7()
2. normalized_work_dir = normalize_work_dir(work_dir)
3. dir = home_dir / "sessions" / encode_work_dir_key(normalized_work_dir) / session_id
4. IF dir exists:
       fail SESSION_ALREADY_EXISTS
5. mkdir(dir, mode=0o700)
6. state = SessionState {
       title,
       last_prompt: None,
       custom: {},
   }
7. write_file(dir / "state.json", json_pretty(state))
8. append_session_index(home_dir, {
       sessionId: session_id,
       sessionDir: dir,
       workDir: normalized_work_dir,
   })
9. summary = build_summary(session_id, dir, normalized_work_dir)
10. RETURN summary
```

### 3.3 `handle_prompt` — 单次 prompt 的完整异步流程

```
INPUT: session: Arc<Session>, agent_id: AgentId, input: PromptInput
SIDE EFFECTS: emits AgentEvent stream, may call requestApproval

1. Update session state: last_prompt = input.text; write state.json.
2. self.event_sink.emit(AgentEvent::UserMessage {
       session_id: session.id,
       agent_id,
       content: input.text.clone(),
   })
3. messages = build_messages(session, input.text)   // system + user
4. request = ChatRequest {
       model: session.model(),
       messages,
       tools: self.tools.definitions(),
       stream: true,
   }
5. accumulated = ""
6. self.provider.chat_stream(request, |delta| {
       if let Some(text) = delta.content {
           accumulated += &text;
           self.event_sink.emit(AgentEvent::AssistantDelta {
               session_id: session.id,
               agent_id,
               delta: text,
           });
       }
       if let Some(tc) = delta.tool_call {
           // prototype: buffer single tool call, no streaming tool args
           buffered_tool_call = Some(tc);
       }
   }).await?
7. self.event_sink.emit(AgentEvent::AssistantFinish {
       session_id: session.id,
       agent_id,
       finish_reason,
   })
8. IF buffered_tool_call exists:
       tool = self.tools.get(&buffered_tool_call.name).ok_or(...)?
       result = tool.execute(session, &buffered_tool_call.id,
                             buffered_tool_call.arguments, approval_client).await?
       self.event_sink.emit(AgentEvent::ToolResult {
           session_id: session.id,
           agent_id,
           tool_call_id: buffered_tool_call.id,
           output: result.output,
           is_error: result.is_error,
       })
9. RETURN
```

### 3.4 `OpenAiProvider::chat_stream` — SSE 流式解析

```
INPUT: request: ChatRequest, on_delta: callback
OUTPUT: FinishReason

1. http_request = build_openai_request(request)   // POST /v1/chat/completions
2. response = self.client.execute(http_request).await?
3. IF response.status != 200:
       fail LlmError::ApiError(status, body)
4. stream = response.bytes_stream()
5. FOR chunk in stream:
       FOR line in chunk.split("\n"):
           IF line starts with "data: ":
               data = line[6..]
               IF data == "[DONE]": BREAK
               event = json_parse::<SseEvent>(data)
               FOR choice in event.choices:
                   delta = ChatDelta {
                       index: choice.index,
                       content: choice.delta.content,
                       tool_call: choice.delta.tool_calls.first().map(...),
                   }
                   on_delta(delta)
                   IF choice.finish_reason is Some(reason):
                       finish_reason = reason
6. RETURN finish_reason.unwrap_or(FinishReason::Stop)
```

### 3.5 `SessionStoreAdapter::summary_from_dir` — 复用 TS 格式构建 summary

```
INPUT: id: SessionId, dir: Path, work_dir: Path
OUTPUT: SessionSummary

1. dir_stat = stat(dir)
2. state = read_optional_json(dir / "state.json")
3. state_mtime = mtime(dir / "state.json") or 0
4. wire_mtime = mtime(dir / "wire.jsonl") or 0
5. updated_at = max(dir_stat.mtime, state_mtime, wire_mtime)
6. RETURN SessionSummary {
       id,
       workDir: work_dir,
       sessionDir: dir,
       createdAt: dir_stat.birthtime,
       updatedAt: updated_at,
       title: state.title.or_else(|| generated_title(state.last_prompt)),
       lastPrompt: state.last_prompt,
       metadata: state.custom,
   }
```

---

## 4. Call-Site Integration

### 4.1 `rust-ody/crates/ody-host/src/main.rs`

```rust
fn main() -> Result<()> {
    let config = parse_cli_and_load_config()?;   // 见 packaging.md
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let (server, event_sink) = transport::build_transport(config.transport).await?;
        let host = Arc::new(CoreHost::new(config, event_sink)?);
        let router = RpcRouter { host };
        let byte_dispatch = Box::new(move |bytes: &[u8]| {
            let router = router.clone();
            Box::pin(async move { router.route(bytes).await })
                as Pin<Box<dyn Future<Output = _> + Send>>
        });
        server.serve(byte_dispatch).await;
        Ok(())
    })
}
```

### 4.2 `rust-ody/Cargo.toml` 新增 member

```toml
[workspace]
members = ["crates/ody-rust", "crates/ody-crypto", "crates/ody-host"]
```

### 4.3 `packages/agent-core/src/rpc/core-api.ts`（不变，作为契约）

TS 侧继续导出 `CoreAPI` / `SDKAPI` 类型；Rust host 的实现目标是与这些类型子集兼容。

---

## 5. Error Handling（局部）

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| `SessionStoreError::AlreadyExists` | Return `SESSION_ALREADY_EXISTS` RPC error | TUI 显示会话已存在 | 用户换 id |
| `SessionStoreError::NotFound` | Return `SESSION_NOT_FOUND` RPC error | TUI 提示会话不存在 | 用户 listSessions 重新选择 |
| `LlmError::ApiError` | Emit `AgentEvent::Error` with provider message | 该 turn 终止，TUI 显示错误 | 用户重试 prompt |
| `LlmError::StreamParse` | Emit `AgentEvent::Error`，关闭 SSE stream | 该 turn 终止 | 用户重试 |
| `ToolError::InvalidArgs` | Return tool result with `is_error=true` | LLM 收到错误结果 | LLM 可能重试 |
| `ApprovalClient::NoHandler` | Return cancelled decision | Tool 不执行 | TUI 注册 approval handler |
| `HostError::ConfigInvalid` | Print to stderr and exit(1) on startup | Host 无法启动 | 用户修正 config |

---

## 6. Local Test Notes

### Must-pass assertions

1. `cargo test -p ody-host`:
   - `session_store_create_writes_state_json` — 创建会话后 `state.json` 存在且字段与 TS `SessionSummaryStateSchema` 兼容。
   - `session_store_list_returns_created` — 刚创建的会话出现在 `listSessions` 结果中。
   - `session_store_close_removes_from_active` — `closeSession` 后内存 map 清理，磁盘目录保留。
2. `ody-host` integration test（无需真实 LLM key，可用 mock server）:
   - `prompt_emits_user_then_assistant_events` — 调用 `prompt` 后依次收到 `UserMessage`、`AssistantDelta`、`AssistantFinish`。
   - `bash_tool_requests_approval_then_executes` — LLM 返回 tool call 时，host  emit `requestApproval`；模拟批准后执行 shell 并 emit `ToolResult`。
3. Cross-language framing test:
   - TS `createStreamTransport` 与 Rust transport server 互相收发 1000 条 request/response，零丢包、零乱序。

### Must-reject assertions

1. `createSession` with duplicate id returns `SESSION_ALREADY_EXISTS`.
2. `prompt` on unknown `sessionId` returns `SESSION_NOT_FOUND`.
3. `bash` tool without approval registration returns cancelled result.
4. Sending malformed length-prefixed frame closes transport with `TRANSPORT_INVALID_FRAMING`.
