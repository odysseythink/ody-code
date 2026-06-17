import type { OdyErrorCode } from './codes';

export interface OdyErrorOptions {
  /** JSON-serializable structured details. */
  readonly details?: Record<string, unknown>;
  /** Original error or value. Local-only; never serialized to the wire. */
  readonly cause?: unknown;
}

/**
 * The single Kimi error class.
 *
 * Discrimination is always by `code`. Cross-process consumers receive
 * `OdyErrorPayload` and must branch on `code` rather than class identity.
 */
export class OdyError extends Error {
  readonly code: OdyErrorCode;
  readonly details?: Record<string, unknown>;
  override readonly cause?: unknown;

  constructor(code: OdyErrorCode, message: string, options: OdyErrorOptions = {}) {
    super(message);
    this.name = 'OdyError';
    this.code = code;
    this.details = options.details;
    this.cause = options.cause;
  }
}
