# Phase B: Persistence Infrastructure

**Goal:** 建立 Session Checkpoint 持久化基础设施：文件锁保护、JSON checkpoint 数据模型、版本管理和历史回退，为事件驱动同步和恢复提供原子、可靠、可降级的存储层。

**Architecture:** 在 `packages/agent-core/src/utils/` 下新增 `file-lock.ts`，封装 Node.js `fs.open` + `flock`（macOS 上为全文件建议锁）。在 `packages/agent-core/src/checkpoint/` 下新增 `session-checkpoint.ts`（保存/加载 JSON checkpoint）、`checkpoint-index.ts`（版本索引和 10 版本轮换）。Checkpoint 只保存会话关键元数据（消息计数、设计模式上下文、工具调用索引），完整消息历史仍由现有 `wire.jsonl` 负责，避免体积爆炸。

---

## Task B1: File lock utilities

**Depends on:** none

**Files:**
- Create: `packages/agent-core/src/utils/file-lock.ts` — `withFileLock` wrapper
- Test: `packages/agent-core/test/utils/file-lock.test.ts` — lock exclusivity + timeout

### Steps

- [ ] Write failing tests:

```ts
import { describe, expect, it } from 'vitest';
import { mkdtemp, writeFile, rm, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { withFileLock } from '../../src/utils/file-lock';

describe('withFileLock', () => {
  it('serializes concurrent writers on the same file', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'file-lock-'));
    const path = join(dir, 'counter.txt');
    await writeFile(path, '0');

    const writers: Promise<void>[] = [];
    for (let i = 0; i < 5; i += 1) {
      writers.push(
        withFileLock(path, async () => {
          const current = Number(await readFile(path, 'utf8'));
          await writeFile(path, String(current + 1));
        }),
      );
    }
    await Promise.all(writers);

    expect(Number(await readFile(path, 'utf8'))).toBe(5);
    await rm(dir, { recursive: true, force: true });
  });

  it('rejects when lock cannot be acquired within timeout', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'file-lock-'));
    const path = join(dir, 'held.txt');
    await writeFile(path, '');

    const holder = withFileLock(path, async () => {
      await new Promise((r) => setTimeout(r, 500));
    });

    await expect(
      withFileLock(path, async () => {}, { timeoutMs: 50 }),
    ).rejects.toThrow('Failed to acquire lock');

    await holder;
    await rm(dir, { recursive: true, force: true });
  });
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/utils/file-lock.test.ts
```

Expected: "withFileLock is not a function" or module not found.

- [ ] Write minimal implementation:

```ts
import { open } from 'node:fs/promises';
import type { FileHandle } from 'node:fs/promises';

export interface FileLockOptions {
  /** Maximum time to wait for the lock before throwing. */
  timeoutMs?: number;
}

class LockTimeoutError extends Error {}

/**
 * Acquire an advisory exclusive lock on a file and execute `fn`.
 *
 * On POSIX (macOS/Linux) this uses flock(2). The lock is whole-file and
 * advisory. The lock is released when the returned FileHandle is closed,
 * which happens automatically when `fn` resolves or rejects.
 */
export async function withFileLock<T>(
  filePath: string,
  fn: () => Promise<T>,
  options: FileLockOptions = {},
): Promise<T> {
  const timeoutMs = options.timeoutMs ?? 30_000;
  const startedAt = Date.now();
  const handle = await open(filePath, 'a+');

  try {
    while (true) {
      try {
        await (handle as unknown as { writeLock: () => Promise<void> }).writeLock();
        break;
      } catch (error) {
        if (Date.now() - startedAt >= timeoutMs) {
          throw new LockTimeoutError(
            `Failed to acquire lock for ${filePath} within ${timeoutMs}ms`,
          );
        }
        await new Promise((r) => setTimeout(r, 10));
      }
    }
    return await fn();
  } finally {
    await handle.close();
  }
}
```

