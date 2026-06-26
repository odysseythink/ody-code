# Part 2 — Scenarios + TS-vs-TS Self-Comparison

本 Part 实现三个可复用的 parity scenario（hello-world、file-edit、multi-turn-tool），以及 TS-vs-TS 自比对测试套件。所有 scenario 必须是确定性的：相同 MockChatProvider 配置在两次运行中产生语义等价的响应与事件序列。

---

## Part 2 依赖图

```
B1 Scenario helpers + multi-turn MockChatProvider
  │
  ├──► B2 hello-world scenario
  ├──► B3 file-edit scenario
  └──► B4 multi-turn-tool scenario
       │
       ▼
       B5 TS-vs-TS parity harness
```

B1 无 Part 2 内部依赖；B2/B3/B4 并行依赖 B1；B5 依赖 B1–B4。

---

## Part 2 范围说明

- **覆盖**：hello-world、file-edit、multi-turn-tool 三个 scenario 的实现与独立测试。
- **不覆盖**：Rust 后端比对（见 Part 3）、CLI 开关（见 Part 4）、CI 集成（见 Part 5）。
- **共享签名**：本 Part 不改现有共享类型或接口，只新增 `scenarios/*` 与测试文件。

---

### Task B2: hello-world Scenario

**Depends on:** Task B1

**Files:**
- Create: `packages/integration-tests/src/parity/scenarios/hello-world.ts`
- Create: `packages/integration-tests/test/parity/scenarios/hello-world.test.ts`

**Goal:** 实现最简单的 parity scenario：创建 session、发送一条用户消息、等待 assistant 文本响应返回。

- [ ] 在 `packages/integration-tests/test/parity/scenarios/hello-world.test.ts` 写入测试：

```ts
import { describe, expect, it } from 'vitest';
import { makeTsBackend, createTempHome, cleanupHome } from '../../../src/parity/backends';
import { ParityDriver } from '../../../src/parity/driver';
import { helloWorldScenario, helloWorldMockLlm } from '../../../src/parity/scenarios/hello-world';

describe('hello-world scenario', () => {
  it('produces the expected assistant text', async () => {
    const homeDir = await createTempHome('hello-');
    const backend = await makeTsBackend({ homeDir, mockLlm: helloWorldMockLlm });
    try {
      const driver = new ParityDriver({ timeoutMs: 10000 });
      const snapshot = await driver.runScenario(backend, helloWorldScenario);
      const deltas = snapshot.events.filter((e: any) => e.type === 'assistant.delta');
      expect(deltas.length).toBeGreaterThan(0);
      const text = deltas.map((e: any) => e.delta).join('');
      expect(text).toContain('Hello, parity!');
    } finally {
      await backend.close();
      await cleanupHome(homeDir);
    }
  });
});
```

- [ ] 运行测试并确认失败：

```bash
pnpm --filter integration-tests vitest run test/parity/scenarios/hello-world.test.ts
```

预期失败：`hello-world.ts` 不存在。

- [ ] 在 `packages/integration-tests/src/parity/scenarios/hello-world.ts` 写入实现：

```ts
import type { ChatProvider } from '@odysseythink/kosong';
import type { Scenario } from '../types';
import { MockChatProvider } from '../fixtures/mock-provider';
import { waitForTurnEnded } from './utils';

export const helloWorldMockLlm: ChatProvider = new MockChatProvider([
  { type: 'text', text: 'Hello, parity!' },
]);

export const helloWorldScenario: Scenario = {
  name: 'hello-world',
  async run(backend) {
    const summary = await backend.client.createSession({
      title: 'hello-world',
      workDir: backend.homeDir,
      permission: 'auto',
    });
    await backend.client.prompt({ sessionId: summary.id, input: 'Say hello' });
    await waitForTurnEnded(backend.client, { timeoutMs: 10000 });
    return { responses: [{ sessionId: summary.id }] };
  },
};
```

- [ ] 运行测试并确认通过：

```bash
pnpm --filter integration-tests vitest run test/parity/scenarios/hello-world.test.ts
```

预期：1 个用例通过。

- [ ] 提交：

```bash
git add packages/integration-tests/src/parity/scenarios/hello-world.ts \
           packages/integration-tests/test/parity/scenarios/hello-world.test.ts
git commit -m "feat(integration-tests): hello-world parity scenario"
```

---

### Task B3: file-edit Scenario

**Depends on:** Task B1

