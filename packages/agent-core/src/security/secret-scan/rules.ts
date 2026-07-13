import { SecretLeakScanner } from './scanner';
import { normalizeAllowList } from './allow-list';

export const DEFAULT_SECRET_SCAN_RULES = [
  {
    id: 'aws-access-key-id',
    name: 'AWS Access Key ID',
    pattern: /\b(AKIA[0-9A-Z]{16})\b/g,
  },
  {
    id: 'aws-secret-access-key',
    name: 'AWS Secret Access Key',
    pattern: /([A-Za-z0-9/+=]{40})/g,
    entropyMin: 4.2,
  },
  {
    id: 'github-pat',
    name: 'GitHub Personal Access Token',
    pattern: /\b(ghp_[A-Za-z0-9_]{36})\b/g,
  },
  {
    id: 'generic-api-key',
    name: 'Generic API Key',
    pattern: /(?:api[_-]?key|apikey)\s*[:=]\s*['"]?([A-Za-z0-9_\-/+=]{16,})['"]?/gi,
    entropyMin: 4.0,
  },
  {
    id: 'generic-secret',
    name: 'Generic Secret',
    pattern: /(?:secret|password|passwd|pwd)\s*[:=]\s*['"]?([A-Za-z0-9_\-/+=]{16,})['"]?/gi,
    entropyMin: 4.0,
  },
  {
    id: 'generic-token',
    name: 'Generic Token',
    pattern: /(?:token)\s*[:=]\s*['"]?([A-Za-z0-9_\-/+=]{16,})['"]?/gi,
    entropyMin: 4.0,
  },
  {
    id: 'jwt',
    name: 'JSON Web Token',
    pattern: /\b(eyJ[A-Za-z0-9_-]*\.eyJ[A-Za-z0-9_-]*\.[A-Za-z0-9_-]*)\b/g,
  },
] as const;

/** Built-in examples / placeholders that must never be flagged. */
export const DEFAULT_SECRET_SCAN_ALLOW_LIST = [
  'EXAMPLE_KEY',
  'YOUR_API_KEY',
  '1234567890abcdef',
  'example-token',
];

export interface DefaultScannerOptions {
  readonly maxScanBytes?: number;
  readonly entropyThreshold?: number;
  readonly allowList?: readonly string[];
}

export function createDefaultScanner(options: DefaultScannerOptions = {}): SecretLeakScanner {
  const allowList = normalizeAllowList([
    ...DEFAULT_SECRET_SCAN_ALLOW_LIST,
    ...(options.allowList ?? []),
  ]);
  return new SecretLeakScanner([...DEFAULT_SECRET_SCAN_RULES], {
    maxScanBytes: options.maxScanBytes,
    entropyThreshold: options.entropyThreshold,
    allowList,
  });
}
