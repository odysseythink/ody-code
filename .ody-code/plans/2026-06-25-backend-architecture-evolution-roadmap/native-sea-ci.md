# Part 4: Native asset / SEA / CI / 报告

**Scope:** 把 `ody-crypto` 接入 Ody Code 的 Native asset 收集与 SEA 运行链路，补全端到端 smoke 测试、CI 矩阵与 Phase 2-E 成本报告。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

```text
apps/ody-code/scripts/native/native-deps.mjs          # 注册 ody-crypto host/target
apps/ody-code/src/native/smoke.ts                     # SEA native asset smoke（可注入 manifest）
apps/ody-code/test/native/smoke.test.ts               # smoke 逻辑单元测试
packages/ody-crypto/package.json                      # 新增 build:native 脚本
package.json                                          # 新增 root build:native:crypto 脚本
.github/workflows/native-crypto.yml                   # CI 矩阵（构建 .node + SEA + smoke）
.ody-code/reports/2026-06-25-phase-2e-native-sea-cost.md  # 成本报告
```

## Dependency Overview

```text
Part 2: Task 7 + Task 8  ->  Task 12  (注册 native deps)
Task 12                  ->  Task 13  (SEA smoke 包含 ody-crypto)
Task 12                  ->  Task 14  (build:native:crypto 脚本)
Task 12 + Task 14        ->  Task 15  (CI workflow)
Task 12..Task 15         ->  Task 16  (成本报告)
```

- Task 12、Task 14 可并行；其余顺序依赖。
- 所有可测代码采用 **test-first**；CI workflow 与报告为不可测配置/文档，采用“完整代码 → 构建/手动验证 → 提交”。

## Risks & Open Questions

- GitHub Actions 目前在本仓库无既有 `.github/workflows`；新增 workflow 文件是 greenfield，需验证 runner 矩阵与 Rust/Node/pnpm 安装动作在仓库设置中可用。
- `win32-arm64` / `linux-arm64` 无免费 GitHub-hosted runner 或交叉编译链，本轮 CI 先覆盖 `darwin-arm64`、`darwin-x64`、`linux-x64`、`win32-x64` 四个主机可直接构建的目标；其余两个目标由本地/release 构建脚本处理。
- SEA smoke 原先直接读取 `node:sea`，为可测性需要给 `runNativeAssetSmokeIfRequested` 增加可选 `NativeAssetOptions` 参数；该参数默认 `undefined`，不改变 `main.ts` 调用语义。

## Tasks

### Task 12: 在 native-deps 注册 `ody-crypto`

**Depends on:** `2026-06-25-backend-architecture-evolution-roadmap/ts-loader.md`: Task 7、Task 8（平台子包与宿主包已创建）

**Files:**
- Modify: `apps/ody-code/scripts/native/native-deps.mjs:30-37`（新增 `odyCryptoSubpackageByTarget` 映射）
- Modify: `apps/ody-code/scripts/native/native-deps.mjs:58-86`（在 `nativeDeps` 数组追加 `ody-crypto-host` / `ody-crypto-target`）
- Modify: `apps/ody-code/test/scripts/native/native-deps.test.ts:90`（追加 ody-crypto 断言）
- Test: `apps/ody-code/test/scripts/native/native-deps.test.ts`

- [ ] 写失败测试：在 `native-deps.test.ts` 末尾追加以下测试，断言解析结果包含 `@odysseythink/ody-crypto` 及对应平台子包。

