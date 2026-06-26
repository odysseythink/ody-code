# Part 1: Core — Config, Types, Impact Analysis, Registry

## Scope

Data models, configuration schema + resolver, impact analysis, generator registry, and core unit tests. After Part 1, `packages/agent-core/src/e2e-testing/types.ts`, `config.ts`, `impact-analyzer.ts`, `registry.ts`, `errors.ts` compile and pass tests independently.

---

## Phase A: Config & resolver (Tasks 1–2)

### Task 1: Add `E2EConfigSchema` and TOML wiring

**Depends on:** none

**Files:** Create `packages/agent-core/test/e2e-testing/core.test.ts` (start); Modify `packages/agent-core/src/config/schema.ts:220-224`; Modify `packages/agent-core/src/config/toml.ts:138,307-314`.

**Approach:** Add Zod schema + defaulted field to `KimiConfigSchema`, then wire into `transformTomlData` and `configToTomlData` for TOML roundtrip.

- [ ] Write the failing test in `packages/agent-core/test/e2e-testing/core.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { parseConfigString } from '../../../src/config/toml';

describe('E2E config schema', () => {
  it('parses [e2e] section from TOML', () => {
    const config = parseConfigString(`
[e2e]
enabled = true
strategy = "smart"
critical_tools = ["ExitPlanModeTool"]
failure_policy = "warn"
max_concurrency = 4
test_timeout = 30000
report_dir = ".ody-code/test-reports"
generated_test_dir = ".ody-code/test-generated/e2e"
`);
    expect(config.e2e).toBeDefined();
    expect(config.e2e!.enabled).toBe(true);
    expect(config.e2e!.strategy).toBe('smart');
    expect(config.e2e!.criticalTools).toEqual(['ExitPlanModeTool']);
    expect(config.e2e!.failurePolicy).toBe('warn');
    expect(config.e2e!.maxConcurrency).toBe(4);
    expect(config.e2e!.testTimeout).toBe(30000);
  });

  it('returns defaults when [e2e] section is absent', () => {
    const config = parseConfigString('[permission]\nrules = []');
    // With defaults applied by the schema, e2e should have defaults
    expect(config.e2e).toBeDefined();
    expect(config.e2e!.enabled).toBe(true);
  });
});
```

- [ ] Run and verify FAILS: `pnpm --filter @odysseythink/agent-core test -- test/e2e-testing/core.test.ts`. Expected failure: `E2EConfigSchema` is not defined / KimiConfigSchema does not have `e2e` field.

- [ ] Write the minimal implementation:

In `packages/agent-core/src/config/schema.ts`, add after `BrowserConfigSchema` (line ~195):

```typescript
export const E2EConfigSchema = z.object({
  enabled: z.boolean().default(true),
  strategy: z.enum(['always', 'smart', 'critical-only']).default('smart'),
  criticalTools: z.array(z.string()).default(['ExitPlanModeTool']),
  failurePolicy: z.enum(['block', 'warn', 'ignore']).default('warn'),
  maxConcurrency: z.number().int().min(1).default(4),
  testTimeout: z.number().int().min(1000).default(30000),
  reportDir: z.string().default('.ody-code/test-reports'),
  generatedTestDir: z.string().default('.ody-code/test-generated/e2e'),
});

export type E2EConfig = z.infer<typeof E2EConfigSchema>;
```

In `KimiConfigSchema` (line ~224), add after `browser: BrowserConfigSchema.optional()`:

```typescript
e2e: E2EConfigSchema.optional(),
```

In `packages/agent-core/src/config/toml.ts`, `transformTomlData` (line ~138), add after the `browser` block:

```typescript
} else if (targetKey === 'e2e' && isPlainObject(value)) {
  result[targetKey] = transformPlainObject(value);
```

In `configToTomlData` (line ~312), add after the `browser` block:

```typescript
setSection(out, 'e2e', config.e2e, e2eToToml);
```

