import { createHash } from 'node:crypto';
import { basename, dirname, isAbsolute, join } from 'pathe';

import { ErrorCodes, OdyError, makeErrorPayload } from '@odysseythink/agent-core-shared';
import { log } from '#logging/logger';
import type { Logger } from '@odysseythink/agent-core-shared';
import type { AgentAPI, AgentEvent, OdyConfig, SDKAgentRPC, UsageStatus } from '#rpc';
import type { LLM, LLMFactoryConfig } from '#loop/llm';
import {
  generate,
  type ChatProvider,
  type Message,
  type Tool,
} from '@odysseythink/kosong';

import type { EnabledPluginSessionStart } from '#plugin';

import type { McpConnectionManager } from '@odysseythink/mcp-host';
import type { PreparedSystemPromptContext, ResolvedAgentProfile } from '../profile';
import type { ModelProvider } from '../session/provider-manager';
import type { SessionGoalStore } from '../session/goal';
import type { SessionSubagentHost } from '../session/subagent-host';
import type { SkillRegistry } from '../skill';
import { noopTelemetryClient, type TelemetryClient } from '../telemetry';
import {
  estimateTokens,
  estimateTokensForMessages,
  estimateTokensForTools,
} from '../utils/tokens';
import { initGlobWasm } from '../utils/wasm-glob';
import type { PromisableMethods } from '../utils/types';
import { BackgroundManager, BackgroundTaskPersistence } from './background';
import {
  FullCompaction,
  MicroCompaction,
  NormalModeTaskCheckpoint,
  SplitPlanCheckpoint,
  type CompactionStrategy,
  type MicroCompactionConfig,
} from './compaction';
import { CronManager } from './cron';
import { ConfigState } from './config';
import { ContextMemory } from './context';
import { HookEngine } from '../session/hooks';

import { parseManifestFiles } from './injection/plan-mode-contract';
import { AdvancedSessionReviewer, shouldEscalate } from './session-mode/reviewer';
import { InjectionManager } from './injection/manager';
import type { CheckpointCoordinator } from '../session/checkpoint/coordinator';
import { PermissionManager, type PermissionManagerOptions } from './permission';
import { SessionMode, type RuntimeMode } from './session-mode';
import {
  AgentRecords,
  BlobStore,
  FileSystemAgentRecordPersistence,
  type AgentRecord,
  type AgentRecordPersistence,
} from './records';
import { ReplayBuilder } from './replay';
import { SkillManager } from './skill';
import { ToolManager } from './tool/index';
import { TurnFlow } from './turn';
import {
  GENERATE_REQUEST_LOG_CONTEXT,
  KosongLLM,
  type GenerateOptionsWithRequestLog,
} from './turn/kosong-llm';
import { UsageRecorder } from './usage';
import { resolveCompletionBudget } from '../utils/completion-budget';
import type { Kaos } from '@odysseythink/kaos';
import type { ToolServices } from '../tools/support/services';
import type { ProductStateStore, GameDesignStateStore } from '@odysseythink/agent-core-shared';
import { NoopProductStateStore, NoopGameDesignStateStore } from '@odysseythink/agent-core-shared';
import type { SupportedLanguage } from '#i18n';


export type { RuntimeMode, SessionModeKind } from './session-mode';
export type { AgentRecord, AgentRecordPersistence } from './records';
export type { BuiltinTool, ToolInfo, ToolSource, UserToolRegistration } from './tool';
export { buildGoalCompletionMessage } from './goal/completion';
export type { LLM, LLMFactoryConfig } from '#loop/llm';

export type AgentType = 'main' | 'sub' | 'independent';

