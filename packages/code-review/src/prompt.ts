import type { CodeReviewFinding, CodeReviewReport } from './types';

export function buildReviewPrompt(
  diff: string,
  description: string | undefined,
  requirements: string | undefined,
): string {
  return [
    'You are a code reviewer. Review the following changes.',
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
    '1. Evaluate the changes against the requirements.',
    '2. Categorize findings as Critical / Important / Minor.',
    '3. For each finding, give a title, detail, file/line location if available, and suggested fix.',
    '4. Conclude with an assessment: Ready to proceed / Needs fixes.',
    '',
    'Output format:',
    '```',
    'Strengths:',
    '- ...',
    '',
    'Findings:',
    'Critical:',
    '- [title] (location)',
    '  detail',
    '  fix: ...',
    '',
    'Important:',
    '- ...',
    '',
    'Minor:',
    '- ...',
    '',
    'Assessment: Ready to proceed / Needs fixes',
    '```',
  ].join('\n');
}

export function parseReviewReport(raw: string, reviewerAlias: string): CodeReviewReport {
  const findings: CodeReviewFinding[] = [];

  const criticalSection = extractSection(raw, 'Critical');
  for (const finding of parseFindings(criticalSection, 'critical')) {
    findings.push(finding);
  }

  const importantSection = extractSection(raw, 'Important');
  for (const finding of parseFindings(importantSection, 'important')) {
    findings.push(finding);
  }

  const minorSection = extractSection(raw, 'Minor');
  for (const finding of parseFindings(minorSection, 'minor')) {
    findings.push(finding);
  }

  const strengthsSection = extractSection(raw, 'Strengths');
  const summary = strengthsSection
    .split('\n')
    .filter((l) => l.trim().startsWith('-'))
    .map((l) => l.replace(/^-\s*/, '').trim())
    .join('\n');

  return {
    ok: true,
    reviewerAlias,
    summary: summary.length > 0 ? summary : undefined,
    findings,
  };
}

function extractSection(text: string, sectionName: string): string {
  const pattern = new RegExp(`(?:^|\\n)${escapeRegex(sectionName)}:[^\\S\\n]*\\n([\\s\\S]*?)(?:\\n\\n|\\n(?:Strengths|Assessment|Findings|Critical|Important|Minor):|$)`);
  const match = text.match(pattern);
  return match?.[1]?.trim() ?? '';
}

function parseFindings(section: string, severity: CodeReviewFinding['severity']): CodeReviewFinding[] {
  if (section.trim().length === 0) return [];

  const findings: CodeReviewFinding[] = [];
  const entries = section.split(/\n(?=-\s*\[)/);
  for (const entry of entries) {
    const clean = entry.trim();
    if (clean.length === 0) continue;

    const titleMatch = clean.match(/^-\s*\[(.+?)\]\s*(?:\(([^)]+)\))?/);
    if (titleMatch === null) continue;

    const title = titleMatch[1]?.trim() ?? '';
    const location = titleMatch[2]?.trim();

    const bodyLines = clean.split('\n').slice(1);
    const detailLines: string[] = [];
    let suggestedFix: string | undefined;

    for (const line of bodyLines) {
      const trimmedLine = line.trim();
      if (trimmedLine.toLowerCase().startsWith('fix:')) {
        suggestedFix = trimmedLine.slice('fix:'.length).trim();
      } else if (trimmedLine.length > 0) {
        detailLines.push(trimmedLine);
      }
    }

    findings.push({
      severity,
      title,
      detail: detailLines.join('\n').trim() || title,
      location,
      suggestedFix,
    });
  }

  return findings;
}

function escapeRegex(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
