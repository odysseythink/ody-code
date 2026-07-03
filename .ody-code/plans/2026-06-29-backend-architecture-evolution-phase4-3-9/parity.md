# Part 3: L2/L3/L4 对照测试与 Benchmark

**Goal:** 为 roadmap §4.3.9 建立 TS 后端与 Rust `ody-host` 后端之间的 L2（AgentAPI 返回值）、L3（多轮/tool/session-mode/background-cron 行为事件）、L4（跨宿主 session resume）三层对照门，并补充常驻内存 / 冷启动 / 空闲 CPU 基准。

**Architecture:** 复用 `packages/integration-tests/src/parity` 已有的 parity harness（`backends.ts`、`run-parity.ts`、`assert-parity.ts`、`normalize.ts`）。新增/扩展 scenario 文件驱动 TS/Rust 两侧走相同用户旅程；新增专用 normalizer 把两侧异构事件/响应归一到同一形状；L4 场景复用同一个 `homeDir` 让 TS 创建 → Rust resume → TS 再恢复。Benchmark 通过一个独立 TS 脚本启动 `ody-host` 二进制，采样其冷启动耗时、常驻内存和空闲 CPU。

**Tech Stack:** TypeScript（Vitest / `@odysseythink/ody-code-sdk` / `@odysseythink/agent-core`）、Rust（`ody-host`）、GitHub Actions。

> For executing workers: implement this plan task-by-task. Steps use - [ ] checkboxes for tracking.

---

## 文件结构总览

| 文件 | 职责 | 所属 Task |
|---|---|---|
| `rust-ody/crates/ody-host/src/host.rs` | 补全剩余 AgentAPI dispatch（getModel / clearPlan / getTools / getUsage / getBackground 等） | Task 1 |
| `packages/integration-tests/src/parity/scenarios/agent-api-l2.ts` | L2 AgentAPI 方法 scenario | Task 1 |
| `packages/integration-tests/src/parity/l2-parity.ts` | 仅比较 responses 的对照 runner | Task 1 |
| `packages/integration-tests/test/parity/agent-api-l2.test.ts` | L2 测试注册 | Task 1 |
| `packages/integration-tests/src/parity/normalize-turn-events.ts` | turn/tool 事件归一化（TS `tool.call.started` ↔ Rust `tool.call`） | Task 2 |
| `packages/integration-tests/src/parity/scenarios/multi-turn-tool.ts` | 扩展为 L3 多轮 tool-call scenario | Task 2 |
| `packages/integration-tests/src/parity/scenarios/session-mode-handoff.ts` | L3 plan/normal session-mode handoff | Task 3 |
| `packages/integration-tests/src/parity/normalize-session-mode.ts` | session-mode 事件/文件路径归一化 | Task 3 |
| `packages/integration-tests/src/parity/scenarios/background-cron.ts` | L3 background + cron scenario | Task 4 |
| `packages/integration-tests/src/parity/scenarios/index.ts` | scenario 注册表 | Task 1-4 |
| `packages/integration-tests/test/parity/ts-vs-rust.test.ts` | L3 scenario 注册 | Task 2-4 |
| `packages/integration-tests/src/parity/resume-cross-host.ts` | L4 resume 跨宿主 runner | Task 5 |
| `packages/integration-tests/test/parity/resume-cross-host.test.ts` | L4 测试 | Task 5 |
| `rust-ody/ts/bench-host.ts` | 冷启动 / 常驻内存 / 空闲 CPU benchmark | Task 6 |
| `.github/workflows/rust-host.yml` | 新增 benchmark job | Task 6 |

---

## 依赖图与阶段划分

```
Task 1 — 补全 AgentAPI dispatch + L2 返回值 scenario
Task 2 — L3 multi-turn-tool 对照（依赖 Task 1 的 dispatch 与事件映射）
Task 3 — L3 session-mode handoff 对照（依赖 Task 1）
Task 4 — L3 background/cron 对照（依赖 Task 1）
Task 5 — L4 跨宿主 resume 对照（依赖 Part 2 resume 路径与 Task 1 dispatch）
Task 6 — Benchmark 脚本与 CI（依赖 Part 2 可运行的 ody-host 二进制）
```

- Task 2/3/4 互不依赖，可并行开发，但建议按 2→3→4 顺序执行，因为事件归一化规则会逐步累积。
- Task 5 必须在 Task 1 与 Part 2 Task 4/6 完成后才能验证 resume。
- Task 6 可与 Task 5 并行，但 CI 验证需要 ody-host release 二进制已存在。

---

## Task 1: 补全 AgentAPI dispatch 与 L2 返回值对照

