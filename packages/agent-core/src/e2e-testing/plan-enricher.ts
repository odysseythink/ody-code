import type { Kaos } from '@odysseythink/kaos';
import type { ResolvedE2EConfig } from './config';
import type { ImpactAnalysisResult } from './types';
import type { AffectedTool } from './types';
import { parseGitStatusShort } from './git-status';

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
    const fromGit = await this.gitStatusFiles(projectRoot);
    if (fromGit.length > 0) return fromGit;
    return extractFilePathsFromPlan(planContent);
  }

  private async gitStatusFiles(projectRoot: string): Promise<string[]> {
    try {
      const k = this.kaos.withCwd(projectRoot);
      const proc = await k.exec('git', 'status', '--short', '--no-renames');
      const chunks: Buffer[] = [];
      proc.stdout.on('data', (chunk: Buffer) => chunks.push(chunk));
      await proc.wait();
      const output = Buffer.concat(chunks).toString('utf-8');
      return parseGitStatusShort(output);
    } catch {
      return [];
    }
  }
}

function extractFilePathsFromPlan(planContent: string): string[] {
  const regex = /(?:packages|apps)\/[a-zA-Z0-9\-_/.]+\.[jt]sx?/g;
  const matches = planContent.match(regex) ?? [];
  return [...new Set(matches)];
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

Based on the changed files, validate the following tools:
${priorityText}

Use the RunE2ETests tool after completing the implementation tasks above.
`;

  return content.trimEnd() + section + '\n';
}
