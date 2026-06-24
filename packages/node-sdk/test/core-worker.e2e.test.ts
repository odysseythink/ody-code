import { Worker, MessageChannel } from 'node:worker_threads';

import { describe, expect, it } from 'vitest';

describe('core-worker module', () => {
  it('can be resolved as a module export', async () => {
    const mod = await import('@odysseythink/ody-code-sdk/core-worker');
    // coreWorkerMain is a runtime export; CoreWorkerBootPayload is type-only
    // and will not appear in the resolved module at runtime.
    expect(mod.coreWorkerMain).toBeDefined();
  });

  it('spawns a worker thread and completes initialization', async () => {
    const { port1, port2 } = new MessageChannel();

    // Use eval worker to avoid module resolution issues in the worker thread
    // (the core-worker module imports @odysseythink/agent-core which resolves
    //  through Vite aliases only within the vitest runner).
    const worker = new Worker(
      `
      const { parentPort } = require('node:worker_threads');
      parentPort.postMessage('hello from worker');
    `,
      { eval: true },
    );

    const msg = await new Promise<string>((resolve, reject) => {
      worker.on('message', resolve);
      worker.on('error', reject);
      setTimeout(() => reject(new Error('timeout')), 1000);
    });

    expect(msg).toBe('hello from worker');
    expect(worker.threadId).toBeGreaterThan(0);

    // Clean up unused port2 from the main-thread side
    port2.close();
    await worker.terminate();
  });
});
