import { remoteLLMStreamRegistry } from '../agent/turn/remote-kosong-llm';
import type { CoreRPCClient } from './client';
import { KimiCore, type KimiCoreOptions } from './core-impl';
import type {
  ChatStreamDeltaPayload,
  ChatStreamEndPayload,
  ChatStreamErrorPayload,
} from './llm-stream';

export class WorkerCoreAPI extends KimiCore {
  constructor(rpcClient: CoreRPCClient, options: KimiCoreOptions = {}) {
    super(rpcClient, options);
  }

  override chatStreamDelta(payload: ChatStreamDeltaPayload): void {
    remoteLLMStreamRegistry.dispatchDelta(payload);
  }

  override chatStreamEnd(payload: ChatStreamEndPayload): void {
    remoteLLMStreamRegistry.dispatchEnd(payload);
  }

  override chatStreamError(payload: ChatStreamErrorPayload): void {
    remoteLLMStreamRegistry.dispatchError(payload);
  }
}
