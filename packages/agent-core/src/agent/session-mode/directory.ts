import { join } from 'pathe';
import type { Agent } from '../..';
import type { SessionModeKind } from './types';

export function getModeOutputSubdirectory(kind: SessionModeKind): string {
  if (kind === 'office-hours') return 'products';
  if (kind === 'game-design') return 'game-design';
  if (kind === 'design') return 'designs';
  return 'plans';
}

export async function resolveSessionModeDirectory(
  agent: Agent,
  kind: SessionModeKind,
): Promise<{ dir: string; isProjectScoped: boolean }> {
  const subdir = getModeOutputSubdirectory(kind);
  const projectDir = join(agent.config.cwd, '.ody-code', subdir);
  try {
    await agent.kaos.mkdir(projectDir, { parents: true, existOk: true });
    return { dir: projectDir, isProjectScoped: true };
  } catch (error) {
    if (isPermissionError(error) && agent.homedir !== undefined) {
      const sessionDir = join(agent.homedir, subdir);
      await agent.kaos.mkdir(sessionDir, { parents: true, existOk: true });
      return { dir: sessionDir, isProjectScoped: false };
    }
    throw error;
  }
}

function isPermissionError(error: unknown): boolean {
  if (error === null || typeof error !== 'object') return false;
  const code = (error as { readonly code?: unknown }).code;
  return code === 'EACCES' || code === 'EPERM';
}
