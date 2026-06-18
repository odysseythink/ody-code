import { join, extname } from 'pathe';
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

export interface PytestJsonReport {
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
  // Try TOML-style dependency array: `dependencies = ["fastapi", "uvicorn"]`
  const tomlMatch = manifest.match(/dependencies\s*=\s*\[([^\]]*)\]/);
  if (tomlMatch) {
    const pkgs = tomlMatch[1]!.matchAll(/["']([a-zA-Z0-9_-]+)["']/g);
    for (const p of pkgs) {
      const dep = p[1];
      if (dep) names.push(dep.toLowerCase());
    }
  }
  // Try setup.py style: `install_requires=["django"]`
  const setupMatch = manifest.match(/install_requires\s*=\s*\[([^\]]*)\]/);
  if (setupMatch) {
    const pkgs = setupMatch[1]!.matchAll(/["']([a-zA-Z0-9_-]+)["']/g);
    for (const p of pkgs) {
      const dep = p[1];
      if (dep) names.push(dep.toLowerCase());
    }
  }
  // Try requirements.txt style: one package name per line
  for (const line of manifest.split('\n')) {
    const m = line.match(/^\s*["']?([a-zA-Z0-9_-]+)/);
    if (m) {
      const name = m[1];
      if (name && !name.match(/^(project|dependencies|install_requires|setup|name|version|from|import|requires|build|tool)\b/)) {
        // Only add names that look like package names (not TOML/INI keys or Python keywords)
        names.push(name.toLowerCase());
      }
    }
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
  _existsSync: typeof import('node:fs').existsSync,
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
  _existsSync: typeof import('node:fs').existsSync,
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
  return candidates.length === 1 ? moduleName(root, candidates[0]!) : '';
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
