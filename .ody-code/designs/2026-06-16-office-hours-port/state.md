# Part 4 — 状态与持久化

## Scope

定义 office-hours 运行期间产生的跨项目状态（builder profile、session history、analytics、learnings）如何持久化到 `ODY_CODE_HOME`，以及如何在 TUI / agent 中读取和更新。

## Interfaces

```typescript
// packages/agent-core/src/office-hours/state.ts
export interface BuilderProfileEntry {
  readonly date: string;           // ISO 8601
  readonly mode: 'startup' | 'builder';
  readonly projectSlug: string;
  readonly signalCount: number;
  readonly signals: readonly string[];
  readonly designDoc: string;
  readonly assignment: string;
  readonly resourcesShown: readonly string[];
  readonly topics: readonly string[];
}

export interface OfficeHoursAnalyticsEvent {
  readonly ts: string;             // ISO 8601
  readonly skill: 'office-hours';
  readonly event: 'started' | 'completed' | 'resources_shown' | 'phase_changed';
  readonly branch: string;
  readonly session: string;
  readonly duration_s?: number;
  readonly outcome?: 'success' | 'abort' | 'unknown';
  readonly count?: number;
  readonly categories?: string;
}

export interface LearningEntry {
  readonly ts: string;
  readonly skill: 'office-hours';
  readonly type: 'operational' | 'eureka';
  readonly key: string;
  readonly insight: string;
  readonly confidence: number;
  readonly source: 'observed';
  readonly branch?: string;
}

export interface OfficeHoursStateStore {
  appendProfile(entry: BuilderProfileEntry): Promise<void>;
  readProfile(): Promise<readonly BuilderProfileEntry[]>;
  appendAnalytics(event: OfficeHoursAnalyticsEvent): Promise<void>;
  appendLearning(entry: LearningEntry): Promise<void>;
  searchLearnings(options: { limit: number; crossProject?: boolean }): Promise<readonly LearningEntry[]>;
  getSessionSummary(): Promise<{ sessionCount: number; tier: string; lastAssignment?: string }>;
}
```

## Data Flow

```
OfficeHoursMode active
  │
  ▼
Phase 4.5 / Phase 6
  │
  ▼
OfficeHoursStateStore.appendProfile(entry)
  │
  ▼
append line to ~/.ody-code/office-hours/builder-profile.jsonl
  │
  ▼
Phase 6 reads profile to determine tier / welcome-back message
```

```
Telemetry / lifecycle hooks
  │
  ▼
OfficeHoursStateStore.appendAnalytics(event)
  │
  ▼
~/.ody-code/office-hours/analytics.jsonl
```

```
Prompt asks to log learning or eureka
  │
  ▼
Agent calls OfficeHoursStateStore.appendLearning(entry)
  │
  ▼
~/.ody-code/office-hours/learnings.jsonl
```

## Algorithms

### Project Slug 推导

```
function deriveProjectSlug(cwd: string, fallback: string): string
  repoTop ← git rev-parse --show-toplevel (best-effort, ignore failure)
  if repoTop then
    return basename(repoTop)
  return fallback
```

### Tier 计算

```
function computeTier(profileEntries: BuilderProfileEntry[]): { tier: Tier; sessionCount: number }
  const sessionCount = profileEntries.length
  if sessionCount === 0        then return { tier: 'introduction', sessionCount }
  if sessionCount <= 3         then return { tier: 'welcome_back', sessionCount }
  if sessionCount <= 7         then return { tier: 'regular', sessionCount }
  return { tier: 'inner_circle', sessionCount }
```

### Resource 去重

```
function selectResources(profileEntries: BuilderProfileEntry[], candidates: Resource[]): Resource[]
  const shown = new Set(profileEntries.flatMap(e => e.resourcesShown))
  if shown.size >= 34 then return []      // upstream threshold [C:UPSTREAM]
  const available = candidates.filter(r => !shown.has(r.url))
  // Mix categories; never 3 of same type.
  return pickThreeMixed(available)
```

### Learnings 搜索

```
function searchLearnings(entries: LearningEntry[], keywords: string[], limit: number): LearningEntry[]
  scored = entries.map(e => ({ entry: e, score: keywordScore(e, keywords) }))
  scored.sort((a, b) => b.score - a.score)
  return scored.slice(0, limit).map(s => s.entry)
```

## Call-Site Integration

