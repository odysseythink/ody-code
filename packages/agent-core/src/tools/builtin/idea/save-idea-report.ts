import { z } from 'zod';
import type { Agent } from '../../../agent';
import type { BuiltinTool } from '../../../agent/tool';
import { ToolAccesses } from '../../../loop/tool-access';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import {
  buildIdeaReportBody,
  ensureIdeasDirectory,
  generateIdeaFilePath,
  isIdeaSkillActive,
  validateIdeaReportInput,
} from './report-helpers';
import SAVE_IDEA_REPORT_DESCRIPTION from './save-idea-report.md';

export const SaveIdeaReportInputSchema = z.object({
  title: z.string().describe('Short, filesystem-safe title for the report.'),
  content: z.string().describe('Full Markdown report body.'),
  type: z.enum(['generator', 'evaluator']).describe('Report kind.'),
  score: z
    .number()
    .min(0)
    .max(10)
    .optional()
    .describe('Final 0-10 score; required for evaluator reports.'),
  tags: z
    .array(z.string())
    .optional()
    .describe('Optional tags such as ["B2B", "AI"].'),
});

export type SaveIdeaReportInputValidated = z.infer<typeof SaveIdeaReportInputSchema>;

export class SaveIdeaReportTool implements BuiltinTool<SaveIdeaReportInputValidated> {
  readonly name = 'SaveIdeaReport' as const;
  readonly description = SAVE_IDEA_REPORT_DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(SaveIdeaReportInputSchema);

  constructor(private readonly agent: Agent) {}

  async resolveExecution(args: SaveIdeaReportInputValidated): Promise<ToolExecution> {
    if (!isIdeaSkillActive(this.agent.context?.history ?? [])) {
      return {
        isError: true,
        output:
          'SaveIdeaReport can only be used after idea-generator or idea-evaluator has been activated.',
      };
    }

    const validation = validateIdeaReportInput(args);
    if (!validation.ok) {
      return {
        isError: true,
        output: validation.error,
      };
    }

    const { data } = validation;
    const cwd = this.agent.config.cwd;
    const ideasDir = await ensureIdeasDirectory(cwd, this.agent.kaos);
    const filePath = await generateIdeaFilePath(ideasDir, data.title, new Date(), async (p) => {
      try {
        await this.agent.kaos.stat(p);
        return true;
      } catch {
        return false;
      }
    });

    const body = buildIdeaReportBody(data, new Date());

    return {
      accesses: ToolAccesses.writeFile(filePath),
      description: `Saving idea report to ${filePath}`,
      display: { kind: 'file_io', operation: 'write', path: filePath, content: body },
      approvalRule: this.name,
      execute: async () => this.execution(filePath, body),
    };
  }

  private async execution(
    filePath: string,
    body: string,
  ): Promise<ExecutableToolResult> {
    try {
      await this.agent.kaos.writeText(filePath, body);
      return { output: `Saved idea report to ${filePath}` };
    } catch (error) {
      return {
        isError: true,
        output: error instanceof Error ? error.message : String(error),
      };
    }
  }
}
