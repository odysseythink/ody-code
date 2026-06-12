import { describe, expect, it, vi } from 'vitest';

import { withCheckpointSaveRetry } from '../../../src/session/checkpoint/save-retry';

function makeNoSpaceError(): NodeJS.ErrnoException {
  const error = new Error('No space left on device') as NodeJS.ErrnoException;
  error.code = 'ENOSPC';
  return error;
}

describe('withCheckpointSaveRetry', () => {
  it('returns the result of a successful save', async () => {
    const save = vi.fn().mockResolvedValue('ok');
    const cleanup = vi.fn().mockResolvedValue(undefined);

    const result = await withCheckpointSaveRetry(save, cleanup);

    expect(result).toBe('ok');
    expect(save).toHaveBeenCalledTimes(1);
    expect(cleanup).not.toHaveBeenCalled();
  });

  it('retries after ENOSPC and runs cleanup', async () => {
    const save = vi
      .fn()
      .mockRejectedValueOnce(makeNoSpaceError())
      .mockResolvedValueOnce('recovered');
    const cleanup = vi.fn().mockResolvedValue(undefined);

    const result = await withCheckpointSaveRetry(save, cleanup);

    expect(result).toBe('recovered');
    expect(save).toHaveBeenCalledTimes(2);
    expect(cleanup).toHaveBeenCalledTimes(1);
  });

  it('throws the last error when retries are exhausted', async () => {
    const save = vi.fn().mockRejectedValue(makeNoSpaceError());
    const cleanup = vi.fn().mockResolvedValue(undefined);

    await expect(withCheckpointSaveRetry(save, cleanup, { maxRetries: 2 })).rejects.toMatchObject({
      code: 'ENOSPC',
    });
    expect(save).toHaveBeenCalledTimes(3);
    expect(cleanup).toHaveBeenCalledTimes(2);
  });

  it('does not retry non-ENOSPC errors', async () => {
    const save = vi.fn().mockRejectedValue(new Error('boom'));
    const cleanup = vi.fn().mockResolvedValue(undefined);

    await expect(withCheckpointSaveRetry(save, cleanup)).rejects.toThrow('boom');
    expect(save).toHaveBeenCalledTimes(1);
    expect(cleanup).not.toHaveBeenCalled();
  });
});
