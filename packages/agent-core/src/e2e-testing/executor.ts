import { dirname, isAbsolute, join } from 'pathe';
import type { Kaos } from '@odysseythink/kaos';
import type { ResolvedE2EConfig } from './config';
import type {
  TestFile,
  TestCaseResult,
  TestSuiteResult,
  E2EExecutionResult,
} from './types';

function timestamp(): string {
  return new Date().toISOString().replaceAll(/[:.]/g, '-');
}

function parseVitestJson(json: Record<string, unknown>): TestSuiteResult[] {
  const testResults = (json['testResults'] ?? []) as Array<Record<string, unknown>>;
  return testResults.map((suite) => {
    const assertions = (suite['assertionResults'] ?? []) as Array<Record<string, unknown>>;
    const tests: TestCaseResult[] = assertions.map((a) => ({
      name: String(a['title'] ?? ''),
      status: (a['status'] as TestCaseResult['status']) ?? 'failed',
      failureMessages: (a['failureMessages'] as string[]) ?? [],
    }));
    return {
      file: String(suite['name'] ?? ''),
      status: (suite['status'] as TestSuiteResult['status']) ?? 'failed',
      duration: ((suite['endTime'] as number) ?? 0) - ((suite['startTime'] as number) ?? 0),
      tests,
    };
  });
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

export class E2ETestExecutor {
  constructor(
    private readonly kaos: Kaos,
    private readonly config: ResolvedE2EConfig,
  ) {}

  async execute(testFiles: TestFile[], projectRoot: string): Promise<E2EExecutionResult> {
    const start = Date.now();
    const generatedTestDir = this.absPath(this.config.generatedTestDir, projectRoot);
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

    const chunkSize = this.config.maxConcurrency;
    const allSuites: TestSuiteResult[] = [];

    for (let i = 0; i < absolutePaths.length; i += chunkSize) {
      const chunk = absolutePaths.slice(i, i + chunkSize);
      const result = await this.runVitestChunk(chunk, generatedTestDir, projectRoot);
      allSuites.push(...result);
    }

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

  private async runVitestChunk(
    files: string[],
    generatedTestDir: string,
    projectRoot: string,
  ): Promise<TestSuiteResult[]> {
    const outputFile = join(generatedTestDir, `.vitest-output-${timestamp()}.json`);
    const k = this.kaos.withCwd(projectRoot);
    const proc = await k.exec(
      'pnpm', 'vitest', 'run',
      '--reporter=json',
      `--outputFile=${outputFile}`,
      `--testTimeout=${this.config.testTimeout}`,
      ...files,
    );

    const stdoutChunks: Buffer[] = [];
    const stderrChunks: Buffer[] = [];
    proc.stdout.on('data', (chunk: Buffer) => stdoutChunks.push(chunk));
    proc.stderr.on('data', (chunk: Buffer) => stderrChunks.push(chunk));

    await proc.wait();

    try {
      const json = JSON.parse(await this.kaos.readText(outputFile)) as Record<string, unknown>;
      const suites = parseVitestJson(json);
      await this.tryRemove(outputFile);
      return suites;
    } catch {
      await this.tryRemove(outputFile);
      return [{
        file: '<vitest crash>',
        status: 'failed' as const,
        duration: 0,
        tests: [{
          name: 'vitest process failed',
          status: 'failed' as const,
          failureMessages: [Buffer.concat(stderrChunks).toString('utf-8').slice(0, 500)],
        }],
      }];
    }
  }

  private async tryRemove(path: string): Promise<void> {
    try {
      await this.kaos.exec('rm', path);
    } catch {
      // best-effort cleanup
    }
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
