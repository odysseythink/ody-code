# Phase A2 — TypeScript Part: Cross-Language RPC Tests

**Scope:** Create `packages/node-sdk/test/rust-host-connect.test.ts` with a shared harness that spawns the real `ody-host` release binary and exercises session lifecycle over stdio, UDS, and TCP. Assert fixed-id and auto-generated-id creation, `listSessions` inclusion, and clean process exit.

**Prerequisite:** Phase A1 CLI launch convention is merged and Phase A Rust optional `id` support is merged and the release binary is available.

## Task B1: Shared harness + stdio lifecycle test

**Depends on:** Task A1

**Files:**
- Create: `packages/node-sdk/test/rust-host-connect.test.ts`

### Steps

- [ ] Write the failing test.

  Create `packages/node-sdk/test/rust-host-connect.test.ts` with the shared harness, the stdio lifecycle test, and the clean-exit test:

  ```typescript
  import type { ChildProcess } from 'node:child_process';
  import { existsSync } from 'node:fs';
  import { mkdtemp, rm } from 'node:fs/promises';
  import { tmpdir } from 'node:os';
  import { join } from 'node:path';
  import { fileURLToPath } from 'node:url';
  import { describe, expect, it } from 'vitest';

  import { SDKRpcClient, type SDKRpcClientConnectOptions } from '../src/rpc';

  interface WithRustHostOptions {
    readonly transport:
      | SDKRpcClientConnectOptions['transport']
      | ((homeDir: string) => SDKRpcClientConnectOptions['transport']);
    readonly binaryPath?: string | undefined;
  }

  interface HostFixture {
    readonly client: SDKRpcClient;
    readonly homeDir: string;
    readonly proc?: ChildProcess | undefined;
  }

  function resolveBinaryPath(env: NodeJS.ProcessEnv = process.env): string {
    if (env.ODY_HOST_BINARY_PATH) {
      return env.ODY_HOST_BINARY_PATH;
    }
    const candidate = fileURLToPath(
      new URL('../../../rust-ody/target/release/ody-host', import.meta.url),
    );
    if (existsSync(candidate)) {
      return candidate;
    }
    throw new Error(
      `ody-host binary not found at ${candidate}. ` +
        `Build with "pnpm run build:host" or set ODY_HOST_BINARY_PATH.`,
    );
  }

  async function withRustHost<T>(
    options: WithRustHostOptions,
    testFn: (fixture: HostFixture) => Promise<T>,
  ): Promise<T> {
    const homeDir = await mkdtemp(join(tmpdir(), 'ody-rust-host-'));
    const transport =
      typeof options.transport === 'function'
        ? options.transport(homeDir)
        : options.transport;
    const client = await SDKRpcClient.connect({
      transport,
      binaryPath: options.binaryPath ?? resolveBinaryPath(process.env),
      homeDir,
    });
    const proc = (client as unknown as { _hostProc?: ChildProcess })._hostProc;
    try {
      return await testFn({ client, homeDir, proc });
    } finally {
      await client.close?.();
      await rm(homeDir, { recursive: true, force: true });
    }
  }

  async function assertSessionLifecycle(
    client: SDKRpcClient,
    homeDir: string,
    fixedId?: string,
  ): Promise<void> {
    const input =
      fixedId !== undefined
        ? { workDir: homeDir, id: fixedId }
        : { workDir: homeDir };
    const session = await client.createSession(input);
    expect(typeof session.id).toBe('string');
    if (fixedId !== undefined) {
      expect(session.id).toBe(fixedId);
    }
    const sessions = await client.listSessions({ workDir: homeDir });
    expect(sessions.some((s) => s.id === session.id)).toBe(true);
    await client.closeSession({ sessionId: session.id });
  }

  describe('SDKRpcClient.connect with real ody-host', () => {
    it('stdio transport creates and lists a session', async () => {
      await withRustHost({ transport: 'stdio' }, async ({ client, homeDir }) => {
        await assertSessionLifecycle(client, homeDir, 'stdio-session-id');
      });
    });

    it('stdio host process exits cleanly after close', async () => {
      await withRustHost({ transport: 'stdio' }, async ({ client, proc }) => {
        expect(proc).toBeDefined();
        const exitPromise = new Promise<number | null>((resolve) =>
          proc!.once('exit', (code) => resolve(code)),
        );
        await client.close?.();
        const exitCode = await exitPromise;
        expect(exitCode).toBe(0);
      });
    });
  });
  ```

- [ ] Run the new test file and verify it FAILS.

  ```bash
  pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts
  ```

  Expected failure (one of):
  - If the release binary is not built: `ody-host binary not found at ...`.
  - If Task A1 is not yet merged: `Expected: "stdio-session-id"` but received a generated UUID.

- [ ] Write the minimal implementation.

  No TypeScript production code changes are required. Build the release binary so the test can spawn the real Rust host:

  ```bash
  pnpm run build:host
  ```

- [ ] Run the test again and verify it PASSES.

  ```bash
  pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts
  ```

  Expected output ends with:

  ```text
  ✓ packages/node-sdk/test/rust-host-connect.test.ts (2 tests)
  ```

- [ ] Typecheck the package to ensure the new test file compiles.

  ```bash
  pnpm --filter @odysseythink/ody-code-sdk run typecheck
  ```

- [ ] Commit.

  ```bash
  git add packages/node-sdk/test/rust-host-connect.test.ts
  git commit -m "test(node-sdk): add real ody-host stdio connect test"
  ```

## Task B2: UDS transport test

**Depends on:** Task B1

**Files:**
- Modify: `packages/node-sdk/test/rust-host-connect.test.ts` (append one `it` block and one helper)

### Steps

