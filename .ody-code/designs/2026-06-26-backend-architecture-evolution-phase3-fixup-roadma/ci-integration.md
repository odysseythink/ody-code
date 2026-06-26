# Part 4 — CI Integration

## 1. Scope

This part rewrites `.github/workflows/rust-host.yml` so that the platform matrix runs the unified Phase A3 verification script. It covers:

- Replacing the scattered Rust-test / cross-lang-test / build-host steps with `pnpm run verify:phase-a3`. [C:USER]
- Preserving the existing matrix (darwin-arm64, darwin-x64, linux-x64). [C:USER]
- Uploading the JSON report, redacted logs, and the `ody-host` binary as artifacts. [C:USER]
- Failing the workflow if any verification step fails (no `continue-on-error`). [C:USER]

Out of scope:
- Adding new runner types (e.g. Windows). [C:USER]
- Notarization or release signing. [C:USER]

---

## 2. Data Models

No new data models beyond the `PhaseA3Report` defined in Part 1.

---

## 3. Workflow YAML

File: `.github/workflows/rust-host.yml`. [C:USER]

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

      - name: Phase A3 verification
        id: phase-a3
        run: pnpm run verify:phase-a3
        shell: bash
        env:
          ODY_HOST_BINARY_PATH: ${{ github.workspace }}/rust-ody/target/release/ody-host
          ODY_CODE_REPORT_DIR: ${{ github.workspace }}/.ody-code/reports

      - name: Upload Phase A3 report
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: phase-a3-report-${{ matrix.target }}
          path: |
            .ody-code/reports/phase-a3-report.json
            .ody-code/reports/phase-a3-logs/**
          if-no-files-found: ignore

      - name: Upload ody-host artifact
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: ody-host-${{ matrix.target }}
          path: rust-ody/target/release/ody-host*
          if-no-files-found: error
```

---

## 4. Algorithms

### 4.1 Verification script invocation in CI

```
workflow job
  install Node 24.15.0 (via .nvmrc)
  install pnpm 10.33.0
  install Rust stable
  pnpm install --frozen-lockfile
  pnpm run verify:phase-a3
    env:
      ODY_HOST_BINARY_PATH: <workspace>/rust-ody/target/release/ody-host
      ODY_CODE_REPORT_DIR: <workspace>/.ody-code/reports
```

### 4.2 Artifact upload order

1. Always upload the report and logs (`if: always()`), so failures are debuggable.
2. Always upload the `ody-host` binary (`if: always()`), so the build artifact is available even if smoke fails.

---

## 5. Call-Site Integration

### 5.1 Package script addition

File: `package.json` (root). [C:USER]

```json
"scripts": {
  "verify:phase-a3": "node scripts/verify-phase-a3.mjs",
  "verify:phase-a3:local": "node scripts/verify-phase-a3.mjs --keep-temp"
}
```

### 5.2 Verification script env handling

File: `scripts/verify-phase-a3.mjs` (Part 1). [C:INFERRED]

```ts
const reportDir = process.env.ODY_CODE_REPORT_DIR ?? join(workspaceRoot, '.ody-code', 'reports');
const hostBinaryPath = process.env.ODY_HOST_BINARY_PATH ?? join(workspaceRoot, 'rust-ody', 'target', 'release', 'ody-host');
```

---

## 6. Error Handling

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| Verification script exits non-zero | Workflow job fails. | Report artifact is still uploaded for debugging. | Fix failing step; rerun workflow. |
| Artifact upload fails | Workflow step fails, but earlier verification result dominates. | None. | Retry workflow or inspect runner storage. |
| Missing `ODY_HOST_BINARY_PATH` | Script falls back to default path; if missing, fails fast. | None. | Ensure `build:host` ran or set env var. |

---

## 7. Test Plan

| Test | Assertion |
|---|---|
| Workflow YAML is valid | `actionlint .github/workflows/rust-host.yml` exits 0. |
| `pnpm run verify:phase-a3` passes locally | Exits 0 and writes `.ody-code/reports/phase-a3-report.json`. |
| CI run on PR passes | All three matrix jobs green. |
| Failure artifact upload | Introduce a temporary failure; confirm report JSON is uploaded. |

Done criteria [C:USER]:
- `.github/workflows/rust-host.yml` invokes `pnpm run verify:phase-a3`.
- The workflow matrix covers darwin-arm64, darwin-x64, linux-x64.
- Failure of any verification step fails the job (no `continue-on-error`).
- Report and logs are uploaded on both success and failure.

---

## 8. Local Notes

- Keep `fail-fast: false` so that a failure on one platform does not cancel the others. [C:INFERRED]
- The workflow name may be updated to "Phase A3 Rust Host Verification" in a follow-up if the team prefers. [C:INFERRED]
- If SEA build is slow on `linux-x64`, consider increasing the verification script timeout for the `sea-build` step rather than splitting the workflow. [C:INFERRED]
