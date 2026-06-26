# Part 4 — Workflow Contract, State Store 与 Integration Tools

**Phase:** D — 依赖 Phase B + C（CLI/TUI wiring 和 session engine 就绪后，本 phase 所有 tasks 可并行）。

## Task 8: office-hours-contract.ts — Phase 1-6 工作流 Prompt Fragments

**Depends on:** Task 1（类型扩展，SessionModeKind 包含 `'office-hours'`）

**Files:**
- Modify: `packages/agent-core/src/agent/injection/office-hours-contract.ts`（替换 Task 6 中的桩实现）
- Test: `packages/agent-core/test/agent/injection/office-hours-contract.test.ts`（新建）

### Steps

- [ ] 创建测试文件 `packages/agent-core/test/agent/injection/office-hours-contract.test.ts`：
  ```typescript
  import { describe, expect, it } from 'vitest';
  import {
    officeHoursEntryMessage,
    officeHoursFullReminder,
    officeHoursSparseReminder,
    officeHoursReentryReminder,
    officeHoursExitReminder,
  } from '#/agent/injection/office-hours-contract';

  describe('office-hours-contract', () => {
    const path = '/project/.ody-code/office-hours/2026-06-16-my-startup.md';

    describe('officeHoursEntryMessage', () => {
      it('includes the design file path', () => {
        const msg = officeHoursEntryMessage(path);
        expect(msg).toContain(path);
      });

      it('includes office hours activation notice', () => {
        const msg = officeHoursEntryMessage(path);
        expect(msg).toContain('Office hours');
      });

      it('forbids writing code', () => {
        const msg = officeHoursEntryMessage(path);
        expect(msg).toContain('Do NOT write code');
      });
    });

    describe('officeHoursFullReminder', () => {
      it('includes all phases', () => {
        const msg = officeHoursFullReminder(path);
        expect(msg).toContain('Phase 1');
        expect(msg).toContain('Phase 2');
        expect(msg).toContain('Phase 3');
        expect(msg).toContain('Phase 4');
        expect(msg).toContain('Phase 5');
        expect(msg).toContain('Phase 6');
      });

      it('includes AskUserQuestion discipline', () => {
        const msg = officeHoursFullReminder(path);
        expect(msg).toContain('AskUserQuestion');
      });

      it('includes design doc template section', () => {
        const msg = officeHoursFullReminder(path);
        expect(msg).toContain('Design Doc');
      });
    });

    describe('officeHoursSparseReminder', () => {
      it('is shorter than full reminder', () => {
        const sparse = officeHoursSparseReminder(path);
        const full = officeHoursFullReminder(path);
        expect(sparse.length).toBeLessThan(full.length);
      });

      it('includes ONE question at a time', () => {
        expect(officeHoursSparseReminder(path)).toContain('ONE question');
      });
    });

    describe('officeHoursExitReminder', () => {
      it('signals session completion', () => {
        expect(officeHoursExitReminder(path)).toContain('complete');
      });
    });

    describe('officeHoursReentryReminder', () => {
      it('acknowledges existing content', () => {
        expect(officeHoursReentryReminder(path)).toContain('existing');
      });
    });
  });
  ```

- [ ] 运行测试验证 FAIL（桩实现不满足断言）：
  ```bash
  pnpm -F @odysseythink/agent-core test -- test/agent/injection/office-hours-contract
  ```
  **预期：** 测试失败 — 桩实现不包含 "Phase 1"、"AskUserQuestion" 等。