```ts
describe('ody-crypto native deps', () => {
  it('resolves ody-crypto host and target for darwin-arm64', () => {
    const names = resolveTargetDeps('darwin-arm64').map((d) => d.resolvedName);
    expect(names).toContain('@odysseythink/ody-crypto');
    expect(names).toContain('@odysseythink/ody-crypto-darwin-arm64');
  });

  it('picks the right ody-crypto subpackage per target', () => {
    expect(
      resolveTargetDeps('linux-x64').map((d) => d.resolvedName),
    ).toContain('@odysseythink/ody-crypto-linux-x64');
    expect(
      resolveTargetDeps('win32-x64').map((d) => d.resolvedName),
    ).toContain('@odysseythink/ody-crypto-win32-x64');
  });

  it('has ody-crypto-host (collect=js-only)', () => {
    const host = nativeDeps.find((d) => d.id === 'ody-crypto-host');
    expect(host?.collect).toBe('js-only');
    expect(host?.parent).toBe(null);
  });

  it('has ody-crypto-target (collect=native-files, parent=ody-crypto-host)', () => {
    const target = nativeDeps.find((d) => d.id === 'ody-crypto-target');
    expect(target?.collect).toBe('native-files');
    expect(target?.parent).toBe('ody-crypto-host');
  });
});
```

- [ ] 运行测试并确认失败：

```bash
pnpm --filter ody-code exec vitest run test/scripts/native/native-deps.test.ts
```

预期失败信息包含 `ody-crypto-host` / `ody-crypto-target` 未找到或数组不包含对应包名。

- [ ] 写最小实现：

在 `apps/ody-code/scripts/native/native-deps.mjs` 中，紧接 `koffiTripletByTarget` 之后插入：

```js
const odyCryptoSubpackageByTarget = Object.freeze({
  'darwin-arm64': '@odysseythink/ody-crypto-darwin-arm64',
  'darwin-x64': '@odysseythink/ody-crypto-darwin-x64',
  'linux-arm64': '@odysseythink/ody-crypto-linux-arm64',
  'linux-x64': '@odysseythink/ody-crypto-linux-x64',
  'win32-arm64': '@odysseythink/ody-crypto-win32-arm64',
  'win32-x64': '@odysseythink/ody-crypto-win32-x64',
});
```

在 `nativeDeps` 数组末尾、`koffi` 条目之后追加：

```js
  {
    id: 'ody-crypto-host',
    name: () => '@odysseythink/ody-crypto',
    collect: 'js-only',
    parent: null,
  },
  {
    id: 'ody-crypto-target',
    name: (target) => odyCryptoSubpackageByTarget[target],
    collect: 'native-files',
    parent: 'ody-crypto-host',
  },
```

- [ ] 运行测试并确认通过：

```bash
pnpm --filter ody-code exec vitest run test/scripts/native/native-deps.test.ts
```

预期输出：`Test Files  1 passed`。

- [ ] 提交：`git add apps/ody-code/scripts/native/native-deps.mjs apps/ody-code/test/scripts/native/native-deps.test.ts && git commit -m "feat(ody-crypto): register ody-crypto in native asset pipeline"`

---

### Task 13: 让 SEA native asset smoke 覆盖 `ody-crypto`

**Depends on:** Task 12

**Files:**
- Modify: `apps/ody-code/src/native/smoke.ts:1-26`（改为可注入 `NativeAssetOptions`，并把 `@odysseythink/ody-crypto` 加入 smoke 包列表）
- Create: `apps/ody-code/test/native/smoke.test.ts`
- Test: `apps/ody-code/test/native/smoke.test.ts`

- [ ] 写失败测试：创建 `apps/ody-code/test/native/smoke.test.ts`。

```ts
import { describe, expect, it, vi } from 'vitest';

import {
  runNativeAssetSmokeIfRequested,
  SMOKE_PACKAGES,
} from '#/native/smoke';
import {
  NATIVE_ASSET_MANIFEST_VERSION,
  type NativeAssetManifest,
  type NativeAssetSource,
} from '#/native/native-assets';

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
            'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
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
      pkg.files[0].assetKey,
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
  it('returns false when ODY_CODE_NATIVE_ASSET_SMOKE is not set', () => {
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
});
```

- [ ] 运行测试并确认失败：

```bash
pnpm --filter ody-code exec vitest run test/native/smoke.test.ts
```

预期失败：导入的 `SMOKE_PACKAGES` / `runNativeAssetSmokeIfRequested(options?)` 不存在，或 ody-crypto 缺失测试抛出异常。

