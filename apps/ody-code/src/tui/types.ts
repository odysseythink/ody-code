import type {
  GoalSnapshot,
  ModelAlias,
  PermissionMode,
  ProviderConfig,
  PromptPart,
  RuntimeMode,
  ToolInputDisplay,
} from '@odysseythink/ody-code-sdk';

import type {
  AuthManagedUsageResult,
  AuthStatus,
  BearerTokenProvider,
  FetchSubmitFeedbackResult,
} from '@odysseythink/kimi-code-oauth';

import type { ExperimentalFlagMap } from '@odysseythink/agent-core';

import type { NotificationsConfig, UpgradePreferences } from './config';
import type { PendingApproval, PendingQuestion } from './reverse-rpc/types';
import type { Theme } from './theme';
import type { ResolvedTheme } from './theme/colors';

export interface AppState {
  model: string;
  workDir: string;
  sessionId: string;
  permissionMode: PermissionMode;
  sessionMode: RuntimeMode;
  sessionModeFilePath?: string | null;
  thinking: boolean;
  contextUsage: number;
  contextTokens: number;
  maxContextTokens: number;
  isCompacting: boolean;
  isReplaying: boolean;
  streamingPhase: 'idle' | 'waiting' | 'thinking' | 'composing';
  streamingStartTime: number;
  theme: Theme;
  version: string;
  editorCommand: string | null;
  notifications: NotificationsConfig;
  upgrade: UpgradePreferences;
  availableModels: Record<string, ModelAlias>;
  availableProviders: Record<string, ProviderConfig>;
  sessionTitle: string | null;
  /** Current goal snapshot for the footer badge; null/undefined when no active goal. */
  goal?: GoalSnapshot | null;
  mcpServersSummary: string | null;
  /** User language detected in product mode. */
  userLanguage?: 'en' | 'zh' | undefined;
  /** /review 接收反馈分支的模型切换状态。当 active 时，下一条普通消息前恢复原模型。 */
  receiveCodeReview?: {
    originalModelAlias: string;
    reviewModelAlias: string;
    active: boolean;
  };
}

export interface ToolCallBlockData {
  id: string;
  name: string;
  args: Record<string, unknown>;
  description?: string;
  display?: ToolInputDisplay;
  streamingArguments?: string;
  streamingStartedAtMs?: number;
  result?: ToolResultBlockData;
  subagent?: SubagentReplayBlockData;
  step?: number;
  turnId?: string;
  /** Set when the step ended (e.g. max_tokens) before the tool call's
   *  arguments finished streaming. Renderer flips the header verb to
   *  "Truncated" and stops showing the in-progress argument preview. */
  truncated?: boolean;
}

export interface ToolResultBlockData {
  tool_call_id: string;
  output: string;
  is_error?: boolean;
  synthetic?: boolean;
}

export interface SubagentReplayToolCallData {
  id: string;
  name: string;
  args: Record<string, unknown>;
  description?: string;
  result?: ToolResultBlockData;
}

export interface SubagentReplayBlockData {
  id: string;
  name?: string;
  text?: string;
  toolCalls?: readonly SubagentReplayToolCallData[];
}

export interface BackgroundAgentMetadata {
  readonly agentId: string;
  readonly parentToolCallId: string;
  readonly agentName?: string;
  readonly description?: string;
}

export type BackgroundAgentStatusPhase = 'started' | 'completed' | 'failed';

export interface BackgroundAgentStatusData {
  readonly phase: BackgroundAgentStatusPhase;
  readonly headline: string;
  readonly detail?: string;
}

export interface CompactionTranscriptData {
  readonly tokensBefore?: number;
  readonly tokensAfter?: number;
  readonly instruction?: string;
}

export interface CronTranscriptData {
  readonly jobId?: string;
  readonly cron?: string;
  readonly recurring?: boolean;
  readonly coalescedCount?: number;
  readonly stale?: boolean;
  readonly missedCount?: number;
}

export type TranscriptEntryKind =
  | 'welcome'
  | 'user'
  | 'assistant'
  | 'tool_call'
  | 'thinking'
  | 'status'
  | 'skill_activation'
  | 'cron';

export type SkillActivationTrigger = 'user-slash' | 'model-tool' | 'nested-skill';

export interface TranscriptEntry {
  id: string;
  kind: TranscriptEntryKind;
  turnId?: string;
  renderMode: 'markdown' | 'plain' | 'notice';
  content: string;
  color?: string;
  detail?: string;
  toolCallData?: ToolCallBlockData;
  backgroundAgentStatus?: BackgroundAgentStatusData;
  compactionData?: CompactionTranscriptData;
  cronData?: CronTranscriptData;
  imageAttachmentIds?: readonly number[];
  skillActivationId?: string;
  skillName?: string;
  skillArgs?: string;
  skillTrigger?: SkillActivationTrigger;
}

export type LivePaneMode =
  | 'idle'
  | 'waiting'
  | 'thinking'
  | 'tool'
  | 'session';

export interface LivePaneState {
  mode: LivePaneMode;
  pendingApproval: PendingApproval | null;
  pendingQuestion: PendingQuestion | null;
}

export interface QueuedMessage {
  readonly text: string;
  readonly agentId?: string;
  readonly parts?: readonly PromptPart[];
  readonly imageAttachmentIds?: readonly number[];
}

