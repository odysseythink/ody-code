# Part 2: TypeScript/Vitest Generator and Test Executor

## Scope

This part defines the Phase 1 generator implementation (`TypeScriptVitestGenerator`) and the test executor (`E2ETestExecutor`). It covers how E2E test files are produced from a `Feature`, how they are run with Vitest, how results are parsed, and how reports are written.

---

## Data Models

### `E2EExecutionResult`

```typescript
interface E2EExecutionResult {
  passed: number;
  failed: number;
  skipped: number;
  durationMs: number;
  reportPath: string;
  summary: string; // markdown summary for tool output
  suites: TestSuiteResult[];
}

interface TestSuiteResult {
  file: string;
  status: 'passed' | 'failed' | 'skipped';
  duration: number;
  tests: TestCaseResult[];
}

interface TestCaseResult {
  name: string;
  status: 'passed' | 'failed' | 'skipped' | 'pending';
  failureMessages: string[];
}
```

### `TypeScriptVitestGenerator`

```typescript
class TypeScriptVitestGenerator implements E2ETestGenerator {
  readonly id = 'typescript-vitest';

  async detectProjectStructure(root: string): Promise<ProjectStructure | null>;
  async generateTestsForFeature(feature: Feature): Promise<TestFile[]>;
}
```

### `E2ETestExecutor`

```typescript
class E2ETestExecutor {
  constructor(kaos: Kaos, config: E2EConfig);

  /**
   * Writes generated tests to disk, runs them with Vitest, parses JSON output,
   * writes a report, and returns a summary. [C:INFERRED]
   */
  execute(testFiles: TestFile[], projectRoot: string): Promise<E2EExecutionResult>;
}
```

---

## Algorithms

### `TypeScriptVitestGenerator.generateTestsForFeature`

```
async function generateTestsForFeature(feature: Feature): Promise<TestFile[]>
  if feature.toolId === 'ExitPlanModeTool'
    return [buildExitPlanModeE2E(feature)]

  // Phase 1 fallback: emit a generic smoke test for any other tool.
  return [buildGenericToolE2E(feature)]
```

### `buildExitPlanModeE2E`

Produces a single temporary test file exercising the full plan-mode handoff.

```
function buildExitPlanModeE2E(feature: Feature): TestFile
  relativePath := 'exit-plan-mode.e2e.test.ts'
  content := renderTemplate(EXIT_PLAN_MODE_TEMPLATE, {
    toolImportPath: '#../../src/tools/builtin/planning/exit-plan-mode',
    agentImportPath: '#../../src/agent',
    harnessImportPath: '../agent/harness/agent', // relative to packages/agent-core/test/
    featureDescription: feature.description ?? 'ExitPlanModeTool E2E',
  })
  return { relativePath, content }
```

Template contract (language-agnostic pseudocode):

```
EXIT_PLAN_MODE_TEMPLATE placeholders:
  - {{AGENT_IMPORT}}
  - {{TOOL_IMPORT}}
  - {{HARNESS_IMPORT}}
  - {{DESCRIPTION}}

Rendered file must:
  1. Import `describe`, `it`, `expect` from 'vitest'.
  2. Import `ExitPlanModeTool`.
  3. Import `testAgent()` harness.
  4. Define `describe('ExitPlanModeTool E2E', ...)`.
  5. Define one `it` block that:
       a. Creates an agent with a mock LLM.
       b. Enters plan mode and writes a minimal plan.
       c. Calls `ExitPlanModeTool` with `selectedLabel`.
       d. Asserts the agent's session mode becomes `'normal'`.
       e. Asserts the handoff reminder contains the plan content.
```

### `buildGenericToolE2E`

```
function buildGenericToolE2E(feature: Feature): TestFile
  relativePath := `${kebabCase(feature.toolId)}.e2e.test.ts`
  content := renderTemplate(GENERIC_TOOL_TEMPLATE, {
    toolId: feature.toolId,
    changedFiles: feature.changedFiles,
    description: feature.description,
  })
  return { relativePath, content }
```

Generic template contract:

```
GENERIC_TOOL_TEMPLATE rendered file must:
  1. Import vitest primitives.
  2. Import the target tool class by convention from the matching builtin path.
  3. Import the agent harness.
  4. Define one smoke test that instantiates the tool and verifies its name/parameters are exported.
```

> Note: the generic template is intentionally minimal for Phase 1; real E2E coverage is provided by the ExitPlanModeTool template and expanded in later phases [C:INFERRED].

### `E2ETestExecutor.execute`