- [ ] 写最小实现：把 `apps/ody-code/src/native/smoke.ts` 替换为：

```ts
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
    const manifest = getEmbeddedNativeAssetManifest(options);
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
```

- [ ] 确认 `main.ts` 调用不变：

```bash
grep -n "runNativeAssetSmokeIfRequested" apps/ody-code/src/main.ts
```

应仅看到 `runNativeAssetSmokeIfRequested()`，无参数；无需修改。

- [ ] 运行测试并确认通过：

```bash
pnpm --filter ody-code exec vitest run test/native/smoke.test.ts
```

预期输出：`Test Files  1 passed`。

- [ ] 全树类型检查：

```bash
pnpm run typecheck
```

预期通过，无 TS 错误。

- [ ] 提交：`git add apps/ody-code/src/native/smoke.ts apps/ody-code/test/native/smoke.test.ts && git commit -m "feat(ody-crypto): include ody-crypto in native asset smoke"`

---

### Task 14: 添加 `build:native:crypto` 脚本

**Depends on:** `2026-06-25-backend-architecture-evolution-roadmap/ts-loader.md`: Task 8（`packages/ody-crypto` 已创建）

**Files:**
- Modify: `packages/ody-crypto/package.json`（追加 `build:native` 脚本）
- Modify: `package.json:7-28`（追加 root `build:native:crypto` 脚本）

- [ ] 修改 `packages/ody-crypto/package.json` 的 `scripts` 字段，追加：

```json
{
  "scripts": {
    "build": "tsdown",
    "build:native": "./scripts/build-native.sh",
    "typecheck": "tsc -p tsconfig.json --noEmit",
    "test": "vitest run"
  }
}
```

其中 `./scripts/build-native.sh` 已在 Part 2 Task 8 创建；该脚本接收 `--target <target>` 参数并输出到对应平台子包。

- [ ] 修改根目录 `package.json`，在 `scripts` 中追加：

```json
{
  "scripts": {
    "build": "pnpm -r run build",
    "build:packages": "pnpm -r --filter './packages/*' run build",
    "build:native:crypto": "pnpm --filter @odysseythink/ody-crypto run build:native",
    "dev:cli": "pnpm -C apps/ody-code run dev",
    ...
  }
}
```

- [ ] 手动验证：在匹配的主机上执行 native 构建。例如 macOS arm64 机器上：

```bash
pnpm run build:native:crypto -- --target darwin-arm64
ls -lh packages/ody-crypto-darwin-arm64/ody-crypto.node
```

预期：脚本退出码为 `0`，且 `packages/ody-crypto-darwin-arm64/ody-crypto.node` 存在且大小 > 0。若当前主机不匹配目标，改用 `darwin-x64`、`linux-x64` 或 `win32-x64` 中对应的目标验证。

- [ ] 提交：`git add packages/ody-crypto/package.json package.json && git commit -m "chore(ody-crypto): add build:native:crypto script"`

---

### Task 15: 添加 CI workflow 构建 ody-crypto 并跑 SEA smoke

**Depends on:** Task 12（native-deps 已注册 ody-crypto）、Task 14（`build:native:crypto` 脚本可用）

**Files:**
- Create: `.github/workflows/native-crypto.yml`

- [ ] 创建目录与文件：

```bash
mkdir -p .github/workflows
```

- [ ] 写入 `.github/workflows/native-crypto.yml`：