export interface AgentOptions {
  readonly kaos: Kaos;
  readonly config?: OdyConfig;
  readonly homedir?: string;
  readonly rpc?: Partial<SDKAgentRPC>;
  readonly persistence?: AgentRecordPersistence;
  readonly type?: AgentType;
  readonly generate?: typeof generate;
  readonly toolServices?: ToolServices;
  readonly compactionStrategy?: CompactionStrategy;
  readonly microCompaction?: Partial<MicroCompactionConfig>;
  readonly modelProvider?: ModelProvider | undefined;
  readonly subagentHost?: SessionSubagentHost | undefined;
  readonly skills?: SkillRegistry;
  readonly mcp?: McpConnectionManager;
  readonly goals?: SessionGoalStore | undefined;
  readonly hookEngine?: HookEngine;
  readonly permission?: PermissionManagerOptions | undefined;
  readonly log?: Logger;
  readonly telemetry?: TelemetryClient | undefined;
  readonly pluginSessionStarts?: readonly EnabledPluginSessionStart[];
  readonly appVersion?: string;
  /** True when this agent is being created as part of resuming an existing session. */
  readonly isResumeSession?: boolean;
  readonly productStateStore?: ProductStateStore;
  readonly gameDesignStateStore?: GameDesignStateStore;
  /** User language restored from Session metadata on resume. */
  readonly userLanguage?: SupportedLanguage | undefined;
  /** Callback for Agent to persist a detected language change back to Session. */
  readonly setUserLanguage?: ((lang: SupportedLanguage) => void) | undefined;
  readonly llmFactory?: ((rpc: Partial<SDKAgentRPC>, config: LLMFactoryConfig) => LLM) | undefined;
}

export class Agent {
  readonly type: AgentType;
  readonly kaos: Kaos;
  kimiConfig?: OdyConfig;
  readonly homedir?: string;
  readonly rpc?: Partial<SDKAgentRPC>;
  readonly toolServices?: ToolServices;
  readonly pluginSessionStarts: readonly EnabledPluginSessionStart[];
  readonly rawGenerate: typeof generate;
  readonly modelProvider?: ModelProvider;
  readonly subagentHost?: SessionSubagentHost;
  readonly mcp?: McpConnectionManager;
  readonly goals?: SessionGoalStore;
  readonly hooks?: HookEngine;
  readonly log: Logger;
  readonly telemetry: TelemetryClient;
  readonly appVersion?: string;
  readonly isResumeSession: boolean;

  readonly blobStore: BlobStore | undefined;
  readonly records: AgentRecords;
  readonly config: ConfigState;

  // Per-mode context partitions. Each mode has its own isolated conversation
  // history, compaction state, and micro-compaction cursor. _activeMode
  // determines which partition agent.context / fullCompaction / microCompaction
  // point to. Changed by setContextMode() as modes are entered and exited.
  private readonly _contexts: Record<RuntimeMode, ContextMemory>;
  private readonly _fullCompactions: Record<RuntimeMode, FullCompaction>;
  private readonly _microCompactions: Record<RuntimeMode, MicroCompaction>;
  /** Detects split-plan/design part boundaries and compacts there when context is
   * over the configured ratio. Current-mode-aware via its agent getters, so one
   * instance serves all partitions. */
  readonly splitPlanCheckpoint: SplitPlanCheckpoint;
  /** Detects TodoList task completion boundaries in normal mode and compacts when
   * context exceeds the configured ratio. Only active when sessionMode is inactive. */
  readonly normalModeTaskCheckpoint: NormalModeTaskCheckpoint;
  private _activeMode: RuntimeMode = 'normal';
  // When setContextMode is called while the current partition has an open step
  // (mid tool-exchange), we defer the switch so the tool.call / tool.result
  // events route to the same partition as step.begin. The flush happens at
  // step.end (via flushDeferredContextSwitch) or at turn end (safety net).
  private _pendingContextSwitch: RuntimeMode | null = null;
  readonly turn: TurnFlow;
  readonly injection: InjectionManager;
  readonly permission: PermissionManager;
  readonly sessionMode: SessionMode;
  readonly usage: UsageRecorder;
  readonly skills: SkillManager | null;
  readonly tools: ToolManager;
  readonly background: BackgroundManager;
  readonly cron: CronManager | null;
  readonly replayBuilder: ReplayBuilder;
  readonly productStateStore!: ProductStateStore;
  readonly gameDesignStateStore!: GameDesignStateStore;
  userLanguage?: SupportedLanguage;
  private readonly _setUserLanguageCallback?: ((lang: SupportedLanguage) => void) | undefined;
  checkpointCoordinator?: CheckpointCoordinator;
  private readonly llmFactory?: ((rpc: Partial<SDKAgentRPC>, config: LLMFactoryConfig) => LLM) | undefined;

