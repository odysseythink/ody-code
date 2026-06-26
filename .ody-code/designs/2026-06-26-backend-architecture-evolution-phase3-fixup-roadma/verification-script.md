# Part 1 — Verification Script

## 1. Scope

This part designs `scripts/verify-phase-a3.mjs` and the JSON report it produces. It covers:

- CLI parsing and `VerificationConfig` defaults. [C:USER]
- The `StepRegistry` and ordered execution model. [C:USER]
- Capturing, redacting, and writing subprocess output. [C:USER]
- Writing `.ody-code/reports/phase-a3-report.json`. [C:USER]
- Updating `docs/designs/rust-host-reversal-adr.md` Prototype Results tables. [C:USER]

Out of scope:
- Individual verification step implementations that touch product code (TUI smoke, vis/web typecheck) — covered in Part 2 and Part 3.
- CI workflow plumbing — covered in Part 4.

---

## 2. Data Models

### 2.1 `VerificationConfig`

```ts
interface VerificationConfig {
  /** Absolute or workspace-relative path to the ody-host binary. [C:INFERRED] */
  hostBinaryPath: string;

  /** Directory for the JSON report and raw logs. [C:USER] */
  reportDir: string;

  /** Timeout in ms for long steps (SEA build, typecheck). [C:INFERRED] */
  defaultTimeoutMs: number;

  /** Per-step overrides. [C:INFERRED] */
  stepTimeoutsMs: Record<string, number>;

  /** If true, skip SEA steps (e.g. on platforms without postject). [C:INFERRED] */
  skipSea: boolean;

  /** If true, keep temp home dirs for debugging. [C:INFERRED] */
  keepTemp: boolean;
}
```

Default resolution [C:INFERRED]:
- `hostBinaryPath` → `process.env.ODY_HOST_BINARY_PATH` → `rust-ody/target/release/ody-host` relative to workspace root.
- `reportDir` → `.ody-code/reports`.
- `defaultTimeoutMs` → `300_000` (5 min).
- `skipSea` → `false` on darwin/linux, `true` on win32 if unsupported.

### 2.2 `StepContext`

```ts
interface StepContext {
  readonly config: VerificationConfig;
  readonly workspaceRoot: string;
  readonly tempHomeDir: string;
  readonly env: NodeJS.ProcessEnv;
}
```

### 2.3 `Step`

```ts
interface Step {
  readonly id: string;
  readonly name: string;
  readonly run(ctx: StepContext): Promise<StepResult>;
}
```

### 2.4 `StepResult`

```ts
interface StepResult {
  readonly id: string;
  readonly status: 'passed' | 'failed' | 'skipped';
  readonly command: string;
  readonly args: string[];
  readonly cwd: string;
  readonly exitCode: number | null;
  readonly signal: string | null;
  readonly durationMs: number;
  readonly stdoutRedacted: string;
  readonly stderrRedacted: string;
  readonly errorMessage?: string;
}
```

### 2.5 `PhaseA3Report`

```ts
interface PhaseA3Report {
  readonly metadata: {
    readonly nodeVersion: string;
    readonly platform: string;
    readonly arch: string;
    readonly timestamp: string; // ISO 8601
    readonly commit?: string;
    readonly hostBinaryPath: string;
  };
  readonly environment: {
    readonly cwd: string;
    readonly pnpmVersion: string;
    readonly cargoVersion: string;
    readonly rustcVersion: string;
  };
  readonly steps: StepResult[];
  readonly summary: {
    readonly overallStatus: 'passed' | 'failed' | 'partial';
    readonly passedCount: number;
    readonly failedCount: number;
    readonly skippedCount: number;
    readonly totalDurationMs: number;
  };
}
```

---

## 3. Algorithms

### 3.1 `main()`

```
function main(argv: string[]): Promise<void>
  ensureNodeVersion('24.15.0')
  config := parseConfig(argv)
  report := runVerification(config)
  writeReportSync(join(config.reportDir, 'phase-a3-report.json'), report)
  if report.summary.overallStatus === 'passed'
    updateAdr(report)
    exit 0
  else
    updateAdr(report)  // record partial/failed state too
    exit 1
```