**Platform note:** `FileHandle.writeLock()` is available in Node 22+. For the repo's Node 24+ target, this should compile. If `pnpm -r typecheck` reports `writeLock` missing from `@types/node`, switch to the lower-level `fs-ext` binding or implement via `child_process` `flock` command. The implementation should be updated in that case within the same task.

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/utils/file-lock.test.ts
```

Expected: both tests pass.

- [ ] Commit:

```bash
git add packages/agent-core/src/utils/file-lock.ts \
        packages/agent-core/test/utils/file-lock.test.ts
git commit -m "feat: add flock-based file lock utility"
```

---

## Task B2: SessionCheckpoint data model + save/load

**Depends on:** Task B1

**Files:**
- Create: `packages/agent-core/src/checkpoint/session-checkpoint.ts` — `SessionCheckpoint` class
- Create: `packages/agent-core/src/checkpoint/types.ts` — checkpoint types
- Test: `packages/agent-core/test/checkpoint/session-checkpoint.test.ts`

### Steps

- [ ] Write failing tests:

```ts
import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { LocalKaos } from '@odysseythink/kaos';
import { SessionCheckpoint } from '../../src/checkpoint/session-checkpoint';

const OS_ENV = { workspaceDir: '/', additionalDirs: [] };

describe('SessionCheckpoint', () => {
  let dir: string;
  let kaos: LocalKaos;

  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), 'checkpoint-'));
    kaos = new LocalKaos(OS_ENV).withCwd(dir);
  });

  afterEach(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('saves a checkpoint to the expected path', async () => {
    const cp = new SessionCheckpoint({
      kaos,
      sessionId: 's1',
      odyHome: dir,
    });

    await cp.save({
      currentMode: 'design',
      messageCount: 42,
      designModeContext: { sessions: [] },
      toolCallIndex: { callIdToResult: {} },
    });

    const path = join(dir, 'session-state', 's1.json');
    const text = await kaos.readText(path);
    const parsed = JSON.parse(text);
    expect(parsed.sessionID).toBe('s1');
    expect(parsed.currentMode).toBe('design');
    expect(parsed.messageCount).toBe(42);
    expect(parsed.checkpointVersion).toBe(1);
  });

  it('loads the latest checkpoint', async () => {
    const cp = new SessionCheckpoint({ kaos, sessionId: 's1', odyHome: dir });
    await cp.save({
      currentMode: 'plan',
      messageCount: 10,
      designModeContext: { sessions: [] },
      toolCallIndex: { callIdToResult: {} },
    });

    const loaded = await cp.loadLatest();
    expect(loaded).not.toBeNull();
    expect(loaded?.currentMode).toBe('plan');
    expect(loaded?.messageCount).toBe(10);
  });

  it('returns null when no checkpoint exists', async () => {
    const cp = new SessionCheckpoint({ kaos, sessionId: 'missing', odyHome: dir });
    const loaded = await cp.loadLatest();
    expect(loaded).toBeNull();
  });
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/session-checkpoint.test.ts
```

Expected: module not found.

- [ ] Write implementation:

`packages/agent-core/src/checkpoint/types.ts`:

```ts
export interface DesignSessionCheckpoint {
  designSessionID: string;
  startedAtMsg: number;
  exitedAtMsg?: number | undefined;
  completeness?: Record<string, boolean> | undefined;
  approvedPath?: string | undefined;
}

export interface SessionCheckpointPayload {
  sessionID: string;
  createdAt: string;
  lastUpdatedAt: string;
  currentMode: 'normal' | 'plan' | 'design';
  messageCount: number;
  designModeContext: {
    sessions: DesignSessionCheckpoint[];
  };
  toolCallIndex: {
    callIdToResult: Record<string, unknown>;
  };
}

export interface SaveCheckpointInput {
  currentMode: SessionCheckpointPayload['currentMode'];
  messageCount: number;
  designModeContext: SessionCheckpointPayload['designModeContext'];
  toolCallIndex: SessionCheckpointPayload['toolCallIndex'];
}
```

`packages/agent-core/src/checkpoint/session-checkpoint.ts`:

```ts
import { join } from 'pathe';
import type { Kaos } from '@odysseythink/kaos';
import { atomicWrite } from '../utils/fs';
import { withFileLock } from '../utils/file-lock';
import type { SaveCheckpointInput, SessionCheckpointPayload } from './types';

export interface SessionCheckpointOptions {
  kaos: Kaos;
  sessionId: string;
  odyHome: string;
}

const CHECKPOINT_VERSION = 1;

export class SessionCheckpoint {
  private readonly kaos: Kaos;
  private readonly sessionId: string;
  private readonly odyHome: string;

  constructor(options: SessionCheckpointOptions) {
    this.kaos = options.kaos;
    this.sessionId = options.sessionId;
    this.odyHome = options.odyHome;
  }

  private get checkpointDir(): string {
    return join(this.odyHome, 'session-state');
  }

  private get checkpointPath(): string {
    return join(this.checkpointDir, `${this.sessionId}.json`);
  }

  private now(): string {
    return new Date().toISOString();
  }

  async save(input: SaveCheckpointInput): Promise<string> {
    await this.kaos.mkdir(this.checkpointDir, { parents: true, existOk: true });

    const payload: SessionCheckpointPayload = {
      sessionID: this.sessionId,
      createdAt: this.now(),
      lastUpdatedAt: this.now(),
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

    await withFileLock(this.checkpointPath, async () => {
      await atomicWrite(this.checkpointPath, text);
    });

    return this.checkpointPath;
  }

  async loadLatest(): Promise<SessionCheckpointPayload | null> {
    try {
      const text = await this.kaos.readText(this.checkpointPath);
      const parsed = JSON.parse(text) as SessionCheckpointPayload & { checkpointVersion?: number };
      if (parsed.checkpointVersion !== CHECKPOINT_VERSION) {
        return null;
      }
      return parsed;
    } catch (error) {
      const code = (error as { code?: string }).code;
      if (code === 'ENOENT') return null;
      throw error;
    }
  }
}
```

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/session-checkpoint.test.ts
```

Expected: all tests pass.

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck 2>&1 | tail -5
```

Expected: no type errors.

- [ ] Commit:

```bash
git add packages/agent-core/src/checkpoint/types.ts \
        packages/agent-core/src/checkpoint/session-checkpoint.ts \
        packages/agent-core/test/checkpoint/session-checkpoint.test.ts
git commit -m "feat: add SessionCheckpoint save/load with atomic write + file lock"
```

---

## Task B3: CheckpointIndex version management + rotation

**Depends on:** Task B2

**Files:**
- Create: `packages/agent-core/src/checkpoint/checkpoint-index.ts` — `CheckpointIndex` class
- Test: `packages/agent-core/test/checkpoint/checkpoint-index.test.ts`

### Steps

- [ ] Write failing tests:

```ts
import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { LocalKaos } from '@odysseythink/kaos';
import { CheckpointIndex } from '../../src/checkpoint/checkpoint-index';
import { SessionCheckpoint } from '../../src/checkpoint/session-checkpoint';

const OS_ENV = { workspaceDir: '/', additionalDirs: [] };

describe('CheckpointIndex', () => {
  let dir: string;
  let kaos: LocalKaos;
  let index: CheckpointIndex;

  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), 'checkpoint-index-'));
    kaos = new LocalKaos(OS_ENV).withCwd(dir);
    index = new CheckpointIndex({ kaos, sessionId: 's1', odyHome: dir });
  });

  afterEach(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('records a version entry after each save', async () => {
    const cp = new SessionCheckpoint({ kaos, sessionId: 's1', odyHome: dir });
    await cp.save({ currentMode: 'design', messageCount: 1, designModeContext: { sessions: [] }, toolCallIndex: { callIdToResult: {} } });
    await index.update(1, true);
    await cp.save({ currentMode: 'design', messageCount: 2, designModeContext: { sessions: [] }, toolCallIndex: { callIdToResult: {} } });
    await index.update(2, true);

    const versions = await index.listVersions();
    expect(versions).toHaveLength(2);
    expect(versions[0].messageCount).toBe(1);
    expect(versions[1].messageCount).toBe(2);
  });

  it('keeps at most 10 historical versions + latest', async () => {
    const cp = new SessionCheckpoint({ kaos, sessionId: 's1', odyHome: dir });
    for (let i = 0; i < 15; i += 1) {
      await cp.save({ currentMode: 'design', messageCount: i + 1, designModeContext: { sessions: [] }, toolCallIndex: { callIdToResult: {} } });
      await index.update(i + 1, true);
    }

    const backups = await readdir(join(dir, 'session-state', 'backups'));
    expect(backups.length).toBeLessThanOrEqual(10);
  });

  it('marks invalid versions and skips them when finding latest valid', async () => {
    const cp = new SessionCheckpoint({ kaos, sessionId: 's1', odyHome: dir });
    await cp.save({ currentMode: 'design', messageCount: 1, designModeContext: { sessions: [] }, toolCallIndex: { callIdToResult: {} } });
    await index.update(1, true);
    await cp.save({ currentMode: 'design', messageCount: 2, designModeContext: { sessions: [] }, toolCallIndex: { callIdToResult: {} } });
    await index.update(2, false);

    const latest = await index.findLatestValid();
    expect(latest?.messageCount).toBe(1);
  });

  it('rebuilds the index from backups when index file is missing', async () => {
    const cp = new SessionCheckpoint({ kaos, sessionId: 's1', odyHome: dir });
    await cp.save({ currentMode: 'design', messageCount: 1, designModeContext: { sessions: [] }, toolCallIndex: { callIdToResult: {} } });
    await index.update(1, true);

    await rm(join(dir, 'session-state', 'checkpoints.json'));
    const rebuilt = new CheckpointIndex({ kaos, sessionId: 's1', odyHome: dir });
    const versions = await rebuilt.listVersions();
    expect(versions).toHaveLength(1);
  });
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/checkpoint-index.test.ts
```

Expected: module not found.

- [ ] Write implementation:

`packages/agent-core/src/checkpoint/checkpoint-index.ts`:

```ts
import { join } from 'pathe';
import type { Kaos } from '@odysseythink/kaos';
import { atomicWrite } from '../utils/fs';
import { withFileLock } from '../utils/file-lock';

export interface CheckpointVersion {
  timestamp: string;
  messageCount: number;
  valid: boolean;
  lastValidParent: string | null;
}

export interface CheckpointIndexData {
  versions: CheckpointVersion[];
  latest: string | null;
}

export interface CheckpointIndexOptions {
  kaos: Kaos;
  sessionId: string;
  odyHome: string;
}

const MAX_BACKUPS = 10;
const INDEX_VERSION = 1;

export class CheckpointIndex {
  private readonly kaos: Kaos;
  private readonly sessionId: string;
  private readonly odyHome: string;

  constructor(options: CheckpointIndexOptions) {
    this.kaos = options.kaos;
    this.sessionId = options.sessionId;
    this.odyHome = options.odyHome;
  }

  private get indexDir(): string {
    return join(this.odyHome, 'session-state');
  }

  private get indexPath(): string {
    return join(this.indexDir, 'checkpoints.json');
  }

  private get backupsDir(): string {
    return join(this.indexDir, 'backups');
  }

  async update(messageCount: number, valid: boolean): Promise<void> {
    await this.kaos.mkdir(this.indexDir, { parents: true, existOk: true });
    await this.kaos.mkdir(this.backupsDir, { parents: true, existOk: true });

    const data = await this.readData();
    const now = new Date().toISOString();
    const versionNumber = data.versions.length + 1;
    const timestamp = `${now}-${String(versionNumber).padStart(4, '0')}`;

    const lastValidParent = this.findLastValidParent(data);
    const entry: CheckpointVersion = {
      timestamp,
      messageCount,
      valid,
      lastValidParent,
    };

    data.versions.push(entry);
    data.latest = timestamp;

    this.rotateVersions(data);
    await this.writeData(data);
  }

  async listVersions(): Promise<CheckpointVersion[]> {
    const data = await this.readData();
    return data.versions;
  }

  async findLatestValid(): Promise<CheckpointVersion | null> {
    const data = await this.readData();
    for (let i = data.versions.length - 1; i >= 0; i -= 1) {
      if (data.versions[i].valid) return data.versions[i];
    }
    return null;
  }

  private findLastValidParent(data: CheckpointIndexData): string | null {
    for (let i = data.versions.length - 1; i >= 0; i -= 1) {
      if (data.versions[i].valid) return data.versions[i].timestamp;
    }
    return null;
  }

  private rotateVersions(data: CheckpointIndexData): void {
    while (data.versions.length > MAX_BACKUPS) {
      data.versions.shift();
    }
  }

  private async readData(): Promise<CheckpointIndexData> {
    try {
      const text = await this.kaos.readText(this.indexPath);
      const parsed = JSON.parse(text) as CheckpointIndexData & { indexVersion?: number };
      if (parsed.indexVersion !== INDEX_VERSION) {
        return await this.rebuildFromBackups();
      }
      return parsed;
    } catch (error) {
      const code = (error as { code?: string }).code;
      if (code === 'ENOENT') {
        return await this.rebuildFromBackups();
      }
      throw error;
    }
  }

  private async rebuildFromBackups(): Promise<CheckpointIndexData> {
    const data: CheckpointIndexData = { versions: [], latest: null };
    try {
      const entries: string[] = [];
      for await (const entry of this.kaos.iterdir(this.backupsDir)) {
        entries.push(entry);
      }
      const sorted = entries
        .filter((name) => name.startsWith(`${this.sessionId}-v`) && name.endsWith('.json'))
        .sort();

      for (const name of sorted) {
        try {
          const text = await this.kaos.readText(join(this.backupsDir, name));
          const parsed = JSON.parse(text) as { messageCount?: number; checkpointVersion?: number };
          if (parsed.checkpointVersion === 1) {
            data.versions.push({
              timestamp: name.replace(/\.json$/, ''),
              messageCount: parsed.messageCount ?? 0,
              valid: true,
              lastValidParent: null,
            });
          }
        } catch {
          // Skip corrupted backup files during rebuild.
        }
      }

      if (data.versions.length > 0) {
        data.latest = data.versions[data.versions.length - 1].timestamp;
      }
    } catch {
      // backupsDir may not exist yet.
    }

    return data;
  }

  private async writeData(data: CheckpointIndexData): Promise<void> {
    const text = JSON.stringify({ ...data, indexVersion: INDEX_VERSION }, null, 2);
    await withFileLock(this.indexPath, async () => {
      await atomicWrite(this.indexPath, text);
    });
  }
}
```

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/checkpoint/checkpoint-index.test.ts
```

Expected: all tests pass.

- [ ] Run whole-tree typecheck:

```bash
pnpm -r typecheck 2>&1 | tail -5
```

Expected: no type errors.

- [ ] Commit:

```bash
git add packages/agent-core/src/checkpoint/checkpoint-index.ts \
        packages/agent-core/test/checkpoint/checkpoint-index.test.ts
git commit -m "feat: add CheckpointIndex with rotation, validity, and rebuild"
```

---

## Self-Review (persistence.md)

- [x] 1. Spec-coverage table: Part 1 (architecture/data layer) and Part 2 (JSON checkpoint, recovery index, backups) covered by B1-B3. I1-I5 integration tests covered by B2-B3 test cases.
- [x] 2. Placeholder scan: no TODO/TBD — all code is concrete.
- [x] 3. No phantom tasks: B1 creates lock utility; B2 creates checkpoint class; B3 creates index class.
- [x] 4. Dependency soundness: B2 → B1, B3 → B2. No forward refs.
- [x] 5. Caller & build soundness: New APIs introduced (`withFileLock`, `SessionCheckpoint`, `CheckpointIndex`) are not used by existing callers yet — they will be wired in Part 3. Each task ends with `pnpm -r typecheck`.
- [x] 6. Test-the-risk: B1 tests lock serialization and timeout (concurrency risk). B2 tests save/load roundtrip and missing-file behavior. B3 tests rotation limit, invalid-version fallback, and index-rebuild from backups.
- [x] 7. Type consistency: Types defined in `checkpoint/types.ts` are used by `SessionCheckpoint`. `CheckpointIndex` returns `CheckpointVersion[]` consistently.
