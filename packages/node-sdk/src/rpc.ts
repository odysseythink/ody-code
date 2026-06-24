import { randomUUID } from 'node:crypto';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { connect as connectNet, type Socket } from 'node:net';
import { once } from 'node:events';
import { MessageChannel, Worker, type MessagePort } from 'node:worker_threads';

import {
  createMessagePortTransport,
  createRPCEndpoint,
  createRPC,
  createStreamTransport,
  createWebSocketTransport,
  ErrorCodes,
  KimiCore,
  KosongLLM,
  makeErrorPayload,
  resolveOdyHome,
  toOdyErrorPayload,
  type AgentContextData,
  type ApprovalRequest,
  type ApprovalResponse,
  type ChatStreamCancelPayload,
  type ChatStreamDeltaPayload,
  type ChatStreamEndPayload,
  type ChatStreamErrorPayload,
  type ChatStreamInitPayload,
  type ChatStreamInitResponse,
  type ChatStreamResult,
  type CodeReviewReportData,
  type CoreAPI,
  type DesignReviewData,
  type Dispatch,
  type Event,
  type ExperimentalFlagMap,
  type OAuthTokenProviderResolver,
  type OpenExternalRequest,
  type OpenExternalResponse,
  type QuestionRequest,
  type QuestionResult,
  type RequestCodeReviewPayload,
  type SDKAPI,
  type SDKRPCClient,
  type TelemetryClient,
  type ToolCallRequest,
  type ToolCallResponse,
  type Transport,
} from '@odysseythink/agent-core';
import { createProvider } from '@odysseythink/kosong';
import { createKimiDefaultHeaders } from '@odysseythink/kimi-code-oauth';
import type { CoreWorkerBootPayload } from '#/core-worker';

import type { ApprovalHandler, OpenExternalHandler, QuestionHandler } from '#/events';
import type {
  BackgroundTaskInfo,
  CreateSessionOptions,
  ExportSessionInput,
  ExportSessionResult,
  CreateGoalInput,
  ForkSessionInput,
  GetConfigOptions,
  GoalSnapshot,
  GoalToolResult,
  OdyConfig,
  OdyConfigPatch,
  ListSessionsOptions,
  McpServerInfo,
  McpStartupMetrics,
  PermissionMode,
  PluginInfo,
  PluginSummary,
  ReloadSummary,
  CompactOptions,
  SessionPlan,
  SessionStatus,
  SessionUsage,
  PromptInput,
  RenameSessionInput,
  ResumeSessionInput,
  ResumedSessionSummary,
  SessionSummary,
  SkillSummary,
  Unsubscribe,
  KimiHostIdentity,
} from '#/types';

const MAIN_AGENT_ID = 'main';

export interface SDKRpcClientOptions {
  readonly homeDir?: string | undefined;
  readonly configPath?: string | undefined;
  readonly identity?: KimiHostIdentity | undefined;
  readonly resolveOAuthTokenProvider?: OAuthTokenProviderResolver | undefined;
  readonly skillDirs?: readonly string[];
  readonly telemetry?: TelemetryClient | undefined;

  /**
   * Run the core in a dedicated Worker thread instead of the current process.
   * Defaults to false unless environment variable ODY_CORE_TRANSPORT=worker is set.
   */
  readonly worker?: boolean | undefined;

  /**
   * Absolute path to the worker entry script. Defaults to the package's `./core-worker` export.
   */
  readonly workerScriptPath?: string | undefined;
}

export interface SDKRpcClientConnectOptions {
  readonly transport:
    | 'stdio'
    | { readonly socketPath: string }
    | { readonly host: string; readonly port: number; readonly webSocket?: boolean };
  readonly token?: string;
  readonly homeDir?: string;
  readonly configPath?: string;
  readonly skillDirs?: readonly string[];
  readonly telemetry?: TelemetryClient;
}

interface ReadyMessage {
  readonly type: 'ready';
  readonly token?: string;
  readonly socketPath?: string;
  readonly host?: string;
  readonly port?: number;
  readonly stdio: boolean;
}

export interface SessionPromptRpcInput {
  readonly sessionId: string;
  readonly input: PromptInput;
}

export interface SessionIdRpcInput {
  readonly sessionId: string;
}

