# Part 2: TS 宿主包与平台子包

本 Part 创建 `packages/ody-crypto` 宿主包（类型、加载器、TS fallback）以及 5 个平台子包，并把新包注册进 workspace / Nix fileset。

## 依赖关系

```text
Task 6 -> Task 8
Task 5 -> Task 7 -> Task 8
```

---

### Task 6: 创建 TS 宿主包 `packages/ody-crypto`

**Depends on:** Task 1（workspace 已存在，可独立并行开始）

**Files：**
- Create: `packages/ody-crypto/package.json`
- Create: `packages/ody-crypto/tsconfig.json`
- Create: `packages/ody-crypto/vitest.config.ts`
- Create: `packages/ody-crypto/src/types.ts`
- Create: `packages/ody-crypto/src/fallback.ts`
- Create: `packages/ody-crypto/src/loader.ts`
- Create: `packages/ody-crypto/src/index.ts`
- Create: `packages/ody-crypto/test/fallback.test.ts`
- Create: `packages/ody-crypto/test/loader.test.ts`

**步骤：**

- [ ] 创建 `packages/ody-crypto/package.json`：

```json
{
  "name": "@odysseythink/ody-crypto",
  "version": "0.1.0",
  "private": true,
  "description": "Native crypto module loader with TS fallback",
  "license": "MIT",
  "type": "module",
  "main": "dist/index.cjs",
  "module": "dist/index.mjs",
  "types": "dist/index.d.ts",
  "exports": {
    ".": {
      "types": "./dist/index.d.ts",
      "import": "./dist/index.mjs",
      "require": "./dist/index.cjs"
    }
  },
  "scripts": {
    "build": "tsdown",
    "build:native": "./scripts/build-native.sh",
    "test": "vitest run",
    "typecheck": "tsc -p tsconfig.json --noEmit"
  },
  "dependencies": {
    "jsonwebtoken": "^9.0.2"
  },
  "devDependencies": {
    "@types/jsonwebtoken": "^9.0.7",
    "vitest": "^4.1.4"
  },
  "optionalDependencies": {
    "@odysseythink/ody-crypto-darwin-arm64": "workspace:^",
    "@odysseythink/ody-crypto-darwin-x64": "workspace:^",
    "@odysseythink/ody-crypto-linux-arm64": "workspace:^",
    "@odysseythink/ody-crypto-linux-x64": "workspace:^",
    "@odysseythink/ody-crypto-win32-x64": "workspace:^"
  }
}
```

- [ ] 创建 `packages/ody-crypto/tsconfig.json`：

```json
{
  "extends": "../../tsconfig.json",
  "compilerOptions": {},
  "include": ["src", "test"]
}
```

- [ ] 创建 `packages/ody-crypto/vitest.config.ts`：

```ts
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    name: 'ody-crypto',
    include: ['test/**/*.test.ts'],
  },
});
```

- [ ] 创建 `packages/ody-crypto/src/types.ts`：

```ts
export interface PkceChallenge {
  readonly codeVerifier: string;
  readonly codeChallenge: string;
}

export interface IdTokenExpected {
  readonly issuer: string;
  readonly audience: string;
  readonly maxAgeSeconds?: number;
}

export interface IdTokenClaims {
  readonly sub: string;
  readonly iss: string;
  readonly aud: string | string[];
  readonly exp: number;
  readonly iat: number;
  readonly [claim: string]: unknown;
}

export interface OdyCrypto {
  randomBytes(length: number): Buffer;
  sha256(input: string | Buffer): string;
  pkceChallenge(length?: number): PkceChallenge;
  verifyIdToken(jwt: string, jwkJson: string, expected: IdTokenExpected): IdTokenClaims;
}
```

- [ ] 创建 `packages/ody-crypto/src/fallback.ts`：

