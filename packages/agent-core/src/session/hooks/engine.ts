import { runHook } from './runner';
import { createProfileGate, type ProfileGate } from './profile-gate';
import type {
  HookBlockDecision,
  HookDef,
  HookEngineOptions,
  HookEngineTriggerArgs,
  HookExecutionRecord,
  HookMatcherValue,
  HookProfile,
  HookResult,
} from './types';

const DEFAULT_HOOK_TIMEOUT_SECONDS = 30;
const EXECUTION_LOG_MAX = 200;

export type { BuiltinHook, BuiltinHookRegistry } from './types';

export class HookEngine {
  private readonly byEvent = new Map<string, HookDef[]>();
  private readonly pendingTriggers = new Set<Promise<HookResult[]>>();
  private readonly runningTriggers = new Set<Promise<unknown>>();
  private readonly executionLog: HookExecutionRecord[] = [];
  private readonly profileGate: ProfileGate;

  constructor(
    hooks: readonly HookDef[] = [],
    private readonly options: HookEngineOptions = {},
  ) {
    this.profileGate = createProfileGate(options.env);
    for (const hook of hooks) {
      if (hook.builtin !== undefined) {
        const builtin = this.options.builtins?.get(hook.builtin);
        if (builtin === undefined) {
          throw new Error(`Unknown builtin hook: ${hook.builtin}`);
        }
      }
      const entries = this.byEvent.get(hook.event) ?? [];
      entries.push(hook);
      this.byEvent.set(hook.event, entries);
    }
  }

  get summary(): Record<string, number> {
    const result: Record<string, number> = {};
    for (const [event, hooks] of this.byEvent.entries()) {
      result[event] = hooks.length;
    }
    return result;
  }

  currentProfile(): HookProfile {
    return this.profileGate.profile;
  }

  disabledHooks(): readonly string[] {
    return Array.from(this.profileGate.disabled);
  }

  executions(): readonly HookExecutionRecord[] {
    return [...this.executionLog];
  }

  trigger(event: string, args: HookEngineTriggerArgs = {}): Promise<HookResult[]> {
    let promise: Promise<HookResult[]>;
    try {
      promise = this.triggerInner(event, args).catch((): HookResult[] => []);
    } catch {
      return Promise.resolve([]);
    }
    this.runningTriggers.add(promise);
    void promise.finally(() => {
      this.runningTriggers.delete(promise);
    });
    return promise;
  }

  async triggerBlock(
    event: string,
    args: HookEngineTriggerArgs = {},
  ): Promise<HookBlockDecision | undefined> {
    return blockDecision(event, await this.trigger(event, args));
  }

  fireAndForgetTrigger(
    event: string,
    args: HookEngineTriggerArgs = {},
  ): Promise<HookResult[]> {
    let promise: Promise<HookResult[]>;
    try {
      promise = this.trigger(event, args).catch((): HookResult[] => []);
    } catch {
      promise = Promise.resolve([]);
    }
    this.pendingTriggers.add(promise);
    void promise.finally(() => {
      this.pendingTriggers.delete(promise);
    });
    return promise;
  }

  async drain(timeoutMs = 5000): Promise<void> {
    const pending = Array.from(this.runningTriggers);
    if (pending.length === 0) return;

    const timer = new Promise<void>((resolve) => setTimeout(resolve, timeoutMs));
    await Promise.race([Promise.allSettled(pending), timer]);

    for (const promise of pending) {
      if (this.runningTriggers.has(promise as Promise<unknown>)) {
        this.appendExecutionRecord({
          ts: Date.now(),
          event: 'drain',
          hookId: '',
          kind: 'builtin',
          action: 'dropped',
          durationMs: timeoutMs,
          reason: `hook did not finish within drain timeout ${timeoutMs}ms`,
        });
      }
    }
  }

  private async triggerInner(
    event: string,
    args: HookEngineTriggerArgs,
  ): Promise<HookResult[]> {
    const matcherValue = matcherValueText(args.matcherValue);
    const inputData = toHookInputData({
      hookEventName: event,
      sessionId: this.options.sessionId ?? '',
      cwd: this.options.cwd ?? '',
      ...args.inputData,
    });
    const matched = this.matchingHooks(event, matcherValue);

    const enabled: HookDef[] = [];
    for (const hook of matched) {
      const hookId = hookIdOf(hook);
      if (!this.profileGate.isEnabled(hook)) {
        this.appendExecutionRecord({
          ts: Date.now(),
          event,
          hookId,
          kind: hook.builtin !== undefined ? 'builtin' : 'command',
          action: 'skipped-profile',
          durationMs: 0,
          reason: `profile=${this.profileGate.profile}`,
        });
        continue;
      }
      enabled.push(hook);
    }

    if (enabled.length === 0) return [];

    this.emitTriggered(event, matcherValue, enabled.length);
    const startedAt = Date.now();
    const groups = await Promise.all(
      enabled.map((hook) => this.runHookGroup(event, hook, inputData, args.signal)),
    );
    for (const group of groups) {
      for (const record of group.records) {
        this.appendExecutionRecord(record);
      }
    }
    const results = groups.flatMap((group) => group.results);
    const { action, reason } = aggregateResults(event, results);
    this.emitResolved(event, matcherValue, action, reason, Date.now() - startedAt);
    return results;
  }

