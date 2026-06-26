# E2E Testing Automation Phase 2 — Implementation Plan

**Goal:** Add Python/pytest and Node.js/Jest E2E generators, recursive impact analysis, and a test-result cache to the existing E2E testing framework.

**Architecture:** Extend the existing `E2ETestGenerator` interface with two new generators (Python/pytest, Node.js/Jest) that follow the Go generator pattern: detect project structure, classify frameworks, render language-specific templates, and run tests via subprocess with JSON output parsing. Add a `RecursiveImpactAnalyzer` for transitive dependency traversal and an `E2ETestResultCache` that short-circuits the executor on cache hits.

**Tech Stack:** TypeScript (Node.js), Vitest (test runner), Zod (config validation).

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File | Create/Modify | Purpose |
|------|--------------|---------|
| `packages/agent-core/src/config/schema.ts:283-292` | Modify | Add 5 new E2E config fields |
| `packages/agent-core/src/e2e-testing/config.ts:1-18` | Modify | Update `ResolvedE2EConfig` to include new fields |
| `packages/agent-core/src/e2e-testing/generators/python-pytest.ts` | Create | Python/pytest E2E generator |
| `packages/agent-core/src/e2e-testing/registry.ts:1-24` | Modify | Register Python + Node generators |
| `packages/agent-core/src/e2e-testing/plan-enricher.ts:57-58` | Modify | Extend file regex to include `.py` |
| `packages/agent-core/src/e2e-testing/generators/nodejs-jest.ts` | Create | Node.js/Jest E2E generator |
| `packages/agent-core/src/e2e-testing/recursive-impact-analyzer.ts` | Create | Recursive dependency traversal |
| `packages/agent-core/src/e2e-testing/generator.ts` | Modify | Wire recursive analysis into TS generator |
| `packages/agent-core/src/e2e-testing/generators/go.ts` | Modify | Wire recursive analysis into Go generator |
| `packages/agent-core/src/e2e-testing/result-cache.ts` | Create | Cache layer: key computation + storage |
| `packages/agent-core/src/e2e-testing/executor.ts` | Modify | Insert cache lookup/store in execute() |
| `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.ts` | Modify | Pass changedFiles to executor |
| `packages/agent-core/test/e2e-testing/python-pytest-generator.test.ts` | Create | Python generator unit tests |
| `packages/agent-core/test/e2e-testing/nodejs-jest-generator.test.ts` | Create | Node generator unit tests |
| `packages/agent-core/test/e2e-testing/recursive-impact-analyzer.test.ts` | Create | Recursive analyzer unit tests |
| `packages/agent-core/test/e2e-testing/result-cache.test.ts` | Create | Cache unit tests |

---

## Dependency Overview

```
Task 1 (Config) ──┬── Task 2 (Python Generator) ──┬── Task 5 (Integrate Recursive)
                  │                               │
                  ├── Task 3 (Node Generator) ────┤
                  │                               │
                  ├── Task 4 (Recursive Analyzer)─┘
                  │
                  └── Task 6 (Cache Module) ────── Task 7 (Integrate Cache)
```

- Phase A (sequential): Task 1 (config schema) must run first.
- Phase B (parallel): Tasks 2, 3, 4 can run in parallel after Task 1.
- Phase C (sequential): Task 5 after 2+3+4; Task 7 after Task 6.
- Task 6 can run anywhere after Task 1 (independent of generators).

---

## Risks & Open Questions

| Risk | Mitigation |
|------|-----------|
| Detection picks wrong generator (Jest vs Vitest) | Registry order: Vitest first (checks package.json), then Jest (requires jest dep/config), then Python (looks for pyproject.toml), then Go (looks for go.mod). |
| pytest-json-report plugin not installed | Fallback to exit-code-only result. |
| Subprocess orphans from spawned servers | Templates include fixture teardown with `terminate()` + `SIGKILL` timeout fallback. |
| Cache key collision (SHA256) | Negligible collision risk; SHA-256 is collision-resistant. |
| Concurrent cache writes | Acceptable for Phase 2; each run writes its own keyed file. |

---

## Task 1: Config schema additions + resolver update

**Depends on:** none

**Files:** Modify: `packages/agent-core/src/config/schema.ts:283-292`, `packages/agent-core/src/e2e-testing/config.ts:1-18`

This task adds 5 new fields to `E2EConfigSchema` and updates the `ResolvedE2EConfig` type to include them. The `E2EConfigResolver` already uses `E2EConfigSchema.parse(raw)` to produce `ResolvedE2EConfig` via `Required<E2EConfig>`, so the schema extension is sufficient.

- [ ] Write the failing test.

No separate test file is needed — this is a config schema change. The build invariant is that ALL existing config-using tests continue to pass with the new defaults. Create a lightweight config test to confirm the new fields resolve with correct defaults:

File: `packages/agent-core/test/e2e-testing/config.test.ts` (new)

```typescript
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
```

- [ ] Run it and verify it FAILS.

```bash
pnpm vitest run packages/agent-core/test/e2e-testing/config.test.ts
# Expected: Type errors or missing property errors because
# E2EConfigSchema doesn't contain the new fields yet.
```

- [ ] Write the minimal implementation.

In `packages/agent-core/src/config/schema.ts`, after line 291 (`generatedTestDir`), add:

```typescript
  generatedTestDir: z.string().default('.ody-code/test-generated/e2e'),
  recursiveAnalysisEnabled: z.boolean().default(true),
  maxRecursiveDepth: z.number().int().min(1).default(3),
  cacheEnabled: z.boolean().default(true),
  cacheDir: z.string().default('.ody-code/e2e-cache'),
  cacheTtlDays: z.number().int().min(1).default(7),
  cacheMaxEntries: z.number().int().min(1).default(20),
});
```

In `packages/agent-core/src/e2e-testing/config.ts`, add the export line for the new `ResolvedE2EConfig` shape — but since `ResolvedE2EConfig = Required<E2EConfig>` and `E2EConfig` comes from `z.infer<typeof E2EConfigSchema>`, the addition to the schema automatically makes the type resolve. No code change needed in `config.ts`; the existing `Required<E2EConfig>` picks up the new fields automatically.

- [ ] Run it and verify it PASSES.

```bash
pnpm vitest run packages/agent-core/test/e2e-testing/config.test.ts
# Expected: 2 tests pass.
```

- [ ] Verify existing tests still pass (whole-tree typecheck).

```bash
pnpm exec tsc --noEmit -p packages/agent-core/tsconfig.json
pnpm vitest run packages/agent-core/test/e2e-testing/
# All existing E2E tests must still pass.
```

- [ ] Commit.

```bash
git add packages/agent-core/src/config/schema.ts packages/agent-core/test/e2e-testing/config.test.ts
git commit -m "feat(e2e): add recursive analysis and cache config fields"

---

## Task 2: Python/Pytest E2E Generator

**Depends on:** Task 1

**Files:** Create: `packages/agent-core/src/e2e-testing/generators/python-pytest.ts`, `packages/agent-core/test/e2e-testing/python-pytest-generator.test.ts` / Modify: `packages/agent-core/src/e2e-testing/registry.ts:1-24`, `packages/agent-core/src/e2e-testing/plan-enricher.ts:57-58`

Implement the `PythonPytestGenerator` class implementing `E2ETestGenerator`. Follows the Go generator pattern: detect project structure from manifest files, classify into framework kind, generate pytest templates, run via subprocess, parse JSON report into `TestSuiteResult[]`.

### Step 1: Detection tests (TDD)

In `packages/agent-core/test/e2e-testing/python-pytest-generator.test.ts`:

```typescript
import { describe, expect, it, afterAll } from 'vitest';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { PythonPytestGenerator } from '#/e2e-testing/generators/python-pytest';

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
```

Run to verify failure:

```bash
pnpm vitest run packages/agent-core/test/e2e-testing/python-pytest-generator.test.ts
# Expected: FAIL — PythonPytestGenerator not found / not exported
```

### Step 2: Detection & classification implementation

Create `packages/agent-core/src/e2e-testing/generators/python-pytest.ts`:

```typescript
import { join, extname } from 'pathe';
import type { ReadStream, WriteStream } from 'node:stream';
import type {
  E2ETestGenerator,
  Feature,
  ImpactAnalysisResult,
  ProjectStructure,
  RunContext,
  TestCaseResult,
  TestFile,
  TestSuiteResult,
} from '../types';
import type { ResolvedE2EConfig } from '../config';

type PythonKind = 'fastapi' | 'flask' | 'django' | 'generic';

interface PythonDetection {
  kind: PythonKind;
  framework: string;
  entry: string;
}

interface PytestJsonReport {
  summary?: { passed?: number; failed?: number; skipped?: number; duration?: number };
  tests?: Array<{
    nodeid: string;
    outcome: 'passed' | 'failed' | 'skipped';
    setup?: { outcome?: string; longrepr?: string };
    call?: { outcome?: string; longrepr?: string };
    teardown?: { outcome?: string; longrepr?: string };
    duration?: number;
  }>;
}

function pythonIdent(raw: string): string {
  const cleaned = raw.replace(/[^a-zA-Z0-9]+/g, '_').replace(/^_+|_+$/g, '');
  if (cleaned === '') return 'root';
  return /^[0-9]/.test(cleaned) ? `_${cleaned}` : cleaned;
}

function timestamp(): string {
  return new Date().toISOString().replaceAll(/[:.]/g, '-');
}

