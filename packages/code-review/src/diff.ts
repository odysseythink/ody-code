import { spawn } from 'node:child_process';
import type { CodeReviewDiffSource } from './types';
import { formatGitDiff } from './wasm-diff';

const GH_PR_REGEX = /^https?:\/\/github\.com\/([^/]+)\/([^/]+)\/pull\/(\d+)$/i;

export function parsePrNumber(urlOrNumber: string): string {
  const trimmed = urlOrNumber.trim();
  if (/^\d+$/.test(trimmed)) return trimmed;
  const match = trimmed.match(GH_PR_REGEX);
  if (match === null) {
    throw new Error('PR URL must be a GitHub pull request URL (e.g. https://github.com/owner/repo/pull/42)');
  }
  const prNumber = match[3];
  if (prNumber === undefined) {
    throw new Error('Failed to extract PR number from URL');
  }
  return prNumber;
}

export function buildDiffSource(options: {
  readonly base?: string | undefined;
  readonly head?: string | undefined;
  readonly pr?: string | undefined;
}): CodeReviewDiffSource {
  if (options.pr !== undefined) {
    return { kind: 'pr', prUrlOrNumber: options.pr };
  }
  if (options.base !== undefined || options.head !== undefined) {
    const base = options.base ?? 'HEAD~1';
    const head = options.head ?? 'HEAD';
    return { kind: 'commits', base, head };
  }
  return { kind: 'working-tree' };
}

export async function fetchDiff(
  source: CodeReviewDiffSource,
  cwd: string,
  _signal?: AbortSignal,
  opts?: { readonly env?: Record<string, string | undefined> },
): Promise<string> {
  let raw: string;
  switch (source.kind) {
    case 'commits':
      raw = await runGitDiff(['diff', source.base, source.head], cwd, opts);
      break;
    case 'working-tree':
      raw = await runGitDiff(['diff'], cwd, opts);
      break;
    case 'pr':
      raw = await runGhPrDiff(parsePrNumber(source.prUrlOrNumber), cwd, opts);
      break;
  }
  return formatGitDiff(raw);
}

async function runGitDiff(
  args: string[],
  cwd: string,
  opts?: { readonly env?: Record<string, string | undefined> },
): Promise<string> {
  return runCommand('git', args, cwd, opts);
}

async function runGhPrDiff(
  prNumber: string,
  cwd: string,
  opts?: { readonly env?: Record<string, string | undefined> },
): Promise<string> {
  const diff = await runCommand('gh', ['pr', 'diff', prNumber], cwd, opts);
  if (diff.trim().length === 0) {
    throw new Error('PR diff is empty. Ensure gh CLI is authenticated (gh auth login) and the PR exists.');
  }
  return diff;
}

function runCommand(
  command: string,
  args: string[],
  cwd: string,
  opts?: { readonly env?: Record<string, string | undefined> },
): Promise<string> {
  return new Promise<string>((resolve, reject) => {
    const child = spawn(command, args, {
      cwd,
      env: opts?.env ?? process.env,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const stdoutChunks: Buffer[] = [];
    const stderrChunks: Buffer[] = [];
    child.stdout.on('data', (chunk: Buffer) => stdoutChunks.push(chunk));
    child.stderr.on('data', (chunk: Buffer) => stderrChunks.push(chunk));
    child.on('error', (err) => {
      reject(new Error(`${command} failed to start: ${err.message}`));
    });
    child.on('close', (code) => {
      if (code !== 0) {
        const stderr = Buffer.concat(stderrChunks).toString('utf-8').trim();
        reject(new Error(`${command} ${args.join(' ')} exited with ${code}${stderr ? ': ' + stderr : ''}`));
      } else {
        resolve(Buffer.concat(stdoutChunks).toString('utf-8'));
      }
    });
  });
}
