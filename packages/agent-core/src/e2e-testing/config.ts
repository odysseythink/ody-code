import { E2EConfigSchema, type KimiConfig } from '#/config/schema';
import { E2EConfigValidationError } from './errors';
import type { ResolvedE2EConfig } from './errors';
export type { ResolvedE2EConfig } from './errors';

export class E2EConfigResolver {
  static resolve(kimiConfig: KimiConfig): ResolvedE2EConfig {
    const raw = kimiConfig.e2e ?? {};
    try {
      return E2EConfigSchema.parse(raw) as ResolvedE2EConfig;
    } catch (error) {
      throw new E2EConfigValidationError(
        error instanceof Error ? error.message : 'Invalid e2e config',
        error,
      );
    }
  }
}
