import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import IDEA_GENERATOR_BODY from './idea-generator.md';

const PSEUDO_PATH = 'builtin://idea-generator';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/idea-generator.md',
  skillDirName: 'idea-generator',
  source: 'builtin',
  text: IDEA_GENERATOR_BODY,
});

export const IDEA_GENERATOR_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
