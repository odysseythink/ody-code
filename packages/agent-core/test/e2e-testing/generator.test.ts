import { describe, expect, it } from 'vitest';
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'pathe';
import { TypeScriptVitestGenerator } from '#/e2e-testing/generator';
import type { Feature } from '#/e2e-testing/types';
import type { ResolvedE2EConfig } from '#/e2e-testing/config';

const tsConfig: ResolvedE2EConfig = {
  enabled: true, strategy: 'smart', criticalTools: ['ExitPlanModeTool'], failurePolicy: 'warn',
  maxConcurrency: 4, testTimeout: 30000,
  reportDir: '.ody-code/test-reports', generatedTestDir: '.ody-code/test-generated/e2e',
  recursiveAnalysisEnabled: false, maxRecursiveDepth: 3,
  cacheEnabled: false, cacheDir: '.ody-code/e2e-cache', cacheTtlDays: 7, cacheMaxEntries: 20,
};

function makeFeature(toolId: string): Feature {
  return {
    toolId,
    changedFiles: ['packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts'],
    projectRoot: join(process.cwd(), 'packages/agent-core'),
  };
}

const OUTPUT_DIR = 'packages/agent-core/.ody-code/test-generated/e2e';

describe('TypeScriptVitestGenerator.generateTestsForFeature', () => {
  it('produces ExitPlanMode E2E test file', async () => {
    const gen = new TypeScriptVitestGenerator();
    const files = await gen.generateTestsForFeature(makeFeature('ExitPlanModeTool'), OUTPUT_DIR);
    expect(files).toHaveLength(1);
    expect(files[0]!.relativePath).toBe('exit-plan-mode.e2e.test.ts');
    expect(files[0]!.content).toContain("import { describe, it, expect } from 'vitest'");
    expect(files[0]!.content).toContain('ExitPlanModeTool');
  });

  it('produces generic E2E test for unknown tool', async () => {
    const gen = new TypeScriptVitestGenerator();
    const files = await gen.generateTestsForFeature(makeFeature('SomeOtherTool'), OUTPUT_DIR);
    expect(files).toHaveLength(1);
    expect(files[0]!.relativePath).toBe('some-other-tool.e2e.test.ts');
    expect(files[0]!.content).toContain("import { describe, it, expect } from 'vitest'");
    expect(files[0]!.content).toContain('SomeOtherTool');
  });
});

describe('generateTestsForFeature edge cases', () => {
  it('ExitPlanMode generated content is valid TypeScript-like', async () => {
    const gen = new TypeScriptVitestGenerator();
    const files = await gen.generateTestsForFeature(makeFeature('ExitPlanModeTool'), OUTPUT_DIR);
    const content = files[0]!.content;
    // Must not contain template placeholder remnants
    expect(content).not.toContain('{{');
    expect(content).not.toContain('}}');
    // Must contain vitest imports
    expect(content).toContain("from 'vitest'");
    expect(content).toContain("describe(");
    expect(content).toContain("it(");
  });

  it('generic test does not have remaining placeholders', async () => {
    const gen = new TypeScriptVitestGenerator();
    const files = await gen.generateTestsForFeature(makeFeature('ArbitraryTool'), OUTPUT_DIR);
    expect(files[0]!.content).not.toContain('{{');
    expect(files[0]!.content).not.toContain('}}');
  });

  it('ExitPlanMode imports are relative to the generated test location', async () => {
    const gen = new TypeScriptVitestGenerator();
    const files = await gen.generateTestsForFeature(makeFeature('ExitPlanModeTool'), OUTPUT_DIR);
    const content = files[0]!.content;
    expect(content).toContain("import { ExitPlanModeTool } from '../../../src/tools/builtin/planning/exit-plan-mode'");
    expect(content).toContain("import { testAgent } from '../../../test/agent/harness/agent'");
  });

  it('ExitPlanMode generated imports resolve to existing source files', async () => {
    const gen = new TypeScriptVitestGenerator();
    const files = await gen.generateTestsForFeature(makeFeature('ExitPlanModeTool'), OUTPUT_DIR);
    const content = files[0]!.content;
    const generatedFile = join(OUTPUT_DIR, files[0]!.relativePath);
    const imports = [...content.matchAll(/from '(.+?)'/g)].map(m => m[1]!).filter(imp => imp.startsWith('.'));
    expect(imports.length).toBeGreaterThanOrEqual(2);
    for (const imp of imports) {
      const resolved = resolve(dirname(generatedFile), imp) + '.ts';
      expect(existsSync(resolved)).toBe(true);
    }
  });
});

describe('TypeScriptVitestGenerator.analyzeImpact', () => {
  const gen = new TypeScriptVitestGenerator();

  it('keeps ody-code self-test behavior when changed files hit the builtin-tool map', () => {
    const result = gen.analyzeImpact(
      ['packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts'],
      tsConfig,
    );
    expect(result.affectedTools).toEqual([{ toolId: 'ExitPlanModeTool', priority: 'critical' }]);
  });

  it('groups a user TypeScript project by package directory', () => {
    const result = gen.analyzeImpact(
      ['src/api/handler.ts', 'src/api/router.ts', 'src/db/client.ts', 'README.md'],
      tsConfig,
    );
    const ids = result.affectedTools.map(t => t.toolId).sort();
    expect(ids).toEqual(['src/api', 'src/db']);
    expect(result.affectedTools.every(t => t.priority === 'important')).toBe(true);
  });

  it('excludes test/spec/e2e and declaration files when grouping a user project', () => {
    const result = gen.analyzeImpact(
      ['src/api/handler.test.ts', 'src/api/handler.spec.ts', 'src/types.d.ts'],
      tsConfig,
    );
    expect(result.affectedTools).toHaveLength(0);
  });

  it('injects general for a user project under the always strategy with no source changes', () => {
    const result = gen.analyzeImpact(['README.md'], { ...tsConfig, strategy: 'always' });
    expect(result.affectedTools).toEqual([{ toolId: 'general', priority: 'nice-to-have' }]);
  });
});
