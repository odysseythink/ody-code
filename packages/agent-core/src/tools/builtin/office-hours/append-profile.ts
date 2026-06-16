import type { Agent } from '#/agent';
import type { BuilderProfileEntry } from '#/office-hours/state';
import { z } from 'zod';

import { t } from '../../../i18n';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './append-profile.md';

export const AppendBuilderProfileInputSchema = z.object({
  mode: z.enum(['startup', 'builder']).describe('Whether this is a startup or builder session.'),
  projectSlug: z.string().describe('Project slug derived from the project name or working directory.'),
  signalCount: z.number().int().nonnegative().describe('Number of founder signals observed during Phase 4.5 synthesis.'),
  signals: z.array(z.string()).describe('List of founder signal names observed. Split demand by verification strength: demand_transacted (paid/signed), demand_observed (watched usage), demand_stated (verbal/waitlist). Other signals: named_users, pushback, others_need, domain_expertise, taste, agency, reasoned_defense.'),
  designDoc: z.string().optional().describe('Path to the design document produced during Phase 5. Defaults to the current office-hours design file path if omitted.'),
  assignment: z.string().optional().describe('The assignment text from the design document. Defaults to empty if omitted.'),
  resourcesShown: z.array(z.string()).describe('URLs of resources shown to the user during this session.'),
  topics: z.array(z.string()).describe('Topics or categories covered in the session.'),
}).strict();
export type AppendBuilderProfileInput = z.infer<typeof AppendBuilderProfileInputSchema>;

export class AppendBuilderProfileTool implements BuiltinTool<AppendBuilderProfileInput> {
  readonly name = 'AppendBuilderProfile' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(AppendBuilderProfileInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: AppendBuilderProfileInput): ToolExecution {
    return {
      description: 'Appending builder profile entry',
      approvalRule: this.name,
      execute: async () => {
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'office-hours') {
          return {
            isError: true,
            output: t('officeHours.modeNotActive', this.agent.userLanguage),
          };
        }

        try {
          const designDoc = args.designDoc ?? this.agent.sessionMode.sessionModeFilePath ?? '';
          const entry: BuilderProfileEntry = {
            date: new Date().toISOString(),
            mode: args.mode,
            projectSlug: args.projectSlug,
            signalCount: args.signalCount,
            signals: args.signals,
            designDoc,
            assignment: args.assignment ?? '',
            resourcesShown: args.resourcesShown,
            topics: args.topics,
          };
          await this.agent.officeHoursStateStore.appendProfile(entry);
          return {
            output: t('officeHours.profileAppended', this.agent.userLanguage),
          };
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to append builder profile entry.';
          return { isError: true, output: `Failed to append builder profile entry: ${message}` };
        }
      },
    };
  }
}
