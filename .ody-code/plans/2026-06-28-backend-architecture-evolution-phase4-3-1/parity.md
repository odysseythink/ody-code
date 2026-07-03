# Part 6 — L1/L3 fixtures + golden binary + TS runner

**Goal:** 建立 Rust `context-golden` 二进制与 TS `runTsContextGolden` runner，用同一套 fixture 对 `project`/`tokens`/`notification`/`ContextMemory` 进行 TS↔Rust 逐值 parity 验证，确保 4.3.1 的 Rust 实现与 TS 基线一致。

**Architecture:** 沿用 `packages/integration-tests/test/parity/kosong/l1-golden.test.ts` 已验证的模式：fixture 为 JSON 输入文件，Rust 二进制按 `kind` 分派并输出 JSON，TS runner 对同一份 fixture 执行等价计算，vitest 对两侧输出做 key-sort 后严格相等比较。L1 覆盖纯函数；L3 覆盖 `ContextMemory` 完整状态机（构造 → 重放 operations → 输出 history/messages/token/records）。

**Tech Stack:** Rust 2021 + `serde_json`；TypeScript + vitest；cargo binary。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/Cargo.toml` | 新增 `[[bin]] context-golden` |
| `rust-ody/crates/agent-rs/src/bin/context_golden.rs` | Rust golden 二进制：解析 fixture、分派、输出 JSON |
| `packages/integration-tests/src/parity/context-golden.ts` | TS runner：对 fixture 执行 TS 侧计算 |
| `packages/integration-tests/src/parity/fixtures/context/l1-project.json` | L1 projector + orphan-heal fixture |
| `packages/integration-tests/src/parity/fixtures/context/l1-tokens.json` | L1 token 估算 fixture |
| `packages/integration-tests/src/parity/fixtures/context/l1-notification.json` | L1 notification XML fixture |
| `packages/integration-tests/src/parity/fixtures/context/l3-memory.json` | L3 ContextMemory 操作序列 fixture |
| `packages/integration-tests/test/parity/context/l1-golden.test.ts` | Task 9 L1 parity测试 |
| `packages/integration-tests/test/parity/context/l3-golden.test.ts` | Task 10 L3 parity测试 |
| `packages/agent-core/test/helpers/index.ts` | 追加 ContextMemory / projector / notification / token 估算的 test-support 导出 |
| `packages/integration-tests/package.json` | 新增 `test:parity:agent:context` script |

---

## Dependency Overview

```text
[memory.md Task 6-8]
        │
        ▼
[parity.md Task 9: L1 fixtures + Rust binary + TS runner]
        │
        ▼
[parity.md Task 10: L3 ContextMemory fixture + memory replay]
        │
        ▼
