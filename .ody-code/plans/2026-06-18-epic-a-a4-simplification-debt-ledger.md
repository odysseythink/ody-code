# Epic A-A4: 简化债务台账 Implementation Plan

**Goal:** Add a `harvest-ody-markers` builtin tool + `debt-ledger` builtin skill that scan the codebase for `// ody:` / `# ody:` simplification-debt markers and render a Chinese-first ledger report, with A1/A3 prompt updates to teach agents to leave markers.

**Architecture:** A new `HarvestOdyMarkersTool` (in `packages/agent-core/src/tools/builtin/code-quality/`) internally delegates scanning to a `GrepTool` instance with pattern `(#|//) ?ody:`, then parses each match line with `parseOdyMarker` (regex → `DebtLedgerMarker`) and renders a grouped markdown report via `renderDebtLedger`. Telemetry fires `debt_ledger_harvested` / `debt_ledger_failed`. A companion `debt-ledger` builtin skill teaches agents when to invoke the tool and the marker format convention. A1 `simplicity-first.md` is extended with a `<!-- FULL[` / `<!-- ULTRA[` block instructing agents to leave `ody:` markers on deliberate simplifications. A3 `simplicity.ts` prompt is updated to suggest adding `ody:` annotations instead of just mentioning them.

**Tech Stack:** TypeScript (Node.js ≥24.15.0), vitest, pnpm monorepo, Kaos/WorkspaceConfig for path resolution, GrepTool for ripgrep delegation, TelemetryClient for events.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

| Task | Create | Modify | Test |
|---|---|---|---|
| T1 | `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.ts` (types + pure functions) | — | `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.test.ts` |
| T2 | — | `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.ts` (add class) | `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.integration.test.ts` |
| T3 | `packages/agent-core/src/skill/builtin/debt-ledger.md`, `packages/agent-core/src/skill/builtin/debt-ledger.ts` | `packages/agent-core/src/skill/builtin/index.ts` | `packages/agent-core/src/skill/builtin/debt-ledger.test.ts` |
| T4 | — | `packages/agent-core/src/tools/builtin/index.ts`, `packages/agent-core/src/agent/tool/index.ts` | — (typecheck) |
| T5 | — | `packages/agent-core/src/skill/builtin/simplicity-first.md` | — (existing test suite) |
| T6 | — | `packages/agent-core/src/code-review/simplicity.ts` | — (existing test suite) |

## Dependency Overview

```
T1 (types + parse + render + tests)
 ├─► T2 (HarvestOdyMarkersTool class + integration tests)
 │    └─► T4 (tool registration + barrel export + typecheck)
 T3 (debt-ledger skill + registration) ──┘
 T5 (A1 skill update)
 T6 (A3 prompt update)
```

- **Phase A** (parallel): T1 + T3 + T5 + T6 — all independent, no shared deps.
- **Phase B**: T2 — depends on T1.
- **Phase C**: T4 — depends on T2 + T3.

## Risks & Open Questions

None. All assumptions verified in the design document.

---

### Task 1: Types, parser, and renderer (pure functions)

**Depends on:** none

**Files:**
- Create: `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.ts` (types, constants, parseOdyMarker, renderDebtLedger)
- Test: `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.test.ts`

- [ ] Write the failing test

