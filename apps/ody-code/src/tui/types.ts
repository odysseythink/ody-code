import type {
  GoalSnapshot,
  ModelAlias,
  PermissionMode,
  ProviderConfig,
  PromptPart,
  RuntimeMode,
  ToolInputDisplay,
} from '@odysseythink/ody-code-sdk';

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
  /** User language detected in office-hours mode. */
  userLanguage?: 'en' | 'zh' | undefined;
  /** /receive-code-review 的模型切换状态。当 active 时，下一条普通消息前恢复原模型。 */
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
  readonly sessionMode: 'normal' | 'plan' | 'design' | 'office-hours' | 'game-design';
  readonly officeHours: boolean;
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
// Harness abstraction for OdyTUI (supports both in-proc KimiHarness and external Rust host)
// ---------------------------------------------------------------------------

import type {
  CreateSessionOptions,
  ExportSessionInput,
  ExportSessionResult,
  ForkSessionInput,
  GetConfigOptions,
  ListSessionsOptions,
  OdyConfig,
  OdyConfigPatch,
  RenameSessionInput,
  SessionSummary,
} from '@odysseythink/ody-code-sdk';
import type { ExperimentalFlagMap } from '@odysseythink/agent-core';

type TelemetryContextPatch = import('@odysseythink/agent-core').TelemetryContextPatch;

export interface OdyHarness {
  readonly homeDir: string;
  readonly configPath: string;
  interactiveAgentId: string;

  track(event: string, properties?: Record<string, unknown>): void;
  setTelemetryContext(patch: TelemetryContextPatch): void;

  ensureConfigFile(): Promise<void>;
  getConfig(options?: GetConfigOptions): Promise<OdyConfig>;
  setConfig(patch: OdyConfigPatch): Promise<OdyConfig>;
  removeProvider(providerId: string): Promise<OdyConfig>;
  getExperimentalFlags(): Promise<ExperimentalFlagMap>;

  createSession(options: CreateSessionOptions): Promise<import('@odysseythink/ody-code-sdk').Session>;
  resumeSession(input: { readonly id: string }): Promise<import('@odysseythink/ody-code-sdk').Session>;
  forkSession(input: ForkSessionInput): Promise<import('@odysseythink/ody-code-sdk').Session>;
  listSessions(options?: ListSessionsOptions): Promise<readonly SessionSummary[]>;
  renameSession(input: RenameSessionInput): Promise<void>;
  exportSession(input: ExportSessionInput): Promise<ExportSessionResult>;
  closeSession(id: string): Promise<void>;

  requestCodeReview(
    input: Record<string, unknown>,
    options?: Record<string, unknown>,
  ): Promise<any>;

  close(): Promise<void>;

  readonly auth: {
    resolveOAuthTokenProvider(providerName: string, oauthRef?: unknown): unknown;
    status(providerName?: string): Promise<any>;
    login(providerName?: string, options?: any): Promise<any>;
    logout(providerName?: string): Promise<any>;
    submitFeedback(input: any, providerName?: string): Promise<any>;
    getManagedUsage(providerName?: string): Promise<any>;
    getCachedAccessToken(providerName?: string): Promise<string | undefined>;
  };
}
