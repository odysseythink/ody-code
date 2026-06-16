import { describe, expect, it, vi } from 'vitest';
import { persistModelSelection } from '../../../src/tui/commands/config';
import type { SlashCommandHost } from '../../../src/tui/commands/dispatch';

function createMockHost(
  config: { defaultModel?: string; defaultThinking?: boolean; modeModels?: { plan?: string; design?: string } },
  sessionMode: 'normal' | 'plan' | 'design' | 'office-hours',
  
) {
  return {
    state: {
      appState: { sessionMode },
    },
    harness: {
      getConfig: vi.fn().mockResolvedValue(config),
      setConfig: vi.fn().mockResolvedValue(undefined),
    },
  } as unknown as SlashCommandHost;
}

describe('persistModelSelection per-mode', () => {
  it('saves to defaultModel in build mode', async () => {
    const host = createMockHost({ defaultModel: 'old-model', defaultThinking: false }, 'normal');
    const persisted = await persistModelSelection(host, 'new-model', true);
    expect(persisted).toBe(true);
    expect(host.harness.setConfig).toHaveBeenCalledWith({
      defaultModel: 'new-model',
      defaultThinking: true,
    });
  });

  it('saves to modeModels.plan in plan mode', async () => {
    const host = createMockHost({ defaultModel: 'build-model', defaultThinking: false }, 'plan');
    const persisted = await persistModelSelection(host, 'plan-model', true);
    expect(persisted).toBe(true);
    expect(host.harness.setConfig).toHaveBeenCalledWith({
      modeModels: { plan: 'plan-model' },
      defaultThinking: true,
    });
  });

  it('saves to modeModels.design in design mode', async () => {
    const host = createMockHost(
      { defaultModel: 'build-model', modeModels: { plan: 'plan-model' }, defaultThinking: false },
      'design',
    );
    const persisted = await persistModelSelection(host, 'design-model', false);
    expect(persisted).toBe(true);
    expect(host.harness.setConfig).toHaveBeenCalledWith({
      modeModels: { plan: 'plan-model', design: 'design-model' },
      defaultThinking: false,
    });
  });

  it('returns false when nothing changed in build mode', async () => {
    const host = createMockHost({ defaultModel: 'same-model', defaultThinking: true }, 'normal');
    const persisted = await persistModelSelection(host, 'same-model', true);
    expect(persisted).toBe(false);
    expect(host.harness.setConfig).not.toHaveBeenCalled();
  });
});
