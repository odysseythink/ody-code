import { describe, it, expect, vi } from 'vitest';
import {
  SESSION_MODE_KINDS,
  RUNTIME_MODES,
  isSessionModeKind,
  isRuntimeMode,
  normalizeRuntimeMode,
} from '../types';
import type { SessionModeKind, RuntimeMode } from '../index';

describe('mode types', () => {
  it('SESSION_MODE_KINDS has exactly the four interaction phases', () => {
    expect(SESSION_MODE_KINDS).toEqual(['plan', 'design', 'office-hours', 'game-design']);
  });

  it('RUNTIME_MODES appends normal to session mode kinds', () => {
    expect(RUNTIME_MODES).toEqual([...SESSION_MODE_KINDS, 'normal']);
  });

  it('isSessionModeKind accepts the four kinds and rejects others', () => {
    expect(isSessionModeKind('plan')).toBe(true);
    expect(isSessionModeKind('office-hours')).toBe(true);
    expect(isSessionModeKind('normal')).toBe(false);
    expect(isSessionModeKind('foo')).toBe(false);
  });

  it('isRuntimeMode accepts all runtime modes and rejects others', () => {
    for (const mode of RUNTIME_MODES) {
      expect(isRuntimeMode(mode)).toBe(true);
    }
    expect(isRuntimeMode('foo')).toBe(false);
    expect(isRuntimeMode('')).toBe(false);
  });

  it('normalizeRuntimeMode returns valid modes unchanged and warns on unknown', () => {
    const warn = vi.fn();
    expect(normalizeRuntimeMode('plan', warn)).toBe('plan');
    expect(normalizeRuntimeMode('normal', warn)).toBe('normal');
    expect(normalizeRuntimeMode('foo', warn)).toBe('normal');
    expect(warn).toHaveBeenCalledTimes(1);
    expect(warn).toHaveBeenCalledWith('Unknown runtime mode "foo", falling back to "normal"');
  });
});

describe('session-mode/index re-exports', () => {
  it('exports RuntimeMode and SessionModeKind from index', () => {
    const kind: SessionModeKind = 'plan';
    const mode: RuntimeMode = 'normal';
    expect(kind).toBe('plan');
    expect(mode).toBe('normal');
  });
});
