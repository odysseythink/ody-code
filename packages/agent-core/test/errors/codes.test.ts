import { describe, expect, it } from 'vitest';

import {
  ErrorCodes,
  fromOdyErrorPayload,
  OdyError,
  ODY_ERROR_INFO,
  toOdyErrorPayload,
} from '../../src/errors';

describe('worker/transport error codes', () => {
  it('exposes worker and transport codes', () => {
    expect(ErrorCodes.WORKER_SPAWN_FAILED).toBe('worker.spawn_failed');
    expect(ErrorCodes.WORKER_EXITED).toBe('worker.exited');
    expect(ErrorCodes.TRANSPORT_CLOSED).toBe('transport.closed');
  });

  it('has metadata for every new code', () => {
    const codes = [ErrorCodes.WORKER_SPAWN_FAILED, ErrorCodes.WORKER_EXITED, ErrorCodes.TRANSPORT_CLOSED];
    for (const code of codes) {
      const info = ODY_ERROR_INFO[code];
      expect(info).toBeDefined();
      expect(info.title).toBeTruthy();
      expect(typeof info.retryable).toBe('boolean');
      expect(typeof info.public).toBe('boolean');
    }
  });

  it('round-trips through OdyError payload', () => {
    const error = new OdyError(ErrorCodes.WORKER_EXITED, 'worker died');
    const payload = toOdyErrorPayload(error);
    expect(payload.code).toBe('worker.exited');
    expect(payload.retryable).toBe(false);

    const restored = fromOdyErrorPayload(payload);
    expect(restored.code).toBe('worker.exited');
    expect(restored.message).toBe('worker died');
  });
});
