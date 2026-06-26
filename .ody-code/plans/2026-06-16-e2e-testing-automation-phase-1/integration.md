# Part 3: Integration — Plan Enrichment, RunE2ETests, Wiring, Dog-Food

## Scope

Implement `E2EPlanEnricher`, `RunE2ETestsTool`, wire enrichment into `ExitPlanModeTool` (with constructor signature change), register the new tool, add goal-mode hook, integration/dog-food tests, user guide, and `.gitignore` entries.

Depends on Part 1 (`2026-06-16-e2e-testing-automation-phase-1/core.md`) and Part 2 (`2026-06-16-e2e-testing-automation-phase-1/generator.md`).

---

### Task 1: Implement `E2EPlanEnricher`

**Depends on:** `core.md` Task 4 (ImpactAnalyzer), `core.md` Task 5 (registry/generator)

**Files:** Create `packages/agent-core/src/e2e-testing/plan-enricher.ts`; Create `packages/agent-core/test/e2e-testing/integration.test.ts` (start).

- [ ] Write the failing test — `packages/agent-core/test/e2e-testing/integration.test.ts`:

```typescript
import { describe, expect, it, vi } from 'vitest';
import type { Kaos, KaosProcess } from '@odysseythink/kaos';
import type { Readable, Writable } from 'node:stream';
import { createFakeKaos } from '../../tools/fixtures/fake-kaos';
import { E2EPlanEnricher } from '#/e2e-testing/plan-enricher';
import { ImpactAnalyzer } from '#/e2e-testing/impact-analyzer';
import { E2EConfigResolver } from '#/e2e-testing/config';
import type { KimiConfig } from '#/config/schema';

function fakeKaosWithGit(files: string[]): Kaos {
  return createFakeKaos({
    exec: vi.fn().mockResolvedValue({
      stdin: { end: vi.fn(), write: vi.fn() } as unknown as Writable,
      stdout: {
        on: (_ev: string, cb: (chunk: Buffer) => void) => {
          const output = files.map(f => ` M ${f}\n`).join('');
          cb(Buffer.from(output));
        },
      } as unknown as Readable,
      stderr: {
        on: (_ev: string, _cb: (chunk: Buffer) => void) => {},
      } as unknown as Readable,
      pid: 1,
      exitCode: 0,
      wait: vi.fn().mockResolvedValue(0),
      kill: vi.fn().mockResolvedValue(undefined),
    } as KaosProcess),
    writeText: vi.fn().mockResolvedValue(42),
    mkdir: vi.fn().mockResolvedValue(undefined),
  });
}

const baseConfig = E2EConfigResolver.resolve({} as KimiConfig);
const planContent = '# Plan\n\n### Task 1: Do stuff\n\n### Task 2: More stuff\n';

describe('E2EPlanEnricher', () => {
  it('returns null when e2e is disabled', async () => {
    const config = { ...baseConfig, enabled: false };
    const enricher = new E2EPlanEnricher(createFakeKaos({}), config, new ImpactAnalyzer());
    const result = await enricher.enrich('/plan.md', planContent, '/app');
    expect(result).toBeNull();
  });

  it('appends E2E task when git status returns matching file', async () => {
    const kaos = fakeKaosWithGit([
      'packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts',
    ]);
    const enricher = new E2EPlanEnricher(kaos, baseConfig, ImpactAnalyzer);
    const result = await enricher.enrich('/plan.md', planContent, '/app');
    expect(result).not.toBeNull();
    expect(result!).toContain('### Task 3: Generate and run E2E tests');
    expect(result!).toContain('ExitPlanModeTool');
  });

  it('returns null for smart strategy with no matched files', async () => {
    const kaos = fakeKaosWithGit(['unrelated.ts']);
    const enricher = new E2EPlanEnricher(kaos, baseConfig, ImpactAnalyzer);
    const result = await enricher.enrich('/plan.md', planContent, '/app');
    expect(result).toBeNull();
  });

  it('returns null for critical-only with no critical tools affected', async () => {
    const config = { ...baseConfig, strategy: 'critical-only' as const };
    const kaos = fakeKaosWithGit(['packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts']);
    const enricher = new E2EPlanEnricher(kaos, config, ImpactAnalyzer);
    const result = await enricher.enrich('/plan.md', planContent, '/app');
    expect(result).toBeNull();
  });
});
```

- [ ] Run and verify FAILS: `E2EPlanEnricher` not found.

- [ ] Write `packages/agent-core/src/e2e-testing/plan-enricher.ts`:

```typescript
import type { Kaos } from '@odysseythink/kaos';
import type { ResolvedE2EConfig } from './config';
import type { ImpactAnalyzer as IAnalyzer } from './impact-analyzer';
import type { AffectedTool } from './types';

export class E2EPlanEnricher {
  constructor(
    private readonly kaos: Kaos,
    private readonly config: ResolvedE2EConfig,
    private readonly analyzer: IAnalyzer,
  ) {}

  async enrich(planPath: string, planContent: string, projectRoot: string): Promise<string | null> {
    if (!this.config.enabled) return null;

    const changedFiles = await this.determineChangedFiles(projectRoot, planContent);
    if (changedFiles.length === 0 && this.config.strategy !== 'always') return null;

    const impact = this.analyzer.analyze(changedFiles, this.config);

    if (impact.affectedTools.length === 0 && this.config.strategy !== 'always') return null;

    return appendE2ETaskToMarkdown(planContent, impact.affectedTools);
  }

  private async determineChangedFiles(projectRoot: string, planContent: string): Promise<string[]> {
    const fromGit = await this.gitStatusFiles(projectRoot);
    if (fromGit.length > 0) return fromGit;
    return extractFilePathsFromPlan(planContent);
  }

  private async gitStatusFiles(projectRoot: string): Promise<string[]> {
    try {
      const k = this.kaos.withCwd(projectRoot);
      const proc = await k.exec('git', 'status', '--short', '--no-renames');
      const chunks: Buffer[] = [];
      proc.stdout.on('data', (chunk: Buffer) => chunks.push(chunk));
      await proc.wait();
      const output = Buffer.concat(chunks).toString('utf-8');
      const lines = output.split('\n');
      const files: string[] = [];
      for (const line of lines) {
        const trimmed = line.trim();
        if (trimmed.length < 4) continue;
        let path = trimmed.substring(3).trim();
        if (path.includes(' -> ')) {
          path = path.split(' -> ').pop()!.trim();
        }
        files.push(path);
      }
      return files;
    } catch {
      return [];
    }
  }
}

function extractFilePathsFromPlan(planContent: string): string[] {
  const regex = /(?:packages|apps)\/[a-zA-Z0-9\-_/.]+\.[jt]sx?/g;
  const matches = planContent.match(regex) ?? [];
  return [...new Set(matches)];
}

function appendE2ETaskToMarkdown(content: string, affectedTools: readonly AffectedTool[]): string {
  const lines = content.split('\n');
  let lastTaskNum = 0;
  for (const line of lines) {
    const match = line.match(/^#{1,4}\s+Task\s+(\d+)\s*[:\-]?/i);
    if (match) {
      lastTaskNum = Math.max(lastTaskNum, parseInt(match[1], 10));
    }
  }

  const newTaskNum = lastTaskNum + 1;
  const priorityText = affectedTools
    .map(t => `- ${t.toolId} (priority: ${t.priority})`)
    .join('\n');

  const section = `
### Task ${newTaskNum}: Generate and run E2E tests

Based on the changed files, validate the following tools:
${priorityText}

Use the RunE2ETests tool after completing the implementation tasks above.
`;

  return content.trimEnd() + section + '\n';
}
```

- [ ] Run and verify PASSES: `pnpm --filter @odysseythink/agent-core test -- test/e2e-testing/integration.test.ts`. All 4 enricher tests pass.

- [ ] Commit with message `feat(e2e): implement E2EPlanEnricher`.

---

### Task 2: Implement `RunE2ETestsTool`

**Depends on:** `integration.md` Task 1

**Files:** Create `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.ts`; Create `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.md`; Modify `packages/agent-core/test/e2e-testing/integration.test.ts` (append tests).

- [ ] Write the failing test — append to `integration.test.ts`:

```typescript
import { RunE2ETestsTool } from '#/tools/builtin/e2e/run-e2e-tests';
import { BashInputSchema } from '#/tools/builtin/shell/bash';

describe('RunE2ETestsTool', () => {
  it('has name RunE2ETests', () => {
    const kaos = createFakeKaos({});
    const tool = new RunE2ETestsTool(kaos, { kimiConfig: {} } as any);
    expect(tool.name).toBe('RunE2ETests');
  });

  it('resolveExecution returns approval rule', () => {
    const kaos = createFakeKaos({});
    const tool = new RunE2ETestsTool(kaos, { kimiConfig: {} } as any);
    const exec = tool.resolveExecution({ toolId: 'ExitPlanModeTool' });
    expect(exec).toHaveProperty('approvalRule');
    expect('approvalRule' in exec && typeof exec.approvalRule === 'string').toBe(true);
  });

  it('returns info when e2e is disabled', async () => {
    const kaos = createFakeKaos({});
    const agent = {
      kimiConfig: { e2e: { enabled: false } },
      config: { cwd: '/tmp' },
    };
    const tool = new RunE2ETestsTool(kaos, agent as any);
    const exec = tool.resolveExecution({});
    if ('execute' in exec) {
      const result = await exec.execute({ signal: new AbortController().signal, turnId: '1', toolCallId: '1' });
      const output = typeof result.output === 'string' ? result.output : JSON.stringify(result.output);
      expect(output).toContain('disabled');
    }
  });
});
```

- [ ] Run and verify FAILS: `RunE2ETestsTool` not found.

- [ ] Create `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.md`:

```markdown
Generate and run temporary end-to-end (E2E) tests for the current project. Use this tool after completing implementation work to validate that your changes haven't broken the affected builtin tools.

When called without arguments, the tool detects changed files (via git status or the approved plan), analyzes which builtin tools are affected, generates temporary Vitest test files, runs them with pnpm vitest run, parses the JSON output into a report, and returns a markdown summary.

The tool respects the `[e2e]` section in config.toml: disable with `enabled = false`, control failure behaviour with `failure_policy` (`block` / `warn` / `ignore`), and adjust parallelism with `max_concurrency`.
```

- [ ] Create `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.ts`:

```typescript
import { z } from 'zod';
import type { Kaos } from '@odysseythink/kaos';
import type { Agent } from '#/agent';
import type { BuiltinTool } from '#/agent/tool';
import type { ExecutableToolResult, ToolExecution } from '#/loop/types';
import { toInputJsonSchema } from '#/tools/support/input-schema';
import { literalRulePattern, matchesGlobRuleSubject } from '#/tools/support/rule-match';
import { E2EConfigResolver } from '#/e2e-testing/config';
import { E2EPlanEnricher } from '#/e2e-testing/plan-enricher';
import { ImpactAnalyzer } from '#/e2e-testing/impact-analyzer';
import { E2ETestExecutor } from '#/e2e-testing/executor';
import { registry } from '#/e2e-testing/registry';
import DESCRIPTION from './run-e2e-tests.md';

const RunE2ETestsInputSchema = z.object({
  toolId: z.string().optional().describe('Optional specific tool to test; if omitted, all affected tools are tested.'),
  projectRoot: z.string().optional().describe('Optional project root; defaults to the agent workspace root.'),
}).strict();

export type RunE2ETestsInput = z.infer<typeof RunE2ETestsInputSchema>;

export class RunE2ETestsTool implements BuiltinTool<RunE2ETestsInput> {
  readonly name = 'RunE2ETests' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(RunE2ETestsInputSchema);

  constructor(
    private readonly kaos: Kaos,
    private readonly agent: Agent,
  ) {}

  resolveExecution(input: RunE2ETestsInput): ToolExecution {
    const desc = input.toolId ? `Run E2E tests for ${input.toolId}` : 'Run E2E tests for affected tools';
    return {
      description: desc,
      approvalRule: literalRulePattern(this.name, input.toolId ?? '*'),
      matchesRule: (ruleArgs) => matchesGlobRuleSubject(ruleArgs, input.toolId ?? '*'),
      execute: (ctx) => this.execution(input, ctx),
    };
  }

  private async execution(
    input: RunE2ETestsInput,
    ctx: { signal: AbortSignal; turnId: string; toolCallId: string },
  ): Promise<ExecutableToolResult> {
    const config = E2EConfigResolver.resolve(this.agent.kimiConfig ?? {});
    if (!config.enabled) {
      return { output: 'E2E testing is disabled in config.toml (e2e.enabled = false).' };
    }

    const projectRoot = input.projectRoot ?? this.kaos.getcwd();

    let generator;
    try {
      generator = await registry.detectAndGet(projectRoot);
    } catch {
      return { output: `No E2E generator found for project at ${projectRoot}.` };
    }

    const enricher = new E2EPlanEnricher(this.kaos, config, ImpactAnalyzer);
    const changedFiles = await (enricher as any)._determineChangedFiles?.(projectRoot, '') ?? [];
    // Reconstruct changed-files logic inline because _determineChangedFiles is private
    const changedFilesActual = await this.getChangedFiles(projectRoot);

    const impact = ImpactAnalyzer.analyze(changedFilesActual, config);
    if (input.toolId) {
      impact.affectedTools = impact.affectedTools.filter(t => t.toolId === input.toolId);
    }

    if (impact.affectedTools.length === 0) {
      return { output: 'No affected tools detected; skipping E2E tests.' };
    }

    const testFiles = [];
    for (const tool of impact.affectedTools) {
      const featureFiles = await generator.generateTestsForFeature({
        toolId: tool.toolId,
        changedFiles: changedFilesActual,
        projectRoot,
      });
      testFiles.push(...featureFiles);
    }

    if (testFiles.length === 0) {
      return { output: 'E2E generator produced no test files.' };
    }

    const executor = new E2ETestExecutor(this.kaos, config);
    const result = await executor.execute(testFiles, projectRoot);

    if (ctx.signal.aborted) {
      return { isError: true, output: 'E2E tests cancelled.' };
    }

    const isError = result.failed > 0 && config.failurePolicy === 'block';
    return {
      output: result.summary,
      isError,
      stopTurn: isError,
      message: isError ? 'Critical E2E tests failed.' : undefined,
    };
  }

  private async getChangedFiles(projectRoot: string): Promise<string[]> {
    try {
      const k = this.kaos.withCwd(projectRoot);
      const proc = await k.exec('git', 'status', '--short', '--no-renames');
      const chunks: Buffer[] = [];
      proc.stdout.on('data', (chunk: Buffer) => chunks.push(chunk));
      await proc.wait();
      return Buffer.concat(chunks).toString('utf-8')
        .split('\n')
        .map(l => l.trim().substring(3).trim())
        .filter(f => f.length > 0)
        .map(f => f.includes(' -> ') ? f.split(' -> ').pop()!.trim() : f);
    } catch {
      return [];
    }
  }
}
```