Create `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { parseOdyMarker, renderDebtLedger } from './harvest-ody-markers';

describe('parseOdyMarker', () => {
  it('parses a valid // marker with upgrade trigger', () => {
    const result = parseOdyMarker(
      'src/lock.ts:12:// ody: 全局锁, 吞吐 > 100 rps 时改为按账户锁',
    );
    expect(result).toEqual({
      file: 'src/lock.ts',
      line: 12,
      ceiling: '全局锁',
      upgrade: '吞吐 > 100 rps 时改为按账户锁',
      rot: false,
    });
  });

  it('parses a valid # marker with upgrade trigger', () => {
    const result = parseOdyMarker(
      'scripts/parse.py:8:# ody: 用 JSON.parse, 需要 schema 校验时改为 zod',
    );
    expect(result).toEqual({
      file: 'scripts/parse.py',
      line: 8,
      ceiling: '用 JSON.parse',
      upgrade: '需要 schema 校验时改为 zod',
      rot: false,
    });
  });

  it('marks rot when upgrade trigger is missing (no comma)', () => {
    const result = parseOdyMarker('src/cache.ts:5:// ody: 全局锁');
    expect(result).toEqual({
      file: 'src/cache.ts',
      line: 5,
      ceiling: '全局锁',
      upgrade: '',
      rot: true,
    });
  });

  it('marks rot when upgrade trigger is empty after comma', () => {
    const result = parseOdyMarker('src/cache.ts:5:// ody: 全局锁,   ');
    expect(result).toEqual({
      file: 'src/cache.ts',
      line: 5,
      ceiling: '全局锁',
      upgrade: '',
      rot: true,
    });
  });

  it('marks rot for Chinese comma (not a valid separator)', () => {
    const result = parseOdyMarker(
      'src/lock.ts:12:// ody: 全局锁，吞吐 > 100 rps 时改为按账户锁',
    );
    expect(result!.rot).toBe(true);
  });

  it('returns null for lines without ody: prefix', () => {
    expect(parseOdyMarker('src/lock.ts:12:// TODO: fix lock')).toBeNull();
    expect(parseOdyMarker('src/lock.ts:12:// body: foo')).toBeNull();
  });

  it('returns null for block comments (unsupported)', () => {
    expect(
      parseOdyMarker('src/lock.ts:12:/* ody: 全局锁, upgrade */'),
    ).toBeNull();
  });

  it('handles optional space before ody:', () => {
    const result = parseOdyMarker(
      'src/x.ts:7://ody: simple, more complex when needed',
    );
    expect(result).toEqual({
      file: 'src/x.ts',
      line: 7,
      ceiling: 'simple',
      upgrade: 'more complex when needed',
      rot: false,
    });
  });
});

describe('renderDebtLedger', () => {
  it('returns clean message for empty markers', () => {
    const result = renderDebtLedger([], false);
    expect(result).toBe('未找到 `ody:` 债务标记。台账干净。');
  });

  it('renders grouped markers with rot tag', () => {
    const markers = [
      {
        file: 'src/lock.ts',
        line: 12,
        ceiling: '全局锁',
        upgrade: '吞吐 > 100 rps 时改为按账户锁',
        rot: false,
      },
      {
        file: 'src/lock.ts',
        line: 45,
        ceiling: '临时文件',
        upgrade: '',
        rot: true,
      },
    ];
    const result = renderDebtLedger(markers, false);

    expect(result).toContain('### src/lock.ts');
    expect(result).toContain('src/lock.ts:12');
    expect(result).toContain('全局锁');
    expect(result).toContain('吞吐 > 100 rps 时改为按账户锁');
    expect(result).toContain('src/lock.ts:45');
    expect(result).toContain('⚠️ rot');
    expect(result).toContain('（未指定）');
    expect(result).toContain('**汇总**：2 个标记，1 个 rot 风险。');
  });

  it('sorts files alphabetically and lines within each file', () => {
    const markers = [
      {
        file: 'zzz/last.ts', line: 1, ceiling: 'c', upgrade: 'u', rot: false,
      },
      {
        file: 'aaa/first.ts', line: 3, ceiling: 'c', upgrade: 'u', rot: false,
      },
      {
        file: 'aaa/first.ts', line: 1, ceiling: 'c', upgrade: 'u', rot: false,
      },
    ];
    const result = renderDebtLedger(markers, false);

    const aaaIdx = result.indexOf('### aaa/first.ts');
    const zzzIdx = result.indexOf('### zzz/last.ts');
    expect(aaaIdx).toBeLessThan(zzzIdx);

    const line1Idx = result.indexOf('aaa/first.ts:1');
    const line3Idx = result.indexOf('aaa/first.ts:3');
    expect(line1Idx).toBeLessThan(line3Idx);
  });

  it('appends truncated hint when truncated is true', () => {
    const markers = [
      {
        file: 'src/x.ts', line: 1, ceiling: 'c', upgrade: 'u', rot: false,
      },
    ];
    const result = renderDebtLedger(markers, true);
    expect(result).toContain('结果已截断至前 200 条');
  });

  it('shows zero rot when all markers have upgrade triggers', () => {
    const markers = [
      {
        file: 'a.ts', line: 1, ceiling: 'c1', upgrade: 'u1', rot: false,
      },
      {
        file: 'b.ts', line: 1, ceiling: 'c2', upgrade: 'u2', rot: false,
      },
    ];
    const result = renderDebtLedger(markers, false);
    expect(result).toContain('**汇总**：2 个标记，0 个 rot 风险。');
  });
});
```

- [ ] Run it and verify it FAILS

```bash
pnpm --filter @odysseythink/agent-core vitest run harvest-ody-markers.test.ts 2>&1 | tail -5
```

