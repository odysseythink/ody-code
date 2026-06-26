# Part 4 — Build, Packaging & Done Criteria

> Scope: Rust host 构建矩阵、Node SEA 单文件分发评估、CI 集成、原型完成标准。  
> Corresponds to index: [Architecture & Data Flow](../2026-06-25-backend-architecture-evolution-phase3.md)

---

## Dependency Overview

This part makes the prototype reproducible in CI and documents the Go/No-Go decision.

```
D1: Add ody-host crate to rust-ody workspace
   │
   ▼
D2: Root package.json scripts for build/test/run
   │
   ▼
D3: SEA evaluation — bundle ody-host binary into native assets
   │
   ▼
D4: Native asset smoke includes ody-host
   │
   ▼
D5: CI workflow for Rust host smoke tests
   │
   ▼
D6: Go/No-Go ADR
   │
   ▼
D7: Done-criteria verification run
```

- **External prerequisite**: Part 2 (`transport.md`) and Part 3 (`tui.md`) must be
  implemented so that `ody-host` binary exists and TS connector tests pass.
- D3/D4 are **optional evaluation**; they do not block D7 done criteria but must
  be attempted and their outcome recorded in the ADR.

---

## Tasks

### Task D1: Add `ody-host` crate to the `rust-ody` workspace

**Depends on:** Part 2 (`transport.md`) Task B7 (host binary source exists)

**Files:**
- Modify: `rust-ody/Cargo.toml`
- Create: `rust-ody/crates/ody-host/Cargo.toml`

**Goal:** Make `cargo build -p ody-host` and `cargo test -p ody-host` work from the repo root.

- [ ] Write the failing verification command:

```bash
cargo build -p ody-host
```

Expected failure: `error: package ID specification 'ody-host' matched no packages`.

- [ ] Write the minimal implementation.

Modify `rust-ody/Cargo.toml`:

```toml
[workspace]
members = ["crates/ody-rust", "crates/ody-crypto", "crates/ody-host"]
resolver = "2"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["stream", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
clap = { version = "4", features = ["derive"] }
toml = "0.8"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

Create `rust-ody/crates/ody-host/Cargo.toml`:

```toml
[package]
name = "ody-host"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { workspace = true }
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
clap = { workspace = true }
toml = { workspace = true }
ody-rust = { path = "../ody-rust" }
ody-crypto = { path = "../ody-crypto" }

[dev-dependencies]
tempfile = "3"
```

- [ ] Run the verification command again:

```bash
cargo build -p ody-host
```

Expected: compilation succeeds (or only fails due to missing source code, which is expected until Part 2 is complete).

- [ ] Commit:

```bash
git add rust-ody/Cargo.toml rust-ody/crates/ody-host/Cargo.toml
git commit -m "build(rust): add ody-host crate to workspace"
```

---

### Task D2: Add root `package.json` scripts for Rust host development

**Depends on:** Task D1

**Files:**
- Modify: `package.json`

**Goal:** Provide one-command build/test/run entry points for the Rust host prototype.

- [ ] Write the failing verification command:

```bash
pnpm run build:host
```

Expected failure: `Unknown script "build:host"`.

- [ ] Write the minimal implementation.

Modify `package.json` scripts section (around line 7):

```json
  "scripts": {
    "build": "pnpm -r run build",
    "build:packages": "pnpm -r --filter './packages/*' run build",
    "build:host": "cd rust-ody && cargo build -p ody-host --release",
    "build:native:crypto": "pnpm --filter @odysseythink/ody-crypto run build:native",
    "test:host": "cd rust-ody && cargo test -p ody-host",
    "proto:rust-host": "pnpm run build:host && pnpm -C apps/ody-code run dev:cli-only -- --host=rust --host-stdio",
    "dev:cli": "pnpm -C apps/ody-code run dev",
    ...
  }
