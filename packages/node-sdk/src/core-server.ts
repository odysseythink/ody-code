import {
  createRPCEndpoint,
  RemoteKosongLLM,
  WorkerCoreAPI,
  type CoreAPI,
  type Dispatch,
  type SDKAgentRPC,
  type SDKAPI,
  type TelemetryClient,
  type Transport,
} from '@odysseythink/agent-core';
import type { OAuthTokenProviderResolver } from '@odysseythink/agent-core';

export interface CoreServerOptions {
  readonly homeDir?: string | undefined;
  readonly configPath?: string | undefined;
  readonly skillDirs?: readonly string[];
  readonly appVersion?: string | undefined;
  readonly telemetry?: TelemetryClient | undefined;
  readonly resolveOAuthTokenProvider?: OAuthTokenProviderResolver | undefined;
}

export function createCoreServer(
  createTransport: (dispatch: Dispatch) => Transport,
  options: CoreServerOptions,
): { close(): void } {
  const endpoint = createRPCEndpoint<CoreAPI, SDKAPI>();
  const transport = createTransport(endpoint.dispatch);
  endpoint.setTransport(transport);

  const core = new WorkerCoreAPI(endpoint.client, {
    homeDir: options.homeDir,
    configPath: options.configPath,
    skillDirs: options.skillDirs,
    appVersion: options.appVersion,
    telemetry: options.telemetry,
    resolveOAuthTokenProvider: options.resolveOAuthTokenProvider,
    llmFactory: (rpc, config) => {
      if (config.provider === undefined) {
        throw new Error('Provider config is required for worker-mode LLM proxy');
      }
      return new RemoteKosongLLM({
        sdk: rpc as SDKAgentRPC,
        ...config,
        provider: config.provider,
      });
    },
  });

  void core;

  return {
    close(): void {
      transport.close?.();
    },
  };
}
