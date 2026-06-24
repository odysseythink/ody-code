import { describe, expect, it } from 'vitest';
import { ImpactAnalyzer } from '../src/impact-analyzer';
import type { ResolvedE2EConfig } from '../src/config';

const defaultConfig: ResolvedE2EConfig = {
  enabled: true, strategy: 'smart', criticalTools: ['ExitPlanModeTool'],
  failurePolicy: 'warn', maxConcurrency: 4, testTimeout: 30000,
  reportDir: '.ody-code/test-reports', generatedTestDir: '.ody-code/test-generated/e2e',
  recursiveAnalysisEnabled: true, maxRecursiveDepth: 3,
  cacheEnabled: true, cacheDir: '.ody-code/e2e-cache', cacheTtlDays: 7, cacheMaxEntries: 20,
};

describe('ImpactAnalyzer', () => {
  it('matches exit-plan-mode.ts to ExitPlanModeTool as critical', () => {
    const result = ImpactAnalyzer.analyze(
      ['packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts'],
      defaultConfig,
    );
    expect(result.affectedTools).toEqual([
      { toolId: 'ExitPlanModeTool', priority: 'critical' },
    ]);
  });

  it('returns empty for unrelated file with smart strategy', () => {
    const result = ImpactAnalyzer.analyze(['unrelated.ts'], defaultConfig);
    expect(result.affectedTools).toHaveLength(0);
  });

  it('returns general for always strategy with no matches', () => {
    const config = { ...defaultConfig, strategy: 'always' as const };
    const result = ImpactAnalyzer.analyze(['unrelated.ts'], config);
    expect(result.affectedTools).toEqual([
      { toolId: 'general', priority: 'nice-to-have' },
    ]);
  });

  it('filters non-critical with critical-only strategy', () => {
    const config = { ...defaultConfig, strategy: 'critical-only' as const, criticalTools: ['ExitPlanModeTool'] };
    const result = ImpactAnalyzer.analyze(
      ['packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts'],
      config,
    );
    expect(result.affectedTools).toHaveLength(0);
  });

  it('normalizes backslash paths', () => {
    const result = ImpactAnalyzer.analyze(
      ['packages\\agent-core\\src\\tools\\builtin\\planning\\exit-plan-mode.ts'],
      defaultConfig,
    );
    expect(result.affectedTools).toHaveLength(1);
  });
});
