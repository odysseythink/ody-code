import { describe, expect, it, vi, afterAll } from 'vitest';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import type { Kaos, KaosProcess } from '@odysseythink/kaos';
import type { Readable, Writable } from 'node:stream';
import { createFakeKaos } from './fixtures/fake-kaos';
import { E2ETestExecutor } from '../src/executor';
import { TypeScriptVitestGenerator } from '../src/generator';
import { computeCacheKey } from '../src/result-cache';
import type { E2ETestGenerator, TestFile, TestSuiteResult, E2EExecutionResult } from '../src/types';
import type { ResolvedE2EConfig } from '../src/config';

const tsGenerator = new TypeScriptVitestGenerator();

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
  recursiveAnalysisEnabled: true, maxRecursiveDepth: 3,
  cacheEnabled: false, cacheDir: '.ody-code/e2e-cache', cacheTtlDays: 7, cacheMaxEntries: 20,
};

describe('E2ETestExecutor', () => {
  it('returns empty result for empty test files', async () => {
    const kaos = fakeKaos();
    const writeText = vi.fn().mockResolvedValue(42);
    (kaos as any).writeText = writeText;
    const executor = new E2ETestExecutor(kaos, defaultConfig, tsGenerator);
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
    const executor = new E2ETestExecutor(kaos, defaultConfig, tsGenerator);
    const testFile: TestFile = { relativePath: 'x.test.ts', content: 'it("ok", () => {})' };
    await executor.execute([testFile], '/tmp');
    expect(writeText).toHaveBeenCalled();
    expect(kaos.exec).toHaveBeenCalled();
  });

  it('respects maxConcurrency by chunking', async () => {
    const kaos = fakeKaos();
    const executor = new E2ETestExecutor(kaos, { ...defaultConfig, maxConcurrency: 2 }, tsGenerator);
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
    const executor = new E2ETestExecutor(kaos, defaultConfig, tsGenerator);
    const tf: TestFile = { relativePath: 'f.test.ts', content: 'it("ok", () => {})' };
    const result = await executor.execute([tf], '/tmp');
    expect(typeof result.summary).toBe('string');
  }, 10000);

  it('handles vitest crash without JSON output', async () => {
    const kaos = fakeKaos();
    (kaos as any).readText = vi.fn().mockRejectedValue(new Error('ENOENT'));
    const executor = new E2ETestExecutor(kaos, defaultConfig, tsGenerator);
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
    const executor = new E2ETestExecutor(kaos, defaultConfig, tsGenerator);
    const tf: TestFile = { relativePath: 'ok.test.ts', content: 'it("ok", () => {})' };
    await executor.execute([tf], '/tmp');
    const rmCalls = (kaos.exec as ReturnType<typeof vi.fn>).mock.calls.filter(
      (call: string[]) => call[0] === 'rm' && call[1]?.includes('.vitest-output-'),
    );
    expect(rmCalls.length).toBeGreaterThanOrEqual(1);
  });
});

describe('E2ETestExecutor is generator-agnostic', () => {
  it('delegates execution to the injected generator and aggregates its suites', async () => {
    const kaos = fakeKaos();
    const suites: TestSuiteResult[] = [
      {
        file: 'pkg/api', status: 'failed', duration: 5,
        tests: [
          { name: 'TestA_E2E', status: 'passed', failureMessages: [] },
          { name: 'TestB_E2E', status: 'failed', failureMessages: ['boom'] },
        ],
      },
    ];
    const runTests = vi.fn().mockResolvedValue(suites);
    const fakeGenerator: E2ETestGenerator = {
      id: 'fake',
      detectProjectStructure: vi.fn(),
      analyzeImpact: vi.fn(),
      generateTestsForFeature: vi.fn(),
      resolveGeneratedTestDir: () => 'e2e_generated',
      runTests,
    };
    const executor = new E2ETestExecutor(kaos, defaultConfig, fakeGenerator);
    const tf: TestFile = { relativePath: 'api_e2e_test.go', content: '// go' };
    const result = await executor.execute([tf], '/tmp');

    expect(runTests).toHaveBeenCalledTimes(1);
    // Writes into the generator-resolved (non-dot) directory, not the config default.
    const writtenPath = (kaos.writeText as ReturnType<typeof vi.fn>).mock.calls[0]![0] as string;
    expect(writtenPath).toContain('/tmp/e2e_generated/');
    expect(result.passed).toBe(1);
    expect(result.failed).toBe(1);
    expect(result.suites).toEqual(suites);
  });
});

describe('Cache integration', () => {
  const tempDirs: string[] = [];

  afterAll(() => {
    for (const dir of tempDirs) {
      try { rmSync(dir, { recursive: true, force: true }); } catch { /* ignore */ }
    }
  });

  it('short-circuits on cache hit and does not invoke test runner', async () => {
    const cacheDir = mkdtempSync(join(tmpdir(), 'exec-cache-'));
    tempDirs.push(cacheDir);
    const testConfig: ResolvedE2EConfig = {
      ...defaultConfig,
      cacheEnabled: true,
      cacheDir,
    };
    const kaos = fakeKaos();
    const executor = new E2ETestExecutor(kaos, testConfig, tsGenerator);

    // Pre-populate cache with a known key
    const testFile: TestFile = { relativePath: 'cached.test.ts', content: 'it("x",()=>{})' };
    const key = computeCacheKey(['src/foo.ts'], [testFile]);
    const cachedResult: E2EExecutionResult = {
      passed: 5, failed: 0, skipped: 0, durationMs: 1,
      reportPath: '/tmp/cached.json', summary: 'Cached result', suites: [],
    };
    mkdirSync(cacheDir, { recursive: true });
    writeFileSync(join(cacheDir, key + '.json'), JSON.stringify({
      createdAt: new Date().toISOString(),
      key,
      result: cachedResult,
    }));

    // Execute with matching inputs
    const result = await executor.execute([testFile], '/tmp', { changedFiles: ['src/foo.ts'] });

    // Should return cached result
    expect(result.passed).toBe(5);
    expect(result.summary).toBe('Cached result');

    // vitest should NOT have been called (cache hit short-circuited)
    const vitestCalls = (kaos.exec as ReturnType<typeof vi.fn>).mock.calls.filter(
      (call: string[]) => call[0] === 'pnpm' && call[1] === 'vitest',
    );
    expect(vitestCalls).toHaveLength(0);
  });
});

describe('parseVitestJson (via integration)', () => {
  it('parses empty testResults as empty suites', async () => {
    const kaos = fakeKaos();
    (kaos as any).readText = vi.fn().mockResolvedValue(JSON.stringify({ testResults: [] }));
    const executor = new E2ETestExecutor(kaos, defaultConfig, tsGenerator);
    const tf: TestFile = { relativePath: 'x.test.ts', content: 'it("ok", () => {})' };
    const result = await executor.execute([tf], '/tmp');
    expect(result.suites).toEqual([]);
  });
});
