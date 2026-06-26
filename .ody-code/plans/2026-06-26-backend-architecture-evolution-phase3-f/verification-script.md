# Part 1 — Verification Script

> Scope: `scripts/verify-phase-a3.mjs`, its unit tests, the JSON report, redaction, and ADR update.

---

### Task A1: Node version check and `VerificationConfig` parsing

**Depends on:** none

**Files:**
- Create: `scripts/verify-phase-a3.mjs`
- Create: `scripts/verify-phase-a3.test.mjs`

**Steps:**

- [ ] Write the failing test.

```js
// scripts/verify-phase-a3.test.mjs
import { describe, it } from 'node:test';
import assert from 'node:assert';
import { ensureNodeVersion, parseConfig } from './verify-phase-a3.mjs';

describe('ensureNodeVersion', () => {
  it('rejects Node older than 24.15.0', () => {
    assert.throws(
      () => ensureNodeVersion('24.15.0', 'v22.0.0'),
      /Node 24\.15\.0\+ required/,
    );
  });

  it('accepts Node 24.15.0', () => {
    assert.doesNotThrow(() => ensureNodeVersion('24.15.0', 'v24.15.0'));
  });

  it('accepts Node 24.16.0', () => {
    assert.doesNotThrow(() => ensureNodeVersion('24.15.0', 'v24.16.0'));
  });
});

describe('parseConfig', () => {
  it('resolves default paths and timeouts', () => {
    const config = parseConfig([], '/workspace');
    assert.strictEqual(config.hostBinaryPath, '/workspace/rust-ody/target/release/ody-host');
    assert.strictEqual(config.reportDir, '/workspace/.ody-code/reports');
    assert.strictEqual(config.defaultTimeoutMs, 300_000);
    assert.deepStrictEqual(config.stepTimeoutsMs, {});
    assert.strictEqual(config.skipSea, false);
    assert.strictEqual(config.keepTemp, false);
  });

  it('reads ODY_HOST_BINARY_PATH env override', () => {
    const config = parseConfig([], '/workspace', { ODY_HOST_BINARY_PATH: '/custom/ody-host' });
    assert.strictEqual(config.hostBinaryPath, '/custom/ody-host');
  });

  it('reads ODY_CODE_REPORT_DIR env override', () => {
    const config = parseConfig([], '/workspace', { ODY_CODE_REPORT_DIR: '/custom/reports' });
    assert.strictEqual(config.reportDir, '/custom/reports');
  });

  it('parses per-step timeouts from ODY_CODE_STEP_TIMEOUTS', () => {
    const config = parseConfig([], '/workspace', { ODY_CODE_STEP_TIMEOUTS: 'sea-build:600000,typecheck:120000' });
    assert.strictEqual(config.stepTimeoutsMs['sea-build'], 600_000);
    assert.strictEqual(config.stepTimeoutsMs['typecheck'], 120_000);
  });

  it('parses --skip-sea, --keep-temp, and win32 default skipSea', () => {
    const config = parseConfig(['--skip-sea', '--keep-temp'], '/workspace', {}, 'linux');
    assert.strictEqual(config.skipSea, true);
    assert.strictEqual(config.keepTemp, true);
    const winConfig = parseConfig([], '/workspace', {}, 'win32');
    assert.strictEqual(winConfig.skipSea, true);
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
node --test scripts/verify-phase-a3.test.mjs
```

Expected failure: `Error [ERR_MODULE_NOT_FOUND]: Cannot find module ... verify-phase-a3.mjs` or `ensureNodeVersion is not a function`.

- [ ] Write the minimal implementation.

