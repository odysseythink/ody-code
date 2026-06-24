import { describe, expect, it, afterAll } from 'vitest';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { NodejsJestGenerator, parseJestJson, type JestJsonOutput } from '../../src/generators/nodejs-jest';
import type { Feature } from '../../src/types';
import type { ResolvedE2EConfig } from '../../src/config';

const tempRoots: string[] = [];

function makeNodeProject(files: Record<string, string>): string {
  const root = mkdtempSync(join(tmpdir(), 'node-e2e-'));
  tempRoots.push(root);
  for (const [rel, content] of Object.entries(files)) {
    const abs = join(root, rel);
    mkdirSync(join(abs, '..'), { recursive: true });
    writeFileSync(abs, content);
  }
  return root;
}

afterAll(() => {
  for (const root of tempRoots) rmSync(root, { recursive: true, force: true });
});

describe('NodejsJestGenerator.detectProjectStructure', () => {
  const gen = new NodejsJestGenerator();

  it('returns null when no package.json exists', async () => {
    const root = makeNodeProject({ 'readme.md': '# hello' });
    expect(await gen.detectProjectStructure(root)).toBeNull();
  });

  it('returns null when package.json exists but no jest config/dep', async () => {
    const root = makeNodeProject({
      'package.json': JSON.stringify({ name: 'demo' }),
    });
    expect(await gen.detectProjectStructure(root)).toBeNull();
  });

  it('detects Express project with jest installed', async () => {
    const root = makeNodeProject({
      'package.json': JSON.stringify({ name: 'demo', dependencies: { express: '^4' }, devDependencies: { jest: '^29' } }),
      'app.js': 'const express = require("express"); const app = express(); app.listen(3000);',
    });
    const result = await gen.detectProjectStructure(root);
    expect(result).toEqual({ language: 'nodejs', framework: 'express', testTool: 'jest', root });
  });

  it('detects NestJS project', async () => {
    const root = makeNodeProject({
      'package.json': JSON.stringify({ name: 'demo', dependencies: { '@nestjs/core': '^10' }, devDependencies: { jest: '^29' } }),
    });
    const result = await gen.detectProjectStructure(root);
    expect(result?.framework).toBe('nestjs');
  });

  it('detects Next.js project', async () => {
    const root = makeNodeProject({
      'package.json': JSON.stringify({ name: 'demo', dependencies: { next: '^14' }, devDependencies: { jest: '^29' } }),
    });
    const result = await gen.detectProjectStructure(root);
    expect(result?.framework).toBe('nextjs');
  });

  it('detects generic Node project with jest config key in package.json', async () => {
    const root = makeNodeProject({
      'package.json': JSON.stringify({ name: 'demo', jest: { testEnvironment: 'node' } }),
      'index.js': 'console.log("hi");',
    });
    const result = await gen.detectProjectStructure(root);
    expect(result?.framework).toBe('generic');
  });
});

const config: ResolvedE2EConfig = {
  enabled: true, strategy: 'smart', criticalTools: [], failurePolicy: 'warn',
  maxConcurrency: 4, testTimeout: 30000,
  reportDir: '.ody-code/test-reports', generatedTestDir: '.ody-code/test-generated/e2e',
  recursiveAnalysisEnabled: true, maxRecursiveDepth: 3,
  cacheEnabled: true, cacheDir: '.ody-code/e2e-cache', cacheTtlDays: 7, cacheMaxEntries: 20,
};

