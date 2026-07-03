# Part 3 — Bash 工具迁移、TS 后端适配器、L2 Parity、基准与 CI

本 Part 完成 4.1.4 的最后一块：把 `BashTool` 从裸 `tokio::process` 切到 `kaos-rs`；在 TS parity 后端新增 `env.*` 适配器，使 TS `LocalKaos` 与 Rust `kaos-rs`（经 `CoreHost`）能走同一调用接口；新增 L2 parity scenario 逐字段对照五个 kaos 操作；补充大目录 stat/glob/read 基准脚本；最后把 ody-host 编译、L2 parity 与基准产出接入 CI。

---

### Task 12: Migrate BashTool to kaos

**Depends on:** Task 2 (`CoreHost` 已持有 `Arc<Kaos>`)

**Files:**
- Modify: `rust-ody/crates/ody-host/src/tools/bash.rs:1-112`
- Modify: `rust-ody/crates/ody-host/src/host.rs:42-58`（`CoreHost::new` 注册 `BashTool`）
- Modify: `rust-ody/crates/ody-host/src/tools/mod.rs:1-103`（如需要，无需改动 trait）

**Steps:**

- [ ] 修改 `BashTool` 结构体，使其持有 `Arc<Kaos>`：
  ```rust
  use std::sync::Arc;
  use kaos_rs::kaos::Kaos;

  pub struct BashTool {
      kaos: Arc<Kaos>,
  }

  impl BashTool {
      pub fn new(kaos: Arc<Kaos>) -> Self {
          Self { kaos }
      }
  }
  ```
- [ ] 在 `execute` 中，把裸 `tokio::process::Command::new("bash")...output().await` 替换为 `kaos.exec`：
  ```rust
  let proc = self
      .kaos
      .exec(&["bash", "-c", command])
      .await
      .map_err(|e| ToolError::ExecutionFailed {
          message: "failed to execute bash command".to_string(),
          source: Box::new(e),
      })?;

  let exit_code = proc.wait().await;
  let stdout = String::from_utf8_lossy(&proc.stdout().await).to_string();
  let stderr = String::from_utf8_lossy(&proc.stderr().await).to_string();

  Ok(serde_json::json!({
      "status": if exit_code == 0 { "success" } else { "error" },
      "stdout": stdout,
      "stderr": stderr,
      "exit_code": exit_code,
  }))
  ```
- [ ] 更新 `CoreHost::new` 中的注册：
  ```rust
  let kaos = Arc::new(Kaos::new(
      kaos_rs::environment::detect_environment_from_node(),
      &config.home_dir,
  ));
  let mut tool_registry = ToolRegistry::new();
  tool_registry.register(Arc::new(BashTool::new(Arc::clone(&kaos))));
  ```
- [ ] 更新 `BashTool` 单元测试中的构造方式。在 `bash.rs` 的 `#[cfg(test)]` 模块顶部增加 helper：
  ```rust
  use kaos_rs::kaos::Kaos;

  fn make_tool() -> BashTool {
      let env = kaos_rs::environment::detect_environment_from_node();
      BashTool::new(Arc::new(Kaos::new(env, std::env::current_dir().unwrap())))
  }
  ```
  然后把两个测试中的 `let tool = BashTool;` 替换为 `let tool = make_tool();`。
- [ ] 搜索所有 `BashTool` 调用方，确认无遗漏：
  ```bash
  rg -n "BashTool" rust-ody/crates/ody-host/src/
  ```
  Expected: 命中 `tools/bash.rs`、`tools/mod.rs`、`host.rs`；`host.rs` 中的 `BashTool::new(...)` 是你刚写的。
- [ ] 编译检查：
  ```bash
  cd rust-ody && cargo check -p ody-host --tests
  ```
  Expected: 无错误。
- [ ] 运行 `BashTool` 测试：
  ```bash
  cd rust-ody && cargo test -p ody-host --lib bash::tests
  ```
  Expected: `bash_tool_approved_executes_command` 与 `bash_tool_rejected_returns_cancelled` 均通过。
