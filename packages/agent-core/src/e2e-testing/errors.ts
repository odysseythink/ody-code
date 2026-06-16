import { ErrorCodes, KimiError } from '#/errors';
import type { E2EConfig } from '#/config/schema';

export class E2EConfigValidationError extends KimiError {
  constructor(message: string, cause?: unknown) {
    super(ErrorCodes.CONFIG_INVALID, message, { cause });
    this.name = 'E2EConfigValidationError';
  }
}

export class E2ENoMatchingGeneratorError extends KimiError {
  constructor(projectRoot: string) {
    super(ErrorCodes.CONFIG_INVALID, `No E2E generator matches the project at ${projectRoot}`);
    this.name = 'E2ENoMatchingGeneratorError';
  }
}

export interface ResolvedE2EConfig extends Required<E2EConfig> {}