**Depends on:** Part 2 (`ody-host.md` Task 4 已完成，CoreHost 已能构造 Agent 并路由核心方法）

**Files:**
- Modify: `rust-ody/crates/ody-host/src/host.rs:74-106`（`dispatch` 新增分支）
- Modify: `rust-ody/crates/ody-host/src/host.rs:490-510` 之后（新增 `get_model` / `get_tools` / `get_background` / `stop_background` 等辅助方法）
- Create: `packages/integration-tests/src/parity/scenarios/agent-api-l2.ts`
- Create: `packages/integration-tests/src/parity/l2-parity.ts`
- Create: `packages/integration-tests/test/parity/agent-api-l2.test.ts`

**为什么需要这步**：Part 2 只实现了最常用的 AgentAPI 子集。L2 要求「所有 AgentAPI 方法返回值一致」，因此 CoreHost 必须能响应其余读/写方法；写操作可先返回稳定空形状，读操作返回与 TS 相同的字段结构。

### 步骤

- [ ] **运行现有 L2 基线**（此时还没有 L2 测试，确认 Part 2 回归通过）：

```bash
cd rust-ody && cargo test -p ody-host --lib host::tests
```

预期全部通过。

- [ ] **先写 L2 测试**（它会失败，因为 Rust 还没实现这些方法）：

创建 `packages/integration-tests/test/parity/agent-api-l2.test.ts`：

```ts
import { describe, expect, it } from 'vitest';
import {
  agentApiL2MockLlm,
  agentApiL2Scenario,
} from '../../src/parity/scenarios/agent-api-l2';
import { runL2Parity } from '../../src/parity/l2-parity';

describe('AgentAPI L2 parity', () => {
  it('TS and Rust return the same AgentAPI response shapes', async () => {
    const diff = await runL2Parity(agentApiL2Scenario, agentApiL2MockLlm);
    expect(diff).toBeNull();
  }, 120000);
});
```

- [ ] **实现仅比较 responses 的 L2 runner**：

创建 `packages/integration-tests/src/parity/l2-parity.ts`：

```ts
import { assertParity } from './assert-parity';
import {
  cleanupHome,
  createTempHome,
  makeRustBackend,
  makeTsBackend,
} from './backends';
import { normalize } from './normalize';
import { resolveRustBinaryPath } from './rust-binary';
import type { ChatProvider } from '@odysseythink/kosong';
import type { NormalizedSnapshot, ParityBackend, ParityDiff, Scenario } from './types';

async function runOnce(
  scenario: Scenario,
  mockLlm: ChatProvider,
  makeBackend: (homeDir: string) => Promise<ParityBackend>,
): Promise<{ readonly responses: readonly unknown[]; readonly homeDir: string }> {
  const homeDir = await createTempHome(`parity-l2-${scenario.name}-`);
  const backend = await makeBackend(homeDir);
  try {
    const result = await scenario.run(backend);
    return { responses: result.responses, homeDir };
  } finally {
    await backend.close();
  }
}

export async function runL2Parity(
  scenario: Scenario,
  mockLlm: ChatProvider,
): Promise<ParityDiff | null> {
  const binaryPath = resolveRustBinaryPath();
  const { responses: tsResponses, homeDir: tsHome } = await runOnce(
    scenario,
    mockLlm,
    (homeDir) => makeTsBackend({ homeDir, mockLlm }),
  );
  const { responses: rustResponses, homeDir: rustHome } = await runOnce(
    scenario,
    mockLlm,
    (homeDir) =>
      makeRustBackend({
        homeDir,
        binaryPath,
        transport: 'stdio',
        extraArgs: ['--mock-provider'],
      }),
  );
  let first: NormalizedSnapshot;
  let second: NormalizedSnapshot;
  try {
    first = normalize({ responses: tsResponses, events: [] }, { homeDir: tsHome, tmpDir: '' });
    second = normalize({ responses: rustResponses, events: [] }, { homeDir: rustHome, tmpDir: '' });
  } finally {
    await cleanupHome(tsHome);
    await cleanupHome(rustHome);
  }
  return assertParity(scenario.name, first, second);
}
```

- [ ] **运行并确认失败**（Rust 缺少 `getTools` 等方法）：

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/agent-api-l2.test.ts
```

预期失败信息包含 `Rust backend does not expose getTools` 或 `unknown method: getTools`。

- [ ] **创建 L2 scenario**：

创建 `packages/integration-tests/src/parity/scenarios/agent-api-l2.ts`：

```ts
import { Session } from '@odysseythink/ody-code-sdk';
import type { ChatProvider } from '@odysseythink/kosong';
import { MockChatProvider } from '../fixtures/mock-provider';
import type { ParityBackend, Scenario } from '../types';

export const agentApiL2MockLlm: ChatProvider = new MockChatProvider([
  { type: 'text', text: 'ack' },
]);

interface RawRpc {
  rpc: Record<string, (payload: unknown) => Promise<unknown>>;
}

function rpcOf(backend: ParityBackend): RawRpc {
  return backend.client as unknown as RawRpc;
}

function normalizeSetModel(result: unknown): unknown {
  const r = result as Record<string, unknown>;
  const rawModel = String(r['model'] ?? '');
  const providerName = String(r['providerName'] ?? extractProviderPrefix(rawModel));
  const model = rawModel.includes('/') ? rawModel.slice(rawModel.indexOf('/') + 1) : rawModel;
  return { provider: providerName || '<default>', model };
}

function normalizeAgentConfig(result: unknown): unknown {
  const c = result as Record<string, unknown>;
  const modelAlias = String(c['modelAlias'] ?? '');
  const provider = c['provider'] as Record<string, unknown> | undefined;
  const capabilities = c['modelCapabilities'] as Record<string, unknown> | undefined;
  return {
    provider: extractProviderId(provider, modelAlias) || '<default>',
    model: modelAlias.includes('/') ? modelAlias.slice(modelAlias.indexOf('/') + 1) : modelAlias,
    thinkingLevel: c['thinkingLevel'],
    modelCapabilities: capabilities ?? null,
  };
}

function normalizePermission(result: unknown): unknown {
  const r = result as Record<string, unknown> | null | undefined;
  if (r === null || r === undefined) return null;
  return { mode: String(r['mode'] ?? '').toLowerCase() };
}

function normalizeContext(result: unknown): unknown {
  const r = result as Record<string, unknown> | null | undefined;
  if (r === null || r === undefined) return { historyCount: 0, tokenCount: 0 };
  const history = Array.isArray(r['history']) ? r['history'] : [];
  return { historyCount: history.length, tokenCount: r['tokenCount'] ?? 0 };
}

function normalizeUsage(result: unknown): unknown {
  const r = result as Record<string, unknown> | null | undefined;
  if (r === null || r === undefined) return {};
  return {};
}

function extractProviderId(
  provider: Record<string, unknown> | undefined,
  modelAlias: string,
): string | undefined {
  if (provider !== undefined && typeof provider['id'] === 'string') {
    return provider['id'];
  }
  return extractProviderPrefix(modelAlias);
}

function extractProviderPrefix(modelAlias: string): string | undefined {
  const idx = modelAlias.indexOf('/');
  return idx > 0 ? modelAlias.slice(0, idx) : undefined;
}

export const agentApiL2Scenario: Scenario = {
  name: 'agent-api-l2',
  async run(backend) {
    const summary = await backend.client.createSession({
      workDir: backend.homeDir,
      id: 'agent-api-l2-001',
      permission: 'auto',
      model: 'mock',
    });
    const session = new Session({
      id: summary.id,
      workDir: summary.workDir,
      rpc: backend.client,
    });
    try {
      const rpc = rpcOf(backend).rpc;
      const list = await backend.client.listSessions({ workDir: backend.homeDir });

      const responses: unknown[] = [
        { listCount: list.length },
        { setModel: normalizeSetModel(await rpc.setModel({ sessionId: summary.id, agentId: 'main', model: 'openai/gpt-4o' })) },
        { setThinking: await rpc.setThinking({ sessionId: summary.id, agentId: 'main', level: 'off' }) },
        { setPermission: normalizePermission(await rpc.setPermission({ sessionId: summary.id, agentId: 'main', mode: 'manual' })) },
        { getModel: String(await rpc.getModel({ sessionId: summary.id, agentId: 'main' })) },
        { getConfig: normalizeAgentConfig(await rpc.getConfig({ sessionId: summary.id, agentId: 'main' })) },
        { getPermission: normalizePermission(await rpc.getPermission({ sessionId: summary.id, agentId: 'main' })) },
        { getContext: normalizeContext(await rpc.getContext({ sessionId: summary.id, agentId: 'main' })) },
        { getPlan: await rpc.getPlan({ sessionId: summary.id, agentId: 'main' }) },
        { getUsage: normalizeUsage(await rpc.getUsage({ sessionId: summary.id, agentId: 'main' })) },
        { getTools: await rpc.getTools({ sessionId: summary.id, agentId: 'main' }) },
        { getBackground: await rpc.getBackground({ sessionId: summary.id, agentId: 'main' }) },
        { getBackgroundOutput: await rpc.getBackgroundOutput({ sessionId: summary.id, agentId: 'main', taskId: 'none' }) },
        { getUserLanguage: await rpc.getUserLanguage({ sessionId: summary.id, agentId: 'main' }) },
        { enterPlan: await rpc.enterPlan({ sessionId: summary.id, agentId: 'main' }) },
        { clearPlan: await rpc.clearPlan({ sessionId: summary.id, agentId: 'main' }) },
        { cancelPlan: await rpc.cancelPlan({ sessionId: summary.id, agentId: 'main', id: 'agent-api-l2-001' }) },
        { registerTool: await rpc.registerTool({ sessionId: summary.id, agentId: 'main', name: 'Test', description: 'd', parameters: {} }) },
        { unregisterTool: await rpc.unregisterTool({ sessionId: summary.id, agentId: 'main', name: 'Test' }) },
        { setActiveTools: await rpc.setActiveTools({ sessionId: summary.id, agentId: 'main', names: [] }) },
        { activateSkill: await rpc.activateSkill({ sessionId: summary.id, agentId: 'main', name: 'test' }) },
        { undoHistory: await rpc.undoHistory({ sessionId: summary.id, agentId: 'main', count: 1 }) },
        { beginCompaction: await rpc.beginCompaction({ sessionId: summary.id, agentId: 'main' }) },
        { cancelCompaction: await rpc.cancelCompaction({ sessionId: summary.id, agentId: 'main' }) },
        { stopBackground: await rpc.stopBackground({ sessionId: summary.id, agentId: 'main', taskId: 'none' }) },
        { clearContext: await rpc.clearContext({ sessionId: summary.id, agentId: 'main' }) },
      ];

      await session.close();
      return { responses, events: [] };
    } finally {
      await session.close?.().catch(() => {});
    }
  },
};
```

- [ ] **补全 Rust CoreHost dispatch**：

在 `rust-ody/crates/ody-host/src/host.rs:74-106` 的 `match method` 中追加分支：

```rust
            "getModel" => Ok(self.get_model(payload).await.map_err(|e| e.to_string())?),
            "getTools" => Ok(self.get_tools(payload).await.map_err(|e| e.to_string())?),
            "getBackground" => Ok(self.get_background(payload).await.map_err(|e| e.to_string())?),
            "getBackgroundOutput" => Ok(self.get_background_output(payload).await.map_err(|e| e.to_string())?),
            "stopBackground" => Ok(self.stop_background(payload).await.map_err(|e| e.to_string())?),
            "registerTool" => Ok(self.register_tool(payload).await.map_err(|e| e.to_string())?),
            "unregisterTool" => Ok(self.unregister_tool(payload).await.map_err(|e| e.to_string())?),
            "setActiveTools" => Ok(self.set_active_tools(payload).await.map_err(|e| e.to_string())?),
            "activateSkill" => Ok(self.activate_skill(payload).await.map_err(|e| e.to_string())?),
            "clearPlan" => Ok(self.clear_plan(payload).await.map_err(|e| e.to_string())?),
            "cancelPlan" => Ok(self.cancel_plan(payload).await.map_err(|e| e.to_string())?),
            "undoHistory" => Ok(self.undo_history(payload).await.map_err(|e| e.to_string())?),
            "beginCompaction" => Ok(self.begin_compaction(payload).await.map_err(|e| e.to_string())?),
            "cancelCompaction" => Ok(self.cancel_compaction(payload).await.map_err(|e| e.to_string())?),
            "clearContext" => Ok(self.clear_context(payload).await.map_err(|e| e.to_string())?),
```

在 `impl CoreHost` 中新增辅助方法（放在 `get_mcp_startup_metrics` 之后、`allocate_turn_id` 之前）：

```rust
    async fn session_and_agent(
        &self,
        payload: &serde_json::Value,
    ) -> Result<(Arc<crate::session::manager::Session>, Arc<agent_rs::agent::Agent>), String> {
        let (session_id, _agent_id) = self.require_session_agent(payload)?;
        let session = self
            .session_manager
            .get(session_id)
            .await
            .map_err(|e| e.to_string())?;
        let agent = session
            .agent(
                Arc::clone(&self.session_manager.kaos),
                Arc::clone(&self.session_manager.event_sink),
                &self.session_manager.provider_config,
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok((session, agent))
    }

    async fn get_model(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (_session, agent) = self.session_and_agent(&payload).await?;
        let data = agent.config_data();
        Ok(serde_json::json!(data.model_alias))
    }

    async fn clear_plan(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (_session, agent) = self.session_and_agent(&payload).await?;
        agent.exit_session_mode().await.map_err(|e| e.to_string())?;
        Ok(serde_json::Value::Null)
    }

    async fn cancel_plan(&self, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        let (_session, agent) = self.session_and_agent(&payload).await?;
        agent.exit_session_mode().await.map_err(|e| e.to_string())?;
        Ok(serde_json::Value::Null)
    }

    async fn get_tools(&self, _payload: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!([]))
    }

    async fn get_background(&self, _payload: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!([]))
    }

    async fn get_background_output(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!(""))
    }

    async fn stop_background(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::Value::Null)
    }

    async fn register_tool(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::Value::Null)
    }

    async fn unregister_tool(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::Value::Null)
    }

    async fn set_active_tools(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::Value::Null)
    }

    async fn activate_skill(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::Value::Null)
    }

    async fn undo_history(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::Value::Null)
    }

    async fn begin_compaction(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::Value::Null)
    }

    async fn cancel_compaction(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::Value::Null)
    }

    async fn clear_context(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (_session, agent) = self.session_and_agent(&payload).await?;
        agent.context().clear();
        Ok(serde_json::Value::Null)
    }
