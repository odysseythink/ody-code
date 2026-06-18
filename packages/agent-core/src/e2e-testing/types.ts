import type { Kaos } from '@odysseythink/kaos';
import type { ResolvedE2EConfig } from './config';

export type E2EPriority = 'critical' | 'important' | 'nice-to-have';

export interface ProjectStructure {
  language: string;
  framework: string;
  testTool: string;
  root: string;
}

export interface Feature {
  toolId: string;
  changedFiles: string[];
  projectRoot: string;
  description?: string;
}

export interface TestFile {
  relativePath: string;
  content: string;
}

export interface AffectedTool {
  toolId: string;
  priority: E2EPriority;
}

export interface ImpactAnalysisResult {
  affectedTools: AffectedTool[];
}

export interface RunContext {
  kaos: Kaos;
  config: ResolvedE2EConfig;
  projectRoot: string;
  signal?: AbortSignal;
}

export interface E2ETestGenerator {
  readonly id: string;
  detectProjectStructure(root: string): Promise<ProjectStructure | null>;
  /**
   * Derive which "features" (ody builtin tools, Go packages, …) the changed
   * files affect. Each generator owns its own impact model.
   */
  analyzeImpact(
    changedFiles: string[],
    config: ResolvedE2EConfig,
    projectRoot?: string,
  ): ImpactAnalysisResult;
  generateTestsForFeature(feature: Feature, outputDir: string): Promise<TestFile[]>;
  /**
   * Where generated test files should be written, relative to (or absolute
   * within) the project root. Lets a generator override the configured dir —
   * e.g. Go must avoid `.`-prefixed dirs because the Go toolchain ignores them.
   */
  resolveGeneratedTestDir(config: ResolvedE2EConfig): string;
  /**
   * Execute the already-written test files and return normalized suites.
   * The executor handles writing files, chunking, report writing and the
   * markdown summary; the generator owns the language-specific subprocess
   * invocation and output parsing.
   */
  runTests(absoluteTestPaths: string[], ctx: RunContext): Promise<TestSuiteResult[]>;
}

export interface TestCaseResult {
  name: string;
  status: 'passed' | 'failed' | 'skipped' | 'pending';
  failureMessages: string[];
}

export interface TestSuiteResult {
  file: string;
  status: 'passed' | 'failed' | 'skipped';
  duration: number;
  tests: TestCaseResult[];
}

export interface E2EExecutionResult {
  passed: number;
  failed: number;
  skipped: number;
  durationMs: number;
  reportPath: string;
  summary: string;
  suites: TestSuiteResult[];
}
