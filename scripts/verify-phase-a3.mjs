// scripts/verify-phase-a3.mjs
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

export function ensureNodeVersion(minVersion, currentVersion = process.version) {
  const current = parseSemver(currentVersion);
  const minimum = parseSemver(minVersion);
  if (
    current.major < minimum.major ||
    (current.major === minimum.major && current.minor < minimum.minor) ||
    (current.major === minimum.major && current.minor === minimum.minor && current.patch < minimum.patch)
  ) {
    throw new Error(`Node ${minVersion}+ required, found ${currentVersion}`);
  }
}

function parseSemver(version) {
  const match = version.match(/^v?(\d+)\.(\d+)\.(\d+)/);
  if (!match) throw new Error(`Invalid version string: ${version}`);
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
  };
}

export function parseConfig(argv, workspaceRoot, env = process.env, platform = process.platform) {
  const args = new Set(argv);
  const hostBinaryPath = env.ODY_HOST_BINARY_PATH
    ? resolve(env.ODY_HOST_BINARY_PATH)
    : resolve(workspaceRoot, 'rust-ody', 'target', 'release', 'ody-host');
  const reportDir = env.ODY_CODE_REPORT_DIR
    ? resolve(env.ODY_CODE_REPORT_DIR)
    : resolve(workspaceRoot, '.ody-code', 'reports');
  const defaultTimeoutMs = Number(env.ODY_CODE_DEFAULT_TIMEOUT_MS ?? '300000');
  const stepTimeoutsMs = parseStepTimeouts(env.ODY_CODE_STEP_TIMEOUTS);
  const skipSea = args.has('--skip-sea') || env.ODY_CODE_SKIP_SEA === '1' || platform === 'win32';
  const keepTemp = args.has('--keep-temp') || env.ODY_CODE_KEEP_TEMP === '1';
  return { hostBinaryPath, reportDir, defaultTimeoutMs, stepTimeoutsMs, skipSea, keepTemp };
}

function parseStepTimeouts(value) {
  if (!value) return {};
  const result = {};
  for (const pair of value.split(',')) {
    const [id, ms] = pair.split(':');
    if (id && ms) result[id.trim()] = Number(ms.trim());
  }
  return result;
}
