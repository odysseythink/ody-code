import { describe, expect, it } from 'vitest';

import { createBuiltinHookRegistry } from '../../../src/session/hooks/builtin/registry';

describe('createBuiltinHookRegistry', () => {
  it('exposes the builtin hooks', () => {
    const registry = createBuiltinHookRegistry();
    expect(registry.ids()).toEqual([
      'edit-accumulator',
      'stop-format-typecheck',
      'session-memory-writer',
      'secret-leak-scanner',
    ]);
    expect(registry.get('edit-accumulator')?.id).toBe('edit-accumulator');
    expect(registry.get('stop-format-typecheck')?.id).toBe('stop-format-typecheck');
    expect(registry.get('session-memory-writer')?.id).toBe('session-memory-writer');
    expect(registry.get('secret-leak-scanner')?.id).toBe('secret-leak-scanner');
    expect(registry.get('unknown')).toBeUndefined();
  });
});
