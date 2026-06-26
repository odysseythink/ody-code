import type { Agent } from '#/agent';
import type { GameDesignProfileEntry } from '@odysseythink/agent-core-shared';
import { z } from 'zod';
import { t } from '../../../i18n';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './append-game-design-profile.md';

export const AppendGameDesignProfileInputSchema = z.object({
  mode: z.enum(['startup', 'builder']).describe('Whether this is a full design startup or a builder session.'),
  projectSlug: z.string().describe('Project slug.'),
  pillars: z.string().describe('The 3 design pillars as a comma-separated string.'),
  audience: z.string().describe('Target audience description.'),
  platform: z.string().describe('Target platform(s).'),
  genre: z.string().describe('Game genre.'),
  designDoc: z.string().optional().describe('Path to the design document. Defaults to the current game-design file path.'),
  signals: z.array(z.string()).optional().describe('Design signals observed.'),
}).strict();
export type AppendGameDesignProfileInput = z.infer<typeof AppendGameDesignProfileInputSchema>;

export class AppendGameDesignProfileTool implements BuiltinTool<AppendGameDesignProfileInput> {
  readonly name = 'AppendGameDesignProfile' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(AppendGameDesignProfileInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: AppendGameDesignProfileInput): ToolExecution {
    return {
      description: 'Appending game-design profile entry',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'game-design') {
          return { isError: true, output: t('gameDesign.modeNotActive', lang) };
        }
        try {
          const designDoc = args.designDoc ?? this.agent.sessionMode.sessionModeFilePath ?? '';
          const entry: GameDesignProfileEntry = {
            date: new Date().toISOString(),
            mode: args.mode,
            projectSlug: args.projectSlug,
            pillars: args.pillars,
            audience: args.audience,
            platform: args.platform,
            genre: args.genre,
            signals: args.signals ?? [],
            designDoc,
          };
          await this.agent.gameDesignStateStore.appendProfile(entry);
          return { output: t('gameDesign.profileAppended', lang) };
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to append profile entry.';
          return { isError: true, output: `Failed to append game-design profile entry: ${message}` };
        }
      },
    };
  }
}
