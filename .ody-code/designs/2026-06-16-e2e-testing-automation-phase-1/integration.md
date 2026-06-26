# Part 3: Plan Enrichment, RunE2ETests Tool, and Dog-Food Tests

## Scope

This part defines how the E2E framework integrates with plan mode:

- `E2EPlanEnricher`: mutates the approved plan markdown before review to add an E2E task.
- `RunE2ETestsTool`: the builtin tool that generates and executes tests during normal-mode execution.
- Tool registration and goal-mode auto-trigger hook.
- Dog-food validation: `ExitPlanModeTool` gets an E2E test and the end-to-end plan flow is exercised.

---

## Data Models

### `RunE2ETestsInput`

```typescript
const RunE2ETestsInputSchema = z.object({
  /** Optional specific tool to test; if omitted, all affected tools are tested. [C:INFERRED] */
  toolId: z.string().optional(),

  /** Optional project root; defaults to the agent workspace root. [C:INFERRED] */
  projectRoot: z.string().optional(),
});

type RunE2ETestsInput = z.infer<typeof RunE2ETestsInputSchema>;
```

### `E2EPlanEnricher`

```typescript
class E2EPlanEnricher {
  constructor(
    private kaos: Kaos,
    private config: E2EConfig,
    private analyzer: ImpactAnalyzer,
  );

  /**
   * Reads the plan, determines changed files, runs impact analysis, and appends
   * an E2E task if any tools are affected. Returns the enriched markdown or null
   * if no enrichment is needed. [C:USER]
   */
  enrich(planPath: string, planContent: string, projectRoot: string): Promise<string | null>;
}
```

### `E2ETaskContext`

Internal helper passed between enrichment and execution so both phases agree on affected tools.

```typescript
interface E2ETaskContext {
  affectedTools: AffectedTool[];
  changedFiles: string[];
  generatedAt: string;
}
```

> Note: in Phase 1 the context is implicit in the plan markdown; the execution phase re-runs impact analysis at tool-call time [C:INFERRED].

---

## Algorithms

### `E2EPlanEnricher.enrich`

```
async function enrich(planPath, planContent, projectRoot): Promise<string | null>
  if config.enabled === false
    return null

  if config.strategy === 'always' and config.criticalTools is empty
    // still proceed; will inject a generic E2E task

  changedFiles := await determineChangedFiles(projectRoot, planContent)
  if changedFiles is empty and config.strategy !== 'always'
    return null

  impact := analyzer.analyze(changedFiles, config)

  if impact.affectedTools is empty and config.strategy !== 'always'
    return null

  if config.strategy === 'critical-only'
    impact.affectedTools := impact.affectedTools.filter(t => t.priority === 'critical')
    if impact.affectedTools is empty
      return null

  return appendE2ETaskToMarkdown(planContent, impact.affectedTools)
```

### `determineChangedFiles`

```
async function determineChangedFiles(projectRoot, planContent): Promise<string[]>
  gitFiles := await gitStatusFiles(projectRoot)
  if gitFiles.length > 0
    return gitFiles

  // Fallback: extract likely file paths from the plan markdown.
  return extractFilePathsFromPlan(planContent)

async function gitStatusFiles(projectRoot): Promise<string[]>
  proc := await kaos.exec('git', ['status', '--short', '--no-renames'], { cwd: projectRoot })
  exitCode := await proc.wait()
  if exitCode !== 0
    return []

  lines := await drain(proc.stdout)
  result := []
  for line in lines.split('\n')
    trimmed := line.trim()
    if trimmed.length < 4
      continue
    // Format: "XY path" or "XY orig -> rename"
    path := trimmed.substring(3).trim()
    if path.includes(' -> ')
      path := path.split(' -> ').pop().trim()
    result.push(path)
  return result

function extractFilePathsFromPlan(planContent): string[]
  // Match paths starting with packages/ or apps/ and ending in a source extension.
  regex := /(?:packages|apps)\/[a-zA-Z0-9\-_\/\.]+\.(?:ts|tsx|js|jsx|mjs|cjs)/g
  matches := planContent.match(regex) ?? []
  return unique(matches)
```

### `appendE2ETaskToMarkdown`

```
function appendE2ETaskToMarkdown(content, affectedTools): string
  lines := content.split('\n')
  lastTaskNum := 0

  for line in lines
    match := line.match(/^#{1,4}\s+Task\s+(\d+)\s*[:\-]?/i)
    if match
      lastTaskNum := max(lastTaskNum, parseInt(match[1], 10))

  newTaskNum := lastTaskNum + 1
  priorityText := affectedTools
    .map(t => `- ${t.toolId} (priority: ${t.priority})`)
    .join('\n')

  section := `
