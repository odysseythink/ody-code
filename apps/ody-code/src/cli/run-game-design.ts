/**
 * Game Design mode runner.
 *
 * Mirrors run-office-hours.ts: creates OdyTUI with sessionMode='game-design',
 * tracks telemetry events, and handles exit.
 */
import { basename } from 'node:path';

import { KimiHarness, log, type RuntimeMode, type TelemetryClient } from '@odysseythink/ody-code-sdk';
import {
  setCrashPhase,
  setTelemetryContext,
  shutdownTelemetry,
  track,
  withTelemetryContext,
} from '@odysseythink/ody-telemetry';

import { CLI_SHUTDOWN_TIMEOUT_MS, CLI_UI_MODE } from '#/constant/app';
import type { TuiConfig } from '#/tui/config';
import { loadTuiConfig, TuiConfigParseError } from '#/tui/config';
import { OdyTUI } from '#/tui/index';
import { detectTerminalTheme } from '#/tui/theme/detect';

import type { CLIOptions } from './options';
import { createCliTelemetryBootstrap, initializeCliTelemetry } from './telemetry';
import { createKimiCodeHostIdentity } from './version';

export async function runGameDesign(opts: CLIOptions, version: string): Promise<void> {
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
  log.info('kimi-code starting in game-design mode', {
    version,
    uiMode: CLI_UI_MODE,
    nodeVersion: process.version,
    platform: `${process.platform}/${process.arch}`,
    workDir,
  });
  await harness.ensureConfigFile();
  const config = await harness.getConfig();

  const tui = new OdyTUI(harness, {
    cliOptions: { ...opts, sessionMode: 'game-design' as RuntimeMode, gameDesign: true },
    tuiConfig,
    version,
    workDir,
    startupNotice: configWarning,
    resolvedTheme,
    officeHours: false,
    gameDesign: true,
  });

  initializeCliTelemetry({
    harness,
    bootstrap: telemetryBootstrap,
    config,
    version,
    uiMode: CLI_UI_MODE,
  });
  setCrashPhase('runtime');

  track('game_design_started', { project_slug: basename(workDir) });

  tui.onExit = async (exitCode = 0) => {
    const sessionId = tui.getCurrentSessionId();
    setCrashPhase('shutdown');
    withTelemetryContext({ sessionId }).track('game_design_completed', {
      duration_s: (Date.now() - startedAt) / 1000,
      project_slug: basename(workDir),
      outcome: exitCode === 0 ? 'success' : 'abort',
    });
    await shutdownTelemetry({ timeoutMs: CLI_SHUTDOWN_TIMEOUT_MS });
    process.exit(exitCode);
  };

  try {
    await tui.start();
  } catch (error) {
    setCrashPhase('shutdown');
    await shutdownTelemetry({ timeoutMs: CLI_SHUTDOWN_TIMEOUT_MS });
    await harness.close();
    throw error;
  }
}
