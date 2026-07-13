import { HOOK_PROFILES, type HookDef, type HookProfile } from './types';

const DEFAULT_PROFILE: HookProfile = 'strict';
const PROFILE_ENV = 'ODY_CODE_HOOK_PROFILE';
const DISABLED_ENV = 'ODY_CODE_DISABLED_HOOKS';

export interface ProfileGate {
  readonly profile: HookProfile;
  readonly disabled: ReadonlySet<string>;
  isEnabled(hook: HookDef): boolean;
  isExplicitlyDisabled(hook: HookDef): boolean;
}

function normalizeProfile(value: string | undefined): HookProfile {
  if (value === undefined) return DEFAULT_PROFILE;
  if ((HOOK_PROFILES as readonly string[]).includes(value)) return value as HookProfile;
  return DEFAULT_PROFILE;
}

function disabledSet(env: Readonly<Record<string, string | undefined>>): Set<string> {
  const raw = env[DISABLED_ENV] ?? '';
  return new Set(
    raw
      .split(',')
      .map((s) => s.trim().toLowerCase())
      .filter((s) => s.length > 0),
  );
}

function hookKey(hook: HookDef): string {
  return (
    hook.id ??
    hook.builtin ??
    hook.command ??
    hook.commands?.join('\x00') ??
    ''
  ).toLowerCase();
}

export function createProfileGate(
  env: Readonly<Record<string, string | undefined>> = process.env,
): ProfileGate {
  const profile = normalizeProfile(env[PROFILE_ENV]);
  const disabled = disabledSet(env);

  return {
    profile,
    disabled,
    isEnabled(hook: HookDef): boolean {
      if (hook.profiles !== undefined && !hook.profiles.includes(profile)) {
        return false;
      }
      return !this.isExplicitlyDisabled(hook);
    },
    isExplicitlyDisabled(hook: HookDef): boolean {
      const key = hookKey(hook);
      return key.length > 0 && disabled.has(key);
    },
  };
}
