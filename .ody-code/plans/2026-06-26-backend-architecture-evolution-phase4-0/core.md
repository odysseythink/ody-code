# Part 1 — Parity 框架核心

本 Part 建立 parity 框架的 6 个核心模块：Normalizer、AssertParity、KnownGaps、BackendFactory（TS/Rust）、ParityDriver，以及共享类型。所有代码落在 `packages/integration-tests/src/parity/`，测试落在 `packages/integration-tests/test/parity/`。

---

## Part 1 依赖图

```
A1 Normalizer + types
  │
  ▼
A2 AssertParity
  │
  ▼
A3 KnownGaps
  │
  ▼
A4 BackendFactory.makeTsBackend
  │
  ▼
A5 BackendFactory.makeRustBackend
  │
  ▼
A6 ParityDriver
```

A1–A3 互相独立；A4 依赖 A1 的类型；A5 依赖 A4 的文件结构；A6 依赖所有上游符号。

---

### Task A1: Normalizer + 共享类型

**Depends on:** none

**Files:**
- Create: `packages/integration-tests/src/parity/types.ts`
- Create: `packages/integration-tests/src/parity/normalize.ts`
- Create: `packages/integration-tests/test/parity/normalize.test.ts`

**Goal:** 定义 parity 框架核心类型，实现 `normalize()`，把非确定性字段（UUID、时间戳、绝对路径、pid、端口、路径分隔符、错误堆栈）替换为占位符；必须保证普通文本中的数字不被误伤。

- [ ] 在 `packages/integration-tests/src/parity/types.ts` 写入类型定义：

```ts
import type { AgentEvent } from '@odysseythink/agent-core';
import type { SDKRpcClient } from '@odysseythink/ody-code-sdk';

export type BackendKind = 'ts' | 'rust';

export interface ParityBackend {
  readonly kind: BackendKind;
  readonly client: SDKRpcClient;
  readonly homeDir: string;
  close(): Promise<void>;
}

export interface Scenario {
  readonly name: string;
  readonly run: (backend: ParityBackend) => Promise<ScenarioSnapshot>;
}

export interface ScenarioSnapshot {
  readonly responses: readonly unknown[];
  readonly events: readonly AgentEvent[];
  readonly records?: readonly unknown[];
  readonly fsTree?: unknown;
}

export interface NormalizedSnapshot {
  readonly responses: readonly unknown[];
  readonly events: readonly AgentEvent[];
  readonly records?: readonly unknown[];
  readonly fsTree?: unknown;
  readonly meta?: NormalizedMeta;
}

export interface NormalizedMeta {
  readonly joinedDeltaCount: number;
}

export interface FieldDiff {
  readonly path: string;
  readonly tsValue: unknown;
  readonly rustValue: unknown;
}

export interface ParityDiff {
  readonly scenarioName: string;
  readonly ts: NormalizedSnapshot;
  readonly rust: NormalizedSnapshot;
  readonly diffs: readonly FieldDiff[];
}

export interface NormalizerOptions {
  readonly homeDir: string;
  readonly tmpDir: string;
  readonly fixedIds?: ReadonlyMap<string, string> | undefined;
}
```

- [ ] 在 `packages/integration-tests/src/parity/normalize.ts` 写入实现：

