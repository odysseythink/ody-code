import { access } from 'node:fs/promises';

import {
  setCrashPhase,
  setTelemetryContext,
  shutdownTelemetry,
  track,
  withTelemetryContext,
} from '@odysseythink/ody-telemetry';
import type { TelemetryClient } from '@odysseythink/ody-code-sdk';

import { CLI_SHUTDOWN_TIMEOUT_MS, CLI_UI_MODE } from '#/constant/app';
import { RustHostConnector, RustHostHarness, type RustHostConnectorOptions } from '#/host/index';
import type { TuiConfig } from '#/tui/config';
import { loadTuiConfig, TuiConfigParseError } from '#/tui/config';
import { CHROME_GUTTER } from '#/tui/constant/rendering';
import { OdyTUI } from '#/tui/index';
import { detectTerminalTheme } from '#/tui/theme/detect';

import type { CLIOptions } from './options';
import { createCliTelemetryBootstrap, initializeCliTelemetry } from './telemetry';

export interface RustHostLaunchOptions {
  readonly mode: 'stdio' | 'socket' | 'tcp';
  readonly binaryPath: string;
  readonly socketPath?: string;
  readonly host?: string;
  readonly port?: number;
  readonly configPath?: string;
  readonly homeDir?: string;
}

export async function resolveHostBinary(opts: Pick<CLIOptions, 'hostBinary'>): Promise<string> {
  if (opts.hostBinary !== undefined) {
    return opts.hostBinary;
  }
  return 'ody-host';
}

export function buildRustHostLaunchOptions(
  opts: Pick<CLIOptions, 'hostStdio' | 'hostSocket' | 'hostTcp' | 'hostBinary'> & {
    configPath?: string;
    homeDir?: string;
  },
): RustHostLaunchOptions {
  const binaryPath = opts.hostBinary ?? 'ody-host';
  if (opts.hostSocket !== undefined) {
    return { mode: 'socket', binaryPath, socketPath: opts.hostSocket, configPath: opts.configPath, homeDir: opts.homeDir };
  }
  if (opts.hostTcp !== undefined) {
    const [host, portStr] = opts.hostTcp.split(':');
    return { mode: 'tcp', binaryPath, host, port: Number(portStr), configPath: opts.configPath, homeDir: opts.homeDir };
  }
  return { mode: 'stdio', binaryPath, configPath: opts.configPath, homeDir: opts.homeDir };
}

export async function runShellWithRustHost(opts: CLIOptions, version: string): Promise<void> {
  const startedAt = Date.now();
  const configStartedAt = startedAt;
  let tuiConfig: TuiConfig;
  let configWarning: string | undefined;
  try {
    tuiConfig = await loadTuiConfig();
  } catch (error) {
    if (!(error instanceof TuiConfigParseError)) throw error;
    tuiConfig = error.fallback;
    configWarning = error.message;
  }

  const resolvedTheme = tuiConfig.theme === 'auto' ? await detectTerminalTheme() : tuiConfig.theme;
  const telemetryBootstrap = createCliTelemetryBootstrap();
  const telemetryClient: TelemetryClient = {
    track,
    withContext: withTelemetryContext,
    setContext: setTelemetryContext,
  };

  const binaryPath = await resolveHostBinary(opts);
  try {
    await access(binaryPath);
  } catch {
    if (opts.hostBinary !== undefined) {
      throw new Error(`Rust host binary not found: ${binaryPath}`);
    }
    // If no explicit path, assume it is on PATH and let spawn fail loudly if not.
  }

  const connector = new RustHostConnector();
  const launchOptions = buildRustHostLaunchOptions(opts);
  const client = await connector.connect(launchOptions as RustHostConnectorOptions);
  const harness = new RustHostHarness({ client, telemetry: telemetryClient });

  await harness.ensureConfigFile();
  const config = await harness.getConfig();
  const configMs = Date.now() - configStartedAt;

  const tui = new OdyTUI(harness, {
    cliOptions: opts,
    tuiConfig,
    version,
    workDir: process.cwd(),
    startupNotice: configWarning,
    resolvedTheme,
    officeHours: false,
    gameDesign: false,
  });

  initializeCliTelemetry({
    harness: harness as any,
    bootstrap: telemetryBootstrap,
    config,
    version,
    uiMode: CLI_UI_MODE,
  });
  setCrashPhase('runtime');

  const trackLifecycle = (event: string, properties?: Record<string, unknown>) => {
    withTelemetryContext({ sessionId: tui.getCurrentSessionId() } as any).track(event, properties as any);
  };

  connector.onDisconnect((error) => {
    console.error(`Rust host disconnected: ${error.message}`);
    void tui.stop(1);
  });

  tui.onExit = async (exitCode = 0) => {
    setCrashPhase('shutdown');
    trackLifecycle('exit', { duration_s: (Date.now() - startedAt) / 1000 });
    await shutdownTelemetry({ timeoutMs: CLI_SHUTDOWN_TIMEOUT_MS });
    const gutter = ' '.repeat(CHROME_GUTTER);
    process.stdout.write(`${gutter}Bye!\n`);
    process.exit(exitCode);
  };

  try {
    const initStartedAt = Date.now();
    await tui.start();
    const initMs = Date.now() - initStartedAt;
    trackLifecycle('started', { host: 'rust', mode: launchOptions.mode, yolo: opts.yolo, auto: opts.auto, sessionMode: opts.sessionMode });
    trackLifecycle('startup_perf', {
      duration_ms: Date.now() - startedAt,
      config_ms: configMs,
      init_ms: initMs,
    });
  } catch (error) {
    setCrashPhase('shutdown');
    trackLifecycle('exit', { duration_s: (Date.now() - startedAt) / 1000 });
    await harness.close();
    throw error;
  }
}