**Files:**
- Create: `packages/integration-tests/src/parity/scenarios/file-edit.ts`
- Create: `packages/integration-tests/test/parity/scenarios/file-edit.test.ts`

**Goal:** 实现一个 tool-call scenario：mock provider 返回 `Write` 工具调用，agent 在 `backend.homeDir` 下创建文件；scenario 读取文件内容并放入 `fsTree`。

- [ ] 在 `packages/integration-tests/test/parity/scenarios/file-edit.test.ts` 写入测试：

```ts
import { describe, expect, it } from 'vitest';
import { makeTsBackend, createTempHome, cleanupHome } from '../../../src/parity/backends';
import { ParityDriver } from '../../../src/parity/driver';
import { fileEditScenario, fileEditMockLlm, FILE_NAME, FILE_CONTENT } from '../../../src/parity/scenarios/file-edit';

describe('file-edit scenario', () => {
  it('writes the expected file and reports it in fsTree', async () => {
    const homeDir = await createTempHome('file-edit-');
    const backend = await makeTsBackend({ homeDir, mockLlm: fileEditMockLlm });
    try {
      const driver = new ParityDriver({ timeoutMs: 15000 });
      const snapshot = await driver.runScenario(backend, fileEditScenario);
      expect(snapshot.fsTree).toEqual({ [FILE_NAME]: FILE_CONTENT });
    } finally {
      await backend.close();
      await cleanupHome(homeDir);
    }
  });
});
```

- [ ] 运行测试并确认失败：

```bash
pnpm --filter integration-tests vitest run test/parity/scenarios/file-edit.test.ts
```

预期失败：`file-edit.ts` 不存在。

- [ ] 在 `packages/integration-tests/src/parity/scenarios/file-edit.ts` 写入实现：

```ts
import { readFile } from 'node:fs/promises';
import { join } from 'pathe';
import type { ChatProvider } from '@odysseythink/kosong';
import type { Scenario } from '../types';
import { MockChatProvider } from '../fixtures/mock-provider';
import { waitForTurnEnded } from './utils';

export const FILE_NAME = 'parity.txt';
export const FILE_CONTENT = 'hello parity';

export const fileEditMockLlm: ChatProvider = new MockChatProvider([
  [
    {
      type: 'tool_call_part',
      toolCallId: 'call-file-edit',
      name: 'Write',
      argumentsPart: JSON.stringify({ path: FILE_NAME, content: FILE_CONTENT }),
    },
  ],
  [{ type: 'text', text: 'File written.' }],
]);

export const fileEditScenario: Scenario = {
  name: 'file-edit',
  async run(backend) {
    const summary = await backend.client.createSession({
      title: 'file-edit',
      workDir: backend.homeDir,
      permission: 'auto',
    });
    await backend.client.prompt({ sessionId: summary.id, input: 'Write parity.txt' });
    await waitForTurnEnded(backend.client, { timeoutMs: 10000 });
    const content = await readFile(join(backend.homeDir, FILE_NAME), 'utf8').catch(() => null);
    return { responses: [], fsTree: { [FILE_NAME]: content } };
  },
};
```

- [ ] 运行测试并确认通过：

```bash
pnpm --filter integration-tests vitest run test/parity/scenarios/file-edit.test.ts
```

预期：1 个用例通过。

- [ ] 提交：

```bash
git add packages/integration-tests/src/parity/scenarios/file-edit.ts \
           packages/integration-tests/test/parity/scenarios/file-edit.test.ts
git commit -m "feat(integration-tests): file-edit parity scenario"
```

---

### Task B4: multi-turn-tool Scenario

**Depends on:** Task B1

**Files:**
- Create: `packages/integration-tests/src/parity/scenarios/multi-turn-tool.ts`
- Create: `packages/integration-tests/test/parity/scenarios/multi-turn-tool.test.ts`

**Goal:** 实现两轮用户输入的 scenario：第一轮写文件，第二轮追加内容；验证两次 `prompt` 调用后文件最终内容正确。

- [ ] 在 `packages/integration-tests/test/parity/scenarios/multi-turn-tool.test.ts` 写入测试：

