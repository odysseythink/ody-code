import { describe, expect, it } from 'vitest';

import {
  abortError,
  isUserCancellation,
  userCancellationReason,
  UserCancellationError,
} from '../src/abort';

describe('userCancellationReason', () => {
  it('is recognised as a deliberate user cancellation', () => {
    expect(isUserCancellation(userCancellationReason())).toBe(true);
  });

  it('stays an AbortError so abort detection keeps treating it as an abort', () => {
    expect(userCancellationReason().name).toBe('AbortError');
  });

  it('is distinguishable from a generic abort, an ordinary error, and undefined', () => {
    // A generic abort (timeout, internal) must NOT read as a user cancellation —
    // that distinction is the whole point: the model needs to know a user
    // pressed stop, not that "something aborted".
    expect(isUserCancellation(abortError())).toBe(false);
    expect(isUserCancellation(new Error('boom'))).toBe(false);
    expect(isUserCancellation(undefined)).toBe(false);
  });

  it('UserCancellationError has userCancelled flag', () => {
    const err = new UserCancellationError();
    expect(err.userCancelled).toBe(true);
  });
});
