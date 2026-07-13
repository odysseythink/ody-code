import type { Agent } from '#agent';
import type { LearningEntry } from '@odysseythink/agent-core-shared';
import { z } from 'zod';

import { t } from '../../../i18n';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './append-learning.md';

export const AppendLearningInputSchema = z.object({
  type: z.enum(['operational', 'eureka']).describe('Type of learning: operational (process/technique) or eureka (insight/discovery).'),
  key: z.string().min(1).describe('Short unique key to identify this learning.'),
  insight: z.string().min(1).describe('The learning insight text.'),
  confidence: z.number().min(0).max(1).describe('Confidence score between 0 and 1.'),
  branch: z.string().optional().describe('Optional git branch identifier for context.'),
}).strict();
export type AppendLearningInput = z.infer<typeof AppendLearningInputSchema>;

export class AppendLearningTool implements BuiltinTool<AppendLearningInput> {
  readonly name = 'AppendLearning' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(AppendLearningInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: AppendLearningInput): ToolExecution {
    return {
      description: 'Appending learning insight',
      approvalRule: this.name,
      execute: async () => {
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'product') {
          return {
            isError: true,
            output: t('product.modeNotActive', this.agent.userLanguage),
          };
        }

        try {
          const entry: LearningEntry = {
            ts: new Date().toISOString(),
            skill: 'product',
            type: args.type,
            key: args.key,
            insight: args.insight,
            confidence: args.confidence,
            source: 'observed',
            branch: args.branch,
          };
          await this.agent.productStateStore.appendLearning(entry);
          return {
            output: t('product.learningRecorded', this.agent.userLanguage)
              .replace('{key}', args.key),
          };
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to append learning.';
          return { isError: true, output: `Failed to append learning: ${message}` };
        }
      },
    };
  }
}
