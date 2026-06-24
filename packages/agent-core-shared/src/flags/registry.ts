import type { FlagDefinitionInput } from './types';

/**
 * Experimental feature flags. Empty by default — there are no experimental features yet.
 *
 * To add one, append an entry and gate the feature with `flags.enabled('my-feature')`:
 *   { id: 'my-feature', env: 'ODY_CODE_EXPERIMENTAL_MY_FEATURE', default: false, surface: 'both' }
 *
 * Keep the `as const satisfies` — it derives the literal `FlagId` union that gives `enabled()`
 * autocomplete and typo-checking. `env` must start with 'ODY_CODE_EXPERIMENTAL_', be unique, and
 * not equal the master switch 'ODY_CODE_EXPERIMENTAL_FLAG'; `id` must not be 'flag'.
 */
export const FLAG_DEFINITIONS = [
  {
    id: 'goal-command',
    env: 'ODY_CODE_EXPERIMENTAL_GOAL_COMMAND',
    default: false,
    surface: 'both',
  },
  {
    id: 'micro-compaction',
    env: 'ODY_CODE_EXPERIMENTAL_MICRO_COMPACTION',
    default: false,
    surface: 'core',
  },
  {
    id: 'background-ask',
    env: 'ODY_CODE_EXPERIMENTAL_BACKGROUND_ASK',
    default: false,
    surface: 'core',
  },
  {
    id: 'repo-knowledge',
    env: 'ODY_CODE_EXPERIMENTAL_REPO_KNOWLEDGE',
    default: false,
    surface: 'core',
  },
  {
    id: 'wasm-tokenizer',
    env: 'ODY_CODE_EXPERIMENTAL_WASM_TOKENIZER',
    default: false,
    surface: 'core',
  },
  {
    id: 'wasm-diff',
    env: 'ODY_CODE_EXPERIMENTAL_WASM_DIFF',
    default: true,
    surface: 'core',
  },
  {
    id: 'wasm-glob',
    env: 'ODY_CODE_EXPERIMENTAL_WASM_GLOB',
    default: false,
    surface: 'core',
  },
] as const satisfies readonly FlagDefinitionInput[];

/** Literal union of registered flag ids. */
export type FlagId = (typeof FLAG_DEFINITIONS)[number]['id'];
