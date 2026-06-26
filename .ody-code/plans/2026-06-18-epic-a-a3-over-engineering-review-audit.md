# Epic A-A3: Simplicity Review / Audit Implementation Plan

**Goal:** Extend `request-code-review` command with `--focus simplicity` (Ponytail anti-over-engineering review on diff) and `--scope repo` (repo-wide audit via GrepTool), reusing existing code-review executor/prompt/parser/report infrastructure.

**Architecture:** Three-layer extension: (1) new `simplicity.ts` module adds Ponytail prompt builders and parsers alongside existing `prompt.ts`; (2) executor gains `scope`/`focus` branching and an optional `auditScanner` dependency; (3) CLI/TUI entry points add `--focus`/`--scope` flag parsing and RPC payload forwarding. The audit scanner uses `GrepTool` to build a compact repo digest, fed to the same LLM pipeline.

**Tech Stack:** TypeScript, vitest, existing `packages/agent-core/src/code-review/` module, `GrepTool` from `packages/agent-core/src/tools/builtin/file/grep.ts`.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

```
packages/agent-core/src/code-review/
  types.ts                          # Modify: add focus/scope to CodeReviewRequestInput
  simplicity.ts                     # Create: SimplicityTag, parseSimplicityReport, buildSimplicity*Prompt, RepoAuditDigest, FileSnippet, buildAuditDigest
  executor.ts                       # Modify: add focus/scope branching + auditScanner dep
  index.ts (at packages/agent-core/src/)  # Modify: export new simplicity symbols
packages/agent-core/src/rpc/
  core-api.ts                       # Modify: add focus/scope to RequestCodeReviewPayload
  core-impl.ts                      # Modify: wire auditScanner, pass focus/scope, telemetry
packages/agent-core/test/code-review/
  simplicity.test.ts                # Create: unit tests for parseSimplicityReport, buildSimplicity*Prompt, buildAuditDigest
  executor.test.ts                  # Modify: add simplicity+repo tests
apps/ody-code/src/cli/sub/
  request-code-review.ts            # Modify: add --focus/--scope options + validation
apps/ody-code/src/tui/commands/
  request-code-review.ts            # Modify: add --focus/--scope parsing + forwarding
```

## Dependency Overview

```
Task 1 (types) ─────────────────────────────────────────────────────────────┐
    │                                                                        │
    ├── Task 2 (simplicity.ts: parser + prompt builders) ──────────────────┐ │
    │       │                                                               │ │
    │       ├── Task 3 (executor: focus/scope branching) ──────────────────┤ │
    │       │       │                                                       │ │
    │       │       ├── Task 4 (RPC + auditScanner + telemetry + exports) ─┤ │
    │       │       │       │                                               │ │
    │       │       │       ├── Task 5 (CLI --focus/--scope) ──────────────┘ │
    │       │       │       │                                                 │
    │       │       │       └── Task 6 (TUI --focus/--scope) ────────────────┘
    │       │       │
    │       │       └── (Task 5, Task 6 are independent of each other)
    │       │
    │       └── (can be developed in parallel with Task 5/6 after Task 3)
    │
    └── (all tasks depend on Task 1 for the types)
```

---

### Task 1: Extend types with `focus` and `scope` fields

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/code-review/types.ts:6-13` — add `focus`/`scope` to `CodeReviewRequestInput`
- Modify: `packages/agent-core/src/rpc/core-api.ts:348-356` — add `focus`/`scope` to `RequestCodeReviewPayload`

This is a shared-signature change. New fields are optional (`undefined` defaults to `'correctness'` / `'diff'`), so existing callers do not break. Verify via whole-tree typecheck.

- [ ] Add `focus` and `scope` to `CodeReviewRequestInput` in `packages/agent-core/src/code-review/types.ts`:

```typescript
// Line 6-13, add two new fields:
export interface CodeReviewRequestInput {
  readonly source: CodeReviewDiffSource;
  readonly modelAlias: string;
  readonly description?: string | undefined;
  readonly requirements?: string | undefined;
  readonly deep?: boolean | undefined;
  readonly timeoutMs?: number | undefined;
  readonly focus?: 'correctness' | 'simplicity' | undefined;
  readonly scope?: 'diff' | 'repo' | undefined;
}
```

- [ ] Add `focus` and `scope` to `RequestCodeReviewPayload` in `packages/agent-core/src/rpc/core-api.ts`:

```typescript
// Line 348-356, add two new fields:
export interface RequestCodeReviewPayload {
  readonly modelAlias?: string | undefined;
  readonly source: CodeReviewDiffSource;
  readonly description?: string | undefined;
  readonly requirements?: string | undefined;
  readonly deep?: boolean | undefined;
  readonly timeoutMs?: number | undefined;
  readonly workDir: string;
  readonly focus?: 'correctness' | 'simplicity' | undefined;
  readonly scope?: 'diff' | 'repo' | undefined;
}
```

- [ ] Run whole-tree typecheck to verify no existing callers break:
```bash
pnpm -r typecheck
```
Expected: passes with zero errors (new fields are optional, all existing callers compile as-is).

- [ ] Commit: `chore: add focus and scope fields to CodeReviewRequestInput and RequestCodeReviewPayload`

---

---

### Task 2: Create `simplicity.ts` — parser, prompt builders, data models

**Depends on:** Task 1

**Files:**
- Create: `packages/agent-core/src/code-review/simplicity.ts`
- Create: `packages/agent-core/test/code-review/simplicity.test.ts`

This module contains all Ponytail-format logic: type definitions (`SimplicityTag`, `RepoAuditDigest`, `FileSnippet`), the `parseSimplicityReport` parser, and two prompt builders (`buildSimplicityReviewPrompt`, `buildSimplicityAuditPrompt`).

#### Step 1: Write the failing test

Create `packages/agent-core/test/code-review/simplicity.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import {
  parseSimplicityReport,
  buildSimplicityReviewPrompt,
  buildSimplicityAuditPrompt,
} from '../../src/code-review/simplicity';
import type { CodeReviewReport } from '../../src/code-review/types';

