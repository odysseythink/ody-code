import {
  getEmbeddedNativeAssetManifest,
  getNativePackageRoot,
  type NativeAssetOptions,
} from './native-assets';

export const SMOKE_PACKAGES = [
  '@mariozechner/clipboard',
  'koffi',
  '@odysseythink/ody-crypto',
];

export function runNativeAssetSmokeIfRequested(
  options?: NativeAssetOptions,
): boolean {
  if (process.env['ODY_CODE_NATIVE_ASSET_SMOKE'] !== '1') {
    return false;
  }

  try {
    const manifest =
      options?.manifest ??
      getEmbeddedNativeAssetManifest(options?.source);
    if (manifest === null) {
      throw new Error('Native asset manifest is not available.');
    }
    for (const packageName of SMOKE_PACKAGES) {
      const packageRoot = getNativePackageRoot(packageName, {
        manifest,
        ...options,
      });
      if (packageRoot === null) {
        throw new Error(`Native package is not available: ${packageName}`);
      }
    }
    process.stdout.write(`Native asset smoke passed: ${manifest.target}\n`);
    process.exit(0);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    process.stderr.write(`Native asset smoke failed: ${message}\n`);
    process.exit(1);
  }
}