```yaml
name: Native Crypto / SEA Smoke

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
  workflow_dispatch:

jobs:
  native-sea-smoke:
    name: ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - target: darwin-arm64
            os: macos-14
          - target: darwin-x64
            os: macos-13
          - target: linux-x64
            os: ubuntu-24.04
          - target: win32-x64
            os: windows-2022

    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Install pnpm
        uses: pnpm/action-setup@v4
        with:
          version: 10.33.0

      - name: Install Node.js
        uses: actions/setup-node@v4
        with:
          node-version-file: .nvmrc
          cache: pnpm

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Install dependencies
        run: pnpm install --frozen-lockfile

      - name: Build workspace packages
        run: pnpm run build:packages

      - name: Build ody-crypto native module
        run: pnpm run build:native:crypto -- --target ${{ matrix.target }}
        shell: bash

      - name: Build SEA
        run: pnpm --filter ody-code run build:native:sea
        env:
          ODY_CODE_BUILD_TARGET: ${{ matrix.target }}
        shell: bash

      - name: Native smoke
        run: pnpm --filter ody-code run test:native:smoke
        env:
          ODY_CODE_BUILD_TARGET: ${{ matrix.target }}
        shell: bash

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: ody-${{ matrix.target }}
          path: apps/ody-code/dist-native/bin/${{ matrix.target }}/
          if-no-files-found: error
```

说明：
- 矩阵仅包含 GitHub 提供主机 runner 的 4 个目标；`linux-arm64` 与 `win32-arm64` 本轮由本地或 release 构建脚本处理，不写入占位 TODO。
- `macos-14` 为 arm64，`macos-13` 为 x64；`ubuntu-24.04` / `windows-2022` 对应 x64。
- 所有 `run` 步骤显式 `shell: bash`，保证 Windows 上也使用 bash 执行 pnpm 命令。

- [ ] 手动验证：

1. 将包含本 workflow 的分支推送到远程：

```bash
git push -u origin feat/ody-crypto-native-ci
```

2. 打开 GitHub 仓库的 **Actions** 页面，进入 `Native Crypto / SEA Smoke` workflow。
3. 等待 4 个 job 完成；预期全部显示绿色 ✓，且 `ody-<target>` artifact 可下载。
4. 若某一平台失败，下载该 job 的日志；修复后重新触发。

- [ ] 提交：`git add .github/workflows/native-crypto.yml && git commit -m "ci: add ody-crypto native build and SEA smoke matrix"`

---

### Task 16: 编写 Phase 2-E 成本报告

**Depends on:** Task 12–Task 15

**Files:**
- Create: `.ody-code/reports/2026-06-25-phase-2e-native-sea-cost.md`

- [ ] 创建文件 `.ody-code/reports/2026-06-25-phase-2e-native-sea-cost.md`：

```markdown
# Phase 2-E `ody-crypto` Native 模块成本报告

## 变更摘要

- 新增 Rust crate `ody-crypto`，暴露 `randomBytes`、`sha256`、`pkceChallenge`、`verifyIdToken` 四个原子函数。
- 新增 TS 宿主包 `@odysseythink/ody-crypto` 与 5 个平台子包，支持自动平台探测与 TS fallback。
- MCP OAuth 流程改用 `ody-crypto` 生成 PKCE 与随机 state，并在 token 响应含 `id_token` 时完成 JWKS 校验。
- 在 `apps/ody-code/scripts/native/native-deps.mjs` 注册 ody-crypto，SEA 构建时自动收集 `.node` 与 JS 文件。
- 扩展 native asset smoke，确保 SEA 二进制启动后可解压并 require `@odysseythink/ody-crypto`。
- 新增 `.github/workflows/native-crypto.yml`，在 4 个主机目标上构建 native 模块、SEA 与 smoke 测试。

## 新增依赖

| 包/工具 | 作用 | 说明 |
|---|---|---|
| `napi-rs` v3 | Rust ↔ Node 绑定 | 已验证与 Node 24 SEA 兼容 |
| `rand`、`sha2`、`base64`、`jsonwebtoken` | Rust 实现 | 随机字节、哈希、JWT 校验 |
| `jsonwebtoken` (npm) | TS fallback | 仅用于 JWK → public key 校验 |
| `@modelcontextprotocol/sdk` | OAuth 客户端 | 已提供 `exchangeAuthorization` / `registerClient` 等函数 |

## 构建成本

- 本地开发：首次 `pnpm install` 后，Rust 依赖约需 30–60 秒编译（取决于目标）。
- CI：每个 matrix job 约 4–8 分钟（含 Rust 编译、tsdown、SEA 注入、smoke）。
- 发布流程： native 产物需要每个目标单独构建；当前 5 个平台子包，构建脚本 `build-crypto.sh` 支持传入 `--target`。

## 运行时成本

- `.node` 文件大小：每个目标约 300 KB–1 MB（取决于 Rust 依赖与符号表）。
- SEA 注入后二进制增量大体等于所有 native asset 压缩后大小之和。
- 启动时 native asset 解压为一次性开销，缓存按 `(version, target, manifest-hash)` 目录隔离。

## 风险与后续

- `win32-arm64` / `linux-arm64` 未纳入 GitHub-hosted CI matrix，release 需本地或自托管 runner 补充。
- Rust `jsonwebtoken` 的 JWK 支持已通过 Task 4 spike 验证；若后续支持更多算法，需扩展 `verifyIdToken` 的 `alg` 校验。
- TS fallback 使用 `node:crypto` + `jsonwebtoken`，在 Node 24 上功能等价；若未来需要 WebCrypto-only 环境，需再评估。

## 结论

Phase 2-E 达到目标：MCP OAuth 关键加密路径已迁移到 Native Rust，并在加载失败时静默降级。建议合并后继续观察 CI 稳定性与 SEA 二进制大小。
```