```ts
import { describe, expect, it } from 'vitest';
import { makeTsBackend, createTempHome, cleanupHome } from '../../../src/parity/backends';
import { ParityDriver } from '../../../src/parity/driver';
import {
  multiTurnToolScenario,
  multiTurnToolMockLlm,
  FILE_NAME,
  FINAL_CONTENT,
} from '../../../src/parity/scenarios/multi-turn-tool';

describe('multi-turn-tool scenario', () => {
  it('writes then appends the expected file', async () => {
    const homeDir = await createTempHome('multi-turn-');
    const backend = await makeTsBackend({ homeDir, mockLlm: multiTurnToolMockLlm });
    try {
      const driver = new ParityDriver({ timeoutMs: 20000 });
      const snapshot = await driver.runScenario(backend, multiTurnToolScenario);
      expect(snapshot.fsTree).toEqual({ [FILE_NAME]: FINAL_CONTENT });
    } finally {
      await backend.close();
      await cleanupHome(homeDir);
    }
  });
});
```

- [ ] 运行测试并确认失败：

```bash
pnpm --filter integration-tests vitest run test/parity/scenarios/multi-turn-tool.test.ts
```

预期失败：`multi-turn-tool.ts` 不存在。

- [ ] 在 `packages/integration-tests/src/parity/scenarios/multi-turn-tool.ts` 写入实现：

```ts
import { readFile } from 'node:fs/promises';
import { join } from 'pathe';
import type { ChatProvider } from '@odysseythink/kosong';
import type { Scenario } from '../types';
import { MockChatProvider } from '../fixtures/mock-provider';
import { waitForTurnEnded } from './utils';

export const FILE_NAME = 'multi-turn.txt';
export const INITIAL_CONTENT = 'line 1';
export const FINAL_CONTENT = 'line 1\nline 2';

export const multiTurnToolMockLlm: ChatProvider = new MockChatProvider([
  [
    {
      type: 'tool_call_part',
      toolCallId: 'call-write',
      name: 'Write',
      argumentsPart: JSON.stringify({ path: FILE_NAME, content: INITIAL_CONTENT }),
    },
  ],
  [{ type: 'text', text: 'Created.' }],
  [
    {
      type: 'tool_call_part',
      toolCallId: 'call-append',
      name: 'Write',
      argumentsPart: JSON.stringify({ path: FILE_NAME, mode: 'append', content: '\nline 2' }),
    },
  ],
  [{ type: 'text', text: 'Appended.' }],
]);

export const multiTurnToolScenario: Scenario = {
  name: 'multi-turn-tool',
  async run(backend) {
    const summary = await backend.client.createSession({
      title: 'multi-turn-tool',
      workDir: backend.homeDir,
      permission: 'auto',
    });
    await backend.client.prompt({ sessionId: summary.id, input: 'Create a file' });
    await waitForTurnEnded(backend.client, { timeoutMs: 10000 });
    await backend.client.prompt({ sessionId: summary.id, input: 'Append a line' });
    await waitForTurnEnded(backend.client, { timeoutMs: 10000 });
    const content = await readFile(join(backend.homeDir, FILE_NAME), 'utf8').catch(() => null);
    return { responses: [], fsTree: { [FILE_NAME]: content } };
  },
};
```

- [ ] 运行测试并确认通过：

```bash
pnpm --filter integration-tests vitest run test/parity/scenarios/multi-turn-tool.test.ts
```

预期：1 个用例通过。

- [ ] 提交：

```bash
git add packages/integration-tests/src/parity/scenarios/multi-turn-tool.ts \
           packages/integration-tests/test/parity/scenarios/multi-turn-tool.test.ts
git commit -m "feat(integration-tests): multi-turn-tool parity scenario"
```

---

### Task B5: TS-vs-TS Parity Harness

**Depends on:** Task B2, Task B3, Task B4

**Files:**
- Create: `packages/integration-tests/src/parity/run-parity.ts`
- Create: `packages/integration-tests/src/parity/scenarios/index.ts`
- Create: `packages/integration-tests/test/parity/ts-vs-ts.test.ts`

**Goal:** 用共享 helper 把每个 scenario 在两份 TS 后端上各跑一遍，normalize 后 `assertParity`，验证自身一致性；为 Part 3 的 TS-vs-Rust 复用同一 helper。

- [ ] 在 `packages/integration-tests/test/parity/ts-vs-ts.test.ts` 写入测试：