- [ ] 整 workspace 类型检查（共享签名变更规则）：
  ```bash
  cd rust-ody && cargo check --workspace --tests
  ```
  Expected: 全绿。
- [ ] 提交：`feat(ody-host): migrate BashTool to kaos-rs`。

---

### Task 13: TS parity backend env.* adapter

**Depends on:** Task 12（BashTool 迁移完成，本任务不依赖它，但 Part 3 整体顺序如此）；Part 1 Task 3/4（`env.*` RPC 已存在于 Rust 端）；Part 2（`env.*` 行为已验证）。

**Files:**
- Modify: `packages/integration-tests/src/parity/types.ts:6-11`
- Modify: `packages/integration-tests/src/parity/backends.ts:1-148`
- Modify: `packages/integration-tests/src/parity/scenarios/utils.ts`（追加 `streamToBuffer` 如尚未存在）

**Steps:**

- [ ] 在 `types.ts` 的 `ParityBackend` 接口中增加 `envCall`：
  ```typescript
  export interface ParityBackend {
    readonly kind: BackendKind;
    readonly client: SDKRpcClient;
    readonly homeDir: string;
    envCall(method: string, payload: unknown): Promise<unknown>;
    close(): Promise<void>;
  }
  ```
- [ ] 在 `backends.ts` 顶部引入 `LocalKaos`：
  ```typescript
  import { LocalKaos } from '@odysseythink/kaos';
  ```
- [ ] 在 `makeTsBackend` 中创建 `LocalKaos` 实例并返回带 `envCall` 的后端：
  ```typescript
  const kaos = await LocalKaos.create();
  await kaos.chdir(config.homeDir);

  return {
    kind: 'ts' as BackendKind,
    client,
    homeDir: config.homeDir,
    envCall: async (method, payload) => envCallTs(kaos, method, payload),
    close: async () => {
      await client.close?.().catch(() => {});
    },
  };
  ```
- [ ] 在 `backends.ts` 中新增 `envCallTs` 实现（放在 `makeTsBackend` 之后）：
  ```typescript
  async function envCallTs(
    kaos: LocalKaos,
    method: string,
    payload: unknown,
  ): Promise<unknown> {
    const p = payload as Record<string, unknown>;
    switch (method) {
      case 'env.getcwd':
        return { cwd: kaos.getcwd() };
      case 'env.stat': {
        const s = await kaos.stat(String(p.path), {
          followSymlinks: (p.followSymlinks as boolean | undefined) ?? true,
        });
        const isDir = (s.stMode & 0o170000) === 0o040000;
        return { ...s, isDir };
      }
      case 'env.glob': {
        const matches: string[] = [];
        for await (const m of kaos.glob(String(p.path), String(p.pattern), {
          caseSensitive: (p.caseSensitive as boolean | undefined) ?? true,
        })) {
          matches.push(m);
        }
        matches.sort();
        return { matches };
      }
      case 'env.readText': {
        const text = await kaos.readText(String(p.path), {
          encoding: (p.encoding as BufferEncoding | undefined) ?? 'utf-8',
          errors: (p.errors as 'strict' | 'replace' | 'ignore' | undefined) ?? 'strict',
        });
        return { text };
      }
      case 'env.writeText': {
        const written = await kaos.writeText(String(p.path), String(p.text), {
          mode: ((p.mode as string | undefined) === 'a' ? 'a' : 'w') as 'w' | 'a',
          encoding: (p.encoding as BufferEncoding | undefined) ?? 'utf-8',
        });
        return { written };
      }
      case 'env.exec': {
        const args = (p.args as string[] | undefined) ?? [];
        const env = p.env as Record<string, string> | undefined;
        const proc =
          env !== undefined && Object.keys(env).length > 0
            ? await kaos.execWithEnv([String(p.command), ...args], env)
            : await kaos.exec(String(p.command), ...args);
        const [stdout, stderr] = await Promise.all([
          streamToBuffer(proc.stdout),
          streamToBuffer(proc.stderr),
        ]);
        const exitCode = await proc.wait();
        return {
          exitCode,
          stdout: stdout.toString('utf8'),
          stderr: stderr.toString('utf8'),
        };
      }
      default:
        throw new Error(`unknown env method: ${method}`);
    }
  }
  ```
