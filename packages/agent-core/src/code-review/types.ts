export type CodeReviewDiffSource =
  | { readonly kind: 'commits'; readonly base: string; readonly head: string }
  | { readonly kind: 'pr'; readonly prUrlOrNumber: string }
  | { readonly kind: 'working-tree' };

export interface CodeReviewRequestInput {
  readonly source: CodeReviewDiffSource;
  readonly modelAlias: string;
  readonly description?: string | undefined;
  readonly requirements?: string | undefined;
  readonly deep?: boolean | undefined;
  readonly timeoutMs?: number | undefined;
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
