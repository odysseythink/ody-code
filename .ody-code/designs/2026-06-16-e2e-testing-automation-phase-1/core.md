# Part 1: Core Abstractions, Config, Registry, and Impact Analysis

## Scope

This part defines the data models, configuration schema, generator registry, and impact-analysis heuristics used by the E2E testing framework. It does **not** cover the TS/Vitest generator, executor, or plan-mode integration (those are Parts 2 and 3).

---

## Data Models

### `E2EConfig`

Runtime configuration loaded from `~/.ody-code/config.toml` under `[e2e]`.

```typescript
interface E2EConfig {
  /** Master toggle. [C:USER] */
  enabled: boolean;

  /** When to inject E2E tasks. [C:USER] */
  strategy: 'always' | 'smart' | 'critical-only';

  /** Tool class names that must be treated as critical. [C:USER] */
  criticalTools: string[];

  /** How to react to test failures. [C:USER] */
  failurePolicy: 'block' | 'warn' | 'ignore';

  /** Maximum concurrent test files. [C:INFERRED] */
  maxConcurrency: number;

  /** Per-test timeout in milliseconds. [C:INFERRED] */
  testTimeout: number;

  /** Directory for JSON reports. [C:USER] */
  reportDir: string;

  /** Directory for generated temporary test files. [C:INFERRED] */
  generatedTestDir: string;
}
```

Default values when `[e2e]` is absent:

```toml
[e2e]
enabled = true
strategy = "smart"
criticalTools = ["ExitPlanModeTool"]
failurePolicy = "warn"
maxConcurrency = 4
testTimeout = 30000
reportDir = ".ody-code/test-reports"
generatedTestDir = ".ody-code/test-generated/e2e"
```

### `ProjectStructure`

Returned by a generator after inspecting a project root.

```typescript
interface ProjectStructure {
  language: string;   // e.g. 'typescript'
  framework: string;  // e.g. 'nodejs'
  testTool: string;   // e.g. 'vitest'
  root: string;       // absolute project root
}
```

### `Feature`

A unit of work for which E2E tests should be generated.

```typescript
interface Feature {
  toolId: string;
  changedFiles: string[];
  projectRoot: string;
  description?: string;
}
```

### `TestFile`

A generated test artifact.

```typescript
interface TestFile {
  relativePath: string; // relative to generatedTestDir
  content: string;
}
```

### `AffectedTool` / `ImpactAnalysisResult`

Output of impact analysis.

```typescript
type E2EPriority = 'critical' | 'important' | 'nice-to-have';

interface AffectedTool {
  toolId: string;
  priority: E2EPriority;
}

interface ImpactAnalysisResult {
  affectedTools: AffectedTool[];
}
```

### `E2ETestGenerator` (interface)

```typescript
interface E2ETestGenerator {
  readonly id: string;

  /** Returns true if this generator can handle the project at `root`. [C:INFERRED] */
  detectProjectStructure(root: string): Promise<ProjectStructure | null>;

  /** Generates test files for one feature. [C:INFERRED] */
  generateTestsForFeature(feature: Feature): Promise<TestFile[]>;
}
```

### `E2EGeneratorRegistry`

```typescript
class E2EGeneratorRegistry {
  register(generator: E2ETestGenerator): void;

  /**
   * Detects the project structure and returns the matching generator.
   * Throws `E2ENoMatchingGeneratorError` if none matches. [C:INFERRED]
   */
  detectAndGet(projectRoot: string): Promise<E2ETestGenerator>;
}
```

---

## Config Loading

### Call Site

`RunE2ETestsTool` obtains the agent config and passes it to `E2EConfigResolver.resolve(config)`.

- File: `packages/agent-core/src/config/toml.ts`
- Lines: 79-84 (`loadRuntimeConfig`)

```typescript
// Pseudocode sketch at call site
const config = loadRuntimeConfig(resolveConfigPath(), process.env);
const e2eConfig = E2EConfigResolver.resolve(config);
if (!e2eConfig.enabled) return { output: 'E2E disabled' };
```

### Schema Patch

Add to `packages/agent-core/src/config/schema.ts` near line 224 [C:USER]:

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

// Insert into KimiConfigSchema:
// e2e: E2EConfigSchema.optional(),
```

### `E2EConfigResolver.resolve` Algorithm

```
function resolve(kimiConfig: KimiConfig): E2EConfig
  raw := kimiConfig.e2e ?? {}
  defaults := { enabled: true, strategy: 'smart', criticalTools: ['ExitPlanModeTool'], failurePolicy: 'warn', maxConcurrency: 4, testTimeout: 30000, reportDir: '.ody-code/test-reports', generatedTestDir: '.ody-code/test-generated/e2e' }
  merged := apply defaults, then override with raw
  return validate(merged)  // throws on type error
```

---

## Generator Registry

### Registration

`TypeScriptVitestGenerator` is registered in a new module `packages/agent-core/src/e2e-testing/registry.ts`:

```typescript
const registry = new E2EGeneratorRegistry();
registry.register(new TypeScriptVitestGenerator());
export { registry };
```

### `detectAndGet` Algorithm

```
async function detectAndGet(projectRoot: string): Promise<E2ETestGenerator>
  matches := []
  for each generator in registeredGenerators
    structure := await generator.detectProjectStructure(projectRoot)
    if structure is not null
      matches.push({ generator, structure })

  if matches is empty
    throw E2ENoMatchingGeneratorError(projectRoot)

  // Phase 1: only TypeScriptVitestGenerator exists, so return first match.
  return matches[0].generator
