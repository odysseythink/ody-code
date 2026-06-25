import type { ContentPart } from '@odysseythink/kosong';

import type { RPCMethods } from './client';
import type { AgentEvent, ToolInputDisplay } from './events';
import type {
  ChatStreamCancelPayload,
  ChatStreamInitPayload,
  ChatStreamInitResponse,
} from './llm-stream';
import type { WithAgentId, WithSessionId } from './types';

export type ApprovalDecision = 'approved' | 'rejected' | 'cancelled';
export type ApprovalScope = 'session';

export interface ApprovalResponse {
  readonly decision: ApprovalDecision;
  readonly scope?: ApprovalScope | undefined;
  readonly feedback?: string | undefined;
  readonly selectedLabel?: string | undefined;
}

export interface ApprovalRequest {
  readonly turnId?: number | undefined;
  readonly toolCallId: string;
  readonly toolName: string;
  readonly action: string;
  readonly display: ToolInputDisplay;
}

export interface QuestionOption {
  readonly label: string;
  readonly description?: string;
}

export interface QuestionItem {
  readonly question: string;
  readonly header?: string;
  readonly body?: string;
  readonly options: readonly QuestionOption[];
  readonly multiSelect?: boolean;
  readonly otherLabel?: string;
  readonly otherDescription?: string;
}

export type QuestionAnswerMethod = 'enter' | 'space' | 'number_key';
export type QuestionAnswers = Record<string, string | true>;

export interface QuestionResponse {
  readonly answers: QuestionAnswers;
  readonly method?: QuestionAnswerMethod | undefined;
}

export type QuestionResult = null | QuestionAnswers | QuestionResponse;

export interface QuestionRequest {
  readonly turnId?: number;
  readonly toolCallId?: string;
  readonly questions: readonly QuestionItem[];
}

export interface ToolCallRequest {
  readonly turnId?: number | undefined;
  readonly toolCallId: string;
  readonly args: unknown;
}

export interface OpenExternalRequest {
  /** Absolute `file://` URL or `http(s)://` URL the host should open in the user's browser. */
  readonly url: string;
  /** Optional human-readable title for the resource (for logging/telemetry). */
  readonly title?: string;
}

export interface OpenExternalResponse {
  /** True if the host accepted the request and attempted to open the resource. */
  readonly opened: boolean;
  /** Populated when `opened` is false (no handler registered, host declined, etc.). */
  readonly error?: string;
}

export interface ToolCallResponse {
  readonly output: string | ContentPart[];
  readonly isError?: boolean | undefined;
}

export interface SDKAgentAPI {
  emitEvent: (event: AgentEvent) => void;
  requestApproval: (request: ApprovalRequest) => Promise<ApprovalResponse>;
  requestQuestion: (request: QuestionRequest) => Promise<QuestionResult>;
  toolCall: (request: ToolCallRequest) => Promise<ToolCallResponse>;
  /**
   * Ask the host to open a local file / URL in the user's browser (the "visual
   * companion" for design mode). Hosts without a desktop browser resolve this
   * with `{ opened: false }`; the agent only calls it via the optional
   * `agent.rpc?.openExternal`, so the ShowDesignMockup tool stays gated.
   */
  openExternal: (request: OpenExternalRequest) => Promise<OpenExternalResponse>;

  chatStreamInit: (payload: ChatStreamInitPayload) => Promise<ChatStreamInitResponse>;
  chatStreamCancel: (payload: ChatStreamCancelPayload) => void;
}
export type SDKAgentRPC = RPCMethods<SDKAgentAPI>;

export type SDKSessionAPI = WithAgentId<SDKAgentAPI>;
export type SDKSessionRPC = RPCMethods<SDKSessionAPI>;

export type SDKAPI = WithSessionId<SDKSessionAPI>;
export type SDKRPC = RPCMethods<SDKAPI>;

export type SDKAPIProtocol = {
  [K in keyof SDKAPI]: {
    payload: Parameters<SDKAPI[K]>[0];
    returns: Awaited<ReturnType<SDKAPI[K]>>;
  };
};
