import { createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import { mkdir, readdir, readFile, stat, writeFile } from 'node:fs/promises';
import { dirname, relative, resolve } from 'node:path';

import {
  collectNativeAssets,
  nativeAssetManifestKey,
  nativeAssetSummary,
} from './assets.mjs';
import { fail, run } from './exec.mjs';
import {
  appRoot,
  nativeBlobPath,
  nativeIntermediatesDir,
  nativeJsBundlePath,
  nativeManifestDir,
  nativeSeaConfigPath,
  targetTriple,
} from './paths.mjs';

async function ensureBundleExists() {
  try {
    await stat(nativeJsBundlePath());
  } catch {
    fail(`Native JS bundle not found at ${nativeJsBundlePath()}. Run 01-bundle.mjs first.`);
  }
}

async function listFilesRecursive(dir) {
  const files = [];
  const entries = await readdir(dir, { withFileTypes: true });
  for (const entry of entries) {
    const path = resolve(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await listFilesRecursive(path)));
    } else {
      files.push(path);
    }
  }
  return files;
}

async function collectBuiltInAssets() {
  const builtInDir = resolve(appRoot, 'built-in');
  if (!existsSync(builtInDir)) {
    return null;
  }

  const assets = {};
  const servers = [];
  const entries = await readdir(builtInDir, { withFileTypes: true });

  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const serverName = entry.name;
    const serverDir = resolve(builtInDir, serverName);
    const files = await listFilesRecursive(serverDir);
    const manifestFiles = [];

    for (const file of files) {
      const content = await readFile(file);
      const hash = createHash('sha256').update(content).digest('hex');
      const relPath = relative(serverDir, file);
      const assetKey = `built-in/${serverName}/${relPath}`;
      assets[assetKey] = file;
      manifestFiles.push({ path: relPath, sha256: hash });
    }

    if (manifestFiles.length > 0) {
      servers.push({ name: serverName, files: manifestFiles });
    }
  }

  if (servers.length === 0) {
    return null;
  }

  const manifest = { version: 1, servers };
  const manifestPath = resolve(nativeIntermediatesDir(), 'built-in-manifest.json');
  await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  assets['built-in/manifest.json'] = manifestPath;

  return { assets, servers };
}

async function writeSeaConfig(target) {
  await mkdir(nativeIntermediatesDir(), { recursive: true });
  const { manifest, manifestJson, assets } = await collectNativeAssets({
    appRoot,
    target,
  });
  const manifestPath = resolve(nativeManifestDir(target), 'manifest.json');
  await mkdir(dirname(manifestPath), { recursive: true });
  await writeFile(manifestPath, manifestJson);

  const seaAssets = {
    [nativeAssetManifestKey(target)]: manifestPath,
    ...assets,
  };

  const builtIn = await collectBuiltInAssets();
  if (builtIn !== null) {
    Object.assign(seaAssets, builtIn.assets);
    console.log(`Collected built-in assets:`);
    for (const server of builtIn.servers) {
      console.log(`- ${server.name}: ${server.files.length} files`);
    }
  }

  const config = {
    main: nativeJsBundlePath(),
    output: nativeBlobPath(),
    assets: Object.fromEntries(
      Object.entries(seaAssets).sort(([a], [b]) => a.localeCompare(b)),
    ),
    disableExperimentalSEAWarning: true,
    useCodeCache: false,
    useSnapshot: false,
  };
  await writeFile(nativeSeaConfigPath(), `${JSON.stringify(config, null, 2)}\n`);

  console.log(`Collected native assets for ${manifest.target}:`);
  for (const line of nativeAssetSummary(manifest)) {
    console.log(`- ${line}`);
  }
}

export async function runSeaBlobStep() {
  await ensureBundleExists();
  const target = targetTriple();
  await writeSeaConfig(target);
  await run(process.execPath, ['--experimental-sea-config', nativeSeaConfigPath()]);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  await runSeaBlobStep();
}