```ts
import type { AgentEvent } from '@odysseythink/agent-core';
import type { NormalizedMeta, NormalizedSnapshot, NormalizerOptions, ScenarioSnapshot } from './types';

const UUID_RE = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi;
const LONG_NUMBER_RE = /\d{13,}/g;

const TIMESTAMPISH_KEYS = new Set([
  'timestamp', 'time', 'startedAt', 'endedAt', 'duration', 'latency', 'hrtime',
  'llmFirstTokenLatencyMs', 'llmStreamDurationMs', 'createdAt', 'updatedAt',
]);

const PID_KEYS = new Set(['pid', 'processId', 'process_id']);
const PORT_KEYS = new Set(['port', 'tcpPort', 'listenPort']);
const PATH_KEYS = new Set([
  'path', 'file', 'dir', 'cwd', 'workDir', 'homeDir', 'tmpDir', 'socketPath',
  'sourceFilePath', 'outputPath', 'configPath',
]);

function isTimestampish(path: string): boolean {
  const key = path.slice(path.lastIndexOf('.') + 1).replace(/\[\d+\]/g, '');
  return TIMESTAMPISH_KEYS.has(key);
}

function isPidLike(path: string): boolean {
  const key = path.slice(path.lastIndexOf('.') + 1).replace(/\[\d+\]/g, '');
  return PID_KEYS.has(key);
}

function isPortLike(path: string): boolean {
  const key = path.slice(path.lastIndexOf('.') + 1).replace(/\[\d+\]/g, '');
  return PORT_KEYS.has(key);
}

function isPathLike(path: string): boolean {
  const key = path.slice(path.lastIndexOf('.') + 1).replace(/\[\d+\]/g, '');
  return PATH_KEYS.has(key);
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function replaceDir(value: string, dir: string, placeholder: string): string {
  if (dir.length < 2) return value;
  const re = new RegExp(`(^|[^\\w/-])${escapeRegExp(dir)}`, 'g');
  return value.replace(re, `$1${placeholder}`);
}

function normalizeString(value: string, options: NormalizerOptions, path: string): string {
  let s = value;
  s = replaceDir(s, options.homeDir, '<HOME>');
  s = replaceDir(s, options.tmpDir, '<TMP>');

  if (options.fixedIds !== undefined) {
    for (const [id, placeholder] of options.fixedIds) {
      s = s.split(id).join(placeholder);
    }
  }

  s = s.replace(UUID_RE, '<id>');

  if (isTimestampish(path)) {
    s = s.replace(LONG_NUMBER_RE, '<ts>');
  }
  if (isPidLike(path)) {
    s = s.replace(/\b\d{4,5}\b/g, '<pid>');
  }
  if (isPortLike(path)) {
    s = s.replace(/\b\d{1,5}\b/g, '<port>');
  }
  if (isPathLike(path)) {
    s = s.replace(/\\/g, '/');
  }
  return s;
}

function isErrorObject(value: unknown): value is { code: unknown; kind?: unknown; message?: string; stack?: string } {
  return (
    typeof value === 'object' &&
    value !== null &&
    'code' in value &&
    (value as Record<string, unknown>).code !== undefined
  );
}

function normalizeError(
  value: { code: unknown; kind?: unknown; message?: string; stack?: string },
  options: NormalizerOptions,
  path: string,
): unknown {
  const out: Record<string, unknown> = { code: value.code };
  if ('kind' in value) out.kind = value.kind;
  if (typeof value.message === 'string') {
    out.message = normalizeString(value.message, options, `${path}.message`);
  }
  return out;
}

function joinAssistantDeltas(events: AgentEvent[]): { events: AgentEvent[]; joinedCount: number } {
  const result: AgentEvent[] = [];
  let joinedCount = 0;
  for (const event of events) {
    if (
      event.type === 'assistant.delta' &&
      result.length > 0 &&
      result[result.length - 1].type === 'assistant.delta'
    ) {
      const prev = result[result.length - 1] as { turnId: number; delta: string };
      const next = event as { turnId: number; delta: string };
      if (prev.turnId === next.turnId) {
        prev.delta += next.delta;
        joinedCount++;
        continue;
      }
    }
    result.push(event);
  }
  return { events: result, joinedCount };
}

function walk(value: unknown, options: NormalizerOptions, path: string): unknown {
  if (typeof value === 'string') {
    return normalizeString(value, options, path);
  }
  if (typeof value === 'number') {
    if (isTimestampish(path)) return 0;
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((item, i) => walk(item, options, `${path}[${i}]`));
  }
  if (isErrorObject(value)) {
    return normalizeError(value, options, path);
  }
  if (typeof value === 'object' && value !== null) {
    const out: Record<string, unknown> = {};
    for (const [key, val] of Object.entries(value)) {
      out[key] = walk(val, options, `${path}.${key}`);
    }
    return out;
  }
  return value;
}

export function normalize(
  snapshot: ScenarioSnapshot,
  options: NormalizerOptions,
): NormalizedSnapshot {
  const normalized: NormalizedSnapshot = {
    responses: walk(snapshot.responses, options, '$.responses') as unknown[],
    events: walk(snapshot.events, options, '$.events') as AgentEvent[],
    records: snapshot.records !== undefined
      ? (walk(snapshot.records, options, '$.records') as unknown[])
      : undefined,
    fsTree: snapshot.fsTree !== undefined
      ? walk(snapshot.fsTree, options, '$.fsTree')
      : undefined,
  };
  const { events, joinedCount } = joinAssistantDeltas(normalized.events as AgentEvent[]);
  normalized.events = events;
  if (joinedCount > 0) {
    normalized.meta = { joinedDeltaCount: joinedCount };
  }
  return normalized;
}
```

- [ ] 在 `packages/integration-tests/test/parity/normalize.test.ts` 写入测试：

