import { afterAll, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import type { Kaos, KaosProcess } from '@odysseythink/kaos';
import type { Readable, Writable } from 'node:stream';
import { createFakeKaos } from '../tools/fixtures/fake-kaos';
import { GoGenerator, parseGoTestJson } from '#/e2e-testing/generators/go';
import type { Feature, RunContext } from '#/e2e-testing/types';
import type { ResolvedE2EConfig } from '#/e2e-testing/config';

const tempRoots: string[] = [];

function makeGoProject(files: Record<string, string>): string {
  const root = mkdtempSync(join(tmpdir(), 'go-e2e-'));
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

const config: ResolvedE2EConfig = {
  enabled: true, strategy: 'smart', criticalTools: [], failurePolicy: 'warn',
  maxConcurrency: 4, testTimeout: 30000,
  reportDir: '.ody-code/test-reports', generatedTestDir: '.ody-code/test-generated/e2e',
  recursiveAnalysisEnabled: true, maxRecursiveDepth: 3,
  cacheEnabled: true, cacheDir: '.ody-code/e2e-cache', cacheTtlDays: 7, cacheMaxEntries: 20,
};

function feature(toolId: string, projectRoot: string): Feature {
  return { toolId, changedFiles: [], projectRoot };
}

describe('GoGenerator.detectProjectStructure', () => {
  it('returns null without go.mod', async () => {
    const root = mkdtempSync(join(tmpdir(), 'no-go-'));
    tempRoots.push(root);
    expect(await new GoGenerator().detectProjectStructure(root)).toBeNull();
  });

  it('detects gin http-server from go.mod', async () => {
    const root = makeGoProject({
      'go.mod': 'module demo\n\ngo 1.22\n\nrequire github.com/gin-gonic/gin v1.9.1\n',
      'main.go': 'package main\nfunc main() {}\n',
    });
    const result = await new GoGenerator().detectProjectStructure(root);
    expect(result).toEqual({ language: 'go', framework: 'gin', testTool: 'go test', root });
  });

  it('detects net/http http-server from source scan', async () => {
    const root = makeGoProject({
      'go.mod': 'module demo\n\ngo 1.22\n',
      'main.go': 'package main\n\nimport "net/http"\n\nfunc main() {\n\thttp.ListenAndServe(":8080", nil)\n}\n',
    });
    const result = await new GoGenerator().detectProjectStructure(root);
    expect(result?.framework).toBe('net-http');
  });

  it('classifies a plain main package as cli', async () => {
    const root = makeGoProject({
      'go.mod': 'module demo\n\ngo 1.22\n',
      'main.go': 'package main\n\nimport "fmt"\n\nfunc main() {\n\tfmt.Println("hi")\n}\n',
    });
    const result = await new GoGenerator().detectProjectStructure(root);
    expect(result?.framework).toBe('cli');
  });
});

describe('GoGenerator.generateTestsForFeature', () => {
  it('produces a real spawn http-server test with no leftover placeholders', async () => {
    const root = makeGoProject({
      'go.mod': 'module demo\n\ngo 1.22\n\nrequire github.com/gin-gonic/gin v1.9.1\n',
      'main.go': 'package main\nfunc main() {}\n',
    });
    const files = await new GoGenerator().generateTestsForFeature(feature('internal/api', root), 'e2e_generated');
    expect(files).toHaveLength(1);
    const f = files[0]!;
    expect(f.relativePath).toBe('internal_api_e2e_test.go');
    expect(f.content).not.toContain('{{');
    expect(f.content).not.toContain('}}');
    expect(f.content.startsWith('//go:build e2e')).toBe(true);
    expect(f.content).toContain('package e2e');
    expect(f.content).toContain('exec.Command("go", "build"');
    expect(f.content).toContain('exec.CommandContext');
    expect(f.content).toContain('client.Get(');
    expect(f.content).toContain('json.NewDecoder(resp.Body).Decode');
    expect(f.content).toContain('func Test_internal_api_E2E(t *testing.T)');
    expect(f.content).toContain(JSON.stringify(root)); // moduleRoot embedded
  });

  it('defaults buildTarget to a single ./cmd/<name> layout', async () => {
    const root = makeGoProject({
      'go.mod': 'module demo\n\ngo 1.22\n\nrequire github.com/gin-gonic/gin v1.9.1\n',
      'cmd/server/main.go': 'package main\nfunc main() {}\n',
    });
    const files = await new GoGenerator().generateTestsForFeature(feature('internal/api', root), 'e2e_generated');
    expect(files[0]!.content).toContain('buildTarget := "./cmd/server"');
  });

  it('produces a generic placeholder for cli projects', async () => {
    const root = makeGoProject({
      'go.mod': 'module demo\n\ngo 1.22\n',
      'main.go': 'package main\n\nimport "fmt"\n\nfunc main() { fmt.Println("x") }\n',
    });
    const files = await new GoGenerator().generateTestsForFeature(feature('cmd/tool', root), 'e2e_generated');
    const f = files[0]!;
    expect(f.content).toContain('package e2e');
    expect(f.content).toContain('t.Log("e2e placeholder for cmd/tool")');
    expect(f.content).not.toContain('{{');
  });
});

describe('GoGenerator.analyzeImpact', () => {
  it('groups changed .go files by package directory', () => {
    const result = new GoGenerator().analyzeImpact(
      ['internal/api/handler.go', 'internal/api/router.go', 'cmd/main.go', 'README.md'],
      config,
    );
    const ids = result.affectedTools.map((t) => t.toolId).sort();
    expect(ids).toEqual(['cmd', 'internal/api']);
    expect(result.affectedTools.every((t) => t.priority === 'important')).toBe(true);
  });

  it('skips _test.go files', () => {
    const result = new GoGenerator().analyzeImpact(['internal/api/handler_test.go'], config);
    expect(result.affectedTools).toHaveLength(0);
  });

  it('honors critical-only with configured critical packages', () => {
    const result = new GoGenerator().analyzeImpact(
      ['internal/api/handler.go', 'internal/util/x.go'],
      { ...config, strategy: 'critical-only', criticalTools: ['internal/api'] },
    );
    expect(result.affectedTools).toEqual([{ toolId: 'internal/api', priority: 'critical' }]);
  });

  it('injects general for always strategy with no go changes', () => {
    const result = new GoGenerator().analyzeImpact(['README.md'], { ...config, strategy: 'always' });
    expect(result.affectedTools).toEqual([{ toolId: 'general', priority: 'nice-to-have' }]);
  });
});

describe('GoGenerator.resolveGeneratedTestDir', () => {
  it('falls back to a non-dot dir when configured dir is hidden', () => {
    expect(new GoGenerator().resolveGeneratedTestDir(config)).toBe('e2e_generated');
  });

  it('keeps a non-dot configured dir', () => {
    expect(new GoGenerator().resolveGeneratedTestDir({ ...config, generatedTestDir: 'test/e2e' })).toBe('test/e2e');
  });
});

function fakeGoProcess(opts: { stdout?: string; wait?: Promise<number>; kill?: ReturnType<typeof vi.fn> }): KaosProcess {
  let onData: ((b: Buffer) => void) | undefined;
  return {
    stdin: { end: vi.fn(), write: vi.fn() } as unknown as Writable,
    stdout: { on: (ev: string, cb: (b: Buffer) => void) => { if (ev === 'data') onData = cb; } } as unknown as Readable,
    stderr: { on: vi.fn() } as unknown as Readable,
    pid: 1,
    exitCode: 0,
    wait: vi.fn().mockImplementation(async () => {
      if (opts.stdout !== undefined) onData?.(Buffer.from(opts.stdout));
      return opts.wait ?? 0;
    }),
    kill: opts.kill ?? vi.fn().mockResolvedValue(undefined),
  } as unknown as KaosProcess;
}

function goCtx(kaos: Kaos, signal?: AbortSignal): RunContext {
  return { kaos, config, projectRoot: '/proj', signal };
}

describe('GoGenerator.runTests', () => {
  it('runs `go test` once with deduped package dirs regardless of file count', async () => {
    const stream = [
      { Action: 'pass', Package: 'proj/e2e_generated', Test: 'Test_a_E2E' },
      { Action: 'pass', Package: 'proj/e2e_generated', Test: 'Test_b_E2E' },
    ].map((e) => JSON.stringify(e)).join('\n');
    const exec = vi.fn().mockResolvedValue(fakeGoProcess({ stdout: stream }));
    const kaos = createFakeKaos({ exec });

    const suites = await new GoGenerator().runTests(
      ['/proj/e2e_generated/a_e2e_test.go', '/proj/e2e_generated/b_e2e_test.go'],
      goCtx(kaos),
    );

    expect(exec).toHaveBeenCalledTimes(1);
    const call = exec.mock.calls[0] as string[];
    expect(call[0]).toBe('go');
    expect(call.filter((a) => a.startsWith('./'))).toEqual(['./e2e_generated/']);
    expect(suites.flatMap((s) => s.tests).filter((t) => t.status === 'passed')).toHaveLength(2);
  });

  it('kills the process when the run is aborted', async () => {
    const kill = vi.fn().mockResolvedValue(undefined);
    let resolveWait: (n: number) => void = () => {};
    const waitP = new Promise<number>((r) => { resolveWait = r; });
    const exec = vi.fn().mockResolvedValue(fakeGoProcess({ wait: waitP, kill }));
    const kaos = createFakeKaos({ exec });

    const controller = new AbortController();
    const promise = new GoGenerator().runTests(['/proj/e2e_generated/a_e2e_test.go'], goCtx(kaos, controller.signal));
    controller.abort();
    resolveWait(0);
    await promise;

    expect(kill).toHaveBeenCalled();
  });
});

describe('parseGoTestJson', () => {
  it('parses a mix of pass / fail / skip into normalized suites', () => {
    const stream = [
      { Action: 'run', Package: 'demo/e2e', Test: 'Test_A_E2E' },
      { Action: 'pass', Package: 'demo/e2e', Test: 'Test_A_E2E' },
      { Action: 'run', Package: 'demo/e2e', Test: 'Test_B_E2E' },
      { Action: 'output', Package: 'demo/e2e', Test: 'Test_B_E2E', Output: 'boom assertion failed\n' },
      { Action: 'fail', Package: 'demo/e2e', Test: 'Test_B_E2E' },
      { Action: 'skip', Package: 'demo/e2e', Test: 'Test_C_E2E' },
    ].map((e) => JSON.stringify(e)).join('\n');

    const suites = parseGoTestJson(stream);
    expect(suites).toHaveLength(1);
    const suite = suites[0]!;
    expect(suite.file).toBe('demo/e2e');
    expect(suite.status).toBe('failed');
    const byName = Object.fromEntries(suite.tests.map((t) => [t.name, t]));
    expect(byName['Test_A_E2E']!.status).toBe('passed');
    expect(byName['Test_B_E2E']!.status).toBe('failed');
    expect(byName['Test_B_E2E']!.failureMessages[0]).toContain('boom assertion failed');
    expect(byName['Test_C_E2E']!.status).toBe('skipped');
  });

  it('surfaces a package-level build failure as a failed test', () => {
    const stream = [
      { Action: 'output', Package: 'demo/e2e', Output: './x.go:1: undefined: Foo\n' },
      { Action: 'fail', Package: 'demo/e2e' },
    ].map((e) => JSON.stringify(e)).join('\n');
    const suites = parseGoTestJson(stream);
    expect(suites[0]!.status).toBe('failed');
    expect(suites[0]!.tests[0]!.name).toContain('(package)');
    expect(suites[0]!.tests[0]!.failureMessages[0]).toContain('undefined: Foo');
  });

  it('returns no suites for empty output', () => {
    expect(parseGoTestJson('')).toEqual([]);
  });
});