export interface SetSessionModelRpcInput extends SessionIdRpcInput {
  readonly model: string;
}

export interface SetSessionModelRpcResult {
  readonly model: string;
  readonly providerName?: string | undefined;
}

export interface SetSessionThinkingRpcInput extends SessionIdRpcInput {
  readonly level: string;
}

export interface SetSessionPermissionRpcInput extends SessionIdRpcInput {
  readonly mode: PermissionMode;
}

export interface SetSessionModeRpcInput extends SessionIdRpcInput {
  readonly mode: 'plan' | 'design' | 'office-hours' | 'game-design' | 'normal';
  readonly sourceFilePath?: string;
}

export interface ActivateSkillRpcInput extends SessionIdRpcInput {
  readonly name: string;
  readonly args?: string | undefined;
}

export interface ReconnectMcpServerRpcInput extends SessionIdRpcInput {
  readonly name: string;
}

export interface ReviewDesignRpcInput extends SessionIdRpcInput {
  readonly path?: string;
  readonly modelAlias?: string;
  readonly kind?: 'plan' | 'design';
  readonly timeoutMs?: number;
}

type ResolvedCoreAPI = Awaited<ReturnType<SDKRPCClient>>;

async function createExternalTransport(
  options: SDKRpcClientConnectOptions,
  dispatch: Dispatch,
): Promise<Transport> {
  if (options.transport === 'stdio') {
    const { spawn } = await import('node:child_process');
    const proc = spawn('ody', ['serve', '--stdio'], {
      stdio: ['pipe', 'pipe', 'pipe'],
    });

    await new Promise<ReadyMessage>((resolve, reject) => {
      const onData = (chunk: Buffer): void => {
        const lines = chunk.toString('utf8').split('\n');
        for (const line of lines) {
          if (!line.trim()) continue;
          try {
            const msg = JSON.parse(line) as ReadyMessage;
            if (msg.type === 'ready' && msg.stdio) {
              proc.stderr.off('data', onData);
              resolve(msg);
              return;
            }
          } catch {
            // ignore non-JSON stderr lines
          }
        }
      };
      proc.stderr.on('data', onData);
      proc.once('error', reject);
      proc.once('exit', (code) => reject(new Error(`ody serve exited with ${String(code)}`)));
    });

    return createStreamTransport(proc.stdout, proc.stdin, dispatch, { framing: 'length-prefixed' });
  }

  if ('socketPath' in options.transport) {
    const socket: Socket = connectNet(options.transport.socketPath);
    await once(socket, 'connect');
    return createStreamTransport(socket, socket, dispatch, { framing: 'length-prefixed' });
  }

  const { host, port, webSocket } = options.transport;

  if (webSocket) {
    const ws = new WebSocket(`ws://${host}:${port}`);
    await new Promise<void>((resolve, reject) => {
      ws.onopen = () => resolve();
      ws.onerror = () => reject(new Error('WebSocket connection failed'));
    });
    const adapted = {
      send: (data: string) => ws.send(data),
      close: () => ws.close(),
      onmessage: null as ((event: { data: string | Uint8Array }) => void) | null,
      onerror: null as ((event: { type: string }) => void) | null,
      onclose: null as ((event: { type: string }) => void) | null,
    };
    ws.onmessage = (event: MessageEvent) => {
      adapted.onmessage?.({ data: typeof event.data === 'string' ? event.data : new Uint8Array(event.data) });
    };
    ws.onerror = () => adapted.onerror?.({ type: 'error' });
    ws.onclose = () => adapted.onclose?.({ type: 'close' });
    return createWebSocketTransport(adapted, dispatch);
  }

  const socket: Socket = connectNet(port, host);
  await once(socket, 'connect');
  return createStreamTransport(socket, socket, dispatch, {
    framing: options.token === undefined ? 'length-prefixed' : undefined,
    token: options.token,
  });
}