```

- [ ] Run the verification command again:

```bash
pnpm run build:host
```

Expected: cargo builds `ody-host` in release mode; executable appears at `rust-ody/target/release/ody-host`.

- [ ] Run the test script:

```bash
pnpm run test:host
```

Expected: cargo test for `ody-host` runs.

- [ ] Commit:

```bash
git add package.json
git commit -m "build: add rust host development scripts"
```

---

### Task D3: Evaluate embedding `ody-host` into the Node SEA bundle

**Depends on:** Task D2 and a successful `pnpm run build:host`

**Files:**
- Modify: `apps/ody-code/scripts/native/02-sea-blob.mjs`
- Modify: `apps/ody-code/scripts/native/native-deps.mjs`
- Modify: `apps/ody-code/src/native/native-assets.ts`
- Create: `apps/ody-code/scripts/native/host-binary.mjs`

**Goal:** Determine whether the Rust binary can be shipped as a SEA asset; if yes, extract it at runtime so `--host=rust` works from a single-file `ody` executable.

- [ ] Write the failing test/verification step first. Create a small script that tries to include a fake `ody-host` binary in a SEA config:

Create `apps/ody-code/scripts/native/host-binary.mjs`:

```javascript
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

const HOST_BINARY_ENV = 'ODY_HOST_BINARY';

export function hostBinaryName(platform = process.platform) {
  return platform === 'win32' ? 'ody-host.exe' : 'ody-host';
}

export function defaultHostBinaryPath(target, platform = process.platform) {
  // Development fallback: repo-relative target/release binary.
  return resolve(
    process.cwd(),
    'rust-ody',
    'target',
    target.endsWith('-debug') ? 'debug' : 'release',
    hostBinaryName(platform),
  );
}

export function resolveHostBinaryPath(target, platform = process.platform) {
  const envPath = process.env[HOST_BINARY_ENV];
  if (envPath !== undefined) return resolve(envPath);
  return defaultHostBinaryPath(target, platform);
}

export function hostBinaryAssetKey(target) {
  return `host/${target}/ody-host`;
}
```

- [ ] Run a verification command to confirm the helper can locate the binary:

```bash
node apps/ody-code/scripts/native/host-binary.mjs
```

Expected failure: `SyntaxError: Unexpected identifier` or no output because the file has no side-effects.

Add a temporary manual runner at the bottom of `host-binary.mjs`:

```javascript
if (import.meta.url === `file://${process.argv[1]}`) {
  const target = `${process.platform}-${process.arch}`;
  console.log(resolveHostBinaryPath(target));
}
```

Run again:

```bash
node apps/ody-code/scripts/native/host-binary.mjs
```

Expected output: `/Users/<you>/workspace/ody-code/rust-ody/target/release/ody-host` (or similar).

- [ ] Write the minimal implementation to include the binary in SEA assets.

Modify `apps/ody-code/scripts/native/native-deps.mjs` to register the host binary as a synthetic native dep. Append to `nativeDeps`:

```javascript
  {
    id: 'ody-host',
    name: () => 'ody-host',
    collect: 'native-files',
    parent: null,
  },
```

But `ody-host` is not installed in `node_modules`. The current `collectPackageFiles` / `resolvePackageRootGeneric` assumes npm packages. We need a custom path for the host binary. Add a special-case branch in `apps/ody-code/scripts/native/assets.mjs` `collectNativeAssets`:

```javascript
import { hostBinaryAssetKey, resolveHostBinaryPath } from './host-binary.mjs';

// Inside collectNativeAssets, after the targetDeps loop:
  const hostBinaryPath = resolveHostBinaryPath(target);
  if (existsSync(hostBinaryPath)) {
    const hostName = 'ody-host';
    const root = `node_modules/${hostName}`;
    const relativePath = `${root}/${hostBinaryName()}`;
    const assetKey = hostBinaryAssetKey(target);
    const sourceBytes = await readFile(hostBinaryPath);
    manifestPackages.push({
      name: hostName,
      root,
      files: [{
        assetKey,
        relativePath,
        sha256: sha256(sourceBytes),
        mode: 0o755,
      }],
    });
    assets[assetKey] = hostBinaryPath;
    console.log(`Collected host binary: ${hostBinaryPath} -> ${assetKey}`);
  } else {
    console.warn(`Host binary not found at ${hostBinaryPath}; SEA will fall back to PATH lookup.`);
  }
