import { randomUUID } from 'node:crypto';
import { homedir } from 'node:os';

import { ErrorCodes, KimiError } from '#/errors';
import { getRootLogger, log } from '#/logging/logger';
import { PluginManager } from '#/plugin';
import { LocalFetchURLProvider } from '#/tools/providers/local-fetch-url';
import { MoonshotFetchURLProvider } from '#/tools/providers/moonshot-fetch-url';
import { MoonshotWebSearchProvider } from '#/tools/providers/moonshot-web-search';
import type { PromisableMethods } from '#/utils/types';
import { getCoreVersion } from '#/version';
import { resolveThinkingLevel } from '../agent/config/thinking';
import {
  ensureOdyHome,
  loadRuntimeConfig,
  mergeConfigPatch,
  readConfigFile,
  resolveConfigPath,
  resolveOdyHome,
  writeConfigFile,
  type KimiConfig,
  type MoonshotServiceConfig,
} from '../config';
import {
  FLAG_DEFINITIONS,
  flags,
  type ExperimentalFlagMap,
  type FlagDefinitionInput,
  type FlagId,
} from '../flags';
import type { Logger } from '../logging/types';
import { resolveSessionMcpConfig, type SessionMcpConfig } from '../mcp';
import { Session, type SessionMeta, type SessionSkillConfig } from '../session';
import { exportSessionDirectory } from '../session/export';
import {
  ProviderManager, type BearerTokenProvider,
  type OAuthTokenProviderResolver
} from '../session/provider-manager';
import { SessionAPIImpl } from '../session/rpc';
import { normalizeWorkDir, SessionStore } from '../session/store';
import { noopTelemetryClient, withTelemetryContext, type TelemetryClient } from '../telemetry';
import type { CoreRPCClient } from './client';
import type {
  ActivateSkillPayload,
  BeginCompactionPayload,
  CancelPayload,
  CancelPlanPayload,
  CloseSessionPayload,
  CoreAPI,
  CoreInfo,
  CreateGoalPayload,
  CreateSessionPayload,
  EmptyPayload,
  ReviewDesignPayload,
  EnterPlanPayload,
  GoalControlPayload,
  GoalSnapshot,
  GoalToolResult,
  ExportSessionPayload,
  ExportSessionResult,
  ForkSessionPayload,
  GetBackgroundOutputPayload,
  GetBackgroundPayload,
  GetKimiConfigPayload,
  GetPluginInfoPayload,
  InstallPluginPayload,
  ListSessionsPayload,
  McpServerInfo,
  McpStartupMetrics,
  PluginInfo,
  PluginSummary,
  PromptPayload,
  ReconnectMcpServerPayload,
  RegisterToolPayload,
  ReloadPluginsResult,
  RemoveKimiProviderPayload,
  RemovePluginPayload,
  RenameSessionPayload,
  ResumeSessionPayload,
  SessionSummary,
  SetActiveToolsPayload,
  SetKimiConfigPayload,
  SetModelPayload,
  SetModelResult,
  SetPermissionPayload,
  SetPluginEnabledPayload,
  SetPluginMcpServerEnabledPayload,
  SetThinkingPayload,
  SkillSummary,
  SteerPayload,
  StopBackgroundPayload,
  UndoHistoryPayload,
  UnregisterToolPayload,
  UpdateSessionMetadataPayload,
} from './core-api';
import type { ResumedAgentState, ResumeSessionResult } from './resumed';
import type { SDKRPC } from './sdk-api';
import { proxyWithExtraPayload } from './types';
import { KaosShellNotFoundError, LocalKaos, type Kaos } from '@odysseythink/kaos';
import type { ToolServices } from '../tools/support/services';
import { BuiltInMcpRegistry } from '../mcp/built-in';
import { createChromeDevToolsServerDefinition } from '../mcp/built-in/chrome-devtools';
import { BuiltInRootNotFoundError } from '../mcp/built-in/resolve-root';

const ODY_CODE_PROVIDER_NAME = 'managed:ody-code';

type AgentScopedPayload<T> = T & { readonly agentId: string };
type SessionScopedPayload<T> = T & { readonly sessionId: string };
type SessionAgentPayload<T> = SessionScopedPayload<AgentScopedPayload<T>>;
type RenameSessionRequest = SessionScopedPayload<RenameSessionPayload>;
type UpdateSessionMetadataRequest = SessionScopedPayload<UpdateSessionMetadataPayload>;

