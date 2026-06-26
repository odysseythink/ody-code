import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  runNativeAssetSmokeIfRequested,
  SMOKE_PACKAGES,
} from '#native/smoke';
import {
  NATIVE_ASSET_MANIFEST_VERSION,
  type NativeAssetManifest,
  type NativeAssetSource,
} from '#native/native-assets';

function fakeManifest(missingPackage?: string): {
  manifest: NativeAssetManifest;
  source: NativeAssetSource;
} {
  const packages = SMOKE_PACKAGES.filter((name) => name !== missingPackage).map(
    (name) => ({
      name,
      root: `node_modules/${name}`,
      files: [
        {
          assetKey: `native/test-target/node_modules/${name}/package.json`,
          relativePath: `node_modules/${name}/package.json`,
          sha256:
            '44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a',
        },
      ],
    }),
  );
  const manifest: NativeAssetManifest = {
    version: NATIVE_ASSET_MANIFEST_VERSION,
    target: 'test-target',
    packages,
  };
  const assets = new Map<string, Buffer>([
    ['native/test-target/manifest.json', Buffer.from(JSON.stringify(manifest))],
    ...packages.map((pkg) => [
      pkg.files[0]!.assetKey,
      Buffer.from('{}'),
    ] as const),
  ]);
  return {
    manifest,
    source: {
      getAssetKeys: () => [...assets.keys()],
      getRawAsset: (key) => {
        const value = assets.get(key);
        if (value === undefined) throw new Error(`missing asset: ${key}`);
        return value;
      },
    },
  };
}

describe('runNativeAssetSmokeIfRequested', () => {
  beforeEach(() => {
    vi.stubEnv('ODY_CODE_NATIVE_ASSET_SMOKE', '1');
  });

  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it('returns false when ODY_CODE_NATIVE_ASSET_SMOKE is not set', () => {
    vi.unstubAllEnvs();
    expect(runNativeAssetSmokeIfRequested()).toBe(false);
  });

  it('passes when all smoke packages are present', () => {
    const exitSpy = vi
      .spyOn(process, 'exit')
      .mockImplementation(() => {
        throw new Error('process.exit called');
      });
    const stdoutSpy = vi
      .spyOn(process.stdout, 'write')
      .mockImplementation(() => true);
    const { source, manifest } = fakeManifest();

    try {
      runNativeAssetSmokeIfRequested({ source, manifest });
    } catch {
      // process.exit mock throws to stop control flow
    }

    expect(stdoutSpy).toHaveBeenCalledWith(
      'Native asset smoke passed: test-target\n',
    );
    expect(exitSpy).toHaveBeenCalledWith(0);
    exitSpy.mockRestore();
    stdoutSpy.mockRestore();
  });

  it('fails when ody-crypto is missing from the manifest', () => {
    const exitSpy = vi
      .spyOn(process, 'exit')
      .mockImplementation(() => {
        throw new Error('process.exit called');
      });
    const stderrSpy = vi
      .spyOn(process.stderr, 'write')
      .mockImplementation(() => true);
    const { source, manifest } = fakeManifest('@odysseythink/ody-crypto');

    try {
      runNativeAssetSmokeIfRequested({ source, manifest });
    } catch {
      // process.exit mock throws to stop control flow
    }

    expect(stderrSpy).toHaveBeenCalledWith(
      expect.stringContaining(
        'Native package is not available: @odysseythink/ody-crypto',
      ),
    );
    expect(exitSpy).toHaveBeenCalledWith(1);
    exitSpy.mockRestore();
    stderrSpy.mockRestore();
  });

  it('fails when ody-host is missing from the manifest', () => {
    const exitSpy = vi
      .spyOn(process, 'exit')
      .mockImplementation(() => {
        throw new Error('process.exit called');
      });
    const stderrSpy = vi
      .spyOn(process.stderr, 'write')
      .mockImplementation(() => true);
    const { source, manifest } = fakeManifest('ody-host');

    try {
      runNativeAssetSmokeIfRequested({ source, manifest });
    } catch {
      // process.exit mock throws to stop control flow
    }

    expect(stderrSpy).toHaveBeenCalledWith(
      expect.stringContaining('Native package is not available: ody-host'),
    );
    expect(exitSpy).toHaveBeenCalledWith(1);
    exitSpy.mockRestore();
    stderrSpy.mockRestore();
  });
});
