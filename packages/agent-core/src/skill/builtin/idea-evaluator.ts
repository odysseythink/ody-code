import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import IDEA_EVALUATOR_BODY from './idea-evaluator.md';

const PSEUDO_PATH = 'builtin://idea-evaluator';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/idea-evaluator.md',
  skillDirName: 'idea-evaluator',
  source: 'builtin',
  text: IDEA_EVALUATOR_BODY,
});

export const IDEA_EVALUATOR_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