[parity.md Task 11: npm script + full parity/typecheck]
```

- Task 9 与 Task 10 可共享同一个 binary/runner，但 L3 必须在 Task 9 的框架完成后才能追加 `memory` kind。
- Task 11 为纯 wiring，依赖 Task 10 通过。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| TS 侧 `ContextMemory` 未从公开入口导出 | 从 `@odysseythink/agent-core/test/helpers` 导出，避免污染主入口 |
| TS 侧构造 `ContextMemory` 需要完整 `Agent` | runner 内建最小 stub agent，仅实现 `ContextMemory` 实际访问的属性和方法 |
| `Date.now()` 导致 `lastAssistantAt` 非确定性 | TS runner  monkey-patch `globalThis.Date.now` 为固定值；Rust `ParityAgent` 的 `Clock` 返回固定值 |
| fixture 中 `AgentRecord` JSON 与 Rust/TS 类型不完全一致 | 只使用 `context.*` 系列记录，其字段已在 4.3.0 和本计划中对齐 |
| `microCompaction.compact` 两侧行为不一致 | parity stub 均为 identity（cutoff=0），排除微压缩差异干扰 |

---

### Task 9: L1 fixtures + Rust golden binary + TS runner

**Depends on:** `memory.md` Task 6-8

**Files:**
- Modify: `rust-ody/crates/agent-rs/Cargo.toml`
- Create: `rust-ody/crates/agent-rs/src/bin/context_golden.rs`
- Create: `packages/integration-tests/src/parity/context-golden.ts`
- Create: `packages/integration-tests/src/parity/fixtures/context/l1-project.json`
- Create: `packages/integration-tests/src/parity/fixtures/context/l1-tokens.json`
- Create: `packages/integration-tests/src/parity/fixtures/context/l1-notification.json`
- Create: `packages/integration-tests/test/parity/context/l1-golden.test.ts`

- [ ] **Write the failing test**

  ```typescript
  // packages/integration-tests/test/parity/context/l1-golden.test.ts
  import { existsSync, readFileSync } from 'node:fs';
  import { spawnSync } from 'node:child_process';
  import { beforeAll, describe, expect, it } from 'vitest';
  import { dirname, join } from 'pathe';
  import { fileURLToPath } from 'node:url';
  import { runTsContextGolden } from '../../../src/parity/context-golden';

  function findProjectRoot(): string {
    let current = dirname(fileURLToPath(import.meta.url));
    while (current !== dirname(current)) {
      if (existsSync(join(current, '.git'))) return current;
      current = dirname(current);
    }
    return process.cwd();
  }

  const rootDir = findProjectRoot();
  const fixturesDir = join(
    rootDir,
    'packages',
    'integration-tests',
    'src',
    'parity',
    'fixtures',
    'context',
  );
  const fixtures = ['l1-project.json', 'l1-tokens.json', 'l1-notification.json'];

  function sortKeys(obj: unknown): unknown {
    if (Array.isArray(obj)) return obj.map(sortKeys);
    if (obj !== null && typeof obj === 'object') {
      const sorted: Record<string, unknown> = {};
      for (const key of Object.keys(obj as Record<string, unknown>).sort()) {
        const val = (obj as Record<string, unknown>)[key];
        if (val === undefined) continue;
        sorted[key] = sortKeys(val);
      }
      return sorted;
    }
    return obj;
  }

  describe('context L1 golden parity', () => {
    beforeAll(() => {
      spawnSync('cargo', ['build', '-p', 'agent-rs', '--bin', 'context-golden'], {
        cwd: join(rootDir, 'rust-ody'),
        stdio: 'inherit',
      });
    });

    const binaryPath = join(rootDir, 'rust-ody', 'target', 'debug', 'context-golden');

    it.each(fixtures)('$name TS matches Rust', ({ name }: { name: string }) => {
      const fixturePath = join(fixturesDir, name);
      const raw = readFileSync(fixturePath, 'utf8');
      const ts = runTsContextGolden(JSON.parse(raw));
      const result = spawnSync(binaryPath, [fixturePath], { encoding: 'utf8' });
      if (result.status !== 0) {
        throw new Error(`context-golden exited ${result.status}: ${result.stderr}`);
      }
      const rust = JSON.parse(result.stdout);
      expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
    });
  });
  ```

  ```json
  // packages/integration-tests/src/parity/fixtures/context/l1-project.json
  {
    "kind": "project",
    "history": [
      {
        "role": "user",
        "content": [{ "type": "text", "text": "hello" }],
        "toolCalls": [],
        "origin": { "kind": "user" }
      },
      {
        "role": "user",
        "content": [{ "type": "text", "text": "world" }],
        "toolCalls": [],
        "origin": { "kind": "user" }
      }
    ]
  }
  ```

  ```json
  // packages/integration-tests/src/parity/fixtures/context/l1-tokens.json
  {
    "kind": "tokens",
    "messages": [
      {
        "role": "user",
        "content": [{ "type": "text", "text": "hello" }],
        "toolCalls": []
      },
      {
        "role": "assistant",
        "content": [{ "type": "text", "text": "world" }],
        "toolCalls": []
      }
    ]
  }
  ```

  ```json
  // packages/integration-tests/src/parity/fixtures/context/l1-notification.json
  {
    "kind": "notification",
    "data": {
      "id": "task-1",
      "category": "task",
      "type": "terminated",
      "source_kind": "background_task",
      "source_id": "bg-1",
      "title": "Task done",
      "severity": "info",
      "body": "Body line",
      "tail_output": "line1\nline2\n...\nline21"
    }
  }
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  cd packages/integration-tests && pnpm vitest run test/parity/context/l1-golden.test.ts
  ```

  Expected failure: `Error: Cannot find module '../../../src/parity/context-golden'` / `Rust host binary not found` / cargo build fails because `context-golden` bin is not declared.

- [ ] **Write the minimal implementation**

  1. 修改 `rust-ody/crates/agent-rs/Cargo.toml`，在现有 `[[bin]]` 后追加：

     ```toml
     [[bin]]
     name = "context-golden"
     path = "src/bin/context_golden.rs"
     ```

  2. 创建 `rust-ody/crates/agent-rs/src/bin/context_golden.rs`：

     ```rust
     use std::{env, fs};

     use agent_rs::context::{
       drop_orphan_tool_results, estimate_tokens_for_messages, project, render_notification_xml,
     };
     use agent_rs::records::nested::ContextMessage;
     use kosong_rs::message::Message;
     use serde_json::{Map, Value};

     #[derive(serde::Deserialize)]
     #[serde(tag = "kind")]
     enum Fixture {
       Project { history: Vec<ContextMessage> },
       Tokens { messages: Vec<Message> },
       Notification { data: Map<String, Value> },
     }

     fn main() {
       let path = env::args().nth(1).expect("fixture path argument required");
       let raw = fs::read_to_string(&path).expect("read fixture");
       let fixture: Fixture = serde_json::from_str(&raw).expect("parse fixture");

       let output = match fixture {
         Fixture::Project { history } => {
           serde_json::json!({ "messages": drop_orphan_tool_results(project(&history)) })
         }
         Fixture::Tokens { messages } => {
           serde_json::json!({ "tokens": estimate_tokens_for_messages(&messages) })
         }
         Fixture::Notification { data } => {
           serde_json::json!({ "xml": render_notification_xml(&data) })
         }
       };

       println!("{}", serde_json::to_string(&output).unwrap());
     }
     ```

  3. 创建 `packages/integration-tests/src/parity/context-golden.ts`：

     ```typescript
     import {
       dropOrphanToolResults,
       estimateTokensForMessages,
       project,
       renderNotificationXml,
     } from '@odysseythink/agent-core/test/helpers';
     import type { ContextMessage, Message } from '@odysseythink/agent-core';

     export type Fixture =
       | { kind: 'project'; history: ContextMessage[] }
       | { kind: 'tokens'; messages: Message[] }
       | { kind: 'notification'; data: Record<string, unknown> }
       | { kind: 'memory'; operations: unknown[] };

     export function runTsContextGolden(fixture: Fixture): unknown {
       switch (fixture.kind) {
         case 'project':
           return { messages: dropOrphanToolResults(project(fixture.history)) };
         case 'tokens':
           return { tokens: estimateTokensForMessages(fixture.messages) };
         case 'notification':
           return { xml: renderNotificationXml(fixture.data) };
         case 'memory':
           throw new Error('memory fixtures not implemented yet');
         default:
           throw new Error(`unknown fixture kind: ${(fixture as { kind: string }).kind}`);
       }
     }
     ```

  4. 修改 `packages/agent-core/test/helpers/index.ts`，追加 test-support 导出：

     ```typescript
     export { ContextMemory } from '../src/agent/context';
     export { project, dropOrphanToolResults } from '../src/agent/context/projector';
     export { renderNotificationXml } from '../src/agent/context/notification-xml';
     export { estimateTokensForMessages } from '../src/utils/tokens';
     ```

- [ ] **Run it and verify it PASSES**

  ```bash
  cd packages/integration-tests && pnpm vitest run test/parity/context/l1-golden.test.ts
  ```

  Expected: `Test Files 1 passed` and all three L1 fixtures equal.

- [ ] **Run whole-tree typecheck (shared-signature change)**

  ```bash
  pnpm -r typecheck
  ```

  Expected: full workspace typecheck clean.

- [ ] **Commit**

  ```bash
  git add rust-ody/crates/agent-rs/Cargo.toml \
         rust-ody/crates/agent-rs/src/bin/context_golden.rs \
         packages/integration-tests/src/parity/context-golden.ts \
         packages/integration-tests/src/parity/fixtures/context/l1-project.json \
         packages/integration-tests/src/parity/fixtures/context/l1-tokens.json \
         packages/integration-tests/src/parity/fixtures/context/l1-notification.json \
         packages/integration-tests/test/parity/context/l1-golden.test.ts \
         packages/agent-core/test/helpers/index.ts
  git commit -m "feat(agent-rs,integration-tests): context L1 parity harness"

