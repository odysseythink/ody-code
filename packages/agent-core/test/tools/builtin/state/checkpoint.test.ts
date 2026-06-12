import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../../../src/agent';
import { CheckpointTool } from '../../../../src/tools/builtin/state/checkpoint';

function makeAgent(coordinator?: { checkpointNow: () => Promise<void> }): Agent {
  return {
    checkpointCoordinator: coordinator,
  } as unknown as Agent;
}

describe('CheckpointTool', () => {
  it('triggers the coordinator when attached', async () => {
    const checkpointNow = vi.fn().mockResolvedValue(undefined);
    const tool = new CheckpointTool(makeAgent({ checkpointNow }));
    const execution = tool.resolveExecution({ reason: 'before risky op' });

    const result = await execution.execute();

    expect(checkpointNow).toHaveBeenCalled();
    expect(result.isError).toBe(false);
    expect(result.output).toContain('saved');
  });

  it('returns an error when no coordinator is attached', async () => {
    const tool = new CheckpointTool(makeAgent(undefined));
    const execution = tool.resolveExecution({});

    const result = await execution.execute();

    expect(result.isError).toBe(true);
    expect(result.output).toContain('not enabled');
  });
});