```

Add `existsSync` import to `assets.mjs` if not already imported.

Modify `apps/ody-code/src/native/native-assets.ts` to expose a helper for resolving the extracted host binary path:

```typescript
export function getHostBinaryPath(options: NativeAssetOptions = {}): string | null {
  const packageRoot = getNativePackageRoot('ody-host', options);
  if (packageRoot === null) return null;
  return join(packageRoot, executableName(process.platform));
}

function executableName(platform: NodeJS.Platform): string {
  return platform === 'win32' ? 'ody-host.exe' : 'ody-host';
}
```

- [ ] Verify the SEA build still works and includes the asset:

```bash
pnpm run build:host
pnpm --filter ody-code run build:native:sea
```

Expected observation: build log contains `Collected host binary: ... -> host/darwin-arm64/ody-host` (target may vary).

- [ ] Run the native smoke test:

```bash
pnpm --filter ody-code run test:native:smoke
```

Expected: smoke passes; if `ody-host` was added to `SMOKE_PACKAGES`, it will be checked too.

- [ ] Commit:

```bash
git add apps/ody-code/scripts/native apps/ody-code/src/native/native-assets.ts
git commit -m "build(sea): evaluate bundling ody-host binary as native asset"
```

---

### Task D4: Update native asset smoke to verify `ody-host` availability

**Depends on:** Task D3

**Files:**
- Modify: `apps/ody-code/src/native/smoke.ts`
- Modify: `apps/ody-code/test/native/smoke.test.ts`

**Goal:** When SEA includes `ody-host`, the smoke test confirms it can be extracted.

- [ ] Write the failing test addition (`apps/ody-code/test/native/smoke.test.ts`):

Add to the existing test file:

```typescript
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
```

- [ ] Run it and verify it FAILS:

```bash
pnpm vitest run apps/ody-code/test/native/smoke.test.ts
```

Expected failure: the new test passes because `fakeManifest('ody-host')` removes `ody-host`, but `SMOKE_PACKAGES` does not yet include `ody-host`, so the assertion string mismatch.

- [ ] Write the minimal implementation.

Modify `apps/ody-code/src/native/smoke.ts`:

```typescript
export const SMOKE_PACKAGES = [
  '@mariozechner/clipboard',
  'koffi',
  '@odysseythink/ody-crypto',
  'ody-host',
];
```

- [ ] Run the test again:

```bash
pnpm vitest run apps/ody-code/test/native/smoke.test.ts
```

Expected: all tests pass, including the new `ody-host` missing-package case.

- [ ] Commit:

```bash
git add apps/ody-code/src/native/smoke.ts apps/ody-code/test/native/smoke.test.ts
git commit -m "test(native): include ody-host in native asset smoke"
```

---

### Task D5: Add CI job for Rust host smoke tests

**Depends on:** Tasks D1, D2

**Files:**
- Create: `.github/workflows/rust-host.yml`

**Goal:** Run `cargo test` and the cross-language TS connector test on every PR/push.

- [ ] Write the failing verification command:

```bash
act -j rust-host-smoke
```

Expected failure: `act` not installed or workflow file missing.

- [ ] Write the minimal implementation.

Create `.github/workflows/rust-host.yml`:

```yaml
name: Rust Host Smoke

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
  workflow_dispatch:

jobs:
  rust-host-smoke:
    name: rust-host-${{ matrix.target }}
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

      - name: Run Rust host unit tests
        run: pnpm run test:host
        shell: bash

      - name: Build Rust host release binary
        run: pnpm run build:host
        shell: bash

      - name: Run cross-language connector test
        run: pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts
        shell: bash

      - name: Upload ody-host artifact
        uses: actions/upload-artifact@v4
        with:
          name: ody-host-${{ matrix.target }}
          path: rust-ody/target/release/ody-host*
          if-no-files-found: error