export interface KimiCoreOptions {
  readonly homeDir?: string | undefined;
  readonly configPath?: string | undefined;
  readonly runtime?: ToolServices | undefined;
  readonly kimiRequestHeaders?: Record<string, string> | undefined;
  readonly resolveOAuthTokenProvider?: OAuthTokenProviderResolver | undefined;
  readonly skillDirs?: readonly string[];
  readonly telemetry?: TelemetryClient | undefined;
  readonly appVersion?: string;
}

export class KimiCore implements PromisableMethods<CoreAPI> {
  readonly sdk: Promise<SDKRPC>;
  readonly homeDir: string;
  readonly configPath: string;
  readonly sessions = new Map<string, Session>();
  readonly telemetry: TelemetryClient;

  private kaos: Promise<Kaos>;
  private runtime: ToolServices | undefined;
  private config: KimiConfig;
  private readonly userHomeDir: string;
  private readonly kimiRequestHeaders: Record<string, string> | undefined;
  private readonly resolveOAuthTokenProvider: OAuthTokenProviderResolver | undefined;
  private readonly skillDirs: readonly string[];
  private readonly sessionStore: SessionStore;
  readonly plugins: PluginManager;
  private pluginsReady: Promise<void>;
  private pluginsLoadError: Error | undefined;
  private readonly appVersion: string | undefined;
  private builtInMcpRegistry: BuiltInMcpRegistry;

  constructor(
    protected readonly rpcClient: CoreRPCClient,
    options: KimiCoreOptions = {},
  ) {
    this.homeDir = resolveOdyHome(options.homeDir);
    this.userHomeDir = homedir();
    this.configPath = resolveConfigPath({
      homeDir: this.homeDir,
      configPath: options.configPath,
    });
    this.kaos = LocalKaos.create().catch((error: unknown) => {
      if (error instanceof KaosShellNotFoundError) {
        throw new KimiError(ErrorCodes.SHELL_GIT_BASH_NOT_FOUND, error.message);
      }
      throw error;
    });
    this.runtime = options.runtime;
    this.kimiRequestHeaders = options.kimiRequestHeaders;
    this.resolveOAuthTokenProvider = options.resolveOAuthTokenProvider;
    this.skillDirs = options.skillDirs ?? [];
    this.telemetry = options.telemetry ?? noopTelemetryClient;
    this.appVersion = options.appVersion;
    ensureOdyHome(this.homeDir);
    this.config = loadRuntimeConfig(this.configPath);
    this.sessionStore = new SessionStore(this.homeDir);
    this.plugins = new PluginManager({ kimiHomeDir: this.homeDir });
    this.builtInMcpRegistry = new BuiltInMcpRegistry();
    try {
      this.builtInMcpRegistry.register(createChromeDevToolsServerDefinition());
    } catch (error) {
      if (error instanceof BuiltInRootNotFoundError) {
        log.warn('Built-in MCP server not found', { server: error.serverName });
        // Register with undefined cwd so the server still appears in MCP lists;
        // it will fail on connect because the entry point is missing, which is expected.
        this.builtInMcpRegistry.register(createChromeDevToolsServerDefinition(null));
      } else {
        throw error;
      }
    }
    // Capture the error rather than swallow it: mutators and explicit /plugins
    // reads rethrow so the user sees what's wrong; createSession/resumeSession
    // degrade silently (no plugin skills, no sessionStart injections) so the harness still
    // starts. Reload clears the error on success.
    this.pluginsReady = this.plugins.load().catch((error: unknown) => {
      this.pluginsLoadError = error instanceof Error ? error : new Error(String(error));
    });

    this.sdk = rpcClient(this);
  }

