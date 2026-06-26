import { mkdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';

import {
  createRPC,
  type CoreAPI,
  type SDKAPI,
  type SDKAgentRPC,
  type Event,
  type ApprovalRequest,
  type ApprovalResponse,
  type QuestionRequest,
  type QuestionResult,
  type ToolCallRequest,
  type ToolCallResponse,
  type OpenExternalRequest,
  type OpenExternalResponse,
  type ChatStreamInitPayload,
  type ChatStreamInitResponse,
  type ChatStreamCancelPayload,
  WorkerCoreAPI,
} from '@odysseythink/agent-core';
import { KosongLLM } from '@odysseythink/agent-core';
import { SDKRpcClient, type SDKRpcClientConnectOptions } from '@odysseythink/ody-code-sdk';
import type { LLMFactoryConfig } from '@odysseythink/agent-core';
import type { ChatProvider } from '@odysseythink/kosong';

import type { BackendKind, ParityBackend } from './types';

export interface TsBackendConfig {
  readonly homeDir: string;
  readonly mockLlm?: ChatProvider | undefined;
}

export interface RustBackendConfig {
  readonly homeDir: string;
  readonly binaryPath: string;
  readonly transport: 'stdio' | { socketPath: string } | { host: string; port: number };
  readonly extraArgs?: readonly string[];
}

class ParityClientAPI implements SDKAPI {
  constructor(
    private readonly client: SDKRpcClient,
    private readonly getRpc: () => Promise<unknown>,
  ) {}

  emitEvent(event: Event): void {
    this.client.receiveEvent(event);
  }

  requestApproval(_request: ApprovalRequest): Promise<ApprovalResponse> {
    return Promise.resolve({ decision: 'cancelled', feedback: 'No approval handler in parity tests.' });
  }

  requestQuestion(_request: QuestionRequest): Promise<QuestionResult> {
    return Promise.resolve(null);
  }

  toolCall(_request: ToolCallRequest): Promise<ToolCallResponse> {
    return Promise.resolve({ output: 'SDK tool calls are not supported in parity tests.', isError: true });
  }

  openExternal(_request: OpenExternalRequest): Promise<OpenExternalResponse> {
    return Promise.resolve({ opened: false, error: 'No open-external handler in parity tests.' });
  }

  async chatStreamInit(_payload: ChatStreamInitPayload): Promise<ChatStreamInitResponse> {
    throw new Error('chatStreamInit is not supported in parity TS backend; use llmFactory instead.');
  }

  chatStreamCancel(_payload: ChatStreamCancelPayload): void {
    // no-op
  }
}

export async function makeTsBackend(config: TsBackendConfig): Promise<ParityBackend> {
  const [connectCore, connectSdk] = createRPC<CoreAPI, SDKAPI>();

  const llmFactory = config.mockLlm !== undefined
    ? (_rpc: Partial<SDKAgentRPC>, factoryConfig: LLMFactoryConfig) =>
        new KosongLLM({
          provider: config.mockLlm as ChatProvider,
          modelName: factoryConfig.modelName,
          systemPrompt: factoryConfig.systemPrompt,
          capability: factoryConfig.capability,
          completionBudgetConfig: factoryConfig.completionBudgetConfig,
        })
    : undefined;

  const core = new WorkerCoreAPI(connectCore, {
    homeDir: config.homeDir,
    llmFactory,
  });
  void core;

  const client = new SDKRpcClient({ homeDir: config.homeDir }, true);
  const clientApi = new ParityClientAPI(client, () => Promise.resolve(coreProxy));
  const coreProxy = await connectSdk(clientApi);
  Object.assign(client, { rpc: coreProxy, ready: Promise.resolve() });

  return {
    kind: 'ts' as BackendKind,
    client,
    homeDir: config.homeDir,
    close: async () => {
      await client.close?.().catch(() => {});
    },
  };
}

export async function makeRustBackend(config: RustBackendConfig): Promise<ParityBackend> {
  const transport: SDKRpcClientConnectOptions['transport'] =
    config.transport === 'stdio'
      ? 'stdio'
      : 'socketPath' in config.transport
        ? { socketPath: config.transport.socketPath, spawn: true }
        : { host: config.transport.host, port: config.transport.port, spawn: true };

  const client = await SDKRpcClient.connect({
    transport,
    binaryPath: config.binaryPath,
    homeDir: config.homeDir,
    extraArgs: config.extraArgs,
  });

  return {
    kind: 'rust' as BackendKind,
    client,
    homeDir: config.homeDir,
    close: async () => {
      await client.close?.().catch(() => {});
    },
  };
}

export async function createTempHome(prefix = 'parity-'): Promise<string> {
  const dir = join(tmpdir(), `${prefix}${Date.now()}-${Math.random().toString(36).slice(2, 8)}`);
  await mkdir(dir, { recursive: true });
  return dir;
}

export async function cleanupHome(dir: string): Promise<void> {
  await rm(dir, { recursive: true, force: true });
}