export const INITIAL_LIVE_PANE: LivePaneState = {
  mode: 'idle',
  pendingApproval: null,
  pendingQuestion: null,
};

// ---------------------------------------------------------------------------
// TUI startup / options types (extracted from ody-tui.ts)
// ---------------------------------------------------------------------------

export interface TUIStartupOptions {
  readonly sessionFlag?: string;
  readonly continueLast: boolean;
  readonly yolo: boolean;
  readonly auto: boolean;
  readonly sessionMode: 'normal' | 'plan' | 'design' | 'product' | 'game-design';
  readonly product: boolean;
  readonly gameDesign: boolean;
  readonly model?: string;
  readonly startupNotice?: string;
  readonly authIntent?: { readonly kind: 'login' | 'logout'; readonly providerType: string };
}

export type TUIStartupState = 'pending' | 'ready' | 'picker';

export interface OdyTUIOptions {
  initialAppState: AppState;
  startup: TUIStartupOptions;
  resolvedTheme?: ResolvedTheme;
  /** If true, bypass the interactive terminal UI (smoke-test mode). */
  smokeTest?: boolean;
}

export interface PendingExit {
  readonly kind: 'ctrl-c' | 'ctrl-d';
  readonly timer: ReturnType<typeof setTimeout>;
}

export interface LoginProgressSpinnerHandle {
  updateLabel(label: string): void;
  stop(opts: { ok: boolean; label: string }): void;
}

export type ProgressSpinnerHandle = LoginProgressSpinnerHandle;

// ---------------------------------------------------------------------------
// OdyHarness — abstract host interface for OdyTUI
// ---------------------------------------------------------------------------

export interface OdyHarness {
  readonly homeDir: string;
  readonly configPath: string;
  interactiveAgentId: string;

  track(event: string, properties?: import('@odysseythink/agent-core').TelemetryProperties): void;
  setTelemetryContext(patch: import('@odysseythink/agent-core').TelemetryContextPatch): void;

  ensureConfigFile(): Promise<void>;
  getConfig(options?: import('@odysseythink/ody-code-sdk').GetConfigOptions): Promise<import('@odysseythink/ody-code-sdk').OdyConfig>;
  setConfig(patch: import('@odysseythink/ody-code-sdk').OdyConfigPatch): Promise<import('@odysseythink/ody-code-sdk').OdyConfig>;
  removeProvider(providerId: string): Promise<import('@odysseythink/ody-code-sdk').OdyConfig>;
  getExperimentalFlags(): Promise<ExperimentalFlagMap>;

  createSession(options: import('@odysseythink/ody-code-sdk').CreateSessionOptions & { sessionMode?: string }): Promise<import('@odysseythink/ody-code-sdk').Session>;
  resumeSession(input: { readonly id: string }): Promise<import('@odysseythink/ody-code-sdk').Session>;
  listSessions(options?: import('@odysseythink/ody-code-sdk').ListSessionsOptions): Promise<readonly import('@odysseythink/ody-code-sdk').SessionSummary[]>;
  closeSession(id: string): Promise<void>;

  renameSession(input: import('@odysseythink/ody-code-sdk').RenameSessionInput): Promise<void>;
  forkSession(input: import('@odysseythink/ody-code-sdk').ForkSessionInput): Promise<import('@odysseythink/ody-code-sdk').Session>;
  exportSession(input: import('@odysseythink/ody-code-sdk').ExportSessionInput): Promise<import('@odysseythink/ody-code-sdk').ExportSessionResult>;

  requestCodeReview(
    input: {
      readonly source:
        | { readonly kind: 'commits'; readonly base: string; readonly head: string }
        | { readonly kind: 'pr'; readonly prUrlOrNumber: string }
        | { readonly kind: 'working-tree' };
      readonly modelAlias?: string | undefined;
      readonly description?: string | undefined;
      readonly requirements?: string | undefined;
      readonly deep?: boolean | undefined;
      readonly timeoutMs?: number | undefined;
      readonly workDir?: string | undefined;
      readonly focus?: 'correctness' | 'simplicity' | undefined;
      readonly scope?: 'diff' | 'repo' | undefined;
    },
    options?: {
      readonly signal?: AbortSignal | undefined;
      readonly onProgress?: (progress: { requestId: string; stage: string; modelAlias: string; detail?: string; meta?: { estimatedTokens?: number; filePath?: string; fileCount?: number } }) => void;
    },
  ): Promise<import('@odysseythink/ody-code-sdk').CodeReviewReport>;

  close(): Promise<void>;

  readonly auth: {
    status(providerName?: string | undefined): Promise<AuthStatus>;
    login(
      providerName?: string | undefined,
      options?: import('@odysseythink/kimi-code-oauth').KimiOAuthLoginOptions | undefined,
    ): Promise<import('@odysseythink/ody-code-sdk').KimiAuthLoginResult>;
    logout(providerName?: string | undefined): Promise<import('@odysseythink/ody-code-sdk').KimiAuthLogoutResult>;
    submitFeedback(
      input: import('@odysseythink/ody-code-sdk').KimiAuthSubmitFeedbackInput,
      providerName?: string | undefined,
    ): Promise<FetchSubmitFeedbackResult>;
    getManagedUsage(providerName?: string | undefined): Promise<AuthManagedUsageResult>;
    resolveOAuthTokenProvider(providerName: string, oauthRef: unknown): BearerTokenProvider;
  };
}