```js
// scripts/verify-phase-a3.mjs
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

export function ensureNodeVersion(minVersion, currentVersion = process.version) {
  const current = parseSemver(currentVersion);
  const minimum = parseSemver(minVersion);
  if (
    current.major < minimum.major ||
    (current.major === minimum.major && current.minor < minimum.minor) ||
    (current.major === minimum.major && current.minor === minimum.minor && current.patch < minimum.patch)
  ) {
    throw new Error(`Node ${minVersion}+ required, found ${currentVersion}`);
  }
}

function parseSemver(version) {
  const match = version.match(/^v?(\d+)\.(\d+)\.(\d+)/);
  if (!match) throw new Error(`Invalid version string: ${version}`);
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
  };
}

export function parseConfig(argv, workspaceRoot, env = process.env, platform = process.platform) {
  const args = new Set(argv);
  const hostBinaryPath = env.ODY_HOST_BINARY_PATH
    ? resolve(env.ODY_HOST_BINARY_PATH)
    : resolve(workspaceRoot, 'rust-ody', 'target', 'release', 'ody-host');
  const reportDir = env.ODY_CODE_REPORT_DIR
    ? resolve(env.ODY_CODE_REPORT_DIR)
    : resolve(workspaceRoot, '.ody-code', 'reports');
  const defaultTimeoutMs = Number(env.ODY_CODE_DEFAULT_TIMEOUT_MS ?? '300000');
  const stepTimeoutsMs = parseStepTimeouts(env.ODY_CODE_STEP_TIMEOUTS);
  const skipSea = args.has('--skip-sea') || env.ODY_CODE_SKIP_SEA === '1' || platform === 'win32';
  const keepTemp = args.has('--keep-temp') || env.ODY_CODE_KEEP_TEMP === '1';
  return { hostBinaryPath, reportDir, defaultTimeoutMs, stepTimeoutsMs, skipSea, keepTemp };
}

function parseStepTimeouts(value) {
  if (!value) return {};
  const result = {};
  for (const pair of value.split(',')) {
    const [id, ms] = pair.split(':');
    if (id && ms) result[id.trim()] = Number(ms.trim());
  }
  return result;
}
```

- [ ] Run it and verify it PASSES.

```bash
node --test scripts/verify-phase-a3.test.mjs
```

Expected: all 7 assertions pass.

- [ ] Commit.

```bash
git add scripts/verify-phase-a3.mjs scripts/verify-phase-a3.test.mjs
git commit -m "feat(scripts): scaffold verify-phase-a3 config and Node version check"
```

---

### Task A2: Secret redaction

**Depends on:** Task A1

**Files:**
- Modify: `scripts/verify-phase-a3.mjs`
- Modify: `scripts/verify-phase-a3.test.mjs`

**Steps:**

- [ ] Write the failing test.

Append to `scripts/verify-phase-a3.test.mjs`:

```js
import { redact } from './verify-phase-a3.mjs';

describe('redact', () => {
  it('masks JSON api_key values', () => {
    const input = '{"api_key":"sk-abc123"}';
    const output = redact(input);
    assert(output.includes('***'));
    assert(!output.includes('abc123'));
    assert(output.includes('"api_key":"sk-ab'));
  });

  it('masks Authorization Bearer tokens', () => {
    const input = 'headers: { authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9 }';
    const output = redact(input);
    assert(output.includes('***'));
    assert(!output.includes('eyJhbGci'));
    assert(output.includes('authorization: bearer eyJh'));
  });

  it('masks inline api_key= values', () => {
    const input = 'curl -H api_key=supersecret';
    const output = redact(input);
    assert(output.includes('***'));
    assert(!output.includes('supersecret'));
    assert(output.includes('api_key=supe'));
  });

  it('masks inline api-key= values preserving original key', () => {
    const input = 'x-api-key=shhh';
    const output = redact(input);
    assert(output.includes('api-key=shhh***') || output.includes('api-key=shh***'));
  });

  // Must-survive cases
  it('preserves non-secret JSON', () => {
    const input = '{"model":"gpt-4o-mini","temperature":0.7}';
    assert.strictEqual(redact(input), input);
  });

  it('preserves short api_key values that the regex deliberately ignores', () => {
    const input = '{"api_key":"ab"}';
    assert.strictEqual(redact(input), input);
  });

  it('preserves a log line that merely contains the word secret', () => {
    const input = 'this is a secret not in a JSON value';
    assert.strictEqual(redact(input), input);
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
node --test scripts/verify-phase-a3.test.mjs
```

Expected failure: `redact is not a function`.

- [ ] Write the minimal implementation.

Append to `scripts/verify-phase-a3.mjs`:

```js
export function redact(text) {
  return text
    .replace(/"api_key"\s*:\s*"([^"]{4,})"/gi, (_, value) => `"api_key":"${value.slice(0, 4)}***"`)
    .replace(/"access_token"\s*:\s*"([^"]{4,})"/gi, (_, value) => `"access_token":"${value.slice(0, 4)}***"`)
    .replace(/"password"\s*:\s*"([^"]*)"/gi, (_, value) => `"password":"${value.slice(0, 4)}***"`)
    .replace(/"secret"\s*:\s*"([^"]{4,})"/gi, (_, value) => `"secret":"${value.slice(0, 4)}***"`)
    .replace(/authorization:\s*bearer\s+(\S+)/gi, (_, token) => `authorization: bearer ${token.slice(0, 4)}***`)
    .replace(/(api[_-]?key)([=:])\s*(\S+)/gi, (_, key, sep, value) => `${key}${sep}${value.slice(0, 4)}***`);
}
```

- [ ] Run it and verify it PASSES.

```bash
node --test scripts/verify-phase-a3.test.mjs
```

Expected: all redaction assertions pass.

- [ ] Commit.

```bash
git add scripts/verify-phase-a3.mjs scripts/verify-phase-a3.test.mjs
git commit -m "feat(scripts): redact secrets in verify-phase-a3 output"
```

---

### Task A3: Step runner with timeout and cleanup

**Depends on:** Task A2

**Files:**
- Modify: `scripts/verify-phase-a3.mjs`
- Modify: `scripts/verify-phase-a3.test.mjs`

**Steps:**

- [ ] Write the failing test.

Append to `scripts/verify-phase-a3.test.mjs`:

```js
import { executeCommand } from './verify-phase-a3.mjs';

describe('executeCommand', () => {
  it('returns passed for exit 0 and redacts stdout', async () => {
    const result = await executeCommand('node', ['-e', 'console.log("hi")'], {
      cwd: '.',
      env: process.env,
      timeoutMs: 5000,
    });
    assert.strictEqual(result.status, 'passed');
    assert.strictEqual(result.exitCode, 0);
    assert.strictEqual(result.stdoutRedacted, 'hi\n');
  });

  it('returns failed for non-zero exit', async () => {
    const result = await executeCommand('node', ['-e', 'process.exit(1)'], {
      cwd: '.',
      env: process.env,
      timeoutMs: 5000,
    });
    assert.strictEqual(result.status, 'failed');
    assert.strictEqual(result.exitCode, 1);
  });

  it('returns failed and kills the process on timeout', async () => {
    const start = Date.now();
    const result = await executeCommand('node', ['-e', 'setTimeout(()=>{}, 60000)'], {
      cwd: '.',
      env: process.env,
      timeoutMs: 100,
    });
    const elapsed = Date.now() - start;
    assert.strictEqual(result.status, 'failed');
    assert.ok(elapsed < 2000, `timeout took ${elapsed}ms`);
    assert.ok(result.signal === 'SIGTERM' || (result.errorMessage ?? '').includes('timeout'));
  });

  it('redacts secrets in captured output', async () => {
    const result = await executeCommand('node', ['-e', 'console.log({"api_key":"sk-leaked"})'], {
      cwd: '.',
      env: process.env,
      timeoutMs: 5000,
    });
    assert(result.stdoutRedacted.includes('***'));
    assert(!result.stdoutRedacted.includes('leaked'));
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
node --test scripts/verify-phase-a3.test.mjs
```

Expected failure: `executeCommand is not a function`.

- [ ] Write the minimal implementation.

Append to `scripts/verify-phase-a3.mjs`:

```js
import { spawn } from 'node:child_process';

export function executeCommand(command, args, options) {
  return new Promise((resolve) => {
    const startedAt = Date.now();
    const child = spawn(command, args, {
      cwd: options.cwd,
      env: options.env,
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    const stdoutChunks = [];
    const stderrChunks = [];
    child.stdout.on('data', (chunk) => stdoutChunks.push(chunk));
    child.stderr.on('data', (chunk) => stderrChunks.push(chunk));

    let timedOut = false;
    const timeout = setTimeout(() => {
      timedOut = true;
      child.kill('SIGTERM');
      // Escalate to SIGKILL if the child refuses to die.
      setTimeout(() => {
        if (!child.killed) child.kill('SIGKILL');
      }, 5000).unref();
    }, options.timeoutMs);

    const finish = (fields) => {
      clearTimeout(timeout);
      resolve({
        status: fields.status,
        exitCode: fields.exitCode ?? null,
        signal: fields.signal ?? null,
        durationMs: Date.now() - startedAt,
        stdoutRedacted: redact(Buffer.concat(stdoutChunks).toString()),
        stderrRedacted: redact(Buffer.concat(stderrChunks).toString()),
        ...(fields.errorMessage ? { errorMessage: fields.errorMessage } : {}),
      });
    };

    child.on('error', (error) => {
      finish({ status: 'failed', errorMessage: error.message });
    });

    child.on('exit', (exitCode, signal) => {
      if (timedOut) {
        finish({ status: 'failed', signal, errorMessage: `Step timed out after ${options.timeoutMs}ms` });
        return;
      }
      const status = exitCode === 0 ? 'passed' : 'failed';
      finish({ status, exitCode, signal });
    });
  });
}
```