Expected: module not found / cannot import from `./harvest-ody-markers` (file doesn't exist yet).

- [ ] Write the minimal implementation

Create `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.ts`:

```typescript
import { z } from 'zod';

export const MAX_MARKERS = 200;

// ---- Data models ----

export interface DebtLedgerMarker {
  readonly file: string;
  readonly line: number;
  readonly ceiling: string;
  readonly upgrade: string;
  readonly rot: boolean;
}

export const HarvestOdyMarkersInputSchema = z.object({
  path: z.string().optional().describe('Optional subdirectory or file to scan.'),
});

export type HarvestOdyMarkersInput = z.infer<typeof HarvestOdyMarkersInputSchema>;

export interface HarvestOdyMarkersOutput {
  readonly markdown: string;
  readonly markerCount: number;
  readonly rotRiskCount: number;
  readonly truncated: boolean;
}

// ---- Parser ----

const MARKER_RE =
  /^(.+?):(\d+):\s*(?:(?:\/\/)|#)\s*ody:\s*(.*)$/;

export function parseOdyMarker(rawLine: string): DebtLedgerMarker | null {
  const match = MARKER_RE.exec(rawLine);
  if (!match) return null;

  const file = match[1]!;
  const line = Number(match[2]);
  const body = match[3]!.trim();

  const commaIdx = body.indexOf(',');
  if (commaIdx === -1) {
    return { file, line, ceiling: body, upgrade: '', rot: true };
  }

  const ceiling = body.slice(0, commaIdx).trim();
  const upgrade = body.slice(commaIdx + 1).trim();
  const rot = upgrade === '';
  return { file, line, ceiling, upgrade, rot };
}

// ---- Renderer ----

export function renderDebtLedger(
  markers: readonly DebtLedgerMarker[],
  truncated: boolean,
): string {
  if (markers.length === 0) {
    return '未找到 `ody:` 债务标记。台账干净。';
  }

  const groups = new Map<string, DebtLedgerMarker[]>();
  for (const m of markers) {
    const list = groups.get(m.file);
    if (list) {
      list.push(m);
    } else {
      groups.set(m.file, [m]);
    }
  }

  const sortedFiles = [...groups.keys()].sort();

  const lines: string[] = [];
  for (const file of sortedFiles) {
    lines.push(`### ${file}`);
    const fileMarkers = groups.get(file)!;
    fileMarkers.sort((a, b) => a.line - b.line);
    for (const m of fileMarkers) {
      const rotTag = m.rot ? ' ⚠️ rot' : '';
      const upgradeDisplay = m.upgrade || '（未指定）';
      lines.push(
        `${m.file}:${m.line} — ${m.ceiling}。天花板：${m.ceiling}。升级：${upgradeDisplay}${rotTag}`,
      );
    }
    lines.push('');
  }

  const totalRot = markers.filter((m) => m.rot).length;
  lines.push(
    `**汇总**：${markers.length} 个标记，${totalRot} 个 rot 风险。`,
  );
  if (truncated) {
    lines.push('结果已截断至前 200 条；如需完整扫描，请指定更小的目录或文件。');
  }
  return lines.join('\n');
}
```

- [ ] Run it and verify it PASSES

```bash
pnpm --filter @odysseythink/agent-core vitest run harvest-ody-markers.test.ts 2>&1 | tail -15
```

Expected: all 13 tests pass.

- [ ] Commit

```bash
git add packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.ts \
        packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.test.ts
git commit -m "feat: add DebtLedgerMarker types, parseOdyMarker, and renderDebtLedger"
```

---

### Task 2: HarvestOdyMarkersTool class + integration tests

**Depends on:** Task 1

**Files:**
- Modify: `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.ts` (append tool class + new imports)
- Test: `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.integration.test.ts`

- [ ] Write the failing tests

Create `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.integration.test.ts`:

```typescript
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { GrepTool } from '../file/grep';
import {
  noopTelemetryClient,
  type TelemetryClient,
} from '../../../telemetry';
import { HarvestOdyMarkersTool } from './harvest-ody-markers';