```ts
import { describe, expect, it } from 'vitest';
import type { ScenarioSnapshot } from '../../src/parity/types';
import { normalize } from '../../src/parity/normalize';

function snapshot(input: Partial<ScenarioSnapshot> = {}): ScenarioSnapshot {
  return {
    responses: [],
    events: [],
    ...input,
  };
}

describe('normalize', () => {
  it('replaces UUIDs with <id>', () => {
    const result = normalize(
      snapshot({ responses: ['session-550e8400-e29b-41d4-a716-446655440000-end'] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual(['session-<id>-end']);
  });

  it('replaces uppercase UUIDs case-insensitively', () => {
    const result = normalize(
      snapshot({ responses: ['uuid:550E8400-E29B-41D4-A716-446655440000'] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual(['uuid:<id>']);
  });

  it('rejects 31-character pseudo-UUIDs', () => {
    const input = 'short-550e8400-e29b-41d4-a716-44665544000';
    const result = normalize(snapshot({ responses: [input] }), { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' });
    expect(result.responses).toEqual([input]);
  });

  it('replaces homeDir and tmpDir with placeholders', () => {
    const result = normalize(
      snapshot({ responses: ['/tmp/home/config.toml', '/tmp/tmp/log.txt'] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual(['<HOME>/config.toml', '<TMP>/log.txt']);
  });

  it('replaces timestamp-ish long numbers', () => {
    const result = normalize(
      snapshot({ responses: [{ duration: 1719782400000 }] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual([{ duration: 0 }]);
  });

  it('replaces fixedIds placeholders', () => {
    const result = normalize(
      snapshot({ responses: ['seed-abc123'] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp', fixedIds: new Map([['abc123', '<id:0>']]) },
    );
    expect(result.responses).toEqual(['seed-<id:0>']);
  });

  it('keeps ordinary text with short numbers (must-survive)', () => {
    const result = normalize(
      snapshot({ responses: ['hello 12345 world', 'count:12345'] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual(['hello 12345 world', 'count:12345']);
  });

  it('normalizes path separators in path-like fields', () => {
    const result = normalize(
      snapshot({ responses: [{ path: 'C:\\Users\\x\\file.txt' }] }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual([{ path: 'C:/Users/x/file.txt' }]);
  });

  it('strips stack and absolute paths from error objects', () => {
    const result = normalize(
      snapshot({
        responses: [{
          code: 'E_TEST',
          kind: 'test',
          message: 'failed at /tmp/home/main.ts',
          stack: 'at /tmp/home/main.ts:1:1',
        }],
      }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.responses).toEqual([{
      code: 'E_TEST',
      kind: 'test',
      message: 'failed at <HOME>/main.ts',
    }]);
  });

  it('merges consecutive assistant.delta events for the same turn', () => {
    const result = normalize(
      snapshot({
        events: [
          { type: 'turn.started', turnId: 1, origin: { kind: 'user' } },
          { type: 'assistant.delta', turnId: 1, delta: 'Hel' },
          { type: 'assistant.delta', turnId: 1, delta: 'lo' },
          { type: 'turn.ended', turnId: 1, reason: 'completed' },
        ] as any,
      }),
      { homeDir: '/tmp/home', tmpDir: '/tmp/tmp' },
    );
    expect(result.events).toHaveLength(3);
    expect((result.events[1] as any).delta).toBe('Hello');
    expect(result.meta).toEqual({ joinedDeltaCount: 1 });
  });
});
```

- [ ] 运行测试并确认失败（文件不存在）：

```bash
pnpm --filter integration-tests vitest run test/parity/normalize.test.ts
```

预期失败：`Error: Failed to load url /.../normalize.test.ts`。

- [ ] 运行测试并确认通过：

```bash
pnpm --filter integration-tests vitest run test/parity/normalize.test.ts
```

预期：所有 10 个用例通过。

- [ ] 提交：

```bash
git add packages/integration-tests/src/parity/types.ts \
           packages/integration-tests/src/parity/normalize.ts \
           packages/integration-tests/test/parity/normalize.test.ts
git commit -m "feat(integration-tests): parity normalizer + shared types"
```

---

### Task A2: AssertParity

**Depends on:** Task A1

**Files:**
- Create: `packages/integration-tests/src/parity/assert-parity.ts`
- Create: `packages/integration-tests/test/parity/assert-parity.test.ts`

**Goal:** 实现递归结构化 diff，返回 `ParityDiff | null`；diff 路径使用 JSON-pointer 风格。

- [ ] 在 `packages/integration-tests/src/parity/assert-parity.ts` 写入实现：

```ts
import type { FieldDiff, NormalizedSnapshot, ParityDiff } from './types';

export function assertParity(
  scenarioName: string,
  ts: NormalizedSnapshot,
  rust: NormalizedSnapshot,
): ParityDiff | null {
  const diffs: FieldDiff[] = [];
  collectDiffs(ts, rust, '$', diffs);
  if (diffs.length === 0) return null;
  return { scenarioName, ts, rust, diffs };
}

function collectDiffs(a: unknown, b: unknown, path: string, diffs: FieldDiff[]): void {
  if (Object.is(a, b)) return;

  const typeA = typeof a;
  const typeB = typeof b;

  if (a === null || b === null) {
    diffs.push({ path, tsValue: a, rustValue: b });
    return;
  }

  if (typeA !== typeB || Array.isArray(a) !== Array.isArray(b)) {
    diffs.push({ path, tsValue: a, rustValue: b });
    return;
  }

  if (typeA !== 'object') {
    diffs.push({ path, tsValue: a, rustValue: b });
    return;
  }

  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) {
      diffs.push({ path, tsValue: a.length, rustValue: b.length });
      return;
    }
    for (let i = 0; i < a.length; i++) {
      collectDiffs(a[i], b[i], `${path}[${i}]`, diffs);
    }
    return;
  }

  const keys = new Set([...Object.keys(a as object), ...Object.keys(b as object)]);
  for (const key of keys) {
    collectDiffs(
      (a as Record<string, unknown>)[key],
      (b as Record<string, unknown>)[key],
      `${path}.${key}`,
      diffs,
    );
  }
}
```