### 3.2 `ensureNodeVersion(minVersion)`

```
function ensureNodeVersion(minVersion: string): void
  current := parseSemver(process.version)
  minimum := parseSemver(minVersion)
  if current < minimum
    throw Error(`Node ${minVersion}+ required, found ${process.version}`)
```

### 3.3 `runVerification(config)`

```
function runVerification(config: VerificationConfig): PhaseA3Report
  ctx := buildContext(config)
  steps := buildStepRegistry(config)
  results := []
  startedAt := now()

  for step in steps
    if shouldSkip(step, config)
      results.push({ id: step.id, status: 'skipped', ... })
      continue

    result := await runStep(step, ctx)
    results.push(result)

    if result.status === 'failed'
      break  // fail-fast [C:USER]

  return {
    metadata: buildMetadata(config),
    environment: buildEnvironment(),
    steps: results,
    summary: buildSummary(results, now() - startedAt),
  }
```

### 3.4 `runStep(step, ctx)`

```
function runStep(step: Step, ctx: StepContext): StepResult
  startedAt := now()
  child := spawn(step.command, step.args, {
    cwd: ctx.workspaceRoot,
    env: ctx.env,
    timeout: resolveTimeout(step.id, ctx.config),
  })

  stdoutChunks := []
  stderrChunks := []
  child.stdout.on('data', chunk => stdoutChunks.push(chunk))
  child.stderr.on('data', chunk => stderrChunks.push(chunk))

  try
    exitCode := await child.exited
    durationMs := now() - startedAt
    status := exitCode === 0 ? 'passed' : 'failed'
    return {
      id: step.id,
      status,
      command: step.command,
      args: step.args,
      cwd: ctx.workspaceRoot,
      exitCode,
      signal: null,
      durationMs,
      stdoutRedacted: redact(Buffer.concat(stdoutChunks).toString()),
      stderrRedacted: redact(Buffer.concat(stderrChunks).toString()),
    }
  catch error
    durationMs := now() - startedAt
    return {
      id: step.id,
      status: 'failed',
      command: step.command,
      args: step.args,
      cwd: ctx.workspaceRoot,
      exitCode: error.code ?? null,
      signal: error.signal ?? null,
      durationMs,
      stdoutRedacted: redact(Buffer.concat(stdoutChunks).toString()),
      stderrRedacted: redact(Buffer.concat(stderrChunks).toString()),
      errorMessage: error.message,
    }
  finally
    if child.exitCode === null
      killTree(child.pid)
```

### 3.5 `redact(text)`

```
function redact(text: string): string
  patterns := [
    /"api_key"\s*:\s*"[^"]{4,}"/gi,
    /"access_token"\s*:\s*"[^"]{4,}"/gi,
    /"password"\s*:\s*"[^"]{1,}"/gi,
    /"secret"\s*:\s*"[^"]{4,}"/gi,
    /authorization:\s*bearer\s+\S+/gi,
    /api[_-]?key[=:]\s*\S+/gi,
  ]
  result := text
  for pattern in patterns
    result := result.replace(pattern, (match) => match.slice(0, visiblePrefix(match)) + '***')
  return result
```

Visible prefix rule: keep the key name and first 4 chars of the value, e.g. `"api_key":"sk-9"***`. [C:INFERRED]

### 3.6 `buildStepRegistry(config)`

```
function buildStepRegistry(config: VerificationConfig): Step[]
  return [
    rustTestStep(),
    crossLangRpcStep(),
    tuiSmokeStep({ transport: 'stdio' }),
    tuiSmokeStep({ transport: 'socket' }),
    tuiSmokeStep({ transport: 'tcp' }),
    seaBuildStep(),
    seaSmokeStep(),
    typecheckStep(),
  ].filter(step => !config.skipSteps.includes(step.id))
```

SEA steps are skipped when `config.skipSea === true`. [C:INFERRED]

### 3.7 `updateAdr(report)`