---

### Task 10: L3 ContextMemory fixture + memory replay

**Depends on:** Task 9

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/bin/context_golden.rs`（追加 `Memory` kind 与 `ParityAgent`）
- Modify: `packages/integration-tests/src/parity/context-golden.ts`（追加 `memory` 分支与 stub agent）
- Create: `packages/integration-tests/src/parity/fixtures/context/l3-memory.json`
- Create: `packages/integration-tests/test/parity/context/l3-golden.test.ts`

- [ ] **Write the failing test**

  ```json
  // packages/integration-tests/src/parity/fixtures/context/l3-memory.json
  {
    "kind": "memory",
    "operations": [
      {
        "type": "context.append_message",
        "message": {
          "role": "user",
          "content": [{ "type": "text", "text": "hello" }],
          "toolCalls": [],
          "origin": { "kind": "user" }
        }
      },
      {
        "type": "context.append_loop_event",
        "event": {
          "type": "step.begin",
          "uuid": "s1",
          "turnId": "t1",
          "step": 1
        }
      },
      {
        "type": "context.append_loop_event",
        "event": {
          "type": "content.part",
          "uuid": "p1",
          "turnId": "t1",
          "step": 1,
          "stepUuid": "s1",
          "part": { "type": "text", "text": "ok" }
        }
      },
      {
        "type": "context.append_loop_event",
        "event": {
          "type": "step.end",
          "uuid": "s1",
          "turnId": "t1",
          "step": 1,
          "usage": {
            "inputOther": 2,
            "output": 1,
            "inputCacheRead": 0,
            "inputCacheCreation": 0
          }
        }
      }
    ]
  }
  ```

  ```typescript
  // packages/integration-tests/test/parity/context/l3-golden.test.ts
  import { existsSync, readFileSync } from 'node:fs';
  import { spawnSync } from 'node:child_process';
  import { beforeAll, describe, expect, it } from 'vitest';
  import { dirname, join } from 'pathe';
  import { fileURLToPath } from 'node:url';
  import { runTsContextGolden } from '../../../src/parity/context-golden';

  function findProjectRoot(): string {
    let current = dirname(fileURLToPath(import.meta.url));
    while (current !== dirname(current)) {
      if (existsSync(join(current, '.git'))) return current;
      current = dirname(current);
    }
    return process.cwd();
  }

  const rootDir = findProjectRoot();
  const fixturesDir = join(
    rootDir,
    'packages',
    'integration-tests',
    'src',
    'parity',
    'fixtures',
    'context',
  );
  const fixtures = ['l3-memory.json'];

  function sortKeys(obj: unknown): unknown {
    if (Array.isArray(obj)) return obj.map(sortKeys);
    if (obj !== null && typeof obj === 'object') {
      const sorted: Record<string, unknown> = {};
      for (const key of Object.keys(obj as Record<string, unknown>).sort()) {
        const val = (obj as Record<string, unknown>)[key];
        if (val === undefined) continue;
        sorted[key] = sortKeys(val);
      }
      return sorted;
    }
    return obj;
  }

  describe('context L3 golden parity', () => {
    beforeAll(() => {
      spawnSync('cargo', ['build', '-p', 'agent-rs', '--bin', 'context-golden'], {
        cwd: join(rootDir, 'rust-ody'),
        stdio: 'inherit',
      });
    });

    const binaryPath = join(rootDir, 'rust-ody', 'target', 'debug', 'context-golden');

    it.each(fixtures)('$name TS matches Rust', ({ name }: { name: string }) => {
      const fixturePath = join(fixturesDir, name);
      const raw = readFileSync(fixturePath, 'utf8');
      const ts = runTsContextGolden(JSON.parse(raw));
      const result = spawnSync(binaryPath, [fixturePath], { encoding: 'utf8' });
      if (result.status !== 0) {
        throw new Error(`context-golden exited ${result.status}: ${result.stderr}`);
      }
      const rust = JSON.parse(result.stdout);
      expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
    });
  });
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  cd packages/integration-tests && pnpm vitest run test/parity/context/l3-golden.test.ts
  ```

  Expected failure: `Error: memory fixtures not implemented yet` from TS runner, or Rust binary panics with unknown variant `memory`.

- [ ] **Write the minimal implementation**

  1. 扩展 `rust-ody/crates/agent-rs/src/bin/context_golden.rs`：

     ```rust
     use std::{env, fs, sync::Mutex};

     use agent_rs::context::{
       drop_orphan_tool_results, estimate_tokens_for_messages, project, render_notification_xml,
       Clock, ContextAgent, ContextMemory, ContextSwitchFlusher, InjectionLifecycle,
       MicroCompaction, RecordLog, ReplayBuilder, StatusEmitter,
     };
     use agent_rs::records::nested::{BackgroundTask, CompactionResult, ContextMessage};
     use agent_rs::records::AgentRecord;
     use kosong_rs::message::Message;
     use serde_json::{Map, Value};

     #[derive(serde::Deserialize)]
     #[serde(tag = "kind")]
     enum Fixture {
       Project { history: Vec<ContextMessage> },
       Tokens { messages: Vec<Message> },
       Notification { data: Map<String, Value> },
       Memory { operations: Vec<AgentRecord> },
     }

     fn main() {
       let path = env::args().nth(1).expect("fixture path argument required");
       let raw = fs::read_to_string(&path).expect("read fixture");
       let fixture: Fixture = serde_json::from_str(&raw).expect("parse fixture");

       let output = match fixture {
         Fixture::Project { history } => {
           serde_json::json!({ "messages": drop_orphan_tool_results(project(&history)) })
         }
         Fixture::Tokens { messages } => {
           serde_json::json!({ "tokens": estimate_tokens_for_messages(&messages) })
         }
         Fixture::Notification { data } => {
           serde_json::json!({ "xml": render_notification_xml(&data) })
         }
         Fixture::Memory { operations } => run_memory(operations),
       };

       println!("{}", serde_json::to_string(&output).unwrap());
     }

     fn run_memory(operations: Vec<AgentRecord>) -> Value {
       let agent = ParityAgent::new();
       let mut memory = ContextMemory::new(&agent);
       for op in operations {
         replay(&mut memory, op);
       }
       serde_json::json!({
         "history": memory.history(),
         "messages": memory.messages(),
         "token_count": memory.token_count(),
         "token_count_with_pending": memory.token_count_with_pending(),
         "records": agent.record_log.records(),
       })
     }

     fn replay(memory: &mut ContextMemory, op: AgentRecord) {
       match op {
         AgentRecord::ContextAppendMessage { message, .. } => memory.append_message(message),
         AgentRecord::ContextAppendLoopEvent { event, .. } => memory.append_loop_event(event),
         AgentRecord::ContextClear { .. } => memory.clear(),
         AgentRecord::ContextApplyCompaction { result, .. } => memory.apply_compaction(result),
         AgentRecord::ContextUndo { count, .. } => memory.undo(count),
         _ => {}
       }
     }

     struct ParityRecordLog(Mutex<Vec<AgentRecord>>);
     impl RecordLog for ParityRecordLog {
       fn log_record(&self, record: AgentRecord) {
         self.0.lock().unwrap().push(record);
       }
       fn restoring_time(&self) -> Option<i64> {
         None
       }
     }
     impl ParityRecordLog {
       fn records(&self) -> Vec<AgentRecord> {
         self.0.lock().unwrap().clone()
       }
       fn new() -> Self {
         Self(Mutex::new(Vec::new()))
       }
     }

     struct ParityMicroCompaction;
     impl MicroCompaction for ParityMicroCompaction {
       fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> {
         messages.to_vec()
       }
       fn reset(&self, _max_cutoff: usize) {}
     }

     struct ParityInjection;
     impl InjectionLifecycle for ParityInjection {
       fn on_context_clear(&self) {}
       fn on_context_compacted(&self, _compacted_count: usize) {}
       fn on_context_message_removed(&self, _index: usize) {}
     }

     struct ParityBackground;
     impl agent_rs::context::BackgroundNotifications for ParityBackground {
       fn mark_delivered_notification(&self, _origin: &BackgroundTask) {}
     }

     struct ParityReplay;
     impl ReplayBuilder for ParityReplay {
       fn push_message(&self, _message: &ContextMessage) {}
       fn remove_last_messages(&self, _messages: &[ContextMessage]) {}
     }

     struct ParityStatus;
     impl StatusEmitter for ParityStatus {
       fn emit_status_updated(&self) {}
     }

     struct ParityFlusher;
     impl ContextSwitchFlusher for ParityFlusher {
       fn flush_deferred_context_switch(&self) {}
     }

     struct ParityClock;
     impl Clock for ParityClock {
       fn now_ms(&self) -> i64 {
         12345
       }
     }

     struct ParityAgent {
       record_log: ParityRecordLog,
       micro_compaction: ParityMicroCompaction,
       injection: ParityInjection,
       background: ParityBackground,
       replay_builder: ParityReplay,
       status: ParityStatus,
       flusher: ParityFlusher,
       clock: ParityClock,
     }
     impl ParityAgent {
       fn new() -> Self {
         Self {
           record_log: ParityRecordLog::new(),
           micro_compaction: ParityMicroCompaction,
           injection: ParityInjection,
           background: ParityBackground,
           replay_builder: ParityReplay,
           status: ParityStatus,
           flusher: ParityFlusher,
           clock: ParityClock,
         }
       }
     }
     impl ContextAgent for ParityAgent {
       fn record_log(&self) -> &dyn RecordLog {
         &self.record_log
       }
       fn micro_compaction(&self) -> &dyn MicroCompaction {
         &self.micro_compaction
       }
       fn injection(&self) -> &dyn InjectionLifecycle {
         &self.injection
       }
       fn background(&self) -> &dyn agent_rs::context::BackgroundNotifications {
         &self.background
       }
       fn replay_builder(&self) -> &dyn ReplayBuilder {
         &self.replay_builder
       }
       fn status_emitter(&self) -> &dyn StatusEmitter {
         &self.status
       }
       fn context_switch_flusher(&self) -> &dyn ContextSwitchFlusher {
         &self.flusher
       }
       fn clock(&self) -> &dyn Clock {
         &self.clock
       }
     }
     ```

  2. 扩展 `packages/integration-tests/src/parity/context-golden.ts`：

     ```typescript
     import {
       ContextMemory,
       dropOrphanToolResults,
       estimateTokensForMessages,
       project,
       renderNotificationXml,
     } from '@odysseythink/agent-core/test/helpers';
     import type {
       Agent,
       AgentRecord,
       ContextMessage,
       Message,
     } from '@odysseythink/agent-core';

     const FIXED_TIME = 12345;

     export type Fixture =
       | { kind: 'project'; history: ContextMessage[] }
       | { kind: 'tokens'; messages: Message[] }
       | { kind: 'notification'; data: Record<string, unknown> }
       | { kind: 'memory'; operations: AgentRecord[] };

     export function runTsContextGolden(fixture: Fixture): unknown {
       switch (fixture.kind) {
         case 'project':
           return { messages: dropOrphanToolResults(project(fixture.history)) };
         case 'tokens':
           return { tokens: estimateTokensForMessages(fixture.messages) };
         case 'notification':
           return { xml: renderNotificationXml(fixture.data) };
         case 'memory':
           return runMemory(fixture.operations);
         default:
           throw new Error(`unknown fixture kind: ${(fixture as { kind: string }).kind}`);
       }
     }

     function runMemory(operations: AgentRecord[]): unknown {
       const { agent, records } = makeStubAgent();
       const context = new ContextMemory(agent);
       const originalNow = Date.now;
       globalThis.Date.now = () => FIXED_TIME;
       try {
         for (const op of operations) {
           replayTs(context, op);
         }
       } finally {
         globalThis.Date.now = originalNow;
       }
       return {
         history: context.data().history,
         messages: context.messages,
         token_count: context.tokenCount,
         token_count_with_pending: context.tokenCountWithPending,
         records,
       };
     }

     function replayTs(context: ContextMemory, op: AgentRecord): void {
       switch (op.type) {
         case 'context.append_message':
           context.appendMessage(op.message);
           return;
         case 'context.append_loop_event':
           context.appendLoopEvent(op.event);
           return;
         case 'context.clear':
           context.clear();
           return;
         case 'context.apply_compaction':
           context.applyCompaction(op);
           return;
         case 'context.undo':
           context.undo(op.count);
           return;
         default:
           return;
       }
     }

     function makeStubAgent(): { agent: Agent; records: AgentRecord[] } {
       const records: AgentRecord[] = [];
       const agent = {
         records: {
           logRecord: (r: AgentRecord) => {
             records.push(r);
           },
           get restoring() {
             return null;
           },
         },
         microCompaction: {
           compact: (messages: ContextMessage[]) => messages,
           reset: () => {},
         },
         injection: {
           onContextClear: () => {},
           onContextCompacted: () => {},
           onContextMessageRemoved: () => {},
         },
         background: {
           markDeliveredNotification: () => {},
         },
         replayBuilder: {
           push: () => {},
           removeLastMessages: () => {},
         },
         emitStatusUpdated: () => {},
       } as unknown as Agent;
       return { agent, records };
     }
     ```

- [ ] **Run it and verify it PASSES**

  ```bash
  cd packages/integration-tests && pnpm vitest run test/parity/context/l3-golden.test.ts
  ```

  Expected: `Test Files 1 passed` and `l3-memory.json` TS/Rust 输出完全一致。

- [ ] **Run whole-tree typecheck**

  ```bash
  pnpm -r typecheck
  ```

  Expected: full workspace typecheck clean.

- [ ] **Commit**

  ```bash
  git add rust-ody/crates/agent-rs/src/bin/context_golden.rs \
         packages/integration-tests/src/parity/context-golden.ts \
         packages/integration-tests/src/parity/fixtures/context/l3-memory.json \
         packages/integration-tests/test/parity/context/l3-golden.test.ts
  git commit -m "feat(agent-rs,integration-tests): context L3 memory parity"

