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

export interface E2ETestGenerator {
  readonly id: string;
  detectProjectStructure(root: string): Promise<ProjectStructure | null>;
  generateTestsForFeature(feature: Feature, outputDir: string): Promise<TestFile[]>;
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
