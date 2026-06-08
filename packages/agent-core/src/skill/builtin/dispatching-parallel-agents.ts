import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import DISPATCHING_PARALLEL_AGENTS_BODY from './dispatching-parallel-agents.md';

const PSEUDO_PATH = 'builtin://dispatching-parallel-agents';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/dispatching-parallel-agents.md',
  skillDirName: 'dispatching-parallel-agents',
  source: 'builtin',
  text: DISPATCHING_PARALLEL_AGENTS_BODY,
});

export const DISPATCHING_PARALLEL_AGENTS_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