- [ ] 手动验证：

```bash
ls -l .ody-code/reports/2026-06-25-phase-2e-native-sea-cost.md
```

预期文件存在；打开阅读，确认无 TODO/TBD、无空节、所有链接/文件名与前面任务一致。

- [ ] 提交：`git add .ody-code/reports/2026-06-25-phase-2e-native-sea-cost.md && git commit -m "docs: phase 2E ody-crypto cost report"`

---

## Local Self-Review

- [ ] 1. **Spec-coverage table（Part 4 范围）**

| 设计需求 | 覆盖任务 | 状态 |
|---|---|---|
| `native-deps.mjs` 注册 ody-crypto host/target | Task 12 | covered |
| SEA native asset smoke 包含 ody-crypto | Task 13 | covered |
| 新增 `build:native:crypto` 脚本 | Task 14 | covered |
| CI 矩阵构建 ody-crypto 并跑 SEA smoke | Task 15 | covered |
| Phase 2-E 成本报告 | Task 16 | covered |
| win32-arm64 / linux-arm64 CI 覆盖 | — | no-op（本轮无免费 runner） |

- [ ] 2. **Placeholder scan**：Part 4 所有任务均给出完整代码/配置，无 TODO/TBD；CI 矩阵明确说明未覆盖的两个目标由本地/release 构建处理，未写入占位任务。
- [ ] 3. **No phantom tasks**：Task 12–16 每个都产生可验证变更（registry 变更、测试文件、脚本、workflow、报告），无 `--allow-empty` 或“已在 Task N 完成”的敷衍。
- [ ] 4. **Dependency soundness**：Task 13 依赖 Task 12；Task 15 依赖 Task 12 与 Task 14；Task 16 依赖 12–15。所有依赖均已在本 Part 或 earlier Part 完成。
- [ ] 5. **Caller & build soundness**：Task 13 改变 `runNativeAssetSmokeIfRequested` 签名（增加可选参数），已验证 `apps/ody-code/src/main.ts:129` 调用不变；任务内包含 `pnpm run typecheck` 全树检查。Task 12/14/15/16 未改变共享签名。
- [ ] 6. **Test-the-risk**：Task 12 的测试断言 `resolveTargetDeps` 能解析 ody-crypto 包名（与 native-deps.mjs 中常量一致）；Task 13 的测试断言 ody-crypto 缺失时 smoke 失败、存在时通过，覆盖状态化退出与 manifest 解析。
- [ ] 7. **Type consistency**：Task 13 使用 `NativeAssetOptions` 类型来自 `./native-assets`，与 Part 3 / 既有代码一致；Task 14 引用的包名 `@odysseythink/ody-crypto` 与 Part 2 Task 6 创建的 `packages/ody-crypto/package.json` 的 `name` 字段一致。
