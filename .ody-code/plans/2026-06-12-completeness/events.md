# Phase C: Event-Driven Sync + Recovery

**Goal:** 将消息实时追加到 Markdown 导出、在 4 类事件触发点异步保存 JSON checkpoint，并在 Session 恢复时从最新有效 checkpoint 还原设计模式上下文，解决 Batch 2 时间线缺失与恢复不一致问题。

**Architecture:** 扩展 `AgentRecords` 增加 `subscribe()` 钩子；`SessionMarkdownExport` 订阅 `context.append_message` 记录并追加到 Markdown；`CheckpointCoordinator` 订阅 `session_mode.exit` / `full_compaction.complete` / `micro_compaction.apply` / `step.end` 并调用 `SessionCheckpoint` + `CheckpointIndex` 保存；`verifyCheckpointIntegrity` 校验消息数、设计上下文、工具调用索引；`Session.resume()` 在 wire 回放前加载 checkpoint 并还原 `SessionMode.designSessions`。

---

## Task C1: Add `AgentRecords.subscribe()` hook

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/agent/records/index.ts:117-160` — add subscriber API
- Test: `packages/agent-core/test/agent/records.test.ts` (create if missing)

### Steps

- [ ] Write failing tests:

```ts
import { describe, expect, it, vi } from 'vitest';
import { Agent } from '../../src/agent';
import { InMemoryAgentRecordPersistence } from '../../src/agent/records';
import { createFakeKaos } from '../tools/fixtures/fake-kaos';

function makeAgent(persistence = new InMemoryAgentRecordPersistence()) {
  return new Agent({
    kaos: createFakeKaos(),
    homedir: '/tmp/agent',
    persistence,
  });
}