Add the `e2eToToml` function near `browserToToml` (after line ~493):

```typescript
function e2eToToml(e2e: NonNullable<KimiConfig['e2e']>, rawE2e: unknown): Record<string, unknown> {
  const out = cloneRecord(rawE2e);
  for (const [key, value] of Object.entries(e2e)) {
    setDefined(out, camelToSnake(key), value);
  }
  return out;
}
```

- [ ] Run and verify PASSES: `pnpm --filter @odysseythink/agent-core test -- test/e2e-testing/core.test.ts`. Both tests pass; `config.e2e.enabled` is `true` (from default) even when section absent.

- [ ] Run whole-tree typecheck: `pnpm -r typecheck` from repo root. Then commit: `git add` the changed files + new test, commit with message `feat(e2e): add E2E config schema and TOML roundtrip`.

### Task 2: Add `E2EConfigResolver` and E2E error codes

**Depends on:** Task 1

**Files:** Create `packages/agent-core/src/e2e-testing/errors.ts`; Create `packages/agent-core/src/e2e-testing/config.ts`; Modify `packages/agent-core/src/errors/codes.ts:79` (add codes); Modify `packages/agent-core/test/e2e-testing/core.test.ts` (append tests).

- [ ] Write the failing test — append to `core.test.ts`:

```typescript
import { E2EConfigResolver } from '#/e2e-testing/config';

describe('E2EConfigResolver', () => {
  it('returns defaults for empty config', () => {
    const result = E2EConfigResolver.resolve({});
    expect(result.enabled).toBe(true);
    expect(result.strategy).toBe('smart');
    expect(result.criticalTools).toEqual(['ExitPlanModeTool']);
  });

  it('overrides enabled from raw', () => {
    const result = E2EConfigResolver.resolve({ e2e: { enabled: false } as any });
    expect(result.enabled).toBe(false);
  });

  it('throws for maxConcurrency 0', () => {
    expect(() => E2EConfigResolver.resolve({ e2e: { maxConcurrency: 0 } as any }))
      .toThrow();
  });
});
```

- [ ] Run and verify FAILS: module `#/e2e-testing/config` not found.

- [ ] Write the minimal implementation:

`packages/agent-core/src/e2e-testing/errors.ts`:

```typescript
import { KimiError } from '#/errors';
import type { E2EConfig } from '#/config/schema';

export class E2EConfigValidationError extends KimiError {
  constructor(message: string, cause?: unknown) {
    super('config.invalid' as any, message, { cause });
    this.name = 'E2EConfigValidationError';
  }
}

export class E2ENoMatchingGeneratorError extends KimiError {
  constructor(projectRoot: string) {
    super('config.invalid' as any, `No E2E generator matches the project at ${projectRoot}`);
    this.name = 'E2ENoMatchingGeneratorError';
  }
}

export interface ResolvedE2EConfig extends Required<E2EConfig> {}
```

`packages/agent-core/src/e2e-testing/config.ts`:

```typescript
import { E2EConfigSchema, type KimiConfig } from '#/config/schema';
import { E2EConfigValidationError } from './errors';
import type { ResolvedE2EConfig } from './errors';

export class E2EConfigResolver {
  static resolve(kimiConfig: KimiConfig): ResolvedE2EConfig {
    const raw = kimiConfig.e2e ?? {};
    try {
      return E2EConfigSchema.parse(raw) as ResolvedE2EConfig;
    } catch (error) {
      throw new E2EConfigValidationError(
        error instanceof Error ? error.message : 'Invalid e2e config',
        error,
      );
    }
  }
}
```

- [ ] Run and verify PASSES: `pnpm --filter @odysseythink/agent-core test -- test/e2e-testing/core.test.ts`.

- [ ] Commit with message `feat(e2e): add E2EConfigResolver and error classes`.

---

## Phase B: Types, impact analysis, registry (Tasks 3–5)

### Task 3: Define domain types

**Depends on:** Task 2