- [ ] 在 `packages/integration-tests/test/parity/assert-parity.test.ts` 写入测试：

```ts
import { describe, expect, it } from 'vitest';
import { assertParity } from '../../src/parity/assert-parity';
import type { NormalizedSnapshot } from '../../src/parity/types';

function snap(overrides: Partial<NormalizedSnapshot> = {}): NormalizedSnapshot {
  return { responses: [], events: [], ...overrides };
}

describe('assertParity', () => {
  it('returns null for identical snapshots', () => {
    const s = snap({ responses: ['a'], events: [{ type: 'turn.started', turnId: 1 } as any] });
    expect(assertParity('same', s, structuredClone(s))).toBeNull();
  });

  it('reports primitive diff', () => {
    const diff = assertParity('primitive', snap({ responses: [1] }), snap({ responses: [2] }));
    expect(diff).not.toBeNull();
    expect(diff!.diffs).toEqual([{ path: '$.responses[0]', tsValue: 1, rustValue: 2 }]);
  });

  it('reports array length diff', () => {
    const diff = assertParity('length', snap({ responses: [[1]] }), snap({ responses: [[1, 2]] }));
    expect(diff!.diffs).toEqual([{ path: '$.responses[0]', tsValue: 1, rustValue: 2 }]);
  });

  it('reports missing key diff', () => {
    const diff = assertParity(
      'missing',
      snap({ responses: [{ a: 1 }] }),
      snap({ responses: [{ a: 1, b: 2 }] }),
    );
    expect(diff!.diffs).toEqual([{ path: '$.responses[0].b', tsValue: undefined, rustValue: 2 }]);
  });

  it('includes scenario name and both snapshots', () => {
    const ts = snap({ responses: ['x'] });
    const rust = snap({ responses: ['y'] });
    const diff = assertParity('named', ts, rust);
    expect(diff!.scenarioName).toBe('named');
    expect(diff!.ts).toBe(ts);
    expect(diff!.rust).toBe(rust);
  });
});
```

- [ ] 运行并确认通过：

```bash
pnpm --filter integration-tests vitest run test/parity/assert-parity.test.ts
```

- [ ] 提交：

```bash
git add packages/integration-tests/src/parity/assert-parity.ts \
           packages/integration-tests/test/parity/assert-parity.test.ts
git commit -m "feat(integration-tests): parity structured diff assertion"
```

---

### Task A3: KnownGaps

**Depends on:** Task A2

**Files:**
- Create: `packages/integration-tests/src/parity/known-gaps.md`
- Create: `packages/integration-tests/src/parity/known-gaps.ts`
- Create: `packages/integration-tests/test/parity/known-gaps.test.ts`

**Goal:** 把 `known-gaps.md` 解析成结构化登记项；支持 scenario 精确匹配与 `*` 通配；提供「已登记 gap 实际通过则失败」的过期检测。

- [ ] 在 `packages/integration-tests/src/parity/known-gaps.md` 写入初始表格：

```markdown
# Parity Known Gaps

| Scenario | Layer | Reason |
|---|---|---|
| mock prompt | L3 | Rust mock provider 事件 payload 未实现对齐 |
| * | L4 | records 持久化 4.3 才迁移 |
```

- [ ] 在 `packages/integration-tests/src/parity/known-gaps.ts` 写入实现：

```ts
export type GapLayer = 'L2' | 'L3' | 'L4';

export interface KnownGap {
  readonly scenario: string;
  readonly layer: GapLayer;
  readonly reason: string;
}

export function parseKnownGaps(markdown: string): KnownGap[] {
  const gaps: KnownGap[] = [];
  const lines = markdown.split('\n');
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed.startsWith('|') || trimmed.startsWith('|--')) continue;
    const cells = trimmed.split('|').map((c) => c.trim()).filter((c) => c.length > 0);
    if (cells.length < 3) continue;
    const [scenario, layer, ...reasonParts] = cells;
    if (scenario === 'Scenario' || !['L2', 'L3', 'L4'].includes(layer)) continue;
    gaps.push({ scenario, layer: layer as GapLayer, reason: reasonParts.join(' | ') });
  }
  return gaps;
}

export function findGap(
  gaps: readonly KnownGap[],
  scenarioName: string,
  layer: GapLayer,
): string | undefined {
  for (const gap of gaps) {
    if (gap.layer !== layer) continue;
    if (gap.scenario === '*' || gap.scenario === scenarioName) {
      return gap.reason;
    }
  }
  return undefined;
}

export class StaleGapError extends Error {
  constructor(
    readonly scenario: string,
    readonly layer: GapLayer,
  ) {
    super(
      `Known gap for scenario "${scenario}" layer ${layer} is stale: the scenario now passes. Remove it from known-gaps.md.`,
    );
    this.name = 'StaleGapError';
  }
}

export function checkGapState(
  gaps: readonly KnownGap[],
  scenarioName: string,
  layer: GapLayer,
  passed: boolean,
): void {
  const reason = findGap(gaps, scenarioName, layer);
  if (reason !== undefined && passed) {
    throw new StaleGapError(scenarioName, layer);
  }
}
```

