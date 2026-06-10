import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import TEST_DRIVEN_DEVELOPMENT_BODY from './test-driven-development.md';

const PSEUDO_PATH = 'builtin://test-driven-development';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/test-driven-development.md',
  skillDirName: 'test-driven-development',
  source: 'builtin',
  text: TEST_DRIVEN_DEVELOPMENT_BODY,
});

export const TEST_DRIVEN_DEVELOPMENT_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
    hiddenInModes: ['plan', 'design'],
  },
};
