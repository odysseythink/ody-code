import { isMainThread, parentPort, workerData, type MessagePort } from 'node:worker_threads';

import { createMessagePortTransport } from '@odysseythink/agent-core';

import { createCoreServer, type CoreServerOptions } from './core-server';

export type CoreWorkerBootPayload = CoreServerOptions;

export function coreWorkerMain(port: MessagePort, options: CoreWorkerBootPayload): void {
  const server = createCoreServer(
    (dispatch) => createMessagePortTransport(port, dispatch),
    options,
  );

  // Signal to the main thread that the worker is ready
  port.postMessage({ type: 'ready' });

  void server;
}

if (!isMainThread && parentPort !== null && workerData !== undefined) {
  // As a worker thread, bootstrap from the parent port passed in workerData
  const { port } = workerData as { port: MessagePort };
  coreWorkerMain(port, workerData as CoreWorkerBootPayload);
}
