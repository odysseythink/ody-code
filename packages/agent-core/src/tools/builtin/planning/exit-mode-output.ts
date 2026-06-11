/**
 * Shared helpers for the ExitPlanMode / ExitDesignMode tools: reading the approval
 * `executionMetadata` and rendering the "Selected approach" directive. Kept in one
 * place so the plan and design exit tools stay consistent (e.g. both validate the
 * chosen label against the declared options before surfacing it).
 */

/** Minimal shape of an approval option — both tools' option types satisfy this. */
export interface ExitModeOptionLike {
  readonly label: string;
}

/** Whether the approval policy marked this execution as user-approved via the review
 * surface (vs. an auto-mode direct execution). */
export function isViaApproval(metadata: unknown): boolean {
  return (
    metadata !== null &&
    typeof metadata === 'object' &&
    'viaApproval' in metadata &&
    (metadata as { viaApproval?: unknown }).viaApproval === true
  );
}

/** The raw `selectedLabel` the approval returned, if any. May be a reserved label
 * (e.g. "Approve"); callers that surface it as a chosen approach should first pass it
 * through {@link declaredOptionLabel}. */
export function selectedLabelOf(metadata: unknown): string | undefined {
  if (metadata === null || typeof metadata !== 'object' || !('selectedLabel' in metadata)) {
    return undefined;
  }
  const label = (metadata as { selectedLabel?: unknown }).selectedLabel;
  return typeof label === 'string' ? label : undefined;
}

/** The label only when it matches a declared option, so reserved approval labels
 * (e.g. "Approve") never surface as a chosen approach. */
export function declaredOptionLabel(
  options: readonly ExitModeOptionLike[] | undefined,
  label: string | undefined,
): string | undefined {
  if (options === undefined || label === undefined) return undefined;
  return options.some((option) => option.label === label) ? label : undefined;
}

/** The "Selected approach: …" directive prefix for a chosen option, or '' when none. */
export function selectedApproachPrefix(label: string | undefined): string {
  return label !== undefined && label.length > 0
    ? `Selected approach: ${label}\nExecute ONLY the selected approach. Do not execute any unselected alternatives.\n\n`
    : '';
}
