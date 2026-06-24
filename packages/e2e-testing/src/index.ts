export type {
  E2EPriority,
  ProjectStructure,
  Feature,
  TestFile,
  AffectedTool,
  ImpactAnalysisResult,
  RunContext,
  E2ETestGenerator,
  TestCaseResult,
  TestSuiteResult,
  E2EExecutionResult,
} from './types';

export type { ResolvedE2EConfig } from './config';
export { E2EConfigResolver } from './config';

export {
  E2EConfigValidationError,
  E2ENoMatchingGeneratorError,
} from './errors';

export { E2ETestExecutor } from './executor';
export { computeCacheKey, E2ETestResultCache } from './result-cache';
export { detectChangedFiles, parseGitStatusShort } from './git-status';
export { ImpactAnalyzer } from './impact-analyzer';
export { RecursiveImpactAnalyzer } from './recursive-impact-analyzer';
export { E2EPlanEnricher } from './plan-enricher';
export { TypeScriptVitestGenerator } from './generator';
export { E2EGeneratorRegistry, registry } from './registry';
export { NodejsJestGenerator, parseJestJson } from './generators/nodejs-jest';
export { PythonPytestGenerator, parsePytestJsonReport } from './generators/python-pytest';
export { GoGenerator, parseGoTestJson } from './generators/go';
