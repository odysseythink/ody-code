import type {
  ExportSessionManifest,
  ResumeSessionResult,
  RuntimeMode,
  ShellEnvironment,
  TelemetryClient,
  TelemetryContextPatch,
  TelemetryProperties,
} from '@odysseythink/agent-core';
import type { KimiHostIdentity, OAuthRefreshOutcome } from '@odysseythink/kimi-code-oauth';
import type { ContentPart } from '@odysseythink/kosong';

export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { readonly [key: string]: JsonValue };
export type JsonObject = { readonly [key: string]: JsonValue };

export type Unsubscribe = () => void;

export type {
  AgentReplayRecord,
  AgentBackgroundTaskInfo,
  BackgroundConfig,
  BackgroundTaskInfo,
  BackgroundTaskStatus,
  ContextMessage,
  CreateGoalInput,
  DesignReviewData,
  ExportSessionManifest,
  GoalBudgetLimits,
  GoalBudgetReport,
  GoalChange,
  GoalChangeStats,
  GoalSnapshot,
  GoalStatus,
  GoalToolResult,
  OdyConfig,
  OdyConfigPatch,
  LoopControl,
  McpServerInfo,
  McpStartupMetrics,
  ModelAlias,
  MoonshotServiceConfig,
  OAuthRef,
  PluginGithubMetadata,
  PluginGithubRef,
  PluginInfo,
  PluginMcpServerInfo,
  PluginSource,
  PluginSummary,
  ProcessBackgroundTaskInfo,
  PromptOrigin,
  ProviderConfig,
  ProviderType,
  QuestionBackgroundTaskInfo,
  ReloadSummary,
  CodeReviewReportData,
  CodeReviewFindingData,
  RequestCodeReviewPayload,
  ReviewFindingData,
  ResumedAgentState,
  RuntimeMode,
  ServicesConfig,
  ShellEnvironment,
  SkillSummary,
  ThinkingConfig,
  ToolInfo,
} from '@odysseythink/agent-core';

export type {
  CodeReviewDiffSource,
  CodeReviewRequestInput,
  CodeReviewReport,
  CodeReviewFinding,
  CodeReviewProgress,
  CodeReviewProgressStage,
} from '@odysseythink/code-review';

export type { KimiHostIdentity, OAuthRefreshOutcome };
export type { TelemetryClient, TelemetryContextPatch, TelemetryProperties };
export type { ContentPart, Role, ToolCall } from '@odysseythink/kosong';

export type PermissionMode = 'yolo' | 'manual' | 'auto';

export type TextPromptPart = Extract<ContentPart, { type: 'text' }>;
export type PromptPart = Extract<ContentPart, { type: 'text' | 'image_url' | 'video_url' }>;

export type PromptInput = readonly PromptPart[];

export interface KimiHarnessOptions {
  readonly identity?: KimiHostIdentity | undefined;
  readonly homeDir?: string | undefined;
  readonly configPath?: string | undefined;
  readonly autoLoadConfig?: boolean | undefined;
  readonly uiMode?: string;
  readonly skillDirs?: readonly string[];
  readonly telemetry?: TelemetryClient | undefined;
  readonly onOAuthRefresh?: ((outcome: OAuthRefreshOutcome) => void) | undefined;
}

export interface CreateSessionOptions {
  readonly id?: string | undefined;
  readonly workDir: string;
  readonly model?: string | undefined;
  readonly thinking?: string | undefined;
  readonly permission?: PermissionMode | undefined;
  readonly sessionMode?: RuntimeMode;
  readonly metadata?: JsonObject | undefined;
}

export interface RenameSessionInput {
  readonly id: string;
  readonly title: string;
}

export interface ResumeSessionInput {
  readonly id: string;
}

export interface ForkSessionInput {
  readonly id: string;
  readonly forkId?: string;
  readonly title?: string;
  readonly metadata?: JsonObject;
}

export interface ExportSessionInput {
  readonly id: string;
  readonly outputPath?: string | undefined;
  readonly includeGlobalLog?: boolean | undefined;
  /** Host version to record in the export manifest. */
  readonly version: string;
  /** How the CLI was installed (e.g. 'npm-global', 'native'). */
  readonly installSource?: string | undefined;
  readonly shellEnv?: ShellEnvironment | undefined;
}

export interface ExportSessionResult {
  readonly zipPath: string;
  readonly entries: readonly string[];
  readonly sessionDir: string;
  readonly manifest: ExportSessionManifest;
}

export interface ListSessionsOptions {
  readonly workDir?: string;
  readonly sessionId?: string;
}

export interface GetConfigOptions {
  readonly reload?: boolean | undefined;
}

export interface CompactOptions {
  readonly instruction?: string | undefined;
}

export interface PlanInfo {
  readonly id: string;
  readonly content: string;
  readonly path: string;
}

export type SessionPlan = PlanInfo | null;

export interface TokenUsage {
  readonly inputOther: number;
  readonly output: number;
  readonly inputCacheRead: number;
  readonly inputCacheCreation: number;
}

export interface SessionUsage {
  readonly byModel?: Record<string, TokenUsage> | undefined;
  readonly currentTurn?: TokenUsage | undefined;
  readonly total?: TokenUsage | undefined;
}

export interface SessionStatus {
  readonly model?: string;
  readonly thinkingLevel: string;
  readonly permission: PermissionMode;
  readonly sessionMode: 'plan' | 'design' | 'office-hours' | 'game-design' | 'normal';
  readonly sessionModeFilePath?: string | null;
  readonly contextTokens: number;
  readonly maxContextTokens: number;
  readonly contextUsage: number;
  readonly usage?: SessionUsage;
  readonly userLanguage?: 'en' | 'zh' | undefined;
}

export interface SessionSummary {
  readonly id: string;
  readonly title?: string | undefined;
  readonly lastPrompt?: string;
  readonly workDir: string;
  readonly sessionDir: string;
  readonly createdAt: number;
  readonly updatedAt: number;
  readonly archived?: boolean | undefined;
  readonly metadata?: JsonObject | undefined;
}

export type ResumedSessionState = Pick<ResumeSessionResult, 'sessionMetadata' | 'agents' | 'warning'>;

export interface ResumedSessionSummary extends SessionSummary, ResumedSessionState {}