- [ ] Write the failing test.

  Append inside the `describe` block, before the closing `});`:

  ```typescript
    it('uds transport creates and lists a session', async () => {
      await withRustHost(
        { transport: (homeDir) => makeUdsTransport(homeDir) },
        async ({ client, homeDir }) => {
          await assertSessionLifecycle(client, homeDir, 'uds-session-id');
        },
      );
    });
  ```

- [ ] Run the test file and verify it FAILS.

  ```bash
  pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts
  ```

  Expected failure: `ReferenceError: makeUdsTransport is not defined` (compile-time or runtime).

- [ ] Write the minimal implementation.

  Add the helper immediately above the `describe` block in the same file:

  ```typescript
  function makeUdsTransport(homeDir: string): SDKRpcClientConnectOptions['transport'] {
    return { socketPath: join(homeDir, 'host.sock'), spawn: true };
  }
  ```

- [ ] Run the test again and verify it PASSES.

  ```bash
  pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts
  ```

  Expected output ends with:

  ```text
  ✓ packages/node-sdk/test/rust-host-connect.test.ts (3 tests)
  ```

- [ ] Commit.

  ```bash
  git add packages/node-sdk/test/rust-host-connect.test.ts
  git commit -m "test(node-sdk): add real ody-host UDS connect test"
  ```

## Task B3: TCP transport test + auto-id test

**Depends on:** Task B1

**Files:**
- Modify: `packages/node-sdk/test/rust-host-connect.test.ts` (append retry helper, TCP test, and auto-id test)

### Steps

- [ ] Write the failing tests.

  Append inside the `describe` block, before the closing `});`:

  ```typescript
    it('tcp transport creates and lists a session', async () => {
      await bindTcpHost(async ({ client, homeDir }) => {
        await assertSessionLifecycle(client, homeDir, 'tcp-session-id');
      });
    });

    it('createSession without id generates a uuid', async () => {
      await withRustHost({ transport: 'stdio' }, async ({ client, homeDir }) => {
        const session = await client.createSession({ workDir: homeDir });
        expect(session.id).toBeDefined();
        expect(session.id).not.toBe('');
        expect(session.id.length).toBeGreaterThan(10);
        const sessions = await client.listSessions({ workDir: homeDir });
        expect(sessions.some((s) => s.id === session.id)).toBe(true);
        await client.closeSession({ sessionId: session.id });
      });
    });
  ```

- [ ] Run the test file and verify it FAILS.

  ```bash
  pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts
  ```

  Expected failure: `ReferenceError: bindTcpHost is not defined`.

- [ ] Write the minimal implementation.

  Add these two helpers immediately above the `describe` block, after the existing helpers:

  ```typescript
  function isAddrInUse(err: Error): boolean {
    const message = err.message.toLowerCase();
    return (
      message.includes('eaddrinuse') ||
      message.includes('address already in use') ||
      message.includes('os error 48') ||
      message.includes('os error 98')
    );
  }

  async function bindTcpHost(
    testFn: (fixture: HostFixture) => Promise<void>,
  ): Promise<void> {
    const basePort = 19090;
    const maxAttempts = 10;
    let lastErr: unknown;
    for (let offset = 0; offset < maxAttempts; offset += 1) {
      const port = basePort + offset;
      try {
        await withRustHost(
          { transport: { host: '127.0.0.1', port, spawn: true } },
          testFn,
        );
        return;
      } catch (err) {
        lastErr = err;
        if (err instanceof Error && isAddrInUse(err) && offset < maxAttempts - 1) {
          continue;
        }
        throw err;
      }
    }
    throw lastErr instanceof Error
      ? lastErr
      : new Error('bindTcpHost exhausted all ports');
  }
  ```

  Filter verification for `isAddrInUse`:

  | Input error message | Must retry? |
  |---|---|
  | `EADDRINUSE: address already in use :::19090` | Yes |
  | `cannot bind tcp socket 127.0.0.1:19090: Address already in use (os error 48)` | Yes |
  | `cannot bind tcp socket 127.0.0.1:19090: Permission denied (os error 13)` | **No** |
  | `unexpected argument 'serve' found` | **No** |

  Confirm none of the must-retry phrases overlap with the must-not-retry cases.

- [ ] Run the test again and verify it PASSES.

  ```bash
  pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts
  ```

  Expected output ends with:

  ```text
  ✓ packages/node-sdk/test/rust-host-connect.test.ts (5 tests)
  ```

- [ ] Typecheck the package.

  ```bash
  pnpm --filter @odysseythink/ody-code-sdk run typecheck
  ```

- [ ] Commit.

  ```bash
  git add packages/node-sdk/test/rust-host-connect.test.ts
  git commit -m "test(node-sdk): add real ody-host TCP and auto-id tests"
  ```

## Local Self-Review

- [ ] 1. Spec coverage: stdio lifecycle (B1), clean process exit (B1), UDS transport (B2), TCP transport (B3), fixed-id assertion (B1/B2/B3), auto-id assertion (B3) — all covered.
- [ ] 2. Placeholder scan: no TODO/TBD; all helpers and test code are complete.
- [ ] 3. No phantom tasks: each task modifies the test file and verifies with `pnpm vitest run`.
- [ ] 4. Dependency soundness: B1 depends on A1; B2 and B3 depend on B1. No symbol is used before it is defined in the file.
- [ ] 5. Caller & build soundness: no shared TypeScript signature is changed. Each task ends with either a package-level typecheck (B1, B3) or a targeted vitest run (B2).
- [ ] 6. Test-the-risk: behavioral asserts on session id mutation, `listSessions` inclusion, process exit code, and TCP retry filtering are present.
- [ ] 7. Type consistency: `SDKRpcClientConnectOptions['transport']`, `SDKRpcClient`, `ChildProcess`, and the payload shapes match the existing SDK and Node.js types.