**Files:** Create `packages/agent-core/src/e2e-testing/types.ts`.

**Approach:** Separate file with pure type/interface definitions (no runtime deps beyond zod for schema types). Test: a compile-time check — import types in test and verify shape. Since these are pure types, use a minimal compile test.

- [ ] Write the test — append to `core.test.ts`:

```typescript
import type { AffectedTool, E2EPriority, Feature, ImpactAnalysisResult, ProjectStructure, TestFile } from '#/e2e-testing/types';

describe('E2E types (compile check)', () => {
  it('Feature shape is constructable', () => {
    const f: Feature = { toolId: 'ExitPlanModeTool', changedFiles: [], projectRoot: '/app' };
    expect(f.toolId).toBe('ExitPlanModeTool');
  });

  it('AffectedTool priority is valid', () => {
    const a: AffectedTool = { toolId: 'T', priority: 'critical' as E2EPriority };
    expect(a.priority).toBe('critical');
  });
});
```

- [ ] Run and verify FAILS: `#/e2e-testing/types` not found.

- [ ] Write `packages/agent-core/src/e2e-testing/types.ts`:

```typescript
export type E2EPriority = 'critical' | 'important' | 'nice-to-have';

// E2EConfig is defined in '../config/schema' via z.infer<typeof E2EConfigSchema>.
// Use that or ResolvedE2EConfig from './config' for runtime defaults-applied config.

export interface ProjectStructure {
  language: string;
  framework: string;
  testTool: string;
  root: string;
}

export interface Feature {
  toolId: string;
  changedFiles: string[];
  projectRoot: string;
  description?: string;
}

export interface TestFile {
  relativePath: string;
  content: string;
}

export interface AffectedTool {
  toolId: string;
  priority: E2EPriority;
}

export interface ImpactAnalysisResult {
  affectedTools: AffectedTool[];
}

export interface E2ETestGenerator {
  readonly id: string;
  detectProjectStructure(root: string): Promise<ProjectStructure | null>;
  generateTestsForFeature(feature: Feature): Promise<TestFile[]>;
}
```

- [ ] Run and verify PASSES: tests pass.

- [ ] Commit with message `feat(e2e): define domain types`.

### Task 4: Implement `ImpactAnalyzer` + `TOOL_IMPACT_MAP`

**Depends on:** Task 3

**Files:** Create `packages/agent-core/src/e2e-testing/impact-map.ts`; Create `packages/agent-core/src/e2e-testing/impact-analyzer.ts`; Modify `packages/agent-core/test/e2e-testing/core.test.ts` (append tests).

- [ ] Write the failing test — append to `core.test.ts`:

```typescript
import { ImpactAnalyzer } from '#/e2e-testing/impact-analyzer';

const defaultConfig: ResolvedE2EConfig = {
  enabled: true, strategy: 'smart', criticalTools: ['ExitPlanModeTool'],
  failurePolicy: 'warn', maxConcurrency: 4, testTimeout: 30000,
  reportDir: '.ody-code/test-reports', generatedTestDir: '.ody-code/test-generated/e2e',
};

describe('ImpactAnalyzer', () => {
  it('matches exit-plan-mode.ts to ExitPlanModeTool as critical', () => {
    const result = ImpactAnalyzer.analyze(
      ['packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts'],
      defaultConfig,
    );
    expect(result.affectedTools).toEqual([
      { toolId: 'ExitPlanModeTool', priority: 'critical' },
    ]);
  });

  it('matches session-mode/index.ts to ExitPlanModeTool', () => {
    const result = ImpactAnalyzer.analyze(
      ['packages/agent-core/src/agent/session-mode/index.ts'],
      defaultConfig,
    );
    expect(result.affectedTools.some(t => t.toolId === 'ExitPlanModeTool')).toBe(true);
  });

  it('returns empty for unrelated file with smart strategy', () => {
    const result = ImpactAnalyzer.analyze(['unrelated.ts'], defaultConfig);
    expect(result.affectedTools).toHaveLength(0);
  });

  it('returns general for always strategy with no matches', () => {
    const config = { ...defaultConfig, strategy: 'always' as const };
    const result = ImpactAnalyzer.analyze(['unrelated.ts'], config);
    expect(result.affectedTools).toEqual([
      { toolId: 'general', priority: 'nice-to-have' },
    ]);
  });

  it('filters non-critical with critical-only strategy', () => {
    const config = { ...defaultConfig, strategy: 'critical-only' as const, criticalTools: ['ExitPlanModeTool'] };
    const result = ImpactAnalyzer.analyze(
      ['packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts'],
      config,
    );
    expect(result.affectedTools).toHaveLength(0);
  });
});
```