### Task ${newTaskNum}: Generate and run E2E tests

Based on the changed files, validate the following tools:
${priorityText}

Use the RunE2ETests tool after completing the implementation tasks above.
`

  return content.trimEnd() + section + '\n'
```

### `RunE2ETestsTool.resolveExecution`

```
resolveExecution(input: RunE2ETestsInput): ToolExecution
  return {
    description: `Run E2E tests for ${input.toolId ?? 'affected tools'}`,
    approvalRule: literalRulePattern(this.name, input.toolId ?? '*'),
    matchesRule: (ruleArgs) => matchesGlobRuleSubject(ruleArgs, input.toolId ?? '*'),
    execute: (ctx) => this.execution(input, ctx),
  }
```

### `RunE2ETestsTool.execution`

```
async function execution(input, ctx): Promise<ExecutableToolResult>
  config := E2EConfigResolver.resolve(this.agent.config)
  if config.enabled === false
    return { output: 'E2E testing is disabled in config.' }

  projectRoot := input.projectRoot ?? this.agent.workspaceRoot ?? this.kaos.getcwd()

  generator := await registry.detectAndGet(projectRoot)

  changedFiles := await determineChangedFiles(projectRoot, '')
  impact := this.analyzer.analyze(changedFiles, config)

  if input.toolId
    impact.affectedTools := impact.affectedTools.filter(t => t.toolId === input.toolId)

  if impact.affectedTools is empty
    return { output: 'No affected tools detected; skipping E2E tests.' }

  testFiles := []
  for tool in impact.affectedTools
    feature := {
      toolId: tool.toolId,
      changedFiles,
      projectRoot,
      priority: tool.priority,
    }
    testFiles.push(...await generator.generateTestsForFeature(feature))

  if testFiles is empty
    return { output: 'E2E generator produced no test files.' }

  executor := new E2ETestExecutor(this.kaos, config)
  result := await executor.execute(testFiles, projectRoot)

  if ctx.signal?.aborted
    return { output: 'E2E tests cancelled.', isError: true }

  isError := result.failed > 0 && config.failurePolicy === 'block'
  return {
    output: result.summary,
    isError,
    stopTurn: isError,
    message: isError ? 'Critical E2E tests failed.' : undefined,
  }
```

### Goal-Mode Auto-Trigger Hook

When the `goal-command` flag is enabled, `TurnFlow.driveGoal()` drives continuation turns. Add a lightweight hook so that completion of an implementation todo can automatically schedule `RunE2ETests` [C:USER].

```
function shouldTriggerE2EAfterTodo(todoTitle: string, config: E2EConfig): boolean
  if config.enabled === false
    return false
  if config.strategy === 'critical-only'
    return false
  lowered := todoTitle.toLowerCase()
  return lowered.includes('e2e') || lowered.includes('test')

// Hook location: normal-task-checkpoint or TurnFlow.driveGoal()
// Pseudocode:
async function afterTodoCompleted(todo, agent)
  if not flags.enabled('goal-command')
    return
  if not shouldTriggerE2EAfterTodo(todo.title, config)
    return
  // Inject a reminder to call RunE2ETests in the next turn context.
  agent.injection.addNormalPartitionReminder(
    'The E2E task is ready. Call RunE2ETests to validate the changes.'
  )
```

> Note: the primary path remains model-driven; this hook is an enhancement available only when goal-command is on [C:INFERRED].

---

## Call-Site Integration

### ExitPlanModeTool Enrichment

File: `packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts` [C:USER].
Lines: 95-124 (`resolveExecution`).

```typescript
// Inside resolveExecution(), before resolvePlanReviewDisplay()
const modeData = this.agent.sessionMode.data();
if (modeData?.kind === 'plan') {
  const e2eConfig = E2EConfigResolver.resolve(this.agent.config);
  const enricher = new E2EPlanEnricher(
    this.kaos,
    e2eConfig,
    new ImpactAnalyzer(),
  );
  const enriched = await enricher.enrich(
    modeData.path,
    modeData.content,
    this.agent.workspaceRoot ?? this.kaos.getcwd(),
  );
  if (enriched !== null) {
    await this.kaos.writeText(modeData.path, enriched);
  }
}
// existing resolvePlanReviewDisplay() reads the updated file
```

