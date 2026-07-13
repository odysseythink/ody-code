import type { ContentPart } from '@odysseythink/kosong';

import { type HookEventType, type HookProfile } from '@odysseythink/agent-core-shared';

export { HOOK_EVENT_TYPES, type HookEventType } from '@odysseythink/agent-core-shared';
export { HOOK_PROFILES, type HookProfile } from '@odysseythink/agent-core-shared';

export interface HookDef {
  readonly event: HookEventType;
  readonly matcher?: string;
  readonly command?: string;
  readonly builtin?: string;
  readonly commands?: readonly string[];
  readonly id?: string;
  readonly profiles?: readonly HookProfile[];
  readonly timeout?: number;
}

export interface HookResult {
  readonly action: 'allow' | 'block';
  readonly message?: string;
  readonly reason?: string;
  readonly stdout?: string;
  readonly stderr?: string;
  readonly exitCode?: number;
  readonly timedOut?: boolean;
  readonly structuredOutput?: boolean;
  readonly errorKind?: 'spawn' | 'timeout' | 'exit' | 'parse' | 'abort';
}

export interface HookExecutionRecord {
  readonly ts: number;
  readonly event: string;
  readonly hookId: string;
  readonly kind: 'command' | 'builtin';
  readonly action: 'allow' | 'block' | 'error' | 'timeout' | 'skipped-profile' | 'dropped';
  readonly durationMs: number;
  readonly reason?: string;
  readonly stdout?: string;
}

export interface HookBlockDecision {
  readonly block: true;
  readonly reason: string;
}

export type HookMatcherValue = string | readonly ContentPart[];

export interface HookEngineTriggerArgs {
  readonly matcherValue?: HookMatcherValue;
  readonly inputData?: Record<string, unknown>;
  readonly signal?: AbortSignal;
}

export type HookTriggeredCallback = (event: string, target: string, count: number) => void;

export type HookResolvedCallback = (
  event: string,
  target: string,
  action: string,
  reason: string | undefined,
  durationMs: number,
) => void;

export interface BuiltinHook {
  readonly id: string;
  run(
    input: Record<string, unknown>,
    ctx: {
      readonly cwd: string | undefined;
      readonly env: Readonly<Record<string, string | undefined>>;
      readonly signal?: AbortSignal;
      readonly timeout: number;
    },
  ): Promise<HookResult>;
}

export interface BuiltinHookRegistry {
  get(id: string): BuiltinHook | undefined;
  ids(): readonly string[];
}

export interface HooksInfo {
  readonly profile: HookProfile;
  readonly disabled: readonly string[];
  readonly summary: Record<string, number>;
  readonly executions: readonly HookExecutionRecord[];
  readonly counts: Record<HookExecutionRecord['action'], number>;
}

export interface HookEngineOptions {
  readonly cwd?: string;
  readonly sessionId?: string;
  readonly env?: Readonly<Record<string, string | undefined>>;
  readonly builtins?: BuiltinHookRegistry;
  readonly onTriggered?: HookTriggeredCallback;
  readonly onResolved?: HookResolvedCallback;
}
