import { isMainThread, parentPort, workerData, type MessagePort } from 'node:worker_threads';

import {
  createMessagePortTransport,
  createRPCEndpoint,
  WorkerCoreAPI,
  type CoreAPI,
  type SDKAPI,
  type SDKAgentRPC,
} from '@odysseythink/agent-core';
import { RemoteKosongLLM } from '@odysseythink/agent-core';

export interface CoreWorkerBootPayload {
  readonly homeDir?: string | undefined;
  readonly configPath?: string | undefined;
  readonly skillDirs?: readonly string[];
  readonly appVersion?: string | undefined;
}

export function coreWorkerMain(port: MessagePort, options: CoreWorkerBootPayload): void {
  const endpoint = createRPCEndpoint<CoreAPI, SDKAPI>();
  const transport = createMessagePortTransport(port, endpoint.dispatch);
  endpoint.setTransport(transport);

  const core = new WorkerCoreAPI(endpoint.client, {
    homeDir: options.homeDir,
    configPath: options.configPath,
    skillDirs: options.skillDirs,
    appVersion: options.appVersion,
    llmFactory: (rpc, config) =>
      new RemoteKosongLLM({
        sdk: rpc as SDKAgentRPC,
        ...config,
      }),
  });

  // WorkerCoreAPI + endpoint begin handling RPC requests from the main thread.
  void core;
}

if (!isMainThread && parentPort !== null && workerData !== undefined) {
  // As a worker thread, bootstrap from the parent port passed in workerData
  const { port } = workerData as { port: MessagePort };
  coreWorkerMain(port, workerData as CoreWorkerBootPayload);
}
