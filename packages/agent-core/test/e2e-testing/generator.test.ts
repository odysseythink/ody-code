import { describe, expect, it } from 'vitest';
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'pathe';
import { TypeScriptVitestGenerator } from '#/e2e-testing/generator';
import type { Feature } from '#/e2e-testing/types';

function makeFeature(toolId: string): Feature {
  return {
    toolId,
    changedFiles: ['packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts'],
    projectRoot: process.cwd(),
  };
}

const OUTPUT_DIR = '.ody-code/test-generated/e2e';

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