  async createSession(input: CreateSessionPayload): Promise<SessionSummary> {
    const options = input;
    const workDir = requiredWorkDir('createSession', options.workDir);
    const config = this.reloadProviderManager();
    const id = options.id ?? createSessionId();
    const thinkingLevel = resolveThinkingLevel(options.thinking, config);
    const permissionMode = options.permission ?? config.defaultPermissionMode;
    const baseMcpConfig = await resolveSessionMcpConfig({
      cwd: workDir,
      homeDir: this.homeDir,
    });
    const summary = await this.sessionStore.create({
      id,
      workDir,
    });
    const result: SessionSummary = {
      ...summary,
      metadata: options.metadata,
    };

    await this.pluginsReady;
    const pluginSessionStarts = this.plugins.enabledSessionStarts();
    let mcpConfig = this.mergePluginMcpConfig(baseMcpConfig);
    mcpConfig = this.mergeBuiltInMcpConfig(mcpConfig, { sessionId: id, config });

    // Session ctor attaches its own log sink. If anything in the setup-after-
    // ctor block throws, `session.close()` releases the sink (and mcp).
    const runtime = await this.resolveRuntime(config);
    const session = new Session({
      kaos: (await this.kaos).withCwd(workDir),
      toolServices: runtime,
      config,
      id,
      homedir: summary.sessionDir,
      kimiHomeDir: this.homeDir,
      rpc: proxyWithExtraPayload(await this.sdk, { sessionId: summary.id }),
      providerManager: this.resolveProviderManager(summary.id),
      background: config.background,
      hooks: config.hooks,
      permissionRules: config.permission?.rules,
      skills: this.resolveSessionSkillConfig(config),
      mcpConfig,
      telemetry: withTelemetryContext(this.telemetry, { sessionId: summary.id }),
      pluginSessionStarts,
      appVersion: this.appVersion,
    });
    try {
      session.metadata = {
        ...session.metadata,
        createdAt: new Date(summary.createdAt).toISOString(),
        updatedAt: new Date(summary.updatedAt).toISOString(),
        ...(summary.title !== undefined
          ? {
              title: summary.title,
              isCustomTitle: true,
            }
          : {}),
        custom: options.metadata === undefined ? {} : { ...options.metadata },
      };
      const mainAgent = await session.createMain();
      mainAgent.config.update({
        modelAlias: options.model ?? config.defaultModel,
        thinkingLevel,
      });
      if (permissionMode !== undefined) {
        mainAgent.permission.setMode(permissionMode);
      }
      // Honor config.defaultSessionMode for fresh sessions. Resumed sessions
      // restore their own plan state from records and never re-apply this.
      if (config.defaultSessionMode !== undefined) {
        await mainAgent.sessionMode.enter(undefined, undefined, undefined, config.defaultSessionMode);
      }
      await session.writeMetadata();
      await session.flushMetadata();
    } catch (error) {
      await session.close().catch(() => {});
      throw error;
    }
    this.sessions.set(id, session);
    return result;
  }

  getCoreInfo(): CoreInfo {
    return { version: getCoreVersion() };
  }

  getExperimentalFlags(): ExperimentalFlagMap {
    const defs: readonly FlagDefinitionInput[] = FLAG_DEFINITIONS;
    return Object.fromEntries(defs.map((def) => [def.id, flags.enabled(def.id as FlagId)]));
  }

  async closeSession({ sessionId }: CloseSessionPayload): Promise<void> {
    const session = this.sessions.get(sessionId);
    if (session) {
      await session.close();
      this.sessions.delete(sessionId);
    }
  }

  async resumeSession(input: ResumeSessionPayload): Promise<ResumeSessionResult> {
    const summary = await this.sessionStore.get(input.sessionId);
    const active = this.sessions.get(summary.id);
    if (active !== undefined) {
      return resumeSessionResult(summary, active);
    }

    const config = this.reloadProviderManager();
    const baseMcpConfig = await resolveSessionMcpConfig({
      cwd: summary.workDir,
      homeDir: this.homeDir,
    });
    await this.pluginsReady;
    const pluginSessionStarts = this.plugins.enabledSessionStarts();
    let mcpConfig = this.mergePluginMcpConfig(baseMcpConfig);
    mcpConfig = this.mergeBuiltInMcpConfig(mcpConfig, { sessionId: summary.id, config });
    const runtime = await this.resolveRuntime(config);
    const session = new Session({
      kaos: (await this.kaos).withCwd(summary.workDir),
      toolServices: runtime,
      config,
      id: summary.id,
      homedir: summary.sessionDir,
      kimiHomeDir: this.homeDir,
      rpc: proxyWithExtraPayload(await this.sdk, { sessionId: summary.id }),
      providerManager: this.resolveProviderManager(summary.id),
      background: config.background,
      hooks: config.hooks,
      permissionRules: config.permission?.rules,
      skills: this.resolveSessionSkillConfig(config),
      mcpConfig,
      telemetry: withTelemetryContext(this.telemetry, { sessionId: summary.id }),
      initializeMainAgent: false,
      pluginSessionStarts,
      appVersion: this.appVersion,
    });
    let warning: string | undefined;
    try {
      const resumeResult = await session.resume();
      warning = resumeResult.warning;
      await this.refreshSessionRuntimeConfig(session, config);
    } catch (error) {
      await session.close().catch(() => {});
      withTelemetryContext(this.telemetry, { sessionId: summary.id }).track('session_load_failed', {
        reason: telemetryErrorReason(error),
      });
      throw error;
    }
    this.sessions.set(summary.id, session);
    return resumeSessionResult(summary, session, warning);
  }

