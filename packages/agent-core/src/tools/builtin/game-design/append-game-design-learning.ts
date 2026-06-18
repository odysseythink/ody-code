import type { Agent } from '#/agent';
import type { GameDesignLearningEntry } from '#/office-hours/state';
import { z } from 'zod';
import { t } from '../../../i18n';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './append-game-design-learning.md';

export const AppendGameDesignLearningInputSchema = z.object({
  type: z.enum(['operational', 'eureka']),
  key: z.string().min(1),
  insight: z.string().min(1),
  confidence: z.number().min(0).max(1),
  branch: z.string().optional(),
}).strict();
export type AppendGameDesignLearningInput = z.infer<typeof AppendGameDesignLearningInputSchema>;

export class AppendGameDesignLearningTool implements BuiltinTool<AppendGameDesignLearningInput> {
  readonly name = 'AppendGameDesignLearning' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(AppendGameDesignLearningInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: AppendGameDesignLearningInput): ToolExecution {
    return {
      description: 'Appending game-design learning insight',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'game-design') {
          return { isError: true, output: t('gameDesign.modeNotActive', lang) };
        }
        try {
          const entry: GameDesignLearningEntry = {
            ts: new Date().toISOString(),
            skill: 'game-design',
            type: args.type,
            key: args.key,
            insight: args.insight,
            confidence: args.confidence,
            source: 'observed',
            branch: args.branch,
          };
          await this.agent.gameDesignStateStore.appendLearning(entry);
          return {
            output: t('gameDesign.learningRecorded', lang).replace('{key}', args.key),
          };
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to append learning.';
          return { isError: true, output: `Failed to append learning: ${message}` };
        }
      },
    };
  }
}