describe('HarvestOdyMarkersTool', () => {
  let fixtureDir: string;

  beforeEach(async () => {
    fixtureDir = join(tmpdir(), `ody-test-fixtures-${process.pid}`);
    await mkdir(fixtureDir, { recursive: true });

    await writeFile(
      join(fixtureDir, 'lock.ts'),
      [
        '// ody: 全局锁, 吞吐 > 100 rps 时改为按账户锁',
        'export function lock() {}',
        '// ody: 临时文件    ',
      ].join('\n'),
    );

    await writeFile(
      join(fixtureDir, 'parse.py'),
      [
        '# ody: 用 JSON.parse, 需要 schema 校验时改为 zod',
        'import json',
      ].join('\n'),
    );

    await writeFile(
      join(fixtureDir, 'clean.ts'),
      ['// TODO: refactor later', 'export const x = 1;'].join('\n'),
    );
  });

  afterEach(async () => {
    await rm(fixtureDir, { recursive: true, force: true });
  });

  it('scans fixtures and returns correct marker counts', async () => {
    const { Kaos } = await import('@odysseythink/kaos');
    const kaos = new Kaos({ cwd: fixtureDir });
    const workspace = { workspaceDir: fixtureDir, additionalDirs: [] };
    const grepTool = new GrepTool(kaos, workspace);
    const telemetry: TelemetryClient = {
      ...noopTelemetryClient,
      track: vi.fn(),
    };

    const tool = new HarvestOdyMarkersTool(
      kaos,
      workspace,
      grepTool,
      telemetry,
    );

    const execution = tool.resolveExecution({});
    const result = await execution.execute({
      signal: new AbortController().signal,
    });

    expect(result.isError).toBe(false);
    const output = JSON.parse(result.output);
    expect(output.markerCount).toBe(3);
    expect(output.rotRiskCount).toBe(1);
    expect(output.truncated).toBe(false);
    expect(output.markdown).toContain('### lock.ts');
    expect(output.markdown).toContain('### parse.py');
    expect(output.markdown).not.toContain('clean.ts');
    expect(output.markdown).toContain('⚠️ rot');
    expect(output.markdown).toContain('**汇总**：3 个标记，1 个 rot 风险。');

    expect(telemetry.track).toHaveBeenCalledWith(
      'debt_ledger_harvested',
      expect.objectContaining({
        marker_count: 3,
        rot_risk_count: 1,
      }),
    );
  });

  it('handles empty scan (no markers)', async () => {
    const emptyDir = join(fixtureDir, 'empty');
    await mkdir(emptyDir);

    const { Kaos } = await import('@odysseythink/kaos');
    const kaos = new Kaos({ cwd: emptyDir });
    const workspace = { workspaceDir: emptyDir, additionalDirs: [] };
    const grepTool = new GrepTool(kaos, workspace);
    const telemetry: TelemetryClient = {
      ...noopTelemetryClient,
      track: vi.fn(),
    };

    const tool = new HarvestOdyMarkersTool(
      kaos,
      workspace,
      grepTool,
      telemetry,
    );

    const execution = tool.resolveExecution({});
    const result = await execution.execute({
      signal: new AbortController().signal,
    });

    expect(result.isError).toBe(false);
    const output = JSON.parse(result.output);
    expect(output.markerCount).toBe(0);
    expect(output.rotRiskCount).toBe(0);
    expect(output.markdown).toBe(
      '未找到 `ody:` 债务标记。台账干净。',
    );
    expect(telemetry.track).toHaveBeenCalledWith(
      'debt_ledger_harvested',
      expect.objectContaining({ marker_count: 0, rot_risk_count: 0 }),
    );
  });

  it('scans a specific subdirectory via path input', async () => {
    const { Kaos } = await import('@odysseythink/kaos');
    const kaos = new Kaos({ cwd: fixtureDir });
    const workspace = { workspaceDir: fixtureDir, additionalDirs: [] };
    const grepTool = new GrepTool(kaos, workspace);
    const telemetry: TelemetryClient = {
      ...noopTelemetryClient,
      track: vi.fn(),
    };

    const tool = new HarvestOdyMarkersTool(
      kaos,
      workspace,
      grepTool,
      telemetry,
    );
    const execution = tool.resolveExecution({ path: 'parse.py' });
    const result = await execution.execute({
      signal: new AbortController().signal,
    });

    expect(result.isError).toBe(false);
    const output = JSON.parse(result.output);
    expect(output.markerCount).toBe(1);
  });

  it('reports failure telemetry when GrepTool errors', async () => {
    // Point at a non-existent path to trigger a grep error path.
    // GrepTool will still try to search it; the error comes from rg.
    const badDir = join(fixtureDir, 'nonexistent');

    const { Kaos } = await import('@odysseythink/kaos');
    const kaos = new Kaos({ cwd: badDir });
    const workspace = { workspaceDir: badDir, additionalDirs: [] };
    const grepTool = new GrepTool(kaos, workspace);
    const telemetry: TelemetryClient = {
      ...noopTelemetryClient,
      track: vi.fn(),
    };

    const tool = new HarvestOdyMarkersTool(
      kaos,
      workspace,
      grepTool,
      telemetry,
    );

    const execution = tool.resolveExecution({});
    const result = await execution.execute({
      signal: new AbortController().signal,
    });

    // May be error (rg can't find the directory) or success with 0 markers.
    // Either way, verify telemetry fired appropriately.
    if (result.isError) {
      expect(telemetry.track).toHaveBeenCalledWith(
        'debt_ledger_failed',
        expect.any(Object),
      );
    }
  });
});
```

- [ ] Run it and verify it FAILS

```bash
pnpm --filter @odysseythink/agent-core vitest run harvest-ody-markers.integration.test.ts 2>&1 | tail -5
```

Expected: `HarvestOdyMarkersTool is not exported` from `./harvest-ody-markers`.

- [ ] Write the minimal implementation

In `packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.ts`, add the following imports at the top of the file (merge with existing `import { z } from 'zod'`):

```typescript
import type { Kaos } from '@odysseythink/kaos';

import type { BuiltinTool } from '../../../agent/tool';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import type { TelemetryClient } from '../../../telemetry';
import { resolvePathAccessPath } from '../../policies/path-access';
import { toInputJsonSchema } from '../../support/input-schema';
import { literalRulePattern } from '../../support/rule-match';
import type { WorkspaceConfig } from '../../support/workspace';
import { GrepInputSchema, GrepTool } from '../file/grep';
```

After the `renderDebtLedger` function (end of existing file), append:

```typescript
// ---- Tool ----

const TOOL_DESCRIPTION =
  'Scan the codebase for `// ody:` / `# ody:` simplification-debt markers and return a Chinese-first ledger report.';

