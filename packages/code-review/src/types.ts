export type CodeReviewDiffSource =
  | { readonly kind: 'commits'; readonly base: string; readonly head: string }
  | { readonly kind: 'pr'; readonly prUrlOrNumber: string }
  | { readonly kind: 'working-tree' };

export type CodeReviewProgressStage =
  | 'preparing'
  | 'fetching-diff'
  | 'audit-scanning'
  | 'deep-review'
  | 'generating'
  | 'completed'
  | 'failed';

export interface CodeReviewProgress {
  readonly requestId: string;
  readonly stage: CodeReviewProgressStage;
  readonly modelAlias: string;
  readonly detail?: string | undefined;
  readonly meta?: {
    readonly estimatedTokens?: number | undefined;
    readonly filePath?: string | undefined;
    readonly fileCount?: number | undefined;
  } | undefined;
}

export interface CodeReviewRequestInput {
  readonly source: CodeReviewDiffSource;
  readonly modelAlias: string;
  readonly description?: string | undefined;
  readonly requirements?: string | undefined;
  readonly deep?: boolean | undefined;
  readonly timeoutMs?: number | undefined;
  readonly focus?: 'correctness' | 'simplicity' | undefined;
  readonly scope?: 'diff' | 'repo' | undefined;
  readonly signal?: AbortSignal | undefined;
  readonly onProgress?: ((progress: CodeReviewProgress) => void) | undefined;
}

export interface CodeReviewFinding {
  readonly severity: 'critical' | 'important' | 'minor';
  readonly title: string;
  readonly detail: string;
  readonly location?: string | undefined;
  readonly suggestedFix?: string | undefined;
}

export interface CodeReviewReport {
  readonly ok: boolean;
  readonly reviewerAlias: string;
  readonly summary?: string | undefined;
  readonly findings: readonly CodeReviewFinding[];
  readonly note?: string | undefined;
}