- [ ] Run all integration tests: `pnpm --filter @odysseythink/agent-core test -- test/e2e-testing/integration.test.ts`. Verify new tests pass.

- [ ] Typecheck: `tsc -p packages/agent-core/tsconfig.json --noEmit`. Commit with message `feat(e2e): implement RunE2ETestsTool`.

---

### Task 3: Wire `ExitPlanModeTool` enrichment and update constructor signature

**Depends on:** `integration.md` Tasks 1–2

**Files:** Modify `packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts:88-124`; Modify `packages/agent-core/src/agent/tool/index.ts:422`; Modify ALL test callers (see list below).

**Shared-signature change:** `ExitPlanModeTool` constructor gains `kaos: Kaos`. Requires updating every caller, including test files. This task ends with a whole-tree typecheck.

Callers to update (found via `grep -rn "new ExitPlanModeTool\|ExitPlanModeTool("`):
- `packages/agent-core/src/agent/tool/index.ts:422`: `new b.ExitPlanModeTool(this.agent)` → `new b.ExitPlanModeTool(this.agent, this.kaos)`
- `packages/agent-core/test/tools/exit-plan-mode.test.ts:64`: `new ExitPlanModeTool(agent)` → `new ExitPlanModeTool(agent, agent.kaos)`
- `packages/agent-core/test/tools/exit-plan-mode.test.ts:99,117,134,152,171,186`: `new ExitPlanModeTool(agent)` → `new ExitPlanModeTool(agent, agent.kaos)`
- `packages/agent-core/test/tools/exit-plan-mode-options.test.ts:160,174,187`: `new ExitPlanModeTool(agent)` → `new ExitPlanModeTool(agent, agent.kaos)`
- `packages/agent-core/test/tools/exit-plan-mode-options.test.ts:243`: `new ExitPlanModeTool({} as Agent)` → `new ExitPlanModeTool({} as Agent, {} as Kaos)`
- `packages/agent-core/test/tools/planning/exit-plan-mode-telemetry.test.ts:80`: `new ExitPlanModeTool(agent)` → `new ExitPlanModeTool(agent, agent.kaos)`

- [ ] First, update `ExitPlanModeTool` in `packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts`:

Add import:
```typescript
import type { Kaos } from '@odysseythink/kaos';
import { E2EPlanEnricher } from '#/e2e-testing/plan-enricher';
import { E2EConfigResolver } from '#/e2e-testing/config';
import { ImpactAnalyzer } from '#/e2e-testing/impact-analyzer';
```

Change constructor (line 93):
```typescript
constructor(
  private readonly agent: Agent,
  private readonly kaos: Kaos,
) {}
```