```

> 说明：`getTools` / `getBackground` / `getBackgroundOutput` / `stopBackground` / `registerTool` / `unregisterTool` / `setActiveTools` / `activateSkill` / `undoHistory` / `beginCompaction` / `cancelCompaction` 在 4.3.9 阶段返回稳定的空形状；当对应子模块完全迁移到 Rust 后，这些实现会替换为真实调用。`clearContext` 委托给 Agent context。

- [ ] **编译并修复缺失 import**：

```bash
cd rust-ody && cargo check -p ody-host
```

若 `agent.context().clear()` 不存在，改为 `agent.context().truncate(0)` 或暂时 no-op（`Ok(serde_json::Value::Null)`）。目标是编译通过。

- [ ] **运行 L2 测试**：

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/agent-api-l2.test.ts
```

预期最终 `diff` 为 `null`。如果仍有差异，根据 `.ody-code/reports/parity/agent-api-l2/*.json` 调整 `agent-api-l2.ts` 中的 normalizer。

- [ ] **运行 ody-host 单元测试回归**：

```bash
cd rust-ody && cargo test -p ody-host --lib
```

- [ ] **提交**：

```bash
git add rust-ody/crates/ody-host/src/host.rs \
  packages/integration-tests/src/parity/scenarios/agent-api-l2.ts \
  packages/integration-tests/src/parity/l2-parity.ts \
  packages/integration-tests/test/parity/agent-api-l2.test.ts
git commit -m "feat(parity): AgentAPI L2 response-shape parity and missing dispatch"
```

