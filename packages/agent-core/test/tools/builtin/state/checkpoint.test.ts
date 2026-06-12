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

    if (!('execute' in execution)) throw new Error('expected a runnable execution');
    const result = await execution.execute({} as never);

    expect(checkpointNow).toHaveBeenCalled();
    expect(result.isError).toBe(false);
    expect(result.output).toContain('saved');
  });

  it('returns an error when no coordinator is attached', async () => {
    const tool = new CheckpointTool(makeAgent(undefined));
    const execution = tool.resolveExecution({});

    if (!('execute' in execution)) throw new Error('expected a runnable execution');
    const result = await execution.execute({} as never);

    expect(result.isError).toBe(true);
    expect(result.output).toContain('not enabled');
  });
});
