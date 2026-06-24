import { homedir } from 'node:os';
import { join } from 'pathe';

export function resolveOdyHome(homeDir?: string | undefined): string {
  return homeDir ?? process.env['ODY_CODE_HOME'] ?? join(homedir(), '.ody-code');
}
