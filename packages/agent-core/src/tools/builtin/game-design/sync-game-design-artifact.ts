import { execFileSync } from 'node:child_process';
import { join } from 'pathe';
import type { Agent } from '#/agent';
import { t } from '../../../i18n';
import { z } from 'zod';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './sync-game-design-artifact.md';

export const SyncGameDesignArtifactInputSchema = z.object({
  designFilePath: z.string().describe('Absolute path to the design document artifact to sync.'),
}).strict();
export type SyncGameDesignArtifactInput = z.infer<typeof SyncGameDesignArtifactInputSchema>;

export class SyncGameDesignArtifactTool implements BuiltinTool<SyncGameDesignArtifactInput> {
  readonly name = 'SyncGameDesignArtifact' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(SyncGameDesignArtifactInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: SyncGameDesignArtifactInput): ToolExecution {
    return {
      description: 'Syncing game-design artifact',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'game-design') {
          return { isError: true, output: t('gameDesign.modeNotActive', lang) };
        }
        const projectRoot = this.agent.config.cwd;
        const gbrainPinPath = join(projectRoot, '.gbrain-source');
        try {
          let gbrainSource: string | undefined;
          try { gbrainSource = (await this.agent.kaos.readText(gbrainPinPath)).trim(); } catch {}
          try { await this.agent.kaos.stat(args.designFilePath); } catch {
            return { isError: true, output: t('gameDesign.designFileNotFound', lang).replace('{path}', args.designFilePath) };
          }
          const mcp = this.agent.mcp;
          let mcpGbrainAvailable = false;
          if (mcp) {
            const servers = mcp.list();
            mcpGbrainAvailable = servers.some((s: any) => s.name.includes('gbrain') && s.status === 'connected');
          }
          if (mcpGbrainAvailable) {
            return {
              output: [
                t('gameDesign.gbrainConnected', lang),
                gbrainSource ? t('gameDesign.gbrainTargetSource', lang).replace('{source}', gbrainSource) : '',
                t('gameDesign.gbrainReadyForSync', lang).replace('{path}', args.designFilePath),
              ].filter(Boolean).join('\n'),
            };
          }
          try {
            const cliArgs = ['artifact', 'add'];
            if (gbrainSource !== undefined && gbrainSource.length > 0) cliArgs.push('--source', gbrainSource);
            cliArgs.push(args.designFilePath);
            execFileSync('gbrain', cliArgs, { cwd: projectRoot, timeout: 30_000 });
            return {
              output: [
                t('gameDesign.gbrainSynced', lang),
                gbrainSource ? t('gameDesign.gbrainTargetSource', lang).replace('{source}', gbrainSource) : '',
                t('gameDesign.gbrainFile', lang).replace('{path}', args.designFilePath),
              ].filter(Boolean).join('\n'),
            };
          } catch (cliError: any) {
            return { isError: true, output: t('gameDesign.gbrainCliFailed', lang).replace('{message}', cliError.message ?? String(cliError)) };
          }
        } catch (error: any) {
          return { isError: true, output: t('gameDesign.failedToSyncArtifact', lang).replace('{message}', error.message ?? String(error)) };
        }
      },
    };
  }
}
