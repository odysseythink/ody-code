import { z } from 'zod';
import type { Kaos } from '@odysseythink/kaos';
import type { Agent } from '#/agent';
import type { BuiltinTool } from '#/agent/tool';
import type { ExecutableToolResult, ToolExecution } from '#/loop/types';
import { toInputJsonSchema } from '#/tools/support/input-schema';
import { literalRulePattern, matchesGlobRuleSubject } from '#/tools/support/rule-match';
import { E2EConfigResolver } from '#/e2e-testing/config';
import type { OdyConfig } from '#/config/schema';
import { E2ETestExecutor } from '#/e2e-testing/executor';
import { registry } from '#/e2e-testing/registry';
import { parseGitStatusShort } from '#/e2e-testing/git-status';
import DESCRIPTION from './run-e2e-tests.md';

const RunE2ETestsInputSchema = z.object({
  toolId: z.string().optional().describe('Optional specific tool to test; if omitted, all affected tools are tested.'),
  projectRoot: z.string().optional().describe('Optional project root; defaults to the agent workspace root.'),
}).strict();

export type RunE2ETestsInput = z.infer<typeof RunE2ETestsInputSchema>;

export class RunE2ETestsTool implements BuiltinTool<RunE2ETestsInput> {
  readonly name = 'RunE2ETests' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(RunE2ETestsInputSchema);

  constructor(
    private readonly kaos: Kaos,
    private readonly agent: Agent,
  ) {}

  resolveExecution(input: RunE2ETestsInput): ToolExecution {
    const desc = input.toolId ? `Run E2E tests for ${input.toolId}` : 'Run E2E tests for affected tools';
    return {
      description: desc,
      approvalRule: literalRulePattern(this.name, input.toolId ?? '*'),
      matchesRule: (ruleArgs) => matchesGlobRuleSubject(ruleArgs, input.toolId ?? '*'),
      execute: (ctx) => this.execution(input, ctx),
    };
  }

  private async execution(
    input: RunE2ETestsInput,
    ctx: { signal: AbortSignal; turnId: string; toolCallId: string },
  ): Promise<ExecutableToolResult> {
    const config = E2EConfigResolver.resolve(this.agent.kimiConfig ?? ({} as OdyConfig));
    if (!config.enabled) {
      return { output: 'E2E testing is disabled in config.toml (e2e.enabled = false).' };
    }

    const workspaceRoot = this.kaos.getcwd();
    const changedFiles = await this.getChangedFiles(input.projectRoot ?? workspaceRoot);
    const projectRoot = input.projectRoot ?? derivePackageRoot(changedFiles) ?? workspaceRoot;

    let generator;
    try {
      generator = await registry.detectAndGet(projectRoot);
    } catch {
      return { output: `No E2E generator found for project at ${projectRoot}.` };
    }

    const impact = generator.analyzeImpact(changedFiles, config, projectRoot);
    if (input.toolId) {
      impact.affectedTools = impact.affectedTools.filter(t => t.toolId === input.toolId);
    }

    if (impact.affectedTools.length === 0) {
      return { output: 'No affected tools detected; skipping E2E tests.' };
    }

    const testFiles = [];
    for (const tool of impact.affectedTools) {
      const featureFiles = await generator.generateTestsForFeature(
        {
          toolId: tool.toolId,
          changedFiles,
          projectRoot,
        },
        config.generatedTestDir,
      );
      testFiles.push(...featureFiles);
    }

    if (testFiles.length === 0) {
      return { output: 'E2E generator produced no test files.' };
    }

    const executor = new E2ETestExecutor(this.kaos, config, generator);
    const result = await executor.execute(testFiles, projectRoot, {
      changedFiles,
      signal: ctx.signal,
    });

    if (ctx.signal.aborted) {
      return { isError: true, output: 'E2E tests cancelled.' };
    }

    const isError = result.failed > 0 && config.failurePolicy === 'block';
    return {
      output: result.summary,
      isError,
      stopTurn: isError,
      message: isError ? 'Critical E2E tests failed.' : undefined,
    };
  }

  private async getChangedFiles(projectRoot: string): Promise<string[]> {
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

/** Derive a package root from monorepo-style changed file paths. */
export function derivePackageRoot(changedFiles: string[]): string | undefined {
  for (const file of changedFiles) {
    const normalized = file.replace(/\\/g, '/');
    const parts = normalized.split('/');
    if ((parts[0] === 'packages' || parts[0] === 'apps') && parts.length >= 2) {
      return `${parts[0]}/${parts[1]}`;
    }
  }
  return undefined;
}