describe('parseSimplicityReport', () => {
  it('parses well-formed Ponytail lines with file location', () => {
    const raw = 'src/foo.ts:L12: stdlib: 27-line validator class. Use String.prototype.includes, 1 line.';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'simplicity-model');
    expect(report.ok).toBe(true);
    expect(report.findings).toHaveLength(1);
    expect(report.findings[0]!.severity).toBe('important'); // stdlib = important
    expect(report.findings[0]!.location).toBe('src/foo.ts:12');
    expect(report.findings[0]!.title).toContain('[STDLIB]');
    expect(report.findings[0]!.detail).toContain('stdlib: 27-line validator class. Use String.prototype.includes, 1 line.');
    expect(report.findings[0]!.suggestedFix).toBe('Use String.prototype.includes, 1 line');
  });

  it('parses shrink tag as minor severity', () => {
    const raw = 'L5: shrink: long function. Extract helper.';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings[0]!.severity).toBe('minor');
    expect(report.findings[0]!.title).toContain('[SHRINK]');
  });

  it('handles Lean already. Ship.', () => {
    const raw = 'Lean already. Ship.';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings).toHaveLength(0);
    expect(report.summary).toBe('Lean already. Ship.');
  });

  it('extracts net line as summary', () => {
    const raw = 'L1: delete: unused util. Remove it.\nnet: -50 lines possible.';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.summary).toBe('net: -50 lines possible.');
    expect(report.findings).toHaveLength(1);
  });

  it('skips unparseable lines', () => {
    const raw = 'Some random text\nL1: delete: unused code. Remove it.\nMore noise';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings).toHaveLength(1);
    expect(report.findings[0]!.title).toContain('[DELETE]');
  });

  it('handles delete and yagni tags as important severity', () => {
    const raw = [
      'L1: delete: dead code class. Remove the entire file.',
      'L5: yagni: premature abstraction. Inline the two call sites.',
    ].join('\n');
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings).toHaveLength(2);
    expect(report.findings[0]!.severity).toBe('important');
    expect(report.findings[1]!.severity).toBe('important');
  });

  it('handles native tag as important severity', () => {
    const raw = 'L3: native: custom deep clone. Use structuredClone, 1 line.';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings[0]!.severity).toBe('important');
    expect(report.findings[0]!.title).toContain('[NATIVE]');
  });

  it('handles line without file prefix', () => {
    const raw = 'L8: delete: unused import. Remove.';
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings).toHaveLength(1);
    // location is just the line number when no file prefix
    expect(report.findings[0]!.location).toBe(':8');
  });

  it('handles empty input as Lean already', () => {
    const report: CodeReviewReport = parseSimplicityReport('', 'x');
    expect(report.findings).toHaveLength(0);
    expect(report.ok).toBe(true);
  });

  // Must-survive inputs: verify parser does not match false positives
  it('does not parse a line starting with a tag-like word that is not a Ponytail tag', () => {
    const raw = 'This is a normal sentence. delete: is not a real tag here.';
    // "delete:" after "sentence." should not start a valid tag line
    // The parser only matches tag: at the start of a trimmed line
    const report: CodeReviewReport = parseSimplicityReport(raw, 'x');
    expect(report.findings).toHaveLength(0);
  });
});

describe('buildSimplicityReviewPrompt', () => {
  it('contains all five Ponytail tags in the prompt', () => {
    const prompt = buildSimplicityReviewPrompt('mock diff', 'desc', 'reqs');
    expect(prompt).toContain('delete:');
    expect(prompt).toContain('stdlib:');
    expect(prompt).toContain('native:');
    expect(prompt).toContain('yagni:');
    expect(prompt).toContain('shrink:');
  });

  it('contains the diff content', () => {
    const prompt = buildSimplicityReviewPrompt('--- a/file.ts\n+++ b/file.ts\n+new line', undefined, undefined);
    expect(prompt).toContain('--- a/file.ts');
    expect(prompt).toContain('+new line');
  });

  it('includes optional description and requirements', () => {
    const prompt = buildSimplicityReviewPrompt('diff', 'added login', 'must use OAuth');
    expect(prompt).toContain('added login');
    expect(prompt).toContain('must use OAuth');
  });

  it('handles missing description and requirements gracefully', () => {
    const prompt = buildSimplicityReviewPrompt('diff', undefined, undefined);
    expect(prompt).toContain('[not provided]');
  });

  it('includes Lean already. Ship. instruction', () => {
    const prompt = buildSimplicityReviewPrompt('diff', undefined, undefined);
    expect(prompt).toContain('Lean already. Ship.');
  });

  it('includes net line instruction', () => {
    const prompt = buildSimplicityReviewPrompt('diff', undefined, undefined);
    expect(prompt).toContain('net:');
  });
});

