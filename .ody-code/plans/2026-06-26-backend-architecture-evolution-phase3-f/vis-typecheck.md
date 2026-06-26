# Part C — `apps/vis/web` Typecheck Remediation

> Scope: eliminate the 15 TypeScript errors surfaced when running `pnpm typecheck` inside `apps/vis/web`. The errors are in transitive workspace dependencies that `apps/vis/web` compiles with its own strict `noUnusedLocals` / `noUnusedParameters` settings. No behavior changes; only type-level cleanups.

**Goal:** Make `cd apps/vis/web && pnpm typecheck` emit zero errors.

**Architecture:** `apps/vis/web` imports from `packages/agent-core`, which in turn imports from `packages/agent-core-shared`, `packages/e2e-testing`, and `packages/mcp-host`. The Vite/React app's `tsconfig.json` enables `noUnusedLocals` and `noUnusedParameters`, so unused imports/parameters in upstream packages are reported here. We fix the upstream sources so the whole workspace typechecks cleanly under these strict settings.

**Tech Stack:** TypeScript, pnpm workspaces, `tsc --noEmit`.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## Dependency Overview

All tasks in this part are independent except the final verification. They can be executed in any order, but the grouping below keeps related files together. The only shared concern is the `wasm-loader.ts` cast in Task C1, which is read by multiple downstream consumers; it does not change any exported signature.

- **Phase C1**: `packages/agent-core-shared/src/wasm-loader.ts` — DOM-typing-safe `WebAssembly.instantiate` result handling.
- **Phase C2**: `packages/agent-core/src/agent/permission/*` — remove unused parameter/type import.
- **Phase C3**: `packages/agent-core/src/rpc/*` — remove unused parameters/functions.
- **Phase C4**: `packages/agent-core/src/session/checkpoint/*` — remove unused imports/types.
- **Phase C5**: `packages/agent-core/src/session/hooks/types.ts` + `packages/agent-core/src/utils/wasm-glob.ts` — remove unused re-exported constants.
- **Phase C6**: `packages/e2e-testing/src/*` — remove unused imports/parameters/properties.
- **Phase C7**: `packages/mcp-host/src/*` — remove unused imports/types.
- **Phase C8**: Final `apps/vis/web` typecheck verification.

## Risks & Open Questions

- These changes are purely cosmetic at the type level, but removing an unused re-export could theoretically break a consumer that imported it transitively. For `ParsedPattern`, `HOOK_EVENT_TYPES`, and `OAuthClientProvider`, the plan keeps the public re-export and only removes the redundant local import binding.
- `GLOB_ERROR` in `wasm-glob.ts` may be intended for future use; if so, prefix with `_` or add an explicit eslint/ts ignore rather than deleting. The plan assumes it is unused and safe to remove.
- `CheckpointVersion` is imported as a type only; verify it is not referenced elsewhere in the file before removing.

---

### Task C1: Fix `wasm-loader.ts` `WebAssembly.instantiate` result type under DOM lib

**Depends on:** none

**Files:**
- Modify: `packages/agent-core-shared/src/wasm-loader.ts:46`

TypeScript under the DOM lib resolves the `BufferSource` overload of `WebAssembly.instantiate` to `Promise<Instance>` in some contexts, so destructuring `{ instance }` fails. Cast the promise result to `WebAssembly.WebAssemblyInstantiatedSource`.

- [ ] Change line 46 from:
  ```ts
  const { instance } = await WebAssembly.instantiate(bytes, {});
  ```
  to:
  ```ts
  const { instance } = await WebAssembly.instantiate(bytes, {}) as WebAssembly.WebAssemblyInstantiatedSource;
  ```

- [ ] Manual verification:
  ```bash
  cd /Users/ranwei/workspace/ody-code/apps/vis/web && pnpm typecheck 2>&1 | grep wasm-loader.ts || true
  ```
  Expected: no output (the `wasm-loader.ts` error is gone). Other errors from unrelated files may still appear.

