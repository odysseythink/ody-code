import {
  KimiHarness,
  renderCodeReviewReportToMarkdown,
} from '@odysseythink/ody-code-sdk';
import type { Command } from 'commander';

import { OptionConflictError } from '#/cli/options';
import { createKimiCodeHostIdentity } from '#/cli/version';

interface RequestCodeReviewCLIOptions {
  base?: string | undefined;
  head?: string | undefined;
  pr?: string | undefined;
  model?: string | undefined;
  description?: string | undefined;
  requirements?: string | undefined;
  deep?: boolean | undefined;
  timeout?: number | undefined;
}

export interface RequestCodeReviewDeps {
  readonly getHarness: () => KimiHarness;
  readonly stdout: { write(msg: string): void };
  readonly stderr: { write(msg: string): void };
  readonly exit: (code: number) => void;
}

export function validateRequestCodeReviewOptions(
  opts: RequestCodeReviewCLIOptions,
): void {
  if (opts.pr !== undefined && (opts.base !== undefined || opts.head !== undefined)) {
    throw new OptionConflictError('Cannot combine --pr with --base/--head.');
  }

  if (opts.pr === undefined && opts.base === undefined && opts.head === undefined) {
    opts.base = 'HEAD~1';
    opts.head = 'HEAD';
  }

  if (opts.base !== undefined && opts.head === undefined) {
    opts.head = 'HEAD';
  }

  if (opts.timeout !== undefined) {
    if (!Number.isInteger(opts.timeout) || opts.timeout <= 0) {
      throw new OptionConflictError('--timeout must be a positive integer (seconds).');
    }
  }
}

export function buildDiffSource(opts: {
  base?: string | undefined;
  head?: string | undefined;
  pr?: string | undefined;
}) {
  if (opts.pr !== undefined) {
    return { kind: 'pr' as const, prUrlOrNumber: opts.pr };
  }
  if (opts.base !== undefined || opts.head !== undefined) {
    return {
      kind: 'commits' as const,
      base: opts.base ?? 'HEAD~1',
      head: opts.head ?? 'HEAD',
    };
  }
  return { kind: 'working-tree' as const };
}

export async function handleRequestCodeReview(
  opts: RequestCodeReviewCLIOptions,
  deps: RequestCodeReviewDeps,
): Promise<void> {
  validateRequestCodeReviewOptions(opts);

  const harness = deps.getHarness();
  await harness.ensureConfigFile();

  const source = buildDiffSource(opts);

  try {
    const report = await harness.requestCodeReview({
      source,
      modelAlias: opts.model,
      description: opts.description,
      requirements: opts.requirements,
      deep: opts.deep,
      timeoutMs: opts.timeout !== undefined ? opts.timeout * 1000 : undefined,
    });

    if (!report.ok) {
      deps.stderr.write(`${report.note ?? 'Code review failed.'}\n`);
      deps.exit(1);
      return;
    }

    deps.stdout.write(`${renderCodeReviewReportToMarkdown(report)}\n`);
  } catch (error) {
    deps.stderr.write(
      `Code review failed: ${error instanceof Error ? error.message : String(error)}\n`,
    );
    deps.exit(1);
  }
}

export function registerRequestCodeReviewCommand(parent: Command): void {
  parent
    .command('request-code-review')
    .description('Request a code review for the current changes.')
    .option('--base <sha>', 'Base commit for the review range.')
    .option('--head <sha>', 'Head commit for the review range. Defaults to HEAD.')
    .option('--pr <url-or-number>', 'Review a GitHub PR (URL or number).')
    .option('-m, --model <model>', 'Model alias to use for this review.')
    .option('-d, --description <text>', 'Short description of what was built.')
    .option('-r, --requirements <text>', 'Requirements or plan the changes should meet.')
    .option('--deep', 'Dispatch a reviewer subagent for deeper analysis.', false)
    .option('-t, --timeout <seconds>', 'Timeout for the review in seconds.', (v: string) => parseInt(v, 10))
    .action(async (opts: RequestCodeReviewCLIOptions) => {
      const identity = createKimiCodeHostIdentity();
      const harness = new KimiHarness({ identity });
      await handleRequestCodeReview(opts, {
        getHarness: () => harness,
        stdout: process.stdout,
        stderr: process.stderr,
        exit: (code: number) => process.exit(code),
      });
    });
}