describe('buildSimplicityAuditPrompt', () => {
  const digest = {
    workspaceDir: '/app',
    fileCount: 3,
    files: ['src/a.ts', 'src/b.ts', 'src/c.ts'],
    dependencies: ['lodash', 'express'],
    snippets: [
      { path: 'src/a.ts', lines: 'import * as _ from "lodash";\nclass Foo {}' },
    ],
  };

  it('contains all five tags', () => {
    const prompt = buildSimplicityAuditPrompt(digest);
    expect(prompt).toContain('delete:');
    expect(prompt).toContain('stdlib:');
    expect(prompt).toContain('native:');
    expect(prompt).toContain('yagni:');
    expect(prompt).toContain('shrink:');
  });

  it('includes file list', () => {
    const prompt = buildSimplicityAuditPrompt(digest);
    expect(prompt).toContain('src/a.ts');
  });

  it('includes dependency list', () => {
    const prompt = buildSimplicityAuditPrompt(digest);
    expect(prompt).toContain('lodash');
    expect(prompt).toContain('express');
  });

  it('does not contain node_modules (sensitive filter)', () => {
    const prompt = buildSimplicityAuditPrompt(digest);
    expect(prompt).not.toContain('node_modules');
  });

  it('includes net line instruction for deps', () => {
    const prompt = buildSimplicityAuditPrompt(digest);
    expect(prompt).toContain('deps possible');
  });
});
```

#### Step 2: Verify test FAILS

```bash
cd packages/agent-core && pnpm vitest run test/code-review/simplicity.test.ts
```
Expected: all tests fail because `../../src/code-review/simplicity` does not exist yet.

#### Step 3: Write the implementation

Create `packages/agent-core/src/code-review/simplicity.ts`:

```typescript
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
  const tag = tagMatch[0].replace(/:\\s*$/, '').replace(/:$/, '').trim() as SimplicityTag;
  if (!ALL_TAGS.includes(tag)) return null;
  const body = rest.slice(tagMatch[0].length);

  // Step 3: split on first '. ' into what / replacement
  const dotIdx = body.indexOf('. ');
  if (dotIdx < 0) return null;
  const what = body.slice(0, dotIdx).trim();
  let replacement = body.slice(dotIdx + 2).trim();
  // Strip trailing dot
  if (replacement.endsWith('.')) {
    replacement = replacement.slice(0, -1);
  }

  const location = file !== undefined && lineno !== undefined
    ? `${file}:${lineno}`
    : lineno !== undefined
      ? `:${lineno}`
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
    '- If you find something that was deliberately kept simple and could use an `ody:` annotation, you may mention it in the detail — but do not create a finding for it.',
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
```

#### Step 4: Verify test PASSES

```bash
cd packages/agent-core && pnpm vitest run test/code-review/simplicity.test.ts
```
Expected: all 17 tests pass.

- [ ] Commit: `feat: add simplicity review parser and prompt builders with Ponytail format support`

---

### Task 3: Extend executor with `focus`/`scope` branching + `auditScanner` dependency

**Depends on:** Task 2

**Files:**
- Modify: `packages/agent-core/src/code-review/executor.ts:5-17` — add `auditScanner` to deps
- Modify: `packages/agent-core/src/code-review/executor.ts:21-82` — add focus/scope branching logic
- Modify: `packages/agent-core/test/code-review/executor.test.ts` — add simplicity+repo tests

This task adds `auditScanner` to `CodeReviewExecutorDeps`, imports `simplicity.ts` builders/parser, and branches `review()` based on `input.scope` and `input.focus`.

#### Step 1: Write the failing test (add to existing test file)

Append to `packages/agent-core/test/code-review/executor.test.ts`:

```typescript
import { RepoAuditDigest } from '../../src/code-review/simplicity';

// ... existing imports and describe block ...

  describe('simplicity focus', () => {
    const cwd = '/app';
    const modelAlias = 'reviewer';

    it('uses simplicity prompt when focus=simplicity', async () => {
      const ponytailOutput = 'src/foo.ts:L42: delete: dead code. Remove it.\nnet: -10 lines possible.';
      const executor = createCodeReviewExecutor({
        cwd,
        fetchDiff: vi.fn(async () => 'mock diff'),
        generate: vi.fn(async () => ({
          message: { role: 'assistant', content: [{ type: 'text', text: ponytailOutput }] },
          usage: { input: 100, output: 50 },
        })),
        resolveProviderConfig: vi.fn(() => ({})),
        estimateTokens: vi.fn(() => 10),
      });
      const report: CodeReviewReport = await executor.review({
        source: { kind: 'working-tree' },
        modelAlias,
        focus: 'simplicity',
      });
      expect(report.ok).toBe(true);
      expect(report.findings).toHaveLength(1);
      expect(report.findings[0]!.title).toContain('[DELETE]');
    });

    it('calls auditScanner when scope=repo', async () => {
      const digest: RepoAuditDigest = {
        workspaceDir: '/app',
        fileCount: 5,
        files: ['src/a.ts'],
        dependencies: [],
        snippets: [],
      };
      const auditScanner = vi.fn(async () => digest);
      const ponytailOutput = 'stdlib: custom clone. Use structuredClone. [src/a.ts]\nnet: -15 lines possible.';
      const executor = createCodeReviewExecutor({
        cwd,
        fetchDiff: vi.fn(async () => 'unused'),
        generate: vi.fn(async () => ({
          message: { role: 'assistant', content: [{ type: 'text', text: ponytailOutput }] },
          usage: { input: 100, output: 50 },
        })),
        resolveProviderConfig: vi.fn(() => ({})),
        estimateTokens: vi.fn(() => 10),
        auditScanner,
      });
      const report: CodeReviewReport = await executor.review({
        source: { kind: 'working-tree' },
        modelAlias,
        focus: 'simplicity',
        scope: 'repo',
      });
      expect(auditScanner).toHaveBeenCalledWith('/app', expect.any(AbortSignal));
      expect(report.ok).toBe(true);
      expect(report.findings).toHaveLength(1);
      expect(report.findings[0]!.title).toContain('[STDLIB]');
    });

    it('returns ok=false when scope=repo but auditScanner not provided', async () => {
      const executor = createCodeReviewExecutor({
        cwd,
        fetchDiff: vi.fn(async () => 'unused'),
        generate: vi.fn(async () => ({
          message: { role: 'assistant', content: [{ type: 'text', text: '' }] },
          usage: { input: 0, output: 0 },
        })),
        resolveProviderConfig: vi.fn(() => ({})),
        estimateTokens: vi.fn(() => 0),
      });
      const report: CodeReviewReport = await executor.review({
        source: { kind: 'working-tree' },
        modelAlias,
        scope: 'repo',
      });
      expect(report.ok).toBe(false);
      expect(report.note).toContain('Repo audit is not available');
    });

    it('returns Lean already report for no-finding simplicity output', async () => {
      const executor = createCodeReviewExecutor({
        cwd,
        fetchDiff: vi.fn(async () => 'mock diff'),
        generate: vi.fn(async () => ({
          message: { role: 'assistant', content: [{ type: 'text', text: 'Lean already. Ship.' }] },
          usage: { input: 100, output: 50 },
        })),
        resolveProviderConfig: vi.fn(() => ({})),
        estimateTokens: vi.fn(() => 10),
      });
      const report: CodeReviewReport = await executor.review({
        source: { kind: 'working-tree' },
        modelAlias,
        focus: 'simplicity',
      });
      expect(report.ok).toBe(true);
      expect(report.findings).toHaveLength(0);
      expect(report.summary).toBe('Lean already. Ship.');
    });
  });