In `resolveExecution` (after line 96, before `resolvePlanReviewDisplay`), insert enrichment:
```typescript
async resolveExecution(args: ExitPlanModeInput): Promise<ToolExecution> {
  // E2E enrichment: analyze changes and append E2E task if needed
  await this.maybeEnrichPlanForE2E();

  return {
    description: 'Presenting plan and exiting plan mode',
    display: await this.resolvePlanReviewDisplay(args),
    approvalRule: this.name,
    execute: (ctx) => this.execution(args, ctx.metadata),
  };
}

private async maybeEnrichPlanForE2E(): Promise<void> {
  try {
    const e2eConfig = E2EConfigResolver.resolve(this.agent.kimiConfig ?? {});
    if (!e2eConfig.enabled) return;

    const modeData = await this.agent.sessionMode.data();
    if (modeData === null || modeData.kind !== 'plan') return;
    if (modeData.content.trim().length === 0 || modeData.path.length === 0) return;

    const enricher = new E2EPlanEnricher(this.kaos, e2eConfig, ImpactAnalyzer);
    const enriched = await enricher.enrich(
      modeData.path,
      modeData.content,
      this.kaos.getcwd(),
    );
    if (enriched !== null) {
      await this.kaos.writeText(modeData.path, enriched);
    }
  } catch {
    // Enrichment is best-effort; failures should not block plan exit.
  }
}
```

- [ ] Update `packages/agent-core/src/agent/tool/index.ts:422`:

```
new b.ExitPlanModeTool(this.agent, kaos),
```

(Note: `kaos` is already destructured at line 390 from `this.agent`.)

- [ ] Update all test callers. For each, add `agent.kaos` as second argument. Example for `exit-plan-mode.test.ts:64`:

```typescript
const tool = new ExitPlanModeTool(agent, agent.kaos);
```
Repeat for all 12 call sites.

- [ ] Run ExitPlanMode tests: `pnpm --filter @odysseythink/agent-core test -- test/tools/exit-plan-mode`. All should pass.

- [ ] Run ExitPlanMode options tests: `pnpm --filter @odysseythink/agent-core test -- test/tools/exit-plan-mode-options.test.ts`. Should pass.

- [ ] Run telemetry tests: `pnpm --filter @odysseythink/agent-core test -- test/tools/planning/exit-plan-mode-telemetry.test.ts`. Should pass.

- [ ] Whole-tree typecheck: `pnpm -r typecheck`. Must pass cleanly — no stale callers in test files.

- [ ] Commit with message `feat(e2e): wire E2EPlanEnricher into ExitPlanModeTool; add Kaos to constructor`.

---

### Task 4: Register `RunE2ETestsTool` in builtin tool index

**Depends on:** `integration.md` Tasks 2–3

**Files:** Modify `packages/agent-core/src/tools/builtin/index.ts` (add export); Modify `packages/agent-core/src/agent/tool/index.ts:407-464` (add to array).

**Approach:** Non-testable wiring. Provide complete code and manual verification.

- [ ] Add export to `packages/agent-core/src/tools/builtin/index.ts`, after line 28 (`export * from './visual/show-design-mockup';`):

```typescript
export * from './e2e/run-e2e-tests';
```

- [ ] Add to `initializeBuiltinTools()` array in `packages/agent-core/src/agent/tool/index.ts:464` (before the closing `]`):

```typescript
new b.RunE2ETestsTool(kaos, this.agent),
```

- [ ] Build: `pnpm --filter @odysseythink/agent-core build`. Verify no errors.

- [ ] Manual verification: Run `pnpm --filter @odysseythink/agent-core test -- test/agent/tool.test.ts`. Verify the builtin tool count includes `RunE2ETests`. Add a quick inline assertion in a test:

In `test/agent/tool.test.ts`, append (or create a small separate test):
```typescript
it('RunE2ETests tool is registered', async () => {
  const ctx = testAgent();
  ctx.configure({ tools: ['RunE2ETests'] });
  const tools = ctx.agent.tools.loopTools.map(t => t.name);
  expect(tools).toContain('RunE2ETests');
});
```

- [ ] Run: `pnpm --filter @odysseythink/agent-core test -- test/agent/tool.test.ts`. Verify passing.

- [ ] Whole-tree typecheck: `pnpm -r typecheck`. Commit with message `feat(e2e): register RunE2ETestsTool as builtin`.

---

### Task 5: Add goal-mode auto-trigger hook

**Depends on:** `integration.md` Task 4