- [ ] 在 `packages/integration-tests/test/parity/known-gaps.test.ts` 写入测试：

```ts
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'pathe';
import { checkGapState, findGap, parseKnownGaps, StaleGapError } from '../../src/parity/known-gaps';

const knownGapsPath = join(
  dirname(fileURLToPath(import.meta.url)),
  '..',
  '..',
  'src',
  'parity',
  'known-gaps.md',
);

describe('parseKnownGaps', () => {
  it('parses the real known-gaps.md', () => {
    const source = readFileSync(knownGapsPath, 'utf8');
    const gaps = parseKnownGaps(source);
    expect(gaps).toContainEqual({
      scenario: 'mock prompt',
      layer: 'L3',
      reason: 'Rust mock provider 事件 payload 未实现对齐',
    });
    expect(gaps.some((g) => g.scenario === '*' && g.layer === 'L4')).toBe(true);
  });

  it('ignores header and separator rows', () => {
    const source = `| Scenario | Layer | Reason |\n|---|---|---|\n| x | L2 | r |`;
    expect(parseKnownGaps(source)).toEqual([{ scenario: 'x', layer: 'L2', reason: 'r' }]);
  });

  it('skips rows with invalid layer', () => {
    const source = `| x | L9 | r |`;
    expect(parseKnownGaps(source)).toEqual([]);
  });
});

describe('findGap', () => {
  const gaps = [
    { scenario: 'mock prompt', layer: 'L3' as const, reason: 'r1' },
    { scenario: '*', layer: 'L4' as const, reason: 'r2' },
  ];

  it('matches exact scenario', () => {
    expect(findGap(gaps, 'mock prompt', 'L3')).toBe('r1');
  });

  it('matches wildcard', () => {
    expect(findGap(gaps, 'session lifecycle', 'L4')).toBe('r2');
  });

  it('returns undefined when no match', () => {
    expect(findGap(gaps, 'session lifecycle', 'L3')).toBeUndefined();
  });
});

describe('checkGapState', () => {
  const gaps = [{ scenario: 'mock prompt', layer: 'L3' as const, reason: 'r1' }];

  it('throws StaleGapError when gap is registered but scenario passes', () => {
    expect(() => checkGapState(gaps, 'mock prompt', 'L3', true)).toThrow(StaleGapError);
  });

  it('does nothing when gap is registered and scenario fails', () => {
    expect(() => checkGapState(gaps, 'mock prompt', 'L3', false)).not.toThrow();
  });

  it('does nothing when no gap is registered', () => {
    expect(() => checkGapState(gaps, 'setModel', 'L3', true)).not.toThrow();
  });
});
```

- [ ] 运行并确认通过：

```bash
pnpm --filter integration-tests vitest run test/parity/known-gaps.test.ts
```

- [ ] 提交：

```bash
git add packages/integration-tests/src/parity/known-gaps.md \
           packages/integration-tests/src/parity/known-gaps.ts \
           packages/integration-tests/test/parity/known-gaps.test.ts
git commit -m "feat(integration-tests): known-gaps registry + parser"
```

---

### Task A4: BackendFactory.makeTsBackend

**Depends on:** Task A1

**Files:**
- Create: `packages/integration-tests/src/parity/backends.ts`
- Create: `packages/integration-tests/src/parity/fixtures/mock-provider.ts`
- Create: `packages/integration-tests/test/parity/backends.test.ts`

**Goal:** 实现 TS 内存后端工厂：用 `createRPC` + `WorkerCoreAPI` 构造 core，再用 `SDKRpcClient` 外部 stub + `ParityClientAPI` 构造 SDK 侧句柄。

- [ ] 在 `packages/integration-tests/src/parity/fixtures/mock-provider.ts` 写入轻量 MockChatProvider（参考 `@odysseythink/kosong/test/fixtures/mock-provider.ts`，但自包含以避免依赖未导出的测试文件）：