  async forkSession(input: ForkSessionPayload): Promise<ResumeSessionResult> {
    const source = await this.sessionStore.get(input.sessionId);
    const active = this.sessions.get(source.id);
    if (active !== undefined) {
      await active.flushMetadata();
    }

    const id = input.id ?? createSessionId();
    await this.sessionStore.fork({
      sourceId: source.id,
      targetId: id,
      title: input.title,
      metadata: input.metadata,
    });
    return this.resumeSession({ sessionId: id });
  }

  async listSessions(input: ListSessionsPayload = {}): Promise<readonly SessionSummary[]> {
    return this.sessionStore.list(input);
  }

  async renameSession({ sessionId, ...payload }: RenameSessionRequest): Promise<void> {
    const session = this.sessions.get(sessionId);
    if (session !== undefined) {
      await new SessionAPIImpl(session).renameSession(payload);
      return;
    }
    await this.sessionStore.rename(sessionId, payload.title);
  }

  async exportSession(input: ExportSessionPayload): Promise<ExportSessionResult> {
    const summary = await this.sessionStore.get(input.sessionId);
    const active = this.sessions.get(input.sessionId);
    // Closed sessions have no `Session.log`; create an ad-hoc child bound to
    // their id so the entries still route to the session log file.
    const exportLog =
      active?.log ?? log.createChild({ sessionId: input.sessionId });
    if (active !== undefined) {
      try {
        await active.flushMetadata();
      } catch (error) {
        exportLog.warn('flushMetadata failed before export', { error });
      }
    }
    await warnIfLogFlushFails(exportLog, 'export session log flush failed', () =>
      getRootLogger().flushSession(input.sessionId),
    );
    if (input.includeGlobalLog === true) {
      await warnIfLogFlushFails(exportLog, 'export global log flush failed', () =>
        getRootLogger().flushGlobal(),
      );
    }
    const result = await exportSessionDirectory({
      request: input,
      summary,
      homeDir: this.homeDir,
      globalLogPath: getRootLogger().getConfig()?.globalLogPath,
    });
    return result;
  }

  async getKimiConfig(input?: GetKimiConfigPayload): Promise<KimiConfig> {
    if (input?.reload) {
      this.config = loadRuntimeConfig(this.configPath);
    }
    return this.config;
  }

  async setKimiConfig(input: SetKimiConfigPayload): Promise<KimiConfig> {
    const config = mergeConfigPatch(readConfigFile(this.configPath), input);
    await writeConfigFile(this.configPath, config);
    this.config = loadRuntimeConfig(this.configPath);
    // Propagate the fresh config to every active agent so runtime changes
    // (including modeModels) are visible without restarting the session.
    for (const session of this.sessions.values()) {
      for (const agent of session.agents.values()) {
        agent.kimiConfig = this.config;
      }
    }
    return this.config;
  }

  async removeKimiProvider(input: RemoveKimiProviderPayload): Promise<KimiConfig> {
    const config = readConfigFile(this.configPath);
    delete config.providers[input.providerId];

    let removedDefault = false;
    const existingModels = config.models ?? {};
    for (const [key, model] of Object.entries(existingModels)) {
      if (
        typeof model === 'object' &&
        model !== null &&
        !Array.isArray(model) &&
        model['provider'] === input.providerId
      ) {
        delete existingModels[key];
        if (config.defaultModel === key) removedDefault = true;
      }
    }
    config.models = existingModels;

    if (removedDefault) {
      config.defaultModel = undefined;
    }

    if (config.defaultProvider === input.providerId) {
      config.defaultProvider = undefined;
    }

    await writeConfigFile(this.configPath, config);
    return this.config = loadRuntimeConfig(this.configPath);
  }

