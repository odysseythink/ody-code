import { describe, it, expect } from 'vitest';
import { isCommandVisibleInMode } from '../../../src/tui/commands/visibility';
import type { KimiSlashCommand } from '../../../src/tui/commands/types';

describe('isCommandVisibleInMode with game-design', () => {
  it('hides most commands in game-design mode (like product)', () => {
    const cmd: Pick<KimiSlashCommand, 'hiddenInModes'> = {
      hiddenInModes: ['product', 'game-design'],
    };
    expect(isCommandVisibleInMode(cmd, 'game-design')).toBe(false);
  });

  it('shows /exit in game-design mode', () => {
    const cmd: Pick<KimiSlashCommand, 'hiddenInModes'> = {};
    expect(isCommandVisibleInMode(cmd, 'game-design')).toBe(true);
  });

  it('shows commands without hiddenInModes in game-design mode', () => {
    const cmd: Pick<KimiSlashCommand, 'hiddenInModes'> = {
      hiddenInModes: [],
    };
    expect(isCommandVisibleInMode(cmd, 'game-design')).toBe(true);
  });
});
