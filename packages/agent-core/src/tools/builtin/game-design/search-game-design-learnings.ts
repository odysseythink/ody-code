import type { Agent } from '#/agent';
import { t } from '../../../i18n';
import { z } from 'zod';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './search-game-design-learnings.md';

export const SearchGameDesignLearningsInputSchema = z.object({
  limit: z.number().int().positive().default(10),
  branch: z.string().optional(),
}).strict();
export type SearchGameDesignLearningsInput = z.infer<typeof SearchGameDesignLearningsInputSchema>;

export class SearchGameDesignLearningsTool implements BuiltinTool<SearchGameDesignLearningsInput> {
  readonly name = 'SearchGameDesignLearnings' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(SearchGameDesignLearningsInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: SearchGameDesignLearningsInput): ToolExecution {
    return {
      description: 'Searching past game-design learnings',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'game-design') {
          return { isError: true, output: t('gameDesign.modeNotActive', lang) };
        }
        try {
          const learnings = await this.agent.gameDesignStateStore.searchLearnings({
            limit: args.limit,
            branch: args.branch,
          });
          if (learnings.length === 0) {
            return { output: t('gameDesign.noLearnings', lang) };
          }
          const formatted = learnings.map((l, i) =>
            `[${i + 1}] ${t('gameDesign.learningTypeLabel', lang)}: ${l.type.toUpperCase()}: ${l.key}\n    ${t('gameDesign.learningInsightLabel', lang)}: ${l.insight}\n    ${t('gameDesign.learningConfidenceLabel', lang)}: ${l.confidence}${l.branch ? `\n    ${t('gameDesign.learningBranchLabel', lang)}: ${l.branch}` : ''}`
          ).join('\n\n');
          return {
            output: t('gameDesign.learningsHeader', lang).replace('{count}', String(learnings.length)) + '\n\n' + formatted,
          };
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to search learnings.';
          return { isError: true, output: `Failed to search learnings: ${message}` };
        }
      },
    };
  }
}
