import type { Agent } from '#agent';
import { z } from 'zod';

import { t } from '../../../i18n';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './exit-product.md';

export const ExitProductModeInputSchema = z.object({}).strict();
export type ExitProductModeInput = z.infer<typeof ExitProductModeInputSchema>;

export class ExitProductModeTool implements BuiltinTool<ExitProductModeInput> {
  readonly name = 'ExitProductMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(ExitProductModeInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: ExitProductModeInput): ToolExecution {
    return {
      description: 'Requesting to exit office hours mode',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'product') {
          return {
            isError: true,
            output: t('product.modeNotActive', lang),
          };
        }

        const path = this.agent.sessionMode.sessionModeFilePath;
        this.agent.sessionMode.exit();

        const parts = [
          t('product.sessionComplete', lang),
        ];
        if (path) {
          parts.push(t('product.designDocSaved', lang).replace('{path}', path));
        }
        parts.push(t('product.appWillExit', lang));

        return {
          output: parts.join('\n'),
        };
      },
    };
  }
}