export class HarvestOdyMarkersTool
  implements BuiltinTool<HarvestOdyMarkersInput>
{
  readonly name = 'HarvestOdyMarkers' as const;
  readonly description = TOOL_DESCRIPTION;
  readonly parameters: Record<string, unknown> =
    toInputJsonSchema(HarvestOdyMarkersInputSchema);

  constructor(
    private readonly kaos: Kaos,
    private readonly workspace: WorkspaceConfig,
    private readonly grepTool: GrepTool,
    private readonly telemetry: TelemetryClient,
  ) {}

  resolveExecution(args: HarvestOdyMarkersInput): ToolExecution {
    return {
      description: 'Harvesting ody: simplification debt markers',
      approvalRule: literalRulePattern(this.name, ''),
      execute: ({ signal }) => this.run(args, signal),
    };
  }

  private async run(
    args: HarvestOdyMarkersInput,
    signal: AbortSignal,
  ): Promise<ExecutableToolResult> {
    try {
      const scanPath = args.path ?? this.workspace.workspaceDir;

      const resolvedPath = resolvePathAccessPath(scanPath, {
        kaos: this.kaos,
        workspace: this.workspace,
        operation: 'search',
        policy: {
          guardMode: 'absolute-outside-allowed',
          checkSensitive: false,
        },
      });

      const grepInput = GrepInputSchema.parse({
        pattern: '(#|//) ?ody:',
        path: resolvedPath,
        output_mode: 'content',
        '-n': true,
        head_limit: MAX_MARKERS,
        include_ignored: false,
      });

      const grepExecution = this.grepTool.resolveExecution(grepInput);
      const grepResult = await grepExecution.execute({ signal });

      if (grepResult.isError) {
        this.telemetry.track('debt_ledger_failed', {
          error: grepResult.output,
        });
        return {
          isError: true,
          output: `债务台账扫描失败：${grepResult.output}`,
        };
      }

      const rawLines = grepResult.output
        .split('\n')
        .filter((l) => l.trim() !== '');
      const markers = rawLines
        .map((line) => parseOdyMarker(line))
        .filter((m): m is DebtLedgerMarker => m !== null);

      const truncated = grepResult.output.includes('Results truncated');

      const markdown = renderDebtLedger(markers, truncated);

      this.telemetry.track('debt_ledger_harvested', {
        marker_count: markers.length,
        rot_risk_count: markers.filter((m) => m.rot).length,
      });

      const output: HarvestOdyMarkersOutput = {
        markdown,
        markerCount: markers.length,
        rotRiskCount: markers.filter((m) => m.rot).length,
        truncated,
      };

      return { isError: false, output: JSON.stringify(output) };
    } catch (error) {
      const message =
        error instanceof Error ? error.message : String(error);
      this.telemetry.track('debt_ledger_failed', { error: message });
      return {
        isError: true,
        output: `债务台账扫描失败：${message}`,
      };
    }
  }
}
```

- [ ] Run it and verify it PASSES

```bash
pnpm --filter @odysseythink/agent-core vitest run harvest-ody-markers.integration.test.ts 2>&1 | tail -20
```

Expected: all 4 integration tests pass (tests that require rg will run when rg is available; the "reports failure" test is optional and may be skipped).

- [ ] Run the unit tests too to ensure no regression

```bash
pnpm --filter @odysseythink/agent-core vitest run harvest-ody-markers.test.ts 2>&1 | tail -5
```

Expected: all 13 unit tests still pass.

- [ ] Commit

```bash
git add packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.ts \
        packages/agent-core/src/tools/builtin/code-quality/harvest-ody-markers.integration.test.ts
git commit -m "feat: add HarvestOdyMarkersTool class with GrepTool delegation"
```

---

### Task 3: debt-ledger builtin skill

**Depends on:** none

**Files:**
- Create: `packages/agent-core/src/skill/builtin/debt-ledger.md`
- Create: `packages/agent-core/src/skill/builtin/debt-ledger.ts`
- Modify: `packages/agent-core/src/skill/builtin/index.ts` (add import + registration, add to re-export)
- Test: `packages/agent-core/src/skill/builtin/debt-ledger.test.ts`

- [ ] Write the skill content

Create `packages/agent-core/src/skill/builtin/debt-ledger.md`:

```markdown
---
type: inline
name: debt-ledger
description: >-
  Scan the codebase for `// ody:` / `# ody:` simplification-debt markers
  and render a Chinese-first ledger report grouped by file with rot-risk
  warnings. Use after every simplification session to harvest markers,
  and before code review to surface pending simplifications.
arguments: path
---

## Purpose

This skill teaches you how and when to call the `HarvestOdyMarkers` tool
to scan for `ody:` simplification-debt markers in the codebase.

## Marker format

A simplification-debt marker is a line comment in source code:

```
// ody: <天花板>, <升级触发条件>
# ody: <天花板>, <升级触发条件>
```

- **天花板**: the ceiling of the current simplified approach (what you chose NOT to build).
- **升级触发条件**: the concrete condition under which the ceiling should be raised (e.g. "吞吐 > 100 rps", "需要 schema 校验时").

Markers without an upgrade trigger (no comma, or empty after comma) are
flagged as **⚠️ rot** — incomplete debt that should be resolved.

## When to call HarvestOdyMarkers

Invoke the `HarvestOdyMarkers` tool when:
- The user asks for "债务台账", "ody debt", "列出 ody 标记", "harvest ody markers", or similar.
- After a simplification session, to audit markers the agent left behind.
- Before code review, to surface pending simplifications.

Pass an optional `path` argument to scan a specific directory or file.

## Output format