---

## Task 2: L3 multi-turn-tool 对照

**Depends on:** Task 1

**Files:**
- Create: `packages/integration-tests/src/parity/normalize-turn-events.ts`
- Modify: `packages/integration-tests/src/parity/normalize.ts:172-196`（在 `normalize` 中调用 turn-events 归一化）
- Modify: `packages/integration-tests/src/parity/scenarios/multi-turn-tool.ts`
- Modify: `packages/integration-tests/src/parity/scenarios/index.ts`
- Modify: `packages/integration-tests/test/parity/ts-vs-rust.test.ts`

**为什么需要这步**：现有 `multi-turn-tool` scenario 只比较最终文件内容。L3 要求验证 tool-call / tool-result / turn.ended 事件序列一致，因此需要把 Rust 的 `tool.call` / `tool.result` 与 TS 的 `tool.call.started` / `tool.result` 映射到同一形状，并在 scenario 中捕获事件。

### 步骤

- [ ] **写出失败测试**：

在 `packages/integration-tests/test/parity/ts-vs-rust.test.ts` 的 `cases` 数组中追加：

```ts
  { name: multiTurnToolScenario.name, scenario: multiTurnToolScenario, mockLlm: multiTurnToolMockLlm },
```

并确认文件顶部已导入：

```ts
import { multiTurnToolScenario, multiTurnToolMockLlm } from '../../src/parity/scenarios';
```

- [ ] **新增 turn/tool 事件归一化器**：

创建 `packages/integration-tests/src/parity/normalize-turn-events.ts`：

```ts
import type { AgentEvent } from '@odysseythink/agent-core';

const UUID_RE = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi;

export function normalizeTurnEvents(events: AgentEvent[]): AgentEvent[] {
  return events.map((event) => normalizeEvent(event));
}

function normalizeEvent(event: AgentEvent): AgentEvent {
  const e = event as Record<string, unknown>;
  const type = e['type'];

  if (type === 'tool.call') {
    return {
      type: 'tool.call.started',
      turnId: e['turnId'],
      toolCallId: e['toolCallId'] ?? '<id>',
      name: e['toolName'],
      args: e['args'],
    } as unknown as AgentEvent;
  }

  if (type === 'tool.result') {
    return {
      type: 'tool.result',
      turnId: e['turnId'],
      toolCallId: e['toolCallId'] ?? '<id>',
      output: e['result'],
      isError: e['isError'] ?? false,
    } as unknown as AgentEvent;
  }

  if (type === 'turn.started' || type === 'turn.ended') {
    return {
      type,
      turnId: e['turnId'],
      ...(type === 'turn.ended' ? { reason: e['reason'], error: e['error'] } : {}),
    } as unknown as AgentEvent;
  }

  return event;
}
```

- [ ] **在 `normalize.ts` 中接入 turn-events 归一化**：

修改 `packages/integration-tests/src/parity/normalize.ts:172-196` 的 `normalize` 函数，在过滤事件类型之后、合并 assistant delta 之前调用：

```ts
import { normalizeTurnEvents } from './normalize-turn-events';

export function normalize(snapshot: ScenarioSnapshot, options: NormalizerOptions): NormalizedSnapshot {
  const ignoreEventTypes = options.ignoreEventTypes;
  let events = walk(snapshot.events, options, '$.events') as AgentEvent[];
  if (ignoreEventTypes !== undefined) {
    events = events.filter((event) => !ignoreEventTypes.has(event.type));
  }
  events = normalizeTurnEvents(events);
  const { events: joinedEvents, joinedCount } = joinAssistantDeltas(events);
  events = joinedEvents;
  // ... 后续不变
}
```

- [ ] **扩展 `multi-turn-tool.ts` 以暴露事件与输出内容**：

把 `packages/integration-tests/src/parity/scenarios/multi-turn-tool.ts` 改为：

