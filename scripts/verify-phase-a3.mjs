// scripts/verify-phase-a3.mjs
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawn } from 'node:child_process';

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

export function redact(text) {
  return text
    .replace(/"api_key"\s*:\s*"([^"]{4,})"/gi, (_, value) => `"api_key":"${value.slice(0, 4)}***"`)
    .replace(/"access_token"\s*:\s*"([^"]{4,})"/gi, (_, value) => `"access_token":"${value.slice(0, 4)}***"`)
    .replace(/"password"\s*:\s*"([^"]*)"/gi, (_, value) => `"password":"${value.slice(0, 4)}***"`)
    .replace(/"secret"\s*:\s*"([^"]{4,})"/gi, (_, value) => `"secret":"${value.slice(0, 4)}***"`)
    .replace(/authorization:\s*bearer\s+(\S+)/gi, (_, token) => `authorization: bearer ${token.slice(0, 4)}***`)
    .replace(/(api[_-]?key)([=:])\s*(\S+)/gi, (_, key, sep, value) => `${key}${sep}${value.slice(0, 4)}***`);
}

export function executeCommand(command, args, options) {
  return new Promise((resolve) => {
    const startedAt = Date.now();
    const child = spawn(command, args, {
      cwd: options.cwd,
      env: options.env,
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    const stdoutChunks = [];
    const stderrChunks = [];
    child.stdout.on('data', (chunk) => stdoutChunks.push(chunk));
    child.stderr.on('data', (chunk) => stderrChunks.push(chunk));

    let timedOut = false;
    const timeout = setTimeout(() => {
      timedOut = true;
      child.kill('SIGTERM');
      // Escalate to SIGKILL if the child refuses to die.
      setTimeout(() => {
        if (!child.killed) child.kill('SIGKILL');
      }, 5000).unref();
    }, options.timeoutMs);

    const finish = (fields) => {
      clearTimeout(timeout);
      resolve({
        status: fields.status,
        exitCode: fields.exitCode ?? null,
        signal: fields.signal ?? null,
        durationMs: Date.now() - startedAt,
        stdoutRedacted: redact(Buffer.concat(stdoutChunks).toString()),
        stderrRedacted: redact(Buffer.concat(stderrChunks).toString()),
        ...(fields.errorMessage ? { errorMessage: fields.errorMessage } : {}),
      });
    };

    child.on('error', (error) => {
      finish({ status: 'failed', errorMessage: error.message });
    });

    child.on('exit', (exitCode, signal) => {
      if (timedOut) {
        finish({ status: 'failed', signal, errorMessage: `Step timed out after ${options.timeoutMs}ms` });
        return;
      }
      const status = exitCode === 0 ? 'passed' : 'failed';
      finish({ status, exitCode, signal });
    });
  });
}
