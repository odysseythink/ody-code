import { describe, expect, it, vi } from 'vitest';
import type { Kaos, KaosProcess } from '@odysseythink/kaos';
import type { Readable, Writable } from 'node:stream';
import { createFakeKaos } from '../tools/fixtures/fake-kaos';
import { E2ETestExecutor } from '#/e2e-testing/executor';
import type { TestFile, TestSuiteResult } from '#/e2e-testing/types';
import type { ResolvedE2EConfig } from '#/e2e-testing/config';

function fakeKaos(): Kaos {
  return createFakeKaos({
    mkdir: vi.fn().mockResolvedValue(undefined),
    writeText: vi.fn().mockResolvedValue(42),
    readText: vi.fn().mockResolvedValue('{}'),
    stat: vi.fn().mockRejectedValue(new Error('ENOENT')),
    exec: vi.fn().mockResolvedValue({
      stdin: { end: vi.fn(), write: vi.fn() } as unknown as Writable,
      stdout: { on: vi.fn() } as unknown as Readable,
      stderr: { on: vi.fn() } as unknown as Readable,
      pid: 1,
      exitCode: 0,
      wait: vi.fn().mockResolvedValue(0),
      kill: vi.fn().mockResolvedValue(undefined),
    } as KaosProcess),
  });
}

const defaultConfig: ResolvedE2EConfig = {
  enabled: true, strategy: 'smart', criticalTools: [], failurePolicy: 'warn',
  maxConcurrency: 4, testTimeout: 30000,
  reportDir: '.ody-code/test-reports', generatedTestDir: '.ody-code/test-generated/e2e',
};

describe('E2ETestExecutor', () => {
  it('returns empty result for empty test files', async () => {
    const kaos = fakeKaos();
    const writeText = vi.fn().mockResolvedValue(42);
    (kaos as any).writeText = writeText;
    const executor = new E2ETestExecutor(kaos, defaultConfig);
    const result = await executor.execute([], '/tmp');
    expect(result.passed).toBe(0);
    expect(result.failed).toBe(0);
    expect(result.skipped).toBe(0);
    expect(result.suites).toEqual([]);
    expect(result.reportPath).not.toBe(defaultConfig.reportDir);
    expect(result.reportPath).toMatch(/e2e-report-/);
    expect(writeText).toHaveBeenCalled();
  });

  it('writes test file and runs vitest', async () => {
    const kaos = fakeKaos();
    const writeText = vi.fn().mockResolvedValue(42);
    (kaos as any).writeText = writeText;
    const executor = new E2ETestExecutor(kaos, defaultConfig);
    const testFile: TestFile = { relativePath: 'x.test.ts', content: 'it("ok", () => {})' };
    await executor.execute([testFile], '/tmp');
    expect(writeText).toHaveBeenCalled();
    expect(kaos.exec).toHaveBeenCalled();
  });

  it('respects maxConcurrency by chunking', async () => {
    const kaos = fakeKaos();
    const executor = new E2ETestExecutor(kaos, { ...defaultConfig, maxConcurrency: 2 });
    const files: TestFile[] = Array.from({ length: 5 }, (_, i) => ({
      relativePath: `t${i}.test.ts`,
      content: `it("test ${i}", () => {})`,
    }));
    await executor.execute(files, '/tmp');
    // With 5 files and maxConcurrency 2, chunk size 2 → 3 vitest invocations.
    const vitestCalls = (kaos.exec as ReturnType<typeof vi.fn>).mock.calls.filter(
      (call: string[]) => call[0] === 'pnpm' && call[1] === 'vitest',
    );
    expect(vitestCalls).toHaveLength(3);
  });
});

describe('E2ETestExecutor edge cases', () => {
  it('handles long failure messages without crashing', async () => {
    const longMsg = 'x'.repeat(300);
    const kaos = fakeKaos();
    const vitestJson = {
      testResults: [{
        name: 'f.test.ts', status: 'failed',
        startTime: 0, endTime: 100,
        assertionResults: [{
          title: 't1', status: 'failed',
          failureMessages: [longMsg],
        }],
      }],
    };
    (kaos as any).readText = vi.fn().mockResolvedValue(JSON.stringify(vitestJson));
    (kaos as any).stat = vi.fn().mockResolvedValue({ stMode: 0, stSize: 100, stMtime: 0 });
    const executor = new E2ETestExecutor(kaos, defaultConfig);
    const tf: TestFile = { relativePath: 'f.test.ts', content: 'it("ok", () => {})' };
    const result = await executor.execute([tf], '/tmp');
    expect(typeof result.summary).toBe('string');
  }, 10000);

  it('handles vitest crash without JSON output', async () => {
    const kaos = fakeKaos();
    (kaos as any).readText = vi.fn().mockRejectedValue(new Error('ENOENT'));
    const executor = new E2ETestExecutor(kaos, defaultConfig);
    const tf: TestFile = { relativePath: 'crash.test.ts', content: 'bad syntax' };
    const result = await executor.execute([tf], '/tmp');
    expect(result.failed).toBeGreaterThanOrEqual(1);
  });

  it('cleans up temporary vitest output files after parsing', async () => {
    const kaos = fakeKaos();
    const vitestJson = {
      testResults: [{
        name: 'ok.test.ts', status: 'passed',
        startTime: 0, endTime: 100,
        assertionResults: [{ title: 't1', status: 'passed', failureMessages: [] }],
      }],
    };
    (kaos as any).readText = vi.fn().mockResolvedValue(JSON.stringify(vitestJson));
    const executor = new E2ETestExecutor(kaos, defaultConfig);
    const tf: TestFile = { relativePath: 'ok.test.ts', content: 'it("ok", () => {})' };
    await executor.execute([tf], '/tmp');
    const rmCalls = (kaos.exec as ReturnType<typeof vi.fn>).mock.calls.filter(
      (call: string[]) => call[0] === 'rm' && call[1]?.includes('.vitest-output-'),
    );
    expect(rmCalls.length).toBeGreaterThanOrEqual(1);
  });
});

describe('parseVitestJson (via integration)', () => {
  it('parses empty testResults as empty suites', async () => {
    const kaos = fakeKaos();
    (kaos as any).readText = vi.fn().mockResolvedValue(JSON.stringify({ testResults: [] }));
    const executor = new E2ETestExecutor(kaos, defaultConfig);
    const tf: TestFile = { relativePath: 'x.test.ts', content: 'it("ok", () => {})' };
    const result = await executor.execute([tf], '/tmp');
    expect(result.suites).toEqual([]);
  });
});
