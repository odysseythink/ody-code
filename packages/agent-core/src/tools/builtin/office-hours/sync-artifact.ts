import { execFileSync } from 'node:child_process';
import { join } from 'pathe';

import type { Agent } from '#/agent';
import { z } from 'zod';

import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './sync-artifact.md';

export const SyncOfficeHoursArtifactInputSchema = z.object({
  designFilePath: z.string().describe('Absolute path to the design document artifact to sync.'),
}).strict();
export type SyncOfficeHoursArtifactInput = z.infer<typeof SyncOfficeHoursArtifactInputSchema>;

export class SyncOfficeHoursArtifactTool implements BuiltinTool<SyncOfficeHoursArtifactInput> {
  readonly name = 'SyncOfficeHoursArtifact' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(SyncOfficeHoursArtifactInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: SyncOfficeHoursArtifactInput): ToolExecution {
    return {
      description: 'Syncing design artifact to gbrain',
      approvalRule: this.name,
      execute: async () => {
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'office-hours') {
          return {
            isError: true,
            output: 'Office hours mode is not active. SyncOfficeHoursArtifact is only available during office hours sessions.',
          };
        }

        const projectRoot = this.agent.config.cwd;
        const gbrainPinPath = join(projectRoot, '.gbrain-source');

        try {
          // 1. Check for .gbrain-source pin
          let gbrainSource: string | undefined;
          try {
            gbrainSource = (await this.agent.kaos.readText(gbrainPinPath)).trim();
          } catch {
            // Pin file doesn't exist or can't be read — that's fine
          }

          // 2. Check if design file exists
          try {
            await this.agent.kaos.stat(args.designFilePath);
          } catch {
            return { isError: true, output: `Design file not found at ${args.designFilePath}.` };
          }

          // 3. Try MCP-based gbrain sync first
          const mcp = this.agent.mcp;
          let mcpGbrainAvailable = false;
          if (mcp) {
            const servers = mcp.list();
            mcpGbrainAvailable = servers.some(
              (s) => s.name.includes('gbrain') && s.status === 'connected',
            );
          }

          if (mcpGbrainAvailable) {
            // The MCP handles the sync via tool calls — the model can call the
            // gbrain tool directly. We just confirm the server is available.
            return {
              output: [
                'gbrain MCP server is connected.',
                gbrainSource ? `Target source: ${gbrainSource}` : 'No .gbrain-source pin found.',
                `Design artifact at ${args.designFilePath} is ready for sync via MCP.`,
              ].filter(Boolean).join('\n'),
            };
          }

          // 4. Fall back to shell-based gbrain CLI
          try {
            const cliArgs = ['artifact', 'add'];
            if (gbrainSource !== undefined && gbrainSource.length > 0) {
              cliArgs.push('--source', gbrainSource);
            }
            cliArgs.push(args.designFilePath);
            execFileSync('gbrain', cliArgs, { cwd: projectRoot, timeout: 30_000 });
            return {
              output: [
                'Design artifact synced via gbrain CLI.',
                gbrainSource ? `Target source: ${gbrainSource}` : '',
                `File: ${args.designFilePath}`,
              ].filter(Boolean).join('\n'),
            };
          } catch (cliError) {
            const message = cliError instanceof Error ? cliError.message : String(cliError);
            return {
              isError: true,
              output: `gbrain CLI sync failed: ${message}. Ensure the gbrain CLI is installed and configured.`,
            };
          }
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to sync design artifact.';
          return { isError: true, output: `Failed to sync design artifact: ${message}` };
        }
      },
    };
  }
}
