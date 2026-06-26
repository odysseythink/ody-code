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