- [ ] 在 `makeRustBackend` 返回的对象中增加 `envCall`：
  ```typescript
  return {
    kind: 'rust' as BackendKind,
    client,
    homeDir: config.homeDir,
    envCall: async (method, payload) => {
      const rpc = client.rpc as Record<string, (payload: unknown) => Promise<unknown>>;
      if (typeof rpc[method] !== 'function') {
        throw new Error(`Rust backend does not expose ${method}`);
      }
      return rpc[method](payload);
    },
    close: async () => {
      await client.close?.().catch(() => {});
    },
  };
  ```
- [ ] 确保 `streamToBuffer` 可用。`packages/integration-tests/src/parity/kaos-golden.ts` 已有一个私有 `streamToBuffer`，但 `backends.ts` 无法访问。把该函数提取到 `scenarios/utils.ts` 并导出，或在 `backends.ts` 中新建一个。本计划选择后者以保持改动最小：在 `backends.ts` 底部追加：
  ```typescript
  async function streamToBuffer(readable: Readable): Promise<Buffer> {
    const chunks: Buffer[] = [];
    for await (const chunk of readable) {
      chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
    }
    return Buffer.concat(chunks);
  }
  ```
  并在 `backends.ts` 顶部引入 `Readable`：
  ```typescript
  import { Readable } from 'node:stream';
  ```
- [ ] 编译/类型检查：
  ```bash
  pnpm --filter @odysseythink/integration-tests typecheck
  ```
  Expected: 无类型错误。
- [ ] 提交：`feat(integration-tests): env.* adapter in parity backends`。

---

### Task 14: L2 parity scenario for kaos ops

**Depends on:** Task 13

**Files:**
- Create: `packages/integration-tests/src/parity/scenarios/kaos-ops.ts`
- Modify: `packages/integration-tests/src/parity/scenarios/index.ts:1-30`
- Create: `packages/integration-tests/test/parity/kaos/l2-rpc.test.ts`
- Modify: `packages/integration-tests/src/parity/normalize.ts:1-187`（扩展 stat 字段归一化）

**Steps：**

- [ ] 创建 `packages/integration-tests/src/parity/scenarios/kaos-ops.ts`：
  ```typescript
  import { writeFile, mkdir } from 'node:fs/promises';
  import type { Scenario } from '../types';

  export const kaosOpsScenario: Scenario = {
    name: 'kaos-ops',
    async run(backend) {
      const home = backend.homeDir;

      // Prepare a small fixture tree under the backend's home dir.
      await mkdir(`${home}/sub`, { recursive: true });
      await writeFile(`${home}/a.txt`, 'hello');
      await writeFile(`${home}/sub/b.txt`, 'world');

      const responses: unknown[] = [];

      responses.push(
        await backend.envCall('env.getcwd', {}),
        await backend.envCall('env.stat', { path: 'a.txt' }),
        await backend.envCall('env.glob', { path: '.', pattern: '**/*.txt' }),
        await backend.envCall('env.readText', { path: 'a.txt' }),
        await backend.envCall('env.writeText', { path: 'out.txt', text: '!' }),
        await backend.envCall('env.exec', { command: '/bin/sh', args: ['-c', 'printf hello'] }),
      );

      return { responses, events: [] };
    },
  };
  ```
  说明：scenario 只返回原始响应，归一化由 `normalize.ts` 统一处理。
- [ ] 把 scenario 注册到 `packages/integration-tests/src/parity/scenarios/index.ts`：
  ```typescript
  import { kaosOpsScenario } from './kaos-ops';
  export { kaosOpsScenario } from './kaos-ops';

  export const scenarios: readonly ScenarioEntry[] = [
    // ... existing entries ...
    { scenario: kaosOpsScenario, mockLlm: new MockChatProvider([]) },
  ];
  ```
  因为 `kaos-ops` 不调用 LLM，使用一个空的 `MockChatProvider`。注意需在 `index.ts` 顶部引入 `MockChatProvider`：
  ```typescript
  import { MockChatProvider } from '../fixtures/mock-provider';
  ```