  prompt({ sessionId, ...payload }: SessionAgentPayload<PromptPayload>) {
    return this.sessionApi(sessionId).prompt(payload);
  }

  steer({ sessionId, ...payload }: SessionAgentPayload<SteerPayload>) {
    return this.sessionApi(sessionId).steer(payload);
  }

  cancel({ sessionId, ...payload }: SessionAgentPayload<CancelPayload>) {
    return this.sessionApi(sessionId).cancel(payload);
  }

  undoHistory({ sessionId, ...payload }: SessionAgentPayload<UndoHistoryPayload>) {
    return this.sessionApi(sessionId).undoHistory(payload);
  }

  async setModel({
    sessionId,
    ...payload
  }: SessionAgentPayload<SetModelPayload>): Promise<SetModelResult> {
    this.reloadProviderManager();
    return this.sessionApi(sessionId).setModel(payload);
  }

  setThinking({ sessionId, ...payload }: SessionAgentPayload<SetThinkingPayload>) {
    return this.sessionApi(sessionId).setThinking(payload);
  }

  setPermission({ sessionId, ...payload }: SessionAgentPayload<SetPermissionPayload>) {
    return this.sessionApi(sessionId).setPermission(payload);
  }

  getModel({ sessionId, ...payload }: SessionAgentPayload<EmptyPayload>) {
    return this.sessionApi(sessionId).getModel(payload);
  }

  enterPlan({ sessionId, ...payload }: SessionAgentPayload<EnterPlanPayload>) {
    return this.sessionApi(sessionId).enterPlan(payload);
  }

  cancelPlan({ sessionId, ...payload }: SessionAgentPayload<CancelPlanPayload>) {
    return this.sessionApi(sessionId).cancelPlan(payload);
  }

  clearPlan({ sessionId, ...payload }: SessionAgentPayload<EmptyPayload>) {
    return this.sessionApi(sessionId).clearPlan(payload);
  }

  beginCompaction({ sessionId, ...payload }: SessionAgentPayload<BeginCompactionPayload>) {
    return this.sessionApi(sessionId).beginCompaction(payload);
  }

  cancelCompaction({ sessionId, ...payload }: SessionAgentPayload<EmptyPayload>) {
    return this.sessionApi(sessionId).cancelCompaction(payload);
  }

  registerTool({ sessionId, ...payload }: SessionAgentPayload<RegisterToolPayload>) {
    return this.sessionApi(sessionId).registerTool(payload);
  }

  unregisterTool({ sessionId, ...payload }: SessionAgentPayload<UnregisterToolPayload>) {
    return this.sessionApi(sessionId).unregisterTool(payload);
  }

  setActiveTools({ sessionId, ...payload }: SessionAgentPayload<SetActiveToolsPayload>) {
    return this.sessionApi(sessionId).setActiveTools(payload);
  }

  stopBackground({ sessionId, ...payload }: SessionAgentPayload<StopBackgroundPayload>) {
    return this.sessionApi(sessionId).stopBackground(payload);
  }

  clearContext({ sessionId, ...payload }: SessionAgentPayload<EmptyPayload>) {
    return this.sessionApi(sessionId).clearContext(payload);
  }

  activateSkill({
    sessionId,
    ...payload
  }: SessionAgentPayload<ActivateSkillPayload>): Promise<void> {
    return this.sessionApi(sessionId).activateSkill(payload);
  }

  getBackgroundOutput({ sessionId, ...payload }: SessionAgentPayload<GetBackgroundOutputPayload>) {
    return this.sessionApi(sessionId).getBackgroundOutput(payload);
  }

  getContext({ sessionId, ...payload }: SessionAgentPayload<EmptyPayload>) {
    return this.sessionApi(sessionId).getContext(payload);
  }

  getConfig({ sessionId, ...payload }: SessionAgentPayload<EmptyPayload>) {
    return this.sessionApi(sessionId).getConfig(payload);
  }

  getPermission({ sessionId, ...payload }: SessionAgentPayload<EmptyPayload>) {
    return this.sessionApi(sessionId).getPermission(payload);
  }

  getPlan({ sessionId, ...payload }: SessionAgentPayload<EmptyPayload>) {
    return this.sessionApi(sessionId).getPlan(payload);
  }