- [ ] Run it and verify it PASSES.

```bash
node --test scripts/verify-phase-a3.test.mjs
```

Expected: all `executeCommand` assertions pass.

- [ ] Commit.

```bash
git add scripts/verify-phase-a3.mjs scripts/verify-phase-a3.test.mjs
git commit -m "feat(scripts): add verify-phase-a3 step runner with timeout"
```

---

### Task A4: StepRegistry, report aggregation, and JSON writer

**Depends on:** Task A3

**Files:**
- Modify: `scripts/verify-phase-a3.mjs`
- Modify: `scripts/verify-phase-a3.test.mjs`

**Steps:**

- [ ] Write the failing test.

Append to `scripts/verify-phase-a3.test.mjs`:

```js
import { buildStepRegistry, buildSummary, buildMetadata, buildEnvironment } from './verify-phase-a3.mjs';

describe('buildStepRegistry', () => {
  it('includes all steps when skipSea is false', () => {
    const steps = buildStepRegistry({ skipSea: false });
    assert.deepStrictEqual(
      steps.map((s) => s.id),
      ['rust-test', 'cross-lang-rpc', 'tui-smoke-stdio', 'tui-smoke-socket', 'tui-smoke-tcp', 'sea-build', 'sea-smoke', 'typecheck'],
    );
  });

  it('skips sea steps when skipSea is true', () => {
    const steps = buildStepRegistry({ skipSea: true });
    assert.ok(!steps.some((s) => s.id === 'sea-build'));
    assert.ok(!steps.some((s) => s.id === 'sea-smoke'));
  });
});

describe('buildSummary', () => {
  it('marks partial when some pass and some fail', () => {
    const summary = buildSummary(
      [
        { status: 'passed' },
        { status: 'passed' },
        { status: 'failed' },
        { status: 'skipped' },
      ],
      1000,
    );
    assert.strictEqual(summary.overallStatus, 'partial');
    assert.strictEqual(summary.passedCount, 2);
    assert.strictEqual(summary.failedCount, 1);
    assert.strictEqual(summary.skippedCount, 1);
    assert.strictEqual(summary.totalDurationMs, 1000);
  });

  it('marks passed only when every step passed', () => {
    const summary = buildSummary([{ status: 'passed' }, { status: 'passed' }], 100);
    assert.strictEqual(summary.overallStatus, 'passed');
  });

  it('marks failed when every step failed', () => {
    const summary = buildSummary([{ status: 'failed' }, { status: 'failed' }], 100);
    assert.strictEqual(summary.overallStatus, 'failed');
  });
});

describe('buildMetadata', () => {
  it('includes node version, platform, arch, host binary path', () => {
    const meta = buildMetadata({ hostBinaryPath: '/bin/ody-host' });
    assert.strictEqual(meta.nodeVersion, process.version);
    assert.strictEqual(meta.platform, process.platform);
    assert.strictEqual(meta.arch, process.arch);
    assert.strictEqual(meta.hostBinaryPath, '/bin/ody-host');
    assert.ok(/\d{4}-\d{2}-\d{2}T/.test(meta.timestamp));
  });
});

describe('buildEnvironment', () => {
  it('includes cwd, pnpm, cargo, rustc versions', () => {
    const env = buildEnvironment();
    assert.strictEqual(env.cwd, process.cwd());
    assert.ok(typeof env.pnpmVersion === 'string');
    assert.ok(typeof env.cargoVersion === 'string');
    assert.ok(typeof env.rustcVersion === 'string');
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
node --test scripts/verify-phase-a3.test.mjs
```

Expected failure: `buildStepRegistry is not a function`.

- [ ] Write the minimal implementation.

Append to `scripts/verify-phase-a3.mjs`:

```js
import { execSync } from 'node:child_process';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const DEFAULT_STEP_TIMEOUTS = {
  'tui-smoke-stdio': 60_000,
  'tui-smoke-socket': 60_000,
  'tui-smoke-tcp': 60_000,
};

export function resolveTimeout(stepId, config) {
  return config.stepTimeoutsMs[stepId] ?? DEFAULT_STEP_TIMEOUTS[stepId] ?? config.defaultTimeoutMs;
}

export async function buildContext(config) {
  const workspaceRoot = process.cwd();
  const tempHomeDir = await mkdtemp(join(tmpdir(), 'phase-a3-'));
  const env = { ...process.env, ODY_HOME: tempHomeDir };
  return { config, workspaceRoot, tempHomeDir, env };
}

export function buildStepRegistry(config) {
  const steps = [
    { id: 'rust-test', name: 'Rust host unit tests' },
    { id: 'cross-lang-rpc', name: 'Cross-language RPC test' },
    { id: 'tui-smoke-stdio', name: 'TUI stdio smoke' },
    { id: 'tui-smoke-socket', name: 'TUI socket smoke' },
    { id: 'tui-smoke-tcp', name: 'TUI tcp smoke' },
    { id: 'sea-build', name: 'SEA full build' },
    { id: 'sea-smoke', name: 'Native smoke' },
    { id: 'typecheck', name: 'Workspace typecheck' },
  ];
  return steps
    .filter((s) => !config.skipSea || (s.id !== 'sea-build' && s.id !== 'sea-smoke'))
    .map((s) => ({
      ...s,
      run: (ctx) => runStepById(s.id, ctx),
    }));
}

async function runStepById(id, ctx) {
  const timeoutMs = resolveTimeout(id, ctx.config);
  if (id === 'rust-test') {
    return wrapResult(await executeCommand('pnpm', ['run', 'test:host'], { cwd: ctx.workspaceRoot, env: ctx.env, timeoutMs }), 'pnpm', ['run', 'test:host'], ctx.workspaceRoot);
  }
  if (id === 'cross-lang-rpc') {
    return wrapResult(await executeCommand('pnpm', ['vitest', 'run', 'packages/node-sdk/test/rust-host-connect.test.ts'], { cwd: ctx.workspaceRoot, env: ctx.env, timeoutMs }), 'pnpm', ['vitest', 'run', 'packages/node-sdk/test/rust-host-connect.test.ts'], ctx.workspaceRoot);
  }
  if (id.startsWith('tui-smoke-')) {
    return runTuiSmoke(id.replace('tui-smoke-', ''), ctx, timeoutMs);
  }
  if (id === 'sea-build') {
    return wrapResult(await executeCommand('pnpm', ['--filter', 'ody-code', 'run', 'build:native:sea'], { cwd: ctx.workspaceRoot, env: ctx.env, timeoutMs }), 'pnpm', ['--filter', 'ody-code', 'run', 'build:native:sea'], ctx.workspaceRoot);
  }
  if (id === 'sea-smoke') {
    return wrapResult(await executeCommand('pnpm', ['--filter', 'ody-code', 'run', 'test:native:smoke'], { cwd: ctx.workspaceRoot, env: ctx.env, timeoutMs }), 'pnpm', ['--filter', 'ody-code', 'run', 'test:native:smoke'], ctx.workspaceRoot);
  }
  if (id === 'typecheck') {
    return wrapResult(await executeCommand('pnpm', ['-r', 'typecheck'], { cwd: ctx.workspaceRoot, env: ctx.env, timeoutMs }), 'pnpm', ['-r', 'typecheck'], ctx.workspaceRoot);
  }
  throw new Error(`Unknown step id: ${id}`);
}

function wrapResult(result, command, args, cwd) {
  return { ...result, command, args, cwd };
}

async function runTuiSmoke(transport, ctx, timeoutMs) {
  const baseArgs = ['--filter', 'ody-code', 'run', 'dev:cli-only', '--', '--host=rust', '--smoke-test'];
  if (transport === 'stdio') {
    baseArgs.push('--host-stdio');
  } else if (transport === 'socket') {
    baseArgs.push('--host-socket', join(ctx.tempHomeDir, 'ody-smoke.sock'));
  } else if (transport === 'tcp') {
    const basePort = 19090;
    const maxAttempts = 10;
    let lastResult;
    for (let offset = 0; offset < maxAttempts; offset += 1) {
      const port = basePort + offset;
      const args = [...baseArgs, '--host-tcp', `127.0.0.1:${port}`];
      lastResult = wrapResult(await executeCommand('pnpm', args, { cwd: ctx.workspaceRoot, env: ctx.env, timeoutMs }), 'pnpm', args, ctx.workspaceRoot);
      if (lastResult.status === 'passed') return lastResult;
      const combined = `${lastResult.stdoutRedacted}\n${lastResult.stderrRedacted}`;
      if (!/eaddrinuse|address already in use/i.test(combined)) return lastResult;
    }
    return lastResult;
  }
  return wrapResult(await executeCommand('pnpm', baseArgs, { cwd: ctx.workspaceRoot, env: ctx.env, timeoutMs }), 'pnpm', baseArgs, ctx.workspaceRoot);
}

export function buildMetadata(config) {
  return {
    nodeVersion: process.version,
    platform: process.platform,
    arch: process.arch,
    timestamp: new Date().toISOString(),
    hostBinaryPath: config.hostBinaryPath,
  };
}

export function buildEnvironment() {
  return {
    cwd: process.cwd(),
    pnpmVersion: execSync('pnpm --version', { encoding: 'utf-8' }).trim(),
    cargoVersion: execSync('cargo --version', { encoding: 'utf-8' }).trim(),
    rustcVersion: execSync('rustc --version', { encoding: 'utf-8' }).trim(),
  };
}

export function buildSummary(results, totalDurationMs) {
  const passedCount = results.filter((r) => r.status === 'passed').length;
  const failedCount = results.filter((r) => r.status === 'failed').length;
  const skippedCount = results.filter((r) => r.status === 'skipped').length;
  let overallStatus = 'passed';
  if (failedCount > 0) overallStatus = results.length === failedCount ? 'failed' : 'partial';
  return { overallStatus, passedCount, failedCount, skippedCount, totalDurationMs };
}

export async function runVerification(config) {
  ensureNodeVersion('24.15.0');
  if (!existsSync(config.hostBinaryPath)) {
    throw new Error(`ody-host binary not found at ${config.hostBinaryPath}. Build with "pnpm run build:host" or set ODY_HOST_BINARY_PATH.`);
  }
  const ctx = await buildContext(config);
  const steps = buildStepRegistry(config);
  const results = [];
  const startedAt = Date.now();
  try {
    for (const step of steps) {
      const base = await step.run(ctx);
      const result = { ...base, id: step.id, name: step.name };
      results.push(result);
      if (result.status === 'failed') break;
    }
  } finally {
    if (!config.keepTemp) {
      await rm(ctx.tempHomeDir, { recursive: true, force: true }).catch(() => {});
    }
  }
  return {
    metadata: buildMetadata(config),
    environment: buildEnvironment(),
    steps: results,
    summary: buildSummary(results, Date.now() - startedAt),
  };
}
```

