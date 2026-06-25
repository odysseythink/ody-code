import type { ChatProvider, ModelCapability } from '@odysseythink/kosong';

/** Completion-token budget for the next LLM request. */
export interface CompletionBudgetConfig {
  /** Explicit user-configured maximum. */
  readonly hardCap?: number;
  /** Conservative cap for providers/models whose output ceiling is unknown. */
  readonly fallback?: number;
}

export const MIN_FLOOR = 1;
export const DEFAULT_UNKNOWN_OUTPUT_FALLBACK = 32000;

/**
 * Safety margin subtracted from the remaining context window when deriving a
 * completion cap from the current input. Accounts for tokenizer differences,
 * tool schema serialization, and message-formatting overhead that local
 * heuristics do not capture.
 */
const CONTEXT_WINDOW_OVERHEAD_TOKENS = 8192;

/**
 * Maximum share of the context window we are willing to reserve for a single
 * completion when we do not have an accurate input-token estimate. This keeps
 * long-context models from requesting an output budget that leaves no room for
 * the prompt itself.
 */
const MAX_CONTEXT_COMPLETION_RATIO = 0.25;

/**
 * Resolve configured completion budget. Env values are explicit hard caps;
 * non-positive env values disable clamping.
 */
export function resolveCompletionBudget(args: {
  readonly reservedContextSize?: number;
  readonly env?: NodeJS.ProcessEnv;
}): CompletionBudgetConfig | undefined {
  const env = args.env ?? process.env;
  const fromNew = parseEnvBudget(env['KIMI_MODEL_MAX_COMPLETION_TOKENS']);
  if (fromNew !== 'absent') {
    return fromNew === 'disabled' ? undefined : { hardCap: fromNew };
  }
  const fromLegacy = parseEnvBudget(env['KIMI_MODEL_MAX_TOKENS']);
  if (fromLegacy !== 'absent') {
    return fromLegacy === 'disabled' ? undefined : { hardCap: fromLegacy };
  }
  if (args.reservedContextSize !== undefined && args.reservedContextSize > 0) {
    return { fallback: args.reservedContextSize };
  }
  return { fallback: DEFAULT_UNKNOWN_OUTPUT_FALLBACK };
}

type EnvBudget = number | 'disabled' | 'absent';

function parseEnvBudget(raw: string | undefined): EnvBudget {
  if (raw === undefined || raw === '') return 'absent';
  const n = Number(raw);
  if (!Number.isFinite(n) || !Number.isInteger(n)) return 'absent';
  if (n <= 0) return 'disabled';
  return n;
}

/**
 * Compute the effective `max_completion_tokens` cap.
 *
 * The cap is the most restrictive of:
 *   - the explicit user hard cap
 *   - the model's declared max_output_tokens
 *   - the remaining context window after `inputTokens`
 *   - a fraction of the total context window
 */
export function computeCompletionBudgetCap(args: {
  readonly budget: CompletionBudgetConfig;
  readonly capability: ModelCapability | undefined;
  /** Estimated tokens already consumed by system prompt + messages + tools. */
  readonly inputTokens?: number;
}): number {
  const maxOutput = args.capability?.max_output_tokens ?? 0;
  const maxContext = args.capability?.max_context_tokens ?? 0;
  let cap =
    args.budget.hardCap ??
    (maxOutput > 0 ? maxOutput : args.budget.fallback ?? DEFAULT_UNKNOWN_OUTPUT_FALLBACK);

  if (maxContext > 0) {
    if (args.inputTokens !== undefined && args.inputTokens > 0) {
      const remaining = maxContext - args.inputTokens - CONTEXT_WINDOW_OVERHEAD_TOKENS;
      cap = Math.min(cap, Math.max(MIN_FLOOR, remaining));
    }
    cap = Math.min(cap, Math.floor(maxContext * MAX_CONTEXT_COMPLETION_RATIO));
  }

  return Math.max(MIN_FLOOR, cap);
}

/**
 * Apply a completion budget to a provider via its optional
 * `withMaxCompletionTokens` capability. Returns the original provider
 * unchanged when no budget is configured or the provider opts out.
 *
 * The returned provider is intentionally a shallow clone that shares the
 * original's HTTP client. Callers MUST treat it as a single-step value
 * and NOT persist it back to durable agent state — see the F3 discussion
 * in `KimiChatProvider._clone()`.
 */
export function applyCompletionBudget(args: {
  readonly provider: ChatProvider;
  readonly budget: CompletionBudgetConfig | undefined;
  readonly capability: ModelCapability | undefined;
  /** Estimated tokens already consumed by system prompt + messages + tools. */
  readonly inputTokens?: number;
}): ChatProvider {
  if (args.budget === undefined) return args.provider;
  if (args.provider.withMaxCompletionTokens === undefined) return args.provider;
  const cap = computeCompletionBudgetCap({
    budget: args.budget,
    capability: args.capability,
    inputTokens: args.inputTokens,
  });
  return args.provider.withMaxCompletionTokens(cap);
}
