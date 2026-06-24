// Barrel re-export so #/errors resolves to a single .ts file (the first
// entry in the package imports map). vitest does not resolve cleanly through
// the directory fallback; this thin barrel keeps the alias working uniformly
// across node, tsc, and vitest. Real module lives under @odysseythink/agent-core-shared.
export {
  ErrorCodes,
  ODY_ERROR_INFO,
  OdyError,
  fromOdyErrorPayload,
  isOdyError,
  makeErrorPayload,
  toOdyErrorPayload,
} from '@odysseythink/agent-core-shared';
export type {
  OdyErrorCode,
  OdyErrorInfo,
  OdyErrorOptions,
  OdyErrorPayload,
} from '@odysseythink/agent-core-shared';
