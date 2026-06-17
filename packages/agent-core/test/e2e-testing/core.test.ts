import { describe, expect, it } from 'vitest';
import { parseConfigString } from '../../src/config/toml';
import { E2EConfigResolver } from '#/e2e-testing/config';
import type { ResolvedE2EConfig } from '#/e2e-testing/config';
import type { OdyConfig } from '#/config/schema';
import { ImpactAnalyzer } from '#/e2e-testing/impact-analyzer';
import { registry } from '#/e2e-testing/registry';
import { TypeScriptVitestGenerator } from '#/e2e-testing/generator';

describe('E2E config schema', () => {
  it('parses [e2e] section from TOML', () => {
    const config = parseConfigString(`
[e2e]
enabled = true
strategy = "smart"
critical_tools = ["ExitPlanModeTool"]
failure_policy = "warn"
max_concurrency = 4
test_timeout = 30000
report_dir = ".ody-code/test-reports"
generated_test_dir = ".ody-code/test-generated/e2e"
`);
    expect(config.e2e).toBeDefined();
    expect(config.e2e!.enabled).toBe(true);
    expect(config.e2e!.strategy).toBe('smart');
    expect(config.e2e!.criticalTools).toEqual(['ExitPlanModeTool']);
    expect(config.e2e!.failurePolicy).toBe('warn');
    expect(config.e2e!.maxConcurrency).toBe(4);
    expect(config.e2e!.testTimeout).toBe(30000);
  });

  it('e2e is undefined when [e2e] section is absent', () => {
    const config = parseConfigString('[permission]\nrules = []');
    expect(config.e2e).toBeUndefined();
  });
});

describe('E2EConfigResolver', () => {
  it('returns defaults for empty config', () => {
    const result = E2EConfigResolver.resolve({} as OdyConfig);
    expect(result.enabled).toBe(true);
    expect(result.strategy).toBe('smart');
    expect(result.criticalTools).toEqual(['ExitPlanModeTool']);
  });

  it('overrides enabled from raw', () => {
    const result = E2EConfigResolver.resolve({ e2e: { enabled: false } as any } as OdyConfig);
    expect(result.enabled).toBe(false);
  });

  it('throws for maxConcurrency 0', () => {
    expect(() => E2EConfigResolver.resolve({ e2e: { maxConcurrency: 0 } as any } as OdyConfig))
      .toThrow();
  });
});

const defaultConfig: ResolvedE2EConfig = {
  enabled: true, strategy: 'smart', criticalTools: ['ExitPlanModeTool'],
  failurePolicy: 'warn', maxConcurrency: 4, testTimeout: 30000,
  reportDir: '.ody-code/test-reports', generatedTestDir: '.ody-code/test-generated/e2e',
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

  it('matches session-mode/index.ts to ExitPlanModeTool', () => {
    const result = ImpactAnalyzer.analyze(
      ['packages/agent-core/src/agent/session-mode/index.ts'],
      defaultConfig,
    );
    expect(result.affectedTools.some(t => t.toolId === 'ExitPlanModeTool')).toBe(true);
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
});

import type { AffectedTool, E2EPriority, Feature, ImpactAnalysisResult, ProjectStructure, TestFile } from '#/e2e-testing/types';

describe('E2E types (compile check)', () => {
  it('Feature shape is constructable', () => {
    const f: Feature = { toolId: 'ExitPlanModeTool', changedFiles: [], projectRoot: '/app' };
    expect(f.toolId).toBe('ExitPlanModeTool');
  });

  it('AffectedTool priority is valid', () => {
    const a: AffectedTool = { toolId: 'T', priority: 'critical' as E2EPriority };
    expect(a.priority).toBe('critical');
  });
});

describe('E2EGeneratorRegistry + TS/Vitest detection', () => {
  it('detectAndGet throws without package.json', async () => {
    await expect(registry.detectAndGet('/no-package-json')).rejects.toThrow('No E2E generator');
  });

  it('detectAndGet returns generator for agent-core project root', async () => {
    const gen = await registry.detectAndGet(process.cwd());
    expect(gen.id).toBe('typescript-vitest');
  });

  it('detectProjectStructure returns null without package.json', async () => {
    const gen = new TypeScriptVitestGenerator();
    const result = await gen.detectProjectStructure('/no-package-json');
    expect(result).toBeNull();
  });

  it('detectProjectStructure returns structure for agent-core', async () => {
    const gen = new TypeScriptVitestGenerator();
    const result = await gen.detectProjectStructure(process.cwd());
    expect(result).toEqual({
      language: 'typescript',
      framework: 'nodejs',
      testTool: 'vitest',
      root: expect.stringContaining('agent-core'),
    });
  });
});

describe('ImpactAnalyzer edge cases', () => {
  it('prioritizes critical over important when both match', () => {
    const config = { ...defaultConfig, criticalTools: ['ExitPlanModeTool', 'EnterPlanModeTool'] };
    const result = ImpactAnalyzer.analyze(
      [
        'packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts',
        'packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts',
      ],
      config,
    );
    const exitPlan = result.affectedTools.find(t => t.toolId === 'ExitPlanModeTool');
    const enterPlan = result.affectedTools.find(t => t.toolId === 'EnterPlanModeTool');
    expect(exitPlan!.priority).toBe('critical');
    expect(enterPlan!.priority).toBe('critical');
  });

  it('normalizes backslash paths', () => {
    const result = ImpactAnalyzer.analyze(
      ['packages\\agent-core\\src\\tools\\builtin\\planning\\exit-plan-mode.ts'],
      defaultConfig,
    );
    expect(result.affectedTools).toHaveLength(1);
  });
});

describe('E2EConfigResolver edge cases', () => {
  it('defaults testTimeout when omitted', () => {
    const result = E2EConfigResolver.resolve({ e2e: {} as any } as OdyConfig);
    expect(result.testTimeout).toBe(30000);
  });

  it('defaults failurePolicy to warn', () => {
    const result = E2EConfigResolver.resolve({} as OdyConfig);
    expect(result.failurePolicy).toBe('warn');
  });
});