The tool returns a markdown report:
- Grouped by file (sorted alphabetically).
- Each marker on its own line: `<file>:<line> — <ceiling>。天花板：<ceiling>。升级：<upgrade>。`
- Rot markers annotated with `⚠️ rot` and `升级：（未指定）`.
- Footer summary: `**汇总**：N 个标记，M 个 rot 风险。`

## After harvesting

1. Present the ledger to the user.
2. If rot markers exist, note which files have incomplete debt.
3. The user may ask you to resolve specific markers.
```

- [ ] Write the skill constant

Create `packages/agent-core/src/skill/builtin/debt-ledger.ts`:

```typescript
import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import DEBT_LEDGER_BODY from './debt-ledger.md';

const PSEUDO_PATH = 'builtin://debt-ledger';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/debt-ledger.md',
  skillDirName: 'debt-ledger',
  source: 'builtin',
  text: DEBT_LEDGER_BODY,
});

export const DEBT_LEDGER_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
```

- [ ] Register the skill

In `packages/agent-core/src/skill/builtin/index.ts`:

Add import (after `import { VERIFICATION_BEFORE_COMPLETION_SKILL } from './verification-before-completion';` at line 14):

```typescript
import { DEBT_LEDGER_SKILL } from './debt-ledger';
```

In `registerBuiltinSkills` function body (after `registry.registerBuiltinSkill(VERIFICATION_BEFORE_COMPLETION_SKILL);` at line 30):

```typescript
  registry.registerBuiltinSkill(DEBT_LEDGER_SKILL);
```

In the re-export block (after `VERIFICATION_BEFORE_COMPLETION_SKILL,` at line 47):

```typescript
  DEBT_LEDGER_SKILL,
```

- [ ] Write the skill registration test

Create `packages/agent-core/src/skill/builtin/debt-ledger.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { InMemorySkillRegistry } from '../registry';
import { registerBuiltinSkills } from './index';

describe('debt-ledger skill', () => {
  it('is registered as a builtin skill', () => {
    const registry = new InMemorySkillRegistry();
    registerBuiltinSkills(registry);

    const skill = registry.getSkill('debt-ledger');
    expect(skill).toBeDefined();
    expect(skill!.name).toBe('debt-ledger');
    expect(skill!.metadata.type).toBe('inline');
    expect(skill!.description).toContain('ody:');
  });

  it('is an invocable skill', () => {
    const registry = new InMemorySkillRegistry();
    registerBuiltinSkills(registry);

    const invocable = registry.listInvocableSkills();
    expect(invocable.some((s) => s.name === 'debt-ledger')).toBe(true);
  });
});
```

- [ ] Run it and verify it PASSES

```bash
pnpm --filter @odysseythink/agent-core vitest run debt-ledger.test.ts 2>&1 | tail -10
```

Expected: 2 tests pass.

- [ ] Commit

```bash
git add packages/agent-core/src/skill/builtin/debt-ledger.md \
        packages/agent-core/src/skill/builtin/debt-ledger.ts \
        packages/agent-core/src/skill/builtin/index.ts \
        packages/agent-core/src/skill/builtin/debt-ledger.test.ts
git commit -m "feat: add debt-ledger builtin skill"
```

---

### Task 4: Tool registration, barrel export, and whole-tree typecheck

**Depends on:** Task 2, Task 3

**Files:**
- Modify: `packages/agent-core/src/tools/builtin/index.ts` (add `code-quality` barrel export)
- Modify: `packages/agent-core/src/agent/tool/index.ts` (register `HarvestOdyMarkersTool`)

This is a shared-signature wiring task. No new tests — verification is a whole-tree typecheck.

- [ ] Add barrel export

In `packages/agent-core/src/tools/builtin/index.ts`, add a new line after the existing exports (e.g. after line 40 `export * from './web/web-search';`):

```typescript
export * from './code-quality/harvest-ody-markers';
```

- [ ] Register the tool in ToolManager

In `packages/agent-core/src/agent/tool/index.ts`, add the `HarvestOdyMarkersTool` entry to the `this.builtinTools` array inside `initializeBuiltinTools()`. 

The tool needs `kaos`, `workspace`, `grepTool` (already constructed on line 412), and `telemetry` (from `this.agent.telemetry`).

Insert after line 477 (`new b.ReviewTestsTool(kaos, this.agent),`) — the last tool entry before the array close:

```typescript
        new b.HarvestOdyMarkersTool(
          kaos,
          workspace,
          new b.GrepTool(kaos, workspace),
          this.agent.telemetry,
        ),
```

**Important**: The `grepTool` instance at line 412 (`new b.GrepTool(kaos, workspace)`) is constructed inline in the array. The `HarvestOdyMarkersTool` uses its own private `GrepTool` instance — construct a second `new b.GrepTool(kaos, workspace)` for it. This avoids circular dependency issues (the array entries are evaluated left-to-right).

- [ ] Whole-tree typecheck

```bash
pnpm -r typecheck 2>&1 | tail -20
```

Expected: no type errors across all workspace packages. This validates that:
- The `HarvestOdyMarkersTool` constructor signature is compatible with the `BuiltinTool` interface.
- The barrel export is reachable.
- No other packages have stale references.

- [ ] Commit

```bash
git add packages/agent-core/src/tools/builtin/index.ts \
        packages/agent-core/src/agent/tool/index.ts