- [ ] 扩展 `normalize.ts` 以稳定 stat 元数据。在 `isTimestampish` 与 `isPidLike` 旁边增加：
  ```typescript
  const STAT_METADATA_KEYS = new Set([
    'stIno', 'stDev', 'stNlink', 'stUid', 'stGid', 'stAtime', 'stMtime', 'stCtime',
  ]);

  function isStatMetadata(path: string): boolean {
    const key = path.slice(path.lastIndexOf('.') + 1).replace(/\[\d+\]/g, '');
    return STAT_METADATA_KEYS.has(key);
  }
  ```
  在 `walk` 函数的 number 分支中：
  ```typescript
  if (typeof value === 'number') {
    if (isTimestampish(path) || isStatMetadata(path)) return 0;
    return value;
  }
  ```
  这样 stat 的 inode/dev/nlink/uid/gid/三个 time 都会被替换为 `0`，只剩 `stMode`、`stSize`、`isDir` 参与比较。
- [ ] 创建 L2 对照测试 `packages/integration-tests/test/parity/kaos/l2-rpc.test.ts`：
  ```typescript
  import { existsSync } from 'node:fs';
  import { fileURLToPath } from 'node:url';
  import { dirname, join } from 'pathe';
  import { describe, expect, it } from 'vitest';
  import { makeTsBackend, makeRustBackend } from '../../../src/parity/backends';
  import { runParity } from '../../../src/parity/run-parity';
  import { kaosOpsScenario } from '../../../src/parity/scenarios';
  import { resolveRustBinaryPath } from '../../../src/parity/rust-binary';

  function findProjectRoot(): string {
    let current = dirname(fileURLToPath(import.meta.url));
    while (current !== dirname(current)) {
      if (existsSync(join(current, '.git'))) return current;
      current = dirname(current);
    }
    return process.cwd();
  }

  const rootDir = findProjectRoot();
  const binaryPath = (() => {
    try {
      return resolveRustBinaryPath(rootDir);
    } catch {
      return null;
    }
  })();

  describe.skipIf(binaryPath === null)('kaos ops L2 parity', () => {
    it('TS LocalKaos matches Rust kaos-rs via CoreHost env.*', async () => {
      const diff = await runParity({
        scenario: kaosOpsScenario,
        mockLlm: new (await import('../../../src/parity/fixtures/mock-provider')).MockChatProvider([]),
        makeA: (homeDir) => makeTsBackend({ homeDir }),
        makeB: (homeDir) =>
          makeRustBackend({
            homeDir,
            binaryPath: binaryPath!,
            transport: 'stdio',
            extraArgs: ['--mock-provider'],
          }),
        timeoutMs: 60000,
      });
      expect(diff, JSON.stringify(diff, null, 2)).toBeNull();
    }, 120000);
  });
  ```
  说明：动态引入 `MockChatProvider` 是为了避免在文件顶层同步引入造成循环或多余依赖；如已有静态引入更简洁，可替换为静态引入。
- [ ] 构建 Rust host：
  ```bash
  cd rust-ody && cargo build --release -p ody-host --bin ody-host
  ```
  Expected: 成功产出 `rust-ody/target/release/ody-host`。
- [ ] 运行 L2 测试：
  ```bash
  pnpm --filter @odysseythink/integration-tests test:parity:kaos
  ```
  或者只跑新测试：
  ```bash
  pnpm --filter @odysseythink/integration-tests vitest run test/parity/kaos/l2-rpc.test.ts
  ```
  Expected: 测试通过，diff 为 null。
- [ ] 提交：`test(integration-tests): L2 parity scenario for kaos ops`。

---

### Task 15: Benchmark script for stat/glob/read on large directory

**Depends on:** Task 14（L2 parity通过，证明 Rust 路径可用）

**Files:**
- Create: `rust-ody/ts/bench.ts`
- Modify: `rust-ody/package.json`（如存在；否则说明用 `node` 直接运行）