  reviewDesign({ sessionId, ...payload }: SessionAgentPayload<ReviewDesignPayload>) {
    return this.sessionApi(sessionId).reviewDesign(payload);
  }

  getUsage({ sessionId, ...payload }: SessionAgentPayload<EmptyPayload>) {
    return this.sessionApi(sessionId).getUsage(payload);
  }

  getTools({ sessionId, ...payload }: SessionAgentPayload<EmptyPayload>) {
    return this.sessionApi(sessionId).getTools(payload);
  }

  getBackground({ sessionId, ...payload }: SessionAgentPayload<GetBackgroundPayload>) {
    return this.sessionApi(sessionId).getBackground(payload);
  }

  updateSessionMetadata({ sessionId, ...payload }: UpdateSessionMetadataRequest): Promise<void> {
    return this.sessionApi(sessionId).updateSessionMetadata(payload);
  }

  getSessionMetadata({ sessionId, ...payload }: SessionScopedPayload<EmptyPayload>): SessionMeta {
    return this.sessionApi(sessionId).getSessionMetadata(payload);
  }

  listSkills({
    sessionId,
    ...payload
  }: SessionScopedPayload<EmptyPayload>): Promise<readonly SkillSummary[]> {
    return this.sessionApi(sessionId).listSkills(payload);
  }

  listMcpServers({
    sessionId,
    ...payload
  }: SessionScopedPayload<EmptyPayload>): readonly McpServerInfo[] {
    return this.sessionApi(sessionId).listMcpServers(payload);
  }

  getMcpStartupMetrics({
    sessionId,
    ...payload
  }: SessionScopedPayload<EmptyPayload>): Promise<McpStartupMetrics> {
    return this.sessionApi(sessionId).getMcpStartupMetrics(payload);
  }

  reconnectMcpServer({
    sessionId,
    ...payload
  }: SessionScopedPayload<ReconnectMcpServerPayload>): Promise<void> {
    return this.sessionApi(sessionId).reconnectMcpServer(payload);
  }

  generateAgentsMd({ sessionId, ...payload }: SessionScopedPayload<EmptyPayload>): Promise<void> {
    return this.sessionApi(sessionId).generateAgentsMd(payload);
  }

  createGoal({
    sessionId,
    ...payload
  }: SessionScopedPayload<CreateGoalPayload>): Promise<GoalSnapshot> {
    return Promise.resolve(this.sessionApi(sessionId).createGoal(payload));
  }

  getGoal({ sessionId, ...payload }: SessionScopedPayload<EmptyPayload>): GoalToolResult {
    return this.sessionApi(sessionId).getGoal(payload);
  }

  pauseGoal({
    sessionId,
    ...payload
  }: SessionScopedPayload<GoalControlPayload>): Promise<GoalSnapshot> {
    return Promise.resolve(this.sessionApi(sessionId).pauseGoal(payload));
  }

  resumeGoal({
    sessionId,
    ...payload
  }: SessionScopedPayload<GoalControlPayload>): Promise<GoalSnapshot> {
    return Promise.resolve(this.sessionApi(sessionId).resumeGoal(payload));
  }

  cancelGoal({
    sessionId,
    ...payload
  }: SessionScopedPayload<GoalControlPayload>): Promise<GoalSnapshot> {
    return Promise.resolve(this.sessionApi(sessionId).cancelGoal(payload));
  }

  async installPlugin(payload: InstallPluginPayload): Promise<PluginSummary> {
    await this.pluginsReady;
    this.assertPluginsLoaded();
    const record = await this.plugins.install(payload.source);
    return this.plugins.summaries().find((s) => s.id === record.id)!;
  }

  async listPlugins(_: EmptyPayload): Promise<readonly PluginSummary[]> {
    await this.pluginsReady;
    this.assertPluginsLoaded();
    return this.plugins.summaries();
  }

  async setPluginEnabled({ id, enabled }: SetPluginEnabledPayload): Promise<void> {
    await this.pluginsReady;
    this.assertPluginsLoaded();
    await this.plugins.setEnabled(id, enabled);
  }

  async setPluginMcpServerEnabled({
    id,
    server,
    enabled,
  }: SetPluginMcpServerEnabledPayload): Promise<void> {
    await this.pluginsReady;
    this.assertPluginsLoaded();
    await this.plugins.setMcpServerEnabled(id, server, enabled);
  }

