# Phase A1 Part 2: TS SDK Spawn Error Handling

> Scope: 在 `packages/node-sdk/src/rpc.ts` 中抽取带 `proc.once('error')` 监听的 `spawnHost` 辅助函数，覆盖 binary 不存在时的立即 reject；并在 mock host 测试中校验 `serve` 参数始终位于 `argv[2]`。

---

### Task 4: 增加 `spawnHost` 辅助函数与 spawn error 监听

**Depends on:** none

**Files:**
- Modify: `packages/node-sdk/src/rpc.ts:180-275`（`ExternalTransportResult` 类型、`createExternalTransport` 函数）
- Test: `packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts`

- [ ] Write the failing test

在 `packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts` 的 `describe('SDKRpcClient.connect with custom binary', ...)` 内新增：

```typescript
  it('rejects when binary does not exist', async () => {
    await expect(
      SDKRpcClient.connect({
        transport: 'stdio',
        binaryPath: '/nonexistent/ody-host-binary',
      }),
    ).rejects.toThrow(/Failed to spawn host \/nonexistent\/ody-host-binary/);
  });
```

- [ ] Run it and verify it FAILS

```bash
cd /Users/ranwei/workspace/ody-code/packages/node-sdk
pnpm vitest run test/sdk-rpc-client-connect-binary.test.ts -t "rejects when binary does not exist"
```

Expected failure：测试超时（默认 5s）后失败，因为当前 `createExternalTransport` 未监听 `spawn('error')`，`waitForReadyMessage` 永远不会 resolve。

- [ ] Write the minimal implementation

在 `packages/node-sdk/src/rpc.ts` 中 `createExternalTransport` 之前新增 `spawnHost` 辅助函数：

```typescript
interface SpawnHostOptions {
  binaryPath: string;
  argv: string[];
  stdio: ['pipe' | 'ignore', 'pipe' | 'ignore', 'pipe'];
  predicate: (msg: ReadyMessage) => boolean;
}

async function spawnHost(
  options: SpawnHostOptions,
): Promise<{ proc: import('node:child_process').ChildProcess; readyMessage: ReadyMessage }> {
  const { spawn } = await import('node:child_process');
  const proc = spawn(options.binaryPath, options.argv, {
    stdio: options.stdio,
  });
  return new Promise((resolve, reject) => {
    let settled = false;
    const cleanup = (): void => {
      proc.off('error', onError);
    };
    const onError = (err: Error): void => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(
        new Error(
          `Failed to spawn host ${options.binaryPath} with args ${JSON.stringify(options.argv)}: ${err.message}`,
        ),
      );
    };
    proc.once('error', onError);
    waitForReadyMessage(proc.stderr!, options.predicate)
      .then((msg) => {
        if (settled) return;
        settled = true;
        cleanup();
        resolve({ proc, readyMessage: msg });
      })
      .catch((err) => {
        if (settled) return;
        settled = true;
        cleanup();
        reject(err);
      });
  });
}
```

然后替换 `createExternalTransport` 中三处 `spawn` + `waitForReadyMessage` 调用。

**stdio 分支**（替换原 198-208 行）：

```typescript
  if (options.transport === 'stdio') {
    const { proc } = await spawnHost({
      binaryPath,
      argv: ['serve', '--stdio', ...extraArgs],
      stdio: ['pipe', 'pipe', 'pipe'],
      predicate: (msg) => msg.stdio === true,
    });
    return {
      transport: createStreamTransport(proc.stdout!, proc.stdin!, dispatch, { framing: 'length-prefixed' }),
      proc,
    };
  }
```

**socket 分支**（替换原 210-229 行）：

```typescript
  if ('socketPath' in options.transport) {
    const { socketPath, spawn: shouldSpawn } = options.transport;
    let proc: import('node:child_process').ChildProcess | undefined;
    if (shouldSpawn) {
      ({ proc } = await spawnHost({
        binaryPath,
        argv: ['serve', '--socket-path', socketPath, ...extraArgs],
        stdio: ['ignore', 'ignore', 'pipe'],
        predicate: (msg) => msg.socketPath === socketPath,
      }));
    }
    const socket: Socket = connectNet(socketPath);
    await new Promise<void>((resolve, reject) => {
      socket.on('connect', resolve);
      socket.on('error', reject);
    });
    return {
      transport: createStreamTransport(socket, socket, dispatch, { framing: 'length-prefixed' }),
      proc,
    };
  }
```

**TCP 分支**（替换原 234-240 行）：

```typescript
  if (shouldSpawn) {
    ({ proc } = await spawnHost({
      binaryPath,
      argv: ['serve', '--tcp-host', host, '--tcp-port', String(port), ...extraArgs],
      stdio: ['ignore', 'ignore', 'pipe'],
      predicate: (msg) => msg.host === host && msg.port === port,
    }));
  }
```

- [ ] Run it and verify it PASSES

```bash
cd /Users/ranwei/workspace/ody-code/packages/node-sdk
pnpm vitest run test/sdk-rpc-client-connect-binary.test.ts -t "rejects when binary does not exist"
```

Expected output（节选）：

```
 ✓ packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts (1) 342ms
   ✓ SDKRpcClient.connect with custom binary (1)
     ✓ rejects when binary does not exist
```