- [ ] Commit: `fix(agent-core-shared): make wasm instantiate result DOM-typing-safe`

---

### Task C2: Clean up unused permission symbols

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/agent/permission/index.ts:325`
- Modify: `packages/agent-core/src/agent/permission/matches-rule.ts:3`

`requestSetupScriptApproval` declares `signal` but never reads it; prefix with underscore to satisfy `noUnusedParameters`. `matches-rule.ts` imports `ParsedPattern` only to re-export it from the same module; remove the local import while keeping the re-export.

- [ ] In `packages/agent-core/src/agent/permission/index.ts:325`, change:
  ```ts
  signal?: AbortSignal,
  ```
  to:
  ```ts
  _signal?: AbortSignal,
  ```

- [ ] In `packages/agent-core/src/agent/permission/matches-rule.ts:3`, change:
  ```ts
  import { parsePattern, type ParsedPattern } from '@odysseythink/agent-core-shared';
  ```
  to:
  ```ts
  import { parsePattern } from '@odysseythink/agent-core-shared';
  ```

- [ ] Manual verification:
  ```bash
  cd /Users/ranwei/workspace/ody-code/apps/vis/web && pnpm typecheck 2>&1 | grep -E "permission/(index|matches-rule)\.ts" || true
  ```
  Expected: no output.

- [ ] Commit: `chore(agent-core): remove unused permission parameter and redundant type import`

---

### Task C3: Clean up unused RPC symbols

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/rpc/client.ts:154`
- Modify: `packages/agent-core/src/rpc/transports/websocket.ts:66`

`mapRpcFunction` declares `fn` but never reads it (the inner closure uses a different `fn` from the outer scope). `decodeJson` in `websocket.ts` is defined but never called.

- [ ] In `packages/agent-core/src/rpc/client.ts:154`, change:
  ```ts
  function mapRpcFunction(methodName: string, fn: Function, transport: Transport): Function {
  ```
  to:
  ```ts
  function mapRpcFunction(methodName: string, _fn: Function, transport: Transport): Function {
  ```

- [ ] In `packages/agent-core/src/rpc/transports/websocket.ts:66`, remove the unused function:
  ```ts
  function decodeJson(bytes: Uint8Array): unknown {
    return JSON.parse(new TextDecoder().decode(bytes));
  }
  ```

- [ ] Manual verification:
  ```bash
  cd /Users/ranwei/workspace/ody-code/apps/vis/web && pnpm typecheck 2>&1 | grep -E "rpc/(client|transports/websocket)\.ts" || true
  ```
  Expected: no output.

- [ ] Commit: `chore(agent-core): remove unused rpc parameter and dead decoder helper`

---

### Task C4: Clean up unused checkpoint symbols

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/session/checkpoint/backup-store.ts:10`
- Modify: `packages/agent-core/src/session/checkpoint/recovery.ts:17`

`backup-store.ts` imports `dirname` from `pathe` but never uses it. `recovery.ts` imports `CheckpointVersion` as a type but never references it.

- [ ] In `packages/agent-core/src/session/checkpoint/backup-store.ts:10`, change:
  ```ts
  import { dirname, join } from 'pathe';
  ```
  to:
  ```ts
  import { join } from 'pathe';
  ```

- [ ] In `packages/agent-core/src/session/checkpoint/recovery.ts:17`, change:
  ```ts
  import type { CheckpointIndexData, CheckpointVersion } from './checkpoint-index';
  ```
  to:
  ```ts
  import type { CheckpointIndexData } from './checkpoint-index';
  ```

- [ ] Manual verification:
  ```bash
  cd /Users/ranwei/workspace/ody-code/apps/vis/web && pnpm typecheck 2>&1 | grep -E "session/checkpoint/(backup-store|recovery)\.ts" || true
  ```
  Expected: no output.

- [ ] Commit: `chore(agent-core): remove unused checkpoint imports and type`

---

### Task C5: Clean up unused re-exported constants in hooks and wasm-glob

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/session/hooks/types.ts:3`
- Modify: `packages/agent-core/src/utils/wasm-glob.ts:13`

