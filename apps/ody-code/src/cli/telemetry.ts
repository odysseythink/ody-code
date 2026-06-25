import { createKimiDeviceId, ODY_CODE_PROVIDER_NAME } from '@odysseythink/kimi-code-oauth';
import { initializeTelemetry } from '@odysseythink/ody-telemetry';
import { resolveOdyHome, type OdyConfig } from '@odysseythink/ody-code-sdk';

import { CLI_USER_AGENT_PRODUCT } from '#/constant/app';
import type { OdyHarness } from '#/tui/types';

export interface CliTelemetryBootstrap {
  readonly homeDir: string;
  readonly deviceId: string;
  readonly firstLaunch: boolean;
}

export interface InitializeCliTelemetryOptions {
  readonly harness: OdyHarness;
  readonly bootstrap: CliTelemetryBootstrap;
  readonly config: Pick<OdyConfig, 'defaultModel' | 'telemetry'>;
  readonly version: string;
  readonly uiMode: string;
  readonly model?: string;
}

export function createCliTelemetryBootstrap(): CliTelemetryBootstrap {
  let firstLaunch = false;
  const homeDir = resolveOdyHome();
  const deviceId = createKimiDeviceId(homeDir, {
    onFirstLaunch: () => {
      firstLaunch = true;
    },
  });
  return { homeDir, deviceId, firstLaunch };
}

export function initializeCliTelemetry(options: InitializeCliTelemetryOptions): void {
  initializeTelemetry({
    homeDir: options.harness.homeDir,
    deviceId: options.bootstrap.deviceId,
    enabled: options.config.telemetry !== false,
    appName: CLI_USER_AGENT_PRODUCT,
    version: options.version,
    uiMode: options.uiMode,
    model: options.model ?? options.config.defaultModel,
    getAccessToken: async () =>
      (await options.harness.auth.getCachedAccessToken(ODY_CODE_PROVIDER_NAME)) ?? null,
  });
  if (options.bootstrap.firstLaunch) {
    options.harness.track('first_launch');
  }
}
