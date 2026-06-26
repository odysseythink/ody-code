# Phase D: Error Handling + Edge Cases

**Goal:** 为 Batch 2 持久化层补齐 8 种错误场景的应对策略：写入重试与磁盘满清理、损坏检查点回退、Markdown 追加失败隔离、恢复索引重建、版本链断裂警告，以及用户手动触发检查点的能力。

**Architecture:** `SessionCheckpoint.save` 写入最新文件的同时保留带版本戳的 backup，并清理超过 10 个的旧版本；保存失败时按错误码重试，磁盘满时先删最旧 backup 再重试。`CheckpointCoordinator.restore` 从 `CheckpointIndex` 拿到版本链后，按从新到旧加载 backup、跑完整性校验，失败则继续回退；全部失败时返回警告让用户选择。`SessionMarkdownExport` 的订阅错误被捕获并记录，永不阻塞主循环。新增 `CheckpointTool` 供模型或用户手动触发保存。

---

## Task D1: Versioned backup files + rotation

**Depends on:** events.md C4, persistence.md B2/B3

**Files:**
- Modify: `packages/agent-core/src/checkpoint/session-checkpoint.ts` — write backups, prune to 10
- Modify: `packages/agent-core/src/checkpoint/checkpoint-index.ts` — `update` accepts `version`
- Modify: `packages/agent-core/src/checkpoint/coordinator.ts` — pass version to index
- Test: `packages/agent-core/test/checkpoint/session-checkpoint.test.ts` (update existing)
- Test: `packages/agent-core/test/checkpoint/checkpoint-index.test.ts` (update existing)

### Steps

- [ ] Write failing tests (append to `session-checkpoint.test.ts`):

```ts
it('writes a backup file and returns version', async () => {
  const cp = new SessionCheckpoint({ kaos, sessionId: 's1', odyHome: dir });
  const { version } = await cp.save({
    currentMode: 'design',
    messageCount: 1,
    designModeContext: { sessions: [] },
    toolCallIndex: { callIdToResult: {} },
  });

  const backupsDir = join(dir, 'session-state', 'backups');
  const files = await readdir(backupsDir);
  expect(files).toHaveLength(1);
  expect(files[0]).toContain('s1-');
  expect(files[0]).toContain(version);
});

it('prunes backups older than the configured limit', async () => {
  const cp = new SessionCheckpoint({ kaos, sessionId: 's1', odyHome: dir });
  for (let i = 0; i < 15; i += 1) {
    await cp.save({
      currentMode: 'design',
      messageCount: i + 1,
      designModeContext: { sessions: [] },
      toolCallIndex: { callIdToResult: {} },
    });
  }

  const backupsDir = join(dir, 'session-state', 'backups');
  const files = await readdir(backupsDir);
  expect(files.length).toBeLessThanOrEqual(10);
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/session-checkpoint.test.ts
```

Expected: backups directory / version not found.

- [ ] Modify `SessionCheckpoint`:

```ts
export interface SaveCheckpointResult {
  readonly path: string;
  readonly version: string;
}

const MAX_BACKUPS = 10;

export class SessionCheckpoint {
  // ... existing fields

  private get backupsDir(): string {
    return join(this.checkpointDir, 'backups');
  }

  async save(input: SaveCheckpointInput): Promise<SaveCheckpointResult> {
    await this.kaos.mkdir(this.checkpointDir, { parents: true, existOk: true });
    await this.kaos.mkdir(this.backupsDir, { parents: true, existOk: true });

    const version = this.now();
    const payload: SessionCheckpointPayload = {
      sessionID: this.sessionId,
      createdAt: version,
      lastUpdatedAt: version,
      currentMode: input.currentMode,
      messageCount: input.messageCount,
      designModeContext: input.designModeContext,
      toolCallIndex: input.toolCallIndex,
    };

    const text = JSON.stringify(
      { ...payload, checkpointVersion: CHECKPOINT_VERSION },
      null,
      2,
    );

    const backupPath = join(this.backupsDir, `${this.sessionId}-${version}.json`);

    await withFileLock(this.checkpointPath, async () => {
      await atomicWrite(this.checkpointPath, text);
      await atomicWrite(backupPath, text);
    });

    await this.pruneOldBackups();

    return { path: this.checkpointPath, version };
  }

  async loadVersion(version: string): Promise<SessionCheckpointPayload | null> {
    const path = join(this.backupsDir, `${this.sessionId}-${version}.json`);
    try {
      const text = await this.kaos.readText(path);
      const parsed = JSON.parse(text) as SessionCheckpointPayload & { checkpointVersion?: number };
      if (parsed.checkpointVersion !== CHECKPOINT_VERSION) return null;
      return parsed;
    } catch (error) {
      const code = (error as { code?: string }).code;
      if (code === 'ENOENT') return null;
      throw error;
    }
  }

  private async pruneOldBackups(): Promise<void> {
    let entries: string[] = [];
    try {
      for await (const entry of this.kaos.iterdir(this.backupsDir)) {
        entries.push(entry);
      }
    } catch {
      return;
    }
    const sorted = entries
      .filter((name) => name.startsWith(`${this.sessionId}-`) && name.endsWith('.json'))
      .sort();
    while (sorted.length > MAX_BACKUPS) {
      const oldest = sorted.shift();
      if (oldest === undefined) break;
      try {
        await this.kaos.unlink(join(this.backupsDir, oldest));
      } catch {
        // Best-effort cleanup.
      }
    }
  }
}
```