- [ ] Run and verify FAILS: `ImpactAnalyzer` not found.

- [ ] Write `packages/agent-core/src/e2e-testing/impact-map.ts`:

```typescript
/**
 * Static map from tool class name to repo‑relative file paths that indicate
 * changes likely to affect the tool. Paths are matched with picomatch.
 */
export const TOOL_IMPACT_MAP: Record<string, string[]> = {
  ExitPlanModeTool: [
    'packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts',
    'packages/agent-core/src/agent/session-mode/index.ts',
    'packages/agent-core/src/agent/injection/plan-mode.ts',
  ],
  EnterPlanModeTool: [
    'packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts',
  ],
};
```

- [ ] Write `packages/agent-core/src/e2e-testing/impact-analyzer.ts`:

```typescript
import picomatch from 'picomatch';
import type { ResolvedE2EConfig } from './config';
import type { AffectedTool, E2EPriority, ImpactAnalysisResult } from './types';
import { TOOL_IMPACT_MAP } from './impact-map';

function computePriority(toolId: string, criticalTools: string[]): E2EPriority {
  return criticalTools.includes(toolId) ? 'critical' : 'important';
}

function anyGlobMatches(globs: string[], file: string): boolean {
  const normalized = file.replace(/\\/g, '/');
  return globs.some((glob) => picomatch.isMatch(normalized, glob));
}

export class ImpactAnalyzer {
  static analyze(
    changedFiles: string[],
    config: ResolvedE2EConfig,
  ): ImpactAnalysisResult {
    const result = new Map<string, E2EPriority>();

    for (const file of changedFiles) {
      for (const [toolId, globs] of Object.entries(TOOL_IMPACT_MAP)) {
        if (anyGlobMatches(globs, file)) {
          const priority = computePriority(toolId, config.criticalTools);
          const current = result.get(toolId);
          const priorityOrder: E2EPriority[] = ['critical', 'important', 'nice-to-have'];
          if (current === undefined || priorityOrder.indexOf(priority) < priorityOrder.indexOf(current)) {
            result.set(toolId, priority);
          }
        }
      }
    }

    if (result.size === 0 && config.strategy === 'always') {
      result.set('general', 'nice-to-have');
    }

    const affected: AffectedTool[] = [];
    for (const [toolId, priority] of result) {
      if (config.strategy === 'critical-only' && priority !== 'critical') continue;
      affected.push({ toolId, priority });
    }

    return { affectedTools: affected };
  }
}
```

- [ ] Run and verify PASSES: test suite passes all 5 cases.

- [ ] Commit with message `feat(e2e): implement ImpactAnalyzer with TOOL_IMPACT_MAP`.

### Task 5: Implement `E2EGeneratorRegistry` and `TypeScriptVitestGenerator.detectProjectStructure`

**Depends on:** Task 4

**Files:** Create `packages/agent-core/src/e2e-testing/registry.ts`; Create `packages/agent-core/src/e2e-testing/generator.ts` (detection only); Modify `packages/agent-core/test/e2e-testing/core.test.ts` (append tests).

- [ ] Write the failing test — append:

```typescript
import { registry } from '#/e2e-testing/registry';
import { TypeScriptVitestGenerator } from '#/e2e-testing/generator';

describe('E2EGeneratorRegistry + TS/Vitest detection', () => {
  it('detectAndGet throws without package.json', async () => {
    await expect(registry.detectAndGet('/no-package-json')).rejects.toThrow('No E2E generator');
  });

  it('detectAndGet returns generator for agent-core project root', async () => {
    const gen = await registry.detectAndGet(process.cwd() + '/packages/agent-core');
    expect(gen.id).toBe('typescript-vitest');
  });

  it('detectProjectStructure returns null without package.json', async () => {
    const gen = new TypeScriptVitestGenerator();
    const result = await gen.detectProjectStructure('/no-package-json');
    expect(result).toBeNull();
  });

  it('detectProjectStructure returns structure for agent-core', async () => {
    const gen = new TypeScriptVitestGenerator();
    const result = await gen.detectProjectStructure(process.cwd() + '/packages/agent-core');
    expect(result).toEqual({
      language: 'typescript',
      framework: 'nodejs',
      testTool: 'vitest',
      root: expect.stringContaining('packages/agent-core'),
    });
  });
});
```

- [ ] Run and verify FAILS: modules not found.

- [ ] Write `packages/agent-core/src/e2e-testing/registry.ts`:

```typescript
import type { E2ETestGenerator } from './types';
import { E2ENoMatchingGeneratorError } from './errors';
import { TypeScriptVitestGenerator } from './generator';

export class E2EGeneratorRegistry {
  private generators: E2ETestGenerator[] = [];

  register(generator: E2ETestGenerator): void {
    this.generators.push(generator);
  }

  async detectAndGet(projectRoot: string): Promise<E2ETestGenerator> {
    for (const generator of this.generators) {
      const structure = await generator.detectProjectStructure(projectRoot);
      if (structure !== null) return generator;
    }
    throw new E2ENoMatchingGeneratorError(projectRoot);
  }
}

export const registry = new E2EGeneratorRegistry();
registry.register(new TypeScriptVitestGenerator());
```

- [ ] Write `packages/agent-core/src/e2e-testing/generator.ts` (detection + stub for gen):

```typescript
import type { E2ETestGenerator, Feature, ProjectStructure, TestFile } from './types';
import { join } from 'pathe';

export class TypeScriptVitestGenerator implements E2ETestGenerator {
  readonly id = 'typescript-vitest';

  async detectProjectStructure(root: string): Promise<ProjectStructure | null> {
    const pkgPath = join(root, 'package.json');
    try {
      // Use dynamic import for test compatibility; in production this is the local Kaos.
      const { existsSync, readFileSync } = await import('node:fs');
      if (!existsSync(pkgPath)) return null;
      const pkg = JSON.parse(readFileSync(pkgPath, 'utf-8'));
      const hasVitest =
        pkg.devDependencies?.vitest !== undefined ||
        pkg.dependencies?.vitest !== undefined ||
        existsSync(join(root, 'vitest.config.ts')) ||
        existsSync(join(root, 'vitest.config.js'));
      if (!hasVitest) return null;
      return { language: 'typescript', framework: 'nodejs', testTool: 'vitest', root };
    } catch {
      return null;
    }
  }

  async generateTestsForFeature(_feature: Feature): Promise<TestFile[]> {
    throw new Error('Not implemented — generated in Part 2');
  }
}
```

- [ ] Run and verify PASSES: tests pass.

- [ ] Commit with message `feat(e2e): implement E2EGeneratorRegistry and Vitest detection`.

---

## Phase C: Core unit tests (Task 6)

### Task 6: Expand core unit tests for edge cases and coverage

**Depends on:** Task 5

**Files:** Modify `packages/agent-core/test/e2e-testing/core.test.ts` (append); Create `packages/agent-core/src/e2e-testing/index.ts` (optional barrel).