`hooks/types.ts` imports `HOOK_EVENT_TYPES` only to re-export it; remove the local import binding and keep the re-export. `wasm-glob.ts` declares `GLOB_ERROR` but never uses it; remove it.

- [ ] In `packages/agent-core/src/session/hooks/types.ts:3`, change:
  ```ts
  import { HOOK_EVENT_TYPES, type HookEventType } from '@odysseythink/agent-core-shared';
  ```
  to:
  ```ts
  import { type HookEventType } from '@odysseythink/agent-core-shared';
  ```

- [ ] In `packages/agent-core/src/utils/wasm-glob.ts`, delete line 13:
  ```ts
  const GLOB_ERROR = 0xFFFFFFFF;
  ```

- [ ] Manual verification:
  ```bash
  cd /Users/ranwei/workspace/ody-code/apps/vis/web && pnpm typecheck 2>&1 | grep -E "(session/hooks/types|utils/wasm-glob)\.ts" || true
  ```
  Expected: no output.

- [ ] Commit: `chore(agent-core): remove unused re-exported constant and dead glob error constant`

---

### Task C6: Clean up unused e2e-testing symbols

**Depends on:** none

**Files:**
- Modify: `packages/e2e-testing/src/generators/python-pytest.ts:1`
- Modify: `packages/e2e-testing/src/recursive-impact-analyzer.ts:304`
- Modify: `packages/e2e-testing/src/result-cache.ts:54`

`python-pytest.ts` imports `extname` but never uses it. `recursive-impact-analyzer.ts` declares `existsSync` as a parameter but never reads it. `result-cache.ts` declares a private `kaos` property but never reads it.

- [ ] In `packages/e2e-testing/src/generators/python-pytest.ts:1`, change:
  ```ts
  import { join, extname } from 'pathe';
  ```
  to:
  ```ts
  import { join } from 'pathe';
  ```

- [ ] In `packages/e2e-testing/src/recursive-impact-analyzer.ts:304`, change:
  ```ts
  existsSync: typeof import('node:fs').existsSync,
  ```
  to:
  ```ts
  _existsSync: typeof import('node:fs').existsSync,
  ```

- [ ] In `packages/e2e-testing/src/result-cache.ts:54`, change:
  ```ts
    private readonly kaos: Kaos,
  ```
  to:
  ```ts
    private readonly _kaos: Kaos,
  ```

- [ ] Manual verification:
  ```bash
  cd /Users/ranwei/workspace/ody-code/apps/vis/web && pnpm typecheck 2>&1 | grep -E "e2e-testing/src/(generators/python-pytest|recursive-impact-analyzer|result-cache)\.ts" || true
  ```
  Expected: no output.

- [ ] Commit: `chore(e2e-testing): remove unused imports, parameter, and private property`

---

### Task C7: Clean up unused mcp-host symbols

**Depends on:** none

**Files:**
- Modify: `packages/mcp-host/src/built-in/sea-builtins.ts:2`
- Modify: `packages/mcp-host/src/oauth/service.ts:29`
- Modify: `packages/mcp-host/src/trace-recorder.ts:2`

`sea-builtins.ts` imports `readFileSync` but never uses it. `oauth/service.ts` imports `OAuthClientProvider` as a type but never references it. `trace-recorder.ts` imports `dirname` from `node:path` but never uses it.

- [ ] In `packages/mcp-host/src/built-in/sea-builtins.ts:2`, change:
  ```ts
  import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
  ```
  to:
  ```ts
  import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
  ```

- [ ] In `packages/mcp-host/src/oauth/service.ts:29`, change:
  ```ts
  discoverOAuthServerInfo,
  exchangeAuthorization,
  registerClient,
  type OAuthClientProvider,
} from '@modelcontextprotocol/sdk/client/auth.js';
  ```
  to:
  ```ts
  discoverOAuthServerInfo,
  exchangeAuthorization,
  registerClient,
} from '@modelcontextprotocol/sdk/client/auth.js';
  ```

