import type { Agent } from '#agent';
import { z } from 'zod';

import { productEntryReminder } from '#agent/injection/product-contract';
import { t } from '../../../i18n';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './enter-product.md';

export const EnterProductModeInputSchema = z.object({}).strict();
export type EnterProductModeInput = z.infer<typeof EnterProductModeInputSchema>;

export class EnterProductModeTool implements BuiltinTool<EnterProductModeInput> {
  readonly name = 'EnterProductMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(EnterProductModeInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: EnterProductModeInput): ToolExecution {
    return {
      description: 'Requesting to enter office hours mode',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (this.agent.sessionMode.isActive) {
          if (this.agent.sessionMode.kind === 'product') {
            return {
              isError: true,
              output: t('product.alreadyActive', lang),
            };
          }
          return {
            isError: true,
            output: t('product.anotherModeActive', lang),
          };
        }

        try {
          await this.agent.sessionMode.enter(undefined, undefined, undefined, 'product');
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to enter office hours mode.';
          return {
            isError: true,
            output: t('product.failedToEnter', lang).replace('{message}', message),
          };
        }

        return {
          output: productEntryReminder(this.agent.sessionMode.sessionModeFilePath),
        };
      },
    };
  }
}