```

- [ ] Validate the YAML syntax:

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/rust-host.yml'))" && echo "YAML OK"
```

Expected output: `YAML OK`.

- [ ] Commit:

```bash
git add .github/workflows/rust-host.yml
git commit -m "ci: add Rust host smoke job"
```

---

### Task D6: Write the Go/No-Go ADR

**Depends on:** Tasks D1–D5 (so that the evaluation facts are known)

**Files:**
- Create: `docs/designs/rust-host-reversal-adr.md`

**Goal:** Capture the decision, trade-offs, prototype results, and recommendation for whether to proceed with the Rust host reversal.

- [ ] Write the ADR.

Create `docs/designs/rust-host-reversal-adr.md`:

```markdown
# ADR: Rust Host Reversal Prototype

## Status

Proposed / Prototype Complete / Go or No-Go pending review.

## Context

The current TS Core worker hosts session runtime, LLM calls, and tool execution.
This ADR evaluates moving the host process to Rust (`ody-host`) while keeping the
TS TUI as the client.

## Decision

Prototype completed with the following scope:
- `ody-host` implements a subset of `CoreAPI` over stdio/socket length-prefixed RPC.
- Session persistence reuses the existing `SessionStore` directory layout.
- One OpenAI-compatible LLM provider and one bash tool with approval are implemented.
- TS TUI connects via `SDKRpcClient.connect` and `--host=rust`.

## Trade-offs

Pros:
- Faster startup and smaller runtime footprint than Node worker.
- Stronger control over concurrency and I/O.

Cons:
- Duplicates session/LLM/tool logic that currently lives in TS.
- OAuth and MCP remain out of scope in the prototype.
- SEA embedding increases binary size by ~XX MB per platform.

## Prototype Results

| Criterion | Result | Notes |
|---|---|---|
| `cargo test -p ody-host` | PASS/FAIL | (fill after D7) |
| Cross-language RPC test | PASS/FAIL | (fill after D7) |
| TUI stdio smoke | PASS/FAIL | (fill after D7) |
| TUI socket smoke | PASS/FAIL | (fill after D7) |
| SEA bundle size | MB | (fill after D3) |

## Recommendation

- **Go** if cross-language tests pass and the team accepts maintaining dual
  implementations for session/tool logic.
- **No-Go** if OAuth/MCP integration proves infeasible or binary size regressions
  are unacceptable.

## Consequences

If Go:
- Gradually migrate more CoreAPI methods to Rust.
- Keep TS TUI as the canonical client.
- Add platform matrix builds to release pipeline.

If No-Go:
- Retire `ody-host` crate or keep as experimental.
- Revert `--host=rust` CLI options behind an experimental flag.
```

- [ ] Verify the ADR renders correctly:

```bash
python3 -c "import pathlib; print(pathlib.Path('docs/designs/rust-host-reversal-adr.md').read_text()[:200])"
```

Expected output: the first 200 characters of the ADR.

- [ ] Commit:

```bash
git add docs/designs/rust-host-reversal-adr.md
git commit -m "docs: add Rust host reversal Go/No-Go ADR"
```

---

### Task D7: Run done-criteria verification

**Depends on:** All previous tasks in all parts.

**Files:** none (manual verification)

**Goal:** Confirm the prototype meets the must-pass done criteria from the design doc.

- [ ] Run each must-pass command and record the result in the ADR.

1. Rust host unit tests:

```bash
pnpm run test:host
```

Expected: `test result: ok. N passed; 0 failed; 0 ignored`.

2. Rust host release build:

```bash
pnpm run build:host
```

Expected: binary exists at `rust-ody/target/release/ody-host` and `--help` prints usage:

```bash
./rust-ody/target/release/ody-host --help
```

Expected output contains `Usage: ody-host` and options for `--stdio`, `--socket-path`, `--tcp-host`, `--tcp-port`, `--config`, `--home`.

3. Cross-language TS connector test:

