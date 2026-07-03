import { mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { Readable } from 'node:stream';

import {
  createRPC,
  KosongLLM,
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
  type LLMFactoryConfig,
  type ToolServices,
  WorkerCoreAPI,
} from '@odysseythink/agent-core';
import { SDKRpcClient, type SDKRpcClientConnectOptions } from '@odysseythink/ody-code-sdk';
import type { ChatProvider } from '@odysseythink/kosong';
import { LocalKaos } from '@odysseythink/kaos';

import type { BackendKind, ParityBackend } from './types';

export interface TsBackendConfig {
  readonly homeDir: string;
  readonly mockLlm?: ChatProvider | undefined;
  readonly runtime?: ToolServices | undefined;
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
    private readonly runtime?: ToolServices | undefined,
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

  toolCall(request: ToolCallRequest): Promise<ToolCallResponse> {
    // Echo back deterministic results for parity-registered mock tools.
    const args = typeof request.args === 'object' && request.args !== null
      ? (request.args as Record<string, unknown>)
      : {};
    const arg = String(args['query'] ?? args['text'] ?? '');
    if (arg.length > 0) {
      return Promise.resolve({ output: `mock result for ${arg}`, isError: false });
    }
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

  // Write a minimal config.toml so the agent can resolve the active model.
  // The actual LLM is injected via llmFactory, so the provider is never used.
  await writeFile(
    join(config.homeDir, 'config.toml'),
    `default_model = "mock"\ndefault_provider = "local"\n\n[providers.local]\ntype = "kimi"\napi_key = "test"\n\n[providers.openai]\ntype = "openai"\napi_key = "test"\n\n[models.mock]\nprovider = "local"\nmodel = "mock"\nmax_context_size = 4096\n\n[models.gpt-4o]\nprovider = "local"\nmodel = "gpt-4o"\nmax_context_size = 4096\n\n[models."openai/gpt-4o"]\nprovider = "openai"\nmodel = "gpt-4o"\nmax_context_size = 128000\n`,
    'utf8',
  );

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

  // WorkerCoreAPI owns the CoreAPI endpoint side of the RPC pair. It is kept alive
  // for the lifetime of the backend, while the client uses coreProxy to call into it.
  const _core = new WorkerCoreAPI(connectCore, {
    homeDir: config.homeDir,
    llmFactory,
    runtime: config.runtime,
  });

  const client = new SDKRpcClient({ homeDir: config.homeDir }, true);
  const clientApi = new ParityClientAPI(client, config.runtime);
  const coreProxy = await connectSdk(clientApi);
  Object.assign(client, { rpc: coreProxy, ready: Promise.resolve() });

  const kaos = await LocalKaos.create();
  await kaos.chdir(config.homeDir);

  return {
    kind: 'ts' as BackendKind,
    client,
    homeDir: config.homeDir,
    envCall: async (method, payload) => envCallTs(kaos, method, payload),
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
    envCall: async (method, payload) => {
      const rpc = (client as unknown as Record<string, unknown>)['rpc'] as Record<string, (payload: unknown) => Promise<unknown>>;
      if (typeof rpc[method] !== 'function') {
        throw new Error(`Rust backend does not expose ${method}`);
      }
      return rpc[method](payload);
    },
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
  for (let attempt = 0; attempt < 10; attempt++) {
    try {
      await rm(dir, { recursive: true, force: true });
      return;
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (code !== 'ENOTEMPTY' && code !== 'EBUSY' && code !== 'EPERM') {
        throw error;
      }
      await new Promise((resolve) => {
        setTimeout(resolve, 10);
      });
    }
  }
  await rm(dir, { recursive: true, force: true });
}

async function envCallTs(
  kaos: LocalKaos,
  method: string,
  payload: unknown,
): Promise<unknown> {
  const p = payload as Record<string, unknown>;
  switch (method) {
    case 'env.getcwd':
      return { cwd: kaos.getcwd() };
    case 'env.stat': {
      const s = await kaos.stat(String(p['path']), {
        followSymlinks: (p['followSymlinks'] as boolean | undefined) ?? true,
      });
      const isDir = (s.stMode & 0o170000) === 0o040000;
      return { ...s, isDir };
    }
    case 'env.glob': {
      const matches: string[] = [];
      for await (const m of kaos.glob(String(p['path']), String(p['pattern']), {
        caseSensitive: (p['caseSensitive'] as boolean | undefined) ?? true,
      })) {
        matches.push(m);
      }
      matches.sort();
      return { matches };
    }
    case 'env.readText': {
      const text = await kaos.readText(String(p['path']), {
        encoding: (p['encoding'] as BufferEncoding | undefined) ?? 'utf-8',
        errors: (p['errors'] as 'strict' | 'replace' | 'ignore' | undefined) ?? 'strict',
      });
      return { text };
    }
    case 'env.writeText': {
      const written = await kaos.writeText(String(p['path']), String(p['text']), {
        mode: ((p['mode'] as string | undefined) === 'a' ? 'a' : 'w') as 'w' | 'a',
        encoding: (p['encoding'] as BufferEncoding | undefined) ?? 'utf-8',
      });
      return { written };
    }
    case 'env.exec': {
      const args = (p['args'] as string[] | undefined) ?? [];
      const env = p['env'] as Record<string, string> | undefined;
      const proc =
        env !== undefined && Object.keys(env).length > 0
          ? await kaos.execWithEnv([String(p['command']), ...args], env)
          : await kaos.exec(String(p['command']), ...args);
      const [stdout, stderr] = await Promise.all([
        streamToBuffer(proc.stdout),
        streamToBuffer(proc.stderr),
      ]);
      const exitCode = await proc.wait();
      return {
        exitCode,
        stdout: Array.from(stdout),
        stderr: Array.from(stderr),
      };
    }
    default:
      throw new Error(`unknown env method: ${method}`);
  }
}

async function streamToBuffer(readable: Readable): Promise<Buffer> {
  const chunks: Buffer[] = [];
  for await (const chunk of readable) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  return Buffer.concat(chunks);
}
