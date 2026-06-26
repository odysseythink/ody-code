// scripts/verify-phase-a3.mjs
import { existsSync } from 'node:fs';
import { rm, mkdtemp } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { spawn, execSync } from 'node:child_process';
import { tmpdir } from 'node:os';

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

const DEFAULT_STEP_TIMEOUTS = {
  'tui-smoke-stdio': 60_000,
  'tui-smoke-socket': 60_000,
  'tui-smoke-tcp': 60_000,
};

export function resolveTimeout(stepId, config) {
  return config.stepTimeoutsMs[stepId] ?? DEFAULT_STEP_TIMEOUTS[stepId] ?? config.defaultTimeoutMs;
}

export async function buildContext(config) {
  const workspaceRoot = process.cwd();
  const tempHomeDir = await mkdtemp(join(tmpdir(), 'phase-a3-'));
  const env = { ...process.env, ODY_HOME: tempHomeDir };
  return { config, workspaceRoot, tempHomeDir, env };
}

export function buildStepRegistry(config) {
  const steps = [
    { id: 'rust-test', name: 'Rust host unit tests' },
    { id: 'cross-lang-rpc', name: 'Cross-language RPC test' },
    { id: 'tui-smoke-stdio', name: 'TUI stdio smoke' },
    { id: 'tui-smoke-socket', name: 'TUI socket smoke' },
    { id: 'tui-smoke-tcp', name: 'TUI tcp smoke' },
    { id: 'sea-build', name: 'SEA full build' },
    { id: 'sea-smoke', name: 'Native smoke' },
    { id: 'typecheck', name: 'Workspace typecheck' },
  ];
  return steps
    .filter((s) => !config.skipSea || (s.id !== 'sea-build' && s.id !== 'sea-smoke'))
    .map((s) => ({
      ...s,
      run: (ctx) => runStepById(s.id, ctx),
    }));
}

async function runStepById(id, ctx) {
  const timeoutMs = resolveTimeout(id, ctx.config);
  if (id === 'rust-test') {
    return wrapResult(await executeCommand('pnpm', ['run', 'test:host'], { cwd: ctx.workspaceRoot, env: ctx.env, timeoutMs }), 'pnpm', ['run', 'test:host'], ctx.workspaceRoot);
  }
  if (id === 'cross-lang-rpc') {
    return wrapResult(await executeCommand('pnpm', ['vitest', 'run', 'packages/node-sdk/test/rust-host-connect.test.ts'], { cwd: ctx.workspaceRoot, env: ctx.env, timeoutMs }), 'pnpm', ['vitest', 'run', 'packages/node-sdk/test/rust-host-connect.test.ts'], ctx.workspaceRoot);
  }
  if (id.startsWith('tui-smoke-')) {
    return runTuiSmoke(id.replace('tui-smoke-', ''), ctx, timeoutMs);
  }
  if (id === 'sea-build') {
    return wrapResult(await executeCommand('pnpm', ['--filter', 'ody-code', 'run', 'build:native:sea'], { cwd: ctx.workspaceRoot, env: ctx.env, timeoutMs }), 'pnpm', ['--filter', 'ody-code', 'run', 'build:native:sea'], ctx.workspaceRoot);
  }
  if (id === 'sea-smoke') {
    return wrapResult(await executeCommand('pnpm', ['--filter', 'ody-code', 'run', 'test:native:smoke'], { cwd: ctx.workspaceRoot, env: ctx.env, timeoutMs }), 'pnpm', ['--filter', 'ody-code', 'run', 'test:native:smoke'], ctx.workspaceRoot);
  }
  if (id === 'typecheck') {
    return wrapResult(await executeCommand('pnpm', ['-r', 'typecheck'], { cwd: ctx.workspaceRoot, env: ctx.env, timeoutMs }), 'pnpm', ['-r', 'typecheck'], ctx.workspaceRoot);
  }
  throw new Error(`Unknown step id: ${id}`);
}

function wrapResult(result, command, args, cwd) {
  return { ...result, command, args, cwd };
}

async function runTuiSmoke(transport, ctx, timeoutMs) {
  const baseArgs = ['--filter', 'ody-code', 'run', 'dev:cli-only', '--', '--host=rust', '--smoke-test'];
  if (transport === 'stdio') {
    baseArgs.push('--host-stdio');
  } else if (transport === 'socket') {
    baseArgs.push('--host-socket', join(ctx.tempHomeDir, 'ody-smoke.sock'));
  } else if (transport === 'tcp') {
    const basePort = 19090;
    const maxAttempts = 10;
    let lastResult;
    for (let offset = 0; offset < maxAttempts; offset += 1) {
      const port = basePort + offset;
      const args = [...baseArgs, '--host-tcp', `127.0.0.1:${port}`];
      lastResult = wrapResult(await executeCommand('pnpm', args, { cwd: ctx.workspaceRoot, env: ctx.env, timeoutMs }), 'pnpm', args, ctx.workspaceRoot);
      if (lastResult.status === 'passed') return lastResult;
      const combined = `${lastResult.stdoutRedacted}\n${lastResult.stderrRedacted}`;
      if (!/eaddrinuse|address already in use/i.test(combined)) return lastResult;
    }
    return lastResult;
  }
  return wrapResult(await executeCommand('pnpm', baseArgs, { cwd: ctx.workspaceRoot, env: ctx.env, timeoutMs }), 'pnpm', baseArgs, ctx.workspaceRoot);
}

export function buildMetadata(config) {
  return {
    nodeVersion: process.version,
    platform: process.platform,
    arch: process.arch,
    timestamp: new Date().toISOString(),
    hostBinaryPath: config.hostBinaryPath,
  };
}

export function buildEnvironment() {
  return {
    cwd: process.cwd(),
    pnpmVersion: execSync('pnpm --version', { encoding: 'utf-8' }).trim(),
    cargoVersion: execSync('cargo --version', { encoding: 'utf-8' }).trim(),
    rustcVersion: execSync('rustc --version', { encoding: 'utf-8' }).trim(),
  };
}

export function buildSummary(results, totalDurationMs) {
  const passedCount = results.filter((r) => r.status === 'passed').length;
  const failedCount = results.filter((r) => r.status === 'failed').length;
  const skippedCount = results.filter((r) => r.status === 'skipped').length;
  let overallStatus = 'passed';
  if (failedCount > 0) overallStatus = results.length === failedCount ? 'failed' : 'partial';
  return { overallStatus, passedCount, failedCount, skippedCount, totalDurationMs };
}

export async function runVerification(config) {
  ensureNodeVersion('24.15.0');
  if (!existsSync(config.hostBinaryPath)) {
    throw new Error(`ody-host binary not found at ${config.hostBinaryPath}. Build with "pnpm run build:host" or set ODY_HOST_BINARY_PATH.`);
  }
  const ctx = await buildContext(config);
  const steps = buildStepRegistry(config);
  const results = [];
  const startedAt = Date.now();
  try {
    for (const step of steps) {
      const base = await step.run(ctx);
      const result = { ...base, id: step.id, name: step.name };
      results.push(result);
      if (result.status === 'failed') break;
    }
  } finally {
    if (!config.keepTemp) {
      await rm(ctx.tempHomeDir, { recursive: true, force: true }).catch(() => {});
    }
  }
  return {
    metadata: buildMetadata(config),
    environment: buildEnvironment(),
    steps: results,
    summary: buildSummary(results, Date.now() - startedAt),
  };
}
