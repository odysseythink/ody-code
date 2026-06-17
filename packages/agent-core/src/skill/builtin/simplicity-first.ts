import { ErrorCodes } from '../../errors/codes';
import { OdyError } from '../../errors/classes';
import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import SIMPLICITY_FIRST_BODY from './simplicity-first.md';

// ---- Types ----

export type SimplicityLevel = 'lite' | 'full' | 'ultra';

// ---- Skill definition (following builtin skill pattern) ----

const PSEUDO_PATH = 'builtin://simplicity-first';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/simplicity-first.md',
  skillDirName: 'simplicity-first',
  source: 'builtin',
  text: SIMPLICITY_FIRST_BODY,
});

export const SIMPLICITY_FIRST_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};

// ---- Level parsing ----

/** Parse a simplicity level from slash-command arguments. */
export function parseSimplicityLevel(rawArgs: string): SimplicityLevel {
  const trimmed = rawArgs.trim();
  if (trimmed.length === 0) return 'full';
  const lower = trimmed.toLowerCase();
  if (lower === 'lite' || lower === 'full' || lower === 'ultra') {
    return lower;
  }
  throw new OdyError(
    ErrorCodes.REQUEST_INVALID,
    `Invalid simplicity level "${rawArgs.trim()}". Use: lite, full, or ultra.`,
  );
}

// ---- Level filtering ----

// Matches opening tags: <!-- LITE[ -->, <!--    FULL   [ -->
const OPEN_RE = /<!--\s*(LITE|FULL|ULTRA)\s*\[[^>]*?-->/gi;
// Matches closing tags: <!-- ]LITE -->, <!-- ] FULL -->
const CLOSE_RE = /<!--\s*\]\s*(LITE|FULL|ULTRA)\s*-->/gi;

/**
 * Strip level-specific HTML-comment blocks from a skill body.
 *
 * Blocks tagged for `level` (or no tag) are kept; blocks tagged for other
 * levels are removed.  Content outside any level block is always preserved.
 *
 * Nesting is NOT supported: an open tag encountered inside another block is
 * treated as literal text (ignored for filtering).  Only top-level blocks
 * (those not nested inside another level block) are filtered.
 */
export function filterSimplicityLevels(body: string, level: SimplicityLevel): string {
  const out: string[] = [];
  let cursor = 0;
  // Tracks the level name of each currently-open block at the top nesting level.
  const blockLevels: string[] = [];

  while (cursor < body.length) {
    OPEN_RE.lastIndex = 0;
    CLOSE_RE.lastIndex = 0;

    const remaining = body.slice(cursor);
    const nextOpen = OPEN_RE.exec(remaining);
    const nextClose = CLOSE_RE.exec(remaining);

    const openPos = nextOpen !== null ? cursor + nextOpen.index : Infinity;
    const closePos = nextClose !== null ? cursor + nextClose.index : Infinity;

    if (openPos === Infinity && closePos === Infinity) {
      // No more tags — flush remainder if not inside a discarding block
      if (!blockLevels.some(l => l !== level)) {
        out.push(body.slice(cursor));
      }
      break;
    }

    if (openPos <= closePos) {
      // nextOpen is guaranteed non-null when openPos !== Infinity
      const openMatch = nextOpen!;
      // Emit content before the open tag (unless currently discarding)
      const discarding = blockLevels.some(l => l !== level);
      if (!discarding) {
        out.push(body.slice(cursor, openPos));
      }

      const tagLevel = (openMatch[1] ?? 'full').toLowerCase();
      const matchLen = openMatch[0].length;
      cursor = openPos + matchLen;

      // Only process this tag if at the outermost level (no enclosing block).
      // Tags encountered inside another block are treated as literal text.
      if (blockLevels.length === 0) {
        blockLevels.push(tagLevel);
      } else if (!discarding) {
        // Inside another block — preserve the open tag text as literal content
        out.push(body.slice(openPos, cursor));
      }
    } else {
      // nextClose is guaranteed non-null when closePos !== Infinity
      const closeMatch = nextClose!;
      // Emit content before the close tag (unless currently discarding)
      const discarding = blockLevels.some(l => l !== level);
      if (!discarding) {
        out.push(body.slice(cursor, closePos));
      }

      const tagLevel = (closeMatch[1] ?? 'full').toLowerCase();
      const matchLen = closeMatch[0].length;
      cursor = closePos + matchLen;

      // Only close if the tag matches the current top-of-stack level.
      if (blockLevels.length > 0 && blockLevels[blockLevels.length - 1] === tagLevel) {
        blockLevels.pop();
      } else if (!discarding) {
        // Orphan or mismatched close tag — preserve the tag text as literal content
        out.push(body.slice(closePos, cursor));
      }
    }
  }

  return out.join('');
}