  async removePlugin({ id }: RemovePluginPayload): Promise<void> {
    await this.pluginsReady;
    this.assertPluginsLoaded();
    await this.plugins.remove(id);
  }

  async reloadPlugins(_: EmptyPayload): Promise<ReloadPluginsResult> {
    try {
      const summary = await this.plugins.reload();
      this.pluginsLoadError = undefined;
      return summary;
    } catch (error) {
      this.pluginsLoadError = error instanceof Error ? error : new Error(String(error));
      throw new KimiError(
        ErrorCodes.PLUGIN_LOAD_FAILED,
        `Failed to reload plugins: ${this.pluginsLoadError.message}`,
        { cause: error, details: { kimiHomeDir: this.homeDir } },
      );
    }
  }

  async getPluginInfo({ id }: GetPluginInfoPayload): Promise<PluginInfo> {
    await this.pluginsReady;
    this.assertPluginsLoaded();
    const info = this.plugins.info(id);
    if (info === undefined) {
      throw new KimiError(
        ErrorCodes.PLUGIN_NOT_FOUND,
        `Plugin "${id}" is not installed`,
        { details: { id } },
      );
    }
    return info;
  }

  private assertPluginsLoaded(): void {
    if (this.pluginsLoadError === undefined) return;
    throw new KimiError(
      ErrorCodes.PLUGIN_LOAD_FAILED,
      `Plugin state failed to load: ${this.pluginsLoadError.message}. ` +
        `Fix the file at ${this.homeDir}/plugins/installed.json and run /plugins reload.`,
      { cause: this.pluginsLoadError, details: { kimiHomeDir: this.homeDir } },
    );
  }

  private async resolveRuntime(config: KimiConfig): Promise<ToolServices> {
    if (this.runtime !== undefined) return this.runtime;
    const runtime = await createRuntimeConfig({
      config,
      kimiRequestHeaders: this.kimiRequestHeaders,
      resolveOAuthTokenProvider: this.resolveOAuthTokenProvider,
    });
    this.runtime = runtime;
    return runtime;
  }

  private resolveSessionSkillConfig(config: KimiConfig): SessionSkillConfig {
    const explicitDirs = this.skillDirs.length > 0 ? this.skillDirs : undefined;
    return {
      userHomeDir: this.userHomeDir,
      explicitDirs,
      extraDirs: config.extraSkillDirs,
      pluginSkillRoots: this.plugins.pluginSkillRoots(),
      mergeAllAvailableSkills: config.mergeAllAvailableSkills,
    };
  }

  private resolveProviderManager(sessionId: string): ProviderManager {
    return new ProviderManager({
      config: () => this.config,
      kimiRequestHeaders: this.kimiRequestHeaders,
      resolveOAuthTokenProvider: this.resolveOAuthTokenProvider,
      promptCacheKey: sessionId,
    });
  }

  private mergePluginMcpConfig(base: SessionMcpConfig | undefined): SessionMcpConfig | undefined {
    const pluginServers = this.plugins.enabledMcpServers();
    if (Object.keys(pluginServers).length === 0) return base;
    return {
      servers: {
        ...base?.servers,
        ...pluginServers,
      },
    };
  }

  private mergeBuiltInMcpConfig(
    base: SessionMcpConfig | undefined,
    ctx: { sessionId: string; config: KimiConfig },
  ): SessionMcpConfig | undefined {
    const builtInServers = this.builtInMcpRegistry.getEnabledConfigs(
      {
        kimiHomeDir: this.homeDir,
        sessionId: ctx.sessionId,
        chromePort: ctx.config.browser?.chromePort,
      },
      ctx.config,
    );
    if (Object.keys(builtInServers).length === 0) return base;
    return {
      servers: {
        ...base?.servers,
        ...builtInServers,
      },
    };
  }

  private sessionApi(sessionId: string): SessionAPIImpl {
    const session = this.sessions.get(sessionId);
    if (session === undefined) {
      throw new KimiError(ErrorCodes.SESSION_NOT_FOUND, `Session "${sessionId}" was not found`, {
        details: { sessionId },
      });
    }
    return new SessionAPIImpl(session);
  }

  private reloadProviderManager(): KimiConfig {
    return this.config = loadRuntimeConfig(this.configPath);
  }

