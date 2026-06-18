import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import type { CodeReviewFinding, CodeReviewReport } from './types';

// ─── Types ────────────────────────────────────────────────────────────────────

export type SimplicityTag = 'delete' | 'stdlib' | 'native' | 'yagni' | 'shrink';

export interface RepoAuditDigest {
  readonly workspaceDir: string;
  readonly fileCount: number;
  readonly files: readonly string[];
  readonly dependencies: readonly string[];
  readonly snippets: readonly FileSnippet[];
}

export interface FileSnippet {
  readonly path: string;
  readonly lines: string;
}

// ─── Constants ────────────────────────────────────────────────────────────────

const ALL_TAGS: readonly SimplicityTag[] = ['delete', 'stdlib', 'native', 'yagni', 'shrink'];
const TAG_ALTERNATION = ALL_TAGS.join('|');

const TAG_TO_SEVERITY: Record<SimplicityTag, CodeReviewFinding['severity']> = {
  delete: 'important',
  stdlib: 'important',
  native: 'important',
  yagni: 'important',
  shrink: 'minor',
};

// ─── Parser ───────────────────────────────────────────────────────────────────

/**
 * Parse an LLM output in Ponytail simplicity review format into a CodeReviewReport.
 *
 * Ponytail format: `<file>:L<line>: <tag> <what>. <replacement>.`
 * When no findings: `Lean already. Ship.`
 * Final line: `net: -<N> lines possible.` (audit also: `, -<M> deps possible.`)
 */
export function parseSimplicityReport(raw: string, reviewerAlias: string): CodeReviewReport {
  const trimmed = raw.trim();
  if (trimmed.length === 0) {
    return { ok: true, reviewerAlias, findings: [] };
  }

  // Check for "Lean already. Ship." as the whole output
  if (/^Lean already\.\s*Ship\.?\s*$/i.test(trimmed)) {
    return { ok: true, reviewerAlias, findings: [], summary: 'Lean already. Ship.' };
  }

  const lines = trimmed.split('\n');
  const findings: CodeReviewFinding[] = [];
  let summary: string | undefined;

  for (const line of lines) {
    const trimmedLine = line.trim();
    if (trimmedLine.length === 0) continue;

    // Extract net summary line
    const netMatch = /^net:\s*(-?\d+)\s*(?:lines?|deps?).*$/.exec(trimmedLine);
    if (netMatch !== null) {
      summary = trimmedLine;
      continue;
    }

    // Also check for Lean already mid-output
    if (/^Lean already\.\s*Ship\.?\s*$/i.test(trimmedLine)) {
      if (findings.length === 0) {
        return { ok: true, reviewerAlias, findings: [], summary: 'Lean already. Ship.' };
      }
      continue;
    }

    const finding = parseSimplicityLine(trimmedLine);
    if (finding !== null) {
      findings.push(finding);
    }
  }

  return { ok: true, reviewerAlias, summary, findings };
}

