import { describe, expect, it } from 'vitest';
import { E2EConfigSchema } from '#/config/schema';
import { E2EConfigResolver } from '#/e2e-testing/config';
import type { OdyConfig } from '#/config/schema';

describe('E2E config schema additions', () => {
  it('resolves new Phase 2 fields with defaults', () => {
    const resolved = E2EConfigResolver.resolve({} as OdyConfig);
    expect(resolved.recursiveAnalysisEnabled).toBe(true);
    expect(resolved.maxRecursiveDepth).toBe(3);
    expect(resolved.cacheEnabled).toBe(true);
    expect(resolved.cacheDir).toBe('.ody-code/e2e-cache');
    expect(resolved.cacheTtlDays).toBe(7);
    expect(resolved.cacheMaxEntries).toBe(20);
  });

  it('keeps existing defaults', () => {
    const resolved = E2EConfigResolver.resolve({} as OdyConfig);
    expect(resolved.enabled).toBe(true);
    expect(resolved.strategy).toBe('smart');
    expect(resolved.reportDir).toBe('.ody-code/test-reports');
  });
});