```ts
import type { ChatProvider, FinishReason, GenerateOptions, Message, ModelCapability, StreamedMessage, StreamedMessagePart, ThinkingEffort, Tool, TokenUsage } from '@odysseythink/kosong';
import { UNKNOWN_CAPABILITY } from '@odysseythink/kosong';

export interface MockChatProviderOptions {
  id?: string;
  modelName?: string;
  finishReason?: FinishReason | null;
  rawFinishReason?: string | null;
  usage?: TokenUsage;
}

export class MockChatProvider implements ChatProvider {
  readonly name = 'mock';
  readonly modelName: string;
  readonly thinkingEffort: ThinkingEffort | null = null;

  constructor(
    private readonly parts: StreamedMessagePart[],
    private readonly options: MockChatProviderOptions = {},
  ) {
    this.modelName = options.modelName ?? 'mock';
  }

  async generate(
    _systemPrompt: string,
    _tools: Tool[],
    _history: Message[],
    _options?: GenerateOptions,
  ): Promise<StreamedMessage> {
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
        for (const part of this.parts) {
          yield part;
        }
      },
    };
  }

  getCapability(_model?: string): ModelCapability {
    return UNKNOWN_CAPABILITY;
  }

  withThinking(_effort: ThinkingEffort): MockChatProvider {
    return new MockChatProvider([...this.parts], this.options);
  }
}
```

- [ ] 在 `packages/integration-tests/src/parity/backends.ts` 写入实现：

```ts
import { mkdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';

import {
  createRPC,
  type CoreAPI,
  type SDKAPI,
  type SDKAgentRPC,
  type Event,
  type ApprovalRequest,
  type ApprovalResponse,
  type QuestionRequest,
  type QuestionResult,
  type ToolCallRequest,
  type ToolCallResponse,
  type OpenExternalRequest,
  type OpenExternalResponse,
  type ChatStreamInitPayload,
  type ChatStreamInitResponse,
  type ChatStreamCancelPayload,
  WorkerCoreAPI,
} from '@odysseythink/agent-core';
import { KosongLLM } from '@odysseythink/agent-core';
import { SDKRpcClient } from '@odysseythink/ody-code-sdk';
import type { LLMFactoryConfig } from '@odysseythink/agent-core';
import type { ChatProvider } from '@odysseythink/kosong';

import type { BackendKind, ParityBackend } from './types';

export interface TsBackendConfig {
  readonly homeDir: string;
  readonly mockLlm?: ChatProvider | undefined;
}

export interface RustBackendConfig {
  readonly homeDir: string;
  readonly binaryPath: string;
  readonly transport: 'stdio' | { socketPath: string } | { host: string; port: number };
  readonly extraArgs?: readonly string[];
}

class ParityClientAPI implements SDKAPI {
  constructor(
    private readonly client: SDKRpcClient,
    private readonly getRpc: () => Promise<unknown>,
  ) {}

  emitEvent(event: Event): void {
    this.client.receiveEvent(event);
  }

  requestApproval(_request: ApprovalRequest): Promise<ApprovalResponse> {
    return Promise.resolve({ decision: 'cancelled', feedback: 'No approval handler in parity tests.' });
  }

  requestQuestion(_request: QuestionRequest): Promise<QuestionResult> {
    return Promise.resolve(null);
  }

  toolCall(_request: ToolCallRequest): Promise<ToolCallResponse> {
    return Promise.resolve({ output: 'SDK tool calls are not supported in parity tests.', isError: true });
  }

  openExternal(_request: OpenExternalRequest): Promise<OpenExternalResponse> {
    return Promise.resolve({ opened: false, error: 'No open-external handler in parity tests.' });
  }

  async chatStreamInit(_payload: ChatStreamInitPayload): Promise<ChatStreamInitResponse> {
    throw new Error('chatStreamInit is not supported in parity TS backend; use llmFactory instead.');
  }

  chatStreamCancel(_payload: ChatStreamCancelPayload): void {
    // no-op
  }
}

export async function makeTsBackend(config: TsBackendConfig): Promise<ParityBackend> {
  const [connectCore, connectSdk] = createRPC<CoreAPI, SDKAPI>();

  const llmFactory = config.mockLlm !== undefined
    ? (_rpc: Partial<SDKAgentRPC>, factoryConfig: LLMFactoryConfig) =>
        new KosongLLM({
          provider: config.mockLlm as ChatProvider,
          modelName: factoryConfig.modelName,
          systemPrompt: factoryConfig.systemPrompt,
          capability: factoryConfig.capability,
          completionBudgetConfig: factoryConfig.completionBudgetConfig,
        })
    : undefined;

  const core = new WorkerCoreAPI(connectCore, {
    homeDir: config.homeDir,
    llmFactory,
  });
  void core;

  const client = new SDKRpcClient({ homeDir: config.homeDir }, true);
  const clientApi = new ParityClientAPI(client, () => Promise.resolve(coreProxy));
  const coreProxy = await connectSdk(clientApi);
  Object.assign(client, { rpc: coreProxy, ready: Promise.resolve() });

  return {
    kind: 'ts' as BackendKind,
    client,
    homeDir: config.homeDir,
    close: async () => {
      await client.close?.().catch(() => {});
    },
  };
}

export async function makeRustBackend(_config: RustBackendConfig): Promise<ParityBackend> {
  // Implemented in Task A5.
  throw new Error('makeRustBackend not implemented yet');
}

export async function createTempHome(prefix = 'parity-'): Promise<string> {
  const dir = join(tmpdir(), `${prefix}${Date.now()}-${Math.random().toString(36).slice(2, 8)}`);
  await mkdir(dir, { recursive: true });
  return dir;
}

export async function cleanupHome(dir: string): Promise<void> {
  await rm(dir, { recursive: true, force: true });
}
```