function parseSimplicityLine(line: string): CodeReviewFinding | null {
  // Step 1: try to strip optional location prefix `<file>:L<line>:`
  let rest = line;
  let file: string | undefined;
  let lineno: string | undefined;

  const locationMatch = /^(.+?):L(\d+):\s*/.exec(line);
  if (locationMatch !== null) {
    const afterPrefix = line.slice(locationMatch[0].length);
    // Verify what follows starts with a known tag followed by ':'
    const tagCheckRe = new RegExp(`^(?:${TAG_ALTERNATION}):\\s`);
    if (tagCheckRe.test(afterPrefix)) {
      file = locationMatch[1];
      lineno = locationMatch[2];
      rest = afterPrefix;
    }
    // else: the L<num>: was not a Ponytail location prefix; treat whole line as rest
  }

  // Also handle `L<line>:` without file prefix
  if (file === undefined) {
    const bareLocationMatch = /^L(\d+):\s*/.exec(rest);
    if (bareLocationMatch !== null) {
      const afterPrefix = rest.slice(bareLocationMatch[0].length);
      const tagCheckRe = new RegExp(`^(?:${TAG_ALTERNATION}):\\s`);
      if (tagCheckRe.test(afterPrefix)) {
        lineno = bareLocationMatch[1];
        rest = afterPrefix;
      }
    }
  }

  // Step 2: parse tag
  const tagRe = new RegExp(`^(?:${TAG_ALTERNATION}):\\s*`);
  const tagMatch = tagRe.exec(rest);
  if (tagMatch === null) return null;
  const tag = tagMatch[0].replace(/:\s*$/, '').replace(/:$/, '').trim() as SimplicityTag;
  if (!ALL_TAGS.includes(tag)) return null;
  const body = rest.slice(tagMatch[0].length);

  // Step 3: split on first '. ' into what / replacement
  const dotIdx = body.indexOf('. ');
  if (dotIdx < 0) return null;
  const what = body.slice(0, dotIdx).trim();
  let replacement = body.slice(dotIdx + 2).trim();

  // Extract trailing [path] from audit-format findings (e.g., "Use structuredClone. [src/a.ts]")
  let trailingPath: string | undefined;
  const pathMatch = replacement.match(/\s*\[([^\]]+)\]$/);
  if (pathMatch !== null) {
    trailingPath = pathMatch[1];
    replacement = replacement.slice(0, pathMatch.index).trim();
  }

  // Strip trailing dot
  if (replacement.endsWith('.')) {
    replacement = replacement.slice(0, -1);
  }

  const location = file !== undefined && lineno !== undefined
    ? `${file}:${lineno}`
    : lineno !== undefined
      ? `:${lineno}`
      : trailingPath !== undefined
        ? trailingPath
        : undefined;

  return {
    severity: TAG_TO_SEVERITY[tag],
    title: `[${tag.toUpperCase()}] ${what}`,
    detail: `${tag}: ${what}. ${replacement}.`,
    location,
    suggestedFix: replacement,
  };
}

// ─── Prompt Builders ──────────────────────────────────────────────────────────

export function buildSimplicityReviewPrompt(
  diff: string,
  description: string | undefined,
  requirements: string | undefined,
): string {
  const tagsDoc = ALL_TAGS.map((t) => `  - \`${t}:\` — ${tagDescription(t)}`).join('\n');

  return [
    'You are an anti-over-engineering reviewer. Hunt unnecessary complexity. Never report correctness bugs, security vulnerabilities, or performance issues — those belong to a normal code review.',
    '',
    '## Context',
    description ? `What was built: ${description}` : 'What was built: [not provided]',
    requirements ? `Requirements: ${requirements}` : 'Requirements: [not provided]',
    '',
    '## Diff',
    '```diff',
    diff,
    '```',
    '',
    '## Your Task',
    'Review the diff line by line. For each finding, output exactly one line in this format:',
    '`<file>:L<line>: <tag> <current state>. <simpler replacement>.`',
    '',
    'Tags (pick the best match):',
    tagsDoc,
    '',
    '## Rules',
    '- Only report unnecessary complexity — dead code, over-abstraction, things the standard library or platform already does.',
    '- Do NOT report correctness bugs, security flaws, or performance problems.',
    '- If there is nothing to simplify, output exactly: `Lean already. Ship.`',
    '- If you find something that was deliberately kept simple and could use an `ody:` annotation, suggest adding `// ody: <ceiling>, <upgrade trigger>` in the detail — but do not create a finding for it.',
    '',
    '## Output format',
    'Each finding on its own line:',
    '`<file>:L<line>: <tag> <current state>. <simpler replacement>.`',
    '',
    'End with:',
    '`net: -<N> lines possible.`',
    '',
    'If nothing to report:',
    '`Lean already. Ship.`',
  ].join('\n');
}

export function buildSimplicityAuditPrompt(digest: RepoAuditDigest): string {
  const tagsDoc = ALL_TAGS.map((t) => `  - \`${t}:\` — ${tagDescription(t)}`).join('\n');
  const fileList = digest.files.join('\n');
  const depList = digest.dependencies.join(', ');
  const snippetText = digest.snippets
    .map((s) => `### ${s.path}\n\`\`\`\n${s.lines}\n\`\`\``)
    .join('\n\n');

  return [
    'You are an anti-over-engineering auditor. Hunt unnecessary complexity across the entire repository. Never report correctness bugs, security vulnerabilities, or performance issues.',
    '',
    '## Repository Snapshot',
    `Workspace: ${digest.workspaceDir}`,
    `Files scanned: ${digest.fileCount}`,
    '',
    '### File List',
    fileList,
    '',
    '### Dependencies',
    depList,
    '',
    '### Code Snippets',
    snippetText,
    '',
    '## Your Task',
    'Scan the repository for over-engineering. Rank findings by lines-of-code that can be eliminated (highest first).',
    'For each finding, output exactly one line:',
    '`<tag> <current state>. <simpler replacement>. [<file path>]`',
    '',
    'Tags:',
    tagsDoc,
    '',
    '## Rules',
    '- Only report unnecessary complexity.',
    '- Do NOT report correctness bugs, security flaws, or performance problems.',
    '- Prefer findings with the largest code-elimination impact first.',
    '- If nothing to simplify, output: `Lean already. Ship.`',
    '',
    '## Output format',
    '`<tag> <current state>. <simpler replacement>. [path]`',
    '',
    'End with:',
    '`net: -<N> lines, -<M> deps possible.`',
    '',
    'If nothing to report:',
    '`Lean already. Ship.`',
  ].join('\n');
}