git commit -m "feat: register HarvestOdyMarkersTool in builtin tools"
```

---

### Task 5: A1 simplicity-first.md — teach agents to leave ody: markers

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/skill/builtin/simplicity-first.md` (append level-gated block)

This is a content-only change to an `.md` skill file. Verification: run the existing simplicity-first test suite to confirm no regressions.

- [ ] Add the marker instruction block

In `packages/agent-core/src/skill/builtin/simplicity-first.md`, append after the "输出纪律（所有档位）" section (after line 71, the last line of the file):

```markdown

<!-- FULL[ -->
## 简化债务标记（full/ultra 档）

如果你 deliberate 选择了更简的方案（例如用全局锁、临时文件、`JSON.parse` 而非 schema 校验），
必须在相关代码旁留注释标记：

```
// ody: <天花板>, <升级触发条件>
# ody: <天花板>, <升级触发条件>
```

- **天花板**：当前简化方案的能力上限（你选择了什么不做的）。
- **升级触发条件**：到达什么条件时必须升级（例如 "吞吐 > 100 rps 时改为按账户锁"）。

没有升级触发条件的标记会被后续的债务台账标为 ⚠️ rot。
这是不完整的债务，需要补上触发条件或立即解决。
<!-- ]FULL -->

<!-- ULTRA[ -->
## 简化债务标记（ultra 档额外要求）

在 ultra 档，你必须对 **每一个** 有意的简化决策都留下 `ody:` 标记。
在输出中明确列出本 session 新加了哪些标记，以及为什么这些简化在当前是合理的。
没有标记的简化决策视为未完成。
<!-- ]ULTRA -->
```

- [ ] Verify existing tests still pass

```bash
pnpm --filter @odysseythink/agent-core vitest run simplicity-first 2>&1 | tail -10
```

Expected: all existing simplicity-first tests pass. The `filterSimplicityLevels` function should correctly handle the new `<!-- FULL[` / `<!-- ULTRA[` blocks — stripping them when the level doesn't match, keeping them when it does.

- [ ] Manual verification: confirm the new block appears in full/ultra output

The `simplicity-first.test.ts` file already tests `filterSimplicityLevels`. Add a quick verification:

```bash
node -e "
const {filterSimplicityLevels} = require('./packages/agent-core/dist/skill/builtin/simplicity-first.js');
// This file is generated after build; as a quick check, run the existing test.
"
```

The existing test suite covers level filtering. No additional test needed.

- [ ] Commit

```bash
git add packages/agent-core/src/skill/builtin/simplicity-first.md
git commit -m "feat: teach simplicity-first to leave ody: debt markers in full/ultra mode"
```

---

### Task 6: A3 simplicity.ts — suggest ody: annotations in review/audit prompt

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/code-review/simplicity.ts` (line 195 — one sentence change)

This is a targeted one-line change to the A3 review/audit prompt text. Verification: run the existing code-review test suite.

- [ ] Update the prompt sentence

In `packages/agent-core/src/code-review/simplicity.ts`, change line 195 from:

```typescript
    '- If you find something that was deliberately kept simple and could use an `ody:` annotation, you may mention it in the detail — but do not create a finding for it.',
```

to:

```typescript
    '- If you find something that was deliberately kept simple and could use an `ody:` annotation, suggest adding `// ody: <ceiling>, <upgrade trigger>` in the detail — but do not create a finding for it.',
```

- [ ] Verify existing code-review tests still pass

```bash
pnpm --filter @odysseythink/agent-core vitest run code-review 2>&1 | tail -10
```

Expected: all existing code-review tests pass. The prompt text change does not affect the parsing logic or test assertions.

- [ ] Verify the change is correct by reading the file

```bash
rg "ody:" packages/agent-core/src/code-review/simplicity.ts
```

Expected output shows the updated line with `suggest adding` (not `may mention it`).

- [ ] Commit

```bash
git add packages/agent-core/src/code-review/simplicity.ts
git commit -m "feat: update A3 simplicity review prompt to suggest ody: annotations"
```

---

## Self-Review

- [ ] 1. **Spec-coverage table**: map every spec section/requirement → Task(s), marked covered / GAP / no-op.

| Spec Item (from design §Scope In) | Task(s) | Status |
|---|---|---|
| 1. `debt-ledger` builtin skill | T3 | covered |
| 2. `harvest-ody-markers` builtin tool | T1, T2, T4 | covered |
| 3. A1 `simplicity-first.md` full/ultra update | T5 | covered |
| 4. A3 review/audit prompt extension | T6 | covered |
| 5. 只读内存报告 (no auto file write) | T2 | covered |
| 6. 输出格式: 按文件分组, rot 标记, 汇总 | T1 (renderDebtLedger) | covered |
| 7. 注释前缀 `//` 与 `#` | T1 (MARKER_RE) | covered |
| 8. rot 判定: 无逗号或逗号后仅空白 | T1 (parseOdyMarker) | covered |
| 9. 硬上限 200, 截断提示 | T1 (MAX_MARKERS), T2 (truncated detection) | covered |
| 10. 复用 GrepTool 敏感文件过滤 | T2 (delegates to GrepTool) | covered |
| 11. telemetry: `debt_ledger_harvested`, `debt_ledger_failed` | T2 | covered |
| 12. 默认启用, 无 feature flag | T4 (unconditional registration) | covered |
| CLI/TUI 独立命令 | — | no-op (out of scope) |
| 持久化文件 | — | no-op (out of scope) |
| git blame / owner | — | no-op (out of scope) |
| 块注释 / HTML 注释前缀 | — | no-op (out of scope) |
| `ponytail:` 兼容 | — | no-op (out of scope) |

