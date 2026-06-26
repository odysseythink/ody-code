import type {
  FinishReason,
  Message,
  ModelCapability,
  ProviderConfig,
  Tool,
  ToolCall,
  TokenUsage,
} from '@odysseythink/kosong';

import type { OdyErrorPayload } from '@odysseythink/agent-core-shared';
import type { LLMRequestLogContext, LLMStreamTiming } from '#loop/llm';
import type { CompletionBudgetConfig } from '#utils/completion-budget';

// ── Stream delta types ──────────────────────────────────────────────

export type StreamDelta =
  | { readonly type: 'text'; readonly text: string }
  | { readonly type: 'think'; readonly think: string }
  | {
      readonly type: 'tool_call_part';
      readonly toolCallId: string;
      readonly name?: string | undefined;
      readonly argumentsPart?: string | undefined;
    };

// ── RPC payloads ────────────────────────────────────────────────────

/**
 * Request to initialise a chat stream on the main thread.
 * All fields must be structuredClone-serializable.
 */
export interface ChatStreamRequest {
  readonly modelName: string;
  readonly systemPrompt: string;
  readonly messages: readonly Message[];
  readonly tools: readonly Tool[];
  readonly capability?: ModelCapability | undefined;
  readonly completionBudgetConfig?: CompletionBudgetConfig | undefined;
  readonly requestLogContext?: LLMRequestLogContext | undefined;
  /** Provider configuration used by the host thread to recreate the LLM provider. Must be structuredClone-serializable. */
  readonly provider: ProviderConfig;
}

/** Response from chatStreamInit — the stream ID to reference in subsequent deltas. */
export interface ChatStreamInitResult {
  readonly streamId: string;
}

/** Payload for chatStreamInit RPC (alias for the wrapping object). */
export interface ChatStreamInitPayload {
  readonly request: ChatStreamRequest;
  /** Stream ID assigned by the worker/server; the host must use this ID for all deltas. */
  readonly streamId: string;
}

/** Response type alias used by SDKAgentAPI. */
export type ChatStreamInitResponse = ChatStreamInitResult;

/** Delta dispatched from the main thread to the worker. */
export interface ChatStreamDeltaPayload {
  readonly streamId: string;
  readonly delta: StreamDelta;
}

/** Terminal success payload. */
export interface ChatStreamEndPayload {
  readonly streamId: string;
  readonly result: ChatStreamResult;
}

/** Terminal error payload. */
export interface ChatStreamErrorPayload {
  readonly streamId: string;
  readonly error: OdyErrorPayload;
}

/** Cancel a running stream. */
export interface ChatStreamCancelPayload {
  readonly streamId: string;
}

// ── Stream result ───────────────────────────────────────────────────

export interface ChatStreamResult {
  readonly toolCalls: readonly ToolCall[];
  readonly providerFinishReason?: FinishReason | undefined;
  readonly rawFinishReason?: string | undefined;
  readonly usage: TokenUsage;
  readonly streamTiming?: LLMStreamTiming | undefined;
}