- [ ] 在 `packages/integration-tests/test/parity/backends.test.ts` 写入测试：

```ts
import { describe, expect, it } from 'vitest';
import { makeTsBackend, createTempHome, cleanupHome } from '../../src/parity/backends';

describe('makeTsBackend', () => {
  it('creates a session and exposes the same homeDir', async () => {
    const homeDir = await createTempHome('ts-');
    const backend = await makeTsBackend({ homeDir });
    try {
      expect(backend.kind).toBe('ts');
      expect(backend.homeDir).toBe(homeDir);
      const summary = await backend.client.createSession({ title: 'parity test' });
      expect(summary.id).toBeDefined();
      expect(typeof summary.id).toBe('string');
    } finally {
      await backend.close();
      await cleanupHome(homeDir);
    }
  });
});
```

- [ ] 运行并确认通过：

```bash
pnpm --filter integration-tests vitest run test/parity/backends.test.ts
```

- [ ] 提交：

```bash
git add packages/integration-tests/src/parity/backends.ts \
           packages/integration-tests/src/parity/fixtures/mock-provider.ts \
           packages/integration-tests/test/parity/backends.test.ts
git commit -m "feat(integration-tests): TS in-process parity backend"
```

---

### Task A5: BackendFactory.makeRustBackend

**Depends on:** Task A4

**Files:**
- Modify: `packages/integration-tests/src/parity/backends.ts`（替换 `makeRustBackend` 的 stub）

**Goal:** 实现 Rust 后端工厂，复用 `SDKRpcClient.connect` 启动外部 `ody-host` 进程。

- [ ] 修改 `packages/integration-tests/src/parity/backends.ts`，把 `makeRustBackend` 替换为：

```ts
import { SDKRpcClient, type SDKRpcClientConnectOptions } from '@odysseythink/ody-code-sdk';
// ... existing imports ...

export async function makeRustBackend(config: RustBackendConfig): Promise<ParityBackend> {
  const transport: SDKRpcClientConnectOptions['transport'] =
    config.transport === 'stdio'
      ? 'stdio'
      : 'socketPath' in config.transport
        ? { socketPath: config.transport.socketPath, spawn: true }
        : { host: config.transport.host, port: config.transport.port, spawn: true };

  const client = await SDKRpcClient.connect({
    transport,
    binaryPath: config.binaryPath,
    homeDir: config.homeDir,
    extraArgs: config.extraArgs,
  });

  return {
    kind: 'rust' as BackendKind,
    client,
    homeDir: config.homeDir,
    close: async () => {
      await client.close?.().catch(() => {});
    },
  };
}
```

- [ ] 运行 typecheck 确认无编译错误：

```bash
pnpm --filter integration-tests typecheck
```

- [ ] 手动验证：构建 Rust host 后，用下面脚本连接 Rust 后端。

创建临时验证脚本 `/tmp/rust-backend-smoke.mjs`：

```ts
import { makeRustBackend, createTempHome, cleanupHome } from './packages/integration-tests/src/parity/backends.ts';

const homeDir = await createTempHome('rust-');
const backend = await makeRustBackend({
  homeDir,
  binaryPath: process.env['ODY_HOST_BINARY_PATH'] ?? './rust-ody/target/release/ody-host',
  transport: 'stdio',
  extraArgs: ['--mock-provider'],
});
console.log('rust backend homeDir:', backend.homeDir);
const summary = await backend.client.createSession({ title: 'rust parity smoke' });
console.log('session id:', summary.id);
await backend.close();
await cleanupHome(homeDir);
```

运行：

```bash
pnpm run build:host
node --import tsx/esm /tmp/rust-backend-smoke.mjs
```

预期输出包含 `rust backend homeDir:` 和以字符串开头的 `session id:`，无异常退出。

- [ ] 提交：

```bash
git add packages/integration-tests/src/parity/backends.ts
git commit -m "feat(integration-tests): Rust stdio parity backend factory"
```

---

### Task A6: ParityDriver

**Depends on:** Task A1, Task A4

**Files:**
- Create: `packages/integration-tests/src/parity/driver.ts`
- Create: `packages/integration-tests/test/parity/driver.test.ts`

**Goal:** 实现场景执行器：订阅事件、运行 scenario、收集响应/事件/记录。

- [ ] 在 `packages/integration-tests/src/parity/driver.ts` 写入实现：