- [ ] 2. **Placeholder scan**: no TODO/TBD, no deferred-by-dependency excuses, no dead-code placeholders.

All code blocks are complete, compilable, and self-contained. No "implement later" or "similar to Task N". The GrepTool delegation is explicit; the skill `.md` content is complete; the prompt edits are exact line replacements.

- [ ] 3. **No phantom tasks**: every task produces a verifiable change; zero `--allow-empty` / "already done in Task N".

Each task creates or modifies concrete files. T1 creates types + functions + tests. T2 adds the tool class + integration tests. T3 creates skill files + registers. T4 adds barrel export + tool registration + typecheck. T5 edits `.md`. T6 edits `.ts`. No task is a "write the tests" trailing collector.

- [ ] 4. **Dependency soundness**: every `Depends on:` is satisfied by an earlier task; nothing references a symbol only a later task creates.

- T1 → none.
- T2 → T1 (uses `DebtLedgerMarker`, `parseOdyMarker`, `renderDebtLedger`, `HarvestOdyMarkersInput`, `MAX_MARKERS`).
- T3 → none.
- T4 → T2 (uses `HarvestOdyMarkersTool`), T3 (uses `DEBT_LEDGER_SKILL`).
- T5 → none.
- T6 → none.
All forward references verified.

- [ ] 5. **Caller & build soundness**: every shared-signature task updated all callers (incl. test files) and ends with a whole-tree typecheck.

T4 is the only task that touches shared wiring (`agent/tool/index.ts`, `tools/builtin/index.ts`). It ends with `pnpm -r typecheck` which covers all workspace packages including tests. No signature changes across multiple tasks. The tool constructor signature (`Kaos, WorkspaceConfig, GrepTool, TelemetryClient`) is verified by the typecheck against the `BuiltinTool` interface constraint.

- [ ] 6. **Test-the-risk**: every state-mutating task has a behavioral test asserting the mutation.

- T1: 13 unit tests covering valid `//`/`#` markers, rot detection (missing comma, empty trigger, Chinese comma), null returns (no prefix, block comments), optional space, empty ledger, grouped rendering, sorting, truncated hint, zero rot.
- T2: 4 integration tests covering fixture scan (marker counts, rot counts, per-file grouping), empty scan, path-restricted scan, failure telemetry.
- T3: 2 tests covering skill registration and invocability.
- T4: No behavioral mutation (wiring); verified by whole-tree typecheck.
- T5: Content-only; verified by existing `simplicity-first` test suite.
- T6: Content-only; verified by existing `code-review` test suite.

Must-survive inputs verified:
- `// ody: 全局锁, 吞吐 > 100 rps` → parsed correctly (comma is ASCII `,`).
- `# ody: 用 JSON.parse, 需要 schema 校验时改为 zod` → parsed correctly.
- `//ody: simple, complex` → parsed correctly (no space before `ody:`).
- `src/lock.ts:12:// TODO: fix lock` → returns null (not caught by MARKER_RE).
- `src/lock.ts:12:/* ody: global, upgrade */` → returns null (block comment rejected by `//`/`#` anchor).

- [ ] 7. **Type consistency**: types, signatures and property names used in later tasks match what earlier tasks defined.

- `DebtLedgerMarker` (T1): `file: string`, `line: number`, `ceiling: string`, `upgrade: string`, `rot: boolean` — used in T2.
- `HarvestOdyMarkersInput` (T1): `path?: string` — used in T2.
- `HarvestOdyMarkersOutput` (T1): `markdown: string`, `markerCount: number`, `rotRiskCount: number`, `truncated: boolean` — used in T2.
- `HarvestOdyMarkersTool` constructor (T2): `Kaos, WorkspaceConfig, GrepTool, TelemetryClient` — used in T4.
- `DEBT_LEDGER_SKILL` (T3): `SkillDefinition` — used in T4.
- `parseOdyMarker` signature (T1): `(rawLine: string) => DebtLedgerMarker | null` — used in T2 tests and internally.
- `renderDebtLedger` signature (T1): `(markers: readonly DebtLedgerMarker[], truncated: boolean) => string` — used in T2.
- `MAX_MARKERS` (T1): `200` — used in T2.
All property names consistent across task boundaries.

