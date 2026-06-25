import type { ChatProvider, ModelCapability } from '@odysseythink/kosong';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  applyCompletionBudget,
  computeCompletionBudgetCap,
  resolveCompletionBudget,
} from '../../src/utils/completion-budget';

function makeCapability(maxContextTokens: number, maxOutputTokens: number = 0): ModelCapability {
  return {
    image_in: false,
    video_in: false,
    audio_in: false,
    thinking: false,
    tool_use: true,
    max_context_tokens: maxContextTokens,
    max_output_tokens: maxOutputTokens,
  };
}

describe('computeCompletionBudgetCap', () => {
  it('uses model max_output_tokens when no hard cap is set', () => {
    const cap = computeCompletionBudgetCap({
      budget: { fallback: 32000 },
      capability: makeCapability(1_000_000, 8192),
    });
    expect(cap).toBe(8192);
  });

  it('caps completion budget to a fraction of max_context_tokens', () => {
    const cap = computeCompletionBudgetCap({
      budget: { fallback: 32000 },
      capability: makeCapability(1_000_000, 500_000),
    });
    expect(cap).toBe(250_000);
  });

  it('limits completion budget when inputTokens leave little room', () => {
    const cap = computeCompletionBudgetCap({
      budget: { fallback: 32000 },
      capability: makeCapability(1_000_000, 384_000),
      inputTokens: 900_000,
    });
    // 1_000_000 - 900_000 - 8192 = 91_808, floored at 1 -> 91_808
    // also capped by 25% of context = 250_000
    expect(cap).toBe(91_808);
  });

  it('falls back to the context-ratio cap when inputTokens are unknown', () => {
    const cap = computeCompletionBudgetCap({
      budget: { fallback: 32000 },
      capability: makeCapability(1_000_000, 384_000),
    });
    expect(cap).toBe(250_000);
  });

  it('lets an explicit hardCap override the context-derived cap', () => {
    const cap = computeCompletionBudgetCap({
      budget: { hardCap: 200_000 },
      capability: makeCapability(1_000_000, 384_000),
      inputTokens: 100_000,
    });
    expect(cap).toBe(200_000);
  });

  it('floors the context-derived cap when inputTokens already exceed the window', () => {
    const cap = computeCompletionBudgetCap({
      budget: { fallback: 32000 },
      capability: makeCapability(100_000, 8192),
      inputTokens: 200_000,
    });
    expect(cap).toBe(1);
  });

  it('ignores context capping when max_context_tokens is unknown', () => {
    const cap = computeCompletionBudgetCap({
      budget: { fallback: 32000 },
      capability: makeCapability(0, 384_000),
      inputTokens: 900_000,
    });
    expect(cap).toBe(384_000);
  });

  it('lets an explicit hardCap win when max_context_tokens is unknown', () => {
    const cap = computeCompletionBudgetCap({
      budget: { hardCap: 1000 },
      capability: makeCapability(0, 384_000),
      inputTokens: 900_000,
    });
    expect(cap).toBe(1000);
  });

  it('uses fallback when max_output_tokens is unknown (0)', () => {
    const cap = computeCompletionBudgetCap({
      budget: { fallback: 16000 },
      capability: makeCapability(100000, 0),
    });
    expect(cap).toBe(16000);
  });

  it('uses DEFAULT_UNKNOWN_OUTPUT_FALLBACK when both capability and fallback are unknown', () => {
    const cap = computeCompletionBudgetCap({
      budget: {},
      capability: undefined,
    });
    expect(cap).toBe(32000);
  });

  it('explicit hard cap wins over max_output_tokens', () => {
    const cap = computeCompletionBudgetCap({
      budget: { hardCap: 1024 },
      capability: makeCapability(1_000_000, 8192),
    });
    expect(cap).toBe(1024);
  });

  it('floors at 1 when computed cap is not positive', () => {
    expect(
      computeCompletionBudgetCap({
        budget: { hardCap: 0 },
        capability: undefined,
      }),
    ).toBe(1);
  });
});

