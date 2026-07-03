import { writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';

import type { ChatProvider } from '@odysseythink/kosong';

import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';
import { waitForEvent } from './utils';

export const backgroundCronMockLlm: ChatProvider = new MockChatProvider([]);

interface RawRpc {
  rpc: {
    getBackground: (p: unknown) => Promise<unknown>;
    stopBackground: (p: unknown) => Promise<unknown>;
  };
}

export const backgroundCronScenario: Scenario = {
  name: 'background-cron',
  async run(backend) {
    const clockFile = join(tmpdir(), `parity-cron-${Date.now()}.txt`);
    writeFileSync(clockFile, '0', 'utf8');

    const previousManualTick = process.env['ODY_CRON_MANUAL_TICK'];
    const previousClock = process.env['ODY_CRON_CLOCK'];
    process.env['ODY_CRON_MANUAL_TICK'] = '1';
    process.env['ODY_CRON_CLOCK'] = `file:${clockFile}`;

    try {
      const summary = await backend.client.createSession({
        workDir: backend.homeDir,
        permission: 'auto',
        model: 'mock',
        id: 'background-cron-001',
      });

      // List background tasks to verify the method exists
      const rpc = (backend.client as unknown as RawRpc).rpc;
      const tasks = await rpc.getBackground({ sessionId: summary.id, agentId: 'main' });

      return {
        responses: [
          { taskCount: Array.isArray(tasks) ? tasks.length : 0 },
        ],
        events: [],
      };
    } finally {
      process.env['ODY_CRON_MANUAL_TICK'] = previousManualTick;
      process.env['ODY_CRON_CLOCK'] = previousClock;
    }
  },
};