```
async function execute(testFiles: TestFile[], projectRoot: string): Promise<E2EExecutionResult>
  start := now()
  ensureDir(config.generatedTestDir)

  for file in testFiles
    absolutePath := join(config.generatedTestDir, file.relativePath)
    ensureDir(dirname(absolutePath))
    await kaos.writeText(absolutePath, file.content)

  absolutePaths := testFiles.map(f => join(config.generatedTestDir, f.relativePath))

  if absolutePaths is empty
    return emptyResult()

  outputFile := join(config.generatedTestDir, `.vitest-output-${timestamp()}.json`)

  chunkSize := config.maxConcurrency
  allSuiteResults := []

  for i from 0 to absolutePaths.length step chunkSize
    chunk := absolutePaths.slice(i, i + chunkSize)
    chunkResult := await runVitestChunk(chunk, projectRoot, outputFile)
    allSuiteResults.push(...chunkResult.suites)

  summary := {
    passed: sum(suites, s => s.tests.filter(t => t.status === 'passed').length),
    failed: sum(suites, s => s.tests.filter(t => t.status === 'failed').length),
    skipped: sum(suites, s => s.tests.filter(t => t.status === 'skipped').length),
    durationMs: now() - start,
    suites: allSuiteResults,
  }

  reportPath := await writeJsonReport(summary)
  return {
    ...summary,
    reportPath,
    summary: renderMarkdownSummary(summary),
  }
```

### `runVitestChunk`

```
async function runVitestChunk(files: string[], projectRoot: string, outputFile: string): Promise<{ suites: TestSuiteResult[] }>
  args := [
    'vitest',
    'run',
    '--reporter=json',
    `--outputFile=${outputFile}`,
    `--testTimeout=${config.testTimeout}`,
    ...files,
  ]

  proc := await kaos.exec('pnpm', args, { cwd: projectRoot })

  // Consume streams to avoid deadlock.
  stdout := await drain(proc.stdout)
  stderr := await drain(proc.stderr)
  exitCode := await proc.wait()

  if not await kaos.stat(outputFile)
    // Vitest may not write JSON on early crash; fall back to empty parseable result.
    return { suites: [makeCrashSuite(files, stdout, stderr, exitCode)] }

  json := JSON.parse(await kaos.readText(outputFile))
  return parseVitestJson(json)
```

### `parseVitestJson`

```
function parseVitestJson(result: VitestJsonOutput): { suites: TestSuiteResult[] }
  suites := []
  for suite in result.testResults ?? []
    tests := []
    for assertion in suite.assertionResults ?? []
      tests.push({
        name: assertion.title,
        status: mapStatus(assertion.status),
        failureMessages: assertion.failureMessages ?? [],
      })
    suites.push({
      file: suite.name,
      status: mapStatus(suite.status),
      duration: (suite.endTime ?? suite.startTime ?? 0) - (suite.startTime ?? 0),
      tests,
    })
  return { suites }
```

### `writeJsonReport`

```
async function writeJsonReport(summary: E2EExecutionResult): Promise<string>
  ensureDir(config.reportDir)
  filename := `e2e-report-${timestamp()}.json`
  path := join(config.reportDir, filename)
  await kaos.writeText(path, JSON.stringify({
    generatedAt: new Date().toISOString(),
    durationMs: summary.durationMs,
    passed: summary.passed,
    failed: summary.failed,
    skipped: summary.skipped,
    suites: summary.suites,
  }, null, 2))
  return path
```

### `renderMarkdownSummary`

```
function renderMarkdownSummary(summary: E2EExecutionResult): string
  lines := [
    '## E2E Test Results',
    `- Passed: ${summary.passed}`,
    `- Failed: ${summary.failed}`,
    `- Skipped: ${summary.skipped}`,
    `- Duration: ${summary.durationMs}ms`,
    `- Report: ${summary.reportPath}`,
  ]

  if summary.failed > 0
    lines.push('### Failures')
    for suite in summary.suites
      for test in suite.tests where test.status === 'failed'
        lines.push(`- ${suite.file} > ${test.name}`)
        for msg in test.failureMessages.slice(0, 3)
          lines.push(`  ${truncate(msg, 200)}`)

  return lines.join('\n')
```

---

## Call-Site Integration

### From `RunE2ETestsTool`

File: `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.ts` (new) [C:USER].