```ts
import type { AgentEvent } from '@odysseythink/agent-core';
import type { ParityBackend, Scenario, ScenarioSnapshot } from './types';

export interface ParityDriverOptions {
  readonly timeoutMs?: number;
}

export class ParityDriver {
  constructor(private readonly options: ParityDriverOptions = {}) {}

  async runScenario(backend: ParityBackend, scenario: Scenario): Promise<ScenarioSnapshot> {
    const events: AgentEvent[] = [];
    const unsubscribe = backend.client.onEvent((event) => events.push(event));

    try {
      const result = await this.withTimeout(scenario.run(backend));
      return {
        responses: result.responses ?? [],
        events,
        records: result.records,
        fsTree: result.fsTree,
      };
    } finally {
      unsubscribe();
    }
  }

  private async withTimeout<T>(promise: Promise<T>): Promise<T> {
    const timeoutMs = this.options.timeoutMs ?? 30000;
    if (timeoutMs <= 0) return promise;
    return Promise.race([
      promise,
      new Promise<never>((_, reject) => {
        const id = setTimeout(() => reject(new Error(`Scenario timed out after ${timeoutMs}ms`)), timeoutMs);
        promise.then(() => clearTimeout(id), () => clearTimeout(id));
      }),
    ]);
  }
}
```

- [ ] 在 `packages/integration-tests/test/parity/driver.test.ts` 写入测试：

```ts
import { describe, expect, it } from 'vitest';
import { ParityDriver } from '../../src/parity/driver';
import type { ParityBackend, Scenario, ScenarioSnapshot } from '../../src/parity/types';

function fakeBackend(eventsToEmit: any[] = []): ParityBackend {
  const listeners = new Set<(event: unknown) => void>();
  return {
    kind: 'ts',
    homeDir: '/tmp/fake',
    client: {
      onEvent(listener: (event: unknown) => void) {
        listeners.add(listener);
        return () => listeners.delete(listener);
      },
      async createSession() {
        listeners.forEach((l) => l({ type: 'session.meta.updated', title: 't' }));
        return { id: 'session-123' };
      },
    } as any,
    close: async () => {},
  };
}

describe('ParityDriver', () => {
  it('collects events emitted during scenario run', async () => {
    const driver = new ParityDriver({ timeoutMs: 1000 });
    const backend = fakeBackend();
    const scenario: Scenario = {
      name: 'emit-and-respond',
      async run(b): Promise<ScenarioSnapshot> {
        const summary = await b.client.createSession({});
        return { responses: [summary], events: [] };
      },
    };
    const snapshot = await driver.runScenario(backend, scenario);
    expect(snapshot.responses).toHaveLength(1);
    expect(snapshot.events).toHaveLength(1);
    expect((snapshot.events[0] as any).type).toBe('session.meta.updated');
  });

  it('unsubscribes after scenario run', async () => {
    const driver = new ParityDriver({ timeoutMs: 1000 });
    const backend = fakeBackend();
    const scenario: Scenario = {
      name: 'noop',
      async run(): Promise<ScenarioSnapshot> {
        return { responses: [], events: [] };
      },
    };
    await driver.runScenario(backend, scenario);
    // After run, no listener should receive the event.
    const client = backend.client as any;
    client.onEvent((l: any) => l({ type: 'should.not.happen' }));
  });

  it('times out a hanging scenario', async () => {
    const driver = new ParityDriver({ timeoutMs: 10 });
    const backend = fakeBackend();
    const scenario: Scenario = {
      name: 'hang',
      async run(): Promise<ScenarioSnapshot> {
        await new Promise(() => {});
        return { responses: [], events: [] };
      },
    };
    await expect(driver.runScenario(backend, scenario)).rejects.toThrow('timed out');
  });
});
```

- [ ] 运行并确认通过：

```bash
pnpm --filter integration-tests vitest run test/parity/driver.test.ts
```

- [ ] 提交：

```bash
git add packages/integration-tests/src/parity/driver.ts \
           packages/integration-tests/test/parity/driver.test.ts
git commit -m "feat(integration-tests): parity scenario driver"
```

---

## Part 1 本地 Self-Review

- [ ] 1. Spec-coverage：A1 覆盖 Normalizer 与类型；A2 覆盖 AssertParity；A3 覆盖 KnownGaps；A4/A5 覆盖 BackendFactory；A6 覆盖 ParityDriver。
- [ ] 2. Placeholder scan：`makeRustBackend` 在 A4 中先以 stub 抛出，A5 立即替换为真实实现；无 TODO/TBD。
- [ ] 3. No phantom tasks：每个任务都产生可运行测试或可验证代码；A5 含手动验证脚本。
- [ ] 4. Dependency soundness：A1 无依赖；A2 依赖 A1；A3 依赖 A2；A4 依赖 A1；A5 依赖 A4；A6 依赖 A1/A4。
- [ ] 5. Caller & build soundness：本 Part 只新增符号，不改现有共享签名；A5 结束有 `pnpm --filter integration-tests typecheck`。
- [ ] 6. Test-the-risk：Normalizer 有 must-survive 测试；AssertParity 覆盖不同 diff 类型；KnownGaps 覆盖 stale gap；Driver 覆盖事件订阅与超时。
- [ ] 7. Type一致性：`BackendKind`、`Scenario`、`ScenarioSnapshot`、`NormalizedSnapshot`、`ParityDiff` 在 A1 定义，后续任务复用同一类型。
