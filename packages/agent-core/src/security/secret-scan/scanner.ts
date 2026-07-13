export interface SecretScanRule {
  readonly id: string;
  readonly name: string;
  readonly pattern: RegExp;
  /** Minimum Shannon entropy (0-8) for the matched text. Undefined = skip entropy check. */
  readonly entropyMin?: number;
}

export interface SecretScanMatch {
  readonly ruleId: string;
  readonly ruleName: string;
  readonly matchedText: string;
  readonly start: number;
  readonly end: number;
}

export interface SecretScanOptions {
  /** Maximum number of bytes to scan per input string. */
  readonly maxScanBytes?: number;
  /** Default entropy threshold for rules that do not specify entropyMin. */
  readonly entropyThreshold?: number;
  /** Exact strings that should never be reported. */
  readonly allowList?: readonly string[];
}

const DEFAULT_MAX_SCAN_BYTES = 8 * 1024;

export class SecretLeakScanner {
  constructor(
    private readonly rules: readonly SecretScanRule[],
    private readonly options: SecretScanOptions = {},
  ) {}

  scan(text: string): SecretScanMatch[] {
    const maxBytes = this.options.maxScanBytes ?? DEFAULT_MAX_SCAN_BYTES;
    const allowList = this.options.allowList ?? [];
    const allowSet = new Set(allowList);
    const target = truncateBytes(text, maxBytes);

    const matches: SecretScanMatch[] = [];
    for (const rule of this.rules) {
      const pattern = new RegExp(
        rule.pattern.source,
        rule.pattern.flags.includes('g') ? rule.pattern.flags : `${rule.pattern.flags}g`,
      );
      for (const m of target.matchAll(pattern)) {
        const raw = m[0] ?? '';
        const secretPart = m[1] ?? raw;
        if (allowSet.has(secretPart)) continue;
        const entropyMin = rule.entropyMin ?? this.options.entropyThreshold;
        if (entropyMin !== undefined && shannonEntropy(secretPart) < entropyMin) {
          continue;
        }
        const start = m.index ?? 0;
        matches.push({
          ruleId: rule.id,
          ruleName: rule.name,
          matchedText: secretPart,
          start,
          end: start + secretPart.length,
        });
      }
    }
    return matches;
  }
}

function truncateBytes(text: string, maxBytes: number): string {
  if (Buffer.byteLength(text, 'utf8') <= maxBytes) return text;
  let low = 0;
  let high = text.length;
  while (low < high) {
    const mid = Math.ceil((low + high) / 2);
    if (Buffer.byteLength(text.slice(0, mid), 'utf8') <= maxBytes) {
      low = mid;
    } else {
      high = mid - 1;
    }
  }
  return text.slice(0, low);
}

function shannonEntropy(input: string): number {
  const len = input.length;
  if (len === 0) return 0;
  const counts = new Map<string, number>();
  for (const ch of input) {
    counts.set(ch, (counts.get(ch) ?? 0) + 1);
  }
  let entropy = 0;
  for (const count of counts.values()) {
    const p = count / len;
    entropy -= p * Math.log2(p);
  }
  return entropy;
}
