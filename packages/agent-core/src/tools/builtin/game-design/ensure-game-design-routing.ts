import { join } from 'pathe';
import type { Agent } from '#agent';
import { t } from '../../../i18n';
import { z } from 'zod';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './ensure-game-design-routing.md';

const ROUTING_SECTION = `
## Skill routing

- **game-design**: Game design workflow based on the 100 Principles of Game Design. Activates via --game-design or when the user requests game design help.

To invoke, ask the agent to start game-design mode.
`;

export const EnsureGameDesignRoutingInputSchema = z.object({}).strict();
export type EnsureGameDesignRoutingInput = z.infer<typeof EnsureGameDesignRoutingInputSchema>;

export class EnsureGameDesignRoutingTool implements BuiltinTool<EnsureGameDesignRoutingInput> {
  readonly name = 'EnsureGameDesignRouting' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(EnsureGameDesignRoutingInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: EnsureGameDesignRoutingInput): ToolExecution {
    return {
      description: 'Ensuring AGENTS.md has skill routing section for game-design',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'game-design') {
          return { isError: true, output: t('gameDesign.modeNotActive', lang) };
        }
        const claudeMdPath = join(this.agent.config.cwd, 'AGENTS.md');
        try {
          let content: string;
          let fileExists = false;
          try {
            content = await this.agent.kaos.readText(claudeMdPath);
            fileExists = true;
          } catch { content = ''; }
          if (!fileExists) {
            await this.agent.kaos.writeText(claudeMdPath, ROUTING_SECTION.trimStart());
            return { output: t('gameDesign.agentsMdCreated', lang).replace('{path}', claudeMdPath) };
          }
          if (content!.includes('## Skill routing')) {
            return { output: t('gameDesign.agentsMdAlreadyHasRouting', lang) };
          }
          const updated = content!.trimEnd() + '\n' + ROUTING_SECTION;
          await this.agent.kaos.writeText(claudeMdPath, updated);
          return { output: t('gameDesign.agentsMdUpdated', lang).replace('{path}', claudeMdPath) };
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to ensure AGENTS.md routing.';
          return { isError: true, output: t('gameDesign.failedToEnsureRouting', lang).replace('{message}', message) };
        }
      },
    };
  }
}