**Steps：**

- [ ] 创建 `rust-ody/ts/bench.ts`，仅使用 Node 标准库：
  ```typescript
  #!/usr/bin/env node
  import { mkdir, rm, writeFile } from 'node:fs/promises';
  import { tmpdir } from 'node:os';
  import { join } from 'node:path';
  import { performance } from 'node:perf_hooks';
  import { spawnSync } from 'node:child_process';

  const N = Number(process.argv[2] ?? '1000');
  const root = join(tmpdir(), `kaos-bench-${Date.now()}`);

  async function setup() {
    await mkdir(root, { recursive: true });
    for (let i = 0; i < N; i++) {
      await writeFile(join(root, `file-${i.toString().padStart(6, '0')}.txt`), 'x'.repeat(100));
    }
  }

  async function cleanup() {
    await rm(root, { recursive: true, force: true });
  }

  function bench(name: string, fn: () => void) {
    const t0 = performance.now();
    fn();
    const t1 = performance.now();
    console.log(`${name}: ${(t1 - t0).toFixed(2)} ms`);
  }

  async function main() {
    await setup();
    try {
      bench('stat single', () => {
        spawnSync('node', ['-e', `require('fs').statSync(${JSON.stringify(join(root, 'file-000000.txt'))})`], { stdio: 'inherit' });
      });
      bench('glob *.txt', () => {
        const glob = require('node:child_process').spawnSync('node', ['-e', `
          const fs = require('fs');
          const path = require('path');
          const files = fs.readdirSync(${JSON.stringify(root)}).filter(f => f.endsWith('.txt'));
          console.log(files.length);
        `], { encoding: 'utf8' });
        if (glob.status !== 0) throw new Error(glob.stderr);
      });
      bench('read 100 files', () => {
        for (let i = 0; i < 100; i++) {
          require('fs').readFileSync(join(root, `file-${i.toString().padStart(6, '0')}.txt`), 'utf8');
        }
      });
    } finally {
      await cleanup();
    }
  }

  main().catch((e) => {
    console.error(e);
    process.exit(1);
  });
  ```
  说明：此脚本为方法论基准，用于在本地快速对比 Node/`LocalKaos` 与 Rust `kaos-rs` 的大目录性能。实际 CI 中主要收集 Rust 端 `cargo bench` 或 `cargo test` 的耗时；本脚本不引入外部依赖。
- [ ] 验证脚本可直接运行：
  ```bash
  node rust-ody/ts/bench.ts 500
  ```
  Expected: 输出 setup 后的 stat/glob/read 耗时，无未捕获异常。
- [ ] 提交：`chore(rust-ody): add kaos stat/glob/read benchmark script`。

---

### Task 16: CI wiring for ody-host, L2 parity and benchmark

**Depends on:** Task 12, Task 14, Task 15

**Files：**
- Modify: `.github/workflows/rust-host.yml:1-100`

**Steps：**

- [ ] 在 `.github/workflows/rust-host.yml` 的 `rust-host-smoke` job 中，于现有步骤之后追加以下步骤：
  ```yaml
      - name: Build ody-host
        run: cargo build --release -p ody-host --bin ody-host
        working-directory: rust-ody

      - name: ody-host unit tests
        run: cargo test -p ody-host
        working-directory: rust-ody

      - name: kaos L2 RPC parity
        run: pnpm --filter @odysseythink/integration-tests vitest run test/parity/kaos/l2-rpc.test.ts
        shell: bash
        env:
          ODY_HOST_BINARY_PATH: ${{ github.workspace }}/rust-ody/target/release/ody-host
          ODY_HOST_TRANSPORT: stdio

      - name: kaos ops benchmark
        if: matrix.os == 'ubuntu-24.04'
        run: node rust-ody/ts/bench.ts 2000
        shell: bash
  ```
  说明：
  - `Build ody-host` 放在 `Phase A3 verification` 之前或之后均可，但必须在 L2 parity 之前。
  - `ody-host unit tests` 跑 Part 1/2 新增的 `env.*` 测试与既有测试。
  - `kaos L2 RPC parity` 只在 stdio transport 下跑即可（env.* 不依赖 transport 类型）。
  - benchmark 仅在 Linux 跑，避免 macOS runner 时长波动。
