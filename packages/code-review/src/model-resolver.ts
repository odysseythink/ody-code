import { ErrorCodes, OdyError } from '@odysseythink/agent-core-shared';
import type { OdyConfig } from '@odysseythink/agent-core-shared';

export interface ResolveModelOverrides {
  readonly explicit?: string | undefined;
  readonly sessionModel?: string | undefined;
}

export function resolveCodeReviewModel(
  kind: 'request' | 'receive',
  modeModels: OdyConfig['modeModels'],
  defaultModel: string | undefined,
  overrides: ResolveModelOverrides,
  validate: (alias: string) => boolean,
): string {
  const candidates: (string | undefined)[] = [];

  if (kind === 'request' && overrides.explicit !== undefined) {
    candidates.push(overrides.explicit);
  }

  if (kind === 'request') {
    candidates.push(modeModels?.codeReviewRequest);
  } else {
    candidates.push(modeModels?.codeReviewReceive);
  }

  candidates.push(modeModels?.codeReview);
  candidates.push(modeModels?.review);

  if (overrides.sessionModel !== undefined) {
    candidates.push(overrides.sessionModel);
  }

  candidates.push(defaultModel);

  for (const alias of candidates) {
    if (isNonEmptyString(alias) && validate(alias)) {
      return alias;
    }
  }

  throw new OdyError(
    ErrorCodes.CONFIG_INVALID,
    'No usable model for code review. Configure [mode_models] with code_review, code_review_request, or code_review_receive, or set default_model.',
  );
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === 'string' && value.trim().length > 0;
}
