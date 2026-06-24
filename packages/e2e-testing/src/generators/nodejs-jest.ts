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

export interface JestJsonOutput {
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
  return candidates.length >= 1 ? relativePath(root, candidates[0]!) : '';
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
      pkg = JSON.parse(readFileSync(pkgPath, 'utf-8')) as Record<string, unknown>;
    } catch {
      return null;
    }

    const deps: Record<string, string> = {
      ...((pkg['dependencies'] as Record<string, string>) ?? {}),
      ...((pkg['devDependencies'] as Record<string, string>) ?? {}),
    };
    const hasJest = 'jest' in deps || 'jest' in pkg || existsJestConfig(existsSync, root);
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
    _projectRoot?: string,
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
      const pkg = JSON.parse(readFileSync(pkgPath, 'utf-8')) as Record<string, unknown>;
      deps = {
        ...((pkg['dependencies'] as Record<string, string>) ?? {}),
        ...((pkg['devDependencies'] as Record<string, string>) ?? {}),
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
