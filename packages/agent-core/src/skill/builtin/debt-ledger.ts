import type { SkillDefinition } from '../types';
import { parseSkillText } from '../parser';
import DEBT_LEDGER_BODY from './debt-ledger.md';

const PSEUDO_PATH = 'builtin://debt-ledger';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/debt-ledger.md',
  skillDirName: 'debt-ledger',
  source: 'builtin',
  text: DEBT_LEDGER_BODY,
});

export const DEBT_LEDGER_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