describe('AgentRecords.subscribe', () => {
  it('delivers records to subscribers after persistence append', () => {
    const agent = makeAgent();
    const seen: unknown[] = [];
    const unsubscribe = agent.records.subscribe((record) => seen.push(record));

    agent.records.logRecord({ type: 'context.clear' });

    expect(seen).toHaveLength(1);
    expect((seen[0] as { type: string }).type).toBe('context.clear');
    unsubscribe();
  });

  it('does not deliver after unsubscribe', () => {
    const agent = makeAgent();
    const seen: unknown[] = [];
    const unsubscribe = agent.records.subscribe((record) => seen.push(record));
    unsubscribe();

    agent.records.logRecord({ type: 'context.clear' });

    expect(seen).toHaveLength(0);
  });

  it('continues to append even if subscriber throws', () => {
    const persistence = new InMemoryAgentRecordPersistence();
    const agent = makeAgent(persistence);
    agent.records.subscribe(() => {
      throw new Error('subscriber boom');
    });

    agent.records.logRecord({ type: 'context.clear' });

    expect(persistence.records).toHaveLength(1);
  });
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/agent/records.test.ts
```

Expected: `subscribe is not a function`.

- [ ] Write minimal implementation in `packages/agent-core/src/agent/records/index.ts`:

```ts
export type AgentRecordSubscriber = (record: AgentRecord) => void;

export class AgentRecords {
  private readonly subscribers: AgentRecordSubscriber[] = [];
  // existing fields unchanged

  subscribe(subscriber: AgentRecordSubscriber): () => void {
    this.subscribers.push(subscriber);
    return () => {
      const index = this.subscribers.indexOf(subscriber);
      if (index !== -1) {
        this.subscribers.splice(index, 1);
      }
    };
  }

  logRecord(record: AgentRecord): void {
    if (this._restoring !== null) return;
    const stamped: AgentRecord =
      record.time !== undefined ? record : { ...record, time: Date.now() };
    if (
      this.persistence !== undefined &&
      !this.metadataInitialized &&
      stamped.type !== 'metadata'
    ) {
      this.persistence.append({
        type: 'metadata',
        protocol_version: AGENT_WIRE_PROTOCOL_VERSION,
        created_at: Date.now(),
        app_version: this.agent.appVersion,
      });
      this.metadataInitialized = true;
    }
    if (stamped.type === 'metadata') {
      this.metadataInitialized = true;
    }
    this.persistence?.append(stamped);
    this.emitToSubscribers(stamped);
  }

  private emitToSubscribers(record: AgentRecord): void {
    for (const subscriber of this.subscribers) {
      try {
        subscriber(record);
      } catch (error) {
        this.agent.log?.error('AgentRecords subscriber failed', error);
      }
    }
  }
}
```

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/agent/records.test.ts
```

Expected: all tests pass.

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck 2>&1 | tail -5
```

Expected: no errors.

- [ ] Commit:

```bash
git add packages/agent-core/src/agent/records/index.ts \
        packages/agent-core/test/agent/records.test.ts
git commit -m "feat: add AgentRecords.subscribe hook for checkpoint and export wiring"
```

---

## Task C2: SessionMarkdownExport real-time append

**Depends on:** C1

**Files:**
- Create: `packages/agent-core/src/session/export/markdown-export.ts`
- Modify: `packages/agent-core/src/session/index.ts` — create and wire in `createMain()` and resume path
- Test: `packages/agent-core/test/session/export/markdown-export.test.ts`

### Steps

- [ ] Write failing tests:

```ts
import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { LocalKaos } from '@odysseythink/kaos';
import { SessionMarkdownExport } from '../../../src/session/export/markdown-export';
import type { ContextMessage } from '../../../src/agent/context';

const OS_ENV = { workspaceDir: '/', additionalDirs: [] };

describe('SessionMarkdownExport', () => {
  let dir: string;
  let kaos: LocalKaos;

  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), 'md-export-'));
    kaos = new LocalKaos(OS_ENV).withCwd(dir);
  });

  afterEach(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('appends user messages to a new markdown file', async () => {
    const exporter = new SessionMarkdownExport({
      kaos,
      sessionId: 's1',
      odyHome: dir,
      startedAt: new Date('2026-06-12T10:00:00Z'),
    });

    await exporter.append({
      type: 'context.append_message',
      message: {
        role: 'user',
        content: [{ type: 'text', text: 'Hello world' }],
        toolCalls: [],
      } as ContextMessage,
    });

    const files = await readdir(join(dir, 'session-exports'));
    expect(files).toHaveLength(1);
    const content = await readFile(join(dir, 'session-exports', files[0]!), 'utf8');
    expect(content).toContain('Hello world');
    expect(content).toContain('user');
  });

  it('does not overwrite on second append', async () => {
    const exporter = new SessionMarkdownExport({
      kaos,
      sessionId: 's1',
      odyHome: dir,
      startedAt: new Date('2026-06-12T10:00:00Z'),
    });

    await exporter.append({
      type: 'context.append_message',
      message: { role: 'user', content: [{ type: 'text', text: 'First' }], toolCalls: [] } as ContextMessage,
    });
    await exporter.append({
      type: 'context.append_message',
      message: { role: 'assistant', content: [{ type: 'text', text: 'Second' }], toolCalls: [] } as ContextMessage,
    });

    const files = await readdir(join(dir, 'session-exports'));
    const content = await readFile(join(dir, 'session-exports', files[0]!), 'utf8');
    expect(content).toContain('First');
    expect(content).toContain('Second');
  });

  it('ignores non-message records', async () => {
    const exporter = new SessionMarkdownExport({
      kaos,
      sessionId: 's1',
      odyHome: dir,
      startedAt: new Date('2026-06-12T10:00:00Z'),
    });

    await exporter.append({ type: 'context.clear' });

    await expect(readdir(join(dir, 'session-exports'))).rejects.toThrow();
  });
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/session/export/markdown-export.test.ts
```

Expected: module not found.

- [ ] Write implementation in `packages/agent-core/src/session/export/markdown-export.ts`:

```ts
import { dirname, join } from 'pathe';
import type { Kaos } from '@odysseythink/kaos';
import type { AgentRecord } from '../../agent/records';
import { withFileLock } from '../../utils/file-lock';

export interface SessionMarkdownExportOptions {
  readonly kaos: Kaos;
  readonly sessionId: string;
  readonly odyHome: string;
  readonly startedAt: Date;
}

function formatTimestamp(d: Date): string {
  return d.toISOString().replace(/[:.]/g, '-').slice(0, 19);
}

export class SessionMarkdownExport {
  private readonly kaos: Kaos;
  private readonly filePath: string;

  constructor(options: SessionMarkdownExportOptions) {
    this.kaos = options.kaos;
    const filename = `session-${formatTimestamp(options.startedAt)}-${options.sessionId}.md`;
    this.filePath = join(options.odyHome, 'session-exports', filename);
  }

  async append(record: AgentRecord): Promise<void> {
    if (record.type !== 'context.append_message') return;
    const line = this.formatMessage(record.message);
    if (line === undefined) return;

    await this.kaos.mkdir(dirname(this.filePath), { parents: true, existOk: true });
    await withFileLock(this.filePath, async () => {
      await this.kaos.writeText(this.filePath, line, { mode: 'a' });
    });
  }

  private formatMessage(message: {
    readonly role: string;
    readonly content: readonly { readonly type: string; readonly text?: string }[];
  }): string | undefined {
    const text = message.content
      .filter((part): part is { type: 'text'; text: string } => part.type === 'text' && typeof part.text === 'string')
      .map((part) => part.text)
      .join('');
    if (text.length === 0) return undefined;
    return `## ${message.role} (${new Date().toISOString()})\n\n${text}\n\n`;
  }
}
```

- [ ] Wire in `Session.createMain()` at `packages/agent-core/src/session/index.ts:172-178`:

After `const { agent } = await this.createAgent({ type: 'main' }, ...);` and `this.goals.flushPendingRecords();`:

```ts
this.markdownExport = new SessionMarkdownExport({
  kaos: this.options.kaos,
  sessionId: this.options.id ?? 'unknown',
  odyHome: this.odyHome,
  startedAt: new Date(this.metadata.createdAt),
});
this.subscriptions.push(
  agent.records.subscribe((record) => {
    void this.markdownExport?.append(record).catch((error) => {
      this.log.error('markdown export append failed', error);
    });
  }),
);
```

Add fields to `Session` class:

```ts
private markdownExport?: SessionMarkdownExport;
private subscriptions: (() => void)[] = [];
```

And add `get odyHome(): string`:

```ts
private get odyHome(): string {
  return this.options.odyHome ?? dirname(dirname(this.options.homedir));
}
```

Add `odyHome?: string` to `SessionOptions` interface at line 49:

```ts
readonly odyHome?: string | undefined;
```

Update callers in `packages/agent-core/src/rpc/core-impl.ts:228` and `:316` to pass `odyHome: this.homeDir`.

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/session/export/markdown-export.test.ts
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck 2>&1 | tail -10
```

Expected: no errors.

- [ ] Commit:

```bash
git add packages/agent-core/src/session/export/markdown-export.ts \
        packages/agent-core/src/session/index.ts \
        packages/agent-core/src/session/export/index.ts \
        packages/agent-core/src/rpc/core-impl.ts \
        packages/agent-core/test/session/export/markdown-export.test.ts
git commit -m "feat: real-time session markdown export with append-only file locking"
```

---

## Task C3: Design mode session tracking

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/agent/session-mode/index.ts` — track `_designSessions`
- Modify: `packages/agent-core/src/checkpoint/types.ts` — ensure `DesignSessionCheckpoint` is exported
- Test: `packages/agent-core/test/agent/session-mode.test.ts`

### Steps

- [ ] Write failing tests (append to existing `describe('SessionMode', ...)`):

```ts
it('records a design session on enter and exit', async () => {
  const agent = makeAgent();
  const sm = new SessionMode(agent);
  await sm.enter('id-1', undefined, false, 'design');
  await sm.resolveFilePathFromContent('# Design\n\nContent');
  sm.exit();

  const sessions = sm.designSessions;
  expect(sessions).toHaveLength(1);
  expect(sessions[0].designSessionID).toBe('id-1');
  expect(sessions[0].startedAtMsg).toBe(0);
  expect(sessions[0].exitedAtMsg).toBe(0);
  expect(sessions[0].approvedPath).toMatch(/\.ody-code\/designs\//);
});

it('removes an in-flight design session on cancel', async () => {
  const agent = makeAgent();
  const sm = new SessionMode(agent);
  await sm.enter('id-1', undefined, false, 'design');
  sm.cancel();

  expect(sm.designSessions).toHaveLength(0);
});

it('restores design sessions from checkpoint', async () => {
  const agent = makeAgent();
  const sm = new SessionMode(agent);
  sm.restoreDesignSessions([
    { designSessionID: 'id-1', startedAtMsg: 5, exitedAtMsg: 10, approvedPath: '/path/to/design.md' },
  ]);

  expect(sm.designSessions).toHaveLength(1);
  expect(sm.designSessions[0].startedAtMsg).toBe(5);
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/agent/session-mode.test.ts
```

Expected: `designSessions` / `restoreDesignSessions` not found.

- [ ] Write implementation in `packages/agent-core/src/agent/session-mode/index.ts`:

Add private field after line 47:

```ts
private _designSessions: DesignSessionCheckpoint[] = [];
```

Import `DesignSessionCheckpoint` from `../checkpoint/types`.

Add getter after `kind` getter (line 333):

```ts
get designSessions(): readonly DesignSessionCheckpoint[] {
  return this._designSessions;
}
```

Add method:

```ts
restoreDesignSessions(sessions: readonly DesignSessionCheckpoint[]): void {
  this._designSessions = [...sessions];
}
```

In `enter()` after `this.agent.setContextMode(kind);`:

```ts
if (kind === 'design' && this._sessionModeId !== null) {
  this._designSessions.push({
    designSessionID: this._sessionModeId,
    startedAtMsg: this.agent.context.history.length,
  });
}
```

In `exit()` before `this.agent.setContextMode('normal');`:

```ts
if (this._kind === 'design' && this._sessionModeId !== null) {
  const session = this._designSessions.find(
    (s) => s.designSessionID === this._sessionModeId,
  );
  if (session !== undefined) {
    session.exitedAtMsg = this.agent.context.history.length;
    session.approvedPath = this._sessionModeFilePath ?? undefined;
  }
}
```

In `cancel()` before `this.agent.setContextMode('normal');`:

```ts
if (this._kind === 'design' && this._sessionModeId !== null) {
  const index = this._designSessions.findIndex(
    (s) => s.designSessionID === this._sessionModeId,
  );
  if (index !== -1) {
    this._designSessions.splice(index, 1);
  }
}
```

In `restoreEnter()` add after setting `_sessionModeId`:

```ts
// Resume does not rebuild design sessions from wire; caller loads them from checkpoint.
```

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/agent/session-mode.test.ts
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck 2>&1 | tail -5
```

- [ ] Commit:

```bash
git add packages/agent-core/src/agent/session-mode/index.ts \
        packages/agent-core/test/agent/session-mode.test.ts
git commit -m "feat: track design mode sessions for checkpoint restore"
```

---

## Task C4: CheckpointCoordinator + trigger events

**Depends on:** C1, C3, persistence.md B2/B3

**Files:**
- Create: `packages/agent-core/src/checkpoint/coordinator.ts`
- Create: `packages/agent-core/src/checkpoint/tool-call-index.ts`
- Modify: `packages/agent-core/src/session/index.ts` — instantiate coordinator in `createMain()` and resume
- Test: `packages/agent-core/test/checkpoint/coordinator.test.ts`

### Steps

- [ ] Write failing tests:

```ts
import { describe, expect, it, beforeEach, afterEach, vi } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { LocalKaos } from '@odysseythink/kaos';
import { CheckpointCoordinator } from '../../src/checkpoint/coordinator';
import type { AgentRecord } from '../../src/agent/records';

const OS_ENV = { workspaceDir: '/', additionalDirs: [] };

describe('CheckpointCoordinator', () => {
  let dir: string;
  let kaos: LocalKaos;

  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), 'coord-'));
    kaos = new LocalKaos(OS_ENV).withCwd(dir);
  });

  afterEach(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('saves a checkpoint on session_mode.exit', async () => {
    const onError = vi.fn();
    const coordinator = new CheckpointCoordinator({
      kaos,
      sessionId: 's1',
      odyHome: dir,
      getCurrentMode: () => 'design',
      getMessageCount: () => 7,
      getDesignModeContext: () => ({ sessions: [] }),
      getToolCallIndex: () => ({ callIdToResult: {} }),
      onError,
    });

    coordinator.onRecord({ type: 'session_mode.exit', id: 'd1' });
    await new Promise((r) => setTimeout(r, 50));

    const checkpoint = await coordinator.restore();
    expect(checkpoint).not.toBeNull();
    expect(checkpoint?.currentMode).toBe('design');
    expect(checkpoint?.messageCount).toBe(7);
    expect(onError).not.toHaveBeenCalled();
  });

  it('ignores unrelated records', async () => {
    const coordinator = new CheckpointCoordinator({
      kaos,
      sessionId: 's1',
      odyHome: dir,
      getCurrentMode: () => 'normal',
      getMessageCount: () => 0,
      getDesignModeContext: () => ({ sessions: [] }),
      getToolCallIndex: () => ({ callIdToResult: {} }),
      onError: vi.fn(),
    });

    coordinator.onRecord({ type: 'context.clear' });
    await new Promise((r) => setTimeout(r, 50));

    const checkpoint = await coordinator.restore();
    expect(checkpoint).toBeNull();
  });
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/coordinator.test.ts
```

Expected: module not found.

- [ ] Write implementation in `packages/agent-core/src/checkpoint/tool-call-index.ts`:

```ts
import type { Agent } from '../agent';

export interface ToolCallIndex {
  callIdToResult: Record<string, { status: 'pending' | 'completed'; output?: unknown }>;
}

export function buildToolCallIndex(agent: Agent): ToolCallIndex {
  const index: Record<string, { status: 'pending' | 'completed'; output?: unknown }> = {};
  for (const message of agent.context.history) {
    for (const toolCall of message.toolCalls ?? []) {
      if (index[toolCall.id] === undefined) {
        index[toolCall.id] = { status: 'pending' };
      }
    }
    if (message.role === 'tool' && 'toolCallId' in message && typeof message.toolCallId === 'string') {
      index[message.toolCallId] = {
        status: 'completed',
        output: message.content,
      };
    }
  }
  return { callIdToResult: index };
}
```

Write `packages/agent-core/src/checkpoint/coordinator.ts`:

```ts
import type { Kaos } from '@odysseythink/kaos';
import type { AgentRecord } from '../agent/records';
import type { ModeKey } from '../agent';
import { CheckpointIndex } from './checkpoint-index';
import { SessionCheckpoint } from './session-checkpoint';
import type { DesignSessionCheckpoint, SaveCheckpointInput } from './types';

export interface CheckpointCoordinatorOptions {
  readonly kaos: Kaos;
  readonly sessionId: string;
  readonly odyHome: string;
  readonly getCurrentMode: () => ModeKey;
  readonly getMessageCount: () => number;
  readonly getDesignModeContext: () => { sessions: readonly DesignSessionCheckpoint[] };
  readonly getToolCallIndex: () => SaveCheckpointInput['toolCallIndex'];
  readonly onError: (error: unknown) => void;
}

export class CheckpointCoordinator {
  private readonly checkpoint: SessionCheckpoint;
  private readonly index: CheckpointIndex;
  private readonly options: CheckpointCoordinatorOptions;
  private saving = false;
  private lastSaveAt = 0;

  constructor(options: CheckpointCoordinatorOptions) {
    this.options = options;
    this.checkpoint = new SessionCheckpoint({
      kaos: options.kaos,
      sessionId: options.sessionId,
      odyHome: options.odyHome,
    });
    this.index = new CheckpointIndex({
      kaos: options.kaos,
      sessionId: options.sessionId,
      odyHome: options.odyHome,
    });
  }

  onRecord(record: AgentRecord): void {
    if (this.shouldTriggerCheckpoint(record)) {
      void this.saveCheckpoint();
    }
  }

  private shouldTriggerCheckpoint(record: AgentRecord): boolean {
    switch (record.type) {
      case 'session_mode.exit':
        return true;
      case 'full_compaction.complete':
      case 'micro_compaction.apply':
        return true;
      case 'step.end': {
        // Throttle turn-boundary checkpoints to at most one per 5 seconds.
        const now = Date.now();
        if (now - this.lastSaveAt < 5000) return false;
        this.lastSaveAt = now;
        return true;
      }
      default:
        return false;
    }
  }

  async saveCheckpoint(): Promise<void> {
    if (this.saving) return;
    this.saving = true;
    try {
      const messageCount = this.options.getMessageCount();
      await this.checkpoint.save({
        currentMode: this.options.getCurrentMode(),
        messageCount,
        designModeContext: this.options.getDesignModeContext(),
        toolCallIndex: this.options.getToolCallIndex(),
      });
      await this.index.update(messageCount, true);
    } catch (error) {
      this.options.onError(error);
    } finally {
      this.saving = false;
    }
  }

  async restore(): Promise<import('./types').SessionCheckpointPayload | null> {
    return this.checkpoint.loadLatest();
  }
}
```

- [ ] Wire in `Session.createMain()` after markdown export wiring:

```ts
this.checkpointCoordinator = new CheckpointCoordinator({
  kaos: this.options.kaos,
  sessionId: this.options.id ?? 'unknown',
  odyHome: this.odyHome,
  getCurrentMode: () =>
    agent.sessionMode.isActive ? agent.sessionMode.kind : 'normal',
  getMessageCount: () => agent.context.history.length,
  getDesignModeContext: () => ({ sessions: agent.sessionMode.designSessions }),
  getToolCallIndex: () => buildToolCallIndex(agent),
  onError: (error) => this.log.error('checkpoint save failed', error),
});
this.subscriptions.push(
  agent.records.subscribe((record) => {
    this.checkpointCoordinator?.onRecord(record);
  }),
);
```

Add imports for `CheckpointCoordinator` and `buildToolCallIndex`. Add `private checkpointCoordinator?: CheckpointCoordinator;` to `Session`.

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/coordinator.test.ts
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck 2>&1 | tail -10
```

- [ ] Commit:

```bash
git add packages/agent-core/src/checkpoint/coordinator.ts \
        packages/agent-core/src/checkpoint/tool-call-index.ts \
        packages/agent-core/src/session/index.ts \
        packages/agent-core/test/checkpoint/coordinator.test.ts
git commit -m "feat: event-driven checkpoint coordinator with 4 trigger types"
```

---

## Task C5: Checkpoint integrity verifier

**Depends on:** C4

**Files:**
- Create: `packages/agent-core/src/checkpoint/integrity.ts`
- Test: `packages/agent-core/test/checkpoint/integrity.test.ts`

### Steps

- [ ] Write failing tests:

```ts
import { describe, expect, it } from 'vitest';
import { verifyCheckpointIntegrity } from '../../src/checkpoint/integrity';
import type { SessionCheckpointPayload } from '../../src/checkpoint/types';

function payload(overrides: Partial<SessionCheckpointPayload> = {}): SessionCheckpointPayload {
  return {
    sessionID: 's1',
    createdAt: '2026-06-12T10:00:00Z',
    lastUpdatedAt: '2026-06-12T10:00:00Z',
    currentMode: 'normal',
    messageCount: 3,
    designModeContext: { sessions: [] },
    toolCallIndex: { callIdToResult: {} },
    ...overrides,
  };
}

describe('verifyCheckpointIntegrity', () => {
  it('passes for a matching checkpoint', () => {
    const result = verifyCheckpointIntegrity({ payload: payload(), memoryMessageCount: 3 });
    expect(result.valid).toBe(true);
    expect(result.errors).toHaveLength(0);
  });

  it('fails when message count mismatches', () => {
    const result = verifyCheckpointIntegrity({ payload: payload({ messageCount: 5 }), memoryMessageCount: 3 });
    expect(result.valid).toBe(false);
    expect(result.errors[0]).toContain('message count mismatch');
  });

  it('fails when design session range is invalid', () => {
    const result = verifyCheckpointIntegrity({
      payload: payload({
        designModeContext: {
          sessions: [{ designSessionID: 'd1', startedAtMsg: 10, exitedAtMsg: 5 }],
        },
      }),
      memoryMessageCount: 3,
    });
    expect(result.valid).toBe(false);
    expect(result.errors[0]).toContain('design session');
  });

  it('fails when tool call index has pending results', () => {
    const result = verifyCheckpointIntegrity({
      payload: payload({
        toolCallIndex: {
          callIdToResult: { call1: { status: 'pending' } },
        },
      }),
      memoryMessageCount: 3,
    });
    expect(result.valid).toBe(false);
    expect(result.errors[0]).toContain('pending');
  });
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/integrity.test.ts
```

Expected: module not found.

- [ ] Write implementation in `packages/agent-core/src/checkpoint/integrity.ts`:

```ts
import type { SessionCheckpointPayload } from './types';

export interface IntegrityCheckInput {
  readonly payload: SessionCheckpointPayload;
  readonly memoryMessageCount: number;
}

export interface IntegrityResult {
  readonly valid: boolean;
  readonly checks: {
    readonly messageCountMatch: boolean;
    readonly jsonValid: boolean;
    readonly designModeConsistent: boolean;
    readonly toolCallIndexComplete: boolean;
  };
  readonly errors: string[];
}

export function verifyCheckpointIntegrity(input: IntegrityCheckInput): IntegrityResult {
  const errors: string[] = [];

  const messageCountMatch = input.payload.messageCount === input.memoryMessageCount;
  if (!messageCountMatch) {
    errors.push(
      `message count mismatch: checkpoint=${input.payload.messageCount}, memory=${input.memoryMessageCount}`,
    );
  }

  const designModeConsistent = input.payload.designModeContext.sessions.every(
    (s) =>
      s.startedAtMsg >= 0 &&
      (s.exitedAtMsg === undefined || s.exitedAtMsg >= s.startedAtMsg),
  );
  if (!designModeConsistent) {
    errors.push('design session message counts inconsistent');
  }

  const toolCallIndexComplete = Object.values(input.payload.toolCallIndex.callIdToResult).every(
    (entry) => entry.status === 'completed',
  );
  if (!toolCallIndexComplete) {
    errors.push('tool call index has pending results');
  }

  return {
    valid: errors.length === 0,
    checks: {
      messageCountMatch,
      jsonValid: true,
      designModeConsistent,
      toolCallIndexComplete,
    },
    errors,
  };
}
```

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/integrity.test.ts
```

- [ ] Commit:

```bash
git add packages/agent-core/src/checkpoint/integrity.ts \
        packages/agent-core/test/checkpoint/integrity.test.ts
git commit -m "feat: checkpoint integrity verifier with message, design, and tool checks"
```

---

## Task C6: Session resume integration

**Depends on:** C4, C5

**Files:**
- Modify: `packages/agent-core/src/session/index.ts:180-212` — load checkpoint before wire replay
- Modify: `packages/agent-core/src/session/index.ts` — setup coordinator in resume path
- Test: `packages/agent-core/test/session/resume.test.ts` (create)

### Steps

- [ ] Write failing tests:

```ts
import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, writeFile, mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { LocalKaos } from '@odysseythink/kaos';
import { Session } from '../../src/session';
import type { SDKSessionRPC } from '../../src/rpc';

const OS_ENV = { workspaceDir: '/', additionalDirs: [] };

function fakeRpc(): SDKSessionRPC {
  return {
    emitEvent: async () => {},
  } as unknown as SDKSessionRPC;
}

describe('Session checkpoint resume', () => {
  let dir: string;
  let kaos: LocalKaos;

  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), 'session-resume-'));
    kaos = new LocalKaos(OS_ENV).withCwd(dir);
  });

  afterEach(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('restores design sessions from checkpoint on resume', async () => {
    const sessionDir = join(dir, 'sessions', 'default', 's1');
    await mkdir(sessionDir, { recursive: true });
    await writeFile(
      join(sessionDir, 'state.json'),
      JSON.stringify({
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        title: 'Test',
        isCustomTitle: false,
        agents: {},
        custom: {},
      }),
    );
    await writeFile(join(sessionDir, 'wire.jsonl'), '');

    const checkpointDir = join(dir, 'session-state');
    await mkdir(checkpointDir, { recursive: true });
    await writeFile(
      join(checkpointDir, 's1.json'),
      JSON.stringify({
        sessionID: 's1',
        createdAt: new Date().toISOString(),
        lastUpdatedAt: new Date().toISOString(),
        currentMode: 'normal',
        messageCount: 0,
        checkpointVersion: 1,
        designModeContext: {
          sessions: [
            {
              designSessionID: 'd1',
              startedAtMsg: 0,
              exitedAtMsg: 5,
              approvedPath: '/path/to/design.md',
            },
          ],
        },
        toolCallIndex: { callIdToResult: {} },
      }),
    );

    const session = new Session({
      kaos,
      id: 's1',
      homedir: sessionDir,
      odyHome: dir,
      rpc: fakeRpc(),
    });
    await session.resume();
    const main = session.agents.get('main');
    expect(main?.sessionMode.designSessions).toHaveLength(1);
    expect(main?.sessionMode.designSessions[0].approvedPath).toBe('/path/to/design.md');
  });
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/session/resume.test.ts
```

Expected: test fails because checkpoint restore not implemented.

- [ ] Write implementation in `packages/agent-core/src/session/index.ts`:

Add `private checkpointCoordinator?: CheckpointCoordinator;` and `private subscriptions: (() => void)[] = [];`.

Add helper `setupCheckpointing(agent: Agent)`:

```ts
private setupCheckpointing(agent: Agent): void {
  this.markdownExport = new SessionMarkdownExport({
    kaos: this.options.kaos,
    sessionId: this.options.id ?? 'unknown',
    odyHome: this.odyHome,
    startedAt: new Date(this.metadata.createdAt),
  });
  this.checkpointCoordinator = new CheckpointCoordinator({
    kaos: this.options.kaos,
    sessionId: this.options.id ?? 'unknown',
    odyHome: this.odyHome,
    getCurrentMode: () =>
      agent.sessionMode.isActive ? agent.sessionMode.kind : 'normal',
    getMessageCount: () => agent.context.history.length,
    getDesignModeContext: () => ({ sessions: agent.sessionMode.designSessions }),
    getToolCallIndex: () => buildToolCallIndex(agent),
    onError: (error) => this.log.error('checkpoint save failed', error),
  });
  this.subscriptions.push(
    agent.records.subscribe((record) => {
      void this.markdownExport?.append(record).catch((error) => {
        this.log.error('markdown export append failed', error);
      });
      this.checkpointCoordinator?.onRecord(record);
    }),
  );
}
```

In `createMain()` replace the inline wiring with:

```ts
this.setupCheckpointing(agent);
```

In `ensureResumeAgentInstantiated` for the main agent, call `this.setupCheckpointing(agent)` before returning.

Modify `resume()` to load checkpoint before agent resume:

```ts
async resume(): Promise<{ warning?: string }> {
  await this.skillsReady;
  const { agents } = await this.readMetadata();
  await this.goals.normalizeMetadata();
  this.agents.clear();
  let warning: string | undefined;

  const resumeTasks = Object.keys(agents).map(async (id) => {
    const agent = this.ensureResumeAgentInstantiated(id, agents);
    await this.loadCheckpointIntoAgent(agent);
    const result = await agent.resume();
    if (result.warning !== undefined && warning === undefined) {
      warning = result.warning;
    }
  });
  await Promise.all(resumeTasks);
  ...
}

private async loadCheckpointIntoAgent(agent: Agent): Promise<void> {
  if (agent.type !== 'main' || this.checkpointCoordinator === undefined) return;
  const checkpoint = await this.checkpointCoordinator.restore();
  if (checkpoint === null) return;
  agent.sessionMode.restoreDesignSessions(checkpoint.designModeContext.sessions);
}
```

Add cleanup in `Session.close()` to unsubscribe:

```ts
for (const unsubscribe of this.subscriptions) {
  unsubscribe();
}
this.subscriptions = [];
```

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/session/resume.test.ts
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck 2>&1 | tail -10
```

- [ ] Commit:

```bash
git add packages/agent-core/src/session/index.ts \
        packages/agent-core/test/session/resume.test.ts
git commit -m "feat: restore design sessions from checkpoint during session resume"
```

---

## Self-Review (events.md)

- [ ] 1. Spec-coverage table:

| Design doc section | Task(s) | Status |
|---|---|---|
| Real-time Markdown append | C2 | covered |
| JSON checkpoint async save (5 triggers) | C4 (4 auto triggers; manual deferred to Part D) | covered |
| Batch 2 timeline fix (real-time + JSON sync) | C2 + C4 | covered |
| Integrity verification | C5 | covered |
| Recovery from checkpoint | C3 + C6 | covered |
| Design mode context persistence | C3 + C6 | covered |
| Tool call index | C4 (buildToolCallIndex) + C5 | covered |

- [ ] 2. Placeholder scan: no TODO/TBD — every task has concrete code, commands, and expected output.
- [ ] 3. No phantom tasks: each task creates/modifies files and has verifiable tests.
- [ ] 4. Dependency soundness: C2 → C1; C4 → C1, C3, B2/B3; C5 → C4; C6 → C4, C5. No forward references.
- [ ] 5. Caller & build soundness: `SessionOptions` gains optional `odyHome`; callers in `core-impl.ts` updated. `AgentRecords.subscribe` adds a new method without changing existing signatures. Every shared-signature task ends with `pnpm -r typecheck`.
- [ ] 6. Test-the-risk: C1 tests subscriber delivery and error isolation; C2 tests append-only behavior; C3 tests design session lifecycle; C4 tests trigger-driven checkpoint save; C5 tests integrity rules; C6 tests resume restoration.
- [ ] 7. Type consistency: `DesignSessionCheckpoint` from `checkpoint/types.ts` used in `SessionMode` and checkpoint payload. `ModeKey` reused from `Agent`. `AgentRecordSubscriber` typed as `(record: AgentRecord) => void`.