Add to top of file if not already present:

```js
import { rm } from 'node:fs/promises';
```

- [ ] Run it and verify it PASSES.

```bash
node --test scripts/verify-phase-a3.test.mjs
```

Expected: `buildStepRegistry`, `buildSummary`, `buildMetadata`, `buildEnvironment` tests pass.

- [ ] Commit.

```bash
git add scripts/verify-phase-a3.mjs scripts/verify-phase-a3.test.mjs
git commit -m "feat(scripts): add verify-phase-a3 step registry and report aggregation"
```

---

### Task A5: ADR updater

**Depends on:** Task A4

**Files:**
- Modify: `scripts/verify-phase-a3.mjs`
- Modify: `scripts/verify-phase-a3.test.mjs`

**Steps:**

- [ ] Write the failing test.

Append to `scripts/verify-phase-a3.test.mjs`:

```js
import { updateAdr } from './verify-phase-a3.mjs';

describe('updateAdr', () => {
  it('updates PASS/FAIL/BLOCKED cells', () => {
    const adr = `| Criterion | Result | Notes |
|---|---|---|
| cargo test -p ody-host | PASS | notes |
| Cross-language RPC test | PASS | notes |
| TUI stdio smoke | PENDING | notes |
| TUI socket smoke | PENDING | notes |
| TUI tcp smoke | PENDING | notes |
| SEA full build | PENDING | notes |
| Native smoke | PENDING | notes |
| Workspace typecheck | PENDING | notes |`;

    const report = {
      steps: [
        { id: 'rust-test', status: 'passed' },
        { id: 'cross-lang-rpc', status: 'passed' },
        { id: 'tui-smoke-stdio', status: 'passed' },
        { id: 'tui-smoke-socket', status: 'failed' },
        { id: 'tui-smoke-tcp', status: 'skipped' },
        { id: 'sea-build', status: 'failed' },
        { id: 'sea-smoke', status: 'skipped' },
        { id: 'typecheck', status: 'passed' },
      ],
    };

    const updated = updateAdr(adr, report);
    assert.match(updated, /\| TUI stdio smoke \| PASS \|/);
    assert.match(updated, /\| TUI socket smoke \| FAIL \|/);
    assert.match(updated, /\| TUI tcp smoke \| BLOCKED \|/);
    assert.match(updated, /\| SEA full build \| FAIL \|/);
    assert.match(updated, /\| Native smoke \| BLOCKED \|/);
    assert.match(updated, /\| Workspace typecheck \| PASS \|/);
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
node --test scripts/verify-phase-a3.test.mjs
```