```

### `TypeScriptVitestGenerator.detectProjectStructure`

```
async function detectProjectStructure(root: string): Promise<ProjectStructure | null>
  if fileExists(join(root, 'package.json')) is false
    return null

  packageJson := JSON.parse(await readText(join(root, 'package.json')))

  if packageJson.devDependencies?.vitest is defined
     or packageJson.dependencies?.vitest is defined
     or fileExists(join(root, 'vitest.config.ts'))
     or fileExists(join(root, 'vitest.config.js'))
    return { language: 'typescript', framework: 'nodejs', testTool: 'vitest', root }

  return null
```

---

## Impact Analysis

### Goal

Map a list of changed file paths to the set of builtin tools that may be affected, each with a priority.

### Tool Mapping Table

Phase 1 uses an explicit, testable mapping from tool class name to filesystem globs. This avoids fragile class-name inference while remaining extensible [C:INFERRED].

```typescript
const TOOL_IMPACT_MAP: Record<string, string[]> = {
  ExitPlanModeTool: [
    'packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts',
    'packages/agent-core/src/agent/session-mode/index.ts',
    'packages/agent-core/src/agent/injection/plan-mode.ts',
  ],
  EnterPlanModeTool: [
    'packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts',
  ],
  // Additional tools added as Phase 1 expands.
};
```

### `ImpactAnalyzer.analyze` Algorithm

```
function analyze(changedFiles: string[], config: E2EConfig): ImpactAnalysisResult
  result := empty map<string, E2EPriority>

  for file in changedFiles
    for (toolId, globs) in TOOL_IMPACT_MAP
      if anyGlobMatches(globs, file)
        priority := computePriority(toolId, config)
        result[toolId] := maxPriority(result[toolId], priority)

  if result is empty and config.strategy === 'always'
    result['general'] := 'nice-to-have'

  affected := []
  for (toolId, priority) in result
    if config.strategy === 'critical-only' and priority !== 'critical'
      continue
    affected.push({ toolId, priority })

  return { affectedTools: affected }

function computePriority(toolId: string, config: E2EConfig): E2EPriority
  if config.criticalTools.includes(toolId)
    return 'critical'
  return 'important'
```

### `anyGlobMatches` Algorithm

```
function anyGlobMatches(globs: string[], file: string): boolean
  normalized := file.replace(/\\/g, '/')
  for glob in globs
    if minimatch(normalized, glob, { matchBase: true })
      return true
  return false
```

> Note: use the existing `packages/agent-core/src/utils/fs.ts` helpers or `minimatch` if already in dependencies. If `minimatch` is not a dependency, use the project's existing glob utilities (`Kaos.glob`) [C:INFERRED].

---

## Error Handling

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| `E2EConfigValidationError` | Tool returns error result with details | Run with defaults | User fixes `config.toml` |
| `E2ENoMatchingGeneratorError` | Tool warns and skips generation | No tests generated | Project uses a generator added in Phase 2 |
| `ImpactAnalysisFailedError` | Log warning, return empty result | No E2E task injected | Fix analyzer or mapping |

---

## Tests (Core Part)

Location: `packages/agent-core/test/e2e-testing/core.test.ts`

### Must-pass assertions

1. `E2EConfigResolver.resolve({})` returns all defaults including `enabled: true`, `strategy: 'smart'`, `criticalTools: ['ExitPlanModeTool']`.
2. `E2EConfigResolver.resolve({ e2e: { enabled: false } })` returns `enabled: false` with other defaults intact.
3. `ImpactAnalyzer.analyze(['packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts'], config)` returns `AffectedTool { toolId: 'ExitPlanModeTool', priority: 'critical' }`.
4. `ImpactAnalyzer.analyze(['packages/agent-core/src/agent/session-mode/index.ts'], config)` returns `ExitPlanModeTool` as `critical`.
5. `ImpactAnalyzer.analyze(['unrelated-file.md'], config)` returns empty array when `strategy` is `'smart'`.
6. `ImpactAnalyzer.analyze(['unrelated-file.md'], { ...config, strategy: 'always' })` returns `general` with `nice-to-have`.
7. `ImpactAnalyzer.analyze(['exit-plan-mode.ts'], { ...config, strategy: 'critical-only' })` returns `ExitPlanModeTool` as `critical`.
8. `E2EGeneratorRegistry.detectAndGet('/path/to/packages/agent-core')` returns the `TypeScriptVitestGenerator`.
9. `E2EGeneratorRegistry.detectAndGet('/path/to/no-package-json')` throws `E2ENoMatchingGeneratorError`.

### Must-reject assertions

1. `E2EConfigResolver.resolve({ e2e: { maxConcurrency: 0 } })` throws validation error.
2. `ImpactAnalyzer.analyze` with an unknown strategy does not throw (falls back to smart behavior) [C:INFERRED].

---

## Local Notes

- Keep `TOOL_IMPACT_MAP` in a separate file `impact-map.ts` so it can be extended without touching the analyzer algorithm [C:INFERRED].
- The registry is a singleton exported from `packages/agent-core/src/e2e-testing/registry.ts`; it is initialized once at module load.
- Config validation should reuse the existing `KimiConfigSchema` patch path so env-model overrides continue to work [C:INFERRED].