```ts
import { createHash, createPublicKey, randomBytes as nodeRandomBytes } from 'node:crypto';
import jwt from 'jsonwebtoken';

import type { IdTokenClaims, IdTokenExpected, OdyCrypto, PkceChallenge } from './types';

const PKCE_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~';

function randomBytes(length: number): Buffer {
  return nodeRandomBytes(length);
}

function sha256(input: string | Buffer): string {
  return createHash('sha256').update(input).digest('hex');
}

function pkceChallenge(length = 43): PkceChallenge {
  if (length < 43 || length > 128) {
    throw new RangeError(`PKCE verifier length ${length} out of range [43, 128]`);
  }
  let verifier = '';
  const bytes = randomBytes(length);
  for (let i = 0; i < length; i++) {
    verifier += PKCE_ALPHABET[bytes[i]! % PKCE_ALPHABET.length];
  }
  const challenge = createHash('sha256').update(verifier).digest('base64url');
  return { codeVerifier: verifier, codeChallenge: challenge };
}

function verifyIdToken(jwtString: string, jwkJson: string, expected: IdTokenExpected): IdTokenClaims {
  const jwk = JSON.parse(jwkJson) as Record<string, unknown>;
  const key = createPublicKey({ key: jwk, format: 'jwk' });
  const payload = jwt.verify(jwtString, key, {
    algorithms: ['RS256', 'ES256'],
    issuer: expected.issuer,
    audience: expected.audience,
    maxAge: expected.maxAgeSeconds,
  }) as Record<string, unknown>;
  return payload as IdTokenClaims;
}

export const tsFallback: OdyCrypto = { randomBytes, sha256, pkceChallenge, verifyIdToken };
```

- [ ] 创建 `packages/ody-crypto/src/loader.ts`：

```ts
import { createRequire } from 'node:module';

import type { OdyCrypto } from './types';
import { tsFallback } from './fallback';

const SUPPORTED_TARGETS = [
  'darwin-arm64',
  'darwin-x64',
  'linux-arm64',
  'linux-x64',
  'win32-arm64',
  'win32-x64',
];

function currentTarget(): string {
  return `${process.platform}-${process.arch}`;
}

function debug(message: string, ...args: unknown[]): void {
  console.debug(`ody-crypto: ${message}`, ...args);
}

export function loadNative(): OdyCrypto | null {
  const target = currentTarget();
  if (!SUPPORTED_TARGETS.includes(target)) {
    debug('unsupported target, using TS fallback', target);
    return null;
  }
  const pkg = `@odysseythink/ody-crypto-${target}`;
  try {
    const req = createRequire(import.meta.url);
    return req(pkg) as OdyCrypto;
  } catch (err) {
    debug('native load failed, using TS fallback', target, (err as Error).message);
    return null;
  }
}

export function getOdyCrypto(): OdyCrypto {
  return loadNative() ?? tsFallback;
}
```

- [ ] 创建 `packages/ody-crypto/src/index.ts`：

```ts
export * from './types';
export { getOdyCrypto, loadNative } from './loader';
export { tsFallback } from './fallback';
```

- [ ] 先写测试 `packages/ody-crypto/test/fallback.test.ts`：

```ts
import { describe, expect, it } from 'vitest';
import { tsFallback } from '../src/fallback';

describe('tsFallback', () => {
  it('randomBytes returns requested length', () => {
    expect(tsFallback.randomBytes(16).length).toBe(16);
  });

  it('sha256 returns known vector', () => {
    expect(tsFallback.sha256('abc')).toBe(
      'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
    );
  });

  it('pkceChallenge default length is 43 and challenge is base64url', () => {
    const result = tsFallback.pkceChallenge();
    expect(result.codeVerifier.length).toBe(43);
    expect(result.codeChallenge).toMatch(/^[A-Za-z0-9_-]+$/);
  });

  it('pkceChallenge rejects 42 and 129', () => {
    expect(() => tsFallback.pkceChallenge(42)).toThrow(RangeError);
    expect(() => tsFallback.pkceChallenge(129)).toThrow(RangeError);
  });
});
```

- [ ] 再写测试 `packages/ody-crypto/test/loader.test.ts`：

