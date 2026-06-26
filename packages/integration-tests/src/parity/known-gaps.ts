export type GapLayer = 'L2' | 'L3' | 'L4';

export interface KnownGap {
  readonly scenario: string;
  readonly layer: GapLayer;
  readonly reason: string;
}

const GAP_LAYERS: readonly GapLayer[] = ['L2', 'L3', 'L4'];

export function parseKnownGaps(markdown: string): KnownGap[] {
  const gaps: KnownGap[] = [];
  const lines = markdown.split('\n');
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed.startsWith('|') || trimmed.startsWith('|--')) continue;
    const cells = trimmed.split('|').map((c) => c.trim()).filter((c) => c.length > 0);
    if (cells.length < 3) continue;
    const [scenario, layer, ...reasonParts] = cells as [string, string, ...string[]];
    if (scenario === 'Scenario' || !GAP_LAYERS.includes(layer as GapLayer)) continue;
    gaps.push({ scenario, layer: layer as GapLayer, reason: reasonParts.join(' | ') });
  }
  return gaps;
}

export function findGap(
  gaps: readonly KnownGap[],
  scenarioName: string,
  layer: GapLayer,
): string | undefined {
  let wildcard: string | undefined;
  for (const gap of gaps) {
    if (gap.layer !== layer) continue;
    if (gap.scenario === scenarioName) {
      return gap.reason;
    }
    if (gap.scenario === '*') {
      wildcard = gap.reason;
    }
  }
  return wildcard;
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

export interface GapMatch {
  readonly layer: GapLayer;
  readonly reason: string;
}

export function affectedLayers(diffPaths: readonly string[]): readonly GapLayer[] {
  const found = new Set<GapLayer>();
  for (const path of diffPaths) {
    if (path.startsWith('$.responses')) found.add('L2');
    else if (path.startsWith('$.events')) found.add('L3');
    else if (path.startsWith('$.records') || path.startsWith('$.fsTree')) found.add('L4');
    else found.add('L3'); // errors / unknown defaults to L3
  }
  const ordered = GAP_LAYERS.filter((layer) => found.has(layer));
  return ordered.length > 0 ? ordered : ['L3'];
}

export function findGapForLayers(
  gaps: readonly KnownGap[],
  scenarioName: string,
  layers: readonly GapLayer[],
): GapMatch | undefined {
  for (const layer of layers) {
    const reason = findGap(gaps, scenarioName, layer);
    if (reason !== undefined) {
      return { layer, reason };
    }
  }
  return undefined;
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