Expected failure: `updateAdr is not a function`.

- [ ] Write the minimal implementation.

Append to `scripts/verify-phase-a3.mjs`:

```js
const ADR_STEP_LABELS = {
  'rust-test': 'cargo test -p ody-host',
  'cross-lang-rpc': 'Cross-language RPC test',
  'tui-smoke-stdio': 'TUI stdio smoke',
  'tui-smoke-socket': 'TUI socket smoke',
  'tui-smoke-tcp': 'TUI tcp smoke',
  'sea-build': 'SEA full build',
  'sea-smoke': 'Native smoke',
  'typecheck': 'Workspace typecheck',
};

export function updateAdr(text, report) {
  let result = text;
  for (const [stepId, label] of Object.entries(ADR_STEP_LABELS)) {
    const step = report.steps.find((s) => s.id === stepId);
    const status = step ? statusToAdr(step.status) : 'BLOCKED';
    const escaped = label.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    result = result.replace(
      new RegExp(`(\\|\\s*${escaped}\\s*\\|\\s*)[^|]*(\\s*\\|)`, 'g'),
      `$1${status}$2`,
    );
  }
  return result;
}

function statusToAdr(status) {
  if (status === 'passed') return 'PASS';
  if (status === 'failed') return 'FAIL';
  return 'BLOCKED';
}
```

- [ ] Run it and verify it PASSES.

```bash
node --test scripts/verify-phase-a3.test.mjs
```

Expected: ADR updater test passes.

- [ ] Commit.

```bash
git add scripts/verify-phase-a3.mjs scripts/verify-phase-a3.test.mjs
git commit -m "feat(scripts): update ADR tables from verify-phase-a3 report"
```

---

### Task A6: Main entry point, package scripts, and dry-run end-to-end

**Depends on:** Task A5

**Files:**
- Modify: `scripts/verify-phase-a3.mjs`
- Modify: `scripts/verify-phase-a3.test.mjs`
- Modify: `package.json` (root), lines 7–34 area

**Steps:**

- [ ] Write the failing test.

Append to `scripts/verify-phase-a3.test.mjs`:

```js
import { writeFileSync, readFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';

describe('main dry run', () => {
  it('writes a valid JSON report with --dry-run --skip-sea', async () => {
    const reportDir = mkdtempSync(join(tmpdir(), 'phase-a3-dry-'));
    const reportPath = join(reportDir, 'phase-a3-report.json');
    const report = await runVerification(parseConfig(['--dry-run', '--skip-sea'], process.cwd(), { ODY_CODE_REPORT_DIR: reportDir }));
    assert.strictEqual(report.summary.overallStatus, 'passed');
    const written = JSON.parse(readFileSync(reportPath, 'utf-8'));
    assert.strictEqual(written.summary.overallStatus, 'passed');
    assert.ok(written.metadata.nodeVersion);
    assert.ok(Array.isArray(written.steps));
    rmSync(reportDir, { recursive: true, force: true });
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
node --test scripts/verify-phase-a3.test.mjs
```

Expected failure: `--dry-run` is not handled, or report is not written.

- [ ] Write the minimal implementation.

Append to `scripts/verify-phase-a3.mjs`:

