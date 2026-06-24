import { describe, expect, it } from 'vitest';
import { E2EConfigSchema } from '@odysseythink/agent-core-shared';
import { E2EConfigResolver } from '../src/config';
import type { OdyConfig } from '@odysseythink/agent-core-shared';

describe('E2EConfigResolver', () => {
  it('returns defaults for empty config', () => {
    const result = E2EConfigResolver.resolve({} as OdyConfig);
    expect(result.enabled).toBe(true);
    expect(result.strategy).toBe('smart');
    expect(result.criticalTools).toEqual(['ExitPlanModeTool']);
    expect(result.recursiveAnalysisEnabled).toBe(true);
    expect(result.cacheEnabled).toBe(true);
    expect(result.cacheDir).toBe('.ody-code/e2e-cache');
  });

  it('overrides enabled from raw', () => {
    const result = E2EConfigResolver.resolve({ e2e: { enabled: false } as any } as OdyConfig);
    expect(result.enabled).toBe(false);
  });

  it('throws for maxConcurrency 0', () => {
    expect(() => E2EConfigResolver.resolve({ e2e: { maxConcurrency: 0 } as any } as OdyConfig))
      .toThrow();
  });

  it('parses e2e section object directly', () => {
    const parsed = E2EConfigSchema.parse({
      enabled: true,
      strategy: 'critical-only',
      criticalTools: ['api'],
    });
    expect(parsed.strategy).toBe('critical-only');
    expect(parsed.criticalTools).toEqual(['api']);
  });
});