  private lastLlmConfigLogSignature?: string;
  private _llm: LLM | undefined;

  constructor(options: AgentOptions) {
    this.type = options.type ?? 'main';
    this.kaos = options.kaos;
    this.kimiConfig = options.config;
    this.homedir = options.homedir;
    this.rpc = options.rpc;
    this.toolServices = options.toolServices;
    this.pluginSessionStarts = options.pluginSessionStarts ?? [];
    this.rawGenerate = options.generate ?? generate;
    this.modelProvider = options.modelProvider;
    this.subagentHost = options.subagentHost;
    this.mcp = options.mcp;
    this.goals = options.goals;
    this.hooks = options.hookEngine;
    this.appVersion = options.appVersion;
    this.isResumeSession = options.isResumeSession ?? false;
    this.log = options.log ?? log;
    this.telemetry = options.telemetry ?? noopTelemetryClient;

    this.blobStore = options.homedir
      ? new BlobStore({ blobsDir: join(options.homedir, 'blobs') })
      : undefined;
    this.records = new AgentRecords(
      this,
      options.persistence ??
        (options.homedir
          ? new FileSystemAgentRecordPersistence(join(options.homedir, 'wire.jsonl'), {
              onError: (error) => {
                this.emitRecordsWriteError(error);
              },
              blobStore: this.blobStore,
            })
          : undefined),
    );
    this._contexts = {
      normal: new ContextMemory(this),
      plan: new ContextMemory(this),
      design: new ContextMemory(this),
      'product': new ContextMemory(this),
      'game-design': new ContextMemory(this),
    } as Record<RuntimeMode, ContextMemory>;
    this._fullCompactions = {
      normal: new FullCompaction(this, options.compactionStrategy),
      plan: new FullCompaction(this, options.compactionStrategy),
      design: new FullCompaction(this, options.compactionStrategy),
      'product': new FullCompaction(this, options.compactionStrategy),
      'game-design': new FullCompaction(this, options.compactionStrategy),
    } as Record<RuntimeMode, FullCompaction>;
    this._microCompactions = {
      normal: new MicroCompaction(this, options.microCompaction),
      plan: new MicroCompaction(this, options.microCompaction),
      design: new MicroCompaction(this, options.microCompaction),
      'product': new MicroCompaction(this, options.microCompaction),
      'game-design': new MicroCompaction(this, options.microCompaction),
    } as Record<RuntimeMode, MicroCompaction>;
    this.splitPlanCheckpoint = new SplitPlanCheckpoint(this);
    this.normalModeTaskCheckpoint = new NormalModeTaskCheckpoint(this);
    this.config = new ConfigState(this);
    this.turn = new TurnFlow(this);
    this.injection = new InjectionManager(this);
    this.permission = new PermissionManager(this, options.permission);
    this.sessionMode = new SessionMode(this);
    this.usage = new UsageRecorder(this);
    this.skills = options.skills ? new SkillManager(this, options.skills) : null;
    this.tools = new ToolManager(this);
    this.background = new BackgroundManager(
      this,
      this.homedir === undefined ? undefined : new BackgroundTaskPersistence(this.homedir),
    );
    this.cron = this.type === 'sub' ? null : new CronManager(this);
    this.replayBuilder = new ReplayBuilder(this);
    this.productStateStore = options.productStateStore ?? new NoopProductStateStore();
    this.gameDesignStateStore = options.gameDesignStateStore ?? new NoopGameDesignStateStore();
    this.userLanguage = options.userLanguage;
    this._setUserLanguageCallback = options.setUserLanguage;
    this.llmFactory = options.llmFactory;

    // Fire-and-forget: load Wasm compute hotspots in the background.
    // globMatch already falls back to JS while Wasm is loading or if it fails,
    // so this never blocks construction and never breaks standalone usage.
    void initGlobWasm().catch(() => {
      /* fallback is automatic */
    });
  }