```ts
import { describe, expect, it } from 'vitest';
import { makeTsBackend } from '../../src/parity/backends';
import { runParity } from '../../src/parity/run-parity';
import {
  helloWorldScenario,
  helloWorldMockLlm,
  fileEditScenario,
  fileEditMockLlm,
  multiTurnToolScenario,
  multiTurnToolMockLlm,
} from '../../src/parity/scenarios';

const cases = [
  { scenario: helloWorldScenario, mockLlm: helloWorldMockLlm },
  { scenario: fileEditScenario, mockLlm: fileEditMockLlm },
  { scenario: multiTurnToolScenario, mockLlm: multiTurnToolMockLlm },
];

describe('TS-vs-TS parity', () => {
  it.each(cases)('$scenario.name is self-consistent across two TS backends', async ({ scenario, mockLlm }) => {
    const diff = await runParity({
      scenario,
      mockLlm,
      makeA: (homeDir) => makeTsBackend({ homeDir, mockLlm }),
      makeB: (homeDir) => makeTsBackend({ homeDir, mockLlm }),
      timeoutMs: 30000,
    });
    expect(diff).toBeNull();
  });
});
```

- [ ] 运行测试并确认失败：

```bash
pnpm --filter integration-tests vitest run test/parity/ts-vs-ts.test.ts
```

预期失败：`run-parity.ts` 或 `scenarios/index.ts` 不存在。

- [ ] 在 `packages/integration-tests/src/parity/run-parity.ts` 写入实现：

```ts
import type { ChatProvider } from '@odysseythink/kosong';
import { makeTsBackend, createTempHome, cleanupHome } from './backends';
import { ParityDriver } from './driver';
import { normalize } from './normalize';
import { assertParity } from './assert-parity';
import type { ParityBackend, ParityDiff, Scenario } from './types';

export interface RunParityOptions {
  readonly scenario: Scenario;
  readonly mockLlm: ChatProvider;
  readonly makeA: (homeDir: string) => Promise<ParityBackend>;
  readonly makeB: (homeDir: string) => Promise<ParityBackend>;
  readonly timeoutMs?: number;
}

export async function runParity(options: RunParityOptions): Promise<ParityDiff | null> {
  const { scenario, mockLlm, makeA, makeB, timeoutMs = 30000 } = options;
  const homeDirA = await createTempHome('parity-a-');
  const homeDirB = await createTempHome('parity-b-');
  const backendA = await makeA(homeDirA);
  const backendB = await makeB(homeDirB);
  try {
    const driver = new ParityDriver({ timeoutMs });
    const snapshotA = await driver.runScenario(backendA, scenario);
    const snapshotB = await driver.runScenario(backendB, scenario);
    const normalizedA = normalize(snapshotA, { homeDir: homeDirA, tmpDir: '/tmp' });
    const normalizedB = normalize(snapshotB, { homeDir: homeDirB, tmpDir: '/tmp' });
    return assertParity(scenario.name, normalizedA, normalizedB);
  } finally {
    await backendA.close().catch(() => {});
    await backendB.close().catch(() => {});
    await cleanupHome(homeDirA);
    await cleanupHome(homeDirB);
  }
}
```

- [ ] 在 `packages/integration-tests/src/parity/scenarios/index.ts` 写入实现：

```ts
export { fileEditScenario, fileEditMockLlm } from './file-edit';
export { helloWorldScenario, helloWorldMockLlm } from './hello-world';
export { multiTurnToolScenario, multiTurnToolMockLlm } from './multi-turn-tool';
```

- [ ] 运行测试并确认通过：

```bash
pnpm --filter integration-tests vitest run test/parity/ts-vs-ts.test.ts
```

预期：3 个用例全部通过。

- [ ] 运行整个 `integration-tests` 包的 typecheck：

```bash
pnpm --filter integration-tests typecheck
```

预期：无编译错误。

- [ ] 提交：

```bash
git add packages/integration-tests/src/parity/run-parity.ts \
           packages/integration-tests/src/parity/scenarios/index.ts \
           packages/integration-tests/test/parity/ts-vs-ts.test.ts
git commit -m "feat(integration-tests): TS-vs-TS parity harness"
```

---

## Part 2 本地 Self-Review

