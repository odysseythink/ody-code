import type { Agent } from '../..';
import type { ResolvedRuntimeProvider } from '../../session/provider-manager';

export function modelAliasHasUsableAuth(
  agent: Agent,
  modelAlias: string,
  resolved: ResolvedRuntimeProvider,
): boolean {
  const withAuth = agent.modelProvider?.resolveAuth?.(modelAlias, { log: agent.log });
  if (withAuth !== undefined) return true;
  const apiKey = (resolved.provider as { apiKey?: string }).apiKey;
  return apiKey !== undefined && apiKey.length > 0;
}
