import type { OdyConfig, McpServerConfig } from '#/config/schema';

export interface BuiltInContext {
  readonly kimiHomeDir: string;
  readonly sessionId?: string;
  readonly chromePort?: number;
}

export interface BuiltInMcpServerDefinition {
  readonly name: string;
  readonly displayName: string;
  readonly enabledByDefault: boolean;
  readonly config: McpServerConfig;
  readonly envResolver?: (ctx: BuiltInContext) => Record<string, string>;
  readonly argsResolver?: (ctx: BuiltInContext) => string[];
}

export class BuiltInMcpRegistry {
  private readonly definitions = new Map<string, BuiltInMcpServerDefinition>();

  register(def: BuiltInMcpServerDefinition): void {
    this.definitions.set(def.name, def);
  }

  getEnabledConfigs(ctx: BuiltInContext, config: OdyConfig): Record<string, McpServerConfig> {
    const result: Record<string, McpServerConfig> = {};
    for (const [name, def] of this.definitions) {
      if (this.isDisabled(name, config)) continue;
      const env = def.envResolver?.(ctx);
      const args = def.argsResolver?.(ctx);
      const base = def.config as Record<string, unknown>;
      const merged: Record<string, unknown> = {
        ...def.config,
        env: env ? { ...(base['env'] as Record<string, string> | undefined), ...env } : base['env'],
      };
      if (args !== undefined) {
        merged['args'] = args;
      }
      result[name] = merged as McpServerConfig;
    }
    return result;
  }

  isDisabled(name: string, config: OdyConfig): boolean {
    const def = this.definitions.get(name);
    if (def === undefined) return true;
    if (name === 'chrome-devtools') {
      return config.browser?.enabled === false;
    }
    return !def.enabledByDefault;
  }
}