  /** Active partition's conversation history — routes to the current mode. */
  get context(): ContextMemory {
    return this._contexts[this._activeMode];
  }

  /** Active partition's full-compaction state. */
  get fullCompaction(): FullCompaction {
    return this._fullCompactions[this._activeMode];
  }

  /** Active partition's micro-compaction state. */
  get microCompaction(): MicroCompaction {
    return this._microCompactions[this._activeMode];
  }

  /** All three partition contexts, keyed by mode. Used for bulk operations (e.g. blob rehydration). */
  get contexts(): Readonly<Record<RuntimeMode, ContextMemory>> {
    return this._contexts;
  }

  /** Switch the active context partition. Called by SessionMode on enter/exit/cancel.
   * When the active partition has an open step we are mid tool-exchange (an
   * Enter/Exit mode tool is executing). Defer the switch until step.end so the tool's
   * assistant message, its tool call, AND its tool result all land in the SAME
   * (current) partition — otherwise the result orphans from its call and the
   * provider rejects the next request with "tool_call_id is not found".
   *
   * This applies to EVERY target mode, not just normal: a design→plan handoff
   * (ExitDesignMode → handoffTo('plan') → exit() then enter('plan')) must defer
   * exactly like a plan→normal exit. handoffTo's two setContextMode calls simply
   * update the deferred target; the actual switch happens once at step.end. Entry
   * via a /plan or /design slash command runs outside any tool exchange (no open
   * step) and so still switches immediately. */
  setContextMode(mode: RuntimeMode): void {
    if (this._contexts[this._activeMode].hasOpenSteps()) {
      this._pendingContextSwitch = mode;
      return;
    }
    this._activeMode = mode;
    this.replayBuilder.setMode(mode);
    this._pendingContextSwitch = null;
  }

  /** Apply any pending context partition switch. Called from step.end and as a
   * safety net at turn end so the partition is never stuck after an abort. */
  flushDeferredContextSwitch(): void {
    if (this._pendingContextSwitch !== null) {
      this._activeMode = this._pendingContextSwitch;
      this.replayBuilder.setMode(this._pendingContextSwitch);
      this._pendingContextSwitch = null;
    }
  }

  get generate(): typeof generate {
    return async (provider, systemPrompt, tools, history, callbacks, options) => {
      const modelAlias = this.config.modelAlias;
      this.log?.debug('agent.generate called', {
        modelAlias,
        providerName: provider.name,
        providerModel: provider.modelName,
        hasAuth: options?.auth !== undefined,
      });
      if (options?.auth !== undefined) {
        this.logLlmRequest(provider, systemPrompt, tools, history, options);
        return this.rawGenerate(provider, systemPrompt, tools, history, callbacks, options);
      }
      const withAuth =
        modelAlias === undefined
          ? undefined
          : this.modelProvider?.resolveAuth?.(modelAlias, { log: this.log });
      if (withAuth === undefined) {
        this.logLlmRequest(provider, systemPrompt, tools, history, options);
        return this.rawGenerate(provider, systemPrompt, tools, history, callbacks, options);
      }
      return withAuth((auth) => {
        const requestOptions = { ...options, auth };
        this.logLlmRequest(provider, systemPrompt, tools, history, requestOptions);
        return this.rawGenerate(provider, systemPrompt, tools, history, callbacks, requestOptions);
      });
    };
  }

  /**
   * Invalidates the cached LLM so the next access creates a fresh instance
   * from the current config. Call this when the active model/provider must
   * change mid-turn (e.g. session-mode handoffs) or at turn boundaries.
   */
  refreshLlm(): void {
    this._llm = undefined;
  }

