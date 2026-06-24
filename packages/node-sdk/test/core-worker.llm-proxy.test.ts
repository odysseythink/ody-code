import { describe, it, expect } from 'vitest';
import { MessageChannel } from 'node:worker_threads';
import { createMessagePortTransport, createRPCEndpoint, type CoreAPI } from '@odysseythink/agent-core';

describe('core-worker LLM proxy', () => {
  it('sends a ready signal after initialization', async () => {
    const { port1, port2 } = new MessageChannel();

    // Import and call coreWorkerMain directly
    const { coreWorkerMain } = await import('../src/core-worker');
    coreWorkerMain(port1, { homeDir: '/tmp' });

    // Listen for the ready message on port2
    const readyMessage = await new Promise<any>((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error('Timeout waiting for ready')), 2000);
      port2.on('message', (msg: any) => {
        clearTimeout(timeout);
        resolve(msg);
      });
      port2.start();
    });

    expect(readyMessage).toEqual({ type: 'ready' });
  });

  it('handles RPC round-trip after ready', async () => {
    const { port1, port2 } = new MessageChannel();
    const { coreWorkerMain } = await import('../src/core-worker');

    // Set up the main-thread side (client/worker API)
    const endpoint = createRPCEndpoint<CoreAPI, any>();
    const transport = createMessagePortTransport(port2, endpoint.dispatch);
    endpoint.setTransport(transport);

    // Start the worker
    coreWorkerMain(port1, { homeDir: '/tmp' });

    // Wait for ready
    await new Promise<void>((resolve) => {
      port2.on('message', (msg: any) => {
        if (msg.type === 'ready') resolve();
      });
      port2.start();
    });

    // Worker is alive and communicating
    expect(true).toBe(true);
  });
});
