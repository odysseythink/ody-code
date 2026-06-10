import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import SUBAGENT_DRIVEN_DEVELOPMENT_BODY from './subagent-driven-development.md';

const PSEUDO_PATH = 'builtin://subagent-driven-development';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/subagent-driven-development.md',
  skillDirName: 'subagent-driven-development',
  source: 'builtin',
  text: SUBAGENT_DRIVEN_DEVELOPMENT_BODY,
});

export const SUBAGENT_DRIVEN_DEVELOPMENT_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
    hiddenInModes: ['plan', 'design'],
  },
};