```ts
import { describe, expect, it, vi } from 'vitest';
import * as nodeModule from 'node:module';
import { getOdyCrypto, loadNative } from '../src/loader';
import { tsFallback } from '../src/fallback';

describe('loadNative', () => {
  it('returns native when .node loads', () => {
    const mock = {
      randomBytes: vi.fn(),
      sha256: vi.fn(),
      pkceChallenge: vi.fn(),
      verifyIdToken: vi.fn(),
    };
    vi.spyOn(nodeModule, 'createRequire').mockReturnValue(() => mock as never);
    const result = loadNative();
    expect(result?.randomBytes).toBe(mock.randomBytes);
    expect(result?.sha256).toBe(mock.sha256);
  });

  it('falls back to ts on require failure', () => {
    vi.spyOn(nodeModule, 'createRequire').mockReturnValue(() => {
      throw new Error('dlopen');
    } as never);
    const result = getOdyCrypto();
    const challenge = result.pkceChallenge();
    expect(challenge.codeVerifier.length).toBe(43);
    expect(result).toBe(tsFallback);
  });
});
```

- [ ] 运行测试并确认失败：

```bash
pnpm install
pnpm --filter @odysseythink/ody-crypto test
```

预期：`vitest` 启动但测试文件引用的 `../src/fallback` 等模块尚未实现，出现 import/函数未定义错误。

- [ ] 补齐实现（上面已给出），再次运行：

```bash
pnpm --filter @odysseythink/ody-crypto test
```

预期：4 个 fallback 测试 + 2 个 loader 测试全部通过。

- [ ] Commit：`git add packages/ody-crypto/ && git commit -m "feat(ody-crypto): TS loader and fallback"`。

---

### Task 7: 创建 5 个平台子包

**Depends on:** Task 5（已有当前平台 `.node` 产物；CI 中由对应 runner 生成）

**Files：**
- Create: `packages/ody-crypto-darwin-arm64/package.json`
- Create: `packages/ody-crypto-darwin-x64/package.json`
- Create: `packages/ody-crypto-linux-arm64/package.json`
- Create: `packages/ody-crypto-linux-x64/package.json`
- Create: `packages/ody-crypto-win32-x64/package.json`
- Create/Modify: `packages/ody-crypto-<target>/ody-crypto.node`（由 `build-crypto.sh` 或 CI 复制）

**步骤：**

- [ ] 创建 5 个子包目录和 `package.json`。以 `packages/ody-crypto-darwin-arm64/package.json` 为例：

```json
{
  "name": "@odysseythink/ody-crypto-darwin-arm64",
  "version": "0.1.0",
  "private": true,
  "description": "Native ody-crypto binary for darwin-arm64",
  "license": "MIT",
  "main": "ody-crypto.node",
  "files": ["ody-crypto.node"],
  "os": ["darwin"],
  "cpu": ["arm64"]
}
```

其余 4 个只需把 `name`、`description`、`os`、`cpu` 替换为对应平台：

| 目录 | `os` | `cpu` |
|---|---|---|
| `ody-crypto-darwin-x64` | `darwin` | `x64` |
| `ody-crypto-linux-arm64` | `linux` | `arm64` |
| `ody-crypto-linux-x64` | `linux` | `x64` |
| `ody-crypto-win32-x64` | `win32` | `x64` |

- [ ] 本地开发时，用 Task 5 的 `rust-ody/build-crypto.sh` 把当前平台 `.node` 放入对应子包。CI 中各 runner 会自行编译并复制。
- [ ] 手动验证目录结构：

```bash
for t in darwin-arm64 darwin-x64 linux-arm64 linux-x64 win32-x64; do
  test -f "packages/ody-crypto-$t/package.json" && echo "OK $t/package.json"
done
ls -lh "packages/ody-crypto-$(node -e 'console.log(process.platform+"-"+process.arch)')/ody-crypto.node"
```

预期：5 个 `package.json` 均存在；当前平台子包内有一个非空 `.node` 文件。