  get llm(): LLM {
    if (this._llm !== undefined) {
      return this._llm;
    }
    const model = this.config.model;
    const systemPrompt = this.config.systemPrompt;
    const loopControl = this.kimiConfig?.loopControl;
    const completionBudgetConfig = resolveCompletionBudget({
      reservedContextSize: loopControl?.reservedContextSize,
    });

    if (this.llmFactory !== undefined) {
      this._llm = this.llmFactory(this.rpc ?? {}, {
        modelName: model,
        systemPrompt,
        capability: this.config.modelCapabilities,
        completionBudgetConfig,
        provider: this.modelProvider !== undefined ? this.config.providerConfig : undefined,
      });
      return this._llm;
    }

    const provider = this.config.provider.withThinking(this.config.thinkingLevel);
    this.log?.debug('agent.llm created', {
      modelAlias: this.config.modelAlias,
      model,
      providerName: provider.name,
      providerModel: provider.modelName,
    });
    this._llm = new KosongLLM({
      provider,
      modelName: model,
      systemPrompt,
      capability: this.config.modelCapabilities,
      generate: this.generate,
      completionBudgetConfig,
    });
    return this._llm;
  }

  private logLlmRequest(
    provider: ChatProvider,
    systemPrompt: string,
    tools: readonly Tool[],
    history: readonly Message[],
    options: Parameters<typeof generate>[5],
  ): void {
    const context = buildLlmRequestContext(options);
    const configMetadata = buildLlmConfigMetadata(
      provider,
      this.config.modelAlias,
      systemPrompt,
      tools,
    );
    this.logLlmConfigIfChanged(
      context,
      configMetadata,
      buildLlmConfigSignature(configMetadata, systemPrompt, tools),
    );

    let partialMessageCount = 0;
    for (const message of history) {
      if (message.partial === true) partialMessageCount += 1;
    }
    const requestMetadata: LlmRequestMetadata = {
      estimatedInputTokens:
        estimateTokens(systemPrompt) +
        estimateTokensForMessages(history) +
        estimateTokensForTools(tools),
    };
    if (partialMessageCount > 0) {
      requestMetadata.partialMessageCount = partialMessageCount;
    }
    this.log.info('llm request', {
      ...context,
      ...requestMetadata,
    });
  }

  private logLlmConfigIfChanged(
    context: LlmRequestContextFields,
    metadata: LlmConfigMetadata,
    signature: string,
  ): void {
    if (signature === this.lastLlmConfigLogSignature) return;
    this.lastLlmConfigLogSignature = signature;
    this.log.info('llm config', {
      ...context,
      ...metadata,
    });
  }

  useProfile(profile: ResolvedAgentProfile, context?: PreparedSystemPromptContext): void {
    const systemPrompt = profile.systemPrompt({
      osEnv: this.kaos.osEnv,
      cwd: this.config.cwd,
      skills: this.skills?.registry,
      cwdListing: context?.cwdListing,
      agentsMd: context?.agentsMd,
      sessionMode: this.sessionMode.isActive ? this.sessionMode.kind : 'normal',
    });
    this.config.update({ profileName: profile.name, systemPrompt });
    this.tools.setActiveTools(profile.tools);
  }

  async resume(): Promise<{ warning?: string }> {
    const result = await this.records.replay();
    await this.background.loadFromDisk();
    await this.background.reconcile();
    await this.cron?.loadFromDisk();
    this.turn.finishResume();
    return result;
  }

