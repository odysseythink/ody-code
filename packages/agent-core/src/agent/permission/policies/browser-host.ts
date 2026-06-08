import type { Agent } from '../..';
import type { PermissionPolicy, PermissionPolicyContext, PermissionPolicyResult } from '../types';

export class BrowserHostPermissionPolicy implements PermissionPolicy {
  readonly name = 'browser-host';

  constructor(private readonly agent: Agent) {}

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    if (!context.toolCall.name.startsWith('Browser')) return;

    const url = this.extractUrl(context.args);
    if (!url) return;

    let host: string;
    try {
      host = new URL(url).host;
    } catch {
      return { kind: 'ask', reason: { invalid_url: url } };
    }

    const config = this.agent.kimiConfig?.browser;

    // Static allowlist
    if (config?.allowedHosts?.includes(host)) {
      return { kind: 'approve', reason: { host, allowlist: true } };
    }

    // Sensitive patterns (always ask)
    const sensitivePatterns = config?.sensitivePatterns ?? [];
    for (const pattern of sensitivePatterns) {
      try {
        if (new RegExp(pattern).test(url)) {
          return { kind: 'ask', reason: { host, sensitive: true } };
        }
      } catch {
        // Invalid regex, skip
      }
    }

    // Default: ask for unknown host
    return { kind: 'ask', reason: { host } };
  }

  private extractUrl(args: unknown): string | undefined {
    if (typeof args !== 'object' || args === null) return;
    const obj = args as Record<string, unknown>;
    if (typeof obj['url'] === 'string') return obj['url'];
    return;
  }
}
