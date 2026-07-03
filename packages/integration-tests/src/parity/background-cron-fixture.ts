import type { ContentPart, FinishReason, TokenUsage, ToolCall } from '@odysseythink/kosong';

export interface BackgroundCronFixture {
  readonly name: string;
  readonly responses: readonly FixtureResponse[];
  readonly actions: readonly BackgroundCronAction[];
}

export interface FixtureResponse {
  readonly toolCalls: readonly ToolCall[];
  readonly finishReason?: string | undefined;
  readonly rawFinishReason?: string | undefined;
  readonly usage: TokenUsage;
}

export type FixtureOrigin =
  | { readonly kind: 'user' }
  | { readonly kind: 'system_trigger'; readonly name: string }
  | { readonly kind: 'hook_result'; readonly event: string; readonly blocked?: boolean | undefined };

export type BackgroundCronAction =
  | { readonly op: 'prompt'; readonly input: readonly ContentPart[]; readonly origin: FixtureOrigin }
  | { readonly op: 'steer'; readonly input: readonly ContentPart[]; readonly origin: FixtureOrigin }
  | { readonly op: 'cancel'; readonly turnId?: number | undefined; readonly reason?: string | undefined }
  | { readonly op: 'wait' }
  | { readonly op: 'advance_clock_to'; readonly epoch_ms: number }
  | { readonly op: 'cron_add'; readonly cron: string; readonly prompt: string; readonly recurring?: boolean | undefined }
  | { readonly op: 'cron_remove_last' }
  | { readonly op: 'cron_tick' }
  | { readonly op: 'background_run_process'; readonly args: readonly string[]; readonly description: string }
  | { readonly op: 'background_wait_last'; readonly timeout_ms: number }
  | { readonly op: 'background_stop_last'; readonly reason?: string | undefined };

export interface BackgroundCronSnapshot {
  readonly name: string;
  readonly turns: readonly TurnSummary[];
  readonly events: readonly unknown[];
  readonly records: readonly unknown[];
  readonly contextInputs: readonly ContextInputSummary[];
  readonly cronTasks: readonly CronTaskSummary[];
  readonly backgroundTasks: readonly BackgroundTaskSummary[];
  readonly telemetry: readonly TelemetrySummary[];
}

export interface TurnSummary {
  readonly turnId: number;
  readonly reason: string;
  readonly error?: unknown;
  readonly stopReason?: string | undefined;
  readonly blockedByUserPromptHook?: boolean | undefined;
}

export interface ContextInputSummary {
  readonly text: string;
  readonly originKind: string;
}

export interface TelemetrySummary {
  readonly event: string;
  readonly properties: unknown;
}

export interface CronTaskSummary {
  readonly id: string;
  readonly cron: string;
  readonly prompt: string;
  readonly recurring: boolean;
  readonly createdAt: number;
  readonly lastFiredAt?: number | undefined;
}

export interface BackgroundTaskSummary {
  readonly taskId: string;
  readonly kind: string;
  readonly description: string;
  readonly status: string;
  readonly startedAt: number;
  readonly endedAt?: number | undefined;
  readonly stopReason?: string | undefined;
}