> `ExitPlanModeTool` currently does not hold a `Kaos` reference; add it via constructor in `ToolManager.initializeBuiltinTools()` [C:INFERRED].

### Tool Registration

File: `packages/agent-core/src/tools/builtin/index.ts` [C:INFERRED].
Add `export { RunE2ETestsTool } from './e2e/run-e2e-tests';`.

File: `packages/agent-core/src/agent/tool/index.ts` [C:INFERRED].
Lines: 407-464 (`initializeBuiltinTools`).
Add `new b.RunE2ETestsTool(kaos, this.agent)` to the array of builtin tools.

### Goal-Mode Hook

File: `packages/agent-core/src/agent/compaction/normal-task-checkpoint.ts` [C:INFERRED].
Lines: 37-64.

```typescript
// After detecting a completed todo in beforeCheckpoint()
if (flags.enabled('goal-command')) {
  const config = E2EConfigResolver.resolve(this.agent.config);
  if (shouldTriggerE2EAfterTodo(todo.title, config)) {
    this.agent.injection.addNormalPartitionReminder(
      'E2E task is ready; call RunE2ETests.',
    );
  }
}
```

---

## Error Handling

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| `E2EPlanEnrichError` (write failure) | Log warning; continue with original plan | E2E task not shown in review | Fix file permissions or disk space |
| `GitStatusError` | Fall back to plan-content path extraction | Impact analysis may be less accurate | Ensure git is available in workspace |
| `RunE2ETestsPermissionDenied` | Tool returns error result | User can approve and retry | Adjust permission policy or yolo mode |
| `NoAffectedToolsError` | Tool returns informational message | Plan continues without E2E | Modify files or config to include tools |

---

## Tests (Integration Part)

Location: `packages/agent-core/test/e2e-testing/integration.test.ts` and `packages/agent-core/test/e2e-testing/plan-enrichment.e2e.test.ts`.

### `integration.test.ts` must-pass assertions

1. `RunE2ETestsTool` is registered in `ToolManager.initializeBuiltinTools()` and has `name === 'RunE2ETests'`.
2. `RunE2ETestsTool.resolveExecution({ toolId: 'ExitPlanModeTool' }).approvalRule` contains `'ExitPlanModeTool'`.
3. `E2EPlanEnricher.enrich(plan, content, projectRoot)` returns null when `enabled: false`.
4. `E2EPlanEnricher.enrich` appends a new task when `exit-plan-mode.ts` is in git status.
5. The appended task number is `lastTaskNum + 1` and does not duplicate existing numbers.
6. `E2EPlanEnricher.enrich` returns null for `strategy: 'smart'` when no files are changed and plan contains no paths.
7. `RunE2ETestsTool.execution` returns `isError: true` when a generated test fails and `failurePolicy: 'block'`.
8. `RunE2ETestsTool.execution` returns `isError: false` when tests fail but `failurePolicy: 'warn'`.

### `plan-enrichment.e2e.test.ts` must-pass assertions

1. Drive an agent through `EnterPlanModeTool` + `ExitPlanModeTool` with a changed `exit-plan-mode.ts` file.
2. Assert the plan file written before review contains the appended `### Task N: Generate and run E2E tests` section.
3. Assert the review display includes the new task text.

### Dog-food end-to-end assertion

1. Run the generated `exit-plan-mode.e2e.test.ts` (from `TypeScriptVitestGenerator`) against `packages/agent-core` and assert it passes.

### Must-reject assertions

1. `E2EPlanEnricher.enrich` must not append a task when `strategy === 'critical-only'` and no critical tool is affected.
2. `RunE2ETestsTool.execution` must reject invalid `projectRoot` inputs (non-absolute paths) [C:INFERRED].

---

## Local Notes

- `ExitPlanModeTool` constructor signature changes from `(agent: Agent)` to `(agent: Agent, kaos: Kaos)`; update its instantiation in `ToolManager.initializeBuiltinTools()` [C:INFERRED].
- The `RunE2ETestsTool` should be available to the model by default in normal mode; no additional flag is required because the feature is stable and default-enabled [C:USER].
- Avoid importing `RunE2ETestsTool` into `ExitPlanModeTool` to prevent circular dependencies; use the shared `E2EPlanEnricher` from `packages/agent-core/src/e2e-testing/plan-enricher.ts` [C:INFERRED].
- Generated temporary tests must be excluded from the regular `pnpm test` run and from git; add `.ody-code/test-generated/` to `.gitignore` [C:INFERRED].