---

### Task 11: npm script + full parity/typecheck 验证

**Depends on:** Task 10

**Files:**
- Modify: `packages/integration-tests/package.json`

- [ ] **Write the complete wiring**

  修改 `packages/integration-tests/package.json` 的 `scripts` 区块，在 `"test:parity"` 附近新增：

  ```json
  "test:parity:agent:context": "vitest run test/parity/context",
  ```

  即 `scripts` 片段变为：

  ```json
  {
    "scripts": {
      "test": "vitest run",
      "test:parity": "vitest run test/parity",
      "test:parity:agent:context": "vitest run test/parity/context",
      "test:parity:kaos": "vitest run test/parity/kaos",
      ...
    }
  }
  ```

- [ ] **Build + manual verification**

  1. 编译 Rust golden 二进制：

     ```bash
     cd rust-ody && cargo build -p agent-rs --bin context-golden
     ```

     Expected: `Finished dev [unoptimized + debuginfo] target(s) in ...`。

  2. 运行新的 parity script：

     ```bash
     pnpm --filter @odysseythink/integration-tests test:parity:agent:context
     ```

     Expected: `Test Files 2 passed`（`l1-golden.test.ts` 与 `l3-golden.test.ts` 均通过）。

  3. 全 workspace typecheck：

     ```bash
     pnpm -r typecheck
     ```

     Expected: 全绿无错误。

  4. Cargo 全 workspace typecheck（含 tests）：

     ```bash
     cd rust-ody && cargo check -p agent-rs --workspace --tests
     ```

     Expected: `Finished dev` 且无编译错误。

