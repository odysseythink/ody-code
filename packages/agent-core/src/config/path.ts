import { mkdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'pathe';

export function resolveOdyHome(homeDir?: string | undefined): string {
  return homeDir ?? process.env['ODY_CODE_HOME'] ?? join(homedir(), '.ody-code');
}

export function resolveConfigPath(input: {
  readonly homeDir?: string | undefined;
  readonly configPath?: string | undefined;
}): string {
  return input.configPath ?? join(resolveOdyHome(input.homeDir), 'config.toml');
}

export function ensureOdyHome(homeDir: string): void {
  mkdirSync(homeDir, { recursive: true, mode: 0o700 });
}
