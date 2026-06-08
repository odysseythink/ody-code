import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import RECEIVING_CODE_REVIEW_BODY from './receiving-code-review.md';

const PSEUDO_PATH = 'builtin://receiving-code-review';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/receiving-code-review.md',
  skillDirName: 'receiving-code-review',
  source: 'builtin',
  text: RECEIVING_CODE_REVIEW_BODY,
});

export const RECEIVING_CODE_REVIEW_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
