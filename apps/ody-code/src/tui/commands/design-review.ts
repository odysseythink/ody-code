/**
 * /design-review — run a SECOND-MODEL critique of the current design.
 *
 * Design mode runs on a cheap model; this asks a different (usually more
 * capable) configured model to audit the finished design in a single pass.
 * Findings are severity-tagged and pre-marked `escalate` (severity × the
 * design file's audit level) by the backend. We render them, then hand them
 * back to the design model with an instruction to confirm every [ESCALATE]
 * finding with the user via AskUserQuestion before editing — so the human
 * verification rides the existing interaction path and the design model fixes
 * its own file.
 */

import type { DesignReviewData, ReviewFindingData } from '@odysseythink/kimi-code-sdk';

import { NO_ACTIVE_SESSION_MESSAGE } from '../constant/kimi-tui';
import type { SlashCommandHost } from './dispatch';

interface ParsedArgs {
  readonly path?: string;
  readonly modelAlias?: string;
}

function parseArgs(args: string): ParsedArgs {
  const tokens = args.trim().split(/\s+/).filter((token) => token.length > 0);
  let path: string | undefined;
  let modelAlias: string | undefined;
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i] ?? '';
    if (token === '--model' || token === '-m') {
      modelAlias = tokens[i + 1];
      i += 1;
    } else if (token.startsWith('--model=')) {
      modelAlias = token.slice('--model='.length);
    } else if (path === undefined) {
      path = token;
    }
  }
  return {
    ...(path !== undefined ? { path } : {}),
    ...(modelAlias !== undefined && modelAlias.length > 0 ? { modelAlias } : {}),
  };
}

const SEVERITY_LABEL: Record<ReviewFindingData['severity'], string> = {
  high: 'HIGH',
  med: 'MED',
  low: 'LOW',
};

function renderFinding(finding: ReviewFindingData, index: number): string {
  const tag = finding.escalate ? ' [ESCALATE]' : '';
  const location = finding.location !== undefined ? ` (${finding.location})` : '';
  const fix = finding.suggestedFix !== undefined ? `\n   fix: ${finding.suggestedFix}` : '';
  return `${index}. [${SEVERITY_LABEL[finding.severity]}]${tag} ${finding.title}${location}\n   ${finding.detail}${fix}`;
}

function buildFollowupMessage(result: DesignReviewData): string {
  const body = result.findings.map((finding, i) => renderFinding(finding, i + 1)).join('\n');
  const escalated = result.findings.filter((finding) => finding.escalate).length;
  const escalationLine =
    escalated > 0
      ? `For each of the ${escalated} finding(s) marked [ESCALATE], you MUST confirm with me via AskUserQuestion — fix it, skip it, or change the approach — BEFORE editing the design. Verify each flagged claim against the code first (run an ephemeral node -e / python -c when it is a filter, regex, or test assertion).`
      : 'None of the findings require my sign-off.';
  return [
    `A second-model design review of the current design (reviewer: ${result.reviewerAlias}, audit level: ${result.auditLevel}) found ${result.findings.length} issue(s):`,
    '',
    body,
    '',
    escalationLine,
    'Fix the findings NOT marked [ESCALATE] directly in the design file. Do not implement anything beyond updating the design.',
  ].join('\n');
}

export async function handleDesignReviewCommand(host: SlashCommandHost, args: string): Promise<void> {
  const session = host.session;
  if (session === undefined) {
    host.showError(NO_ACTIVE_SESSION_MESSAGE);
    return;
  }

  const { path, modelAlias } = parseArgs(args);
  host.showStatus('Running design review on the reviewer model…');

  let result: DesignReviewData;
  try {
    result = await session.reviewDesign({
      ...(path !== undefined ? { path } : {}),
      ...(modelAlias !== undefined ? { modelAlias } : {}),
    });
  } catch (error) {
    host.showError(`Design review failed: ${error instanceof Error ? error.message : String(error)}`);
    return;
  }

  if (!result.ok) {
    host.showError(`Design review unavailable: ${result.note ?? 'unknown error'}`);
    return;
  }
  if (result.findings.length === 0) {
    host.showStatus(
      `Design review (${result.reviewerAlias}) found no issues. Audit level: ${result.auditLevel}.`,
    );
    return;
  }

  const escalated = result.findings.filter((finding) => finding.escalate).length;
  host.showStatus(
    `Design review (${result.reviewerAlias}, ${result.auditLevel}): ${result.findings.length} finding(s), ${escalated} need your sign-off. Handing to the design model…`,
  );
  host.sendNormalUserInput(buildFollowupMessage(result));
}
