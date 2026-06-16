import { join } from 'pathe';

import type { Agent } from '#/agent';
import { t } from '../../../i18n';
import { z } from 'zod';

import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './ensure-routing.md';

const ROUTING_SECTION = `
## Skill routing

- **office-hours**: YC office hours diagnostic workflow. Activates when the user explicitly requests office hours or asks for startup/builder diagnostic help.

To invoke, ask the agent to start office hours.
`;

export const EnsureClaudeMdRoutingInputSchema = z.object({}).strict();
export type EnsureClaudeMdRoutingInput = z.infer<typeof EnsureClaudeMdRoutingInputSchema>;

export class EnsureClaudeMdRoutingTool implements BuiltinTool<EnsureClaudeMdRoutingInput> {
  readonly name = 'EnsureClaudeMdRouting' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(EnsureClaudeMdRoutingInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: EnsureClaudeMdRoutingInput): ToolExecution {
    return {
      description: 'Ensuring AGENTS.md has skill routing section for office hours',
      approvalRule: this.name,
      execute: async () => {
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'office-hours') {
          return {
            isError: true,
            output: t('officeHours.modeNotActive', this.agent.userLanguage),
          };
        }

        const claudeMdPath = join(this.agent.config.cwd, 'AGENTS.md');

        try {
          let content: string;
          let fileExists = false;
          try {
            content = await this.agent.kaos.readText(claudeMdPath);
            fileExists = true;
          } catch {
            content = '';
          }

          if (!fileExists) {
            await this.agent.kaos.writeText(claudeMdPath, ROUTING_SECTION.trimStart());
            return { output: t('officeHours.agentsMdCreated', this.agent.userLanguage).replace('{path}', claudeMdPath) };
          }

          if (content!.includes('## Skill routing')) {
            return { output: t('officeHours.agentsMdAlreadyHasRouting', this.agent.userLanguage) };
          }

          const updated = content!.trimEnd() + '\n' + ROUTING_SECTION;
          await this.agent.kaos.writeText(claudeMdPath, updated);
          return { output: t('officeHours.agentsMdUpdated', this.agent.userLanguage).replace('{path}', claudeMdPath) };
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to ensure AGENTS.md routing.';
          return { isError: true, output: t('officeHours.failedToEnsureRouting', this.agent.userLanguage).replace('{message}', message) };
        }
      },
    };
  }
}