- [ ] 将 `packages/agent-core/src/agent/injection/office-hours-contract.ts` 的桩实现替换为完整的 YC Office Hours Phase 1-6 prompt。完整内容如下：

  ```typescript
  import type { SessionModeFilePath } from '../session-mode';

  // ── Entry message (tool output when EnterOfficeHoursMode fires) ──────────

  export function officeHoursEntryMessage(designFilePath: SessionModeFilePath): string {
    const path = designFilePath ?? '(not yet assigned)';
    return [
      `Office hours is now active. Your job is to act as a YC office hours partner —`,
      `a sharp, experienced builder who asks hard questions and pushes for clarity.`,
      ``,
      `## HARD GATES`,
      `- Do NOT write code. Your ONLY output is a design document.`,
      `- Ask ONE question at a time via AskUserQuestion.`,
      `- Design file (write ONLY to this path): ${path}`,
      ``,
      `Follow the workflow phases below. Begin with Phase 1: Context Gathering.`,
    ].join('\n');
  }

  // ── Full reminder (injected at turn start, and every 5+ assistant turns) ──

  export function officeHoursFullReminder(designFilePath: SessionModeFilePath): string {
    const path = designFilePath ?? '(not yet assigned)';
    return [
      `## Office Hours — Full Workflow`,
      ``,
      `### HARD GATES`,
      `- Do NOT write code. Produce only a design document.`,
      `- Write the design doc to EXACTLY: ${path}`,
      `- Ask ONE question at a time via AskUserQuestion. End every turn with AskUserQuestion or ExitOfficeHoursMode.`,
      `- Voice: builder-to-builder. Concrete. No AI buzzwords.`,
      ``,
      `### Phase 1: Context Gathering`,
      `1. Read CLAUDE.md if it exists in the project root.`,
      `2. Read any TODOS.md, README.md, or other project docs.`,
      `3. Check git log for recent activity (last 20 commits).`,
      `4. Map the codebase: what does this project do? What's the stack?`,
      `5. Determine mode: startup (building a company, has customers/revenue/go-to-market) or builder (hackathon, open source, side project, learning, having fun).`,
      ``,
      `### Phase 2A: Startup Diagnostic`,
      `If startup mode — ask startup questions. Select 2-4 based on product stage:`,
      `- Pre-product: "Who exactly are you building this for? What's the wedge?"`,
      `  "What's the fastest path to something someone can use?"`,
      `  "What assumptions are you making that could be wrong?"`,
      `- Has users: "What have you learned from your users that surprised you?"`,
      `  "Where's the demand coming from? What's your best signal?"`,
      `  "What would make your best users genuinely upset if you removed it?"`,
      `- Has paying customers: "What's your revenue? What's growing fastest?"`,
      `  "If you had to 10x revenue this quarter, what's the one lever?"`,
      `  "What's the biggest threat to your business right now?"`,
      `- Engineering-heavy: "What's the hardest technical problem you're solving?"`,
      `  "Is the technical risk the real bottleneck, or is it distribution?"`,
      ``,
      `### Phase 2B: Builder Diagnostic`,
      `If builder mode — ask builder questions:`,
      `1. "What's the coolest version of this? What would make it genuinely delightful?"`,
      `2. "Who would you show this to? What would make them say 'whoa'?"`,
      `3. "What's the fastest path to something you can actually use or share?"`,
      `4. "What existing thing is closest to this, and how is yours different?"`,
      `5. "What would you add if you had unlimited time? What's the 10x version?"`,
      `Ask at most 3-4 questions from this list.`,
      ``,
      `### Phase 2.5: Related Design Discovery`,
      `Search .ody-code/designs/ and .ody-code/office-hours/ for related design documents. If a relevant prior design exists, mention it and ask whether to build on it.`,
      ``,
      `### Phase 2.75: Landscape Awareness`,
      `If the problem space is novel or competitive, offer to search the web for context (WebSearch). Honor the user's privacy preference — skip if they decline.`,
      ``,
      `### Phase 3: Premise Challenge`,
      `List the premises you've identified. Ask the user: "Here are the premises I see. Which ones feel shaky? Which are you most confident about?" Push back gently on unquestioned assumptions.`,
      ``,
      `### Phase 4: Alternatives Generation`,
      `Generate 2-3 genuinely different approaches. For each:`,
      `- What it looks like concretely`,
      `- What has to be true for it to work`,
      `- Biggest risk`,
      `Present them via AskUserQuestion and let the user pick.`,
      ``,
      `### Phase 4.5: Founder Signal Synthesis`,
      `Count founder signals from the conversation:`,
      `- named_users: mentions specific users or customers`,
      `- demand_evidence: revenue, waitlist, usage, inbound interest`,
      `- pushback: pushed back on your premises or questions`,
      `- others_need: solving a problem they personally observed in others`,
      `- domain_expertise: shows deep understanding of the space`,
      `- taste: cares about details, design, UX`,
      `- agency: already building, shipped something, made progress`,
      `- reasoned_defense: defended premises with reasoning, not emotion`,
      `After counting, call AppendBuilderProfile to persist.`,
      ``,
      `### Phase 5: Design Doc`,
      `Write the design document to ${path}. Use the appropriate template:`,
      ``,
      `**Startup template sections:** Problem Statement, Demand Evidence, Status Quo, Target User & Wedge, Constraints, Premises, Approaches, Recommended Approach, Open Questions, Success Criteria, Distribution Plan, Dependencies, The Assignment, What I Noticed.`,
      ``,
      `**Builder template sections:** Problem Statement, What Makes This Cool, Constraints, Premises, Approaches, Recommended Approach, Open Questions, Success Criteria, Distribution Plan, Next Steps, What I Noticed.`,
      ``,
      `Tag decisions: [C:USER] for user-confirmed, [C:INFERRED] for inferred. Include an ## Assumptions section.`,
      ``,
      `### Phase 6: Handoff`,
      `After the design doc is approved:`,
      `1. Determine tier from builder profile (introduction / welcome_back / regular / inner_circle).`,
      `2. Select 2-3 resources not shown before (call SearchLearnings if relevant).`,
      `3. Recommend next steps or follow-up skills.`,
      `4. Call ExitOfficeHoursMode to end the session.`,
      ``,
      `### Turn Discipline`,
      `- EVERY turn ends with AskUserQuestion or ExitOfficeHoursMode.`,
      `- Never combine multiple questions in one turn.`,
      `- If the user seems impatient, acknowledge it, ask 1-2 more critical questions, then move to Phase 5.`,
    ].join('\n');
  }

  // ── Sparse reminder (injected after 2-4 assistant turns) ──────────────────

  export function officeHoursSparseReminder(designFilePath: SessionModeFilePath): string {
    return [
      `Office hours continues. Remember:`,
      `- ONE question at a time via AskUserQuestion.`,
      `- Current phase: follow the workflow.`,
      `- Design doc target: ${designFilePath ?? '(not yet assigned)'}`,
      `- End when ready: ExitOfficeHoursMode.`,
    ].join('\n');
  }

  // ── Reentry reminder (design file already has content from prior session) ──

  export function officeHoursReentryReminder(designFilePath: SessionModeFilePath): string {
    return [
      `Office hours resumed. The design document at ${designFilePath ?? '(unknown)'} already has content.`,
      `Read the existing content, pick up where you left off, and continue the workflow.`,
      `If the document looks complete, move to Phase 6: Handoff.`,
    ].join('\n');
  }

  // ── Exit reminder (mode ended, injected once on exit) ─────────────────────

  export function officeHoursExitReminder(designFilePath: SessionModeFilePath | null): string {
    return designFilePath
      ? `Office hours session complete. Design document saved to: ${designFilePath}. The application will now exit.`
      : `Office hours session ended — no design document was produced.`;
  }
  ```

- [ ] 运行测试验证 PASS：
  ```bash
  pnpm -F @odysseythink/agent-core test -- test/agent/injection/office-hours-contract
  ```
  **预期：** 所有测试通过。

- [ ] 运行 `pnpm -F @odysseythink/agent-core typecheck` 确认编译通过。

- [ ] Commit: `feat: add full YC Office Hours Phase 1-6 workflow prompt contract`

---

## Task 9: OfficeHoursStateStore（Builder Profile + Analytics + Learnings 持久化）

**Depends on:** Task 1

**Files:**
- Create: `packages/agent-core/src/office-hours/state.ts`
- Modify: `packages/agent-core/src/agent/index.ts:80-101,103-120,159-200`（AgentOptions + Agent 构造函数）
- Test: `packages/agent-core/test/office-hours/state.test.ts`（新建）

### Steps

- [ ] 创建测试文件 `packages/agent-core/test/office-hours/state.test.ts`：
  ```typescript
  import { describe, expect, it, vi } from 'vitest';
  import { join } from 'pathe';
  import {
    FileSystemOfficeHoursStateStore,
    NoopOfficeHoursStateStore,
    computeTier,
    selectResources,
  } from '#/office-hours/state';
  import type { BuilderProfileEntry } from '#/office-hours/state';

  function mockKaos() {
    const files: Record<string, string> = {};
    return {
      mkdir: vi.fn().mockResolvedValue(undefined),
      readText: vi.fn(async (p: string) => {
        if (p in files) return files[p];
        throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
      }),
      writeText: vi.fn(async (p: string, data: string, opts?: { append?: boolean }) => {
        if (opts?.append) {
          files[p] = (files[p] ?? '') + data;
        } else {
          files[p] = data;
        }
      }),
    };
  }

  describe('FileSystemOfficeHoursStateStore', () => {
    it('appendProfile and readProfile round-trip', async () => {
      const kaos = mockKaos();
      const store = new FileSystemOfficeHoursStateStore(kaos, '/fake/home/.ody-code');
      const entry: BuilderProfileEntry = {
        date: '2026-06-16T12:00:00.000Z',
        mode: 'startup',
        projectSlug: 'my-project',
        signalCount: 3,
        signals: ['named_users', 'demand_evidence', 'agency'],
        designDoc: '/proj/.ody-code/office-hours/2026-06-16-test.md',
        assignment: 'Build the MVP',
        resourcesShown: ['https://example.com/resource1'],
        topics: ['saas', 'b2b'],
      };
      await store.appendProfile(entry);
      const entries = await store.readProfile();
      expect(entries).toHaveLength(1);
      expect(entries[0].projectSlug).toBe('my-project');
      expect(entries[0].signalCount).toBe(3);
    });

    it('appendAnalytics writes to analytics.jsonl', async () => {
      const kaos = mockKaos();
      const store = new FileSystemOfficeHoursStateStore(kaos, '/fake/home/.ody-code');
      await store.appendAnalytics({
        ts: '2026-06-16T12:00:00.000Z',
        skill: 'office-hours',
        event: 'started',
        branch: 'main',
        session: 's1',
      });
      const writeCalls = kaos.writeText.mock.calls.filter(
        (c: unknown[]) => typeof c[0] === 'string' && c[0].includes('analytics'),
      );
      expect(writeCalls.length).toBeGreaterThan(0);
    });

    it('appendLearning and searchLearnings', async () => {
      const kaos = mockKaos();
      const store = new FileSystemOfficeHoursStateStore(kaos, '/fake/home/.ody-code');
      await store.appendLearning({
        ts: '2026-06-16T12:00:00.000Z',
        skill: 'office-hours',
        type: 'eureka',
        key: 'pricing-model',
        insight: 'Freemium works better than trial for this segment',
        confidence: 0.8,
        source: 'observed',
      });
      const results = await store.searchLearnings({ limit: 5 });
      expect(results).toHaveLength(1);
      expect(results[0].key).toBe('pricing-model');
    });
  });

  describe('computeTier', () => {
    it('0 entries → introduction', () => {
      expect(computeTier([]).tier).toBe('introduction');
    });
    it('3 entries → welcome_back', () => {
      expect(computeTier([{}, {}, {}] as BuilderProfileEntry[]).tier).toBe('welcome_back');
    });
    it('8 entries → inner_circle', () => {
      expect(computeTier(Array(8).fill({}) as BuilderProfileEntry[]).tier).toBe('inner_circle');
    });
  });

  describe('selectResources', () => {
    const candidates = [
      { url: 'https://a.com', category: 'essay' },
      { url: 'https://b.com', category: 'video' },
      { url: 'https://c.com', category: 'essay' },
      { url: 'https://d.com', category: 'book' },
    ];
    it('filters already-shown resources', () => {
      const shown = [{ resourcesShown: ['https://a.com'] }] as BuilderProfileEntry[];
      const result = selectResources(shown, candidates);
      expect(result.find(r => r.url === 'https://a.com')).toBeUndefined();
    });
    it('returns empty when 34+ already shown', () => {
      const shown = [{ resourcesShown: Array(34).fill('https://x.com') }] as BuilderProfileEntry[];
      expect(selectResources(shown, candidates)).toHaveLength(0);
    });
  });

  describe('NoopOfficeHoursStateStore', () => {
    it('does not throw on any method', async () => {
      const store = new NoopOfficeHoursStateStore();
      await expect(store.appendProfile({} as BuilderProfileEntry)).resolves.toBeUndefined();
      await expect(store.readProfile()).resolves.toEqual([]);
    });
  });
  ```

- [ ] 运行测试验证 FAIL（文件不存在）：
  ```bash
  pnpm -F @odysseythink/agent-core test -- test/office-hours/state
  ```
  **预期：** 模块未找到错误。

- [ ] 创建 `packages/agent-core/src/office-hours/state.ts`：
  ```typescript
  import { join } from 'pathe';
  import type { Kaos } from '@odysseythink/kaos';

  export interface BuilderProfileEntry {
    readonly date: string;
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
    readonly ts: string;
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

  export type Tier = 'introduction' | 'welcome_back' | 'regular' | 'inner_circle';

  export interface Resource {
    readonly url: string;
    readonly category: string;
  }

  export interface OfficeHoursStateStore {
    appendProfile(entry: BuilderProfileEntry): Promise<void>;
    readProfile(): Promise<readonly BuilderProfileEntry[]>;
    appendAnalytics(event: OfficeHoursAnalyticsEvent): Promise<void>;
    appendLearning(entry: LearningEntry): Promise<void>;
    searchLearnings(options: { limit: number; crossProject?: boolean }): Promise<readonly LearningEntry[]>;
    getSessionSummary(): Promise<{ sessionCount: number; tier: Tier }>;
  }

  export class FileSystemOfficeHoursStateStore implements OfficeHoursStateStore {
    private readonly baseDir: string;

    constructor(
      private readonly kaos: Kaos,
      homeDir: string,
    ) {
      this.baseDir = join(homeDir, 'office-hours');
    }

    private profilePath(): string {
      return join(this.baseDir, 'builder-profile.jsonl');
    }

    private analyticsPath(): string {
      return join(this.baseDir, 'analytics.jsonl');
    }

    private learningsPath(): string {
      return join(this.baseDir, 'learnings.jsonl');
    }

    private async ensureDir(): Promise<void> {
      await this.kaos.mkdir(this.baseDir, { parents: true, existOk: true });
    }

    async appendProfile(entry: BuilderProfileEntry): Promise<void> {
      await this.ensureDir();
      await this.kaos.writeText(this.profilePath(), JSON.stringify(entry) + '\n', { append: true });
    }

    async readProfile(): Promise<readonly BuilderProfileEntry[]> {
      try {
        const text = await this.kaos.readText(this.profilePath());
        return text
          .split('\n')
          .filter((line) => line.trim().length > 0)
          .map((line) => JSON.parse(line) as BuilderProfileEntry);
      } catch {
        return [];
      }
    }

    async appendAnalytics(event: OfficeHoursAnalyticsEvent): Promise<void> {
      await this.ensureDir();
      await this.kaos.writeText(this.analyticsPath(), JSON.stringify(event) + '\n', { append: true });
    }

    async appendLearning(entry: LearningEntry): Promise<void> {
      await this.ensureDir();
      await this.kaos.writeText(this.learningsPath(), JSON.stringify(entry) + '\n', { append: true });
    }

    async searchLearnings(options: {
      limit: number;
      crossProject?: boolean;
    }): Promise<readonly LearningEntry[]> {
      try {
        const text = await this.kaos.readText(this.learningsPath());
        const entries = text
          .split('\n')
          .filter((line) => line.trim().length > 0)
          .map((line) => JSON.parse(line) as LearningEntry);
        return entries.slice(-options.limit).reverse();
      } catch {
        return [];
      }
    }

    async getSessionSummary(): Promise<{ sessionCount: number; tier: Tier }> {
      const entries = await this.readProfile();
      return computeTier(entries);
    }
  }

  export class NoopOfficeHoursStateStore implements OfficeHoursStateStore {
    async appendProfile(): Promise<void> {}
    async readProfile(): Promise<readonly BuilderProfileEntry[]> { return []; }
    async appendAnalytics(): Promise<void> {}
    async appendLearning(): Promise<void> {}
    async searchLearnings(): Promise<readonly LearningEntry[]> { return []; }
    async getSessionSummary(): Promise<{ sessionCount: number; tier: Tier }> {
      return { sessionCount: 0, tier: 'introduction' };
    }
  }

  export function computeTier(
    entries: readonly BuilderProfileEntry[],
  ): { tier: Tier; sessionCount: number } {
    const sessionCount = entries.length;
    if (sessionCount === 0) return { tier: 'introduction', sessionCount };
    if (sessionCount <= 3) return { tier: 'welcome_back', sessionCount };
    if (sessionCount <= 7) return { tier: 'regular', sessionCount };
    return { tier: 'inner_circle', sessionCount };
  }

  export function selectResources(
    profileEntries: readonly BuilderProfileEntry[],
    candidates: Resource[],
  ): Resource[] {
    const shown = new Set(profileEntries.flatMap((e) => e.resourcesShown));
    if (shown.size >= 34) return [];
    const available = candidates.filter((r) => !shown.has(r.url));
    // Mix categories: never 3 of same type. Pick up to 3 with category diversity.
    const result: Resource[] = [];
    const usedCategories = new Set<string>();
    for (const r of available) {
      if (result.length >= 3) break;
      if (usedCategories.has(r.category) && usedCategories.size < available.length) continue;
      result.push(r);
      usedCategories.add(r.category);
    }
    return result;
  }
  ```

- [ ] 在 `packages/agent-core/src/agent/index.ts` 的 `AgentOptions` 接口中新增字段（line 100 之前）：
  ```typescript
  readonly officeHoursStateStore?: OfficeHoursStateStore;
  ```

  在文件顶部添加 import：
  ```typescript
  import type { OfficeHoursStateStore } from '#/office-hours/state';
  import { NoopOfficeHoursStateStore } from '#/office-hours/state';
  ```

- [ ] 在 `Agent` 类中新增属性（line 145 附近）：
  ```typescript
  readonly officeHoursStateStore: OfficeHoursStateStore;
  ```

- [ ] 在构造函数中初始化（line 175 附近）：
  ```typescript
  this.officeHoursStateStore = options.officeHoursStateStore ?? new NoopOfficeHoursStateStore();
  ```

- [ ] 运行测试验证 PASS：
  ```bash
  pnpm -F @odysseythink/agent-core test -- test/office-hours/state
  ```
  **预期：** 测试通过。

- [ ] 运行 `pnpm -F @odysseythink/agent-core typecheck` 确认编译通过。

- [ ] Commit: `feat: add OfficeHoursStateStore with builder profile, analytics, and learnings persistence`

---

## Task 10: State Tools（AppendBuilderProfile / AppendLearning / SearchLearnings）

**Depends on:** Task 9

**Files:**
- Create: `packages/agent-core/src/tools/builtin/office-hours/append-profile.ts`
- Create: `packages/agent-core/src/tools/builtin/office-hours/append-profile.md`
- Create: `packages/agent-core/src/tools/builtin/office-hours/append-learning.ts`
- Create: `packages/agent-core/src/tools/builtin/office-hours/append-learning.md`
- Create: `packages/agent-core/src/tools/builtin/office-hours/search-learnings.ts`
- Create: `packages/agent-core/src/tools/builtin/office-hours/search-learnings.md`
- Modify: `packages/agent-core/src/tools/builtin/index.ts`（追加 export）
- Modify: `packages/agent-core/src/agent/tool/index.ts:407-465`（注册 tools）

### Steps

- [ ] 创建 `packages/agent-core/src/tools/builtin/office-hours/append-profile.md`：
  ```markdown
  Append a builder profile entry for the current office-hours session. Captures founder signals, design doc path, resources shown, and session metadata for cross-session continuity (tier calculation, welcome-back messages, resource deduplication).
  ```

- [ ] 创建 `packages/agent-core/src/tools/builtin/office-hours/append-profile.ts`：
  ```typescript
  import type { Agent } from '#/agent';
  import { z } from 'zod';
  import type { BuiltinTool } from '../../../agent/tool';
  import type { ToolExecution } from '../../../loop/types';
  import { toInputJsonSchema } from '../../support/input-schema';
  import DESCRIPTION from './append-profile.md';

  export const AppendBuilderProfileInputSchema = z.object({
    mode: z.enum(['startup', 'builder']),
    projectSlug: z.string(),
    signalCount: z.number().int().min(0),
    signals: z.array(z.string()),
    designDoc: z.string(),
    assignment: z.string(),
    resourcesShown: z.array(z.string()),
    topics: z.array(z.string()),
  }).strict();
  export type AppendBuilderProfileInput = z.infer<typeof AppendBuilderProfileInputSchema>;

  export class AppendBuilderProfileTool implements BuiltinTool<AppendBuilderProfileInput> {
    readonly name = 'AppendBuilderProfile' as const;
    readonly description: string = DESCRIPTION;
    readonly parameters: Record<string, unknown> = toInputJsonSchema(AppendBuilderProfileInputSchema);

    constructor(private readonly agent: Agent) {}

    resolveExecution(args: AppendBuilderProfileInput): ToolExecution {
      return {
        description: 'Persisting builder profile entry',
        execute: async () => {
          if (this.agent.sessionMode.kind !== 'office-hours') {
            return {
              isError: true,
              output: 'AppendBuilderProfile is only available in office-hours mode.',
            };
          }
          try {
            await this.agent.officeHoursStateStore.appendProfile({
              date: new Date().toISOString(),
              ...args,
            });
            return { output: 'Builder profile entry saved.' };
          } catch (error) {
            const msg = error instanceof Error ? error.message : 'unknown error';
            return { isError: true, output: `Failed to save profile entry: ${msg}` };
          }
        },
      };
    }
  }
  ```

- [ ] 创建 `packages/agent-core/src/tools/builtin/office-hours/append-learning.md`：
  ```markdown
  Record a reusable insight or lesson learned from the current office-hours session. Learnings persist across projects and sessions and can be searched later to inform future office-hours diagnostics.
  ```

- [ ] 创建 `packages/agent-core/src/tools/builtin/office-hours/append-learning.ts`：
  ```typescript
  import type { Agent } from '#/agent';
  import { z } from 'zod';
  import type { BuiltinTool } from '../../../agent/tool';
  import type { ToolExecution } from '../../../loop/types';
  import { toInputJsonSchema } from '../../support/input-schema';
  import DESCRIPTION from './append-learning.md';

  export const AppendLearningInputSchema = z.object({
    type: z.enum(['operational', 'eureka']),
    key: z.string().min(1),
    insight: z.string().min(1),
    confidence: z.number().min(0).max(1),
    branch: z.string().optional(),
  }).strict();
  export type AppendLearningInput = z.infer<typeof AppendLearningInputSchema>;

  export class AppendLearningTool implements BuiltinTool<AppendLearningInput> {
    readonly name = 'AppendLearning' as const;
    readonly description: string = DESCRIPTION;
    readonly parameters: Record<string, unknown> = toInputJsonSchema(AppendLearningInputSchema);

    constructor(private readonly agent: Agent) {}

    resolveExecution(args: AppendLearningInput): ToolExecution {
      return {
        description: 'Recording learning/insight',
        execute: async () => {
          if (this.agent.sessionMode.kind !== 'office-hours') {
            return {
              isError: true,
              output: 'AppendLearning is only available in office-hours mode.',
            };
          }
          try {
            await this.agent.officeHoursStateStore.appendLearning({
              ts: new Date().toISOString(),
              skill: 'office-hours',
              source: 'observed',
              ...args,
            });
            return { output: 'Learning recorded.' };
          } catch (error) {
            const msg = error instanceof Error ? error.message : 'unknown error';
            return { isError: true, output: `Failed to record learning: ${msg}` };
          }
        },
      };
    }
  }
  ```

- [ ] 创建 `packages/agent-core/src/tools/builtin/office-hours/search-learnings.md`：
  ```markdown
  Search previously recorded office-hours learnings for insights relevant to the current session. Used during Phase 6 handoff to surface reusable patterns or resources.
  ```

- [ ] 创建 `packages/agent-core/src/tools/builtin/office-hours/search-learnings.ts`：
  ```typescript
  import type { Agent } from '#/agent';
  import { z } from 'zod';
  import type { BuiltinTool } from '../../../agent/tool';
  import type { ToolExecution } from '../../../loop/types';
  import { toInputJsonSchema } from '../../support/input-schema';
  import DESCRIPTION from './search-learnings.md';

  export const SearchLearningsInputSchema = z.object({
    limit: z.number().int().min(1).max(20).default(5),
    crossProject: z.boolean().default(false),
  }).strict();
  export type SearchLearningsInput = z.infer<typeof SearchLearningsInputSchema>;

  export class SearchLearningsTool implements BuiltinTool<SearchLearningsInput> {
    readonly name = 'SearchLearnings' as const;
    readonly description: string = DESCRIPTION;
    readonly parameters: Record<string, unknown> = toInputJsonSchema(SearchLearningsInputSchema);

    constructor(private readonly agent: Agent) {}

    resolveExecution(args: SearchLearningsInput): ToolExecution {
      return {
        description: 'Searching previous learnings',
        execute: async () => {
          if (this.agent.sessionMode.kind !== 'office-hours') {
            return {
              isError: true,
              output: 'SearchLearnings is only available in office-hours mode.',
            };
          }
          try {
            const results = await this.agent.officeHoursStateStore.searchLearnings(args);
            if (results.length === 0) {
              return { output: 'No previous learnings found.' };
            }
            const formatted = results
              .map(
                (l) =>
                  `- [${l.type}] ${l.key}: ${l.insight} (confidence: ${l.confidence}, ${l.ts})`,
              )
              .join('\n');
            return { output: formatted };
          } catch (error) {
            const msg = error instanceof Error ? error.message : 'unknown error';
            return { isError: true, output: `Failed to search learnings: ${msg}` };
          }
        },
      };
    }
  }
  ```

- [ ] 在 `packages/agent-core/src/tools/builtin/index.ts` 末尾追加：
  ```typescript
  export * from './office-hours/append-profile';
  export * from './office-hours/append-learning';
  export * from './office-hours/search-learnings';
  ```

- [ ] 在 `packages/agent-core/src/agent/tool/index.ts:419-465` 的 `builtinTools` Map 中追加注册（始终注册，内部检查 mode）：
  ```typescript
  new b.AppendBuilderProfileTool(this.agent),
  new b.AppendLearningTool(this.agent),
  new b.SearchLearningsTool(this.agent),
  ```

- [ ] 运行 `pnpm -F @odysseythink/agent-core typecheck` 确认编译通过。

- [ ] 运行全量测试确认无回归：
  ```bash
  pnpm -F @odysseythink/agent-core test
  ```
  **预期：** 全部通过。

- [ ] Commit: `feat: add AppendBuilderProfile, AppendLearning, and SearchLearnings tools`

---

## Task 11: Integration Tools（Telemetry、CLAUDE.md Routing、gbrain Sync）

**Depends on:** Task 9, Task 10

**Files:**
- Create: `packages/agent-core/src/tools/builtin/office-hours/ensure-routing.ts`
- Create: `packages/agent-core/src/tools/builtin/office-hours/ensure-routing.md`
- Create: `packages/agent-core/src/tools/builtin/office-hours/sync-artifact.ts`
- Create: `packages/agent-core/src/tools/builtin/office-hours/sync-artifact.md`
- Modify: `packages/agent-core/src/tools/builtin/index.ts`（追加 export）
- Modify: `packages/agent-core/src/agent/tool/index.ts:407-465`（注册 tools）
- Modify: `apps/ody-code/src/cli/run-office-hours.ts`（telemetry 事件）

### Steps

- [ ] 创建 `packages/agent-core/src/tools/builtin/office-hours/ensure-routing.md`：
  ```markdown
  Check whether office-hours routing rules exist in the project's CLAUDE.md file. If absent, ask the user whether to add them. When approved, appends a "## Skill routing" section with office-hours skill routing rules to CLAUDE.md.
  ```

- [ ] 创建 `packages/agent-core/src/tools/builtin/office-hours/ensure-routing.ts`：
  ```typescript
  import { join } from 'pathe';
  import type { Agent } from '#/agent';
  import { z } from 'zod';
  import type { BuiltinTool } from '../../../agent/tool';
  import type { ToolExecution } from '../../../loop/types';
  import { toInputJsonSchema } from '../../support/input-schema';
  import DESCRIPTION from './ensure-routing.md';

  const ROUTING_MARKER = '## Skill routing';
  const ROUTING_RULES = [
    '',
    '',
    '## Skill routing',
    '',
    'When the user\'s request matches an available skill, invoke it via the Skill tool.',
    '',
    'Key routing rules:',
    '- Product ideas / brainstorming → `ody --office-hours`',
    '- Strategy / scope → `/plan`',
    '- Architecture / design → `/design`',
    '- Bugs / errors → `/investigate`',
    '- QA / testing → `/qa`',
    '- Code review → `/review`',
  ].join('\n');

  export const EnsureClaudeMdRoutingInputSchema = z.object({}).strict();
  export type EnsureClaudeMdRoutingInput = z.infer<typeof EnsureClaudeMdRoutingInputSchema>;

  export class EnsureClaudeMdRoutingTool implements BuiltinTool<EnsureClaudeMdRoutingInput> {
    readonly name = 'EnsureClaudeMdRouting' as const;
    readonly description: string = DESCRIPTION;
    readonly parameters: Record<string, unknown> = toInputJsonSchema(EnsureClaudeMdRoutingInputSchema);

    constructor(private readonly agent: Agent) {}

    resolveExecution(_args: EnsureClaudeMdRoutingInput): ToolExecution {
      return {
        description: 'Checking CLAUDE.md routing rules',
        execute: async () => {
          if (this.agent.sessionMode.kind !== 'office-hours') {
            return {
              isError: true,
              output: 'EnsureClaudeMdRouting is only available in office-hours mode.',
            };
          }
          const claudeMdPath = join(this.agent.config.cwd, 'CLAUDE.md');
          try {
            const content = await this.agent.kaos.readText(claudeMdPath);
            if (content.includes(ROUTING_MARKER)) {
              return { output: 'CLAUDE.md already has routing rules — no changes needed.' };
            }
            await this.agent.kaos.writeText(claudeMdPath, content + ROUTING_RULES);
            return { output: 'Added office-hours routing rules to CLAUDE.md.' };
          } catch (error) {
            if ((error as { code?: string }).code === 'ENOENT') {
              // File doesn't exist — create it with routing rules
              try {
                await this.agent.kaos.writeText(claudeMdPath, ROUTING_RULES.trimStart());
                return { output: 'Created CLAUDE.md with office-hours routing rules.' };
              } catch (writeError) {
                return {
                  isError: true,
                  output: `Failed to create CLAUDE.md: ${writeError instanceof Error ? writeError.message : 'unknown error'}`,
                };
              }
            }
            return {
              isError: true,
              output: `Failed to update CLAUDE.md: ${error instanceof Error ? error.message : 'unknown error'}`,
            };
          }
        },
      };
    }
  }
  ```

- [ ] 创建 `packages/agent-core/src/tools/builtin/office-hours/sync-artifact.md`：
  ```markdown
  Sync the office-hours design document to gbrain (if configured). Checks for `.gbrain-source` pin and gbrain configuration. If gbrain is available, indexes the design file for cross-project search.
  ```

- [ ] 创建 `packages/agent-core/src/tools/builtin/office-hours/sync-artifact.ts`：
  ```typescript
  import { join } from 'pathe';
  import type { Agent } from '#/agent';
  import { z } from 'zod';
  import type { BuiltinTool } from '../../../agent/tool';
  import type { ToolExecution } from '../../../loop/types';
  import { toInputJsonSchema } from '../../support/input-schema';
  import DESCRIPTION from './sync-artifact.md';

  export const SyncOfficeHoursArtifactInputSchema = z.object({
    designFilePath: z.string().min(1),
  }).strict();
  export type SyncOfficeHoursArtifactInput = z.infer<typeof SyncOfficeHoursArtifactInputSchema>;

  export class SyncOfficeHoursArtifactTool implements BuiltinTool<SyncOfficeHoursArtifactInput> {
    readonly name = 'SyncOfficeHoursArtifact' as const;
    readonly description: string = DESCRIPTION;
    readonly parameters: Record<string, unknown> = toInputJsonSchema(SyncOfficeHoursArtifactInputSchema);

    constructor(private readonly agent: Agent) {}

    resolveExecution(args: SyncOfficeHoursArtifactInput): ToolExecution {
      return {
        description: 'Syncing design document to gbrain',
        execute: async () => {
          if (this.agent.sessionMode.kind !== 'office-hours') {
            return {
              isError: true,
              output: 'SyncOfficeHoursArtifact is only available in office-hours mode.',
            };
          }

          // Check for .gbrain-source pin
          const gbrainSourcePath = join(this.agent.config.cwd, '.gbrain-source');
          let pinExists = false;
          try {
            await this.agent.kaos.readText(gbrainSourcePath);
            pinExists = true;
          } catch {
            // no pin — gbrain not configured for this project
          }

          if (!pinExists) {
            return {
              output:
                'No .gbrain-source pin found in project root. gbrain sync not configured. ' +
                'The design document is saved locally. To enable gbrain sync, set up gbrain for this project.',
            };
          }

          // Check if gbrain MCP tool is available
          const gbrainTool = this.agent.tools.isToolActive('mcp__gbrain__index');
          if (gbrainTool) {
            return {
              output:
                `gbrain is configured for this project. Call the gbrain MCP tool to index: ${args.designFilePath}`,
            };
          }

          // Try shell-based gbrain as fallback
          try {
            const result = await this.agent.kaos.bash(`gbrain index "${args.designFilePath}"`, {
              cwd: this.agent.config.cwd,
              timeout: 10_000,
            });
            return { output: `gbrain sync complete:\n${result.stdout}` };
          } catch {
            return {
              output:
                'gbrain CLI not available. Install gbrain or configure the MCP server for artifact sync.',
            };
          }
        },
      };
    }
  }
  ```

- [ ] 在 `packages/agent-core/src/tools/builtin/index.ts` 末尾追加：
  ```typescript
  export * from './office-hours/ensure-routing';
  export * from './office-hours/sync-artifact';
  ```

- [ ] 在 `packages/agent-core/src/agent/tool/index.ts:407-465` 的 `builtinTools` Map 中追加注册：
  ```typescript
  new b.EnsureClaudeMdRoutingTool(this.agent),
  new b.SyncOfficeHoursArtifactTool(this.agent),
  ```

- [ ] 在 `apps/ody-code/src/cli/run-office-hours.ts` 中添加 telemetry 事件。在 `tui.start()` 之前：
  ```typescript
  track('office_hours_started', {
    project_slug: basename(workDir),
  });
  ```

  在 `tui.onExit` 中扩展 `track` 调用：
  ```typescript
  tui.onExit = async (exitCode = 0) => {
    setCrashPhase('shutdown');
    const sessionId = tui.getCurrentSessionId();
    withTelemetryContext({ sessionId }).track('office_hours_completed', {
      duration_s: (Date.now() - startedAt) / 1000,
      project_slug: basename(workDir),
      outcome: exitCode === 0 ? 'success' : 'abort',
    });
    await shutdownTelemetry({ timeoutMs: CLI_SHUTDOWN_TIMEOUT_MS });
    process.exit(exitCode);
  };
  ```

  注意：`run-office-hours.ts` 需要新增 `basename` import：
  ```typescript
  import { basename } from 'pathe';
  ```

- [ ] 运行 `pnpm -F @odysseythink/agent-core typecheck` 确认编译通过。

- [ ] 运行 `pnpm -F @odysseythink/ody-code typecheck` 确认编译通过。

- [ ] 运行全量测试：
  ```bash
  pnpm -F @odysseythink/agent-core test
  pnpm -F @odysseythink/ody-code test
  ```
  **预期：** 全部通过。

- [ ] Commit: `feat: add EnsureClaudeMdRouting and SyncOfficeHoursArtifact integration tools`

## Self-Review

- [ ] 1. Spec-coverage: Tasks 8-11 cover spec items 3 (Phase 1-6 workflow), 6 (builder profile), 7 (telemetry), 8 (learnings), 9 (CLAUDE.md routing), 10 (gbrain sync).
- [ ] 2. Placeholder scan: no TODO/TBD. All prompt fragments, store implementations, and tool code are complete.
- [ ] 3. No phantom tasks: Task 8 produces full contract; Task 9 produces state store + tests; Task 10 produces 3 state tools; Task 11 produces 2 integration tools + telemetry wiring.
- [ ] 4. Dependency soundness: Task 8 dep on Task 1; Task 9 dep on Task 1; Task 10 dep on Task 9; Task 11 dep on Task 9 + Task 10. All satisfied.
- [ ] 5. Caller & build soundness: `OfficeHoursInjector` imports from `office-hours-contract.ts` (now with full implementation). `Agent.officeHoursStateStore` is initialized with `NoopOfficeHoursStateStore` when not provided — all existing callers compile without changes. Tools use `agent.sessionMode.kind !== 'office-hours'` guard internally. `run-office-hours.ts` telemetry uses `track()` and `withTelemetryContext()` from `packages/telemetry` — already imported. Ends with `pnpm -F @odysseythink/agent-core typecheck` and `pnpm -F @odysseythink/ody-code typecheck`.
- [ ] 6. Test-the-risk: Task 8 tests verify Phase 1-6 content, hard gates, and AskUserQuestion discipline. Task 9 tests verify persistence round-trip, tier calculation edge cases (0/3/8 entries), resource dedup (34+ threshold), and noop safety. State-mutating tools (append profile, append learning) are covered by unit tests on the store.
- [ ] 7. Type consistency: `BuilderProfileEntry`, `OfficeHoursAnalyticsEvent`, `LearningEntry` match the design's data model spec. Tool input schemas use `z.enum(['startup', 'builder'])` matching `OfficeHoursMode`.
