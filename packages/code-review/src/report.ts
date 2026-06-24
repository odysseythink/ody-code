import type { CodeReviewReport } from './types';

export function renderCodeReviewReportToMarkdown(report: CodeReviewReport): string {
  const lines: string[] = [
    `# Code Review Report (${report.reviewerAlias})`,
    '',
  ];

  if (report.summary !== undefined && report.summary.length > 0) {
    lines.push(report.summary, '');
  } else {
    lines.push('_No summary provided._', '');
  }

  lines.push(`## Findings (${report.findings.length})`, '');

  if (report.findings.length === 0) {
    lines.push('No issues found. ✅', '');
  } else {
    for (const finding of report.findings) {
      lines.push(`### [${finding.severity.toUpperCase()}] ${finding.title}`);
      if (finding.location !== undefined) {
        lines.push(`- **Location:** ${finding.location}`);
      }
      lines.push(finding.detail);
      if (finding.suggestedFix !== undefined) {
        lines.push(`- **Suggested fix:** ${finding.suggestedFix}`);
      }
      lines.push('');
    }
  }

  return lines.join('\n');
}