  get rpcMethods(): PromisableMethods<AgentAPI> {
    return {
      prompt: (payload) => {
        this.turn.prompt(payload.input);
      },
      steer: (payload) => {
        this.telemetry.track('input_steer', { parts: payload.input.length });
        this.turn.steer(payload.input);
      },
      cancel: (payload) => {
        if (this.turn.hasActiveTurn) {
          this.telemetry.track('cancel', { from: 'streaming' });
        }
        this.turn.cancel(payload.turnId);
      },
      undoHistory: (payload) => {
        this.context.undo(payload.count);
      },
      setThinking: (payload) => {
        const wasEnabled = this.config.thinkingLevel !== 'off';
        this.config.update({ thinkingLevel: payload.level });
        const enabled = this.config.thinkingLevel !== 'off';
        if (enabled !== wasEnabled) {
          this.telemetry.track('thinking_toggle', { enabled });
        }
      },
      setPermission: (payload) => {
        const wasYolo = this.permission.mode === 'yolo';
        const wasAuto = this.permission.mode === 'auto';
        this.permission.setMode(payload.mode);
        const enabled = this.permission.mode === 'yolo';
        if (enabled !== wasYolo) {
          this.telemetry.track('yolo_toggle', { enabled });
        }
        const afkEnabled = this.permission.mode === 'auto';
        if (afkEnabled !== wasAuto) {
          this.telemetry.track('afk_toggle', { enabled: afkEnabled });
        }
      },
      setModel: (payload) => {
        // Validate the alias resolves before recording it so resume / runtime
        // callers fail fast on missing aliases instead of deferring to the
        // next prompt.
        const resolved = this.modelProvider?.resolveProviderConfig(payload.model);
        if (this.config.modelAlias !== payload.model) {
          this.config.update({ modelAlias: payload.model });
          this.telemetry.track('model_switch', { model: payload.model });
        }
        return {
          model: payload.model,
          providerName: resolved?.providerName,
        };
      },
      getModel: () => {
        return this.config.modelAlias ?? '';
      },
      enterPlan: async (payload) => {
        if (
          this.sessionMode.isActive &&
          this.sessionMode.kind !== (payload.kind ?? 'plan')
        ) {
          // No finalize needed — path is resolved lazily on first write
        }
        let sourceAbs: string | undefined;
        if (payload.sourceFilePath !== undefined) {
          sourceAbs = isAbsolute(payload.sourceFilePath)
            ? payload.sourceFilePath
            : join(this.config.cwd, payload.sourceFilePath);
          // Validate BEFORE mutating session mode: a bad path must not leave the
          // session stuck in an empty plan mode. Throws propagate over RPC.
          await this.sessionMode.validatePlanSource(sourceAbs);
        }
        await this.sessionMode.enter(
          undefined,
          undefined,
          undefined,
          payload.kind ?? 'plan',
        );
        if (sourceAbs !== undefined) {
          await this.sessionMode.setWritingPlanSource(sourceAbs);
        }
      },
      cancelPlan: async (payload) => {
        this.sessionMode.cancel(payload.id);
      },
      clearPlan: () => this.sessionMode.clear(),
      beginCompaction: (payload) => {
        this.fullCompaction.begin({ source: 'manual', instruction: payload.instruction });
      },
      cancelCompaction: () => {
        if (this.fullCompaction.isCompacting) {
          this.telemetry.track('cancel', { from: 'compacting' });
        }
        this.fullCompaction.cancel();
      },
      registerTool: (payload) => {
        this.tools.registerUserTool(payload);
      },
      unregisterTool: (payload) => {
        this.tools.unregisterUserTool(payload.name);
      },
      setActiveTools: (payload) => {
        this.tools.setActiveTools(payload.names);
      },
      stopBackground: (payload) => {
        void this.background.stop(payload.taskId, payload.reason);
      },
      clearContext: () => {
        this.context.clear();
        this.splitPlanCheckpoint.reset();
        this.normalModeTaskCheckpoint.reset();
      },
      activateSkill: (payload) => {
        if (this.skills === null) {
          throw new OdyError(ErrorCodes.SKILL_NOT_FOUND, `Skill "${payload.name}" was not found`);
        }
        this.skills.activate(payload);
      },
      getBackgroundOutput: (payload) => this.background.readOutput(payload.taskId, payload.tail),
      getContext: () => this.context.data(),
      getConfig: () => this.config.data(),
      getPermission: () => this.permission.data(),
      getPlan: () => this.sessionMode.data(),
      reviewDesign: async (payload) => {
        let content: string;
        let path: string;
        let kind: 'plan' | 'design';
        if (payload.path !== undefined && payload.path.length > 0) {
          try {
            content = await this.kaos.readText(payload.path);
          } catch {
            throw new OdyError(
              ErrorCodes.SESSION_PLAN_MODE_INVALID,
              `Plan/design file not found or unreadable: ${payload.path}`,
            );
          }
          path = payload.path;
          kind = payload.kind ?? 'design';
        } else {
          const data = await this.sessionMode.data();
          if (data === null || data.content.trim().length === 0) {
            throw new OdyError(
              ErrorCodes.SESSION_PLAN_MODE_INVALID,
              'No plan/design file to review. Enter plan or design mode, or pass a file path.',
            );
          }
          content = data.content;
          path = data.path;
          kind = payload.kind ?? (data.kind === 'product' || data.kind === 'game-design' ? 'design' : data.kind);
        }
        if (content.trim().length === 0) {
          throw new OdyError(ErrorCodes.SESSION_PLAN_MODE_INVALID, `Document is empty: ${path}`);
        }

        // A split plan keeps its tasks in sibling files listed in the index's Parts
        // manifest; gather them so the reviewer attacks the whole plan, not just the
        // index. Single-file plans and designs have no manifest → review as-is.
        let reviewContent = content;
        if (kind === 'plan') {
          // Split parts live in a subdirectory named after the index file's stem
          // (`<dir>/<stem>/<part>.md`), matching the write-permission guard.
          const dir = dirname(path);
          const stem = basename(path).replace(/\.md$/, '');
          for (const file of parseManifestFiles(content)) {
            const siblingPath = join(dir, stem, file);
            if (siblingPath === path) continue;
            try {
              const siblingContent = await this.kaos.readText(siblingPath);
              reviewContent += `\n\n===== FILE: ${file} =====\n\n${siblingContent}`;
            } catch {
              // Skip an unreadable sibling rather than failing the whole review.
            }
          }
        }

        const reviewerAlias =
          payload.modelAlias ??
          this.kimiConfig?.modeModels?.review ??
          this.kimiConfig?.modeModels?.plan ??
          this.kimiConfig?.defaultModel;
        if (reviewerAlias === undefined || reviewerAlias.length === 0) {
          throw new OdyError(
            ErrorCodes.CONFIG_INVALID,
            'No reviewer model configured. Set mode_models.review (or default_model) in config.toml.',
          );
        }

        // Warn when the concatenated content exceeds a rough context-window budget.
        // 300 K characters ≈ 75–100 K tokens — most reviewer models will truncate
        // or error above this. We surface a clear message rather than letting the
        // reviewer silently fail on an oversized payload.
        const REVIEW_CONTENT_WARN_CHARS = 300_000;
        if (reviewContent.length > REVIEW_CONTENT_WARN_CHARS) {
          this.log?.warn(
            `plan-review: combined plan content is ${reviewContent.length} chars (>${REVIEW_CONTENT_WARN_CHARS}); ` +
              'the reviewer model may truncate or time out. Consider reviewing index + parts separately.',
          );
        }

        const defaultTimeoutMs = 120_000;
        const result = await new AdvancedSessionReviewer(this, {
          reviewerAlias,
          kind,
          timeoutMs: payload.timeoutMs ?? defaultTimeoutMs,
        }).review(reviewContent);
        return {
          path,
          auditLevel: result.auditLevel,
          reviewerAlias,
          ok: result.ok,
          ...(result.note !== undefined ? { note: result.note } : {}),
          findings: result.findings.map((finding) => ({
            ...finding,
            escalate: shouldEscalate(finding.severity, finding.confidence, result.auditLevel),
          })),
        };
      },
      getUsage: () => this.usage.data(),
      getTools: () => this.tools.data(),
      getBackground: (payload) => this.background.list(payload.activeOnly ?? false, payload.limit),
      getUserLanguage: () => this.userLanguage,
    };
  }

