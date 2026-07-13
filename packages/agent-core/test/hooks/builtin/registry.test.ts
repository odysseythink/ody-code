import { describe, expect, it } from 'vitest';

import { createBuiltinHookRegistry } from '../../../src/session/hooks/builtin/registry';

describe('createBuiltinHookRegistry', () => {
  it('exposes edit-accumulator and stop-format-typecheck', () => {
    const registry = createBuiltinHookRegistry();
    expect(registry.ids()).toEqual(['edit-accumulator', 'stop-format-typecheck']);
    expect(registry.get('edit-accumulator')?.id).toBe('edit-accumulator');
    expect(registry.get('stop-format-typecheck')?.id).toBe('stop-format-typecheck');
    expect(registry.get('unknown')).toBeUndefined();
  });
});