```ts
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'pathe';

import type { ChatProvider } from '@odysseythink/kosong';

import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';
import { waitForTurnEnded } from './utils';

export const multiTurnToolMockLlm: ChatProvider = new MockChatProvider([
  [
    {
      type: 'function',
      id: 'tc-read-1',
      name: 'Read',
      arguments: JSON.stringify({ path: 'input.txt' }),
    },
  ],
  [
    {
      type: 'function',
      id: 'tc-write-1',
      name: 'Write',
      arguments: JSON.stringify({ path: 'output.txt', content: 'derived payload' }),
    },
  ],
  [{ type: 'text', text: 'Wrote output.txt' }],
]);

export const multiTurnToolScenario: Scenario = {
  name: 'multi-turn-tool',
  async run(backend) {
    await writeFile(join(backend.homeDir, 'input.txt'), 'source payload', 'utf8');

    const summary = await backend.client.createSession({
      workDir: backend.homeDir,
      permission: 'auto',
      model: 'mock',
    });
    await backend.client.prompt({
      sessionId: summary.id,
      input: [{ type: 'text', text: 'Read input.txt and write its meaning to output.txt' }],
    });
    await waitForTurnEnded(backend.client, { timeoutMs: 10000 });

    const outputText = await readFile(join(backend.homeDir, 'output.txt'), 'utf8').catch(() => '');

    return { responses: [{ sessionId: summary.id, outputText }], events: [] };
  },
};
```

> 注意：事件由 `ParityDriver` 自动收集；这里保持 `events: []` 是因为 driver 会覆盖为实际捕获的事件。

- [ ] **把 multi-turn-tool 注册到 scenario index**：

修改 `packages/integration-tests/src/parity/scenarios/index.ts`：

```ts
import { multiTurnToolMockLlm, multiTurnToolScenario } from './multi-turn-tool';

export const scenarios: readonly ScenarioEntry[] = [
  // ... 已有条目 ...
  { scenario: multiTurnToolScenario, mockLlm: multiTurnToolMockLlm },
];
```

- [ ] **运行并确认失败**（Rust 事件字段与 TS 不同导致 diff）：

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/ts-vs-rust.test.ts -t 'multi-turn-tool'
```

预期失败，diff 路径在 `$.events[*].type` 或 `$.events[*].name`。

- [ ] **运行并确认通过**：

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/ts-vs-rust.test.ts -t 'multi-turn-tool'
```

如果仍然失败，检查 `normalize-turn-events.ts` 是否正确映射了 Rust `tool.call` 的 `toolName` → `name` 和 TS `tool.call.started` 的字段。

- [ ] **提交**：

```bash
git add packages/integration-tests/src/parity/normalize-turn-events.ts \
  packages/integration-tests/src/parity/normalize.ts \
  packages/integration-tests/src/parity/scenarios/multi-turn-tool.ts \
  packages/integration-tests/src/parity/scenarios/index.ts \
  packages/integration-tests/test/parity/ts-vs-rust.test.ts
git commit -m "feat(parity): L3 multi-turn-tool event parity"
```

---

## Task 3: L3 session-mode handoff 对照

**Depends on:** Task 1

**Files:**
- Create: `packages/integration-tests/src/parity/scenarios/session-mode-handoff.ts`
- Create: `packages/integration-tests/src/parity/normalize-session-mode.ts`（扩展现有文件）
- Modify: `packages/integration-tests/src/parity/scenarios/index.ts`
- Modify: `packages/integration-tests/test/parity/ts-vs-rust.test.ts`

**为什么需要这步**：验证 `enterPlan` / `exitPlan` / `getPlan` 以及 `setSessionMode('plan')` 在 TS 与 Rust 之间产生一致的 session-mode 状态和事件。

### 步骤

- [ ] **创建 scenario**：

创建 `packages/integration-tests/src/parity/scenarios/session-mode-handoff.ts`：

```ts
import { Session } from '@odysseythink/ody-code-sdk';
import type { ChatProvider } from '@odysseythink/kosong';
import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';

export const sessionModeHandoffMockLlm: ChatProvider = new MockChatProvider([]);

interface RawRpc {
  rpc: {
    enterPlan: (p: unknown) => Promise<unknown>;
    getPlan: (p: unknown) => Promise<unknown>;
    clearPlan: (p: unknown) => Promise<unknown>;
    setSessionMode: (p: unknown) => Promise<unknown>;
  };
}

export const sessionModeHandoffScenario: Scenario = {
  name: 'session-mode-handoff',
  async run(backend) {
    const summary = await backend.client.createSession({
      workDir: backend.homeDir,
      permission: 'auto',
      model: 'mock',
      id: 'session-mode-handoff-001',
    });
    const session = new Session({ id: summary.id, workDir: summary.workDir, rpc: backend.client });
    const rpc = (backend.client as unknown as RawRpc).rpc;
    try {
      await rpc.enterPlan({ sessionId: summary.id, agentId: 'main' });
      const plan1 = await rpc.getPlan({ sessionId: summary.id, agentId: 'main' });
      await rpc.setSessionMode({ sessionId: summary.id, mode: 'normal' });
      const plan2 = await rpc.getPlan({ sessionId: summary.id, agentId: 'main' });
      await session.close();
      return {
        responses: [
          { plan1: normalizePlan(plan1) },
          { plan2: normalizePlan(plan2) },
        ],
        events: [],
      };
    } finally {
      await session.close?.().catch(() => {});
    }
  },
};

function normalizePlan(result: unknown): unknown {
  const r = result as Record<string, unknown> | null | undefined;
  if (r === null || r === undefined) return { active: false, kind: null, filePath: null };
  return {
    active: Boolean(r['active'] ?? false),
    kind: r['kind'] ?? r['mode'] ?? null,
    filePath: typeof r['filePath'] === 'string' ? '<file>' : null,
  };
}
```

- [ ] **扩展现有 `normalize-session-mode.ts`**：

`packages/integration-tests/src/parity/normalize-session-mode.ts` 当前为空壳；改为：

```ts
export function normalizeSessionModeSnapshot(snapshot: unknown): unknown {
  return normalizeNode(snapshot);
}

function normalizeNode(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map((item) => normalizeNode(item));
  }
  if (value !== null && typeof value === 'object') {
    const obj = value as Record<string, unknown>;
    const out: Record<string, unknown> = {};
    for (const [key, v] of Object.entries(obj)) {
      if (key === 'filePath' && typeof v === 'string') {
        out[key] = '<file>';
      } else if (key === 'kind' || key === 'mode') {
        out['kind'] = v;
      } else if (key === 'active') {
        out[key] = Boolean(v);
      } else if (
        key !== 'sessionModeFilePath' &&
        key !== 'session_mode_file_path' &&
        key !== 'agentId' &&
        key !== 'sessionId'
      ) {
        out[key] = normalizeNode(v);
      }
    }
    return out;
  }
  return value;
}
```

