import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import ROADMAP_ARCHITECT_BODY from './roadmap-architect.md';

const PSEUDO_PATH = 'builtin://roadmap-architect';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/roadmap-architect.md',
  skillDirName: 'roadmap-architect',
  source: 'builtin',
  text: ROADMAP_ARCHITECT_BODY,
});

export const ROADMAP_ARCHITECT_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
