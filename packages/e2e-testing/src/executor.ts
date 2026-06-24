import { dirname, isAbsolute, join } from 'pathe';
import type { Kaos } from '@odysseythink/kaos';
import type { ResolvedE2EConfig } from './config';
import type {
  E2ETestGenerator,
  TestFile,
  TestSuiteResult,
  E2EExecutionResult,
} from './types';
import { E2ETestResultCache, computeCacheKey } from './result-cache';

function timestamp(): string {
  return new Date().toISOString().replaceAll(/[:.]/g, '-');
}

function renderMarkdownSummary(result: E2EExecutionResult): string {
  const lines = [
    '## E2E Test Results',
    `- Passed: ${result.passed}`,
    `- Failed: ${result.failed}`,
    `- Skipped: ${result.skipped}`,
    `- Duration: ${result.durationMs}ms`,
    `- Report: ${result.reportPath}`,
  ];
  if (result.failed > 0) {
    lines.push('### Failures');
    for (const suite of result.suites) {
      for (const test of suite.tests) {
        if (test.status === 'failed') {
          lines.push(`- ${suite.file} > ${test.name}`);
          for (const msg of test.failureMessages.slice(0, 3)) {
            const truncated = msg.length > 200 ? msg.slice(0, 200) + '…' : msg;
            lines.push(`  ${truncated}`);
          }
        }
      }
    }
  }
  return lines.join('\n');
}

/**
 * Language-agnostic orchestrator: resolves directories, writes the generated
 * test files, chunks them by `maxConcurrency`, delegates the actual run +
 * output parsing to the per-language generator, then aggregates results,
 * writes a JSON report and renders a markdown summary.
 */
export class E2ETestExecutor {
  constructor(
    private readonly kaos: Kaos,
    private readonly config: ResolvedE2EConfig,
    private readonly generator: E2ETestGenerator,
  ) {}

  async execute(
    testFiles: TestFile[],
    projectRoot: string,
    options?: { changedFiles?: string[]; signal?: AbortSignal },
  ): Promise<E2EExecutionResult> {
    const changedFiles = options?.changedFiles ?? [];

    // --- Cache: try to short-circuit ---
    if (this.config.cacheEnabled && testFiles.length > 0) {
      const cache = new E2ETestResultCache(this.kaos, this.config);
      const key = computeCacheKey(changedFiles, testFiles);
      const cached = await cache.get(key);
      if (cached !== null) return cached;

      // Execute normally, then cache the result before returning
      const result = await this.executeUncached(testFiles, projectRoot, options?.signal);
      await cache.set(key, result);
      return result;
    }

    return this.executeUncached(testFiles, projectRoot, options?.signal);
  }

  private async executeUncached(
    testFiles: TestFile[],
    projectRoot: string,
    signal?: AbortSignal,
  ): Promise<E2EExecutionResult> {
    const start = Date.now();
    const generatedTestDir = this.absPath(
      this.generator.resolveGeneratedTestDir(this.config),
      projectRoot,
    );
    const reportDir = this.absPath(this.config.reportDir, projectRoot);

    await this.kaos.mkdir(generatedTestDir, { parents: true, existOk: true });

    const absolutePaths: string[] = [];
    for (const file of testFiles) {
      const absPath = join(generatedTestDir, file.relativePath);
      await this.kaos.mkdir(dirname(absPath), { parents: true, existOk: true });
      await this.kaos.writeText(absPath, file.content);
      absolutePaths.push(absPath);
    }

    if (absolutePaths.length === 0) {
      const reportPath = await this.writeReport(reportDir, [], 0, 0, 0, start);
      const summary = renderMarkdownSummary({
        passed: 0, failed: 0, skipped: 0, durationMs: Date.now() - start,
        reportPath, summary: '', suites: [],
      });
      return { passed: 0, failed: 0, skipped: 0, durationMs: Date.now() - start, reportPath, summary, suites: [] };
    }

    // The generator owns its own execution strategy (e.g. the TS generator
    // chunks by maxConcurrency to bound parallel vitest processes; the Go
    // generator runs each unique package dir once). Hand it the full file set.
    const allSuites = await this.generator.runTests(absolutePaths, {
      kaos: this.kaos,
      config: this.config,
      projectRoot,
      signal,
    });

    const passed = allSuites.reduce((s, suite) => s + suite.tests.filter(t => t.status === 'passed').length, 0);
    const failed = allSuites.reduce((s, suite) => s + suite.tests.filter(t => t.status === 'failed').length, 0);
    const skipped = allSuites.reduce((s, suite) => s + suite.tests.filter(t => t.status === 'skipped').length, 0);

    const reportPath = await this.writeReport(reportDir, allSuites, passed, failed, skipped, start);
    const summary = renderMarkdownSummary({
      passed, failed, skipped, durationMs: Date.now() - start,
      reportPath, summary: '', suites: allSuites,
    });

    return { passed, failed, skipped, durationMs: Date.now() - start, reportPath, summary, suites: allSuites };
  }

  private absPath(path: string, projectRoot: string): string {
    return isAbsolute(path) ? path : join(projectRoot, path);
  }

  private async writeReport(
    reportDir: string,
    suites: TestSuiteResult[],
    passed: number, failed: number, skipped: number,
    start: number,
  ): Promise<string> {
    await this.kaos.mkdir(reportDir, { parents: true, existOk: true });
    const filename = `e2e-report-${timestamp()}.json`;
    const path = join(reportDir, filename);
    const report = {
      generatedAt: new Date().toISOString(),
      durationMs: Date.now() - start,
      passed, failed, skipped,
      suites,
    };
    try {
      await this.kaos.writeText(path, JSON.stringify(report, null, 2));
      return path;
    } catch {
      return '<report write failed>';
    }
  }
}
