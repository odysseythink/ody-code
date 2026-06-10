import { describe, expect, it, vi } from 'vitest';

import { handleWritingPlanCommand } from '../../../src/tui/commands/writing-plan';
import type { SlashCommandHost } from '../../../src/tui/commands/dispatch';

function createHost(session: unknown): SlashCommandHost {
  return {
    session,
    showError: vi.fn(),
    showNotice: vi.fn(),
    setAppState: vi.fn(),
    sendNormalUserInput: vi.fn(),
  } as unknown as SlashCommandHost;
}

describe('handleWritingPlanCommand', () => {
  it('errors with usage when no file argument is given', async () => {
    const session = { writingPlan: vi.fn() };
    const host = createHost(session);

    await handleWritingPlanCommand(host, '   ');

    expect(host.showError).toHaveBeenCalledWith('用法: /writing-plan <文件路径>');
    expect(session.writingPlan).not.toHaveBeenCalled();
  });

  it('errors when there is no active session', async () => {
    const host = createHost(undefined);
    await handleWritingPlanCommand(host, 'designs/spec.md');
    expect(host.showError).toHaveBeenCalled();
  });

  it('locks the plan to the source file and kicks off the conversion turn', async () => {
    const session = {
      writingPlan: vi.fn().mockResolvedValue(undefined),
      getPlan: vi.fn().mockResolvedValue({ path: '/x/.ody-code/plans/spec.md' }),
    };
    const host = createHost(session);

    await handleWritingPlanCommand(host, 'designs/spec.md');

    expect(session.writingPlan).toHaveBeenCalledWith({ filePath: 'designs/spec.md' });
    expect(host.setAppState).toHaveBeenCalledWith({ sessionMode: 'plan' });
    expect(host.sendNormalUserInput).toHaveBeenCalledTimes(1);
    expect(host.sendNormalUserInput).toHaveBeenCalledWith(expect.stringContaining('designs/spec.md'));
    expect(host.showError).not.toHaveBeenCalled();
  });

  it('surfaces a missing-file error from the agent', async () => {
    const session = {
      writingPlan: vi.fn().mockRejectedValue(new Error('文件不存在: designs/nope.md')),
    };
    const host = createHost(session);

    await handleWritingPlanCommand(host, 'designs/nope.md');

    expect(host.showError).toHaveBeenCalledWith(expect.stringContaining('writing-plan 失败'));
    expect(host.sendNormalUserInput).not.toHaveBeenCalled();
  });
});