  emitEvent(event: AgentEvent): void {
    if (this.records.restoring) return;
    void this.rpc?.emitEvent?.(event);
  }

  setUserLanguage(lang: SupportedLanguage): void {
    this.userLanguage = lang;
    try {
      this._setUserLanguageCallback?.(lang);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.log.warn('failed to persist user language', { error: message });
    }
    this.emitStatusUpdated();
  }

  emitStatusUpdated(): void {
    if (this.records.restoring) return;
    if (!this.config.hasModel) return;

    const contextTokens = this.context.tokenCount;
    const maxContextTokens = this.config.modelCapabilities.max_context_tokens;
    const contextUsage =
      maxContextTokens !== undefined && maxContextTokens > 0
        ? contextTokens / maxContextTokens
        : undefined;
    const usage: UsageStatus | undefined = this.usage.status();
    const model = this.config.model;

    this.emitEvent({
      type: 'agent.status.updated',
      model,
      contextTokens,
      maxContextTokens,
      contextUsage,
      sessionMode: this.sessionMode.isActive ? this.sessionMode.kind : 'normal',
      sessionModeFilePath: this.sessionMode.sessionModeFilePath,
      permission: this.permission.mode,
      usage,
      userLanguage: this.userLanguage,
    });
  }

  private emitRecordsWriteError(error: unknown, record?: AgentRecord | undefined): void {
    const message = error instanceof Error ? error.message : String(error);
    this.log.error('wire record persist failed', {
      agentHomedir: this.homedir,
      recordType: record?.type,
      error,
    });
    this.emitEvent({
      type: 'error',
      ...makeErrorPayload(
        ErrorCodes.RECORDS_WRITE_FAILED,
        `Failed to write agent records: ${message}`,
        {
          details: { recordType: record?.type },
        },
      ),
    });
  }
}