Check whether `Kaos` has `unlink`. If not, use Node `fs/promises` `unlink` after resolving path. The `Kaos` interface from earlier did not show `unlink`. Add a note: if `Kaos.unlink` is missing, import `unlink` from `node:fs/promises` and use it directly (backup files are local by design).

- [ ] Update `CheckpointIndex.update` signature at line 529:

```ts
async update(input: { version: string; messageCount: number; valid: boolean }): Promise<void> {
  const { version, messageCount, valid } = input;
  // ... existing body, replace `timestamp = `${now}-${String(versionNumber).padStart(4, '0')}` with `timestamp = version`
}
```

Remove the local `versionNumber`/`now` computation; use the provided `version` as `timestamp`.

- [ ] Update `CheckpointCoordinator.saveCheckpoint`:

```ts
const { version } = await this.checkpoint.save({...});
await this.index.update({ version, messageCount, valid: true });
```

- [ ] Update `CheckpointIndex` tests in `checkpoint-index.test.ts`: replace `await index.update(1, true)` with `await index.update({ version: 'v1', messageCount: 1, valid: true })` and update assertions to use the provided version.

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/session-checkpoint.test.ts test/checkpoint/checkpoint-index.test.ts test/checkpoint/coordinator.test.ts
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck 2>&1 | tail -10
```

- [ ] Commit:

```bash
git add packages/agent-core/src/checkpoint/session-checkpoint.ts \
        packages/agent-core/src/checkpoint/checkpoint-index.ts \
        packages/agent-core/src/checkpoint/coordinator.ts \
        packages/agent-core/src/checkpoint/types.ts \
        packages/agent-core/test/checkpoint/session-checkpoint.test.ts \
        packages/agent-core/test/checkpoint/checkpoint-index.test.ts \
        packages/agent-core/test/checkpoint/coordinator.test.ts
git commit -m "feat: versioned checkpoint backups with rotation"
```

---

## Task D2: Resilient checkpoint save (E1 + E5)

**Depends on:** D1

**Files:**
- Modify: `packages/agent-core/src/checkpoint/session-checkpoint.ts` — retry + ENOSPC prune
- Test: `packages/agent-core/test/checkpoint/session-checkpoint.test.ts`

### Steps

- [ ] Write failing tests:

```ts
it('retries and succeeds after a transient write failure', async () => {
  let attempts = 0;
  const failingKaos = {
    ...kaos,
    writeText: vi.fn(async (path: string, data: string, options?: { mode?: string }) => {
      attempts += 1;
      if (attempts < 2) {
        throw Object.assign(new Error('EAGAIN'), { code: 'EAGAIN' });
      }
      return kaos.writeText(path, data, options);
    }),
  } as unknown as LocalKaos;

  const cp = new SessionCheckpoint({ kaos: failingKaos, sessionId: 's1', odyHome: dir });
  await cp.save({
    currentMode: 'design',
    messageCount: 1,
    designModeContext: { sessions: [] },
    toolCallIndex: { callIdToResult: {} },
  });

  expect(attempts).toBe(2);
  const loaded = await cp.loadLatest();
  expect(loaded).not.toBeNull();
});

it('prunes oldest backup on ENOSPC and retries', async () => {
  // Pre-seed old backups
  const cp = new SessionCheckpoint({ kaos, sessionId: 's1', odyHome: dir });
  for (let i = 0; i < 3; i += 1) {
    await cp.save({ currentMode: 'design', messageCount: i + 1, designModeContext: { sessions: [] }, toolCallIndex: { callIdToResult: {} } });
  }

  let attempts = 0;
  const fullKaos = {
    ...kaos,
    writeText: vi.fn(async (path: string, data: string, options?: { mode?: string }) => {
      attempts += 1;
      if (attempts === 1) {
        throw Object.assign(new Error('No space left'), { code: 'ENOSPC' });
      }
      return kaos.writeText(path, data, options);
    }),
  } as unknown as LocalKaos;

  const cp2 = new SessionCheckpoint({ kaos: fullKaos, sessionId: 's1', odyHome: dir });
  await cp2.save({ currentMode: 'design', messageCount: 99, designModeContext: { sessions: [] }, toolCallIndex: { callIdToResult: {} } });

  expect(attempts).toBe(2);
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/session-checkpoint.test.ts -t "retries"
```