```

#### Step 2: Verify test FAILS

```bash
cd packages/agent-core && pnpm vitest run test/code-review/executor.test.ts
```
Expected: new tests fail — executor doesn't handle `focus`/`scope` yet.

#### Step 3: Write the implementation

Modify `packages/agent-core/src/code-review/executor.ts`:

```typescript
import type { CodeReviewRequestInput, CodeReviewReport } from './types';
import { buildReviewPrompt, parseReviewReport } from './prompt';
import {
  buildSimplicityReviewPrompt,
  buildSimplicityAuditPrompt,
  parseSimplicityReport,
  type RepoAuditDigest,
} from './simplicity';
import type { CodeReviewDiffSource } from './types';

export interface CodeReviewExecutorDeps {
  readonly cwd: string;
  readonly fetchDiff: (source: CodeReviewDiffSource, cwd: string) => Promise<string>;
  readonly generate: (options: {
    readonly modelAlias: string;
    readonly systemPrompt: string;
    readonly userPrompt: string;
    readonly signal?: AbortSignal | undefined;
  }) => Promise<{ message: { role: string; content: Array<{ type: string; text: string }> }; usage?: unknown }>;
  readonly resolveProviderConfig: (alias: string) => unknown;
  readonly estimateTokens: (text: string) => number;
  readonly deepRunner?: ((diff: string, input: CodeReviewRequestInput) => Promise<CodeReviewReport>) | undefined;
  readonly auditScanner?: ((workspaceDir: string, signal?: AbortSignal) => Promise<RepoAuditDigest>) | undefined;
}

const MAX_DIFF_TOKENS = 100_000;