```js
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';

export async function main(argv) {
  const config = parseConfig(argv, process.cwd());
  ensureNodeVersion('24.15.0');
  if (!existsSync(config.hostBinaryPath) && !argv.includes('--dry-run')) {
    throw new Error(`ody-host binary not found at ${config.hostBinaryPath}. Build with "pnpm run build:host" or set ODY_HOST_BINARY_PATH.`);
  }
  const report = await runVerification(config);
  mkdirSync(config.reportDir, { recursive: true });
  const reportPath = join(config.reportDir, 'phase-a3-report.json');
  writeFileSync(reportPath, JSON.stringify(report, null, 2));
  updateAdrFile(report);
  process.exit(report.summary.overallStatus === 'passed' ? 0 : 1);
}

function updateAdrFile(report) {
  const adrPath = join(process.cwd(), 'docs', 'designs', 'rust-host-reversal-adr.md');
  if (!existsSync(adrPath)) return;
  const text = readFileSync(adrPath, 'utf-8');
  writeFileSync(adrPath, updateAdr(text, report));
}

// Allow --dry-run to bypass real steps for a deterministic smoke test of the harness.
const originalBuildStepRegistry = buildStepRegistry;
export function buildStepRegistry(config) {
  const args = process.argv.slice(2);
  if (args.includes('--dry-run')) {
    return [
      {
        id: 'dry-run',
        name: 'Dry run',
        run: () =>
          Promise.resolve({
            status: 'passed',
            command: 'echo',
            args: ['dry-run'],
            cwd: process.cwd(),
            exitCode: 0,
            signal: null,
            durationMs: 0,
            stdoutRedacted: '',
            stderrRedacted: '',
          }),
      },
    ];
  }
  return originalBuildStepRegistry(config);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
```

Wait: redefining `buildStepRegistry` after its first definition is fragile. Instead, modify the existing `buildStepRegistry` to check for `--dry-run` early:

```js
export function buildStepRegistry(config) {
  if (process.argv.slice(2).includes('--dry-run')) {
    return [{
      id: 'dry-run',
      name: 'Dry run',
      run: () => Promise.resolve({
        status: 'passed',
        command: 'echo',
        args: ['dry-run'],
        cwd: process.cwd(),
        exitCode: 0,
        signal: null,
        durationMs: 0,
        stdoutRedacted: '',
        stderrRedacted: '',
      }),
    }];
  }
  // ... existing registry
}
```

- [ ] Update root `package.json` scripts.

Modify `package.json` (root), add inside `"scripts"`:

```json
"verify:phase-a3": "node scripts/verify-phase-a3.mjs",
"verify:phase-a3:local": "node scripts/verify-phase-a3.mjs --keep-temp"
```

- [ ] Run it and verify it PASSES.

```bash
node --test scripts/verify-phase-a3.test.mjs
```

Expected: dry-run test passes.

Also run the script manually:

```bash
node scripts/verify-phase-a3.mjs --dry-run --skip-sea
```

Expected: exits 0 and writes `.ody-code/reports/phase-a3-report.json` containing `overallStatus: passed`.

- [ ] Commit.

```bash
git add scripts/verify-phase-a3.mjs scripts/verify-phase-a3.test.mjs package.json
git commit -m "feat(scripts): wire verify-phase-a3 main entry point and package scripts"
```

---

## Local Self-Review

- [ ] 1. Spec-coverage table (Part 1): `scripts/verify-phase-a3.mjs` (A1, A4, A6), JSON report (A4), ADR update (A5), Node version enforcement (A1), redaction (A2), fail-fast registry (A4) — all covered.
- [ ] 2. Placeholder scan: no TODO/TBD; every function is fully implemented in the steps above.
- [ ] 3. No phantom tasks: each task creates/modifies real files and ends with a passing test + commit.
- [ ] 4. Dependency soundness: A2 depends on A1, A3 on A2, A4 on A3, A5 on A4, A6 on A5; no forward references.
- [ ] 5. Caller & build soundness: Part 1 introduces no shared TypeScript signatures; it is a standalone Node script. The `package.json` script addition is a new identifier, not a rename.
- [ ] 6. Test-the-risk: redaction tests include must-survive cases that prove the regexes do not over-match; timeout test asserts the process is killed within 2s; dry-run test asserts the report file is written.
- [ ] 7. Type consistency: all object shapes (`VerificationConfig`, `StepResult`, `PhaseA3Report`) match the approved design interfaces.