```bash
pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts
```

Expected: all tests pass.

4. TUI stdio end-to-end:

```bash
pnpm run proto:rust-host
```

Expected: TUI starts; you can create a session, send a prompt, and observe an assistant response (or a clear tool/LLM error if no provider is configured).

5. TUI socket end-to-end:

```bash
SOCKET=$(mktemp -u)
pnpm -C apps/ody-code run dev:cli-only -- --host=rust --host-socket "$SOCKET" --host-binary $(pwd)/rust-ody/target/release/ody-host --home $(mktemp -d)
```

Expected: same behavior as stdio mode.

6. SEA smoke (optional):

```bash
pnpm run build:host
pnpm --filter ody-code run build:native:sea
pnpm --filter ody-code run test:native:smoke
```

Expected: smoke passes with `Native asset smoke passed: <target>` including `ody-host`.

- [ ] Update the ADR results table with the actual outcomes.

- [ ] If any must-pass criterion fails, file a follow-up issue or fix before closing the prototype.

- [ ] Generate a changeset for the prototype:

```bash
pnpm changeset
# select ody-code and any changed packages; use 'minor' for the prototype feature
```

- [ ] Commit:

```bash
git add docs/designs/rust-host-reversal-adr.md .changeset
git commit -m "docs: record prototype done-criteria results and changeset"
```

---

## Local Self-Review

- [ ] **1. Spec-coverage table**

| Design section | Requirement | Task(s) | Status |
|---|---|---|---|
| packaging.md §2.1 | Build outputs (`ody-host` binary, SEA blob) | D1, D2 | covered |
| packaging.md §2.2 | SEA asset manifest for `odyHost` | D3 | covered |
| packaging.md §2.3 | Host binary resolver | D3, D7 | covered |
| packaging.md §3.1 | Development build flow | D1, D2, D7 | covered |
| packaging.md §3.2 | `ody-host` CLI argument parsing | implemented in Part 2; verified in D7 | covered |
| packaging.md §3.3 | SEA embed & extract flow | D3, D4 | covered |
| packaging.md §3.4 | CI integration | D5 | covered |
| packaging.md §4.1-4.4 | Call-site integration (Cargo.toml, package.json, native assets) | D1-D4 | covered |
| packaging.md §5 | Error handling for build/SEA | D3, D7 | covered |
| packaging.md §6.1 | Must-pass done criteria commands | D7 | covered |
| packaging.md §6.2 | Optional evaluation criteria | D3, D7 | covered |
| index §In Scope 1 | ADR document | D6, D7 | covered |

- [ ] **2. Placeholder scan**: No `TODO`/`TBD`/deferred placeholders. The ADR template contains explicit `(fill after D7)` instructions, which are data-entry prompts for the executing engineer, not implementation placeholders.

- [ ] **3. No phantom tasks**: Each task produces a verifiable change (Cargo.toml, package.json scripts, SEA asset plumbing, CI YAML, ADR, verification run). Task D7 is a manual verification task with concrete commands and expected outputs.

- [ ] **4. Dependency soundness**: D1 has no internal prerequisite. D2 depends on D1. D3 depends on D2. D4 depends on D3. D5 depends on D1/D2. D6 depends on D1–D5. D7 depends on all previous parts and tasks.

- [ ] **5. Caller & build soundness**: The only shared-signature change in this part is adding `ody-host` to `SMOKE_PACKAGES` in D4, which updates the constant and its test in the same task. D3 extends native asset collection with a new asset key; the runtime consumer `getHostBinaryPath` is added in the same task.

- [ ] **6. Test-the-risk**: D4 adds a behavioral test asserting that missing `ody-host` from the manifest fails smoke. D7 lists concrete commands that assert the produced binary behaves correctly.

- [ ] **7. Type consistency**: Asset key naming (`host/${target}/ody-host`) is consistent between `host-binary.mjs` and `assets.mjs`. `SMOKE_PACKAGES` string `'ody-host'` matches the package name used in the manifest package entry.