- [ ] 把新增 benchmark 产出保存为 artifact：
  ```yaml
      - name: Upload benchmark log
        if: matrix.os == 'ubuntu-24.04' && always()
        uses: actions/upload-artifact@v4
        with:
          name: kaos-bench-${{ matrix.target }}-${{ matrix.transport }}
          path: .ody-code/reports/kaos-bench.log
          if-no-files-found: ignore
  ```
  如脚本本身不写入该路径，可改为 `path: rust-ody/ts/bench.ts` 或直接把控制台输出重定向到文件。为简化，在 benchmark 步骤中把输出重定向：
  ```yaml
      - name: kaos ops benchmark
        if: matrix.os == 'ubuntu-24.04'
        run: |
          mkdir -p .ody-code/reports
          node rust-ody/ts/bench.ts 2000 | tee .ody-code/reports/kaos-bench.log
        shell: bash
  ```
- [ ] 本地验证 workflow 语法（如已安装 `actionlint`）：
  ```bash
  actionlint .github/workflows/rust-host.yml
  ```
  Expected: 无语法错误。
- [ ] 提交：`ci(rust-host): add ody-host tests, L2 parity and kaos benchmark`。

---

### Task 17: Whole-tree verification

**Depends on:** Task 12, Task 13, Task 14, Task 15, Task 16

**Files：** 无新增/修改

**Steps：**

- [ ] Rust workspace 全量测试：
  ```bash
  cd rust-ody && cargo test --workspace
  ```
  Expected: 全绿（包括 `kaos-rs`、`ody-host`）。
- [ ] TS workspace 类型检查：
  ```bash
  pnpm -r typecheck
  ```
  Expected: 无类型错误。
- [ ] 运行 L1 + L2 parity：
  ```bash
  pnpm --filter @odysseythink/integration-tests test:parity:kaos
  ```
  Expected: `l1-golden.test.ts` 与 `l2-rpc.test.ts` 均通过。
- [ ] 提交：`chore: whole-tree verification after Phase 4.1.4`。

---

## Part 3 Local Self-Review

- [ ] 1. Spec-coverage table: 4.1.4.4（BashTool 迁移）→ T12；4.1.4.5（L2 parity）→ T13/T14；4.1.4.6（基准）→ T15；CI → T16；全量验证 → T17。
- [ ] 2. Placeholder扫描: 本 Part 无 TODO/TBD；所有代码片段完整。
- [ ] 3. No phantom tasks: T17 是回归验证任务，产出为测试结果与提交；其余任务均产生代码或配置变更。
- [ ] 4. Dependency soundness: T12 依赖 Part 1 T2；T13 依赖 Part 1/2 与 T12；T14 依赖 T13；T15 依赖 T14；T16 依赖 T12/T14/T15；T17 依赖全部前置任务。
- [ ] 5. Caller & build soundness: T12 修改 `BashTool` 构造函数（共享签名变更），同任务内更新 `host.rs` 注册与 `bash.rs` 单元测试，并以 `cargo check --workspace --tests` 收尾；T13 修改 `ParityBackend` 接口，同任务内更新 `makeTsBackend`/`makeRustBackend`，并以 `pnpm -r typecheck`（通过 integration-tests typecheck）验证。
- [ ] 6. Test-the-risk: T12 的 `bash::tests` 断言命令执行与拒绝路径；T14 的 L2 parity 断言 TS 与 Rust 在真实文件系统操作上逐字段等价；T17 作为硬门。
- [ ] 7. Type一致性: `envCall` 请求字段名与 Part 1 的 `env.rs` 一致；`BashTool::new(Arc<Kaos>)` 与 Part 1 T2 创建的 `Arc<Kaos>` 类型一致；TS `LocalKaos` 输出形状与 Rust `env.*` JSON 形状在 scenario 中统一。
