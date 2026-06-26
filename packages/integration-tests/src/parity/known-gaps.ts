export type GapLayer = 'L2' | 'L3' | 'L4';

export interface KnownGap {
  readonly scenario: string;
  readonly layer: GapLayer;
  readonly reason: string;
}

export function parseKnownGaps(markdown: string): KnownGap[] {
  const gaps: KnownGap[] = [];
  const lines = markdown.split('\n');
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed.startsWith('|') || trimmed.startsWith('|--')) continue;
    const cells = trimmed.split('|').map((c) => c.trim()).filter((c) => c.length > 0);
    if (cells.length < 3) continue;
    const [scenario, layer, ...reasonParts] = cells;
    if (scenario === 'Scenario' || !['L2', 'L3', 'L4'].includes(layer)) continue;
    gaps.push({ scenario, layer: layer as GapLayer, reason: reasonParts.join(' | ') });
  }
  return gaps;
}

export function findGap(
  gaps: readonly KnownGap[],
  scenarioName: string,
  layer: GapLayer,
): string | undefined {
  for (const gap of gaps) {
    if (gap.layer !== layer) continue;
    if (gap.scenario === '*' || gap.scenario === scenarioName) {
      return gap.reason;
    }
  }
  return undefined;
}

export class StaleGapError extends Error {
  constructor(
    readonly scenario: string,
    readonly layer: GapLayer,
  ) {
    super(
      `Known gap for scenario "${scenario}" layer ${layer} is stale: the scenario now passes. Remove it from known-gaps.md.`,
    );
    this.name = 'StaleGapError';
  }
}

export function checkGapState(
  gaps: readonly KnownGap[],
  scenarioName: string,
  layer: GapLayer,
  passed: boolean,
): void {
  const reason = findGap(gaps, scenarioName, layer);
  if (reason !== undefined && passed) {
    throw new StaleGapError(scenarioName, layer);
  }
}