| 检查项 | 结论 |
|---|---|
| 1. Spec-coverage | hello-world scenario → B2；file-edit scenario → B3；multi-turn-tool scenario → B4；TS-vs-TS 自比对 → B5；多轮 mock provider + 等待工具 → B1。 |
| 2. Placeholder scan | 无 TODO/TBD；所有实现均为可执行代码。 |
| 3. No phantom tasks | 每个任务都产生新文件或测试；无 `--allow-empty`。 |
| 4. Dependency soundness | B1 依赖 `core.md` A4/A6；B2/B3/B4 依赖 B1；B5 依赖 B2/B3/B4。无反向依赖。 |
| 5. Caller & build soundness | 本 Part 只新增符号，不改现有共享签名；B5 结束有 `pnpm --filter integration-tests typecheck`。 |
| 6. Test-the-risk | 每个 scenario 都断言最终文件内容或 assistant 文本；TS-vs-TS 断言 diff 为 null；多轮 mock provider 断言响应按顺序消费。 |
| 7. Type consistency | `Scenario`、`ParityBackend`、`ParityDiff` 均来自 Part 1；新增 scenario 返回值与 `ScenarioSnapshot` 一致。 |

- [ ] 1. Spec-coverage table: hello-world → B2, file-edit → B3, multi-turn-tool → B4, TS-vs-TS → B5, helpers/mock → B1.
- [ ] 2. Placeholder scan: 无 TODO/TBD/占位符。
- [ ] 3. No phantom tasks: 每个任务都有新文件和可运行测试。
- [ ] 4. Dependency soundness: B1 依赖 `core.md` A4/A6；B2–B4 依赖 B1；B5 依赖 B2–B4。
- [ ] 5. Caller & build soundness: 只新增符号；B5 运行 `pnpm --filter integration-tests typecheck`。
- [ ] 6. Test-the-risk: scenario 测试断言状态变更（文件内容/文本）；TS-vs-TS 断言 diff 为 null。
- [ ] 7. Type consistency: 复用 Part 1 的 `Scenario`、`ParityBackend`、`ParityDiff` 等类型。

---

### Task B1: Scenario Helpers + Multi-Turn MockChatProvider

**Depends on:** `core.md: Task A4`, `core.md: Task A6`

**Files:**
- Modify: `packages/integration-tests/src/parity/fixtures/mock-provider.ts`
- Create: `packages/integration-tests/src/parity/scenarios/utils.ts`
- Create: `packages/integration-tests/test/parity/scenarios/utils.test.ts`

**Goal:** 让 `MockChatProvider` 支持多轮对话（每次 `generate` 按顺序取下一组 response），并新增 scenario 等待事件的小工具。

- [ ] 在 `packages/integration-tests/test/parity/scenarios/utils.test.ts` 写入测试：

```ts
import { describe, expect, it } from 'vitest';
import type { AgentEvent } from '@odysseythink/agent-core';
import { MockChatProvider } from '../../../src/parity/fixtures/mock-provider';
import { waitForEvent, waitForTurnEnded } from '../../../src/parity/scenarios/utils';

function fakeClient(eventsToEmit: AgentEvent[] = []) {
  const listeners = new Set<(event: { event: AgentEvent }) => void>();
  return {
    onEvent(listener: (event: { event: AgentEvent }) => void) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    emit(event: AgentEvent) {
      listeners.forEach((l) => l({ event }));
    },
  } as any;
}

describe('MockChatProvider multi-turn', () => {
  it('cycles through multiple responses', async () => {
    const provider = new MockChatProvider([
      [{ type: 'text', text: 'first' }],
      [{ type: 'text', text: 'second' }],
    ]);
    const msg1 = await provider.generate('', [], []);
    const chunks1 = await collectChunks(msg1);
    expect(chunks1).toEqual([{ type: 'text', text: 'first' }]);

    const msg2 = await provider.generate('', [], []);
    const chunks2 = await collectChunks(msg2);
    expect(chunks2).toEqual([{ type: 'text', text: 'second' }]);
  });

  it('still supports single-response constructor', async () => {
    const provider = new MockChatProvider([{ type: 'text', text: 'hello' }]);
    const msg = await provider.generate('', [], []);
    const chunks = await collectChunks(msg);
    expect(chunks).toEqual([{ type: 'text', text: 'hello' }]);
  });

  async function collectChunks(msg: { [Symbol.asyncIterator](): AsyncIterable<unknown> }) {
    const out: unknown[] = [];
    for await (const chunk of msg) out.push(chunk);
    return out;
  }
});

describe('waitForEvent', () => {
  it('resolves when predicate matches', async () => {
    const client = fakeClient();
    const promise = waitForEvent(client, (e) => e.type === 'turn.ended');
    client.emit({ type: 'turn.ended', turnId: 1, reason: 'completed' } as any);
    const event = await promise;
    expect(event.type).toBe('turn.ended');
  });

  it('rejects on timeout', async () => {
    const client = fakeClient();
    await expect(waitForEvent(client, () => false, { timeoutMs: 10 })).rejects.toThrow('Timeout');
  });
});

describe('waitForTurnEnded', () => {
  it('resolves on turn.ended', async () => {
    const client = fakeClient();
    const promise = waitForTurnEnded(client);
    client.emit({ type: 'turn.ended', turnId: 1, reason: 'completed' } as any);
    await expect(promise).resolves.toBeDefined();
  });
});
```