Expected: tests fail because no retry logic.

- [ ] Modify `SessionCheckpoint.save`:

```ts
private async writeCheckpoint(text: string): Promise<void> {
  const maxRetries = 3;
  for (let attempt = 0; attempt < maxRetries; attempt += 1) {
    try {
      await atomicWrite(this.checkpointPath, text);
      return;
    } catch (error) {
      const code = (error as { code?: string }).code;
      if (code === 'ENOSPC') {
        await this.pruneOldestBackup();
      }
      if (attempt === maxRetries - 1) throw error;
      await new Promise((r) => setTimeout(r, 10 * 2 ** attempt));
    }
  }
}
```

In `save()`, replace the atomicWrite call with:

```ts
await withFileLock(this.checkpointPath, async () => {
  await atomicWrite(this.checkpointPath, text);
  await atomicWrite(backupPath, text);
});
```

Wait, the retry should be around the whole lock operation, not just atomicWrite. But the backup write also needs retry on ENOSPC. Put retry around the `withFileLock` block:

```ts
for (let attempt = 0; attempt < maxRetries; attempt += 1) {
  try {
    await withFileLock(this.checkpointPath, async () => {
      await atomicWrite(this.checkpointPath, text);
      await atomicWrite(backupPath, text);
    });
    break;
  } catch (error) {
    const code = (error as { code?: string }).code;
    if (code === 'ENOSPC') {
      await this.pruneOldestBackup();
    }
    if (attempt === maxRetries - 1) throw error;
    await new Promise((r) => setTimeout(r, 10 * 2 ** attempt));
  }
}
```

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/session-checkpoint.test.ts -t "retries" -t "ENOSPC"
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck 2>&1 | tail -5
```

- [ ] Commit:

```bash
git add packages/agent-core/src/checkpoint/session-checkpoint.ts \
        packages/agent-core/test/checkpoint/session-checkpoint.test.ts
git commit -m "feat: resilient checkpoint save with retry and disk-full prune"
```

---

## Task D3: Restore fallback chain (E2 + E7 + E8)

**Depends on:** D1, events.md C5

**Files:**
- Modify: `packages/agent-core/src/checkpoint/coordinator.ts` — `restore` walks versions + integrity
- Modify: `packages/agent-core/src/session/index.ts` — surface resume warning
- Test: `packages/agent-core/test/checkpoint/coordinator.test.ts`
- Test: `packages/agent-core/test/session/resume.test.ts`

### Steps

- [ ] Write failing tests:

```ts
it('returns the latest valid checkpoint when older versions are corrupt', async () => {
  const coordinator = new CheckpointCoordinator({
    kaos,
    sessionId: 's1',
    odyHome: dir,
    getCurrentMode: () => 'normal',
    getMessageCount: () => 1,
    getDesignModeContext: () => ({ sessions: [] }),
    getToolCallIndex: () => ({ callIdToResult: {} }),
    onError: vi.fn(),
  });

  await coordinator.saveCheckpoint();
  await coordinator.saveCheckpoint();
  // Corrupt the latest backup file
  const backupsDir = join(dir, 'session-state', 'backups');
  const files = (await readdir(backupsDir)).sort();
  await writeFile(join(backupsDir, files[files.length - 1]), 'not-json');

  const result = await coordinator.restoreWithFallback();
  expect(result.payload).not.toBeNull();
});