- [ ] Commit

```bash
cd /Users/ranwei/workspace/ody-code
git add packages/node-sdk/src/rpc.ts packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts
git commit -m "feat(node-sdk): add spawn error handling in createExternalTransport"
```

---

### Task 5: mock host 校验 `serve` 参数

**Depends on:** Task 4

**Files:**
- Test: `packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts:8-31`（`createMockHostScript`）

- [ ] Write the failing test（mock 断言）

修改 `createMockHostScript` 中的 mock script，在顶部增加 `serve` 参数校验：

```typescript
async function createMockHostScript(): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), 'ody-mock-host-'));
  const script = join(dir, 'mock-host.mjs');
  await writeFile(
    script,
    `#!/usr/bin/env node
import { createServer } from 'node:net';
if (process.argv[2] !== 'serve') {
  console.error(JSON.stringify({ type: 'error', message: 'expected argv[2] === "serve", got: ' + process.argv[2] }));
  process.exit(1);
}
const mode = process.argv.includes('--stdio') ? 'stdio' : 'socket';
const socketArg = process.argv.find((a, i) => i > 0 && process.argv[i - 1] === '--socket-path');
if (mode === 'socket' && socketArg) {
  const server = createServer((socket) => {
    socket.on('data', (chunk) => socket.write(chunk));
  });
  await new Promise((resolve) => server.listen(socketArg, resolve));
  console.error(JSON.stringify({ type: 'ready', socketPath: socketArg }));
} else {
  console.error(JSON.stringify({ type: 'ready', stdio: true }));
  process.stdin.on('data', (chunk) => process.stdout.write(chunk));
}
`,
  );
  await chmod(script, 0o755);
  return script;
}
```

- [ ] Run it and verify it FAILS

```bash
cd /Users/ranwei/workspace/ody-code/packages/node-sdk
pnpm vitest run test/sdk-rpc-client-connect-binary.test.ts
```

Expected failure：若当前 `createExternalTransport` 未传递 `serve`，mock 脚本会输出 `expected argv[2] === "serve"...` 并以非零退出，导致 `SDKRpcClient.connect` 在默认超时内无法 resolve，测试失败。

> 注：若 Task 4 已正确保留 `['serve', '--stdio', ...]` 与 `['serve', '--socket-path', ...]`，此测试会直接通过；但必须先写断言并确认失败（例如临时将某处 argv 中的 `'serve'` 移除以验证），再恢复。

- [ ] Write the minimal implementation

Task 4 的实现已保持 `serve` 参数传递，无需新增代码。确认三处 `spawnHost` 调用的 `argv` 均以 `'serve'` 开头。

- [ ] Run it and verify it PASSES

```bash
cd /Users/ranwei/workspace/ody-code/packages/node-sdk
pnpm vitest run test/sdk-rpc-client-connect-binary.test.ts
```

Expected output（节选）：

```
 ✓ packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts (3) 456ms
   ✓ SDKRpcClient.connect with custom binary (3)
     ✓ spawns stdio binary and passes --config/--home
     ✓ spawns socket binary and connects
     ✓ rejects when binary does not exist
```

- [ ] Commit

```bash
cd /Users/ranwei/workspace/ody-code
git add packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts
git commit -m "test(node-sdk): assert serve argument in mock host"
```

---

## Local Self-Review

- [ ] 1. Spec-coverage table（本 Part）：
  - TS `createExternalTransport` 保持传递 `serve` 参数 → Task 5 covered
  - TS 侧增加 spawn error 监听 → Task 4 covered
  - TS 测试验证 mock host 收到 `serve` 参数 → Task 5 covered
- [ ] 2. Placeholder scan：本 Part 无 TODO/TBD/"implement later"。
- [ ] 3. No phantom tasks：每个 Task 都产生可验证的代码/测试变更。
- [ ] 4. Dependency soundness：Task 5 仅依赖 Task 4 创建的 `spawnHost` 辅助函数及保留的 `serve` 参数。
- [ ] 5. Caller & build soundness：
  - `createExternalTransport` 的签名未变；内部新增 `spawnHost` 私有辅助函数。
  - 搜索所有 `createExternalTransport` 调用者：
    ```bash
    rg -n "createExternalTransport" packages/node-sdk/src/
    ```
    仅 `SDKRpcClient.connect` 一处调用，无需更新外部签名。
  - 全树类型检查（因未改动公开签名，但仍需确保 workspace 类型干净）：
    ```bash
    cd /Users/ranwei/workspace/ody-code && pnpm -r typecheck
    ```
- [ ] 6. Test-the-risk：
  - 状态变化：spawn 失败由“无限挂起”变为“立即 reject 并携带 binaryPath 与 argv”；测试断言错误消息正则匹配 `/Failed to spawn host .../`，对应 `spawnHost` 中的模板字符串。
  - mock 断言直接校验 `process.argv[2] === 'serve'`，失败时退出码非零；测试通过即证明 `createExternalTransport` 的 argv 构建常量正确。
- [ ] 7. Type consistency：Task 4 定义的 `SpawnHostOptions` 与 `spawnHost` 返回值类型在 Task 5 中未使用；后续 ADR Part 不依赖 TS 类型。