  private async refreshSessionRuntimeConfig(
    session: Session,
    config: KimiConfig,
  ): Promise<void> {
    const main = session.agents.get('main')!;
    const currentMode = main.sessionMode.isActive ? main.sessionMode.kind : 'normal';
    const fromConfig =
      currentMode === 'plan'
        ? config.modeModels?.plan
        : currentMode === 'design'
          ? config.modeModels?.design
          : config.defaultModel;
    const model = fromConfig?.trim() ?? '';
    if (model.length === 0) return;
    const api = new SessionAPIImpl(session);
    await api.setModel({ agentId: 'main', model });
    await session.flushMetadata();
  }
}

async function createRuntimeConfig(input: {
  readonly config: KimiConfig;
  readonly kimiRequestHeaders?: Record<string, string> | undefined;
  readonly resolveOAuthTokenProvider?: OAuthTokenProviderResolver | undefined;
}): Promise<ToolServices> {
  const localFetcher = new LocalFetchURLProvider();
  const searchService = input.config.services?.moonshotSearch;
  const fetchService = input.config.services?.moonshotFetch;

  return {
    urlFetcher:
      fetchService?.baseUrl === undefined
        ? localFetcher
        : new MoonshotFetchURLProvider({
            baseUrl: fetchService.baseUrl,
            localFallback: localFetcher,
            defaultHeaders: input.kimiRequestHeaders,
            ...serviceCredentials(fetchService, input.resolveOAuthTokenProvider),
          }),
    webSearcher:
      searchService?.baseUrl === undefined
        ? undefined
        : new MoonshotWebSearchProvider({
            baseUrl: searchService.baseUrl,
            defaultHeaders: input.kimiRequestHeaders,
            ...serviceCredentials(searchService, input.resolveOAuthTokenProvider),
          }),
  };
}

function serviceCredentials(
  service: MoonshotServiceConfig,
  resolveOAuthTokenProvider: OAuthTokenProviderResolver | undefined,
): {
  readonly apiKey?: string | undefined;
  readonly tokenProvider?: BearerTokenProvider | undefined;
  readonly customHeaders?: Record<string, string> | undefined;
} {
  const apiKey = nonEmptyString(service.apiKey);
  return {
    apiKey,
    tokenProvider:
      service.oauth !== undefined
        ? resolveOAuthTokenProvider?.(ODY_CODE_PROVIDER_NAME, service.oauth)
        : undefined,
    customHeaders: service.customHeaders,
  };
}

function nonEmptyString(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed === undefined || trimmed.length === 0 ? undefined : trimmed;
}

function requiredWorkDir(operation: string, value: string): string {
  if (typeof value !== 'string' || value.trim() === '') {
    throw new KimiError(ErrorCodes.REQUEST_WORK_DIR_REQUIRED, `${operation} requires workDir`);
  }
  return normalizeWorkDir(value);
}

function createSessionId(): string {
  return `session_${randomUUID()}`;
}

function telemetryErrorReason(error: unknown): string {
  if (error instanceof KimiError) return error.code;
  if (error instanceof Error && error.name.length > 0) return error.name;
  return typeof error;
}

async function resumeSessionResult(
  summary: SessionSummary,
  session: Session,
  warning?: string,
): Promise<ResumeSessionResult> {
  const api = new SessionAPIImpl(session);
  const agents: Record<string, ResumedAgentState> = {};
  for (const [agentId, agent] of session.agents) {
    const config = await api.getConfig({ agentId });
    const context = await api.getContext({ agentId });
    const permission = await api.getPermission({ agentId });
    const sessionMode = await api.getPlan({ agentId });
    const usage = await api.getUsage({ agentId });
    agents[agentId] = {
      type: agent.type,
      config,
      context,
      replay: agent.replayBuilder.buildResult(),
      permission,
      sessionMode,
      usage,
      tools: await api.getTools({ agentId }),
      toolStore: agent.tools.storeData(),
      background: agent.background.list(false),
    };
  }
  return {
    ...summary,
    sessionMetadata: api.getSessionMetadata({}),
    agents,
    warning,
  };
}

async function warnIfLogFlushFails(
  exportLog: Logger,
  message: string,
  flush: () => Promise<boolean>,
): Promise<void> {
  try {
    if (await flush()) return;
    exportLog.warn(message);
  } catch (error) {
    exportLog.warn(message, { error });
  }
  try {
    await flush();
  } catch {}
}