export namespace Agent {
  export type RuntimeMode = import('./session-mode').RuntimeMode;
}

interface LlmRequestContextFields {
  turnStep?: string;
  attempt?: string;
}

interface LlmRequestMetadata {
  estimatedInputTokens: number;
  partialMessageCount?: number;
}

/**
 * Fields that identify an LLM configuration for deduplication.
 * Keep this interface simple and avoid dynamic keys — the shape is
 * serialized with `JSON.stringify` to produce a stable signature in
 * `logLlmConfigIfChanged`.
 */
interface LlmConfigMetadata {
  provider: string;
  model: string;
  modelAlias?: string;
  thinkingEffort?: string;
  systemPromptChars: number;
  toolCount: number;
}

function buildLlmRequestContext(options: Parameters<typeof generate>[5]): LlmRequestContextFields {
  const context = requestLogContext(options);
  if (context === undefined) return {};

  const fields: LlmRequestContextFields = {
    turnStep:
      context.turnId === undefined || context.step === undefined
        ? undefined
        : `${context.turnId}.${String(context.step)}`,
  };
  if (
    context.attempt !== undefined &&
    context.maxAttempts !== undefined &&
    context.attempt > 1
  ) {
    fields.attempt = `${String(context.attempt)}/${String(context.maxAttempts)}`;
  }
  return fields;
}

function buildLlmConfigMetadata(
  provider: ChatProvider,
  modelAlias: string | undefined,
  systemPrompt: string,
  tools: readonly Tool[],
): LlmConfigMetadata {
  return {
    provider: provider.name,
    model: provider.modelName,
    modelAlias,
    thinkingEffort: provider.thinkingEffort ?? undefined,
    systemPromptChars: systemPrompt.length,
    toolCount: tools.length,
  };
}

function buildLlmConfigSignature(
  metadata: LlmConfigMetadata,
  systemPrompt: string,
  tools: readonly Tool[],
): string {
  const toolsForSignature = tools.map(({ name, description, parameters }) => ({
    name,
    description,
    parameters,
  }));
  return JSON.stringify({
    ...metadata,
    systemPromptHash: fingerprint(systemPrompt),
    toolsHash: fingerprint(JSON.stringify(toolsForSignature)),
  });
}

function fingerprint(content: string): string {
  return createHash('sha256').update(content).digest('hex');
}

function requestLogContext(options: Parameters<typeof generate>[5]) {
  return (options as GenerateOptionsWithRequestLog | undefined)?.[GENERATE_REQUEST_LOG_CONTEXT];
}

export { RemoteKosongLLM, remoteLLMStreamRegistry } from './turn/remote-kosong-llm';