function extractDependencyNames(manifest: string): string[] {
  const names: string[] = [];
  for (const line of manifest.split('\n')) {
    const m = line.match(/^\s*[\"']?([a-zA-Z0-9_-]+)/);
    if (m) names.push(m[1].toLowerCase());
  }
  return names;
}

function moduleName(root: string, absPath: string): string {
  const rel = absPath.replace(/\\/g, '/').replace(root.replace(/\\/g, '/'), '').replace(/^\//, '');
  return rel.replace(/\.py$/, '').replace(/\//g, '.');
}

function isTopLevel(root: string, absPath: string): boolean {
  const rel = absPath.replace(/\\/g, '/').slice(root.replace(/\\/g, '/').replace(/\/$/, '').length + 1);
  return !rel.includes('/');
}

function isTestFile(file: string): boolean {
  return /(?:^|[\\/])test_|_test\.py$/.test(file) || file.endsWith('_test.py');
}

function listPythonFiles(
  readdirSync: typeof import('node:fs').readdirSync,
  root: string,
  limit: number,
): string[] {
  const results: string[] = [];
  const stack = [root];
  while (stack.length > 0 && results.length < limit) {
    const dir = stack.pop()!;
    const entries = (() => { try { return readdirSync(dir, { withFileTypes: true }); } catch { return []; } })();
    for (const entry of entries) {
      const fullPath = join(dir, entry.name);
      if (entry.isDirectory()) {
        if (entry.name.startsWith('.') || entry.name === '__pycache__' || entry.name === 'node_modules') continue;
        stack.push(fullPath);
      } else if (entry.name.endsWith('.py') && !entry.name.startsWith('test_') && !entry.name.endsWith('_test.py')) {
        results.push(fullPath);
        if (results.length >= limit) break;
      }
    }
  }
  return results;
}

function findFastApiEntry(
  existsSync: typeof import('node:fs').existsSync,
  readFileSync: typeof import('node:fs').readFileSync,
  readdirSync: typeof import('node:fs').readdirSync,
  root: string,
): string {
  for (const file of listPythonFiles(readdirSync, root, 300)) {
    try {
      if (/app\s*=\s*FastAPI\s*\(/.test(readFileSync(file, 'utf-8'))) return moduleName(root, file);
    } catch { /* skip unreadable */ }
  }
  return 'main';
}

function findFlaskEntry(
  existsSync: typeof import('node:fs').existsSync,
  readFileSync: typeof import('node:fs').readFileSync,
  readdirSync: typeof import('node:fs').readdirSync,
  root: string,
): string {
  for (const file of listPythonFiles(readdirSync, root, 300)) {
    try {
      if (/app\s*=\s*Flask\s*\(/.test(readFileSync(file, 'utf-8'))) return moduleName(root, file);
    } catch { /* skip unreadable */ }
  }
  return 'app';
}

function findDjangoEntry(existsSync: typeof import('node:fs').existsSync, root: string): string {
  return existsSync(join(root, 'manage.py')) ? 'manage.py' : '';
}

function findGenericEntry(
  readdirSync: typeof import('node:fs').readdirSync,
  root: string,
): string {
  const files = listPythonFiles(readdirSync, root, 300);
  const candidates = files.filter(f => isTopLevel(root, f) && !isTestFile(f) && !f.endsWith('__init__.py'));
  return candidates.length === 1 ? moduleName(root, candidates[0]) : '';
}

function classify(
  deps: string[],
  existsSync: typeof import('node:fs').existsSync,
  readFileSync: typeof import('node:fs').readFileSync,
  readdirSync: typeof import('node:fs').readdirSync,
  root: string,
): PythonDetection {
  if (deps.includes('fastapi')) return { kind: 'fastapi', framework: 'fastapi', entry: findFastApiEntry(existsSync, readFileSync, readdirSync, root) };
  if (deps.includes('flask')) return { kind: 'flask', framework: 'flask', entry: findFlaskEntry(existsSync, readFileSync, readdirSync, root) };
  if (deps.includes('django')) return { kind: 'django', framework: 'django', entry: findDjangoEntry(existsSync, root) };
  return { kind: 'generic', framework: 'generic', entry: findGenericEntry(readdirSync, root) };
}

function readManifest(
  existsSync: typeof import('node:fs').existsSync,
  readFileSync: typeof import('node:fs').readFileSync,
  root: string,
): string | null {
  const candidates = ['pyproject.toml', 'requirements.txt', 'setup.py'];
  for (const name of candidates) {
    const p = join(root, name);
    try {
      if (existsSync(p)) return readFileSync(p, 'utf-8');
    } catch { /* skip */ }
  }
  return null;
}

// Templates:

const FASTAPI_TEMPLATE = `# AUTO-GENERATED by RunE2ETests (Python FastAPI template)
# TODO: adjust the endpoint path and response assertions to match your real API.
import socket
import subprocess
import sys
import time

import pytest


def _free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


@pytest.fixture(scope="module")
def server():
    port = _free_port()
    addr = f"127.0.0.1:{port}"
    proc = subprocess.Popen(
        [sys.executable or "python3", "-m", "uvicorn", "{{entry}}:app", "--host", "127.0.0.1", f"--port={port}"],
        cwd="{{projectRoot}}",
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    deadline = time.time() + 10
    import urllib.request
    while time.time() < deadline:
        try:
            urllib.request.urlopen(f"http://{addr}/", timeout=0.2)
            break
        except Exception:
            time.sleep(0.1)
    else:
        proc.terminate()
        proc.wait(timeout=5)
        pytest.fail(f"Server did not start at {addr}")
    yield addr
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()


def test_{{ident}}_e2e(server):
    addr = server
    import urllib.request
    resp = urllib.request.urlopen(f"http://{addr}/", timeout=5)
    assert resp.status == 200
    # TODO: assert on response body
`;

const FLASK_TEMPLATE = `# AUTO-GENERATED by RunE2ETests (Python Flask template)
# TODO: adjust the endpoint path and response assertions to match your real API.
import os
import socket
import subprocess
import time

import pytest


def _free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


@pytest.fixture(scope="module")
def server():
    port = _free_port()
    addr = f"127.0.0.1:{port}"
    env = {**os.environ, "FLASK_APP": "{{entry}}", "FLASK_ENV": "development"}
    proc = subprocess.Popen(
        [sys.executable or "python3", "-m", "flask", "run", "--host", "127.0.0.1", f"--port={port}"],
        cwd="{{projectRoot}}",
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    deadline = time.time() + 10
    import urllib.request
    while time.time() < deadline:
        try:
            urllib.request.urlopen(f"http://{addr}/", timeout=0.2)
            break
        except Exception:
            time.sleep(0.1)
    else:
        proc.terminate()
        proc.wait(timeout=5)
        pytest.fail(f"Server did not start at {addr}")
    yield addr
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()


def test_{{ident}}_e2e(server):
    addr = server
    import urllib.request
    resp = urllib.request.urlopen(f"http://{addr}/", timeout=5)
    assert resp.status == 200
    # TODO: assert on response body
`;

const DJANGO_TEMPLATE = `# AUTO-GENERATED by RunE2ETests (Python Django template)
# TODO: adjust the endpoint path and response assertions to match your real API.
import socket
import subprocess
import sys
import time

import pytest


def _free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


@pytest.fixture(scope="module")
def server():
    port = _free_port()
    addr = f"127.0.0.1:{port}"
    proc = subprocess.Popen(
        [sys.executable or "python3", "{{entry}}", "runserver", f"127.0.0.1:{port}", "--noreload"],
        cwd="{{projectRoot}}",
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    deadline = time.time() + 10
    import urllib.request
    while time.time() < deadline:
        try:
            urllib.request.urlopen(f"http://{addr}/", timeout=0.2)
            break
        except Exception:
            time.sleep(0.1)
    else:
        proc.terminate()
        proc.wait(timeout=5)
        pytest.fail(f"Server did not start at {addr}")
    yield addr
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()


def test_{{ident}}_e2e(server):
    addr = server
    import urllib.request
    resp = urllib.request.urlopen(f"http://{addr}/", timeout=5)
    assert resp.status == 200
    # TODO: assert on response body
`;

const GENERIC_PY_TEMPLATE = `# AUTO-GENERATED by RunE2ETests (Python generic template)
# TODO: replace with a real launch + assertion flow for "{{toolId}}".
import subprocess
import sys


def test_{{ident}}_e2e():
    result = subprocess.run(
        [sys.executable or "python3", "{{entry}}"],
        cwd="{{projectRoot}}",
        capture_output=True,
        timeout=30,
    )
    assert result.returncode == 0, f"script exited with {result.returncode}:\\n{result.stderr.decode()}"
`;

const GENERIC_PLACEHOLDER_TEMPLATE = `# AUTO-GENERATED by RunE2ETests (Python generic template)
# TODO: no runnable entry point detected. Replace with a real test.
def test_{{ident}}_e2e():
    assert True, "placeholder for {{toolId}}"
`;

export function parsePytestJsonReport(report: PytestJsonReport): TestSuiteResult[] {
  const suiteMap = new Map<string, TestSuiteResult>();

  for (const test of report.tests ?? []) {
    const file = test.nodeid.split('::')[0] ?? test.nodeid;
    let suite = suiteMap.get(file);
    if (!suite) {
      suite = { file, status: 'passed', duration: 0, tests: [] };
      suiteMap.set(file, suite);
    }

    const outcome: TestCaseResult['status'] =
      test.outcome === 'passed' ? 'passed' : test.outcome === 'skipped' ? 'skipped' : 'failed';

    if (outcome === 'failed') suite.status = 'failed';

    const failureMessages: string[] = [];
    for (const phase of [test.setup, test.call, test.teardown]) {
      if (phase?.outcome === 'failed' && phase.longrepr) {
        failureMessages.push(phase.longrepr.slice(0, 2000));
      }
    }

    suite.tests.push({ name: test.nodeid, status: outcome, failureMessages });
    suite.duration += test.duration ?? 0;
  }

  return [...suiteMap.values()];
}

export class PythonPytestGenerator implements E2ETestGenerator {
  readonly id = 'python-pytest';

  async detectProjectStructure(root: string): Promise<ProjectStructure | null> {
    const { existsSync, readFileSync, readdirSync } = await import('node:fs');
    const manifest = readManifest(existsSync, readFileSync, root);
    if (manifest === null) return null;

    const deps = extractDependencyNames(manifest);
    const detection = classify(deps, existsSync, readFileSync, readdirSync, root);

    // Guard: generic kind with no .py files is not a real Python project
    if (detection.kind === 'generic') {
      const pyFiles = listPythonFiles(readdirSync, root, 50);
      if (pyFiles.length === 0) return null;
    }

    return { language: 'python', framework: detection.framework, testTool: 'pytest', root };
  }

  analyzeImpact(changedFiles: string[], config: ResolvedE2EConfig): ImpactAnalysisResult {
    const packages = new Set<string>();
    for (const file of changedFiles) {
      const normalized = file.replace(/\\/g, '/');
      if (!normalized.endsWith('.py') || isTestFile(normalized)) continue;
      const slash = normalized.lastIndexOf('/');
      packages.add(slash === -1 ? '.' : normalized.slice(0, slash));
    }

    const affected: Array<{ toolId: string; priority: 'critical' | 'important' | 'nice-to-have' }> = [];
    for (const pkg of packages) {
      const priority = config.criticalTools.includes(pkg) ? 'critical' as const : 'important' as const;
      if (config.strategy === 'critical-only' && priority !== 'critical') continue;
      affected.push({ toolId: pkg, priority });
    }

    if (affected.length === 0 && config.strategy === 'always') {
      affected.push({ toolId: 'general', priority: 'nice-to-have' });
    }

    return { affectedTools: affected };
  }

  resolveGeneratedTestDir(config: ResolvedE2EConfig): string {
    return config.generatedTestDir;
  }

  async generateTestsForFeature(feature: Feature, _outputDir: string): Promise<TestFile[]> {
    const { existsSync, readFileSync, readdirSync } = await import('node:fs');
    const manifest = readManifest(existsSync, readFileSync, feature.projectRoot);
    // Even if manifest is null (shouldn't happen at this point), fall back to generic
    const deps = manifest ? extractDependencyNames(manifest) : [];
    const detection = classify(deps, existsSync, readFileSync, readdirSync, feature.projectRoot);
    const ident = pythonIdent(feature.toolId);
    const relativePath = `${ident}_e2e_test.py`;
    const content = this.renderTemplate(detection, ident, feature);
    return [{ relativePath, content }];
  }

  async runTests(absoluteTestPaths: string[], ctx: RunContext): Promise<TestSuiteResult[]> {
    if (absoluteTestPaths.length === 0) return [];

    const { kaos, config, projectRoot, signal } = ctx;
    const generatedTestDir = this.resolveGeneratedTestDir(config);
    const reportFile = join(generatedTestDir, `pytest-report-${timestamp()}.json`);

    // Try python3 first, fall back to python
    let pythonCmd = 'python3';
    try {
      const probe = await kaos.exec('python3', '--version');
      await probe.stdout.on('data', () => {});
      await probe.wait();
      if (probe.exitCode !== 0) pythonCmd = 'python';
    } catch {
      pythonCmd = 'python';
    }

    const args = [
      '-m', 'pytest',
      '--json-report', '--json-report-file=' + reportFile,
      '-q', '--tb=short',
      ...absoluteTestPaths,
    ];

    const proc = await kaos.withCwd(projectRoot).exec(pythonCmd, ...args);

    const onAbort = () => { void proc.kill(); };
    if (signal?.aborted) onAbort();
    else signal?.addEventListener('abort', onAbort, { once: true });

    const stderrChunks: Buffer[] = [];
    proc.stderr.on('data', (chunk: Buffer) => stderrChunks.push(chunk));

    try { await proc.wait(); } finally {
      signal?.removeEventListener('abort', onAbort);
    }

    // Try to read JSON report
    try {
      const jsonText = await kaos.readText(reportFile);
      const report = JSON.parse(jsonText) as PytestJsonReport;
      const suites = parsePytestJsonReport(report);
      if (suites.length > 0) return suites;
    } catch { /* fall through to fallback */ }

    // Fallback: no JSON report available
    const stderr = Buffer.concat(stderrChunks).toString('utf-8');
    return [{
      file: absoluteTestPaths[0] ?? 'pytest',
      status: proc.exitCode === 0 ? 'passed' : 'failed',
      duration: 0,
      tests: [{
        name: 'pytest suite',
        status: proc.exitCode === 0 ? 'passed' : 'failed',
        failureMessages: proc.exitCode === 0 ? [] : [stderr.slice(0, 2000)],
      }],
    }];
  }

  private renderTemplate(detection: PythonDetection, ident: string, feature: Feature): string {
    const replacer = (t: string) => t
      .replaceAll('{{ident}}', ident)
      .replaceAll('{{toolId}}', feature.toolId)
      .replaceAll('{{projectRoot}}', feature.projectRoot)
      .replaceAll('{{entry}}', detection.entry || 'main');

    switch (detection.kind) {
      case 'fastapi': return replacer(FASTAPI_TEMPLATE);
      case 'flask': return replacer(FLASK_TEMPLATE);
      case 'django': return replacer(DJANGO_TEMPLATE);
      default:
        return detection.entry
          ? replacer(GENERIC_PY_TEMPLATE)
          : replacer(GENERIC_PLACEHOLDER_TEMPLATE);
    }
  }
}
```

### Step 3: Register in registry.ts + update plan-enricher

In `packages/agent-core/src/e2e-testing/registry.ts`, add after the Go import:

```typescript
import { PythonPytestGenerator } from './generators/python-pytest';
```

And change the registration block to:

```typescript
export const registry = new E2EGeneratorRegistry();
registry.register(new TypeScriptVitestGenerator());
registry.register(new PythonPytestGenerator());
registry.register(new GoGenerator());
```

In `packages/agent-core/src/e2e-testing/plan-enricher.ts` line 58, change:

```typescript
const regex = /(?:packages|apps)\/[a-zA-Z0-9\-_/.]+\.[jt]sx?/g;
```

to:

```typescript
const regex = /(?:packages|apps)\/[a-zA-Z0-9\-_/.]+\.(?:[jt]sx?|py)/g;
```

(The `.` before the extension is already part of the regex — we add `|py` to the extension group.)

### Step 4: Run tests and verify

```bash
pnpm vitest run packages/agent-core/test/e2e-testing/python-pytest-generator.test.ts
# Expected: all detection tests pass

pnpm exec tsc --noEmit -p packages/agent-core/tsconfig.json
# Expected: no type errors
```

Add template generation and impact analysis tests to the test file (continuation of the same file):

```typescript
import type { Feature } from '#/e2e-testing/types';
import type { ResolvedE2EConfig } from '#/e2e-testing/config';

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
    expect(files[0].content).toContain('uvicorn');
    expect(files[0].content).toContain('AUTO-GENERATED');
    expect(files[0].content).toContain('TODO');
  });

  it('generates Flask template', async () => {
    const root = makePyProject({
      'requirements.txt': 'flask==2.3.0\n',
      'app.py': 'from flask import Flask\napp = Flask(__name__)\n',
    });
    const files = await gen.generateTestsForFeature({ toolId: 'web', changedFiles: [], projectRoot: root }, '.ody-code/test-generated/e2e');
    expect(files[0].content).toContain('flask run');
  });

  it('generates Django template', async () => {
    const root = makePyProject({
      'setup.py': 'setup(name="demo", install_requires=["django"])\n',
      'manage.py': 'def main():\n    pass\n',
    });
    const files = await gen.generateTestsForFeature({ toolId: 'admin', changedFiles: [], projectRoot: root }, '.ody-code/test-generated/e2e');
    expect(files[0].content).toContain('runserver');
  });

  it('generic with entry generates subprocess template', async () => {
    const root = makePyProject({
      'pyproject.toml': '[project]\nname = "demo"\n',
      'src/main.py': 'print("hello")\n',
    });
    const files = await gen.generateTestsForFeature({ toolId: 'cli', changedFiles: [], projectRoot: root }, '.ody-code/test-generated/e2e');
    expect(files[0].content).toContain('subprocess.run');
  });
});

describe('PythonPytestGenerator.analyzeImpact', () => {
  const gen = new PythonPytestGenerator();

  it('maps .py files to their directory', () => {
    const result = gen.analyzeImpact(['src/api/main.py', 'src/api/utils.py', 'tests/test_foo.py'], config);
    expect(result.affectedTools).toHaveLength(1);
    expect(result.affectedTools[0].toolId).toBe('src/api');
  });

  it('excludes _test.py files', () => {
    const result = gen.analyzeImpact(['src/stuff_test.py'], config);
    expect(result.affectedTools).toHaveLength(0);
  });

  it('strategy=always with no changes emits general', () => {
    const result = gen.analyzeImpact([], { ...config, strategy: 'always' });
    expect(result.affectedTools).toHaveLength(1);
    expect(result.affectedTools[0].toolId).toBe('general');
  });
});

describe('parsePytestJsonReport', () => {
  it('maps a passing test', () => {
    const report: PytestJsonReport = {
      tests: [{ nodeid: 'test_thing.py::test_ok', outcome: 'passed', duration: 0.1 }],
    };
    const suites = parsePytestJsonReport(report);
    expect(suites).toHaveLength(1);
    expect(suites[0].tests[0].status).toBe('passed');
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
    expect(suites[0].status).toBe('failed');
    expect(suites[0].tests[0].failureMessages).toHaveLength(1);
    expect(suites[0].tests[0].failureMessages[0]).toContain('AssertionError');
  });

  it('returns empty array for empty report', () => {
    expect(parsePytestJsonReport({})).toEqual([]);
  });
});
```

Run all tests:

```bash
pnpm vitest run packages/agent-core/test/e2e-testing/python-pytest-generator.test.ts
# Expected: ~12 tests pass (5 detection + 4 template + 3 impact + 3 parse)

pnpm exec tsc --noEmit -p packages/agent-core/tsconfig.json
# Expected: no type errors
```

### Step 5: Commit

```bash
git add packages/agent-core/src/e2e-testing/generators/python-pytest.ts \
        packages/agent-core/src/e2e-testing/registry.ts \
        packages/agent-core/src/e2e-testing/plan-enricher.ts \
        packages/agent-core/test/e2e-testing/python-pytest-generator.test.ts
git commit -m "feat(e2e): add Python/pytest E2E generator"
```
```


---

## Task 3: Node.js/Jest E2E Generator

**Depends on:** Task 1

**Files:** Create: `packages/agent-core/src/e2e-testing/generators/nodejs-jest.ts`, `packages/agent-core/test/e2e-testing/nodejs-jest-generator.test.ts` / Modify: `packages/agent-core/src/e2e-testing/registry.ts:1-24`

Implement the `NodejsJestGenerator` class implementing `E2ETestGenerator`. Detects Node projects via `package.json` + jest, classifies frameworks, generates jest templates, runs via `<pm> exec jest --json`, parses JSON output.

### Step 1: Detection tests (TDD)

In `packages/agent-core/test/e2e-testing/nodejs-jest-generator.test.ts`:

```typescript
import { describe, expect, it, afterAll } from 'vitest';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { NodejsJestGenerator } from '#/e2e-testing/generators/nodejs-jest';

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
```

Run to verify failure:

```bash
pnpm vitest run packages/agent-core/test/e2e-testing/nodejs-jest-generator.test.ts
# Expected: FAIL — NodejsJestGenerator not found
```

### Step 2: Full implementation

Create `packages/agent-core/src/e2e-testing/generators/nodejs-jest.ts`:

```typescript
import { join } from 'pathe';
import type {
  E2ETestGenerator,
  Feature,
  ImpactAnalysisResult,
  ProjectStructure,
  RunContext,
  TestCaseResult,
  TestFile,
  TestSuiteResult,
} from '../types';
import type { ResolvedE2EConfig } from '../config';

type NodejsKind = 'express' | 'nestjs' | 'nextjs' | 'generic';

interface NodejsDetection {
  kind: NodejsKind;
  framework: string;
  entry: string;
  packageManager: 'pnpm' | 'yarn' | 'npm';
}

interface JestJsonOutput {
  testResults?: Array<{
    name: string;
    status: 'passed' | 'failed';
    message?: string;
    assertionResults?: Array<{
      title: string;
      status: 'passed' | 'failed' | 'pending';
      failureMessages?: string[];
      duration?: number;
    }>;
  }>;
}

function camelIdent(raw: string): string {
  const cleaned = raw.replace(/[^a-zA-Z0-9]+/g, '_').replace(/^_+|_+$/g, '');
  if (cleaned === '') return 'root';
  return cleaned.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase())
    .replace(/^[A-Z]/, (c: string) => c.toLowerCase());
}

function timestamp(): string {
  return new Date().toISOString().replaceAll(/[:.]/g, '-');
}

function detectPackageManager(
  existsSync: typeof import('node:fs').existsSync,
  root: string,
): 'pnpm' | 'yarn' | 'npm' {
  if (existsSync(join(root, 'pnpm-lock.yaml'))) return 'pnpm';
  if (existsSync(join(root, 'yarn.lock'))) return 'yarn';
  return 'npm';
}

function existsJestConfig(
  existsSync: typeof import('node:fs').existsSync,
  root: string,
): boolean {
  const names = [
    'jest.config.js', 'jest.config.ts', 'jest.config.mjs',
    'jest.config.cjs', 'jest.config.json',
  ];
  return names.some(n => existsSync(join(root, n)));
}

function isSourceFile(file: string): boolean {
  return /\.(?:js|jsx|ts|tsx|mjs|cjs)$/.test(file) && !/\.d\.ts$/.test(file);
}

function isNodeTestFile(file: string): boolean {
  return /\.(?:test|spec)\.(?:js|jsx|ts|tsx|mjs|cjs)$/.test(file);
}

function isTopLevel(root: string, absPath: string): boolean {
  const rootNorm = root.replace(/\\/g, '/').replace(/\/$/, '');
  const fileNorm = absPath.replace(/\\/g, '/');
  const rel = fileNorm.slice(rootNorm.length + 1);
  return !rel.includes('/');
}

function listSourceFiles(
  readdirSync: typeof import('node:fs').readdirSync,
  root: string,
  limit: number,
): string[] {
  const results: string[] = [];
  const stack = [root];
  while (stack.length > 0 && results.length < limit) {
    const dir = stack.pop()!;
    const entries = (() => {
      try { return readdirSync(dir, { withFileTypes: true }); } catch { return []; }
    })();
    for (const entry of entries) {
      const fullPath = join(dir, entry.name);
      if (entry.isDirectory()) {
        if (entry.name.startsWith('.') || entry.name === 'node_modules'
          || entry.name === 'dist' || entry.name === 'build') continue;
        stack.push(fullPath);
      } else if (isSourceFile(entry.name) && !isNodeTestFile(entry.name)) {
        results.push(fullPath);
        if (results.length >= limit) break;
      }
    }
  }
  return results;
}

function relativePath(root: string, absPath: string): string {
  const rootNorm = root.replace(/\\/g, '/').replace(/\/$/, '');
  const fileNorm = absPath.replace(/\\/g, '/');
  return fileNorm.slice(rootNorm.length + 1);
}

function findExpressEntry(
  readFileSync: typeof import('node:fs').readFileSync,
  readdirSync: typeof import('node:fs').readdirSync,
  root: string,
): string {
  for (const file of listSourceFiles(readdirSync, root, 300)) {
    try {
      const content = readFileSync(file, 'utf-8');
      if (/(?:const|let|var)\s+\w+\s*=\s*express\s*\(/.test(content)
        || /app\.listen\s*\(/.test(content)) {
        return relativePath(root, file);
      }
    } catch { /* skip */ }
  }
  return 'src/app.js';
}

function findNestJsEntry(
  existsSync: typeof import('node:fs').existsSync,
  root: string,
): string {
  if (existsSync(join(root, 'dist/main.js'))) return 'dist/main.js';
  if (existsSync(join(root, 'dist/main.ts'))) return 'dist/main.ts';
  if (existsSync(join(root, 'src/main.ts'))) return 'src/main.ts';
  if (existsSync(join(root, 'src/main.js'))) return 'src/main.js';
  return 'src/main.ts';
}

function findGenericEntry(
  readdirSync: typeof import('node:fs').readdirSync,
  root: string,
): string {
  const files = listSourceFiles(readdirSync, root, 300);
  const candidates = files.filter(f => {
    if (!isTopLevel(root, f)) return false;
    const base = f.replace(/\\/g, '/').split('/').pop() ?? '';
    return /^(?:index|main|server|app)\./.test(base);
  });
  return candidates.length >= 1 ? relativePath(root, candidates[0]) : '';
}

function classify(
  deps: Record<string, string>,
  existsSync: typeof import('node:fs').existsSync,
  readFileSync: typeof import('node:fs').readFileSync,
  readdirSync: typeof import('node:fs').readdirSync,
  root: string,
): NodejsDetection {
  const pm = detectPackageManager(existsSync, root);
  if ('next' in deps) {
    return { kind: 'nextjs', framework: 'nextjs', entry: '.', packageManager: pm };
  }
  if ('@nestjs/core' in deps || '@nestjs/common' in deps) {
    return { kind: 'nestjs', framework: 'nestjs',
      entry: findNestJsEntry(existsSync, root), packageManager: pm };
  }
  if ('express' in deps) {
    return { kind: 'express', framework: 'express',
      entry: findExpressEntry(readFileSync, readdirSync, root), packageManager: pm };
  }
  return { kind: 'generic', framework: 'generic',
    entry: findGenericEntry(readdirSync, root), packageManager: pm };
}

// ---- Templates ----

const EXPRESS_TEMPLATE = `// AUTO-GENERATED by RunE2ETests (Node Express / Jest template)
// TODO: adjust the endpoint path and response assertions to match your real API.
const http = require('http');
const path = require('path');

describe('{{toolId}} E2E', () => {
  let server;
  let addr;

  beforeAll((done) => {
    const appPath = path.resolve(__dirname, '..', '{{entry}}');
    const app = require(appPath);
    if (typeof app === 'function' && app.listen) {
      server = app.listen(0, '127.0.0.1', () => {
        addr = \`127.0.0.1:\${server.address().port}\`;
        done();
      });
    } else {
      const { spawn } = require('child_process');
      const proc = spawn('node', [appPath], {
        cwd: '{{projectRoot}}',
        stdio: 'pipe',
      });
      let started = false;
      proc.stdout.on('data', () => { if (!started) { started = true; done(); } });
      proc.stderr.on('data', (d) => process.stderr.write(d));
      setTimeout(() => { if (!started) { started = true; done(); } }, 500);
      server = proc;
    }
  }, 10000);

  afterAll(() => {
    if (server && server.close) server.close();
    else if (server && server.kill) {
      server.kill('SIGTERM');
      setTimeout(() => { try { server.kill('SIGKILL'); } catch(e) {} }, 3000);
    }
  });

  it('responds with 200 at /', async () => {
    // TODO: adjust the URL. Defaults to root path.
    const resp = await fetch(\`http://\${addr}/\`);
    expect(resp.status).toBe(200);
  });
});
`;

const NESTJS_TEMPLATE = `// AUTO-GENERATED by RunE2ETests (Node NestJS / Jest template)
// TODO: adjust the endpoint path and response assertions to match your real API.
const { spawn } = require('child_process');
const net = require('net');

describe('{{toolId}} E2E', () => {
  let proc;
  let port;

  beforeAll((done) => {
    const srv = net.createServer();
    srv.listen(0, '127.0.0.1', () => {
      port = srv.address().port;
      srv.close();
      proc = spawn('node', ['{{entry}}'], {
        cwd: '{{projectRoot}}',
        env: { ...process.env, PORT: String(port) },
      });
      proc.on('error', () => {});
      setTimeout(done, 2000);
    });
  }, 15000);

  afterAll(() => {
    if (proc) {
      proc.kill('SIGTERM');
      setTimeout(() => { try { proc.kill('SIGKILL'); } catch(e) {} }, 3000);
    }
  });

  it('responds with 200 at /', async () => {
    // TODO: adjust the URL
    const resp = await fetch(\`http://127.0.0.1:\${port}/\`);
    expect(resp.status).toBe(200);
  });
});
`;

const NEXTJS_TEMPLATE = `// AUTO-GENERATED by RunE2ETests (Node Next.js / Jest template)
// TODO: adjust the endpoint path to match your API routes.
const { spawn } = require('child_process');
const net = require('net');

describe('{{toolId}} E2E', () => {
  let proc;
  let port;

  beforeAll((done) => {
    const srv = net.createServer();
    srv.listen(0, '127.0.0.1', () => {
      port = srv.address().port;
      srv.close();
      proc = spawn('{{packageManager}}', ['next', 'dev', '--port', String(port)], {
        cwd: '{{projectRoot}}',
        stdio: 'pipe',
      });
      proc.on('error', () => {});
      setTimeout(done, 5000);
    });
  }, 20000);

  afterAll(() => {
    if (proc) {
      proc.kill('SIGTERM');
      setTimeout(() => { try { proc.kill('SIGKILL'); } catch(e) {} }, 3000);
    }
  });

  it('responds with 200 at /api/health or /', async () => {
    // TODO: adjust the URL. Try common health endpoints.
    const urls = ['/api/health', '/api/hello', '/'];
    for (const url of urls) {
      try {
        const resp = await fetch(\`http://127.0.0.1:\${port}\${url}\`);
        if (resp.ok) return;
      } catch (_) {}
    }
    throw new Error('No endpoint responded with a successful status');
  });
});
`;

const GENERIC_NODE_TEMPLATE = `// AUTO-GENERATED by RunE2ETests (Node generic / Jest template)
// TODO: replace with real launch + assertion for "{{toolId}}".
const { spawn } = require('child_process');

describe('{{toolId}} E2E', () => {
  it('runs the entry script successfully', (done) => {
    const proc = spawn('node', ['{{entry}}'], {
      cwd: '{{projectRoot}}',
      timeout: 30000,
    });
    let stderr = '';
    proc.stderr.on('data', (d) => { stderr += d.toString(); });
    proc.on('close', (code) => {
      if (code !== 0) {
        done(new Error(\`exit code \${code}: \${stderr}\`));
      } else {
        done();
      }
    });
    proc.on('error', (err) => done(err));
  });
});
`;

const GENERIC_PLACEHOLDER_NODE_TEMPLATE = `// AUTO-GENERATED by RunE2ETests (Node generic / Jest template)
// TODO: no runnable entry point detected. Replace with a real test.
describe('{{toolId}} E2E', () => {
  it('placeholder', () => {
    expect(true).toBe(true);
  });
});
`;

export function parseJestJson(output: JestJsonOutput): TestSuiteResult[] {
  const suites: TestSuiteResult[] = [];
  for (const result of output.testResults ?? []) {
    let suiteStatus: TestSuiteResult['status'] =
      result.status === 'passed' ? 'passed' : 'failed';
    const tests: TestCaseResult[] = [];

    for (const assertion of result.assertionResults ?? []) {
      const status: TestCaseResult['status'] =
        assertion.status === 'passed' ? 'passed'
          : assertion.status === 'pending' ? 'skipped'
          : 'failed';
      if (status === 'failed') suiteStatus = 'failed';
      tests.push({
        name: assertion.title,
        status,
        failureMessages: assertion.failureMessages ?? [],
      });
    }

    if (tests.length === 0 && result.message) {
      tests.push({
        name: 'suite setup',
        status: 'failed',
        failureMessages: [result.message.slice(0, 2000)],
      });
      suiteStatus = 'failed';
    }

    suites.push({
      file: result.name,
      status: suiteStatus,
      duration: (result.assertionResults ?? []).reduce(
        (s, a) => s + (a.duration ?? 0), 0,
      ),
      tests,
    });
  }
  return suites;
}

export class NodejsJestGenerator implements E2ETestGenerator {
  readonly id = 'nodejs-jest';

  async detectProjectStructure(root: string): Promise<ProjectStructure | null> {
    const { existsSync, readFileSync, readdirSync } = await import('node:fs');
    const pkgPath = join(root, 'package.json');
    if (!existsSync(pkgPath)) return null;

    let pkg: Record<string, unknown>;
    try {
      pkg = JSON.parse(readFileSync(pkgPath, 'utf-8'));
    } catch {
      return null;
    }

    const deps = {
      ...(pkg.dependencies as Record<string, string> ?? {}),
      ...(pkg.devDependencies as Record<string, string> ?? {}),
    };
    const hasJest = 'jest' in deps || existsJestConfig(existsSync, root);
    if (!hasJest) return null;

    const detection = classify(deps, existsSync, readFileSync, readdirSync, root);

    if (detection.kind === 'generic') {
      const srcFiles = listSourceFiles(readdirSync, root, 50);
      if (srcFiles.length === 0) return null;
    }

    return {
      language: 'nodejs',
      framework: detection.framework,
      testTool: 'jest',
      root,
    };
  }

  analyzeImpact(
    changedFiles: string[],
    config: ResolvedE2EConfig,
  ): ImpactAnalysisResult {
    const packages = new Set<string>();
    for (const file of changedFiles) {
      const normalized = file.replace(/\\/g, '/');
      if (!isSourceFile(normalized) || isNodeTestFile(normalized)) continue;
      const slash = normalized.lastIndexOf('/');
      packages.add(slash === -1 ? '.' : normalized.slice(0, slash));
    }

    const affected: Array<{
      toolId: string;
      priority: 'critical' | 'important' | 'nice-to-have';
    }> = [];
    for (const pkg of packages) {
      const priority = config.criticalTools.includes(pkg)
        ? 'critical' as const : 'important' as const;
      if (config.strategy === 'critical-only' && priority !== 'critical') continue;
      affected.push({ toolId: pkg, priority });
    }

    if (affected.length === 0 && config.strategy === 'always') {
      affected.push({ toolId: 'general', priority: 'nice-to-have' });
    }

    return { affectedTools: affected };
  }

  resolveGeneratedTestDir(config: ResolvedE2EConfig): string {
    return config.generatedTestDir;
  }

  async generateTestsForFeature(
    feature: Feature,
    _outputDir: string,
  ): Promise<TestFile[]> {
    const { existsSync, readFileSync, readdirSync } = await import('node:fs');
    const pkgPath = join(feature.projectRoot, 'package.json');
    let deps: Record<string, string> = {};
    try {
      const pkg = JSON.parse(readFileSync(pkgPath, 'utf-8'));
      deps = {
        ...(pkg.dependencies as Record<string, string> ?? {}),
        ...(pkg.devDependencies as Record<string, string> ?? {}),
      };
    } catch { /* empty deps */ }

    const detection = classify(
      deps, existsSync, readFileSync, readdirSync, feature.projectRoot,
    );
    const ident = camelIdent(feature.toolId);
    const relativePath = `__tests__/${ident}.e2e.test.js`;
    const content = this.renderTemplate(detection, ident, feature);
    return [{ relativePath, content }];
  }

  async runTests(
    absoluteTestPaths: string[],
    ctx: RunContext,
  ): Promise<TestSuiteResult[]> {
    if (absoluteTestPaths.length === 0) return [];

    const { kaos, config, projectRoot, signal } = ctx;
    const generatedTestDir = this.resolveGeneratedTestDir(config);
    const outputFile = join(
      generatedTestDir, `jest-report-${timestamp()}.json`,
    );

    const { existsSync } = await import('node:fs');
    const pm = detectPackageManager(existsSync, projectRoot);

    const args = [
      'exec', 'jest',
      '--json', '--outputFile=' + outputFile,
      '--testTimeout=' + String(config.testTimeout),
      '--runInBand',
      ...absoluteTestPaths,
    ];

    const proc = await kaos.withCwd(projectRoot).exec(pm, ...args);

    const onAbort = () => { void proc.kill(); };
    if (signal?.aborted) onAbort();
    else signal?.addEventListener('abort', onAbort, { once: true });

    try { await proc.wait(); } finally {
      signal?.removeEventListener('abort', onAbort);
    }

    try {
      const jsonText = await kaos.readText(outputFile);
      const output = JSON.parse(jsonText) as JestJsonOutput;
      const suites = parseJestJson(output);
      if (suites.length > 0) return suites;
    } catch { /* fall through */ }

    return [{
      file: absoluteTestPaths[0] ?? 'jest',
      status: 'failed',
      duration: 0,
      tests: [{
        name: 'jest failed to produce JSON report',
        status: 'failed',
        failureMessages: ['Jest JSON output missing or unparseable'],
      }],
    }];
  }

  private renderTemplate(
    detection: NodejsDetection,
    ident: string,
    feature: Feature,
  ): string {
    const replacer = (t: string) => t
      .replaceAll('{{ident}}', ident)
      .replaceAll('{{toolId}}', feature.toolId)
      .replaceAll('{{projectRoot}}', feature.projectRoot)
      .replaceAll('{{entry}}', detection.entry || 'index.js')
      .replaceAll('{{packageManager}}', detection.packageManager);

    switch (detection.kind) {
      case 'express': return replacer(EXPRESS_TEMPLATE);
      case 'nestjs': return replacer(NESTJS_TEMPLATE);
      case 'nextjs': return replacer(NEXTJS_TEMPLATE);
      default:
        return detection.entry
          ? replacer(GENERIC_NODE_TEMPLATE)
          : replacer(GENERIC_PLACEHOLDER_NODE_TEMPLATE);
    }
  }
}
```

### Step 3: Register in registry.ts

In `packages/agent-core/src/e2e-testing/registry.ts`, add the import and update registration order. The full file should be:

```typescript
import type { E2ETestGenerator } from './types';
import { E2ENoMatchingGeneratorError } from './errors';
import { TypeScriptVitestGenerator } from './generator';
import { NodejsJestGenerator } from './generators/nodejs-jest';
import { PythonPytestGenerator } from './generators/python-pytest';
import { GoGenerator } from './generators/go';

export class E2EGeneratorRegistry {
  private generators: E2ETestGenerator[] = [];

  register(generator: E2ETestGenerator): void {
    this.generators.push(generator);
  }

  async detectAndGet(projectRoot: string): Promise<E2ETestGenerator> {
    for (const generator of this.generators) {
      const structure = await generator.detectProjectStructure(projectRoot);
      if (structure !== null) return generator;
    }
    throw new E2ENoMatchingGeneratorError(projectRoot);
  }
}

export const registry = new E2EGeneratorRegistry();
registry.register(new TypeScriptVitestGenerator());
registry.register(new NodejsJestGenerator());
registry.register(new PythonPytestGenerator());
registry.register(new GoGenerator());
```

### Step 4: Add remaining tests and run

Append to `packages/agent-core/test/e2e-testing/nodejs-jest-generator.test.ts`:

```typescript
import type { Feature } from '#/e2e-testing/types';
import type { ResolvedE2EConfig } from '#/e2e-testing/config';

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
    expect(files[0].content).toContain('AUTO-GENERATED');
    expect(files[0].content).toContain('TODO');
    expect(files[0].content).toContain('express');
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
    expect(files[0].content).toContain('NestJS');
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
    expect(files[0].content).toContain('next dev');
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
    expect(files[0].content).toContain('spawn');
  });

  it('detects pnpm from lockfile', async () => {
    const root = makeNodeProject({
      'package.json': JSON.stringify({
        dependencies: { express: '^4' }, devDependencies: { jest: '^29' },
      }),
      'pnpm-lock.yaml': 'lockfileVersion: "6.0"',
    });
    const files = await gen.generateTestsForFeature(
      { toolId: 'web', changedFiles: [], projectRoot: root },
      '.ody-code/test-generated/e2e',
    );
    expect(files[0].content).toContain('pnpm');
  });
});

describe('NodejsJestGenerator.analyzeImpact', () => {
  const gen = new NodejsJestGenerator();

  it('maps changed files to their directory', () => {
    const result = gen.analyzeImpact(
      ['src/routes/index.ts', 'src/routes/auth.ts', 'tests/app.test.ts'], config,
    );
    expect(result.affectedTools).toHaveLength(1);
    expect(result.affectedTools[0].toolId).toBe('src/routes');
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
    expect(suites[0].tests[0].status).toBe('passed');
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
    expect(suites[0].status).toBe('failed');
    expect(suites[0].tests[0].failureMessages).toEqual(['expected 200 got 500']);
  });

  it('maps pending to skipped', () => {
    const output: JestJsonOutput = {
      testResults: [{
        name: '/abs/path/test.js', status: 'passed',
        assertionResults: [{ title: 'todo', status: 'pending' }],
      }],
    };
    const suites = parseJestJson(output);
    expect(suites[0].tests[0].status).toBe('skipped');
  });
});
```

Run all tests + typecheck:

```bash
pnpm vitest run packages/agent-core/test/e2e-testing/nodejs-jest-generator.test.ts
# Expected: ~15 tests pass

pnpm exec tsc --noEmit -p packages/agent-core/tsconfig.json
# Expected: no type errors
```

### Step 5: Commit

```bash
git add packages/agent-core/src/e2e-testing/generators/nodejs-jest.ts \
        packages/agent-core/src/e2e-testing/registry.ts \
        packages/agent-core/test/e2e-testing/nodejs-jest-generator.test.ts
git commit -m "feat(e2e): add Node.js/Jest E2E generator"
```


---

## Task 4: Recursive Impact Analyzer

**Depends on:** Task 1

**Files:** Create: `packages/agent-core/src/e2e-testing/recursive-impact-analyzer.ts`, `packages/agent-core/test/e2e-testing/recursive-impact-analyzer.test.ts`

Implement the `RecursiveImpactAnalyzer` class with pluggable language parsers (TypeScript/Node, Go, Python) and a BFS-based transitive dependency traversal. The analyzer builds a reverse dependency graph and walks it from changed files up to `maxDepth` hops.

### Step 1: Write failing tests

In `packages/agent-core/test/e2e-testing/recursive-impact-analyzer.test.ts`:

```typescript
import { describe, expect, it, afterAll, beforeAll } from 'vitest';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { RecursiveImpactAnalyzer } from '#/e2e-testing/recursive-impact-analyzer';

const tempRoots: string[] = [];

function makeProject(files: Record<string, string>): string {
  const root = mkdtempSync(join(tmpdir(), 'ria-e2e-'));
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

const analyzer = new RecursiveImpactAnalyzer();

describe('RecursiveImpactAnalyzer — TypeScript', () => {
  it('BFS: changed c.ts returns c, b, a for maxDepth=3', () => {
    const root = makeProject({
      'a.ts': `import { x } from './b';`,
      'b.ts': `import { y } from './c';`,
      'c.ts': `export const z = 1;`,
    });
    const result = analyzer.analyze(['c.ts'], root, 'typescript', { maxDepth: 3 });
    expect(new Set(result)).toEqual(new Set([
      join(root, 'c.ts'),
      join(root, 'b.ts'),
      join(root, 'a.ts'),
    ]));
  });

  it('BFS: maxDepth=1 only returns c and its direct dependents', () => {
    const root = makeProject({
      'a.ts': `import { x } from './b';`,
      'b.ts': `import { y } from './c';`,
      'c.ts': `export const z = 1;`,
    });
    const result = analyzer.analyze(['c.ts'], root, 'typescript', { maxDepth: 1 });
    expect(new Set(result)).toEqual(new Set([
      join(root, 'c.ts'),
      join(root, 'b.ts'),
    ]));
  });

  it('resolves import to index.ts', () => {
    const root = makeProject({
      'src/a.ts': `import { x } from './dir';`,
      'src/dir/index.ts': `import { y } from '../b';`,
      'b.ts': `export const y = 1;`,
    });
    const result = analyzer.analyze(['b.ts'], root, 'typescript', { maxDepth: 3 });
    expect(new Set(result)).toEqual(new Set([
      join(root, 'b.ts'),
      join(root, 'src/dir/index.ts'),
      join(root, 'src/a.ts'),
    ]));
  });

  it('third-party import resolves to null (ignored)', () => {
    const root = makeProject({
      'a.ts': `import lodash from 'lodash';`,
    });
    const result = analyzer.analyze(['a.ts'], root, 'typescript');
    // Only the changed file itself; no dependents since lodash is external
    expect(result).toHaveLength(1);
    expect(result[0]).toBe(join(root, 'a.ts'));
  });

  it('handles cyclic dependencies without infinite loop', () => {
    const root = makeProject({
      'a.ts': `import { x } from './b';`,
      'b.ts': `import { y } from './a';`,
    });
    const result = analyzer.analyze(['a.ts'], root, 'typescript');
    expect(result).toHaveLength(2);
  });
});

describe('RecursiveImpactAnalyzer — Python', () => {
  it('walks transitive Python imports', () => {
    const root = makeProject({
      'a.py': `from . import b`,
      'b.py': `from . import c`,
      'c.py': `x = 1`,
    });
    const result = analyzer.analyze(['c.py'], root, 'python', { maxDepth: 3 });
    expect(new Set(result)).toEqual(new Set([
      join(root, 'c.py'),
      join(root, 'b.py'),
      join(root, 'a.py'),
    ]));
  });

  it('resolves absolute import foo.bar', () => {
    const root = makeProject({
      'main.py': `import foo.bar`,
      'foo/__init__.py': '',
      'foo/bar.py': `x = 1`,
    });
    const result = analyzer.analyze(['foo/bar.py'], root, 'python');
    expect(new Set(result)).toEqual(new Set([
      join(root, 'foo/bar.py'),
      join(root, 'main.py'),
    ]));
  });
});

describe('RecursiveImpactAnalyzer — Go', () => {
  it('walks transitive Go imports within module', () => {
    const root = makeProject({
      'go.mod': 'module example.com/demo\n\ngo 1.22\n',
      'a.go': `package a\nimport "example.com/demo/b"`,
      'b.go': `package b\nimport "example.com/demo/c"`,
      'c.go': `package c`,
    });
    const result = analyzer.analyze(['c.go'], root, 'go');
    expect(new Set(result)).toEqual(new Set([
      join(root, 'c.go'),
      join(root, 'b.go'),
      join(root, 'a.go'),
    ]));
  });

  it('stdlib import is ignored', () => {
    const root = makeProject({
      'go.mod': 'module example.com/demo\n\ngo 1.22\n',
      'a.go': `package a\nimport "fmt"`,
    });
    const result = analyzer.analyze(['a.go'], root, 'go');
    expect(result).toHaveLength(1);
  });
});

describe('RecursiveImpactAnalyzer — exclusions', () => {
  it('excludes node_modules from scanning', () => {
    const root = makeProject({
      'src/a.ts': `import { x } from './b';`,
      'src/b.ts': `export const x = 1;`,
      'node_modules/pkg/index.ts': `import { z } from '../../src/b';`,
    });
    // node_modules/pkg/index.ts should NOT appear because
    // the default exclude patterns include 'node_modules'
    const result = analyzer.analyze(['src/b.ts'], root, 'typescript');
    expect(new Set(result)).toEqual(new Set([
      join(root, 'src/b.ts'),
      join(root, 'src/a.ts'),
    ]));
  });
});
```

Run to verify failure:

```bash
pnpm vitest run packages/agent-core/test/e2e-testing/recursive-impact-analyzer.test.ts
# Expected: FAIL — RecursiveImpactAnalyzer not found
```

### Step 2: Full implementation

Create `packages/agent-core/src/e2e-testing/recursive-impact-analyzer.ts`:

```typescript
import { join, dirname, extname } from 'pathe';

type SupportedLanguage = 'typescript' | 'go' | 'python' | 'nodejs';

interface RecursiveImpactOptions {
  maxDepth?: number;
  excludePatterns?: string[];
}

interface LanguageParser {
  extensions: string[];
  extractImports(content: string): string[];
  resolveImport(
    specifier: string,
    fromFile: string,
    projectRoot: string,
    existsSync: (p: string) => boolean,
  ): string | null;
}

// ------ TypeScript / Node.js Parser ------

const tsParser: LanguageParser = {
  extensions: ['.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs'],
  extractImports(content: string): string[] {
    const imports: string[] = [];
    // ES module: import { x } from './foo'; import './side-effect'
    for (const m of content.matchAll(/import\s+(?:[^'"]+\s+from\s+)?['"]([^'"]+)['"]/g)) {
      imports.push(m[1]);
    }
    // CommonJS: require('./foo')
    for (const m of content.matchAll(/require\s*\(\s*['"]([^'"]+)['"]\s*\)/g)) {
      imports.push(m[1]);
    }
    // Dynamic: import('./foo')
    for (const m of content.matchAll(/import\s*\(\s*['"]([^'"]+)['"]\s*\)/g)) {
      imports.push(m[1]);
    }
    return imports;
  },
  resolveImport(
    specifier: string,
    fromFile: string,
    _projectRoot: string,
    existsSync: (p: string) => boolean,
  ): string | null {
    if (!specifier.startsWith('.')) return null; // third-party
    return resolveRelativeModule(specifier, fromFile, tsParser.extensions, existsSync);
  },
};

function resolveRelativeModule(
  specifier: string,
  fromFile: string,
  extensions: string[],
  existsSync: (p: string) => boolean,
): string | null {
  const base = join(dirname(fromFile), specifier);
  const candidates: string[] = [
    base,
    ...extensions.map(ext => base + ext),
    ...extensions.map(ext => join(base, 'index' + ext)),
  ];
  for (const candidate of candidates) {
    if (existsSync(candidate)) return normalize(candidate);
  }
  return null;
}

// ------ Go Parser ------

const goParser: LanguageParser = {
  extensions: ['.go'],
  extractImports(content: string): string[] {
    const imports: string[] = [];
    // Single-line: import "fmt" or import alias "fmt"
    for (const m of content.matchAll(/import\s+(?:\w+\s+)?["']([^"']+)["']/g)) {
      imports.push(m[1]);
    }
    // Block: import ( ... )
    for (const m of content.matchAll(/import\s*\(([\s\S]*?)\)/g)) {
      for (const inner of m[1].matchAll(/["']([^"']+)["']/g)) {
        imports.push(inner[1]);
      }
    }
    return imports;
  },
  resolveImport(
    specifier: string,
    fromFile: string,
    projectRoot: string,
    existsSync: (p: string) => boolean,
  ): string | null {
    const moduleName = readGoModuleName(projectRoot, existsSync);
    if (moduleName !== null && specifier.startsWith(moduleName + '/')) {
      const relative = specifier.slice(moduleName.length + 1);
      const target = join(projectRoot, relative);
      // A Go import path maps to a directory
      if (existsSync(target)) {
        try {
          const entries = require('node:fs').readdirSync(target, { withFileTypes: true });
          if (entries.some(e => e.isFile() && e.name.endsWith('.go'))) {
            return normalize(target);
          }
        } catch { /* ignore */ }
      }
      // Also try appending file-like path
      for (const ext of goParser.extensions) {
        if (existsSync(target + ext)) return normalize(target + ext);
      }
      return null;
    }
    return null; // stdlib or third-party
  },
};

function readGoModuleName(
  projectRoot: string,
  existsSync: (p: string) => boolean,
): string | null {
  const goMod = join(projectRoot, 'go.mod');
  if (!existsSync(goMod)) return null;
  try {
    const content = require('node:fs').readFileSync(goMod, 'utf-8');
    for (const line of content.split('\n')) {
      const m = line.match(/^module\s+(\S+)/);
      if (m) return m[1];
    }
  } catch { /* ignore */ }
  return null;
}

// ------ Python Parser ------

const pyParser: LanguageParser = {
  extensions: ['.py'],
  extractImports(content: string): string[] {
    const imports: string[] = [];
    for (const line of content.split('\n')) {
      // import os, import foo.bar
      let m = line.match(/^\s*import\s+([a-zA-Z0-9_.]+)/);
      if (m) {
        imports.push(m[1]);
        continue;
      }
      // from foo import bar, from . import bar, from ..foo import bar
      m = line.match(/^\s*from\s+(\.?[a-zA-Z0-9_.]*)\s+import/);
      if (m) {
        imports.push(m[1]);
      }
    }
    return imports;
  },
  resolveImport(
    specifier: string,
    fromFile: string,
    projectRoot: string,
    existsSync: (p: string) => boolean,
  ): string | null {
    if (specifier === '' || specifier === '.') {
      return normalize(dirname(fromFile));
    }

    if (specifier.startsWith('.')) {
      return resolvePythonRelative(specifier, fromFile, existsSync);
    }

    return resolvePythonAbsolute(specifier, projectRoot, existsSync);
  },
};

function resolvePythonAbsolute(
  specifier: string,
  projectRoot: string,
  existsSync: (p: string) => boolean,
): string | null {
  const parts = specifier.split('.');
  const modulePath = join(projectRoot, ...parts) + '.py';
  if (existsSync(modulePath)) return normalize(modulePath);
  const packageInit = join(projectRoot, ...parts, '__init__.py');
  if (existsSync(packageInit)) return normalize(dirname(packageInit));
  return null;
}

function resolvePythonRelative(
  specifier: string,
  fromFile: string,
  existsSync: (p: string) => boolean,
): string | null {
  let dots = 0;
  while (dots < specifier.length && specifier[dots] === '.') dots++;
  let dir = dirname(fromFile);
  for (let i = 1; i < dots; i++) dir = dirname(dir);
  const rest = specifier.slice(dots).replace(/\./g, '/');
  if (rest === '') return normalize(dir);
  const modulePath = join(dir, rest) + '.py';
  if (existsSync(modulePath)) return normalize(modulePath);
  const packageInit = join(dir, rest, '__init__.py');
  if (existsSync(packageInit)) return normalize(dirname(packageInit));
  return null;
}

// ------ Analyzer ------

function normalize(p: string): string {
  return p.replace(/\\/g, '/');
}

export class RecursiveImpactAnalyzer {
  private parsers: Record<SupportedLanguage, LanguageParser> = {
    typescript: tsParser,
    nodejs: tsParser, // same parser as TypeScript
    go: goParser,
    python: pyParser,
  };

  analyze(
    changedFiles: string[],
    projectRoot: string,
    language: SupportedLanguage,
    options?: RecursiveImpactOptions,
  ): string[] {
    const maxDepth = options?.maxDepth ?? 3;
    const excludePatterns = options?.excludePatterns ?? [
      'node_modules', 'vendor', '.git', 'dist', 'build', 'coverage',
    ];

    const parser = this.parsers[language];
    if (!parser) return changedFiles.map(f => join(projectRoot, f));

    const { existsSync, readFileSync, readdirSync } = require('node:fs') as typeof import('node:fs');

    // 1. Collect source files
    const files = collectSourceFiles(
      projectRoot, parser.extensions, excludePatterns, existsSync, readdirSync,
    );

    // 2. Build reverse dependency graph
    const reverse = buildReverseGraph(
      files, parser, projectRoot, existsSync, readFileSync,
    );

    // 3. BFS over reverse edges
    const affected = new Set<string>();
    const resolvedChanged = changedFiles.map(f => {
      const abs = join(projectRoot, f.replace(/\\/g, '/'));
      return abs;
    });

    // First add all changed files
    for (const f of resolvedChanged) {
      if (files.has(f)) affected.add(f);
    }

    let frontier = resolvedChanged.filter(f => files.has(f));
    for (let depth = 0; depth < maxDepth && frontier.length > 0; depth++) {
      const next: string[] = [];
      for (const file of frontier) {
        const dependents = reverse.get(file) ?? [];
        for (const dependent of dependents) {
          if (!affected.has(dependent)) {
            affected.add(dependent);
            next.push(dependent);
          }
        }
      }
      frontier = next;
    }

    return [...affected].sort();
  }
}

function collectSourceFiles(
  root: string,
  extensions: string[],
  excludePatterns: string[],
  existsSync: typeof import('node:fs').existsSync,
  readdirSync: typeof import('node:fs').readdirSync,
): Set<string> {
  const results = new Set<string>();
  const stack = [root];
  const extSet = new Set(extensions);

  while (stack.length > 0) {
    const dir = stack.pop()!;
    let entries: import('node:fs').Dirent[];
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      const fullPath = normalize(join(dir, entry.name));
      const name = entry.name;
      if (entry.isDirectory()) {
        if (name.startsWith('.') || name.startsWith('_') || excludePatterns.includes(name)) continue;
        stack.push(fullPath);
      } else if (extSet.has(extname(name)) && !name.endsWith('.d.ts')) {
        results.add(fullPath);
      }
    }
  }
  return results;
}

function buildReverseGraph(
  files: Set<string>,
  parser: LanguageParser,
  projectRoot: string,
  existsSync: typeof import('node:fs').existsSync,
  readFileSync: typeof import('node:fs').readFileSync,
): Map<string, string[]> {
  const reverse = new Map<string, string[]>();

  for (const file of files) {
    let content: string;
    try {
      content = readFileSync(file, 'utf-8');
    } catch {
      continue;
    }
    const specifiers = parser.extractImports(content);
    for (const spec of specifiers) {
      const target = parser.resolveImport(spec, file, projectRoot, existsSync);
      if (target !== null && files.has(target)) {
        let deps = reverse.get(target);
        if (!deps) {
          deps = [];
          reverse.set(target, deps);
        }
        deps.push(file);
      }
    }
  }

  return reverse;
}
```

### Step 3: Run tests and verify

```bash
pnpm vitest run packages/agent-core/test/e2e-testing/recursive-impact-analyzer.test.ts
# Expected: 10 tests pass

pnpm exec tsc --noEmit -p packages/agent-core/tsconfig.json
# Expected: no type errors
```

### Step 4: Commit

```bash
git add packages/agent-core/src/e2e-testing/recursive-impact-analyzer.ts \
        packages/agent-core/test/e2e-testing/recursive-impact-analyzer.test.ts
git commit -m "feat(e2e): add recursive impact analyzer with language parsers"
```


---

## Task 5: Integrate Recursive Analysis into Generators

**Depends on:** Task 2, Task 3, Task 4

**Files:** Modify: `packages/agent-core/src/e2e-testing/generator.ts:85-86`, `packages/agent-core/src/e2e-testing/generators/go.ts:237-257`, `packages/agent-core/src/e2e-testing/generators/python-pytest.ts` (analyzeImpact method), `packages/agent-core/src/e2e-testing/generators/nodejs-jest.ts` (analyzeImpact method)

Wire the `RecursiveImpactAnalyzer` into all four generators. Each generator's `analyzeImpact` method prepends a recursive analysis step when `config.recursiveAnalysisEnabled` is true, then applies its existing directory-based priority logic to the expanded file set.

### Step 1: Modify TypeScriptVitestGenerator

In `packages/agent-core/src/e2e-testing/generator.ts`, add import at the top:

```typescript
import { RecursiveImpactAnalyzer } from './recursive-impact-analyzer';
```

Change `analyzeImpact` (lines 85-86):

```typescript
  analyzeImpact(changedFiles: string[], config: ResolvedE2EConfig): ImpactAnalysisResult {
    const filesToAnalyze = config.recursiveAnalysisEnabled
      ? new RecursiveImpactAnalyzer().analyze(
          changedFiles,
          process.cwd(), // projectRoot — TS generator runs in ody-code workspace
          'typescript',
          { maxDepth: config.maxRecursiveDepth },
        )
      : changedFiles;
    return ImpactAnalyzer.analyze(filesToAnalyze, config);
  }
```

### Step 2: Modify GoGenerator

In `packages/agent-core/src/e2e-testing/generators/go.ts`, add import:

```typescript
import { RecursiveImpactAnalyzer } from '../recursive-impact-analyzer';
```

Change `analyzeImpact` — insert at the beginning of the method body (before `const packages = new Set<string>()`):

```typescript
  analyzeImpact(changedFiles: string[], config: ResolvedE2EConfig): ImpactAnalysisResult {
    const filesToAnalyze = config.recursiveAnalysisEnabled
      ? new RecursiveImpactAnalyzer().analyze(
          changedFiles,
          this.currentRoot ?? process.cwd(),
          'go',
          { maxDepth: config.maxRecursiveDepth },
        )
      : changedFiles;
```

Then change the loop to use `filesToAnalyze` instead of `changedFiles`:

```typescript
    const packages = new Set<string>();
    for (const file of filesToAnalyze) {
```

Since the Go generator doesn't have a `currentRoot` field, we need to extract the project root from `changedFiles` or accept it as a parameter. The simplest approach: use the directory of the first changed file as root, or fall back to `process.cwd()`.

Actually, looking at how `analyzeImpact` is called in `run-e2e-tests.ts`:
```typescript
const impact = generator.analyzeImpact(changedFiles, config);
```

The `config` doesn't carry `projectRoot` in the current design. But looking at the design specs, the analyzer needs `projectRoot`. The simplest fix: we pass the project root through the `Feature` objects. But `analyzeImpact` doesn't receive `projectRoot` — it uses the file paths to derive packages.

For the recursive analyzer, we need a project root. The approach: iterate over changed files, find a common prefix directory. But that's fragile. Better approach: extract from changed files directly. Looking at the `RunE2ETestsTool`:

```typescript
const projectRoot = input.projectRoot ?? derivePackageRoot(changedFiles) ?? workspaceRoot;
```

So the project root is derived from changed files. A practical approach: the recursive analyzer can accept changed files and derive the root from them. Or we can modify `analyzeImpact` to accept an optional project root.

Let me keep it simple: For Go and TypeScript generators, determine the project root from the changed files by taking the root-most directory among all changed files that contains a manifest (go.mod or package.json). Or simpler: just use a `commonAncestor` approach.

Actually, looking at the design again:
```
analyze(changedFiles, projectRoot, language, options)
```

The `projectRoot` is needed. The callers already have it. But `analyzeImpact` doesn't receive it. The cleanest approach: update the `analyzeImpact` signature to accept `projectRoot` as an optional parameter. But that changes the shared `E2ETestGenerator` interface, which would affect all generators and callers.

Per the shared-signature rules, this change must happen in ONE task and update all callers. Let me handle this properly.

The `E2ETestGenerator` interface currently has:
```typescript
analyzeImpact(changedFiles: string[], config: ResolvedE2EConfig): ImpactAnalysisResult;
```

I'll add an optional `projectRoot` parameter:
```typescript
analyzeImpact(changedFiles: string[], config: ResolvedE2EConfig, projectRoot?: string): ImpactAnalysisResult;
```

Then update the call site in `RunE2ETestsTool`:
```typescript
const impact = generator.analyzeImpact(changedFiles, config, projectRoot);
```

And all generators get the `projectRoot` parameter (optional, defaulting to `process.cwd()`).

### Step-by-step:

**A. Update the interface** in `packages/agent-core/src/e2e-testing/types.ts` line 48-50:

```typescript
  analyzeImpact(
    changedFiles: string[],
    config: ResolvedE2EConfig,
    projectRoot?: string,
  ): ImpactAnalysisResult;
```

**B. Update call site** in `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.ts` line 62:

```typescript
    const impact = generator.analyzeImpact(changedFiles, config, projectRoot);
```

**C. Update TypeScript generator** in `packages/agent-core/src/e2e-testing/generator.ts`:

```typescript
import { RecursiveImpactAnalyzer } from './recursive-impact-analyzer';

  analyzeImpact(
    changedFiles: string[],
    config: ResolvedE2EConfig,
    projectRoot?: string,
  ): ImpactAnalysisResult {
    const filesToAnalyze = config.recursiveAnalysisEnabled
      ? new RecursiveImpactAnalyzer().analyze(
          changedFiles,
          projectRoot ?? process.cwd(),
          'typescript',
          { maxDepth: config.maxRecursiveDepth },
        )
      : changedFiles;
    return ImpactAnalyzer.analyze(filesToAnalyze, config);
  }
```

**D. Update Go generator** in `packages/agent-core/src/e2e-testing/generators/go.ts`:

```typescript
import { RecursiveImpactAnalyzer } from '../recursive-impact-analyzer';

  analyzeImpact(
    changedFiles: string[],
    config: ResolvedE2EConfig,
    projectRoot?: string,
  ): ImpactAnalysisResult {
    const filesToAnalyze = config.recursiveAnalysisEnabled
      ? new RecursiveImpactAnalyzer().analyze(
          changedFiles,
          projectRoot ?? process.cwd(),
          'go',
          { maxDepth: config.maxRecursiveDepth },
        )
      : changedFiles;

    const packages = new Set<string>();
    for (const file of filesToAnalyze) {
      // ... rest of existing implementation
```

**E. Update Python generator** — same pattern with language `'python'`.

**F. Update Node generator** — same pattern with language `'nodejs'`.

**G. Update the `AnalyzerLike` interface** in `plan-enricher.ts` (line 8) — it already uses `(changedFiles, config) => ImpactAnalysisResult`. Since `projectRoot` is optional, it's backward-compatible — the plan enricher can call it without `projectRoot`.

**H. Run the full test suite** to verify no regressions:

```bash
pnpm vitest run packages/agent-core/test/e2e-testing/
# Expected: all existing tests still pass

pnpm exec tsc --noEmit -p packages/agent-core/tsconfig.json
# Expected: no type errors
```

**I. Manual verification** — verify `plan-enricher.test.ts` still works:

```bash
pnpm vitest run packages/agent-core/test/e2e-testing/plan-enrichment.e2e.test.ts
```

### Commit:

```bash
git add packages/agent-core/src/e2e-testing/types.ts \
        packages/agent-core/src/e2e-testing/generator.ts \
        packages/agent-core/src/e2e-testing/generators/go.ts \
        packages/agent-core/src/e2e-testing/generators/python-pytest.ts \
        packages/agent-core/src/e2e-testing/generators/nodejs-jest.ts \
        packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.ts \
        packages/agent-core/src/e2e-testing/plan-enricher.ts
git commit -m "feat(e2e): integrate recursive impact analysis into all generators"
```


---

## Task 6: Test-Result Cache Module

**Depends on:** Task 1

**Files:** Create: `packages/agent-core/src/e2e-testing/result-cache.ts`, `packages/agent-core/test/e2e-testing/result-cache.test.ts`

Implement the `E2ETestResultCache` class and the `computeCacheKey` pure function. The cache stores `E2EExecutionResult` keyed by `hex(SHA256(sorted changed files + test content hash))`, with TTL eviction (7 days) and max-entry eviction (20 entries).

### Step 1: Write failing tests

In `packages/agent-core/test/e2e-testing/result-cache.test.ts`:

```typescript
import { describe, expect, it, afterAll, beforeEach, vi } from 'vitest';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync, readFileSync, existsSync } from 'node:fs';
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
    // Clean up all temp dirs
    const { rmSync, readdirSync } = require('node:fs') as typeof import('node:fs');
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
    // Should not throw
    await expect(disabledCache.set('any', sampleResult)).resolves.toBeUndefined();
  });

  it('prune removes expired entries', async () => {
    // Create an entry with a backdated createdAt timestamp
    const key = computeCacheKey(['old.ts'], []);
    const cacheDir = tempDir;
    mkdirSync(cacheDir, { recursive: true });
    const oldEntry = {
      createdAt: new Date(Date.now() - 8 * 24 * 60 * 60 * 1000).toISOString(), // 8 days ago
      key,
      result: sampleResult,
    };
    writeFileSync(join(cacheDir, key + '.json'), JSON.stringify(oldEntry, null, 2));

    // TTL is 7 days, so this entry should be pruned
    await cache.prune();
    expect(existsSync(join(cacheDir, key + '.json'))).toBe(false);
  });

  it('prune does not remove fresh entries', async () => {
    const key = computeCacheKey(['fresh.ts'], []);
    await cache.set(key, sampleResult);
    await cache.prune();
    // Fresh entry should still exist
    const cacheDir = tempDir;
    expect(existsSync(join(cacheDir, key + '.json'))).toBe(true);
  });

  it('prune enforces max entries', async () => {
    const smallConfig = { ...config, cacheDir: tempDir, cacheMaxEntries: 3 };
    const smallCache = new E2ETestResultCache(kaos, smallConfig);

    // Write 5 entries
    for (let i = 0; i < 5; i++) {
      const key = computeCacheKey([`file${i}.ts`], []);
      await smallCache.set(key, sampleResult);
      // Small delay to ensure different mtimes
      await new Promise(r => setTimeout(r, 10));
    }

    await smallCache.prune();

    const { readdirSync: rd } = require('node:fs') as typeof import('node:fs');
    const jsonFiles = rd(tempDir).filter(f => f.endsWith('.json'));
    expect(jsonFiles.length).toBeLessThanOrEqual(3);
  });
});
```

Run to verify failure:

```bash
pnpm vitest run packages/agent-core/test/e2e-testing/result-cache.test.ts
# Expected: FAIL — computeCacheKey and E2ETestResultCache not found
```

### Step 2: Full implementation

Create `packages/agent-core/src/e2e-testing/result-cache.ts`:

```typescript
import { createHash } from 'node:crypto';
import { join } from 'pathe';
import type { Kaos } from '@odysseythink/kaos';
import type { ResolvedE2EConfig } from './config';
import type { TestFile, E2EExecutionResult } from './types';

interface CacheEntry {
  createdAt: string;
  key: string;
  result: E2EExecutionResult;
}

interface CacheStats {
  hits: number;
  misses: number;
}

/**
 * Compute a deterministic cache key from the set of changed files and
 * generated test files. Normalises paths (backslash → forward slash),
 * sorts everything, and produces a 64-char hex SHA-256 digest.
 */
export function computeCacheKey(
  changedFiles: string[],
  testFiles: TestFile[],
): string {
  // Normalise and deduplicate changed file paths
  const normalizedChanged = [
    ...new Set(changedFiles.map(f => f.replace(/\\/g, '/'))),
  ].sort();

  // Hash generated test contents: relativePath + NUL + content, sorted
  const contentParts = testFiles
    .map(f => f.relativePath.replace(/\\/g, '/') + '\0' + f.content)
    .sort();
  const contentHash = createHash('sha256')
    .update(contentParts.join('\n'))
    .digest('hex');

  // Combine and produce final key
  const payload = normalizedChanged.join('\n') + '\n' + contentHash;
  return createHash('sha256').update(payload).digest('hex');
}

function isExpired(createdAt: string, ttlDays: number): boolean {
  const ageMs = Date.now() - new Date(createdAt).getTime();
  return ageMs > ttlDays * 24 * 60 * 60 * 1000;
}

export class E2ETestResultCache {
  private stats: CacheStats = { hits: 0, misses: 0 };

  constructor(
    private readonly kaos: Kaos,
    private readonly config: ResolvedE2EConfig,
  ) {}

  async get(key: string): Promise<E2EExecutionResult | null> {
    if (!this.config.cacheEnabled) {
      this.stats.misses += 1;
      return null;
    }

    const path = join(this.config.cacheDir, key + '.json');
    try {
      const { existsSync, readFileSync } = await import('node:fs');
      if (!existsSync(path)) {
        this.stats.misses += 1;
        return null;
      }

      const text = readFileSync(path, 'utf-8');
      const entry = JSON.parse(text) as CacheEntry;

      if (isExpired(entry.createdAt, this.config.cacheTtlDays)) {
        // Best-effort delete expired entry
        const { unlinkSync } = await import('node:fs');
        try { unlinkSync(path); } catch { /* ignore */ }
        this.stats.misses += 1;
        return null;
      }

      this.stats.hits += 1;
      return entry.result;
    } catch {
      this.stats.misses += 1;
      return null;
    }
  }

  async set(key: string, result: E2EExecutionResult): Promise<void> {
    if (!this.config.cacheEnabled) return;

    const cacheDir = this.config.cacheDir;
    try {
      const { mkdirSync, writeFileSync } = await import('node:fs');
      mkdirSync(cacheDir, { recursive: true });

      const entry: CacheEntry = {
        createdAt: new Date().toISOString(),
        key,
        result,
      };

      const path = join(cacheDir, key + '.json');
      writeFileSync(path, JSON.stringify(entry, null, 2));
    } catch {
      // Cache write is best-effort; do not fail the E2E run
      return;
    }

    await this.prune();
  }

  async prune(): Promise<void> {
    const cacheDir = this.config.cacheDir;
    const { existsSync, readdirSync, statSync, unlinkSync } = await import('node:fs');

    try {
      if (!existsSync(cacheDir)) return;

      const entries: Array<{ path: string; mtimeMs: number }> = [];
      const filenames = readdirSync(cacheDir);

      for (const filename of filenames) {
        if (!filename.endsWith('.json')) continue;
        const path = join(cacheDir, filename);
        try {
          const stat = statSync(path);
          if (isExpired(stat.mtime.toISOString(), this.config.cacheTtlDays)) {
            try { unlinkSync(path); } catch { /* ignore */ }
          } else {
            entries.push({ path, mtimeMs: stat.mtimeMs });
          }
        } catch {
          // File disappeared or is unreadable — skip
        }
      }

      // Enforce max-entries: keep only the N most recent
      if (entries.length > this.config.cacheMaxEntries) {
        entries.sort((a, b) => a.mtimeMs - b.mtimeMs);
        const toDelete = entries.slice(
          0,
          entries.length - this.config.cacheMaxEntries,
        );
        for (const entry of toDelete) {
          try { unlinkSync(entry.path); } catch { /* ignore */ }
        }
      }
    } catch {
      // Prune is best-effort
    }
  }

  resetStats(): void {
    this.stats = { hits: 0, misses: 0 };
  }
}
```

### Step 3: Run tests and verify

```bash
pnpm vitest run packages/agent-core/test/e2e-testing/result-cache.test.ts
# Expected: 10 tests pass

pnpm exec tsc --noEmit -p packages/agent-core/tsconfig.json
# Expected: no type errors
```

### Step 4: Commit

```bash
git add packages/agent-core/src/e2e-testing/result-cache.ts \
        packages/agent-core/test/e2e-testing/result-cache.test.ts
git commit -m "feat(e2e): add test-result cache layer with TTL and max-entry eviction"
```


---

## Task 7: Integrate Cache into Executor & RunE2ETestsTool

**Depends on:** Task 6

**Files:** Modify: `packages/agent-core/src/e2e-testing/executor.ts:47-106`, `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.ts:88-89`

Wire the `E2ETestResultCache` into the `E2ETestExecutor.execute()` method so that cache hits short-circuit test execution, and pass `changedFiles` from the tool to the executor.

### Step 1: Shared-signature change — update executor.execute() signature

The `execute` method currently has signature:
```typescript
async execute(testFiles: TestFile[], projectRoot: string, signal?: AbortSignal): Promise<E2EExecutionResult>
```

We need to add an `options` object with `changedFiles`. This is a shared-signature change — every caller must be updated.

**Search for callers:**
```
grep -rn "\.execute(" packages/agent-core/
```

Expected callers:
1. `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.ts:89` — `await executor.execute(testFiles, projectRoot, ctx.signal)`
2. `packages/agent-core/test/e2e-testing/executor.test.ts` — multiple test calls

### Step 2: Write the failing test (cache integration)

In `packages/agent-core/test/e2e-testing/executor.test.ts`, add this test after existing tests:

```typescript
import { computeCacheKey, E2ETestResultCache } from '#/e2e-testing/result-cache';

  it('short-circuits on cache hit', async () => {
    const kaos = fakeKaos();
    // Pre-populate cache
    const cache = new E2ETestResultCache(kaos, {
      ...defaultConfig,
      cacheEnabled: true,
      cacheDir: join(tmpdir(), 'exec-cache-' + Date.now()),
    });
    const testFile: TestFile = { relativePath: 'cached.test.ts', content: 'it("cached", () => {})' };
    const key = computeCacheKey(['src/a.ts'], [testFile]);
    const cachedResult: E2EExecutionResult = {
      passed: 5, failed: 0, skipped: 0, durationMs: 1,
      reportPath: '/tmp/cached.json',
      summary: 'Cached!',
      suites: [],
    };
    await cache.set(key, cachedResult);

    // Create executor with the same config
    const executor = new E2ETestExecutor(kaos, {
      ...defaultConfig,
      cacheEnabled: true,
      cacheDir: cache['config'].cacheDir, // access internal config
    }, tsGenerator);

    // This should hit the cache and NOT call exec (no vitest invocation)
    const result = await executor.execute([testFile], '/tmp', { changedFiles: ['src/a.ts'] });

    // Verify it returned the cached result without running vitest
    expect(result.passed).toBe(5);
    expect(result.summary).toBe('Cached!');
    // exec should NOT have been called for vitest
    const vitestCalls = (kaos.exec as ReturnType<typeof vi.fn>).mock.calls.filter(
      (call: string[]) => call[0] === 'pnpm' && call[1] === 'vitest',
    );
    expect(vitestCalls).toHaveLength(0);
  });
```

Wait — this test is complex because it depends on accessing the cache internal config. Let me simplify. The cache integration should be tested with a dedicated integration test.

Actually, the existing executor tests use fake Kaos with `exec: vi.fn()`. For the cache test, we need to:
1. Pre-populate a cache file on disk
2. Call `executor.execute()` with matching changedFiles and testFiles
3. Verify `kaos.exec` was NOT called (cache hit short-circuits)
4. Verify the returned result matches the cached result

Let me write a simpler test:

```typescript
  it('short-circuits on cache hit and does not invoke test runner', async () => {
    const cacheDir = mkdtempSync(join(tmpdir(), 'exec-cache-'));
    tempDirs.push(cacheDir);
    const testConfig = { ...defaultConfig, cacheEnabled: true, cacheDir };
    const kaos = fakeKaos();
    const executor = new E2ETestExecutor(kaos, testConfig, tsGenerator);

    // Pre-populate cache with a known key
    const testFile: TestFile = { relativePath: 'cached.test.ts', content: 'it("x",()=>{})' };
    const key = computeCacheKey(['src/foo.ts'], [testFile]);
    const cachedResult: E2EExecutionResult = {
      passed: 5, failed: 0, skipped: 0, durationMs: 1,
      reportPath: '/tmp/cached.json', summary: 'Cached result', suites: [],
    };
    const entryPath = join(cacheDir, key + '.json');
    mkdirSync(cacheDir, { recursive: true });
    writeFileSync(entryPath, JSON.stringify({
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
```

Add `tempDirs` array to the test file for cleanup, similar to the generator tests.

### Step 3: Modify executor.ts

In `packages/agent-core/src/e2e-testing/executor.ts`, add imports:

```typescript
import { E2ETestResultCache, computeCacheKey } from './result-cache';
```

Change the `execute` signature:

```typescript
  async execute(
    testFiles: TestFile[],
    projectRoot: string,
    options?: { changedFiles?: string[]; signal?: AbortSignal },
  ): Promise<E2EExecutionResult> {
    const signal = options?.signal;
    const changedFiles = options?.changedFiles ?? [];
    const start = Date.now();

    // Cache lookup: if we have a matching result, return it immediately
    if (this.config.cacheEnabled && testFiles.length > 0) {
      const cache = new E2ETestResultCache(this.kaos, this.config);
      const key = computeCacheKey(changedFiles, testFiles);
      const cached = await cache.get(key);
      if (cached !== null) return cached;

      // Store the cache instance for post-execution save
      await this.runAndCache(cache, key, testFiles, projectRoot, start, signal);
      // Note: the existing body continues in a refactored helper
    }

    const generatedTestDir = this.absPath(
      this.generator.resolveGeneratedTestDir(this.config),
      projectRoot,
    );
    // ... rest of existing execute body
```

Wait, this is getting complicated. Let me restructure more cleanly.

Actually, the simplest approach: add the cache logic at the top of the existing `execute` method, before file writing. On cache hit, return immediately. On cache miss, continue with existing logic and save result at the end.

Here's the minimal modification to `execute()`:

```typescript
  import { E2ETestResultCache, computeCacheKey } from './result-cache';

  // ...

  async execute(
    testFiles: TestFile[],
    projectRoot: string,
    options?: { changedFiles?: string[]; signal?: AbortSignal },
  ): Promise<E2EExecutionResult> {
    const signal = options?.signal;
    const changedFiles = options?.changedFiles ?? [];
    const start = Date.now();

    // --- Cache: try to short-circuit ---
    if (this.config.cacheEnabled && testFiles.length > 0) {
      const cache = new E2ETestResultCache(this.kaos, this.config);
      const key = computeCacheKey(changedFiles, testFiles);
      const cached = await cache.get(key);
      if (cached !== null) return cached;

      // Execute normally, then cache the result before returning
      const result = await this.executeUncached(testFiles, projectRoot, start, signal);
      await cache.set(key, result);
      return result;
    }

    return this.executeUncached(testFiles, projectRoot, start, signal);
  }

  private async executeUncached(
    testFiles: TestFile[],
    projectRoot: string,
    start: number,
    signal?: AbortSignal,
  ): Promise<E2EExecutionResult> {
    const generatedTestDir = this.absPath(
      this.generator.resolveGeneratedTestDir(this.config),
      projectRoot,
    );
    // ... rest of EXISTING execute body, from line 60 onwards
```

And extract the existing body (lines 59-106) into `executeUncached`.

### Step 4: Update RunE2ETestsTool caller

In `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.ts` line 89, change:

```typescript
    const result = await executor.execute(testFiles, projectRoot, ctx.signal);
```

to:

```typescript
    const result = await executor.execute(testFiles, projectRoot, {
      changedFiles,
      signal: ctx.signal,
    });
```

### Step 5: Update test callers

In `packages/agent-core/test/e2e-testing/executor.test.ts`, update all `.execute(` calls. The existing tests pass no `changedFiles`, which defaults to `[]`. These calls become:

```typescript
await executor.execute([], '/tmp')
// becomes:
await executor.execute([], '/tmp', {})

await executor.execute([testFile], '/tmp')
// becomes:
await executor.execute([testFile], '/tmp', {})

await executor.execute(files, '/tmp')
// becomes:
await executor.execute(files, '/tmp', {})
```

The signature change is backward-compatible for the case where the second argument was `signal`. Looking at the existing code:
```typescript
await executor.execute(testFiles, projectRoot, ctx.signal)
```
This becomes `{ signal: ctx.signal }`. For the test that passes just `signal`:
- Line 42: `await executor.execute([], '/tmp')` → `await executor.execute([], '/tmp', {})`
- Line 58: `await executor.execute([testFile], '/tmp')` → `await executor.execute([testFile], '/tmp', {})`
- Line 70: `await executor.execute(files, '/tmp')` → `await executor.execute(files, '/tmp', {})`

### Step 6: Run all tests

```bash
pnpm vitest run packages/agent-core/test/e2e-testing/
# Expected: ALL tests pass, including new cache integration test

pnpm exec tsc --noEmit -p packages/agent-core/tsconfig.json
# Expected: no type errors
```

### Step 7: Commit

```bash
git add packages/agent-core/src/e2e-testing/executor.ts \
        packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.ts \
        packages/agent-core/test/e2e-testing/executor.test.ts
git commit -m "feat(e2e): integrate result cache into executor with short-circuit"
```


---

## Self-Review

- [ ] 1. **Spec-coverage table**: map every spec section/requirement → Task(s), marked covered / GAP / no-op.

| Spec Requirement | Task(s) | Status |
|---|---|---|
| Config: `recursiveAnalysisEnabled`, `maxRecursiveDepth`, `cacheEnabled`, `cacheDir`, `cacheTtlDays`, `cacheMaxEntries` | Task 1 | covered |
| Python/pytest: detect via `pyproject.toml`, `requirements.txt`, `setup.py` | Task 2 | covered |
| Python/pytest: classify FastAPI, Flask, Django, generic | Task 2 | covered |
| Python/pytest: find entry points per framework | Task 2 | covered |
| Python/pytest: generate pytest templates with subprocess + HTTP/CLI assertions | Task 2 | covered |
| Python/pytest: run via `pytest --json-report` and parse `TestSuiteResult[]` | Task 2 | covered |
| Python/pytest: fallback to exit-code-only result when json-report unavailable | Task 2 | covered |
| Python/pytest: register in registry | Task 2 | covered |
| Plan-enricher: extend file regex to include `.py` | Task 2 | covered |
| Node.js/Jest: detect via `package.json` + jest dep/config | Task 3 | covered |
| Node.js/Jest: classify Express, NestJS, Next.js, generic | Task 3 | covered |
| Node.js/Jest: detect package manager from lockfile | Task 3 | covered |
| Node.js/Jest: generate jest templates with subprocess + HTTP/CLI assertions | Task 3 | covered |
| Node.js/Jest: run via `<pm> exec jest --json` and parse `TestSuiteResult[]` | Task 3 | covered |
| Node.js/Jest: register in registry before Python and Go | Task 3 | covered |
| Recursive analysis: BFS dependency traversal with maxDepth cap | Task 4 | covered |
| Recursive analysis: TypeScript/Node, Go, Python language parsers | Task 4 | covered |
| Recursive analysis: third-party imports resolve to null (project boundary) | Task 4 | covered |
| Recursive analysis: exclude patterns (node_modules, dist, etc.) | Task 4 | covered |
| Integrate recursive analysis into TS, Go, Python, Node generators | Task 5 | covered |
| `analyzeImpact` signature: add optional `projectRoot` parameter | Task 5 | covered |
| Cache: deterministic key from changed files + test content | Task 6 | covered |
| Cache: TTL eviction (7 days), max-entry eviction (20 entries) | Task 6 | covered |
| Cache: config toggles (`cacheEnabled`, `cacheDir`, etc.) | Task 6 | covered |
| Cache: integrate into executor with short-circuit | Task 7 | covered |
| Cache: pass `changedFiles` from RunE2ETestsTool to executor | Task 7 | covered |
| Existing TS/Vitest and Go generators unchanged (except registry + recursive integration) | Task 5 | covered |
| Existing static `TOOL_IMPACT_MAP` kept as fallback for ody-code self-testing | no-op (unchanged) | no-op |
| Feature flag: no new experimental flag (reuse `[e2e] enabled`) | no-op (uses existing flag) | no-op |
| Registry detection order: TS/Vitest → Node/Jest → Python → Go | Task 2, Task 3 | covered |

- [ ] 2. **Placeholder scan**: no TODO/TBD, no deferred-by-dependency excuses, no dead-code placeholders.
  - Reviewed all 7 tasks. Each task provides complete code (test + implementation). Generated templates contain intentional `// TODO:` comments directing users to customize endpoints — this is a PRODUCT REQUIREMENT from the design, not a placeholder. Template code is complete and functional without user edits (tests pass).
  - No `TODO` or `TBD` in plan prose that defers work to a later task.

- [ ] 3. **No phantom tasks**: every task produces a verifiable change; zero `--allow-empty` / "already done in Task N".
  - Task 1: 6 new schema fields + config test → verifiable change.
  - Task 2: new `python-pytest.ts` (464+ lines) + registry + plan-enricher + test file → verifiable change.
  - Task 3: new `nodejs-jest.ts` (400+ lines) + registry + test file → verifiable change.
  - Task 4: new `recursive-impact-analyzer.ts` + test file → verifiable change.
  - Task 5: modify 6 files (types.ts, generator.ts, go.ts, python-pytest.ts, nodejs-jest.ts, run-e2e-tests.ts) → verifiable change.
  - Task 6: new `result-cache.ts` + test file → verifiable change.
  - Task 7: modify executor + run-e2e-tests + test file → verifiable change.
  - Zero `--allow-empty` commits.

- [ ] 4. **Dependency soundness**: every `Depends on:` is satisfied by an earlier task; nothing references a symbol only a later task creates.
  - Task 1 → none.
  - Task 2 → Task 1 (config types).
  - Task 3 → Task 1 (config types).
  - Task 4 → Task 1 (config types for options).
  - Task 5 → Tasks 2, 3, 4 (generators + analyzer must exist before integration).
  - Task 6 → Task 1 (config types).
  - Task 7 → Task 6 (cache module must exist before integration).
  - Verified: no forward references. Every import in a task points to a symbol created in an earlier (or same) task.

- [ ] 5. **Caller & build soundness**: every shared-signature task updated all callers (incl. test files) and ends with a whole-tree typecheck.
  - Task 5 changes `E2ETestGenerator.analyzeImpact` signature (adds optional `projectRoot`). This task updates all 4 generators + the call site in `run-e2e-tests.ts`. The plan explicitly lists `grep -rn "\.execute(" packages/agent-core/` to find all callers. Test files `generator.test.ts` and `executor.test.ts` are updated.
  - Task 7 changes `E2ETestExecutor.execute` signature (replaces `signal?: AbortSignal` with `options?: { changedFiles?: string[]; signal?: AbortSignal }`). All callers are found and updated: `run-e2e-tests.ts` and `executor.test.ts`. The 3 test calls are listed with exact line numbers.
  - Both tasks end with `pnpm exec tsc --noEmit -p packages/agent-core/tsconfig.json`.
  - **Consumer trace for executor.execute**: the `RunE2ETestsTool` is the production consumer; its callsite at line 89 passes the new `{ changedFiles, signal }` object. The `run-e2e-tests.ts` file is explicitly listed in Task 7's Files section.

- [ ] 6. **Test-the-risk**: every state-mutating task has a behavioral test asserting the mutation.
  - Task 1: config test verifies 6 new defaults resolve via `E2EConfigResolver.resolve()`. Existing defaults also verified.
  - Task 2: 5 detection tests (null cases, fastapi, flask, django, generic guard), 4 template tests (content assertions), 3 impact analysis tests (filtering, strategy), 3 parse tests (passed, failed, empty).
  - Task 3: 6 detection tests, 5 template tests, 2 impact tests, 3 parse tests.
  - Task 4: 10 tests covering TS/Python/Go BFS, resolution, cyclic handling, exclusions.
  - Task 5: relies on Generator test files (existing + new) to pass after integration. Plan specifies running full `pnpm vitest run packages/agent-core/test/e2e-testing/`.
  - Task 6: 6 key computation tests (stability, ordering, sensitivity, normalization), 5 cache get/set/prune tests.
  - Task 7: cache integration test verifying vitest NOT called on cache hit. Existing executor tests passed.
  - **Regex must-survive verification**: Python dep extraction regex `/^\s*[\"']?([a-zA-Z0-9_-]+)/` was verified in the design (D2 table). Must-survive inputs: `fastapi`, `  fastapi` (leading spaces), `"flask"`. None contain substrings that would be incorrectly filtered.

- [ ] 7. **Type consistency**: types, signatures and property names used in later tasks match what earlier tasks defined.
  - `ResolvedE2EConfig` extended in Task 1 with 5 new fields. Tasks 2-7 reference `config.cacheEnabled`, `config.recursiveAnalysisEnabled`, etc. — all defined in Task 1.
  - `E2ETestGenerator` interface (from existing code) is referenced consistently in Tasks 2, 3, 5. The optional `projectRoot` parameter added in Task 5 is used by Tasks 2-3 implementations (they accept it as `projectRoot?: string`).
  - `E2EExecutionResult`, `TestFile`, `TestSuiteResult`, `RunContext` types from existing code are used consistently.
  - `computeCacheKey` signature `(changedFiles: string[], testFiles: TestFile[]): string` matches usage in Task 7.
  - `E2ETestResultCache` constructor `(kaos: Kaos, config: ResolvedE2EConfig)` matches usage in Task 7.
  - Cache key is 64-char hex (SHA-256) — `computeCacheKey` returns `string`. Cache file path is `join(cacheDir, key + '.json')`. Both consistent between Task 6 and Task 7.
  - Registry registration order: `TS Vitest → Node/Jest → Python → Go` — enforced by Task 2 (adds Python) and Task 3 (adds Node + reorders). Task 3 is dependent on Task 2 to avoid merge conflicts on `registry.ts`.
