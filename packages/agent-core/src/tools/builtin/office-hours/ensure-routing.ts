import { join } from 'node:path';

import type { Agent } from '#/agent';
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
      description: 'Ensuring CLAUDE.md has skill routing section for office hours',
      approvalRule: this.name,
      execute: async () => {
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'office-hours') {
          return {
            isError: true,
            output: 'Office hours mode is not active. EnsureClaudeMdRouting is only available during office hours sessions.',
          };
        }

        const claudeMdPath = join(this.agent.config.cwd, 'CLAUDE.md');

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
            return { output: `CLAUDE.md created at ${claudeMdPath} with ## Skill routing section.` };
          }

          if (content!.includes('## Skill routing')) {
            return { output: 'CLAUDE.md already has a ## Skill routing section — no changes needed.' };
          }

          const updated = content!.trimEnd() + '\n' + ROUTING_SECTION;
          await this.agent.kaos.writeText(claudeMdPath, updated);
          return { output: `Appended ## Skill routing section to CLAUDE.md at ${claudeMdPath}.` };
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to ensure CLAUDE.md routing.';
          return { isError: true, output: `Failed to ensure CLAUDE.md routing: ${message}` };
        }
      },
    };
  }
}