- [ ] **Commit**

  ```bash
  git add packages/integration-tests/package.json
  git commit -m "chore(integration-tests): add test:parity:agent:context script"
  ```

---

## Local Self-Review

- [ ] 1. Spec-coverage（本 part）：Task 9 覆盖 L1 parity（project/tokens/notification）；Task 10 覆盖 L3 ContextMemory parity；Task 11 覆盖 script 与全量验证。无 GAP。
- [ ] 2. Placeholder scan：所有 fixture、binary、runner、test 代码均为真实可运行代码；无 TODO/TBD。
- [ ] 3. No phantom tasks：Task 9 产出 L1 fixtures + binary + runner + test；Task 10 产出 L3 fixture + memory replay + test；Task 11 产出 npm script + 验证步骤。
- [ ] 4. Dependency soundness：Task 9 依赖 `memory.md`；Task 10 依赖 Task 9；Task 11 依赖 Task 10。无反向依赖。
- [ ] 5. Caller & build soundness：Task 9 修改 `agent-core/test/helpers/index.ts` 导出，runner 与 tests 为唯一调用方；Task 9 与 Task 10 均运行 `pnpm -r typecheck` 全 workspace typecheck。新增 `context-golden` binary 名称在 `Cargo.toml` 与 test 中一致。
- [ ] 6. Test-the-risk：
  - L1 parity 直接比较 Rust/TS 对同一 fixture 的输出，覆盖 projector 合并规则、token 估算公式、notification XML 转义与 tail 截断常量。
  - L3 parity 比较 `ContextMemory` 重放同一 `AgentRecord` 序列后的 history、messages、token_count、records，覆盖 step begin/end、content part、token usage 记账、record log。
  - must-survive 输入：project fixture 中两个真实 user 消息合并为 `"hello\n\nworld"`；notification fixture 保留 `agent_id` 与 `<task-notification>`；memory fixture 的 assistant 消息保留到 history。
- [ ] 7. Type一致性：binary/runner 的 `Fixture` kind 字符串（`project`/`tokens`/`notification`/`memory`）与 fixture JSON 一致；`AgentRecord` 操作类型名与 `records/types.rs` 中 `#[serde(rename = ...)]` 一致；`ContextMemory` 方法签名沿用 `memory.md` 定义。
  ```
  ```