```typescript
// Pseudocode sketch inside resolveExecution()
const e2eConfig = E2EConfigResolver.resolve(agent.config);
if (!e2eConfig.enabled) {
  return { output: 'E2E testing is disabled in config.' };
}

const generator = await registry.detectAndGet(feature.projectRoot);
const testFiles = await generator.generateTestsForFeature(feature);

const executor = new E2ETestExecutor(this.kaos, e2eConfig);
const result = await executor.execute(testFiles, feature.projectRoot);

return {
  output: result.summary,
  isError: result.failed > 0 && e2eConfig.failurePolicy === 'block',
  stopTurn: result.failed > 0 && e2eConfig.failurePolicy === 'block',
};
```

### Subprocess Execution

`E2ETestExecutor` must use `Kaos.exec` from `packages/kaos/src/kaos.ts` (lines 12-89) rather than raw `child_process` [C:INFERRED].

---

## Error Handling

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| `E2EGeneratorReturnedNoTestsError` | Tool returns warning; no files executed | Continue plan execution | Improve generator template |
| `VitestProcessError` (non-zero exit) | Parse any partial JSON; treat missing suites as failed | Apply `failurePolicy` | Fix generated or source tests |
| `VitestOutputParseError` | Return raw stdout/stderr in summary; mark all as failed | Apply `failurePolicy` | Fix parser or vitest version |
| `ReportWriteError` | Log warning; still return tool summary | No JSON report on disk | Fix directory permissions |
| `E2ETestTimeoutError` | Kill process via `signal`; mark timed-out tests failed | Apply `failurePolicy` | Increase `testTimeout` or split tests |

---

## Tests (Generator & Executor Part)

Location: `packages/agent-core/test/e2e-testing/generator.test.ts` and `packages/agent-core/test/e2e-testing/executor.test.ts`.

### `generator.test.ts` must-pass assertions

1. `TypeScriptVitestGenerator.detectProjectStructure(packages/agent-core)` returns `{ language: 'typescript', framework: 'nodejs', testTool: 'vitest', root }`.
2. `TypeScriptVitestGenerator.detectProjectStructure('/no-package-json')` returns `null`.
3. `generateTestsForFeature({ toolId: 'ExitPlanModeTool', ... })` returns exactly one `TestFile` whose `relativePath` is `exit-plan-mode.e2e.test.ts`.
4. The generated content for ExitPlanModeTool contains `import { describe, it, expect } from 'vitest'`.
5. The generated content is valid TypeScript when parsed by `tsc --noEmit`.
6. `generateTestsForFeature({ toolId: 'SomeOtherTool', ... })` returns a generic smoke test.

### `executor.test.ts` must-pass assertions

1. `E2ETestExecutor.execute([], projectRoot)` returns `passed: 0, failed: 0, skipped: 0` and an empty `suites` array.
2. `E2ETestExecutor.execute([validTestFile], projectRoot)` writes the file to `config.generatedTestDir`, runs vitest, and returns `passed >= 1`.
3. The executor writes a JSON report to `config.reportDir` after a successful run.
4. `E2ETestExecutor.execute([failingTestFile], projectRoot)` returns `failed >= 1` and a markdown summary containing the failing test name.
5. With `maxConcurrency: 1`, only one vitest process runs at a time (verify by mocking `Kaos.exec`).
6. With `maxConcurrency: 4` and 6 test files, `Kaos.exec` is called exactly twice.
7. `parseVitestJson` returns correct counts for a sample vitest JSON fixture.
8. `renderMarkdownSummary` truncates failure messages longer than 200 characters.

### Must-reject assertions

1. `E2ETestExecutor.execute` with a test file that cannot be parsed by vitest returns `failed >= 1` (does not throw unhandled).
2. `TypeScriptVitestGenerator.generateTestsForFeature` rejects if `feature.projectRoot` is not an absolute path [C:INFERRED].

---

## Local Notes

- The generated test file for `ExitPlanModeTool` must import from the package root using the existing `#/...` path alias if available; otherwise use relative paths computed from `packages/agent-core/test/e2e-generated/e2e/` back to `packages/agent-core/src/` [C:INFERRED].
- Vitest JSON reporter schema may vary by version; the parser must treat missing fields as defaults (e.g. `assertionResults ?? []`) [C:INFERRED].
- Report filenames include a timestamp with milliseconds to avoid collision during concurrent runs or rapid re-runs [C:INFERRED].
- `E2ETestExecutor` should accept an optional `AbortSignal` from `ExecutableToolContext` and forward it to `Kaos.exec` cancellation; if `Kaos.exec` does not support signals directly, kill the process on abort [C:INFERRED].