describe('applyCompletionBudget', () => {
  let withMaxCompletionTokens: ReturnType<typeof vi.fn>;
  let original: ChatProvider;

  beforeEach(() => {
    const cloneFactory = (n: number): ChatProvider => {
      const clone = { ...original, _maxTokensApplied: n };
      return clone as unknown as ChatProvider;
    };
    withMaxCompletionTokens = vi.fn(cloneFactory);
    original = {
      name: 'mock',
      modelName: 'mock-model',
      thinkingEffort: null,
      generate: vi.fn() as unknown as ChatProvider['generate'],
      withThinking: vi.fn() as unknown as ChatProvider['withThinking'],
      withMaxCompletionTokens: withMaxCompletionTokens as unknown as (
        n: number,
      ) => ChatProvider,
    };
  });

  it('returns the original provider when no budget is configured', () => {
    const result = applyCompletionBudget({
      provider: original,
      budget: undefined,
      capability: makeCapability(10000, 4096),
    });
    expect(result).toBe(original);
    expect(withMaxCompletionTokens).not.toHaveBeenCalled();
  });

  it('returns the original provider when withMaxCompletionTokens is not implemented', () => {
    const { withMaxCompletionTokens: _drop, ...rest } = original;
    void _drop;
    const opaque = rest as unknown as ChatProvider;
    const result = applyCompletionBudget({
      provider: opaque,
      budget: { hardCap: 8192 },
      capability: makeCapability(10000, 4096),
    });
    expect(result).toBe(opaque);
  });

  it('clones the provider with max_output_tokens-derived cap when budget is configured', () => {
    const result = applyCompletionBudget({
      provider: original,
      budget: { fallback: 32000 },
      capability: makeCapability(100000, 8192),
    });
    expect(withMaxCompletionTokens).toHaveBeenCalledOnce();
    expect(withMaxCompletionTokens.mock.calls[0]?.[0]).toBe(8192);
    expect(result).not.toBe(original);
  });

  it('uses the explicit hard cap when configured', () => {
    const result = applyCompletionBudget({
      provider: original,
      budget: { hardCap: 1024 },
      capability: makeCapability(100000, 8192),
    });
    expect(withMaxCompletionTokens).toHaveBeenCalledOnce();
    expect(withMaxCompletionTokens.mock.calls[0]?.[0]).toBe(1024);
    expect(result).not.toBe(original);
  });

  it('passes inputTokens through to cap the effective completion budget', () => {
    applyCompletionBudget({
      provider: original,
      budget: { fallback: 32000 },
      capability: makeCapability(100000, 8192),
      inputTokens: 95_000,
    });
    expect(withMaxCompletionTokens).toHaveBeenCalledOnce();
    // 100000 - 95000 - 8192 = -3192 -> floored to 1
    expect(withMaxCompletionTokens.mock.calls[0]?.[0]).toBe(1);
  });
});

describe('resolveCompletionBudget', () => {
  it('reads KIMI_MODEL_MAX_COMPLETION_TOKENS first', () => {
    const budget = resolveCompletionBudget({
      reservedContextSize: 1000,
      env: {
        KIMI_MODEL_MAX_COMPLETION_TOKENS: '4096',
        KIMI_MODEL_MAX_TOKENS: '2048',
      },
    });
    expect(budget?.hardCap).toBe(4096);
  });

  it('falls back to legacy KIMI_MODEL_MAX_TOKENS when the new var is unset', () => {
    const budget = resolveCompletionBudget({
      reservedContextSize: 1000,
      env: { KIMI_MODEL_MAX_TOKENS: '2048' },
    });
    expect(budget?.hardCap).toBe(2048);
  });

  it('uses reservedContextSize as the unknown-context fallback when no env var is set', () => {
    const budget = resolveCompletionBudget({
      reservedContextSize: 12345,
      env: {},
    });
    expect(budget?.hardCap).toBeUndefined();
    expect(budget?.fallback).toBe(12345);
  });

  it('falls back to 32000 only for unknown context when nothing is configured', () => {
    const budget = resolveCompletionBudget({ env: {} });
    expect(budget?.hardCap).toBeUndefined();
    expect(budget?.fallback).toBe(32000);
  });

  it('ignores reservedContextSize when it is 0', () => {
    const budget = resolveCompletionBudget({
      reservedContextSize: 0,
      env: {},
    });
    expect(budget?.hardCap).toBeUndefined();
    expect(budget?.fallback).toBe(32000);
  });

  it('treats non-positive KIMI_MODEL_MAX_COMPLETION_TOKENS as an opt-out', () => {
    expect(
      resolveCompletionBudget({
        reservedContextSize: 1000,
        env: { KIMI_MODEL_MAX_COMPLETION_TOKENS: '0' },
      }),
    ).toBeUndefined();
    expect(
      resolveCompletionBudget({
        reservedContextSize: 1000,
        env: { KIMI_MODEL_MAX_COMPLETION_TOKENS: '-1' },
      }),
    ).toBeUndefined();
  });

  it('treats non-positive legacy KIMI_MODEL_MAX_TOKENS as an opt-out when the new var is unset', () => {
    expect(
      resolveCompletionBudget({
        reservedContextSize: 1000,
        env: { KIMI_MODEL_MAX_TOKENS: '-1' },
      }),
    ).toBeUndefined();
  });

  it('lets the new var override a legacy disable signal', () => {
    const budget = resolveCompletionBudget({
      env: {
        KIMI_MODEL_MAX_COMPLETION_TOKENS: '4096',
        KIMI_MODEL_MAX_TOKENS: '-1',
      },
    });
    expect(budget?.hardCap).toBe(4096);
  });

  it('falls back to defaults when the env var is non-numeric garbage', () => {
    const budget = resolveCompletionBudget({
      env: { KIMI_MODEL_MAX_COMPLETION_TOKENS: 'not-a-number' },
    });
    expect(budget?.hardCap).toBeUndefined();
    expect(budget?.fallback).toBe(32000);
  });
});