describe('NodejsJestGenerator.generateTestsForFeature', () => {
  const gen = new NodejsJestGenerator();

  it('generates Express template', async () => {
    const root = makeNodeProject({
      'package.json': JSON.stringify({
        name: 'demo', dependencies: { express: '^4' },
        devDependencies: { jest: '^29' },
      }),
      'app.js': 'const express = require("express"); const app = express(); app.listen(3000);',
    });
    const feature: Feature = { toolId: 'web', changedFiles: [], projectRoot: root };
    const files = await gen.generateTestsForFeature(feature, '.ody-code/test-generated/e2e');
    expect(files).toHaveLength(1);
    expect(files[0]!.content).toContain('AUTO-GENERATED');
    expect(files[0]!.content).toContain('TODO');
    expect(files[0]!.content).toContain('Express');
  });

  it('generates NestJS template', async () => {
    const root = makeNodeProject({
      'package.json': JSON.stringify({
        name: 'demo', dependencies: { '@nestjs/core': '^10' },
        devDependencies: { jest: '^29' },
      }),
    });
    const files = await gen.generateTestsForFeature(
      { toolId: 'api', changedFiles: [], projectRoot: root },
      '.ody-code/test-generated/e2e',
    );
    expect(files[0]!.content).toContain('NestJS');
  });

  it('generates Next.js template', async () => {
    const root = makeNodeProject({
      'package.json': JSON.stringify({
        name: 'demo', dependencies: { next: '^14' },
        devDependencies: { jest: '^29' },
      }),
    });
    const files = await gen.generateTestsForFeature(
      { toolId: 'frontend', changedFiles: [], projectRoot: root },
      '.ody-code/test-generated/e2e',
    );
    expect(files[0]!.content).toContain("'next', 'dev'");
  });

  it('generic with entry generates spawn template', async () => {
    const root = makeNodeProject({
      'package.json': JSON.stringify({ name: 'demo', devDependencies: { jest: '^29' } }),
      'index.js': 'console.log("hi");',
    });
    const files = await gen.generateTestsForFeature(
      { toolId: 'cli', changedFiles: [], projectRoot: root },
      '.ody-code/test-generated/e2e',
    );
    expect(files[0]!.content).toContain('spawn');
  });

  it('detects pnpm from lockfile in Next.js template', async () => {
    const root = makeNodeProject({
      'package.json': JSON.stringify({
        dependencies: { next: '^14' }, devDependencies: { jest: '^29' },
      }),
      'pnpm-lock.yaml': 'lockfileVersion: "6.0"',
    });
    const files = await gen.generateTestsForFeature(
      { toolId: 'web', changedFiles: [], projectRoot: root },
      '.ody-code/test-generated/e2e',
    );
    expect(files[0]!.content).toContain('pnpm');
  });
});

describe('NodejsJestGenerator.analyzeImpact', () => {
  const gen = new NodejsJestGenerator();

  it('maps changed files to their directory', () => {
    const result = gen.analyzeImpact(
      ['src/routes/index.ts', 'src/routes/auth.ts', 'tests/app.test.ts'], config,
    );
    expect(result.affectedTools).toHaveLength(1);
    expect(result.affectedTools[0]!.toolId).toBe('src/routes');
  });

  it('excludes test files', () => {
    const result = gen.analyzeImpact(['src/app.test.js'], config);
    expect(result.affectedTools).toHaveLength(0);
  });
});

describe('parseJestJson', () => {
  it('maps a passing assertion', () => {
    const output: JestJsonOutput = {
      testResults: [{
        name: '/abs/path/test.js', status: 'passed',
        assertionResults: [{ title: 'works', status: 'passed' }],
      }],
    };
    const suites = parseJestJson(output);
    expect(suites).toHaveLength(1);
    expect(suites[0]!.tests[0]!.status).toBe('passed');
  });

  it('maps a failing assertion with failureMessages', () => {
    const output: JestJsonOutput = {
      testResults: [{
        name: '/abs/path/test.js', status: 'failed',
        assertionResults: [{
          title: 'fails', status: 'failed',
          failureMessages: ['expected 200 got 500'],
        }],
      }],
    };
    const suites = parseJestJson(output);
    expect(suites[0]!.status).toBe('failed');
    expect(suites[0]!.tests[0]!.failureMessages).toEqual(['expected 200 got 500']);
  });

  it('maps pending to skipped', () => {
    const output: JestJsonOutput = {
      testResults: [{
        name: '/abs/path/test.js', status: 'passed',
        assertionResults: [{ title: 'todo', status: 'pending' }],
      }],
    };
    const suites = parseJestJson(output);
    expect(suites[0]!.tests[0]!.status).toBe('skipped');
  });
});
