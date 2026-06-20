import type { Kaos } from '@odysseythink/kaos';
import type { ResolvedE2EConfig } from './config';
import type { ImpactAnalysisResult } from './types';
import type { AffectedTool } from './types';
import { detectChangedFiles } from './git-status';

interface AnalyzerLike {
  analyze(changedFiles: string[], config: ResolvedE2EConfig): ImpactAnalysisResult;
}

const E2E_ENRICHMENT_MARKER = '<!-- e2e-enriched -->';

export class E2EPlanEnricher {
  constructor(
    private readonly kaos: Kaos,
    private readonly config: ResolvedE2EConfig,
    private readonly analyzer: AnalyzerLike,
  ) {}

  async enrich(_planPath: string, planContent: string, projectRoot: string): Promise<string | null> {
    if (!this.config.enabled) return null;

    // Avoid adding the E2E task section more than once.
    if (planContent.includes(E2E_ENRICHMENT_MARKER)) return null;

    const changedFiles = await this.determineChangedFiles(projectRoot, planContent);
    if (changedFiles.length === 0 && this.config.strategy !== 'always') return null;

    const impact = this.analyzer.analyze(changedFiles, this.config);

    if (impact.affectedTools.length === 0 && this.config.strategy !== 'always') return null;

    return appendE2ETaskToMarkdown(planContent, impact.affectedTools);
  }

  private async determineChangedFiles(projectRoot: string, planContent: string): Promise<string[]> {
    // Real changes (uncommitted + committed-since-base) take priority. But at
    // plan-exit the implementation usually hasn't been written yet, so fall back
    // to the file paths the plan itself declares — that is the reliable signal at
    // planning time, before any code exists.
    const fromGit = await detectChangedFiles(this.kaos, projectRoot);
    if (fromGit.length > 0) return fromGit;
    return extractFilePathsFromPlan(planContent);
  }
}

/**
 * Extract source-file paths a plan declares it will touch. Matches path-like
 * tokens (must contain a `/`) ending in a known source extension, so it works
 * for any user project language (Go, Rust, Java, …) — not just the TS monorepo.
 */
function extractFilePathsFromPlan(planContent: string): string[] {
  const regex = /[\w.\-/]+\.(?:go|ts|tsx|js|jsx|mjs|cjs|py|rs|java|rb|kt|php|swift|scala)\b/g;
  const matches = planContent.match(regex) ?? [];
  return [...new Set(matches)].filter((p) => p.includes('/'));
}

function appendE2ETaskToMarkdown(content: string, affectedTools: readonly AffectedTool[]): string {
  const lines = content.split('\n');
  let lastTaskNum = 0;
  for (const line of lines) {
    const match = line.match(/^#{1,4}\s+Task\s+(\d+)\s*[:\-]?/i);
    if (match) {
      lastTaskNum = Math.max(lastTaskNum, parseInt(match[1]!, 10));
    }
  }

  const newTaskNum = lastTaskNum + 1;
  const priorityText = affectedTools
    .map(t => `- ${t.toolId} (priority: ${t.priority})`)
    .join('\n');

  const section = `
${E2E_ENRICHMENT_MARKER}

### Task ${newTaskNum}: Generate and run E2E tests

Based on the changed files, validate the following areas:
${priorityText}

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. You may also use the
RunE2ETests tool to scaffold and run E2E tests.
`;

  return content.trimEnd() + section + '\n';
}
