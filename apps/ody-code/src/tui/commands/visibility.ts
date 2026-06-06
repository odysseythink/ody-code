import type { KimiSlashCommand, SessionMode } from './types';

export function isCommandVisibleInMode(
  command: Pick<KimiSlashCommand, 'hiddenInModes'>,
  mode: SessionMode,
): boolean {
  if (command.hiddenInModes === undefined || command.hiddenInModes.length === 0) {
    return true;
  }
  return !command.hiddenInModes.includes(mode);
}
