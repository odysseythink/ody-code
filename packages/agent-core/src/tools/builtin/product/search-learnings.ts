import type { Agent } from '#agent';
import { t } from '../../../i18n';
import { z } from 'zod';

import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './search-learnings.md';

export const SearchLearningsInputSchema = z.object({
  limit: z.number().int().positive().default(10).describe('Maximum number of learnings to return.'),
  crossProject: z.boolean().optional().describe('Whether to search across all projects (true) or only the current project (false).'),
}).strict();
export type SearchLearningsInput = z.infer<typeof SearchLearningsInputSchema>;

export class SearchLearningsTool implements BuiltinTool<SearchLearningsInput> {
  readonly name = 'SearchLearnings' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(SearchLearningsInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: SearchLearningsInput): ToolExecution {
    return {
      description: 'Searching past learnings',
      approvalRule: this.name,
      execute: async () => {
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'product') {
          return {
            isError: true,
            output: t('product.modeNotActive', this.agent.userLanguage),
          };
        }

        try {
          const learnings = await this.agent.productStateStore.searchLearnings({
            limit: args.limit,
            crossProject: args.crossProject,
          });

          if (learnings.length === 0) {
            return {
              output: t('product.noLearnings', this.agent.userLanguage),
            };
          }

          const lang = this.agent.userLanguage;
          const formatted = learnings.map((l, i) =>
            `[${i + 1}] ${t('product.learningTypeLabel', lang)}: ${l.type.toUpperCase()}: ${l.key}\n    ${t('product.learningInsightLabel', lang)}: ${l.insight}\n    ${t('product.learningConfidenceLabel', lang)}: ${l.confidence}\n    ${t('product.learningDateLabel', lang)}: ${l.ts}${l.branch ? `\n    ${t('product.learningBranchLabel', lang)}: ${l.branch}` : ''}`
          ).join('\n\n');

          return {
            output: t('product.learningsHeader', lang)
              .replace('{count}', String(learnings.length)) + '\n\n' + formatted,
          };
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to search learnings.';
          return { isError: true, output: `Failed to search learnings: ${message}` };
        }
      },
    };
  }
}