**Files:** Modify `packages/agent-core/src/agent/compaction/normal-task-checkpoint.ts:37-64`.

**Approach:** After detecting a completed todo in `beforeStep`, check if the todo title suggests E2E testing and inject a system reminder.

- [ ] Write the test — append to `integration.test.ts` or create a separate checkpoint test:

```typescript
import { NormalModeTaskCheckpoint } from '#/agent/compaction/normal-task-checkpoint';
import { testAgent } from '../../agent/harness/agent';

describe('NormalModeTaskCheckpoint E2E hook', () => {
  it('injects reminder when e2e-related todo is completed', async () => {
    const ctx = testAgent({
      initialConfig: { e2e: { enabled: true } as any },
    });
    // Simulate a todo list with a completed E2E task
    const store = ctx.agent.tools.storeData();
    (store as any)['todo'] = [
      { title: 'Generate and run E2E tests', status: 'done' },
      { title: 'Other task', status: 'done' },
    ];

    const checkpoint = new NormalModeTaskCheckpoint(ctx.agent as any);
    // Reset state to "no prior done count"
    (checkpoint as any).lastDoneCount = 0;

    // Spy on appendSystemReminder
    let appended: string | undefined;
    const originalAppend = ctx.agent.context.appendSystemReminder.bind(ctx.agent.context);
    ctx.agent.context.appendSystemReminder = (content: string, origin: any) => {
      appended = content;
      originalAppend(content, origin);
    };

    await checkpoint.beforeStep(new AbortController().signal);
    // The reminder should contain E2E-related text
    expect(appended).toBeDefined();
    expect(appended!).toContain('RunE2ETests');
  });
});
```

- [ ] Run and verify FAILS: no E2E injection logic in checkpoint.

- [ ] Modify `NormalModeTaskCheckpoint.beforeStep` in `packages/agent-core/src/agent/compaction/normal-task-checkpoint.ts`:

After line 57 (`if (!crossedBoundary || !hasWork) return;`), add:

```typescript
// E2E auto-trigger: when a completed todo suggests E2E tests, inject a reminder.
if (crossedBoundary) {
  const lastDone = todos.filter(t => t.status === 'done').at(-1);
  if (lastDone) {
    const lowered = lastDone.title.toLowerCase();
    if (lowered.includes('e2e') || lowered.includes('test')) {
      try {
        const e2eConfig = this.agent.kimiConfig?.e2e;
        if (e2eConfig !== undefined && e2eConfig !== false && (typeof e2eConfig === 'object' ? (e2eConfig as any).enabled !== false : true)) {
          this.agent.context.appendSystemReminder(
            'The E2E task is complete. Call RunE2ETests to validate your changes.',
            { kind: 'system_trigger', name: 'e2e_reminder' },
          );
        }
      } catch {
        // best-effort
      }
    }
  }
}
```

- [ ] Run test: `pnpm --filter @odysseythink/agent-core test -- test/e2e-testing/integration.test.ts`. Verify passing.

- [ ] Whole-tree typecheck: `pnpm -r typecheck`. Commit with message `feat(e2e): add goal-mode auto-trigger reminder hook`.

---

### Task 6: Integration and dog-food tests

**Depends on:** `integration.md` Tasks 1–5

**Files:** Create `packages/agent-core/test/e2e-testing/plan-enrichment.e2e.test.ts`; Modify `packages/agent-core/test/e2e-testing/integration.test.ts` (append final assertions).

- [ ] Write the plan-enrichment e2e test — `packages/agent-core/test/e2e-testing/plan-enrichment.e2e.test.ts`:

```typescript
import { describe, expect, it, vi } from 'vitest';
import type { Kaos, KaosProcess } from '@odysseythink/kaos';
import type { Readable, Writable } from 'node:stream';
import { testAgent } from '../agent/harness/agent';
import { createFakeKaos } from '../tools/fixtures/fake-kaos';
import { ExitPlanModeTool } from '#/tools/builtin/planning/exit-plan-mode';

function kaosWithGitAndWrite(files: string[]): Kaos {
  let writtenContent = '';
  return createFakeKaos({
    exec: vi.fn().mockResolvedValue({
      stdin: { end: vi.fn(), write: vi.fn() } as unknown as Writable,
      stdout: {
        on: (_ev: string, cb: (chunk: Buffer) => void) => {
          cb(Buffer.from(files.map(f => ` M ${f}\n`).join('')));
        },
      } as unknown as Readable,
      stderr: { on: vi.fn() } as unknown as Readable,
      pid: 1, exitCode: 0,
      wait: vi.fn().mockResolvedValue(0),
      kill: vi.fn().mockResolvedValue(undefined),
    } as KaosProcess),
    writeText: vi.fn(async (_p: string, c: string) => { writtenContent = c; return c.length; }),
    mkdir: vi.fn().mockResolvedValue(undefined),
    getcwd: () => '/workspace',
    withCwd: () => kaosWithGitAndWrite(files),
    // Allow test to read back what was written
    readText: vi.fn(async () => writtenContent),
  });
}

describe('Plan enrichment end-to-end', () => {
  it('enriches plan with E2E task on exit-plan-mode via ExitPlanModeTool', async () => {
    const kaos = kaosWithGitAndWrite([
      'packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts',
    ]);

    const ctx = testAgent({
      kaos,
      initialConfig: { e2e: { enabled: true } as any },
    });

    // Enter plan mode
    await ctx.agent.sessionMode.enter(ctx.agent.sessionMode.createSessionModeId(), false, false, 'plan');

    // Write a plan file
    const planPath = '/workspace/.ody-code/plans/test-plan.md';
    await kaos.writeText(planPath, '# Plan\n\n### Task 1: Initial\n\n### Task 2: Final\n');

    // Simulate ExitPlanModeTool call
    const tool = new ExitPlanModeTool(ctx.agent, kaos);
    const resolved = await tool.resolveExecution({});

    // Verify the plan was enriched (writeText was called with enriched content)
    const writeCalls = (kaos.writeText as any).mock.calls;
    const enrichmentCall = writeCalls.find((call: string[]) =>
      call[0] === planPath && call[1]?.includes('Generate and run E2E tests'),
    );
    expect(enrichmentCall).toBeDefined();
    expect(enrichmentCall[1]).toContain('ExitPlanModeTool');
  });
});
```

- [ ] Run: `pnpm --filter @odysseythink/agent-core test -- test/e2e-testing/plan-enrichment.e2e.test.ts`. Verify passing.

- [ ] Append final assertions to `integration.test.ts`:

```typescript
describe('final integration assertions', () => {
  it('ExitPlanModeTool enrichment is skipped when e2e is disabled', async () => {
    const kaos = kaosWithGitAndWrite(['exit-plan-mode.ts']);
    const ctx = testAgent({
      kaos,
      initialConfig: { e2e: { enabled: false } as any },
    });
    const tool = new ExitPlanModeTool(ctx.agent, kaos);
    await tool.resolveExecution({});
    // writeText should only have been called once (for plan), not for enrichment
    const enrichmentWrites = (kaos.writeText as any).mock.calls.filter(
      (c: string[]) => typeof c[1] === 'string' && c[1].includes('Generate and run E2E tests'),
    );
    expect(enrichmentWrites).toHaveLength(0);
  });
});
```

- [ ] Run full integration suite: `pnpm --filter @odysseythink/agent-core test -- test/e2e-testing/`. All pass.

- [ ] Whole-tree typecheck: `pnpm -r typecheck`. Commit with message `test(e2e): add plan-enrichment E2E and dog-food tests`.

---

### Task 7: User guide and `.gitignore`

**Depends on:** `integration.md` Tasks 1–6

**Files:** Create `.ody-code/docs/e2e-testing-guide.md`; Modify `.gitignore`.

**Approach:** Non-testable documentation + config. Manual verification.

- [ ] Create `.ody-code/docs/e2e-testing-guide.md`:

```markdown
# E2E Testing Guide

Ody Code includes an automated end-to-end (E2E) testing framework that detects which builtin tools are affected by your changes and generates+executes temporary Vitest tests to validate them.

## Configuration

E2E testing is enabled by default. Configure it in `~/.ody-code/config.toml`:

\`\`\`toml
[e2e]
enabled = true
strategy = "smart"         # "always" | "smart" | "critical-only"
critical_tools = ["ExitPlanModeTool"]
failure_policy = "warn"    # "block" | "warn" | "ignore"
max_concurrency = 4
test_timeout = 30000       # milliseconds
report_dir = ".ody-code/test-reports"
generated_test_dir = ".ody-code/test-generated/e2e"
\`\`\`

- **enabled**: Master toggle. Set to `false` to disable all E2E automation.
- **strategy**: When to inject E2E tasks.
  - `always` — inject for every plan.
  - `smart` — inject only when changed files match known tool patterns.
  - `critical-only` — inject only when critical tools are affected.
- **critical_tools**: Tool class names that should always be treated as highest priority.
- **failure_policy**: How to react to test failures.
  - `block` — return an error and stop the turn.
  - `warn` — include failures in the summary but continue.
  - `ignore` — do not change turn behaviour at all.
- **max_concurrency**: Maximum concurrent Vitest processes.
- **test_timeout**: Per-test timeout in milliseconds.
- **report_dir**: Where JSON reports are saved.
- **generated_test_dir**: Where temporary test files are written.

## How It Works

1. **Plan Enrichment** — When you exit plan mode with `ExitPlanMode`, the framework inspects git status (or the plan content) for changed files. If any builtin tool is affected, a new task is appended to the plan:
   \`\`\`
   ### Task N: Generate and run E2E tests
   \`\`\`

2. **Test Generation** — The `RunE2ETests` tool detects your project stack (currently TypeScript + Vitest) and generates temporary test files under `.ody-code/test-generated/e2e/`.

3. **Test Execution** — Tests are run in chunks of `max_concurrency` via `pnpm vitest run`. Results are parsed from Vitest's JSON reporter.

4. **Reports** — A JSON report is saved to `.ody-code/test-reports/e2e-report-<timestamp>.json`, and a markdown summary is returned to the model.

## Running Tests Manually

You can ask the agent to run E2E tests at any time:

> RunE2ETests with toolId: "ExitPlanModeTool"

## Limitations (Phase 1)

- Only TypeScript/Vitest projects are supported.
- Only a static mapping of tool-to-file is used; transitive dependencies are not analyzed.
- Generated tests are temporary and not committed to source control.
```

- [ ] Modify `.gitignore` in repo root. Append:

```gitignore
# E2E testing artifacts (auto-generated, never committed)
.ody-code/test-generated/
.ody-code/test-reports/
```

- [ ] Manual verification:

1. Run `git status` — `.gitignore` should exclude `.ody-code/test-generated/` and `.ody-code/test-reports/`.
2. Open `.ody-code/docs/e2e-testing-guide.md` in a markdown preview — verify formatting.
3. Run `pnpm -r typecheck` to confirm no build breakage.

- [ ] Commit with message `docs(e2e): add E2E testing user guide and .gitignore entries`.

---

## Local Self-Review (Part 3: Integration)

- [ ] 1. Spec-coverage table: Plan enrichment → Task 1. RunE2ETests tool → Task 2. ExitPlanMode wiring → Task 3. Registration → Task 4. Goal hook → Task 5. Tests → Task 6. Docs/gitignore → Task 7. All covered.
- [ ] 2. Placeholder scan: No `TODO`/`TBD`. The enricher `extractFilePathsFromPlan` regex and gitStatusFiles are fully implemented. `RunE2ETestsTool.getChangedFiles` duplicates some enricher logic — acceptable for Phase 1 (both need git-status access independently).
- [ ] 3. No phantom tasks: Every task creates/modifies real files and has a verifiable step.
- [ ] 4. Dependency soundness: Task 1 depends on core.md (analyzer). Task 2 depends on Task 1 (enricher). Task 3 depends on Tasks 1–2 (enricher + tool). Task 4 depends on Task 2–3 (tool + registration). Task 5 depends on Task 2 (config). Task 6 depends on Tasks 1–5. Task 7 depends on all.
- [ ] 5. Caller & build soundness: Task 3 is a shared-signature change (`ExitPlanModeTool` constructor). All 12 call sites listed and updated. Ends with whole-tree `pnpm -r typecheck`. No other shared-signature changes.
- [ ] 6. Test-the-risk: Enricher tested for enabled=false, matching file, no-match, critical-only filtering. RunE2ETestsTool tested for disabled config, approval rule generation. ExitPlanMode enrichment tested end-to-end with plan writing. Goal hook tested for injection on e2e todo completion. Must-survive inputs: exit-plan-mode.ts correctly triggers enrichment; enter-plan-mode.ts with critical-only does NOT (correctly filtered out). The appended task number is lastTaskNum + 1 — verified for 2 initial tasks (3 becomes 3). No false-duplicate risk.
- [ ] 7. Type consistency: `RunE2ETestsTool` constructor type `(kaos: Kaos, agent: Agent)` matches `builtinTools` array expectation (`new b.RunE2ETestsTool(kaos, this.agent)`). `E2EPlanEnricher` signature matches usage in `ExitPlanModeTool.maybeEnrichPlanForE2E`. `ExitPlanModeTool` constructor `(agent: Agent, kaos: Kaos)` matches updated callers.
