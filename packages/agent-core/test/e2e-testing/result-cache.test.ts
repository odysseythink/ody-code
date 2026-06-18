import { describe, expect, it, afterAll, beforeEach } from 'vitest';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync, existsSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import type { Kaos } from '@odysseythink/kaos';
import { createFakeKaos } from '../tools/fixtures/fake-kaos';
import { computeCacheKey, E2ETestResultCache } from '#/e2e-testing/result-cache';
import type { TestFile, E2EExecutionResult, TestSuiteResult } from '#/e2e-testing/types';
import type { ResolvedE2EConfig } from '#/e2e-testing/config';

const config: ResolvedE2EConfig = {
  enabled: true, strategy: 'smart', criticalTools: [], failurePolicy: 'warn',
  maxConcurrency: 4, testTimeout: 30000,
  reportDir: '.ody-code/test-reports', generatedTestDir: '.ody-code/test-generated/e2e',
  recursiveAnalysisEnabled: true, maxRecursiveDepth: 3,
  cacheEnabled: true, cacheDir: '.ody-code/e2e-cache', cacheTtlDays: 7, cacheMaxEntries: 20,
};

const sampleResult: E2EExecutionResult = {
  passed: 3, failed: 0, skipped: 0, durationMs: 100,
  reportPath: '/tmp/report.json',
  summary: '## E2E Results\n- Passed: 3',
  suites: [] as TestSuiteResult[],
};

describe('computeCacheKey', () => {
  it('same inputs produce identical keys', () => {
    const key1 = computeCacheKey(
      ['src/a.ts', 'src/b.ts'],
      [{ relativePath: 'test.ts', content: 'it("x", () => {})' }],
    );
    const key2 = computeCacheKey(
      ['src/a.ts', 'src/b.ts'],
      [{ relativePath: 'test.ts', content: 'it("x", () => {})' }],
    );
    expect(key1).toBe(key2);
  });

  it('reordering changed files does not change key', () => {
    const key1 = computeCacheKey(
      ['src/b.ts', 'src/a.ts'],
      [{ relativePath: 'test.ts', content: 'x' }],
    );
    const key2 = computeCacheKey(
      ['src/a.ts', 'src/b.ts'],
      [{ relativePath: 'test.ts', content: 'x' }],
    );
    expect(key1).toBe(key2);
  });

  it('reordering test files does not change key', () => {
    const key1 = computeCacheKey(
      ['src/a.ts'],
      [
        { relativePath: 'b.test.ts', content: 'b' },
        { relativePath: 'a.test.ts', content: 'a' },
      ],
    );
    const key2 = computeCacheKey(
      ['src/a.ts'],
      [
        { relativePath: 'a.test.ts', content: 'a' },
        { relativePath: 'b.test.ts', content: 'b' },
      ],
    );
    expect(key1).toBe(key2);
  });

  it('different changed file produces different key', () => {
    const key1 = computeCacheKey(
      ['src/a.ts'],
      [{ relativePath: 'test.ts', content: 'x' }],
    );
    const key2 = computeCacheKey(
      ['src/b.ts'],
      [{ relativePath: 'test.ts', content: 'x' }],
    );
    expect(key1).not.toBe(key2);
  });

  it('different test content produces different key', () => {
    const key1 = computeCacheKey(
      ['src/a.ts'],
      [{ relativePath: 'test.ts', content: 'x' }],
    );
    const key2 = computeCacheKey(
      ['src/a.ts'],
      [{ relativePath: 'test.ts', content: 'y' }],
    );
    expect(key1).not.toBe(key2);
  });

  it('backslash paths are normalized', () => {
    const key1 = computeCacheKey(
      ['src\\a.ts', 'src\\b.ts'],
      [{ relativePath: 'test.ts', content: 'x' }],
    );
    const key2 = computeCacheKey(
      ['src/a.ts', 'src/b.ts'],
      [{ relativePath: 'test.ts', content: 'x' }],
    );
    expect(key1).toBe(key2);
  });
});

describe('E2ETestResultCache', () => {
  let tempDir: string;
  let cache: E2ETestResultCache;
  let kaos: Kaos;

  beforeEach(() => {
    tempDir = mkdtempSync(join(tmpdir(), 'cache-e2e-'));
    const testConfig = { ...config, cacheDir: tempDir };
    kaos = createFakeKaos({});
    cache = new E2ETestResultCache(kaos, testConfig);
  });

  afterAll(() => {
    const parent = join(tmpdir());
    for (const entry of readdirSync(parent, { withFileTypes: true })) {
      if (entry.isDirectory() && entry.name.startsWith('cache-e2e-')) {
        rmSync(join(parent, entry.name), { recursive: true, force: true });
      }
    }
  });

  it('get returns null for missing key', async () => {
    const result = await cache.get('nonexistent');
    expect(result).toBeNull();
  });

  it('set then get returns the same result', async () => {
    const key = computeCacheKey(['a.ts'], []);
    await cache.set(key, sampleResult);
    const result = await cache.get(key);
    expect(result).toEqual(sampleResult);
  });

  it('get returns null when cacheEnabled=false', async () => {
    const disabledConfig = { ...config, cacheEnabled: false, cacheDir: tempDir };
    const disabledCache = new E2ETestResultCache(kaos, disabledConfig);
    const key = computeCacheKey(['a.ts'], []);
    await disabledCache.set(key, sampleResult);
    const result = await disabledCache.get(key);
    expect(result).toBeNull();
  });

  it('set does not throw when cacheEnabled=false', async () => {
    const disabledConfig = { ...config, cacheEnabled: false, cacheDir: tempDir };
    const disabledCache = new E2ETestResultCache(kaos, disabledConfig);
    await expect(disabledCache.set('any', sampleResult)).resolves.toBeUndefined();
  });

  it('prune removes expired entries', async () => {
    const key = computeCacheKey(['old.ts'], []);
    const cacheDir = tempDir;
    mkdirSync(cacheDir, { recursive: true });
    const oldEntry = {
      createdAt: new Date(Date.now() - 8 * 24 * 60 * 60 * 1000).toISOString(), // 8 days ago
      key,
      result: sampleResult,
    };
    writeFileSync(join(cacheDir, key + '.json'), JSON.stringify(oldEntry, null, 2));

    await cache.prune();
    expect(existsSync(join(cacheDir, key + '.json'))).toBe(false);
  });

  it('prune does not remove fresh entries', async () => {
    const key = computeCacheKey(['fresh.ts'], []);
    await cache.set(key, sampleResult);
    await cache.prune();
    const cacheDir = tempDir;
    expect(existsSync(join(cacheDir, key + '.json'))).toBe(true);
  });

  it('prune enforces max entries', async () => {
    const smallConfig = { ...config, cacheDir: tempDir, cacheMaxEntries: 3 };
    const smallCache = new E2ETestResultCache(kaos, smallConfig);

    for (let i = 0; i < 5; i++) {
      const key = computeCacheKey([`file${i}.ts`], []);
      await smallCache.set(key, sampleResult);
      await new Promise(r => setTimeout(r, 10));
    }

    await smallCache.prune();

    const jsonFiles = readdirSync(tempDir).filter(f => f.endsWith('.json'));
    expect(jsonFiles.length).toBeLessThanOrEqual(3);
  });
});
