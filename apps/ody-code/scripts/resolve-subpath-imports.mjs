import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

const SUBPATH_PATTERN = '#/*';

function findPackageRoot(importer) {
  let dir = dirname(importer);
  while (true) {
    const pkgPath = join(dir, 'package.json');
    if (existsSync(pkgPath)) {
      const pkg = JSON.parse(readFileSync(pkgPath, 'utf-8'));
      if (pkg.imports?.[SUBPATH_PATTERN] !== undefined) {
        return dir;
      }
      // A package.json without the expected subpath import mapping is treated
      // as a boundary: do not keep traversing into parent directories.
      return null;
    }
    const parent = dirname(dir);
    if (parent === dir) return null;
    dir = parent;
  }
}

function tryResolve(source, packageRoot) {
  const subpath = source.slice(SUBPATH_PATTERN.length - 1); // keep leading '/'
  const candidates = [
    join(packageRoot, 'src', `${subpath}.ts`),
    join(packageRoot, 'src', subpath, 'index.ts'),
  ];
  for (const candidate of candidates) {
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

/**
 * Rollup/Rolldown plugin that resolves Node.js subpath imports (`#/...`)
 * relative to the importing package's own `src` directory.
 *
 * This is needed for the native SEA bundle, where every workspace package
 * declares a `"#/*"` imports mapping to TypeScript files under `src/`.
 * The regular ESM build relies on Node's resolver at runtime, but the
 * single-file CJS bundle must resolve these aliases during bundling.
 */
export function resolveSubpathImportsPlugin() {
  return {
    name: 'resolve-subpath-imports',
    async resolveId(source, importer) {
      if (!source.startsWith('#/') || importer === undefined) return null;
      const packageRoot = findPackageRoot(importer);
      if (packageRoot === null) return null;
      const resolved = tryResolve(source, packageRoot);
      return resolved ?? null;
    },
  };
}