  private async runHookGroup(
    event: string,
    hook: HookDef,
    inputData: Record<string, unknown>,
    signal?: AbortSignal,
  ): Promise<{ results: HookResult[]; records: HookExecutionRecord[] }> {
    const hookId = hookIdOf(hook);
    const timeout = hook.timeout ?? DEFAULT_HOOK_TIMEOUT_SECONDS;
    const cwd = this.options.cwd === '' ? undefined : this.options.cwd;
    const env = this.options.env ?? {};

    if (hook.builtin !== undefined) {
      const builtin = this.options.builtins?.get(hook.builtin);
      if (builtin === undefined) {
        const reason = `Unknown builtin hook: ${hook.builtin}`;
        return {
          results: [{ action: 'allow', stderr: reason, errorKind: 'spawn' }],
          records: [
            {
              ts: Date.now(),
              event,
              hookId,
              kind: 'builtin',
              action: 'error',
              durationMs: 0,
              reason,
            },
          ],
        };
      }
      const startedAt = Date.now();
      const result = await builtin.run(inputData, { cwd, env, signal, timeout });
      return {
        results: [result],
        records: [this.resultToRecord(event, hookId, 'builtin', result, Date.now() - startedAt)],
      };
    }

    const commands = hook.commands ?? (hook.command !== undefined ? [hook.command] : []);
    const results: HookResult[] = [];
    const records: HookExecutionRecord[] = [];
    for (const command of commands) {
      const startedAt = Date.now();
      const result = await runHook(command, inputData, {
        timeout,
        cwd,
        signal,
      });
      results.push(result);
      records.push(this.resultToRecord(event, hookId, 'command', result, Date.now() - startedAt));
      if (result.action === 'block') break;
    }
    return { results, records };
  }

  private matchingHooks(event: string, matcherValue: string): HookDef[] {
    const seen = new Set<string>();
    const matched: HookDef[] = [];
    for (const hook of this.byEvent.get(event) ?? []) {
      if (!matches(hook.matcher ?? '', matcherValue)) continue;
      const key = groupKey(hook);
      if (seen.has(key)) continue;
      seen.add(key);
      matched.push(hook);
    }
    return matched;
  }

  private resultToRecord(
    event: string,
    hookId: string,
    kind: 'command' | 'builtin',
    result: HookResult,
    durationMs: number,
  ): HookExecutionRecord {
    let action: HookExecutionRecord['action'] = result.action;
    if (result.errorKind === 'timeout' || result.timedOut) {
      action = 'timeout';
    } else if (result.errorKind !== undefined) {
      action = 'error';
    }
    return {
      ts: Date.now(),
      event,
      hookId,
      kind,
      action,
      durationMs,
      reason: result.reason ?? result.stderr ?? result.message,
      stdout: result.stdout,
    };
  }

  private appendExecutionRecord(record: HookExecutionRecord): void {
    this.executionLog.push(record);
    if (this.executionLog.length > EXECUTION_LOG_MAX) {
      this.executionLog.splice(0, this.executionLog.length - EXECUTION_LOG_MAX);
    }
  }

  private emitTriggered(event: string, target: string, count: number): void {
    try {
      this.options.onTriggered?.(event, target, count);
    } catch {}
  }

  private emitResolved(
    event: string,
    target: string,
    action: string,
    reason: string | undefined,
    durationMs: number,
  ): void {
    try {
      this.options.onResolved?.(event, target, action, reason, durationMs);
    } catch {}
  }
}

function hookIdOf(hook: HookDef): string {
  return (
    hook.id ??
    hook.builtin ??
    hook.command ??
    hook.commands?.join(', ') ??
    'anonymous'
  );
}

function groupKey(hook: HookDef): string {
  if (hook.id !== undefined) return `id:${hook.id}`;
  if (hook.builtin !== undefined) return `builtin:${hook.builtin}`;
  if (hook.command !== undefined) return `cmd:${hook.command}`;
  if (hook.commands !== undefined) return `cmds:${hook.commands.join('\x00')}`;
  return `anon:${Math.random()}`;
}

function matches(pattern: string, value: string): boolean {
  if (pattern.length === 0) return true;
  try {
    return new RegExp(pattern).test(value);
  } catch {
    return false;
  }
}

function matcherValueText(value: HookMatcherValue | undefined): string {
  if (value === undefined) return '';
  if (typeof value === 'string') return value;
  return value
    .filter((part) => part.type === 'text')
    .map((part) => part.text)
    .join(' ');
}

function aggregateResults(
  event: string,
  results: readonly HookResult[],
): {
  readonly action: 'allow' | 'block';
  readonly reason?: string;
} {
  const block = blockDecision(event, results);
  if (block !== undefined) {
    return { action: 'block', reason: block.reason };
  }
  return { action: 'allow' };
}

function blockDecision(
  event: string,
  results: readonly HookResult[],
): HookBlockDecision | undefined {
  const block = results.find((result) => result.action === 'block');
  if (block === undefined) return undefined;
  const reason = block.reason?.trim();
  return {
    block: true,
    reason: reason === undefined || reason.length === 0 ? `Blocked by ${event} hook` : reason,
  };
}

function toHookInputData(input: Record<string, unknown>): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(input)) {
    result[camelToSnake(key)] = value;
  }
  return result;
}

function camelToSnake(value: string): string {
  return value.replaceAll(/[A-Z]/g, (ch) => `_${ch.toLowerCase()}`);
}