export class SDKRpcClient {
  readonly core: KimiCore;
  interactiveAgentId = MAIN_AGENT_ID;
  private readonly ready: Promise<void>;
  private rpc: ResolvedCoreAPI | undefined;
  private readonly eventListeners = new Set<(event: Event) => void>();
  private readonly approvalHandlers = new Map<string, ApprovalHandler>();
  private readonly questionHandlers = new Map<string, QuestionHandler>();
  private readonly openExternalHandlers = new Map<string, OpenExternalHandler>();
  private readonly codeReviewProgressHandlers = new Map<string, (progress: { requestId: string; stage: string; modelAlias: string; detail?: string; meta?: { estimatedTokens?: number; filePath?: string; fileCount?: number } }) => void>();

  constructor(options: SDKRpcClientOptions = {}, _external?: boolean) {
    if (_external) {
      const homeDir = resolveOdyHome(options.homeDir);
      const configPath = options.configPath;
      this.core = { homeDir, configPath } as KimiCore;
      this.ready = Promise.resolve();
      this.eventListeners = new Set();
      this.approvalHandlers = new Map();
      this.questionHandlers = new Map();
      this.openExternalHandlers = new Map();
      this.codeReviewProgressHandlers = new Map();
      return;
    }
    const useWorker = options.worker ?? process.env['ODY_CORE_TRANSPORT'] === 'worker';
    const homeDir = resolveOdyHome(options.homeDir);
    const configPath = options.configPath;
    const kimiRequestHeaders =
      options.identity === undefined
        ? undefined
        : createKimiDefaultHeaders({ homeDir, ...options.identity });
    const coreOptions = {
      homeDir: options.homeDir,
      configPath,
      kimiRequestHeaders,
      resolveOAuthTokenProvider: options.resolveOAuthTokenProvider,
      skillDirs: options.skillDirs,
      telemetry: options.telemetry,
      appVersion: options.identity?.version,
    };

    if (!useWorker) {
      const [coreRpc, sdkRpc] = createRPC<CoreAPI, SDKAPI>();
      this.core = new KimiCore(coreRpc, coreOptions);
      this.ready = sdkRpc(new ClientAPI(this, () => this.getRpc())).then((rpc) => {
        this.rpc = rpc;
      });
      return;
    }

    // Worker mode: Core runs in a dedicated worker thread.
    const { port1, port2 } = new MessageChannel();
    const endpoint = createRPCEndpoint<SDKAPI, CoreAPI>();
    const bootPayload: CoreWorkerBootPayload = {
      homeDir: options.homeDir,
      configPath,
      skillDirs: options.skillDirs,
      appVersion: options.identity?.version,
    };
    const worker = this.spawnCoreWorker(port2, options.workerScriptPath, bootPayload);

    const transport = createMessagePortTransport(port1, endpoint.dispatch, {
      onError: (error) => {
        worker.terminate().catch(() => {});
        throw error;
      },
    });
    endpoint.setTransport(transport);

    // In worker mode the real CoreAPI implementation lives in the worker; the local
    // `core` field is only exposed for `homeDir`/`configPath` getters.
    this.core = { homeDir, configPath } as KimiCore;

    const clientApi = new ClientAPI(this, () => this.getRpc());
    this.ready = Promise.all([
      this.waitForWorkerReady(port1, worker),
      endpoint.client(clientApi).then((rpc) => {
        this.rpc = rpc;
      }),
    ]).then(() => undefined);
  }

  private spawnCoreWorker(
    port: MessagePort,
    workerScriptPath: string | undefined,
    bootPayload: CoreWorkerBootPayload,
  ): Worker {
    const workerData = { port, ...bootPayload };
    if (workerScriptPath !== undefined) {
      return new Worker(workerScriptPath, { workerData, transferList: [port] });
    }

    // Prefer the built worker entry in production/CI; fall back to tsx in development.
    const distWorkerPath = fileURLToPath(
      new URL('../dist/core-worker.mjs', import.meta.url),
    );
    if (existsSync(distWorkerPath)) {
      return new Worker(distWorkerPath, { workerData, transferList: [port] });
    }

    const srcWorkerPath = fileURLToPath(new URL('./core-worker.ts', import.meta.url));
    return new Worker(srcWorkerPath, {
      workerData,
      transferList: [port],
      execArgv: ['--import', 'tsx/esm'],
    });
  }