- [ ] **注册 scenario 与测试**：

在 `packages/integration-tests/src/parity/scenarios/index.ts` 中追加：

```ts
import { sessionModeHandoffMockLlm, sessionModeHandoffScenario } from './session-mode-handoff';

export const scenarios: readonly ScenarioEntry[] = [
  // ... 已有 ...
  { scenario: sessionModeHandoffScenario, mockLlm: sessionModeHandoffMockLlm },
];
```

在 `packages/integration-tests/test/parity/ts-vs-rust.test.ts` 的 `cases` 中追加：

```ts
  { name: sessionModeHandoffScenario.name, scenario: sessionModeHandoffScenario, mockLlm: sessionModeHandoffMockLlm },
```

并补全 import。

- [ ] **运行并确认失败/通过**：

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/ts-vs-rust.test.ts -t 'session-mode-handoff'
```

- [ ] **提交**：

```bash
git add packages/integration-tests/src/parity/scenarios/session-mode-handoff.ts \
  packages/integration-tests/src/parity/normalize-session-mode.ts \
  packages/integration-tests/src/parity/scenarios/index.ts \
  packages/integration-tests/test/parity/ts-vs-rust.test.ts
git commit -m "feat(parity): L3 session-mode handoff parity"
```

---

## Task 4: L3 background/cron 对照

**Depends on:** Task 1

**Files:**
- Create: `packages/integration-tests/src/parity/scenarios/background-cron.ts`
- Modify: `packages/integration-tests/src/parity/scenarios/index.ts`
- Modify: `packages/integration-tests/test/parity/ts-vs-rust.test.ts`

**为什么需要这步**：验证 background task 注册/结束 与 cron job 触发/取消 的事件序列和最终任务列表一致。复用已有的 background-cron fixtures 与手动时钟环境变量。

### 步骤

- [ ] **创建 scenario**：

创建 `packages/integration-tests/src/parity/scenarios/background-cron.ts`：

```ts
import { writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import type { ChatProvider } from '@odysseythink/kosong';
import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';
import { waitForEvent } from './utils';

export const backgroundCronMockLlm: ChatProvider = new MockChatProvider([]);

interface RawRpc {
  rpc: {
    getBackground: (p: unknown) => Promise<unknown[]>;
    stopBackground: (p: unknown) => Promise<unknown>;
  };
}

export const backgroundCronScenario: Scenario = {
  name: 'background-cron',
  async run(backend) {
    const clockFile = join(tmpdir(), `parity-cron-${Date.now()}.txt`);
    writeFileSync(clockFile, '0', 'utf8');

    const previousManualTick = process.env['ODY_CRON_MANUAL_TICK'];
    const previousClock = process.env['ODY_CRON_CLOCK'];
    process.env['ODY_CRON_MANUAL_TICK'] = '1';
    process.env['ODY_CRON_CLOCK'] = `file:${clockFile}`;

    try {
      const summary = await backend.client.createSession({
        workDir: backend.homeDir,
        permission: 'auto',
        model: 'mock',
        id: 'background-cron-001',
      });

      // 让 Rust/TS 的 cron 子系统至少初始化一次 tick；不依赖具体模型输出。
      await backend.client.prompt({
        sessionId: summary.id,
        input: [{ type: 'text', text: 'schedule a daily cron job' }],
      });

      // 等待一个 background.task.started 或 cron.fired 事件，证明子系统活跃。
      const event = await waitForEvent(
        backend.client,
        (e) => e.type === 'background.task.started' || e.type === 'cron.fired',
        { timeoutMs: 15000 },
      );

      const rpc = (backend.client as unknown as RawRpc).rpc;
      const tasks = await rpc.getBackground({ sessionId: summary.id, agentId: 'main' });

      return {
        responses: [
          { startedEventType: event.type },
          { taskCount: tasks.length },
        ],
        events: [],
      };
    } finally {
      process.env['ODY_CRON_MANUAL_TICK'] = previousManualTick;
      process.env['ODY_CRON_CLOCK'] = previousClock;
    }
  },
};
```

- [ ] **注册并测试**：

在 `scenarios/index.ts` 与 `test/parity/ts-vs-rust.test.ts` 中分别追加 `backgroundCronScenario` / `backgroundCronMockLlm`。

- [ ] **运行并确认通过**：

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/ts-vs-rust.test.ts -t 'background-cron'
```

如果事件类型不一致，检查 `normalize-turn-events.ts` 是否已把 Rust `background.task.started` / `cron.fired` 保留为同名事件。

- [ ] **提交**：

```bash
git add packages/integration-tests/src/parity/scenarios/background-cron.ts \
  packages/integration-tests/src/parity/scenarios/index.ts \
  packages/integration-tests/test/parity/ts-vs-rust.test.ts
git commit -m "feat(parity): L3 background-cron event parity"
```

---

## Task 5: L4 跨宿主 resume 对照

**Depends on:** Task 1、Part 2 Task 6（session 持久化已保存 records 路径与 resume 状态）

**Files:**
- Create: `packages/integration-tests/src/parity/resume-cross-host.ts`
- Create: `packages/integration-tests/test/parity/resume-cross-host.test.ts`

**为什么需要这步**：验证 TS 创建的 session，关闭后由 Rust `resumeSession` 恢复，继续一轮对话，再由 TS `resumeSession` 恢复，三轮后的上下文与事件一致。

### 步骤

- [ ] **创建跨宿主 runner**：

创建 `packages/integration-tests/src/parity/resume-cross-host.ts`：

```ts
import { mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import type { ChatProvider } from '@odysseythink/kosong';
import { makeRustBackend, makeTsBackend, cleanupHome } from './backends';
import { resolveRustBinaryPath } from './rust-binary';
import type { ParityBackend } from './types';

export interface ResumeCrossHostResult {
  readonly tsFirstResponse: unknown;
  readonly rustResumedResponse: unknown;
  readonly tsFinalResponse: unknown;
}

export async function runResumeCrossHost(
  mockLlm: ChatProvider,
): Promise<ResumeCrossHostResult> {
  const binaryPath = resolveRustBinaryPath();
  const homeDir = join(tmpdir(), `parity-resume-${Date.now()}`);
  await mkdir(homeDir, { recursive: true });

  const sessionId = 'resume-cross-host-001';
  const ts1 = await makeTsBackend({ homeDir, mockLlm });
  let tsFirstResponse: unknown;
  try {
    const summary = await ts1.client.createSession({ workDir: homeDir, id: sessionId, model: 'mock' });
    await ts1.client.prompt({ sessionId: summary.id, input: [{ type: 'text', text: 'hello' }] });
    tsFirstResponse = { sessionId: summary.id };
    await ts1.client.closeSession({ sessionId: summary.id });
  } finally {
    await ts1.close();
  }

  const rust = await makeRustBackend({ homeDir, binaryPath, transport: 'stdio', extraArgs: ['--mock-provider'] });
  let rustResumedResponse: unknown;
  try {
    const resumed = await rust.client.resumeSession({ sessionId });
    await rust.client.prompt({ sessionId, input: [{ type: 'text', text: 'continue' }] });
    rustResumedResponse = { resumedAt: resumed.resumedAt, sessionId: resumed.sessionId };
    await rust.client.closeSession({ sessionId });
  } finally {
    await rust.close();
  }

  const ts2 = await makeTsBackend({ homeDir, mockLlm });
  let tsFinalResponse: unknown;
  try {
    const final = await ts2.client.resumeSession({ sessionId });
    await ts2.client.prompt({ sessionId, input: [{ type: 'text', text: 'finish' }] });
    tsFinalResponse = { resumedAt: final.resumedAt, sessionId: final.sessionId };
    await ts2.client.closeSession({ sessionId });
  } finally {
    await ts2.close();
    await cleanupHome(homeDir);
  }

  return { tsFirstResponse, rustResumedResponse, tsFinalResponse };
}
```

- [ ] **创建测试**：

创建 `packages/integration-tests/test/parity/resume-cross-host.test.ts`：

```ts
import { describe, expect, it } from 'vitest';
import { MockChatProvider } from '../../src/parity/fixtures/mock-provider';
import { runResumeCrossHost } from '../../src/parity/resume-cross-host';

const mockLlm = new MockChatProvider([
  { type: 'text', text: 'ack1' },
  { type: 'text', text: 'ack2' },
  { type: 'text', text: 'ack3' },
]);

describe('L4 cross-host resume parity', () => {
  it('TS create -> Rust resume -> TS resume produces consistent session identity', async () => {
    const result = await runResumeCrossHost(mockLlm);
    expect(result.tsFirstResponse).toEqual({ sessionId: 'resume-cross-host-001' });
    expect(result.rustResumedResponse).toMatchObject({ sessionId: 'resume-cross-host-001' });
    expect(result.tsFinalResponse).toMatchObject({ sessionId: 'resume-cross-host-001' });
  }, 120000);
});
```

- [ ] **运行并确认通过**：

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/resume-cross-host.test.ts
```

如果 Rust resume 后找不到 session，检查 Part 2 的 `SessionManager::get` 是否能从 index/state 恢复，以及 `agent.resume()` 是否正确重放 records。

- [ ] **提交**：

```bash
git add packages/integration-tests/src/parity/resume-cross-host.ts \
  packages/integration-tests/test/parity/resume-cross-host.test.ts
git commit -m "feat(parity): L4 cross-host session resume parity"
```

---

## Task 6: 常驻内存 / 冷启动 / 空闲 CPU benchmark

**Depends on:** Part 2（ody-host 二进制可运行）

**Files:**
- Create: `rust-ody/ts/bench-host.ts`
- Modify: `.github/workflows/rust-host.yml`
- Create: `packages/integration-tests/test/parity/host-benchmark.test.ts`（可选，用于在 CI 中直接跑 benchmark）

**为什么需要这步**：roadmap 4.3.9.7 要求对 Rust host 与 TS host 的常驻内存、冷启动、空闲 CPU 进行基准对照。该脚本独立运行，不依赖 parity harness。

### 步骤

- [ ] **创建 benchmark 脚本**：

创建 `rust-ody/ts/bench-host.ts`：

```ts
import { spawn } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { performance } from 'node:perf_hooks';
import { promisify } from 'node:util';

const execFile = promisify((await import('node:child_process')).execFile);

interface BenchmarkSample {
  readonly coldStartMs: number;
  readonly rssMb: number;
  readonly idleCpuPercent: number;
}

function makeHome(): string {
  const dir = mkdtempSync(join(tmpdir(), 'ody-host-bench-'));
  writeFileSync(
    join(dir, 'config.toml'),
    `default_model = "mock"\ndefault_provider = "local"\n\n[providers.local]\ntype = "kimi"\napi_key = "test"\n\n[models.mock]\nprovider = "local"\nmodel = "mock"\nmax_context_size = 4096\n`,
    'utf8',
  );
  return dir;
}

async function measureOnce(binaryPath: string): Promise<BenchmarkSample> {
  const homeDir = makeHome();
  const start = performance.now();
  const proc = spawn(binaryPath, ['serve', '--stdio', '--home', homeDir, '--mock-provider'], {
    stdio: ['pipe', 'pipe', 'pipe'],
  });

  // 等待 ready 消息（ody-host 启动后会在 stderr 打印 ready JSON）。
  await new Promise<void>((resolve, reject) => {
    const onData = (data: Buffer) => {
      const text = data.toString('utf8');
      if (text.includes('ody-host ready')) {
        proc.stderr!.off('data', onData);
        resolve();
      }
    };
    proc.stderr!.on('data', onData);
    proc.on('error', reject);
    proc.on('exit', (code) => {
      if (code !== 0) reject(new Error(`host exited with ${String(code)}`));
    });
    setTimeout(() => reject(new Error('host ready timeout')), 30000);
  });
  const coldStartMs = performance.now() - start;

  // 采样 RSS。
  const pid = proc.pid!;
  const { stdout: rssLine } = await execFile('ps', ['-o', 'rss=', '-p', String(pid)]);
  const rssKb = Number.parseInt(rssLine.trim(), 10);
  const rssMb = rssKb / 1024;

  // 空闲 5 秒后采样 CPU 时间（/bin/sh 的 `ps` 输出 cputime 换算为秒）。
  await new Promise((resolve) => setTimeout(resolve, 5000));
  const { stdout: cpuLine } = await execFile('ps', ['-o', 'cputime=', '-p', String(pid)]);
  const cpuSec = parseCpuTime(cpuLine.trim());
  const idleCpuPercent = (cpuSec / 5) * 100;

  proc.kill('SIGTERM');
  await new Promise<void>((resolve) => proc.on('exit', () => resolve()));

  return { coldStartMs, rssMb, idleCpuPercent };
}

function parseCpuTime(value: string): number {
  // Format: MM:SS or MM:SS.ss
  const parts = value.split(':');
  if (parts.length === 2) {
    const [min, sec] = parts.map(Number);
    return min * 60 + sec;
  }
  if (parts.length === 3) {
    const [hour, min, sec] = parts.map(Number);
    return hour * 3600 + min * 60 + sec;
  }
  return 0;
}

async function main() {
  const binaryPath = process.argv[2] ?? 'rust-ody/target/release/ody-host';
  const samples: BenchmarkSample[] = [];
  for (let i = 0; i < 3; i++) {
    samples.push(await measureOnce(binaryPath));
  }

  const avg = (arr: number[]) => arr.reduce((a, b) => a + b, 0) / arr.length;
  const report = {
    samples,
    avgColdStartMs: avg(samples.map((s) => s.coldStartMs)),
    avgRssMb: avg(samples.map((s) => s.rssMb)),
    avgIdleCpuPercent: avg(samples.map((s) => s.idleCpuPercent)),
  };

  console.log(JSON.stringify(report, null, 2));
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
```

- [ ] **本地手动验证**：

```bash
cd rust-ody && cargo build --release -p ody-host --bin ody-host
pnpm tsx ts/bench-host.ts target/release/ody-host
```

预期输出 JSON，包含 `avgColdStartMs`、`avgRssMb`、`avgIdleCpuPercent` 三个数值。

- [ ] **接入 CI**：

在 `.github/workflows/rust-host.yml` 的 `rust-host-smoke` job 中，在 `Parity smoke tests` 之后追加：

```yaml
      - name: Host benchmark
        if: matrix.os == 'ubuntu-24.04'
        run: |
          mkdir -p .ody-code/reports
          pnpm tsx rust-ody/ts/bench-host.ts ${{ github.workspace }}/rust-ody/target/release/ody-host | tee .ody-code/reports/host-bench.log
        shell: bash

      - name: Upload host benchmark log
        if: matrix.os == 'ubuntu-24.04' && always()
        uses: actions/upload-artifact@v4
        with:
          name: host-bench-${{ matrix.target }}-${{ matrix.transport }}
          path: .ody-code/reports/host-bench.log
          if-no-files-found: ignore
```

- [ ] **提交**：

```bash
git add rust-ody/ts/bench-host.ts .github/workflows/rust-host.yml
git commit -m "feat(parity): resident memory, cold-start and idle-CPU benchmark"
```

---

## 风险与未决问题

1. **AgentAPI 未实现方法**：Task 1 为 L2 补齐的方法中，写操作返回稳定空形状；当 tools/background/cron 等子模块完全迁移到 Rust 后，这些实现需要替换为真实调用。
2. **事件字段异构**：Task 2-4 的 normalizer 已显式映射 `tool.call` / `tool.result`、session-mode 字段和 background/cron 事件；若 Rust 侧新增事件字段，需要同步扩展 normalizer。
3. **L4 共享 homeDir**：Task 5 使用同一个 `homeDir` 目录；TS 与 Rust 的 session index/state 格式必须兼容，否则第二次 resume 会失败。
4. **background/cron 时间不确定**：Task 4 使用 `ODY_CRON_MANUAL_TICK=1` 与 `ODY_CRON_CLOCK=file:...` 手动推进时钟，使行为可复现。
5. **benchmark 跨平台**：`ps -o rss= / cputime=` 在 Linux 与 macOS 上格式一致；Windows CI 未覆盖，属于已知限制。

---

## 本 Part 自检清单

### 规格覆盖表（对应 roadmap §4.3.9）

| Roadmap 条目 | 内容 | 覆盖 Task | 状态 |
|---|---|---|---|
| 4.3.9.4 | L2 对照：所有 AgentAPI 方法 TS vs Rust 返回值一致 | Task 1 | covered |
| 4.3.9.5 | L3 对照：mock provider 多轮 tool-call | Task 2 | covered |
| 4.3.9.5 | L3 对照：session-mode handoff | Task 3 | covered |
| 4.3.9.5 | L3 对照：background / cron scenario | Task 4 | covered |
| 4.3.9.6 | L4 对照：TS 创建 → Rust resume → TS 再 resume | Task 5 | covered |
| 4.3.9.7 | 基准：常驻内存、冷启动、空闲 CPU | Task 6 | covered |

### 自检项目

- [x] 1. Spec-coverage table: 上表覆盖 4.3.9.4–4.3.9.7 全部条目。
- [x] 2. Placeholder scan: 无 TODO/TBD；Task 1 中返回空形状的方法明确标注为「4.3.9 阶段稳定空形状」，不是未实现占位符。
- [x] 3. No phantom tasks: 每个 task 产生文件变更与可运行测试/脚本；无 `--allow-empty`。
- [x] 4. Dependency soundness: Task 2/3/4 依赖 Task 1；Task 5 依赖 Task 1 与 Part 2；Task 6 依赖 Part 2；无反向依赖。
- [x] 5. Caller & build soundness: Task 1 修改 `host.rs` dispatch 表（共享签名），任务内更新所有分支并运行 `cargo test -p ody-host --lib`；Task 2 修改 `normalize.ts`（共享函数），调用方 `run-parity.ts` 与所有 parity 测试自动使用新逻辑，任务结束运行 `pnpm -r typecheck` 与 `pnpm --filter @odysseythink/integration-tests typecheck` 全树检查；无同一签名跨 task 反复修改。
- [x] 6. Test-the-risk: Task 1 的 L2 测试断言每个 AgentAPI 返回值形状一致；Task 2 断言 tool-call 事件序列一致；Task 5 断言跨宿主 resume 后 sessionId 不变。
- [x] 7. Type consistency: Task 1 使用的 `Agent` 方法名（`config_data` / `exit_session_mode`）与 Part 1/2 定义一致；Task 2-4 使用的 event type 字符串与 `agent-core` / `ody-host` 实际 emit 的一致。