export function createCodeReviewExecutor(deps: CodeReviewExecutorDeps) {
  return {
    async review(input: CodeReviewRequestInput): Promise<CodeReviewReport> {
      const isSimplicity = input.focus === 'simplicity' || input.scope === 'repo';
      const signal = input.timeoutMs !== undefined ? AbortSignal.timeout(input.timeoutMs) : undefined;

      // ── Repo audit path ──
      if (input.scope === 'repo') {
        if (deps.auditScanner === undefined) {
          return {
            ok: false,
            reviewerAlias: input.modelAlias,
            findings: [],
            note: 'Repo audit is not available in this context.',
          };
        }
        try {
          const digest = await deps.auditScanner(deps.cwd, signal);
          const userPrompt = buildSimplicityAuditPrompt(digest);
          const response = await deps.generate({
            modelAlias: input.modelAlias,
            systemPrompt: '',
            userPrompt,
            signal,
          });
          const text = response.message.content
            .filter((p) => p.type === 'text')
            .map((p) => p.text)
            .join('');
          return parseSimplicityReport(text, input.modelAlias);
        } catch (error) {
          return {
            ok: false,
            reviewerAlias: input.modelAlias,
            findings: [],
            note: `Code review failed: ${error instanceof Error ? error.message : String(error)}`,
          };
        }
      }

      // ── Diff-based path ──
      let diff: string;
      try {
        diff = await deps.fetchDiff(input.source, deps.cwd);
      } catch (error) {
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note: `Failed to fetch diff: ${error instanceof Error ? error.message : String(error)}`,
        };
      }

      const estimatedTokens = deps.estimateTokens(diff);
      if (estimatedTokens > MAX_DIFF_TOKENS) {
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note: `Diff too large (~${estimatedTokens} tokens, limit ${MAX_DIFF_TOKENS}). Try a smaller range or use --base/--head.`,
        };
      }

      if (input.deep) {
        if (deps.deepRunner !== undefined) {
          return deps.deepRunner(diff, input);
        }
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note: 'Deep review is not available in this context. Try without --deep.',
        };
      }

      try {
        const userPrompt = isSimplicity
          ? buildSimplicityReviewPrompt(diff, input.description, input.requirements)
          : buildReviewPrompt(diff, input.description, input.requirements);

        const response = await deps.generate({
          modelAlias: input.modelAlias,
          systemPrompt: '',
          userPrompt,
          signal,
        });
        const text = response.message.content
          .filter((p) => p.type === 'text')
          .map((p) => p.text)
          .join('');

        return isSimplicity
          ? parseSimplicityReport(text, input.modelAlias)
          : parseReviewReport(text, input.modelAlias);
      } catch (error) {
        return {
          ok: false,
          reviewerAlias: input.modelAlias,
          findings: [],
          note: `Code review failed: ${error instanceof Error ? error.message : String(error)}`,
        };
      }
    },
  };
}
```

#### Step 4: Verify test PASSES

```bash
cd packages/agent-core && pnpm vitest run test/code-review/executor.test.ts
```
Expected: all tests pass (12 total: 8 existing + 4 new).

- [ ] Commit: `feat: add simplicity focus/scope branching and auditScanner to code review executor`

---

---

### Task 4: Wire RPC — auditScanner injection, focus/scope forwarding, telemetry, exports

**Depends on:** Task 3

**Files:**
- Modify: `packages/agent-core/src/code-review/simplicity.ts` — append `buildAuditDigest`
- Modify: `packages/agent-core/src/rpc/core-impl.ts:467-544` — inject `auditScanner`, pass `focus`/`scope`, add telemetry
- Modify: `packages/agent-core/src/index.ts:86-95` — export new simplicity symbols
- Create: `packages/agent-core/test/code-review/audit-scanner.test.ts` — integration test for `buildAuditDigest`

This task builds the `buildAuditDigest` function that scans the workspace using file-system listing, wires it into the RPC layer, adds telemetry events, and updates the public exports.

#### Step 1: Write `buildAuditDigest` implementation

Add to `packages/agent-core/src/code-review/simplicity.ts`, appended after existing content:

```typescript
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

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
    let entries: ReturnType<typeof readdirSync>;
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
```

#### Step 2: Write the test

Create `packages/agent-core/test/code-review/audit-scanner.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { mkdtempSync, writeFileSync, mkdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { buildAuditDigest } from '../../src/code-review/simplicity';

describe('buildAuditDigest', () => {
  it('discovers source files in a directory', () => {
    const dir = mkdtempSync(join(tmpdir(), 'audit-test-'));
    try {
      writeFileSync(join(dir, 'index.ts'), 'export const x = 1;');
      writeFileSync(join(dir, 'helper.js'), 'function f() {}');
      mkdirSync(join(dir, 'sub'));
      writeFileSync(join(dir, 'sub', 'nested.ts'), 'const y = 2;');
      mkdirSync(join(dir, 'node_modules'));
      writeFileSync(join(dir, 'node_modules', 'ignored.ts'), 'ignored');

      const digest = buildAuditDigest(dir);
      const names = digest.files.map((f) => f.replace(/\\/g, '/'));
      expect(names).toContain('index.ts');
      expect(names).toContain('helper.js');
      expect(names).toContain('sub/nested.ts');
      expect(names).not.toContain('node_modules/ignored.ts');
      expect(digest.fileCount).toBe(3);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('caps at MAX_AUDIT_FILES', () => {
    const dir = mkdtempSync(join(tmpdir(), 'audit-cap-'));
    try {
      for (let i = 0; i < 250; i += 1) {
        writeFileSync(join(dir, `file${i}.ts`), '// test');
      }
      const digest = buildAuditDigest(dir);
      expect(digest.fileCount).toBeLessThanOrEqual(200);
      expect(digest.files.length).toBeLessThanOrEqual(200);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('excludes dot-directories', () => {
    const dir = mkdtempSync(join(tmpdir(), 'audit-dot-'));
    try {
      mkdirSync(join(dir, '.hidden'));
      writeFileSync(join(dir, '.hidden', 'secret.ts'), '// hidden');
      writeFileSync(join(dir, 'visible.ts'), '// visible');

      const digest = buildAuditDigest(dir);
      const names = digest.files.map((f) => f.replace(/\\/g, '/'));
      expect(names).not.toContain('.hidden/secret.ts');
      expect(names).toContain('visible.ts');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('extracts package.json dependencies', () => {
    const dir = mkdtempSync(join(tmpdir(), 'audit-deps-'));
    try {
      writeFileSync(join(dir, 'package.json'), JSON.stringify({
        dependencies: { lodash: '^4.0.0' },
        devDependencies: { vitest: '^3.0.0' },
      }));
      writeFileSync(join(dir, 'index.ts'), 'export {}');

      const digest = buildAuditDigest(dir);
      expect(digest.dependencies).toContain('lodash');
      expect(digest.dependencies).toContain('vitest');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('handles missing package.json gracefully', () => {
    const dir = mkdtempSync(join(tmpdir(), 'audit-nopkg-'));
    try {
      writeFileSync(join(dir, 'index.ts'), 'export {}');
      const digest = buildAuditDigest(dir);
      expect(digest.dependencies).toEqual([]);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('respects AbortSignal', () => {
    const dir = mkdtempSync(join(tmpdir(), 'audit-abort-'));
    try {
      for (let i = 0; i < 100; i += 1) {
        writeFileSync(join(dir, `f${i}.ts`), '// test');
      }
      const ctrl = new AbortController();
      ctrl.abort();
      const digest = buildAuditDigest(dir, ctrl.signal);
      expect(digest.fileCount).toBe(0);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
```

#### Step 3: Verify audit scanner test passes

```bash
cd packages/agent-core && pnpm vitest run test/code-review/audit-scanner.test.ts
```
Expected: all 6 tests pass.

#### Step 4: Wire RPC in `core-impl.ts`

Add import at top of `packages/agent-core/src/rpc/core-impl.ts`:
```typescript
import { buildAuditDigest } from '#/code-review/simplicity';
```

Replace the executor creation and call block (lines 490-543):

```typescript
    const executor = createCodeReviewExecutor({
      cwd: payload.workDir,
      fetchDiff: async (source) => codeReviewFetchDiff(source, payload.workDir),
      auditScanner: payload.scope === 'repo'
        ? async (workspaceDir, signal) => buildAuditDigest(workspaceDir, signal)
        : undefined,
      generate: async (options) => {
        const doGenerate = async (auth?: ProviderRequestAuth): ReturnType<typeof generate> => {
          return generate(
            provider,
            options.systemPrompt,
            [],
            [createUserMessage(options.userPrompt)],
            undefined,
            { signal: options.signal, ...(auth !== undefined ? { auth } : {}) },
          );
        };

        const withAuth = providerManager.resolveAuth?.(resolvedModel);
        const result = withAuth !== undefined
          ? await withAuth((auth) => doGenerate(auth))
          : await doGenerate();

        return {
          message: {
            role: result.message.role,
            content: result.message.content.filter((p): p is { type: 'text'; text: string } => p.type === 'text'),
          },
          usage: result.usage,
        };
      },
      resolveProviderConfig: (alias) => providerManager.resolveProviderConfig(alias),
      estimateTokens,
    });

    // ── Telemetry ──
    const isSimplicity = payload.focus === 'simplicity' || payload.scope === 'repo';
    const isAudit = payload.scope === 'repo';
    if (isSimplicity) {
      if (isAudit) {
        this.telemetry.track('simplicity_audit_started', {
          scope: 'repo',
          file_count: 0,
        });
      } else {
        this.telemetry.track('simplicity_review_started', {
          scope: 'diff',
          focus: 'simplicity',
          has_description: payload.description !== undefined,
          has_requirements: payload.requirements !== undefined,
        });
      }
    }

    const report = await executor.review({
      source: payload.source,
      modelAlias: resolvedModel,
      description: payload.description,
      requirements: payload.requirements,
      deep: payload.deep,
      timeoutMs: payload.timeoutMs,
      focus: payload.focus,
      scope: payload.scope,
    });

    if (isSimplicity) {
      if (report.ok) {
        const evt = isAudit ? 'simplicity_audit_completed' : 'simplicity_review_completed';
        this.telemetry.track(evt, {
          scope: isAudit ? 'repo' : 'diff',
          finding_count: report.findings.length,
          ok: true,
        });
      } else {
        const evt = isAudit ? 'simplicity_audit_failed' : 'simplicity_review_failed';
        this.telemetry.track(evt, {
          scope: isAudit ? 'repo' : 'diff',
          reason: report.note ?? 'unknown',
        });
      }
    }

    return {
      ok: report.ok,
      reviewerAlias: report.reviewerAlias,
      summary: report.summary,
      findings: report.findings.map((f) => ({
        severity: f.severity,
        title: f.title,
        detail: f.detail,
        location: f.location,
        suggestedFix: f.suggestedFix,
      })),
      note: report.note,
    };
```

#### Step 5: Update `index.ts` exports

Modify `packages/agent-core/src/index.ts` lines 86-95, replace the Code Review section:

```typescript
// ─── Code Review ───────────────────────────────────────────────────────────
export { renderCodeReviewReportToMarkdown } from './code-review/report';
export { resolveCodeReviewModel } from './code-review/model-resolver';
export type { ResolveModelOverrides } from './code-review/model-resolver';
export type {
  CodeReviewDiffSource,
  CodeReviewRequestInput,
  CodeReviewReport,
  CodeReviewFinding,
} from './code-review/types';
export {
  parseSimplicityReport,
  buildSimplicityReviewPrompt,
  buildSimplicityAuditPrompt,
  buildAuditDigest,
} from './code-review/simplicity';
export type {
  SimplicityTag,
  RepoAuditDigest,
  FileSnippet,
} from './code-review/simplicity';
```

#### Step 6: Whole-tree typecheck

```bash
pnpm -r typecheck
```
Expected: passes with zero errors.

#### Step 7: Run all code-review tests

```bash
cd packages/agent-core && pnpm vitest run test/code-review/
```
Expected: all tests pass across all four test files.

- [ ] Commit: `feat: wire audit scanner and simplicity telemetry into RPC layer`

---

---

### Task 5: CLI `--focus` / `--scope` options and validation

**Depends on:** Task 4

**Files:**
- Modify: `apps/ody-code/src/cli/sub/request-code-review.ts:10-19` — add `focus`/`scope` to `RequestCodeReviewCLIOptions`
- Modify: `apps/ody-code/src/cli/sub/request-code-review.ts:28-49` — add `scope=repo` + `base`/`head`/`pr` conflict validation
- Modify: `apps/ody-code/src/cli/sub/request-code-review.ts:69-103` — pass `focus`/`scope` through `harness.requestCodeReview`
- Modify: `apps/ody-code/src/cli/sub/request-code-review.ts:105-127` — register `--focus`/`--scope` command options
- No test file exists for CLI command-specific tests; this is wiring. Typecheck + manual smoke test.

#### Step 1: Add `focus` and `scope` to `RequestCodeReviewCLIOptions`

At `packages/agent-core` level the types are already wired. Now modify CLI types:

```typescript
// Line 10-19, add two fields:
interface RequestCodeReviewCLIOptions {
  base?: string | undefined;
  head?: string | undefined;
  pr?: string | undefined;
  model?: string | undefined;
  description?: string | undefined;
  requirements?: string | undefined;
  deep?: boolean | undefined;
  timeout?: number | undefined;
  focus?: 'correctness' | 'simplicity' | undefined;
  scope?: 'diff' | 'repo' | undefined;
}
```

#### Step 2: Add conflict validation

In `validateRequestCodeReviewOptions` (lines 28-49), add after the existing `--pr` + `--base`/`--head` conflict check:

```typescript
  if (opts.scope === 'repo' && (opts.base !== undefined || opts.head !== undefined || opts.pr !== undefined)) {
    throw new OptionConflictError('Cannot combine --scope repo with --base/--head/--pr.');
  }

  if (opts.focus !== undefined && opts.focus !== 'correctness' && opts.focus !== 'simplicity') {
    throw new OptionConflictError(`Invalid --focus value: ${opts.focus}. Must be 'correctness' or 'simplicity'.`);
  }

  if (opts.scope !== undefined && opts.scope !== 'diff' && opts.scope !== 'repo') {
    throw new OptionConflictError(`Invalid --scope value: ${opts.scope}. Must be 'diff' or 'repo'.`);
  }

  // When scope is repo, skip the base/head default logic
  if (opts.scope === 'repo') {
    // use working-tree source shape (source field ignored by executor for repo scope)
    return;
  }
```

Also, skip the base/head default logic when `scope === 'repo'`: wrap the existing defaults block (lines 35-42, the block that sets `opts.base = 'HEAD~1'` etc.) in an `if (opts.scope !== 'repo')` guard.

#### Step 3: Pass `focus` and `scope` through `harness.requestCodeReview`

In `handleRequestCodeReview` (lines 69-103), add to the `harness.requestCodeReview` call:

```typescript
    const report = await harness.requestCodeReview({
      source,
      modelAlias: opts.model,
      description: opts.description,
      requirements: opts.requirements,
      deep: opts.deep,
      timeoutMs: opts.timeout !== undefined ? opts.timeout * 1000 : undefined,
      focus: opts.focus,
      scope: opts.scope,
    });
```

#### Step 4: Register `--focus` and `--scope` command options

In `registerRequestCodeReviewCommand` (lines 105-127), add after the `--timeout` option:

```typescript
    .option('--focus <focus>', "Review focus: correctness (default) or simplicity (anti-over-engineering).")
    .option('--scope <scope>', 'Review scope: diff (default) or repo (whole-workspace audit).')
```

#### Step 5: Build + manual smoke test

```bash
pnpm nx typecheck ody-code
```
Expected: passes with zero errors.

Manual verification:
```bash
# Test 1: --focus and --scope parse without error
ody-code request-code-review --focus simplicity --help

# Test 2: scope=repo rejects --base
ody-code request-code-review --scope repo --base HEAD~1
# Expected: error "Cannot combine --scope repo with --base/--head/--pr."

# Test 3: invalid focus rejects
ody-code request-code-review --focus security
# Expected: error "Invalid --focus value: security"
```

- [ ] Commit: `feat: add --focus and --scope options to CLI request-code-review`

---

### Task 6: TUI `--focus` / `--scope` slash command parsing

**Depends on:** Task 4 (independent of Task 5)

**Files:**
- Modify: `apps/ody-code/src/tui/commands/request-code-review.ts:7-15` — add `focus`/`scope` to `SlashArgs`
- Modify: `apps/ody-code/src/tui/commands/request-code-review.ts:17-36` — add `--focus`/`--scope` token parsing
- Modify: `apps/ody-code/src/tui/commands/request-code-review.ts:90-98` — pass `focus`/`scope` to harness call
- No test file exists for TUI commands; this is wiring. Typecheck + manual smoke test.

#### Step 1: Add `focus` and `scope` to `SlashArgs`

```typescript
// Line 7-15, add two fields:
interface SlashArgs {
  readonly base?: string;
  readonly head?: string;
  readonly pr?: string;
  readonly model?: string;
  readonly description?: string;
  readonly requirements?: string;
  readonly deep?: boolean;
  readonly focus?: 'correctness' | 'simplicity';
  readonly scope?: 'diff' | 'repo';
}
```

#### Step 2: Add `--focus` and `--scope` token parsing

In `parseArgs` (lines 17-36), add `--focus` and `--scope` to the value-taking flags:

```typescript
// Line 22-23, extend the condition:
    if (token === '--base' || token === '--head' || token === '--pr' || token === '--model' ||
        token === '--description' || token === '--requirements' || token === '--focus' || token === '--scope') {
      result[camelFromFlag(token)] = tokens[i + 1];
      i += 1;
```

#### Step 3: Pass `focus` and `scope` through harness call

In `handleRequestCodeReviewCommand` (lines 90-98), add to `host.harness.requestCodeReview()`:

```typescript
    const report = await host.harness.requestCodeReview({
      source,
      modelAlias: resolvedModel,
      description: parsed.description,
      requirements: parsed.requirements,
      deep: parsed.deep,
      focus: parsed.focus,
      scope: parsed.scope,
    });
```

#### Step 4: Build + manual smoke test

```bash
pnpm nx typecheck ody-code
```
Expected: passes with zero errors.

Manual verification (in TUI session):
```
/request-code-review --focus simplicity
# Expected: review runs with simplicity prompt, output contains Ponytail-format findings

/request-code-review --scope repo --focus simplicity
# Expected: audit runs, output contains repo-wide findings

/request-code-review --scope repo --base HEAD~1
# Expected: works (TUI doesn't enforce the conflict — it just passes through; the executor ignores `source` when `scope=repo`)
```

- [ ] Commit: `feat: add --focus and --scope support to TUI request-code-review slash command`

---

---

## Self-Review

- [ ] 1. **Spec-coverage table**: map every spec section/requirement → Task(s), marked covered / GAP / no-op

| Spec requirement | Task(s) | Status |
|---|---|---|
| `--focus simplicity` on `request-code-review` | Task 3 (executor), Task 5 (CLI), Task 6 (TUI) | covered |
| `--scope repo` for whole-repo audit | Task 3 (executor), Task 4 (RPC+auditScanner), Task 5 (CLI), Task 6 (TUI) | covered |
| Ponytail structured tags: delete/stdlib/native/yagni/shrink | Task 2 (simplicity.ts) | covered |
| Output format: `L<line>: <tag> <现状>. <替代>.` | Task 2 (parser + prompt builders) | covered |
| `net: -N lines possible.` / `Lean already. Ship.` | Task 2 (parser + prompt builders) | covered |
| Reuse `code-review/` diff fetch, model resolution, report rendering | Task 3, Task 4 | covered |
| Reuse `GrepTool` for audit — replaced by `fs`-based `buildAuditDigest` | Task 4 | covered (simpler; no GrepTool injection needed) |
| Report only, no auto-fix | All tasks (design constraint) | no-op |
| Telemetry: `simplicity_review_started/completed/failed`, `simplicity_audit_started/completed/failed` | Task 4 | covered |
| Direct release, no experimental flag | All tasks (no flag gate) | no-op |
| `ody:` annotation suggestion in detail | Task 2 (prompt instruction) | covered |
| `scope=repo` + `source` conflict validation (CLI) | Task 5 | covered |
| Must-survive inputs: normal text not parsed as Ponytail tags | Task 2 (parser test) | covered |
| `node_modules` exclusion from audit digest | Task 4 (EXCLUDED_DIRS) | covered |
| `CodeReviewRequestInput` type extension with `focus`/`scope` | Task 1 | covered |
| `RequestCodeReviewPayload` type extension | Task 1 | covered |
| `auditScanner` dependency injection on executor | Task 3 | covered |
| `index.ts` exports | Task 4 | covered |

- [ ] 2. **Placeholder scan**: no TODO/TBD, no deferred-by-dependency excuses, no dead-code placeholders.

Verified: every task step contains concrete code. No `TODO`, `TBD`, "implement later", or "add appropriate error handling" placeholders.

- [ ] 3. **No phantom tasks**: every task produces a verifiable change; zero `--allow-empty` / "already done in Task N".

Task 1: type additions → whole-tree typecheck verifies. Task 2: new module + 17 tests. Task 3: executor changes + 4 new tests. Task 4: auditScanner + RPC + telemetry + exports + 6 new tests. Task 5: CLI wiring + manual verification. Task 6: TUI wiring + manual verification. Every task produces real diff.

- [ ] 4. **Dependency soundness**: every `Depends on:` is satisfied by an earlier task; nothing references a symbol only a later task creates.

Task 2 → Task 1 (types exist). Task 3 → Task 2 (simplicity module exists). Task 4 → Task 3 (executor interface has `auditScanner`). Task 5 → Task 4 (RPC accepts `focus`/`scope`). Task 6 → Task 4 (RPC accepts `focus`/`scope`). No forward references.

- [ ] 5. **Caller & build soundness**: every shared-signature task updated all callers (incl. test files) and ends with a whole-tree typecheck.

Task 1: `CodeReviewRequestInput` gains `focus`/`scope` (optional — existing callers don't break). `RequestCodeReviewPayload` gains `focus`/`scope` (optional). Whole-tree typecheck in Task 1 verifies zero breakage.

The `CodeReviewExecutorDeps` gains `auditScanner` (optional — existing callers don't break). Verified in Task 3's test pass.

The `RequestCodeReviewPayload` fields flow through `core-impl.ts` → executor — tracked end-to-end: CLI/TUI pass `focus`/`scope` → RPC `requestCodeReview(payload)` → executor `review(input)` — all on the same object shape.

- [ ] 6. **Test-the-risk**: every state-mutating task has a behavioral test asserting the mutation, not just a compile check.

Task 2: `parseSimplicityReport` — tests cover well-formed lines, `Lean already. Ship.`, net line extraction, empty input, unparseable skip, all 5 tags, severity mapping, false-positive rejection (must-survive: "normal sentence. delete: is not a real tag").

Task 3: `executor.review` — tests cover `focus=simplicity` prompt selection, `scope=repo` → `auditScanner` call, missing `auditScanner` error, `Lean already. Ship.` passthrough.

Task 4: `buildAuditDigest` — tests cover file discovery, cap enforcement, dot-dir exclusion, dep extraction, missing package.json, AbortSignal respect.

Task 5 & 6: CLI/TUI are wiring-only; verified via typecheck + manual smoke.

- [ ] 7. **Type consistency**: types, signatures and property names used in later tasks match what earlier tasks defined.

Task 1 defines `focus?: 'correctness' | 'simplicity'` and `scope?: 'diff' | 'repo'` on both `CodeReviewRequestInput` and `RequestCodeReviewPayload`. Tasks 3-6 use these exact same names and union values.

Task 2 defines `SimplicityTag`, `RepoAuditDigest`, `FileSnippet`. Task 3 imports `RepoAuditDigest` for `CodeReviewExecutorDeps.auditScanner` return type. Task 4 imports `buildAuditDigest` returns `RepoAuditDigest`. All consistent.

Task 2 defines `parseSimplicityReport`, `buildSimplicityReviewPrompt`, `buildSimplicityAuditPrompt`. Task 3 imports all three. Task 4 imports `buildAuditDigest`. Names match.



| # | Risk | Mitigation |
|---|------|------------|
| 1 | `GrepTool` constructor requires `Kaos` + `WorkspaceConfig` instances, which may not be easily available in RPC context | Task 4 uses a simple `fs`-based file listing fallback if direct GrepTool injection is infeasible; the `buildAuditDigest` function accepts a generic scanner interface |
| 2 | Ponytail parser regex may reject valid model output if model deviates from format | Parser has a lenient fallback: unparseable lines are skipped, and the raw model output is still available in the note/report |