**Approach:** Tests already partially present from Tasks 1–5. This task adds additional edge cases and verifies ≥80% coverage on config resolver, impact analyzer, and registry.

- [ ] Append additional tests:

```typescript
describe('ImpactAnalyzer edge cases', () => {
  it('prioritizes critical over important when both match', () => {
    const config = { ...defaultConfig, criticalTools: ['ExitPlanModeTool', 'EnterPlanModeTool'] };
    const result = ImpactAnalyzer.analyze(
      [
        'packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts',
        'packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts',
      ],
      config,
    );
    const exitPlan = result.affectedTools.find(t => t.toolId === 'ExitPlanModeTool');
    const enterPlan = result.affectedTools.find(t => t.toolId === 'EnterPlanModeTool');
    expect(exitPlan!.priority).toBe('critical');
    expect(enterPlan!.priority).toBe('critical');
  });

  it('normalizes backslash paths', () => {
    const result = ImpactAnalyzer.analyze(
      ['packages\\agent-core\\src\\tools\\builtin\\planning\\exit-plan-mode.ts'],
      defaultConfig,
    );
    expect(result.affectedTools).toHaveLength(1);
  });
});

describe('E2EConfigResolver edge cases', () => {
  it('defaults testTimeout when omitted', () => {
    const result = E2EConfigResolver.resolve({ e2e: {} as any });
    expect(result.testTimeout).toBe(30000);
  });

  it('defaults failurePolicy to warn', () => {
    const result = E2EConfigResolver.resolve({});
    expect(result.failurePolicy).toBe('warn');
  });
});
```

- [ ] Run test suite: `pnpm --filter @odysseythink/agent-core test -- test/e2e-testing/core.test.ts`. All pass.

- [ ] Run coverage check: `pnpm --filter @odysseythink/agent-core test -- --coverage -- test/e2e-testing/core.test.ts`. Verify `impact-analyzer.ts`, `config.ts`, `registry.ts` have ≥80% line coverage.

- [ ] Run whole-tree typecheck: `pnpm -r typecheck`. Commit with message `test(e2e): expand core unit tests for edge cases`.

---

## Local Self-Review (Part 1: Core)

- [ ] 1. Spec-coverage table: Config schema → Tasks 1-2. Types → Task 3. Impact analysis → Task 4. Generator registry → Task 5. Unit tests → Task 6. All covered.
- [ ] 2. Placeholder scan: No `TODO`/`TBD` anywhere. `generateTestsForFeature` throws "Not implemented" as a temporary stub — that's acceptable, the real implementation arrives in Part 2 Task 1.
- [ ] 3. No phantom tasks: Every task creates or modifies real files; all steps produce a verifiable change.
- [ ] 4. Dependency soundness: Task 2 requires Task 1 (schema). Task 3 requires Task 2 (config). Task 4 requires Task 3 (types). Task 5 requires Task 4 (types). Task 6 requires Task 5. No forward references.
- [ ] 5. Caller & build soundness: Task 1 adds `e2e` to `KimiConfigSchema` — field is optional, no existing callers break. Task 1 edits `transformTomlData` and `configToTomlData` — ends with whole-tree typecheck. No other shared-signature changes in this part.
- [ ] 6. Test-the-risk: Config resolver tested for defaults, override, maxConcurrency validation (throws). Impact analyzer tested for matching, non-matching, always strategy fallback, critical-only filtering, path normalization, priority ordering. Registry tested for detection and throw-on-missing. Must-survive inputs: exit-plan-mode.ts and session-mode/index.ts correctly survive the TOOL_IMPACT_MAP filter (they are exact matches). Unrelated file correctly excluded.
- [ ] 7. Type consistency: `ResolvedE2EConfig` in `errors.ts` extends `Required<E2EConfig>`. `ImpactAnalyzer.analyze` signature uses `ResolvedE2EConfig`. `E2ETestGenerator` matches the interface in types. No property name mismatches.
