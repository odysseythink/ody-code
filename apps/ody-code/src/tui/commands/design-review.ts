/**
 * /design-review and /plan-review — a SECOND-MODEL critique of the current
 * design or execution plan.
 *
 * Plan/design mode runs on a cheap model; this asks a different (usually more
 * capable) configured model to attack the finished document in a single pass.
 * Findings are severity-tagged and pre-marked `escalate` (severity × the
 * document's audit level) by the backend. We render them, then hand them back
 * to the authoring model with an instruction to confirm every [ESCALATE]
 * finding with the user via AskUserQuestion before editing — so the human
 * verification rides the existing interaction path and the authoring model
 * fixes its own file. The two commands share one implementation; only the
 * document kind, mode guard, and wording differ.
 */

import type { DesignReviewData, ReviewFindingData } from '@odysseythink/kimi-code-sdk';

import { NO_ACTIVE_SESSION_MESSAGE } from '../constant/kimi-tui';
import type { SlashCommandHost } from './dispatch';

type ReviewKind = 'plan' | 'design';

interface ParsedArgs {
  readonly path?: string;
  readonly modelAlias?: string;
  readonly timeoutMs?: number;
}

function parseArgs(args: string): ParsedArgs {
  const tokens = args.trim().split(/\s+/).filter((token) => token.length > 0);
  let path: string | undefined;
  let modelAlias: string | undefined;
  let timeoutMs: number | undefined;
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i] ?? '';
    if (token === '--model' || token === '-m') {
      modelAlias = tokens[i + 1];
      i += 1;
    } else if (token.startsWith('--model=')) {
      modelAlias = token.slice('--model='.length);
    } else if (token === '--timeout' || token === '-t') {
      const raw = parseInt(tokens[i + 1] ?? '', 10);
      if (!isNaN(raw) && raw > 0) timeoutMs = raw * 1000;
      i += 1;
    } else if (token.startsWith('--timeout=')) {
      const raw = parseInt(token.slice('--timeout='.length), 10);
      if (!isNaN(raw) && raw > 0) timeoutMs = raw * 1000;
    } else if (path === undefined) {
      path = token;
    }
  }
  return {
    ...(path !== undefined ? { path } : {}),
    ...(modelAlias !== undefined && modelAlias.length > 0 ? { modelAlias } : {}),
    ...(timeoutMs !== undefined ? { timeoutMs } : {}),
  };
}

const SEVERITY_LABEL: Record<ReviewFindingData['severity'], string> = {
  high: 'HIGH',
  med: 'MED',
  low: 'LOW',
};

export function renderFinding(finding: ReviewFindingData, index: number): string {
  const tag = finding.escalate ? ' [ESCALATE]' : '';
  const specTag = finding.confidence === 'speculative' ? ' ~spec' : '';
  const location = finding.location !== undefined ? ` (${finding.location})` : '';
  const fix = finding.suggestedFix !== undefined ? `\n   fix: ${finding.suggestedFix}` : '';
  return `${index}. [${SEVERITY_LABEL[finding.severity]}]${tag} ${finding.title}${specTag}${location}\n   ${finding.detail}${fix}`;
}

export function buildFollowupMessage(result: DesignReviewData, label: string): string {
  const body = result.findings.map((finding, i) => renderFinding(finding, i + 1)).join('\n');
  const escalated = result.findings.filter((finding) => finding.escalate).length;
  const escalationLine =
    escalated > 0
      ? `For each of the ${escalated} finding(s) marked [ESCALATE], verify the claim against the code first (run an ephemeral node -e / python -c when it is a filter, regex, or test assertion). If verification DISPROVES a finding — the trigger does not fire, or the output it describes is actually correct — report it as a disproven false-positive and skip AskUserQuestion for it. Only CONFIRMED [ESCALATE] findings must be confirmed with me via AskUserQuestion — fix it, skip it, or change the approach — BEFORE editing the ${label}.`
      : 'None of the findings require my sign-off.';
  return [
    `A second-model ${label} review of the current ${label} (reviewer: ${result.reviewerAlias}, audit level: ${result.auditLevel}) found ${result.findings.length} issue(s):`,
    '',
    body,
    '',
    escalationLine,
    `These findings are against the reviewed ${label} file: \`${result.path}\`. Apply every fix to THAT file — do not search for or recreate the ${label} file. Do not implement anything beyond updating the ${label}.`,
  ].join('\n');
}

/** Shared core for both commands; `kind` selects the attack surface, guard, and wording. */
async function runSecondModelReview(
  host: SlashCommandHost,
  args: string,
  kind: ReviewKind,
): Promise<void> {
  const session = host.session;
  if (session === undefined) {
    host.showError(NO_ACTIVE_SESSION_MESSAGE);
    return;
  }

  const label = kind === 'plan' ? 'plan' : 'design';
  const Cap = kind === 'plan' ? 'Plan' : 'Design';
  const { path, modelAlias, timeoutMs } = parseArgs(args);

  // Pre-flight: when no explicit file path is given, the review will use the current
  // plan/design-mode file. If the user is not in that mode, there is nothing to
  // review and no point charging the reviewer model. Show the guard message early.
  const inMode =
    kind === 'plan'
      ? (host.state.appState.planMode ?? false)
      : (host.state.appState.designMode ?? false);
  if (!inMode && path === undefined) {
    host.showStatus(
      `Not in ${label} mode — enter ${label} mode first, or pass an explicit file path: /${label}-review <path>.`,
    );
    return;
  }

  host.showStatus(`Running ${label} review on the reviewer model…`);

  let result: DesignReviewData;
  try {
    result = await session.reviewDesign({
      ...(path !== undefined ? { path } : {}),
      ...(modelAlias !== undefined ? { modelAlias } : {}),
      ...(timeoutMs !== undefined ? { timeoutMs } : {}),
      kind,
    });
  } catch (error) {
    host.showError(`${Cap} review failed: ${error instanceof Error ? error.message : String(error)}`);
    return;
  }

  if (!result.ok) {
    host.showError(`${Cap} review unavailable: ${result.note ?? 'unknown error'}`);
    return;
  }
  if (result.findings.length === 0) {
    host.showStatus(
      `${Cap} review (${result.reviewerAlias}) found no issues. Audit level: ${result.auditLevel}.`,
    );
    return;
  }

  const escalated = result.findings.filter((finding) => finding.escalate).length;

  if (!inMode) {
    host.showStatus(
      `${Cap} review (${result.reviewerAlias}, ${result.auditLevel}): ${result.findings.length} finding(s).\n` +
        `Not in ${label} mode — enter ${label} mode, then re-run /${label}-review to have the model address the findings.`,
    );
    return;
  }

  host.showStatus(
    `${Cap} review (${result.reviewerAlias}, ${result.auditLevel}): ${result.findings.length} finding(s), ${escalated} need your sign-off. Handing to the ${label} model…`,
  );
  host.sendNormalUserInput(buildFollowupMessage(result, label));
}

export async function handleDesignReviewCommand(host: SlashCommandHost, args: string): Promise<void> {
  await runSecondModelReview(host, args, 'design');
}

export async function handlePlanReviewCommand(host: SlashCommandHost, args: string): Promise<void> {
  await runSecondModelReview(host, args, 'plan');
}