  private waitForWorkerReady(port: MessagePort, worker: Worker): Promise<void> {
    return new Promise((resolve, reject) => {
      const cleanup = (): void => {
        worker.off('error', onError);
        worker.off('exit', onExit);
        port.off('message', onMessage);
      };
      const onError = (error: Error): void => {
        cleanup();
        reject(error);
      };
      const onExit = (code: number): void => {
        if (code !== 0) {
          cleanup();
          reject(new Error(`Core worker exited with code ${code}`));
        }
      };
      const onMessage = (msg: unknown): void => {
        if (typeof msg !== 'object' || msg === null) return;
        const typed = msg as { type?: string; error?: string };
        if (typed.type === 'ready') {
          cleanup();
          resolve();
        } else if (typed.type === 'error') {
          cleanup();
          reject(new Error(typed.error ?? 'Core worker failed to start'));
        }
      };
      worker.on('error', onError);
      worker.on('exit', onExit);
      port.on('message', onMessage);
    });
  }

  static async connect(options: SDKRpcClientConnectOptions): Promise<SDKRpcClient> {
    const instance = new SDKRpcClient(
      {
        homeDir: options.homeDir,
        configPath: options.configPath,
        skillDirs: options.skillDirs,
        telemetry: options.telemetry,
      },
      true,
    );

    const endpoint = createRPCEndpoint<SDKAPI, CoreAPI>();
    const transport = await createExternalTransport(options, endpoint.dispatch);
    endpoint.setTransport(transport);

    const clientApi = new ClientAPI(instance, () => instance.getRpc());
    const rpc = await endpoint.client(clientApi);
    Object.assign(instance, { rpc, ready: Promise.resolve() });
    return instance;
  }

  get homeDir(): string {
    return this.core.homeDir;
  }

  get configPath(): string {
    return this.core.configPath;
  }

  async createSession(input: CreateSessionOptions): Promise<SessionSummary> {
    const rpc = await this.getRpc();
    const { sessionMode, ...coreInput } = input;
    void sessionMode;
    return rpc.createSession(coreInput);
  }

  async resumeSession(input: ResumeSessionInput): Promise<ResumedSessionSummary> {
    const rpc = await this.getRpc();
    return rpc.resumeSession({ sessionId: input.id });
  }

  async forkSession(input: ForkSessionInput): Promise<SessionSummary> {
    const rpc = await this.getRpc();
    return rpc.forkSession({
      sessionId: input.id,
      id: input.forkId,
      title: input.title,
      metadata: input.metadata,
    });
  }