function tagDescription(tag: SimplicityTag): string {
  switch (tag) {
    case 'delete': return 'Code that can be deleted entirely.';
    case 'stdlib': return 'Custom implementation of something the standard library already provides.';
    case 'native': return 'Custom implementation of something the platform/runtime already provides.';
    case 'yagni': return 'Premature abstraction or future-proofing that is not needed now.';
    case 'shrink': return 'Code that works but can be significantly shortened without losing clarity.';
  }
}

// ─── Audit Scanner ────────────────────────────────────────────────────────────

const MAX_AUDIT_FILES = 200;
const MAX_SNIPPET_BYTES = 2048;
const MAX_SNIPPETS = 30;

const EXCLUDED_DIRS = new Set([
  '.git', 'node_modules', 'dist', 'build', '.next', '.nuxt',
  '__pycache__', '.venv', 'venv', 'target', 'coverage',
]);

const SOURCE_EXTENSIONS = new Set([
  '.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs',
  '.py', '.rs', '.go', '.rb', '.java', '.kt', '.swift',
  '.css', '.scss', '.less',
]);

export function buildAuditDigest(
  workspaceDir: string,
  signal?: AbortSignal,
): RepoAuditDigest {
  const allFiles: string[] = [];

  function walk(dir: string) {
    if (signal?.aborted) return;
    let entries: readonly { isDirectory(): boolean; isFile(): boolean; name: string }[];
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (signal?.aborted) return;
      if (entry.isDirectory()) {
        if (EXCLUDED_DIRS.has(entry.name)) continue;
        if (entry.name.startsWith('.')) continue;
        walk(join(dir, entry.name));
      } else if (entry.isFile()) {
        const ext = entry.name.includes('.') ? entry.name.slice(entry.name.lastIndexOf('.')) : '';
        if (SOURCE_EXTENSIONS.has(ext) || entry.name === 'package.json') {
          allFiles.push(join(dir, entry.name));
        }
      }
    }
  }

  walk(workspaceDir);

  allFiles.sort((a, b) => {
    try {
      return statSync(b).mtimeMs - statSync(a).mtimeMs;
    } catch {
      return 0;
    }
  });

  const capped = allFiles.slice(0, MAX_AUDIT_FILES);
  const relativeFiles = capped.map((f) => relative(workspaceDir, f));

  const dependencies: string[] = [];
  try {
    const pkgPath = join(workspaceDir, 'package.json');
    const pkgRaw = readFileSync(pkgPath, 'utf-8');
    const pkg = JSON.parse(pkgRaw) as { dependencies?: Record<string, string>; devDependencies?: Record<string, string> };
    for (const key of Object.keys(pkg.dependencies ?? {})) dependencies.push(key);
    for (const key of Object.keys(pkg.devDependencies ?? {})) dependencies.push(key);
  } catch {
    // no package.json or unparseable — ok
  }

  const snippets: FileSnippet[] = [];
  for (const file of capped) {
    if (snippets.length >= MAX_SNIPPETS) break;
    try {
      const fd = readFileSync(file, 'utf-8');
      const bytes = fd.slice(0, MAX_SNIPPET_BYTES);
      const lines = bytes.split('\n').slice(0, 30).join('\n');
      if (lines.trim().length > 0) {
        snippets.push({ path: relative(workspaceDir, file), lines });
      }
    } catch {
      // skip unreadable files
    }
  }

  return {
    workspaceDir,
    fileCount: capped.length,
    files: relativeFiles,
    dependencies,
    snippets,
  };
}
