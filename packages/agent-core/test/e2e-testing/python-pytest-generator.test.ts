import { describe, expect, it, afterAll } from 'vitest';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { PythonPytestGenerator, parsePytestJsonReport, type PytestJsonReport } from '#/e2e-testing/generators/python-pytest';
import type { Feature } from '#/e2e-testing/types';
import type { ResolvedE2EConfig } from '#/e2e-testing/config';

const tempRoots: string[] = [];

function makePyProject(files: Record<string, string>): string {
  const root = mkdtempSync(join(tmpdir(), 'py-e2e-'));
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

describe('PythonPytestGenerator.detectProjectStructure', () => {
  const gen = new PythonPytestGenerator();

  it('returns null when no Python manifest exists', async () => {
    const root = makePyProject({ 'readme.md': '# hello' });
    expect(await gen.detectProjectStructure(root)).toBeNull();
  });

  it('detects FastAPI from pyproject.toml', async () => {
    const root = makePyProject({
      'pyproject.toml': '[project]\nname = "demo"\ndependencies = ["fastapi"]\n',
      'main.py': 'from fastapi import FastAPI\napp = FastAPI()\n',
    });
    const result = await gen.detectProjectStructure(root);
    expect(result).toEqual({ language: 'python', framework: 'fastapi', testTool: 'pytest', root });
  });

  it('detects Flask from requirements.txt', async () => {
    const root = makePyProject({
      'requirements.txt': 'flask==2.3.0\n',
      'app.py': 'from flask import Flask\napp = Flask(__name__)\n',
    });
    const result = await gen.detectProjectStructure(root);
    expect(result?.framework).toBe('flask');
  });

  it('detects Django from setup.py', async () => {
    const root = makePyProject({
      'setup.py': 'from setuptools import setup\nsetup(name="demo", install_requires=["django"])\n',
      'manage.py': 'def main():\n    pass\n',
    });
    const result = await gen.detectProjectStructure(root);
    expect(result?.framework).toBe('django');
  });

  it('returns null for dir with only non-py files', async () => {
    const root = makePyProject({
      'pyproject.toml': '[project]\nname = "demo"\n',
      'readme.md': '# hello',
    });
    expect(await gen.detectProjectStructure(root)).toBeNull();
  });
});

const config: ResolvedE2EConfig = {
  enabled: true, strategy: 'smart', criticalTools: [], failurePolicy: 'warn',
  maxConcurrency: 4, testTimeout: 30000,
  reportDir: '.ody-code/test-reports', generatedTestDir: '.ody-code/test-generated/e2e',
  recursiveAnalysisEnabled: true, maxRecursiveDepth: 3,
  cacheEnabled: true, cacheDir: '.ody-code/e2e-cache', cacheTtlDays: 7, cacheMaxEntries: 20,
};

describe('PythonPytestGenerator.generateTestsForFeature', () => {
  const gen = new PythonPytestGenerator();

  it('generates FastAPI template with uvicorn', async () => {
    const root = makePyProject({
      'pyproject.toml': '[project]\ndependencies = ["fastapi"]\n',
      'main.py': 'from fastapi import FastAPI\napp = FastAPI()\n',
    });
    const feature: Feature = { toolId: 'api', changedFiles: [], projectRoot: root };
    const files = await gen.generateTestsForFeature(feature, '.ody-code/test-generated/e2e');
    expect(files).toHaveLength(1);
    expect(files[0]!.content).toContain('uvicorn');
    expect(files[0]!.content).toContain('AUTO-GENERATED');
    expect(files[0]!.content).toContain('TODO');
  });

  it('generates Flask template', async () => {
    const root = makePyProject({
      'requirements.txt': 'flask==2.3.0\n',
      'app.py': 'from flask import Flask\napp = Flask(__name__)\n',
    });
    const files = await gen.generateTestsForFeature({ toolId: 'web', changedFiles: [], projectRoot: root }, '.ody-code/test-generated/e2e');
    expect(files[0]!.content).toContain('"flask", "run"');
  });

  it('generates Django template', async () => {
    const root = makePyProject({
      'setup.py': 'setup(name="demo", install_requires=["django"])\n',
      'manage.py': 'def main():\n    pass\n',
    });
    const files = await gen.generateTestsForFeature({ toolId: 'admin', changedFiles: [], projectRoot: root }, '.ody-code/test-generated/e2e');
    expect(files[0]!.content).toContain('runserver');
  });

  it('generic with entry generates subprocess template', async () => {
    const root = makePyProject({
      'pyproject.toml': '[project]\nname = "demo"\n',
      'main.py': 'print("hello")\n',
    });
    const files = await gen.generateTestsForFeature({ toolId: 'cli', changedFiles: [], projectRoot: root }, '.ody-code/test-generated/e2e');
    expect(files[0]!.content).toContain('subprocess.run');
  });
});

describe('PythonPytestGenerator.analyzeImpact', () => {
  const gen = new PythonPytestGenerator();

  it('maps .py files to their directory', () => {
    const result = gen.analyzeImpact(['src/api/main.py', 'src/api/utils.py', 'tests/test_foo.py'], config);
    expect(result.affectedTools).toHaveLength(1);
    expect(result.affectedTools[0]!.toolId).toBe('src/api');
  });

  it('excludes _test.py files', () => {
    const result = gen.analyzeImpact(['src/stuff_test.py'], config);
    expect(result.affectedTools).toHaveLength(0);
  });

  it('strategy=always with no changes emits general', () => {
    const result = gen.analyzeImpact([], { ...config, strategy: 'always' });
    expect(result.affectedTools).toHaveLength(1);
    expect(result.affectedTools[0]!.toolId).toBe('general');
  });
});

describe('parsePytestJsonReport', () => {
  it('maps a passing test', () => {
    const report: PytestJsonReport = {
      tests: [{ nodeid: 'test_thing.py::test_ok', outcome: 'passed', duration: 0.1 }],
    };
    const suites = parsePytestJsonReport(report);
    expect(suites).toHaveLength(1);
    expect(suites[0]!.tests[0]!.status).toBe('passed');
  });

  it('maps a failing test with failure messages', () => {
    const report: PytestJsonReport = {
      tests: [{
        nodeid: 'test_thing.py::test_fail',
        outcome: 'failed',
        call: { outcome: 'failed', longrepr: 'AssertionError: expected 200 got 500' },
      }],
    };
    const suites = parsePytestJsonReport(report);
    expect(suites[0]!.status).toBe('failed');
    expect(suites[0]!.tests[0]!.failureMessages).toHaveLength(1);
    expect(suites[0]!.tests[0]!.failureMessages[0]!).toContain('AssertionError');
  });

  it('returns empty array for empty report', () => {
    expect(parsePytestJsonReport({})).toEqual([]);
  });
});
