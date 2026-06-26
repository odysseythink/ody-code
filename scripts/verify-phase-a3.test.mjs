// scripts/verify-phase-a3.test.mjs
import { describe, it } from 'node:test';
import assert from 'node:assert';
import { writeFileSync, readFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { ensureNodeVersion, parseConfig, redact, executeCommand, buildStepRegistry, buildSummary, buildMetadata, buildEnvironment, updateAdr, runVerification } from './verify-phase-a3.mjs';

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

describe('redact', () => {
  it('masks JSON api_key values', () => {
    const input = '{"api_key":"sk-abc123"}';
    const output = redact(input);
    assert(output.includes('***'));
    assert(!output.includes('abc123'));
    assert(output.includes('"api_key":"sk-a'));
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