- [ ] Commit：`git add packages/ody-crypto-*/package.json packages/ody-crypto-*/ody-crypto.node && git commit -m "chore(ody-crypto): platform subpackages"`。

---

### Task 8: 注册 workspace 并同步 flake.nix

**Depends on:** Task 6, Task 7

**Files：**
- Modify: `pnpm-workspace.yaml`
- Modify: `flake.nix`

**步骤：**

- [ ] `pnpm-workspace.yaml` 的 `packages:` 列表已经包含 `packages/*`，因此 `packages/ody-crypto` 与 5 个子包会自动加入 workspace。本任务只需确认无需额外添加顶层目录。
- [ ] 修改 `flake.nix`，在 `workspacePaths` 中追加 6 条：

```nix
      workspacePaths = [
        # ... 原有条目 ...
        ./packages/ody-crypto
        ./packages/ody-crypto-darwin-arm64
        ./packages/ody-crypto-darwin-x64
        ./packages/ody-crypto-linux-arm64
        ./packages/ody-crypto-linux-x64
        ./packages/ody-crypto-win32-x64
      ];
```

- [ ] 在 `workspaceNames` 中追加 6 条：

```nix
      workspaceNames = [
        # ... 原有条目 ...
        "@odysseythink/ody-crypto"
        "@odysseythink/ody-crypto-darwin-arm64"
        "@odysseythink/ody-crypto-darwin-x64"
        "@odysseythink/ody-crypto-linux-arm64"
        "@odysseythink/ody-crypto-linux-x64"
        "@odysseythink/ody-crypto-win32-x64"
      ];
```

- [ ] 同步 pnpm 锁文件并安装：

```bash
pnpm install
```

- [ ] 运行宿主包构建与类型检查：

```bash
pnpm --filter @odysseythink/ody-crypto run build
pnpm --filter @odysseythink/ody-crypto run typecheck
```

- [ ] 运行全 workspace 类型检查（含 test 文件），验证 flake 中新加的包不会破坏类型：

```bash
pnpm run typecheck
```

- [ ] 预期：`ody-crypto` 构建成功；全 workspace `typecheck` 通过。
- [ ] Commit：`git add pnpm-workspace.yaml flake.nix pnpm-lock.yaml && git commit -m "chore(workspace): register ody-crypto packages"`。

---

## Local Self-Review

- [ ] 1. Spec-coverage table：本 Part 覆盖 TS 宿主包、平台子包、workspace 注册、Nix fileset 同步。
- [ ] 2. Placeholder scan：所有 package.json、源代码、测试代码均已给出，无 `TODO`/`TBD`。
- [ ] 3. No phantom tasks：Task 7 创建真实目录与 package.json；Task 8 修改 flake.nix 并跑全量 typecheck。
- [ ] 4. Dependency soundness：Task 6 不依赖 Rust 实现；Task 7 依赖 Task 5 的 `.node`；Task 8 依赖 Task 6/7 的 package 已存在。
- [ ] 5. Caller & build soundness：Task 8 修改 `flake.nix` 的 `workspacePaths`/`workspaceNames`，这两个列表是 Nix build 解析 workspace 的唯一来源；新增路径/名称必须与 `pnpm-workspace.yaml` 中的目录和 `package.json` 中的 `name` 完全一致，否则 `pnpmConfigHook` 会失败。Task 8 以全 workspace `pnpm run typecheck` 收尾。
- [ ] 6. Test-the-risk：fallback 的 PKCE 边界、sha256 向量、loader 的 native/fallback 分支均有断言；loader 测试枚举了 unsupported target（本任务未显式测试，但 `SUPPORTED_TARGETS` 列表与 `apps/ody-code/scripts/native/native-deps.mjs` 的列表在 Task 12 中通过 native-deps 测试间接校验）。
- [ ] 7. Type consistency：`OdyCrypto` 接口的字段名/类型与 Part 1 Rust napi 导出对象一致；`IdTokenClaims` 的 `aud: string | string[]` 对应 Rust `Either<String, Vec<String>>`。