- [ ] 运行测试并确认失败：

```bash
pnpm --filter integration-tests vitest run test/parity/scenarios/utils.test.ts
```

预期失败：文件不存在或 `waitForEvent` / `MockChatProvider` 缺少多轮能力。

- [ ] 修改 `packages/integration-tests/src/parity/fixtures/mock-provider.ts`：

```ts
export class MockChatProvider implements ChatProvider {
  readonly name = 'mock';
  readonly modelName: string;
  readonly thinkingEffort: ThinkingEffort | null = null;
  private callIndex = 0;

  constructor(
    private readonly partsOrResponses: StreamedMessagePart[] | StreamedMessagePart[][],
    private readonly options: MockChatProviderOptions = {},
  ) {
    this.modelName = options.modelName ?? 'mock';
  }

  private currentParts(): StreamedMessagePart[] {
    const first = (this.partsOrResponses as StreamedMessagePart[][])[0];
    if (Array.isArray(first)) {
      const responses = this.partsOrResponses as StreamedMessagePart[][];
      const parts = responses[this.callIndex % responses.length];
      this.callIndex++;
      return parts;
    }
    return this.partsOrResponses as StreamedMessagePart[];
  }

  async generate(
    _systemPrompt: string,
    _tools: Tool[],
    _history: Message[],
    _options?: GenerateOptions,
  ): Promise<StreamedMessage> {
    const parts = this.currentParts();
    const id = this.options.id ?? 'mock';
    const finishReason = this.options.finishReason ?? 'completed';
    const rawFinishReason = this.options.rawFinishReason ?? 'stop';
    const usage = this.options.usage ?? null;
    return {
      id,
      usage,
      finishReason,
      rawFinishReason,
      async *[Symbol.asyncIterator]() {
        for (const part of parts) {
          yield part;
        }
      },
    };
  }

  getCapability(_model?: string): ModelCapability {
    return UNKNOWN_CAPABILITY;
  }

  withThinking(_effort: ThinkingEffort): MockChatProvider {
    return new MockChatProvider([...this.partsOrResponses as StreamedMessagePart[][]], this.options);
  }
}
```

- [ ] 在 `packages/integration-tests/src/parity/scenarios/utils.ts` 写入实现：

```ts
import type { AgentEvent, Event } from '@odysseythink/agent-core';
import type { SDKRpcClient } from '@odysseythink/ody-code-sdk';

export interface WaitOptions {
  readonly timeoutMs?: number;
}

export function waitForEvent(
  client: SDKRpcClient,
  predicate: (event: AgentEvent) => boolean,
  options: WaitOptions = {},
): Promise<AgentEvent> {
  const { timeoutMs = 10000 } = options;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      unsubscribe();
      reject(new Error(`Timeout after ${timeoutMs}ms waiting for event`));
    }, timeoutMs);

    const unsubscribe = client.onEvent((wrapper: Event) => {
      const event = wrapper.event;
      if (predicate(event)) {
        clearTimeout(timer);
        unsubscribe();
        resolve(event);
      }
    });
  });
}

export function waitForTurnEnded(
  client: SDKRpcClient,
  options: WaitOptions = {},
): Promise<AgentEvent> {
  return waitForEvent(client, (event) => event.type === 'turn.ended', options);
}
```

- [ ] 运行测试并确认通过：

```bash
pnpm --filter integration-tests vitest run test/parity/scenarios/utils.test.ts
```

预期：5 个用例全部通过。

- [ ] 提交：

```bash
git add packages/integration-tests/src/parity/fixtures/mock-provider.ts \
           packages/integration-tests/src/parity/scenarios/utils.ts \
           packages/integration-tests/test/parity/scenarios/utils.test.ts
git commit -m "feat(integration-tests): scenario helpers + multi-turn mock provider"
```

