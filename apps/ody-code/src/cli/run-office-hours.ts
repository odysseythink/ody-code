import { execSync } from 'node:child_process';
import { basename } from 'node:path';

import {
  setCrashPhase,
  setTelemetryContext,
  shutdownTelemetry,
  track,
  withTelemetryContext,
} from '@odysseythink/ody-telemetry';
import { KimiHarness, log, type TelemetryClient } from '@odysseythink/ody-code-sdk';

import { CLI_SHUTDOWN_TIMEOUT_MS, CLI_UI_MODE } from '#/constant/app';
import type { TuiConfig } from '#/tui/config';
import { loadTuiConfig, TuiConfigParseError } from '#/tui/config';
import { KimiTUI } from '#/tui/index';
import { detectTerminalTheme } from '#/tui/theme/detect';

import type { CLIOptions } from './options';
import { createCliTelemetryBootstrap, initializeCliTelemetry } from './telemetry';
import { createKimiCodeHostIdentity } from './version';

export async function runOfficeHours(opts: CLIOptions, version: string): Promise<void> {
  const startedAt = Date.now();
  let tuiConfig: TuiConfig;
  let configWarning: string | undefined;
  try {
    tuiConfig = await loadTuiConfig();
  } catch (error) {
    if (!(error instanceof TuiConfigParseError)) throw error;
    tuiConfig = error.fallback;
    configWarning = error.message;
  }

  const resolvedTheme =
    tuiConfig.theme === 'auto' ? await detectTerminalTheme() : tuiConfig.theme;

  const workDir = process.cwd();
  const telemetryBootstrap = createCliTelemetryBootstrap();
  const telemetryClient: TelemetryClient = {
    track,
    withContext: withTelemetryContext,
    setContext: setTelemetryContext,
  };
  const harness = new KimiHarness({
    homeDir: telemetryBootstrap.homeDir,
    identity: createKimiCodeHostIdentity(version),
    telemetry: telemetryClient,
  });
  log.info('kimi-code starting in office-hours mode', {
    version,
    uiMode: CLI_UI_MODE,
    nodeVersion: process.version,
    platform: `${process.platform}/${process.arch}`,
    workDir,
  });
  await harness.ensureConfigFile();
  const config = await harness.getConfig();

  const tui = new KimiTUI(harness, {
    cliOptions: { ...opts, sessionMode: 'office-hours', officeHours: true },
    tuiConfig,
    version,
    workDir,
    startupNotice: configWarning,
    resolvedTheme,
    officeHours: true,
  });

  initializeCliTelemetry({
    harness,
    bootstrap: telemetryBootstrap,
    config,
    version,
    uiMode: CLI_UI_MODE,
  });
  setCrashPhase('runtime');

  track('office_hours_started', { project_slug: basename(workDir) });

  tui.onExit = async (exitCode = 0) => {
    const sessionId = tui.getCurrentSessionId();
    setCrashPhase('shutdown');
    withTelemetryContext({ sessionId }).track('office_hours_completed', {
      duration_s: (Date.now() - startedAt) / 1000,
      project_slug: basename(workDir),
      outcome: exitCode === 0 ? 'success' : 'abort',
    });
    await shutdownTelemetry({ timeoutMs: CLI_SHUTDOWN_TIMEOUT_MS });
    process.exit(exitCode);
  };

  try {
    execSync('stty -ixon', { stdio: 'ignore' });
  } catch {
    /* ignore */
  }

  try {
    await tui.start();
  } catch (error) {
    setCrashPhase('shutdown');
    await shutdownTelemetry({ timeoutMs: CLI_SHUTDOWN_TIMEOUT_MS });
    await harness.close();
    throw error;
  }
}
