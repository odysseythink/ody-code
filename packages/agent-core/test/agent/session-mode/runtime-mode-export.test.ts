import { expect, it } from 'vitest';
import { isRuntimeMode, normalizeRuntimeMode } from '../../../src/agent/session-mode';
import { Agent } from '../../../src/agent';

it('RuntimeMode is exported and includes all five values', () => {
  expect(isRuntimeMode('normal')).toBe(true);
  expect(isRuntimeMode('plan')).toBe(true);
  expect(isRuntimeMode('design')).toBe(true);
  expect(isRuntimeMode('product')).toBe(true);
  expect(isRuntimeMode('game-design')).toBe(true);
  expect(isRuntimeMode('foo')).toBe(false);
  expect(normalizeRuntimeMode('foo')).toBe('normal');
});

it('Agent namespace exposes RuntimeMode', () => {
  type T = Agent.RuntimeMode;
  const check: T = 'normal';
  expect(check).toBe('normal');
});