  async closeSession(input: SessionIdRpcInput): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.closeSession({ sessionId: input.sessionId });
  }

  async listSessions(input: ListSessionsOptions = {}): Promise<readonly SessionSummary[]> {
    const rpc = await this.getRpc();
    return rpc.listSessions(input);
  }

  async renameSession(input: RenameSessionInput): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.renameSession({
      sessionId: input.id,
      title: input.title,
    });
  }

  async exportSession(input: ExportSessionInput): Promise<ExportSessionResult> {
    const rpc = await this.getRpc();
    return rpc.exportSession({
      sessionId: input.id,
      outputPath: input.outputPath,
      includeGlobalLog: input.includeGlobalLog,
      version: input.version,
      installSource: input.installSource,
      shellEnv: input.shellEnv,
    });
  }

  async getConfig(input?: GetConfigOptions): Promise<OdyConfig> {
    const rpc = await this.getRpc();
    return rpc.getOdyConfig(input ?? {});
  }

  async getExperimentalFlags(): Promise<ExperimentalFlagMap> {
    const rpc = await this.getRpc();
    return rpc.getExperimentalFlags({});
  }

  async setConfig(input: OdyConfigPatch): Promise<OdyConfig> {
    const rpc = await this.getRpc();
    return rpc.setOdyConfig(input);
  }

  async removeProvider(providerId: string): Promise<OdyConfig> {
    const rpc = await this.getRpc();
    return rpc.removeKimiProvider({ providerId });
  }

  async prompt(input: SessionPromptRpcInput): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.prompt({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
      input: input.input,
    });
  }

  async steer(input: SessionPromptRpcInput): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.steer({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
      input: input.input,
    });
  }

  async generateAgentsMd(input: SessionIdRpcInput): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.generateAgentsMd({ sessionId: input.sessionId });
  }

  async runSetupScript(input: SessionIdRpcInput): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.runSetupScript({ sessionId: input.sessionId });
  }

  async cancel(input: SessionIdRpcInput): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.cancel({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
    });
  }

  async setModel(input: SetSessionModelRpcInput): Promise<SetSessionModelRpcResult> {
    const rpc = await this.getRpc();
    return rpc.setModel({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
      model: input.model,
    });
  }

  async setThinking(input: SetSessionThinkingRpcInput): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.setThinking({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
      level: input.level,
    });
  }

  async setPermission(input: SetSessionPermissionRpcInput): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.setPermission({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
      mode: input.mode,
    });
  }

  async setSessionMode(input: SetSessionModeRpcInput): Promise<void> {
    const rpc = await this.getRpc();
    if (input.mode === 'normal') {
      return rpc.cancelPlan({
        sessionId: input.sessionId,
        agentId: this.interactiveAgentId,
      });
    }
    return rpc.enterPlan({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
      kind: input.mode,
      sourceFilePath: input.sourceFilePath,
    });
  }

  async getPlan(input: SessionIdRpcInput): Promise<SessionPlan> {
    const rpc = await this.getRpc();
    return rpc.getPlan({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
    });
  }

  async clearPlan(input: SessionIdRpcInput): Promise<void> {
    const rpc = await this.getRpc();
    await rpc.clearPlan({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
    });
  }

  async writingPlan(input: SessionIdRpcInput & { filePath: string }): Promise<void> {
    const rpc = await this.getRpc();
    await rpc.enterPlan({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
      kind: 'plan',
      sourceFilePath: input.filePath,
    });
  }

  async reviewDesign(input: ReviewDesignRpcInput): Promise<DesignReviewData> {
    const rpc = await this.getRpc();
    return rpc.reviewDesign({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
      path: input.path,
      modelAlias: input.modelAlias,
      kind: input.kind,
      timeoutMs: input.timeoutMs,
    });
  }

  async requestCodeReview(
    input: RequestCodeReviewPayload & {
      readonly onProgress?: (progress: { requestId: string; stage: string; modelAlias: string; detail?: string; meta?: { estimatedTokens?: number; filePath?: string; fileCount?: number } }) => void;
    },
  ): Promise<CodeReviewReportData> {
    const rpc = await this.getRpc();
    let requestId: string | undefined;
    if (input.onProgress) {
      requestId = randomUUID();
      this.codeReviewProgressHandlers.set(requestId, input.onProgress);
    }
    try {
      const { onProgress: _, ...payload } = input;
      return await rpc.requestCodeReview({ ...payload, requestId, workDir: (payload as any).workDir ?? process.cwd() });
    } finally {
      if (requestId !== undefined) {
        this.codeReviewProgressHandlers.delete(requestId);
      }
    }
  }

  async compact(input: SessionIdRpcInput & CompactOptions): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.beginCompaction({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
      ...(input.instruction !== undefined ? { instruction: input.instruction } : {}),
    });
  }

  async cancelCompaction(input: SessionIdRpcInput): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.cancelCompaction({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
    });
  }

  async undoHistory(input: SessionIdRpcInput & { count: number }): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.undoHistory({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
      count: input.count,
    });
  }

  async getContext(input: SessionIdRpcInput): Promise<AgentContextData> {
    const rpc = await this.getRpc();
    return rpc.getContext({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
    });
  }

  async getUsage(input: SessionIdRpcInput): Promise<SessionUsage> {
    const rpc = await this.getRpc();
    return rpc.getUsage({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
    });
  }

  async getStatus(input: SessionIdRpcInput): Promise<SessionStatus> {
    const rpc = await this.getRpc();
    const agentId = this.interactiveAgentId;
    const config = await rpc.getConfig({
      sessionId: input.sessionId,
      agentId,
    });
    const context = await rpc.getContext({
      sessionId: input.sessionId,
      agentId,
    });
    const permission = await rpc.getPermission({
      sessionId: input.sessionId,
      agentId,
    });
    const plan = await rpc.getPlan({
      sessionId: input.sessionId,
      agentId,
    });
    const usage = await rpc.getUsage({
      sessionId: input.sessionId,
      agentId,
    });
    const maxContextTokens = config.modelCapabilities?.max_context_tokens ?? 0;
    const contextTokens = context.tokenCount;
    const contextUsage = maxContextTokens > 0 ? contextTokens / maxContextTokens : 0;
    const hasUsage =
      usage.byModel !== undefined || usage.total !== undefined || usage.currentTurn !== undefined;
    const userLanguage = await rpc.getUserLanguage({
      sessionId: input.sessionId,
      agentId,
    });
    return {
      model: config.modelAlias ?? config.provider?.model,
      thinkingLevel: config.thinkingLevel,
      permission: permission.mode,
      sessionMode: plan !== null ? plan.kind : 'normal',
      sessionModeFilePath: plan?.path ?? null,
      contextTokens,
      maxContextTokens,
      contextUsage,
      usage: hasUsage ? usage : undefined,
      userLanguage,
    };
  }

  async listSkills(input: SessionIdRpcInput & { sessionMode?: 'normal' | 'plan' | 'design' | 'office-hours' | 'game-design' }): Promise<readonly SkillSummary[]> {
    const rpc = await this.getRpc();
    return rpc.listSkills({ sessionId: input.sessionId, sessionMode: input.sessionMode });
  }

  async listBackgroundTasks(
    input: SessionIdRpcInput & { activeOnly?: boolean; limit?: number },
  ): Promise<readonly BackgroundTaskInfo[]> {
    const rpc = await this.getRpc();
    return rpc.getBackground({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
      activeOnly: input.activeOnly,
      limit: input.limit,
    });
  }

  async getBackgroundTaskOutput(
    input: SessionIdRpcInput & { taskId: string; tail?: number },
  ): Promise<string> {
    const rpc = await this.getRpc();
    return rpc.getBackgroundOutput({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
      taskId: input.taskId,
      tail: input.tail,
    });
  }

  async stopBackgroundTask(
    input: SessionIdRpcInput & { taskId: string; reason?: string },
  ): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.stopBackground({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
      taskId: input.taskId,
      reason: input.reason,
    });
  }

  async createGoal(input: SessionIdRpcInput & CreateGoalInput): Promise<GoalSnapshot> {
    const rpc = await this.getRpc();
    return rpc.createGoal({
      sessionId: input.sessionId,
      objective: input.objective,
      completionCriterion: input.completionCriterion,
      budgetLimits: input.budgetLimits,
      replace: input.replace,
    });
  }

  async getGoal(input: SessionIdRpcInput): Promise<GoalToolResult> {
    const rpc = await this.getRpc();
    return rpc.getGoal({ sessionId: input.sessionId });
  }

  async pauseGoal(input: SessionIdRpcInput & { reason?: string }): Promise<GoalSnapshot> {
    const rpc = await this.getRpc();
    return rpc.pauseGoal({ sessionId: input.sessionId, reason: input.reason });
  }

  async resumeGoal(input: SessionIdRpcInput & { reason?: string }): Promise<GoalSnapshot> {
    const rpc = await this.getRpc();
    return rpc.resumeGoal({ sessionId: input.sessionId, reason: input.reason });
  }

  async cancelGoal(input: SessionIdRpcInput & { reason?: string }): Promise<GoalSnapshot> {
    const rpc = await this.getRpc();
    return rpc.cancelGoal({ sessionId: input.sessionId, reason: input.reason });
  }

  async listMcpServers(input: SessionIdRpcInput): Promise<readonly McpServerInfo[]> {
    const rpc = await this.getRpc();
    return rpc.listMcpServers({ sessionId: input.sessionId });
  }

  async getMcpStartupMetrics(input: SessionIdRpcInput): Promise<McpStartupMetrics> {
    const rpc = await this.getRpc();
    return rpc.getMcpStartupMetrics({ sessionId: input.sessionId });
  }

  async reconnectMcpServer(input: ReconnectMcpServerRpcInput): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.reconnectMcpServer({ sessionId: input.sessionId, name: input.name });
  }

  async listPlugins(): Promise<readonly PluginSummary[]> {
    const rpc = await this.getRpc();
    return rpc.listPlugins({});
  }

  async installPlugin(source: string): Promise<PluginSummary> {
    const rpc = await this.getRpc();
    return rpc.installPlugin({ source });
  }

  async setPluginEnabled(id: string, enabled: boolean): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.setPluginEnabled({ id, enabled });
  }

  async setPluginMcpServerEnabled(
    id: string,
    server: string,
    enabled: boolean,
  ): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.setPluginMcpServerEnabled({ id, server, enabled });
  }

  async removePlugin(id: string): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.removePlugin({ id });
  }

  async reloadPlugins(): Promise<ReloadSummary> {
    const rpc = await this.getRpc();
    return rpc.reloadPlugins({});
  }

  async getPluginInfo(id: string): Promise<PluginInfo> {
    const rpc = await this.getRpc();
    return rpc.getPluginInfo({ id });
  }

  async activateSkill(input: ActivateSkillRpcInput): Promise<void> {
    const rpc = await this.getRpc();
    return rpc.activateSkill({
      sessionId: input.sessionId,
      agentId: this.interactiveAgentId,
      name: input.name,
      args: input.args,
    });
  }

  onEvent(listener: (event: Event) => void): Unsubscribe {
    this.eventListeners.add(listener);
    return () => {
      this.eventListeners.delete(listener);
    };
  }

  receiveEvent(event: Event): void {
    for (const listener of this.eventListeners) {
      listener(event);
    }
    if (event.type === 'codeReview.progress') {
      const handler = this.codeReviewProgressHandlers.get(event.requestId);
      if (handler) {
        try {
          handler({
            requestId: event.requestId,
            stage: event.stage,
            modelAlias: event.modelAlias,
            detail: event.detail,
            meta: event.meta,
          });
        } catch {
          // Silently swallow user callback errors to avoid breaking the request
        }
      }
    }
  }

  setApprovalHandler(sessionId: string, handler: ApprovalHandler | undefined): void {
    if (handler === undefined) {
      this.approvalHandlers.delete(sessionId);
      return;
    }
    this.approvalHandlers.set(sessionId, handler);
  }

  setQuestionHandler(sessionId: string, handler: QuestionHandler | undefined): void {
    if (handler === undefined) {
      this.questionHandlers.delete(sessionId);
      return;
    }
    this.questionHandlers.set(sessionId, handler);
  }

  setOpenExternalHandler(sessionId: string, handler: OpenExternalHandler | undefined): void {
    if (handler === undefined) {
      this.openExternalHandlers.delete(sessionId);
      return;
    }
    this.openExternalHandlers.set(sessionId, handler);
  }

  clearSessionHandlers(sessionId: string): void {
    this.approvalHandlers.delete(sessionId);
    this.questionHandlers.delete(sessionId);
    this.openExternalHandlers.delete(sessionId);
  }

  async requestApproval(
    request: ApprovalRequest & { sessionId: string; agentId: string },
  ): Promise<ApprovalResponse> {
    const handler = this.approvalHandlers.get(request.sessionId);
    if (handler === undefined) {
      return {
        decision: 'cancelled',
        feedback: 'No approval handler registered.',
      };
    }

    try {
      return await handler(request);
    } catch (error) {
      this.receiveEvent({
        type: 'error',
        sessionId: request.sessionId,
        agentId: request.agentId,
        ...makeErrorPayload(ErrorCodes.SESSION_APPROVAL_HANDLER_ERROR, errorMessage(error)),
      });
      return {
        decision: 'cancelled',
        feedback: 'Approval handler failed.',
      };
    }
  }

  async requestQuestion(
    request: QuestionRequest & { sessionId: string; agentId: string },
  ): Promise<QuestionResult> {
    const handler = this.questionHandlers.get(request.sessionId);
    if (handler === undefined) return null;

    try {
      return await handler(request);
    } catch (error) {
      this.receiveEvent({
        type: 'error',
        sessionId: request.sessionId,
        agentId: request.agentId,
        ...makeErrorPayload(ErrorCodes.SESSION_QUESTION_HANDLER_ERROR, errorMessage(error)),
      });
      return null;
    }
  }

  async openExternal(
    request: OpenExternalRequest & { sessionId: string; agentId: string },
  ): Promise<OpenExternalResponse> {
    const handler = this.openExternalHandlers.get(request.sessionId);
    if (handler === undefined) {
      return { opened: false, error: 'No open-external handler registered for this session.' };
    }

    try {
      return await handler(request);
    } catch (error) {
      return { opened: false, error: errorMessage(error) };
    }
  }

  async toolCall(request: ToolCallRequest): Promise<ToolCallResponse> {
    return {
      output: `SDK custom tool calls are not supported: ${request.toolCallId}`,
      isError: true,
    };
  }

  private async getRpc(): Promise<ResolvedCoreAPI> {
    await this.ready;
    if (this.rpc === undefined) {
      throw new Error('SDK RPC client was not initialized.');
    }
    return this.rpc;
  }
}

