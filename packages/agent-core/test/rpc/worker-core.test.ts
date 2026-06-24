import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { describe, expect, it, vi } from 'vitest';

import { remoteLLMStreamRegistry } from '../../src/agent/turn/remote-kosong-llm';
import { WorkerCoreAPI } from '../../src/rpc/worker-core';
import { createRPC } from '../../src/rpc/client';
import type { CoreAPI, SDKAPI } from '../../src/rpc';

describe('WorkerCoreAPI stream routing', () => {
  it('routes chatStream* to the remote LLM registry', async () => {
    const tmpDir = mkdtempSync(join(tmpdir(), 'ody-worker-core-'));
    const [connectCore] = createRPC<CoreAPI, SDKAPI>();
    const core = new WorkerCoreAPI(connectCore, { homeDir: tmpDir });
    try {
      const onDelta = vi.fn();
      const onEnd = vi.fn();
      const onError = vi.fn();
      remoteLLMStreamRegistry.register('stream-1', { onDelta, onEnd, onError });

      core.chatStreamDelta({ streamId: 'stream-1', delta: { type: 'text', text: 'hi' } });
      core.chatStreamEnd({
        streamId: 'stream-1',
        result: { toolCalls: [], usage: { totalTokens: 1 } as any },
      });

      expect(onDelta).toHaveBeenCalledWith({ type: 'text', text: 'hi' });
      expect(onEnd).toHaveBeenCalled();
      expect(onError).not.toHaveBeenCalled();
    } finally {
      rmSync(tmpDir, { recursive: true, force: true });
    }
  });
});