### 1. packages/agent-core/src/office-hours/state.ts [C:INFERRED]

新建模块，封装 `OfficeHoursStateStore`，使用 `Kaos` 进行文件追加：

```typescript
export class FileSystemOfficeHoursStateStore implements OfficeHoursStateStore {
  constructor(private readonly kaos: Kaos, private readonly homeDir: string) {}

  private profilePath(): string { return join(this.homeDir, 'office-hours', 'builder-profile.jsonl'); }
  private analyticsPath(): string { return join(this.homeDir, 'office-hours', 'analytics.jsonl'); }
  private learningsPath(): string { return join(this.homeDir, 'office-hours', 'learnings.jsonl'); }

  async appendProfile(entry: BuilderProfileEntry): Promise<void> {
    await this.kaos.mkdir(dirname(this.profilePath()), { parents: true, existOk: true });
    await this.kaos.writeText(this.profilePath(), JSON.stringify(entry) + '\n', { append: true });
  }

  // ... readProfile, appendAnalytics, appendLearning, searchLearnings
}
```

### 2. packages/agent-core/src/agent/index.ts [C:INFERRED]

`Agent` 构造函数新增可选依赖：

```typescript
export interface AgentOptions {
  // ... existing fields ...
  readonly officeHoursStateStore?: OfficeHoursStateStore;
}

export class Agent {
  readonly officeHoursStateStore: OfficeHoursStateStore;
  // ...
  constructor(options: AgentOptions) {
    // ...
    this.officeHoursStateStore = options.officeHoursStateStore ?? new NoopOfficeHoursStateStore();
  }
}
```

### 3. packages/agent-core/src/rpc/core-impl.ts [C:INFERRED]

`KimiCore` 创建 `Agent` 时传入 `officeHoursStateStore`（如果可用）。

### 4. packages/agent-core/src/agent/index.ts:Agent.enterPlan [C:INFERRED]

在 office-hours mode 进入时记录 analytics：

```typescript
enterPlan: async (payload) => {
  // ... existing logic ...
  if ((payload.kind ?? 'plan') === 'office-hours') {
    this.officeHoursStateStore.appendAnalytics({
      ts: new Date().toISOString(),
      skill: 'office-hours',
      event: 'started',
      branch: await currentBranch(),
      session: this.sessionMode.sessionModeId ?? 'unknown',
    });
  }
}
```

### 5. Prompt contract 中引用 state [C:UPSTREAM]

`office-hours-contract.ts` 的 Phase 1 和 Phase 6 包含读取 profile 和写入 profile 的指令；实现时使用 `Bash` 调用一个轻量 CLI 或直接使用 agent state store。

为简化并避免在 prompt 中暴露 Kaos，设计两个专用 tools：

```typescript
// AppendBuilderProfileTool
// AppendLearningTool
// SearchLearningsTool
```

这些 tools 仅当 `sessionMode.kind === 'office-hours'` 时注册到 `ToolManager`。

## Error & Degradation

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| Profile file not writable | catch and warn | Phase 6 falls back to introduction tier | 修复 `ODY_CODE_HOME` 权限 |
| JSON parse error in profile | skip corrupted lines, load rest | 可能低估 sessionCount | 手动修复 jsonl |
| Learning append fails | catch and warn | 不阻塞流程 | 磁盘可写后下次成功 |
| Git branch unavailable | use 'unknown' branch | analytics 缺少分支信息 | 在 git repo 内运行 |

## Test Plan

1. **BuilderProfile round-trip**（`packages/agent-core/test/office-hours/state.test.ts` 新增）：
   - `await store.appendProfile(entry)`
   - `const entries = await store.readProfile()`
   - `expect(entries).toHaveLength(1)`
   - `expect(entries[0].projectSlug).toBe('my-project')`

2. **Tier 计算**：
   - 0 entries → `introduction`
   - 3 entries → `welcome_back`
   - 8 entries → `inner_circle`

3. **Resource 去重**：
   - 给定 34+ 已展示 URLs，返回空数组。
   - 已展示 URL 不出现在结果中。

4. **Noop store**：
   - 未提供 store 时不抛错。

## Done Criteria

- `pnpm -F @odysseythink/agent-core typecheck` passes.
- `pnpm -F @odysseythink/agent-core test` passes.
- Profile/analytics/learnings 文件在运行后正确追加到 `~/.ody-code/office-hours/`。
