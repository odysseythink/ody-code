// scripts/verify-phase-a3.test.mjs
import { describe, it } from 'node:test';
import assert from 'node:assert';
import { ensureNodeVersion, parseConfig, redact, executeCommand } from './verify-phase-a3.mjs';

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