- [ ] In `packages/mcp-host/src/trace-recorder.ts:2`, change:
  ```ts
  import { dirname, join } from 'node:path';
  ```
  to:
  ```ts
  import { join } from 'node:path';
  ```

- [ ] Manual verification:
  ```bash
  cd /Users/ranwei/workspace/ody-code/apps/vis/web && pnpm typecheck 2>&1 | grep -E "mcp-host/src/(built-in/sea-builtins|oauth/service|trace-recorder)\.ts" || true
  ```
  Expected: no output.

- [ ] Commit: `chore(mcp-host): remove unused imports and type import`

---

### Task C8: Final `apps/vis/web` typecheck verification

**Depends on:** Task C1, Task C2, Task C3, Task C4, Task C5, Task C6, Task C7

**Files:** none (verification only)

- [ ] Run the full workspace typecheck from `apps/vis/web`:
  ```bash
  cd /Users/ranwei/workspace/ody-code/apps/vis/web && pnpm typecheck
  ```
  Expected output: command exits with code 0 and prints nothing after the `tsc --noEmit` banner.

- [ ] Run the root-level typecheck to ensure no regressions in other strict packages:
  ```bash
  cd /Users/ranwei/workspace/ody-code && pnpm -r typecheck
  ```
  Expected output: all workspace packages report success.

- [ ] Commit (if any remaining formatting changes): `chore(vis): verify whole-tree typecheck after upstream cleanup`

---

## Self-Review (local to Part C)

- [ ] 1. **Spec-coverage table:**
  | Requirement | Task(s) | Status |
  |---|---|---|
  | Fix `wasm-loader.ts` instance type error | C1 | covered |
  | Remove unused `signal` parameter in permission index | C2 | covered |
  | Remove unused `ParsedPattern` local import in matches-rule | C2 | covered |
  | Remove unused `fn` parameter in rpc client | C3 | covered |
  | Remove unused `decodeJson` helper in websocket transport | C3 | covered |
  | Remove unused `dirname` import in backup-store | C4 | covered |
  | Remove unused `CheckpointVersion` type in recovery | C4 | covered |
  | Remove unused `HOOK_EVENT_TYPES` local import in hooks types | C5 | covered |
  | Remove unused `GLOB_ERROR` constant in wasm-glob | C5 | covered |
  | Remove unused `extname` import in python-pytest generator | C6 | covered |
  | Remove unused `existsSync` parameter in recursive impact analyzer | C6 | covered |
  | Remove unused `kaos` property in result cache | C6 | covered |
  | Remove unused `readFileSync` import in sea-builtins | C7 | covered |
  | Remove unused `OAuthClientProvider` type import in oauth service | C7 | covered |
  | Remove unused `dirname` import in trace-recorder | C7 | covered |
  | `apps/vis/web` typecheck passes with zero errors | C8 | covered |

- [ ] 2. **Placeholder scan:** no TODO/TBD/deferred items; every task shows the exact edit.
- [ ] 3. **No phantom tasks:** every task produces a concrete diff; verification steps are explicit commands.
- [ ] 4. **Dependency soundness:** Task C8 depends only on earlier tasks; all other tasks depend on none and can run in parallel.
- [ ] 5. **Caller & build soundness:** no shared signatures are changed. Public re-exports of `ParsedPattern` and `HOOK_EVENT_TYPES` remain intact; only redundant local import bindings are removed. Task C8 ends with whole-tree `pnpm -r typecheck`.
- [ ] 6. **Test-the-risk:** these are non-behavioral type cleanups; risk is limited to accidental removal of live exports. The plan preserves all public re-exports and only removes confirmed-unused bindings.
- [ ] 7. **Type consistency:** no new types introduced; existing types and property names remain unchanged.