```
function updateAdr(report: PhaseA3Report): void
  adrPath := 'docs/designs/rust-host-reversal-adr.md'
  text := readFileSync(adrPath, 'utf-8')
  mapping := {
    'rust-test': 'cargo test -p ody-host',
    'cross-lang-rpc': 'Cross-language RPC test',
    'tui-smoke-stdio': 'TUI stdio smoke',
    'tui-smoke-socket': 'TUI socket smoke',
    'tui-smoke-tcp': 'TUI tcp smoke',
    'sea-build': 'SEA full build',
    'sea-smoke': 'Native smoke',
    'typecheck': 'Workspace typecheck',
  }

  for stepId, tableLabel of mapping
    result := report.steps.find(s => s.id === stepId)
    status := result ? statusToAdr(result.status) : 'BLOCKED'
    text := replaceTableCell(text, tableLabel, status)

  writeFileSync(adrPath, text)
```

`statusToAdr` mapping [C:INFERRED]:
- `passed` → `PASS`
- `failed` → `FAIL`
- `skipped` → `BLOCKED`

---

## 4. Error Handling

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| Node version < 24.15.0 | Throw before any subprocess; exit 1 with message. | None — user must switch Node. | Rerun with Node 24.15.0+. |
| Binary not found (`ODY_HOST_BINARY_PATH` invalid) | Throw in `buildContext`; exit 1. | None. | Set valid path or run `pnpm run build:host`. |
| Step timeout | `runStep` returns `failed` with `errorMessage`; kill process tree. | Partial report is written; ADR marked FAIL. | Increase timeout or fix slow step. |
| Step non-zero exit | `runStep` returns `failed`; fail-fast stops registry. | Remaining steps are skipped and marked in summary. | Fix failing step; rerun. |
| Redaction pattern false negative | Secret may leak into report. | Manual review of CI artifacts. | Update `redact` patterns. |

---

## 5. Call-Site Integration

### 5.1 `scripts/verify-phase-a3.mjs` is invoked from package.json

File: `package.json` (root), line 13 area. [C:INFERRED]

```json
"scripts": {
  "verify:phase-a3": "node scripts/verify-phase-a3.mjs",
  "verify:phase-a3:local": "node scripts/verify-phase-a3.mjs --keep-temp"
}
```

### 5.2 CI workflow calls the script

File: `.github/workflows/rust-host.yml` (covered in Part 4). [C:USER]

```yaml
- name: Phase A3 verification
  run: pnpm run verify:phase-a3
  env:
    ODY_HOST_BINARY_PATH: ${{ github.workspace }}/rust-ody/target/release/ody-host
```

---

## 6. Test Plan

| Test | Assertion |
|---|---|
| `ensureNodeVersion` rejects Node 22 | `ensureNodeVersion('24.15.0')` with `process.version = 'v22.0.0'` throws. |
| `ensureNodeVersion` accepts Node 24.15.0 | Does not throw. |
| `redact` masks API key | Input `"api_key":"sk-abc123"` → output contains `***` and not `abc123`. |
| `redact` preserves non-secret text | Input `"model":"gpt-4o-mini"` → unchanged. |
| `runStep` returns failed on exit 1 | `exitCode === 1` and `status === 'failed'`. |
| `runStep` kills orphan process on timeout | Mock slow process receives SIGTERM. |
| `buildSummary` counts correctly | 5 passed + 1 failed + 1 skipped → `overallStatus === 'partial'`. |
| `updateAdr` updates correct cell | ADR text contains `\| TUI stdio smoke \| PASS \|`. |
| End-to-end dry run | `node scripts/verify-phase-a3.mjs --skip-sea` writes valid JSON report. |

Done criteria [C:USER]:
- `pnpm run verify:phase-a3` exits 0 when all steps pass.
- `.ody-code/reports/phase-a3-report.json` is valid JSON and contains all required fields.
- `docs/designs/rust-host-reversal-adr.md` Prototype Results tables are updated.

---

## 7. Local Notes

- The script should be implemented as an ES module (`*.mjs`) to match the repository convention. [C:INFERRED]
- Keep the script dependency-free beyond Node built-ins; do not add `zx`, `execa`, etc. [C:INFERRED]
- Raw logs can be written to `.ody-code/reports/phase-a3-logs/<stepId>.log` for debugging, but the JSON report only contains redacted summaries. [C:INFERRED]