it('returns a warning when all checkpoints are corrupt', async () => {
  const coordinator = new CheckpointCoordinator({
    kaos,
    sessionId: 's1',
    odyHome: dir,
    getCurrentMode: () => 'normal',
    getMessageCount: () => 1,
    getDesignModeContext: () => ({ sessions: [] }),
    getToolCallIndex: () => ({ callIdToResult: {} }),
    onError: vi.fn(),
  });

  await coordinator.saveCheckpoint();
  const backupsDir = join(dir, 'session-state', 'backups');
  const files = await readdir(backupsDir);
  for (const file of files) {
    await writeFile(join(backupsDir, file), 'not-json');
  }

  const result = await coordinator.restoreWithFallback();
  expect(result.payload).toBeNull();
  expect(result.warning).toContain('checkpoint');
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/coordinator.test.ts -t "fallback"
```

Expected: `restoreWithFallback` not found.

- [ ] Modify `CheckpointCoordinator`:

```ts
export interface RestoreResult {
  readonly payload: SessionCheckpointPayload | null;
  readonly warning?: string;
}

export class CheckpointCoordinator {
  // ... existing

  async restoreWithFallback(): Promise<RestoreResult> {
    const versions = await this.index.listVersions();
    let gapWarning = false;
    let previousVersion: string | null = null;

    for (let i = versions.length - 1; i >= 0; i -= 1) {
      const version = versions[i];
      if (!version.valid) continue;
      if (previousVersion !== null && version.lastValidParent !== previousVersion) {
        gapWarning = true;
      }
      previousVersion = version.timestamp;

      const payload = await this.checkpoint.loadVersion(version.timestamp);
      if (payload === null) continue;

      const integrity = verifyCheckpointIntegrity({
        payload,
        memoryMessageCount: this.options.getMessageCount(),
      });
      if (integrity.valid) {
        return {
          payload,
          warning: gapWarning
            ? 'Checkpoint version chain has gaps; continuing with the latest valid checkpoint. Consider running a disk health check.'
            : undefined,
        };
      }
    }

    return {
      payload: null,
      warning: 'No valid checkpoint found. Session will resume from wire replay only.',
    };
  }
}
```

- [ ] Update `Session.resume()` to use `restoreWithFallback` and surface warning:

```ts
private async loadCheckpointIntoAgent(agent: Agent): Promise<string | undefined> {
  if (agent.type !== 'main' || this.checkpointCoordinator === undefined) return undefined;
  const result = await this.checkpointCoordinator.restoreWithFallback();
  if (result.warning !== undefined) {
    this.log.warn(result.warning);
  }
  if (result.payload !== null) {
    agent.sessionMode.restoreDesignSessions(result.payload.designModeContext.sessions);
  }
  return result.warning;
}
```

Capture the warning in `resume()` and include it in the return value.

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/coordinator.test.ts test/session/resume.test.ts
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck 2>&1 | tail -5
```

- [ ] Commit:

```bash
git add packages/agent-core/src/checkpoint/coordinator.ts \
        packages/agent-core/src/session/index.ts \
        packages/agent-core/test/checkpoint/coordinator.test.ts \
        packages/agent-core/test/session/resume.test.ts
git commit -m "feat: checkpoint restore with fallback chain and integrity verification"
```

---

## Task D4: Markdown export error isolation (E3)

**Depends on:** events.md C2

**Files:**
- Modify: `packages/agent-core/src/session/export/markdown-export.ts` — catch and log, expose error count
- Modify: `packages/agent-core/src/session/index.ts` — telemetry on error
- Test: `packages/agent-core/test/session/export/markdown-export.test.ts`

### Steps

- [ ] Write failing test:

```ts
it('logs errors but does not throw when append fails', async () => {
  const failingKaos = {
    ...kaos,
    mkdir: vi.fn().mockResolvedValue(undefined),
    writeText: vi.fn().mockRejectedValue(new Error('disk read-only')),
  } as unknown as LocalKaos;

  const exporter = new SessionMarkdownExport({
    kaos: failingKaos,
    sessionId: 's1',
    odyHome: dir,
    startedAt: new Date('2026-06-12T10:00:00Z'),
  });

  await exporter.append({
    type: 'context.append_message',
    message: { role: 'user', content: [{ type: 'text', text: 'Hello' }], toolCalls: [] } as ContextMessage,
  });

  expect(exporter.errorCount).toBe(1);
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/session/export/markdown-export.test.ts -t "logs errors"
```

Expected: errorCount not found or throws.

- [ ] Modify `SessionMarkdownExport`:

```ts
export class SessionMarkdownExport {
  private errorCount = 0;
  // ... existing fields

  get errorCount(): number {
    return this.errorCount;
  }

  async append(record: AgentRecord): Promise<void> {
    if (record.type !== 'context.append_message') return;
    const line = this.formatMessage(record.message);
    if (line === undefined) return;

    try {
      await this.kaos.mkdir(dirname(this.filePath), { parents: true, existOk: true });
      await withFileLock(this.filePath, async () => {
        await this.kaos.writeText(this.filePath, line, { mode: 'a' });
      });
    } catch (error) {
      this.errorCount += 1;
      // Re-throw so the caller (Session subscription) can log/telemetry.
      throw error;
    }
  }
}
```

The caller in `Session.setupCheckpointing` already catches and logs; with re-throw it now also gets telemetry.

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/session/export/markdown-export.test.ts
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck 2>&1 | tail -5
```

- [ ] Commit:

```bash
git add packages/agent-core/src/session/export/markdown-export.ts \
        packages/agent-core/test/session/export/markdown-export.test.ts
git commit -m "feat: isolate markdown export errors and expose error count"
```

---

## Task D5: Manual `/checkpoint` trigger tool

**Depends on:** events.md C4

**Files:**
- Create: `packages/agent-core/src/tools/builtin/state/checkpoint.ts`
- Create: `packages/agent-core/src/tools/builtin/state/checkpoint.md`
- Modify: `packages/agent-core/src/agent/tool/index.ts:407-421` — register tool
- Test: `packages/agent-core/test/tools/checkpoint.test.ts`

### Steps

- [ ] Write failing tests:

```ts
import { describe, expect, it, vi } from 'vitest';
import { CheckpointTool } from '../../src/tools/builtin/state/checkpoint';
import type { Agent } from '../../src/agent';

describe('CheckpointTool', () => {
  it('triggers coordinator save and returns success', async () => {
    const save = vi.fn().mockResolvedValue(undefined);
    const agent = {
      session: { checkpointCoordinator: { saveCheckpoint: save } },
    } as unknown as Agent;
    const tool = new CheckpointTool(agent);
    const result = await tool.resolveExecution({}).execute();

    expect(save).toHaveBeenCalled();
    expect(result.output).toContain('saved');
  });

  it('returns error when no coordinator is available', async () => {
    const agent = { session: {} } as unknown as Agent;
    const tool = new CheckpointTool(agent);
    const result = await tool.resolveExecution({}).execute();

    expect(result.isError).toBe(true);
  });
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/tools/checkpoint.test.ts
```

Expected: module not found.

- [ ] Create `packages/agent-core/src/tools/builtin/state/checkpoint.md`:

```md
Trigger a manual session checkpoint. Saves the current session state (mode, message count, design mode context, tool call index) to `.ody-code/session-state/` and updates the recovery index. Use this before a risky operation or when the user explicitly asks to freeze state.
```

- [ ] Create `packages/agent-core/src/tools/builtin/state/checkpoint.ts`:

```ts
import type { Agent } from '#/agent';
import { z } from 'zod';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './checkpoint.md';

export const CheckpointInputSchema = z.object({}).strict();
export type CheckpointInput = z.infer<typeof CheckpointInputSchema>;

export class CheckpointTool implements BuiltinTool<CheckpointInput> {
  readonly name = 'Checkpoint' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(CheckpointInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: CheckpointInput): ToolExecution {
    return {
      description: 'Saving a manual session checkpoint',
      approvalRule: this.name,
      execute: async () => {
        const coordinator = this.agent.session?.checkpointCoordinator;
        if (coordinator === undefined) {
          return { isError: true, output: 'Checkpoint coordinator is not available.' };
        }
        try {
          await coordinator.saveCheckpoint();
          return { output: 'Checkpoint saved successfully.' };
        } catch (error) {
          const message = error instanceof Error ? error.message : String(error);
          return { isError: true, output: `Checkpoint failed: ${message}` };
        }
      },
    };
  }
}
```

- [ ] Expose `checkpointCoordinator` on `Agent` via a getter or store it on `Agent.session`. Since `Agent` doesn't hold a session reference directly, store a back-link `agent.session = session` in `Session.createMain()` / `ensureResumeAgentInstantiated()`.

In `packages/agent-core/src/agent/index.ts` add:

```ts
session?: Session;
```

In `Session.setupCheckpointing` add `agent.session = this;`.

- [ ] Register tool in `packages/agent-core/src/agent/tool/index.ts`:

```ts
import { CheckpointTool } from '../tools/builtin/state/checkpoint';

// In built-in tool list:
new b.CheckpointTool(this.agent),
```

Add `CheckpointTool` to the `b` namespace import if grouped, or import directly.

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/tools/checkpoint.test.ts
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck 2>&1 | tail -5
```

- [ ] Commit:

```bash
git add packages/agent-core/src/tools/builtin/state/checkpoint.ts \
        packages/agent-core/src/tools/builtin/state/checkpoint.md \
        packages/agent-core/src/agent/tool/index.ts \
        packages/agent-core/src/agent/index.ts \
        packages/agent-core/src/session/index.ts \
        packages/agent-core/test/tools/checkpoint.test.ts
git commit -m "feat: add Checkpoint tool for manual session checkpoint trigger"
```

---

## Task D6: Recovery index rebuild verification (E4)

**Depends on:** D1

**Files:**
- Modify: `packages/agent-core/src/checkpoint/checkpoint-index.ts` — ensure rebuild uses backups
- Test: `packages/agent-core/test/checkpoint/checkpoint-index.test.ts`

### Steps

- [ ] Write failing test:

```ts
it('rebuilds index from versioned backup files', async () => {
  const cp = new SessionCheckpoint({ kaos, sessionId: 's1', odyHome: dir });
  await cp.save({ currentMode: 'design', messageCount: 1, designModeContext: { sessions: [] }, toolCallIndex: { callIdToResult: {} } });
  await cp.save({ currentMode: 'design', messageCount: 2, designModeContext: { sessions: [] }, toolCallIndex: { callIdToResult: {} } });

  await rm(join(dir, 'session-state', 'checkpoints.json'));
  const rebuilt = new CheckpointIndex({ kaos, sessionId: 's1', odyHome: dir });
  const versions = await rebuilt.listVersions();
  expect(versions).toHaveLength(2);
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/checkpoint-index.test.ts -t "rebuilds index from versioned backup files"
```

Expected: may already pass if D1 backup naming matches rebuild filter; if not, fix `rebuildFromBackups`.

- [ ] Update `CheckpointIndex.rebuildFromBackups` if needed to parse version timestamp from backup filename (`<sessionId>-<timestamp>.json`) and match `SessionCheckpoint` output.

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/checkpoint-index.test.ts
```

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck 2>&1 | tail -5
```

- [ ] Commit:

```bash
git add packages/agent-core/src/checkpoint/checkpoint-index.ts \
        packages/agent-core/test/checkpoint/checkpoint-index.test.ts
git commit -m "test: verify recovery index rebuilds from versioned backups"
```

---

## Self-Review (errors.md)

- [ ] 1. Spec-coverage table:

| Design doc error | Task(s) | Status |
|---|---|---|
| E1 JSON 写入失败 | D2 | covered |
| E2 Checkpoint 损坏 | D3 | covered |
| E3 Markdown 追加失败 | D4 | covered |
| E4 Recovery Index 丢失 | D6 | covered |
| E5 磁盘满 | D2 | covered |
| E6 并发写入冲突 | persistence.md B1 | already covered |
| E7 恢复后仍然错误 | D3 | covered |
| E8 版本链断裂 | D3 | covered |
| Manual /checkpoint | D5 | covered |

- [ ] 2. Placeholder scan: no TODO/TBD — every task has concrete code, paths, commands.
- [ ] 3. No phantom tasks: each task modifies files and has verifiable tests.
- [ ] 4. Dependency soundness: D2 → D1; D3 → D1, C5; D4 → C2; D5 → C4; D6 → D1. No forward references.
- [ ] 5. Caller & build soundness: `CheckpointIndex.update` signature changes to object form; update all callers (coordinator, B3 tests, D6 tests). `Agent` gains optional `session` back-link; `Session.setupCheckpointing` sets it. `CheckpointCoordinator.restore` becomes `restoreWithFallback`; update `Session.resume`. Every shared-signature task ends with `pnpm -r typecheck`.
- [ ] 6. Test-the-risk: D2 tests retry/ENOSPC; D3 tests corrupt fallback; D4 tests append failure isolation; D5 tests manual trigger; D6 tests index rebuild.
- [ ] 7. Type consistency: `SaveCheckpointResult` returned by `SessionCheckpoint.save` matches coordinator usage. `RestoreResult` used by `restoreWithFallback`. `CheckpointInputSchema` is empty strict object.