export class ClientAPI implements SDKAPI {
  private readonly activeStreams = new Map<string, AbortController>();

  constructor(
    readonly client: SDKRpcClient,
    private readonly getRpc: () => Promise<ResolvedCoreAPI>,
  ) {}

  emitEvent(event: Event): void {
    this.client.receiveEvent(event);
  }

  requestApproval(
    request: ApprovalRequest & { sessionId: string; agentId: string },
  ): Promise<ApprovalResponse> {
    return this.client.requestApproval(request);
  }

  requestQuestion(
    request: QuestionRequest & { sessionId: string; agentId: string },
  ): Promise<QuestionResult> {
    return this.client.requestQuestion(request);
  }

  openExternal(
    request: OpenExternalRequest & { sessionId: string; agentId: string },
  ): Promise<OpenExternalResponse> {
    return this.client.openExternal(request);
  }

  toolCall(request: ToolCallRequest): Promise<ToolCallResponse> {
    return this.client.toolCall(request);
  }

  async chatStreamInit(
    payload: ChatStreamInitPayload & { sessionId: string; agentId: string },
  ): Promise<ChatStreamInitResponse> {
    const streamId = randomUUID();
    const { request } = payload;
    const abortController = new AbortController();
    this.activeStreams.set(streamId, abortController);

    const rpc = await this.getRpc();
    const provider = createProvider(request.provider);
    const llm = new KosongLLM({
      provider,
      modelName: request.modelName,
      systemPrompt: request.systemPrompt,
      capability: request.capability,
      completionBudgetConfig: request.completionBudgetConfig,
    });

    void (async (): Promise<void> => {
      try {
        const response = await llm.chat({
          messages: [...request.messages],
          tools: [...request.tools],
          signal: abortController.signal,
          requestLogContext: request.requestLogContext,
          onTextDelta: (text): void => {
            this.dispatchDelta(rpc, { streamId, delta: { type: 'text', text } });
          },
          onThinkDelta: (think): void => {
            this.dispatchDelta(rpc, { streamId, delta: { type: 'think', think } });
          },
          onToolCallDelta: (delta): void => {
            this.dispatchDelta(rpc, {
              streamId,
              delta: {
                type: 'tool_call_part',
                toolCallId: delta.toolCallId,
                name: delta.name,
                argumentsPart: delta.argumentsPart,
              },
            });
          },
        });

        const result: ChatStreamResult = {
          toolCalls: response.toolCalls,
          providerFinishReason: response.providerFinishReason,
          rawFinishReason: response.rawFinishReason,
          usage: response.usage,
          streamTiming: response.streamTiming,
        };
        this.dispatchEnd(rpc, { streamId, result });
      } catch (error) {
        this.dispatchError(rpc, { streamId, error: toOdyErrorPayload(error) });
      } finally {
        this.activeStreams.delete(streamId);
      }
    })();

    return { streamId };
  }

  chatStreamCancel(
    payload: ChatStreamCancelPayload & { sessionId: string; agentId: string },
  ): void {
    this.activeStreams.get(payload.streamId)?.abort();
    this.activeStreams.delete(payload.streamId);
  }

  private dispatchDelta(rpc: ResolvedCoreAPI, payload: ChatStreamDeltaPayload): void {
    rpc.chatStreamDelta(payload).catch(() => {});
  }

  private dispatchEnd(rpc: ResolvedCoreAPI, payload: ChatStreamEndPayload): void {
    rpc.chatStreamEnd(payload).catch(() => {});
  }

  private dispatchError(rpc: ResolvedCoreAPI, payload: ChatStreamErrorPayload): void {
    rpc.chatStreamError(payload).catch(() => {});
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
