# Part D — CI Integration

> Scope: rewrite `.github/workflows/rust-host.yml` to run the unified Phase A3 verification script (`pnpm run verify:phase-a3`) across the existing platform matrix, and upload the report, logs, and `ody-host` binary as artifacts.

**Goal:** The Rust Host CI workflow invokes the Phase A3 verification script, preserves the darwin-arm64 / darwin-x64 / linux-x64 matrix, and uploads debugging artifacts on both success and failure.

**Architecture:** The workflow remains a single matrix job per platform. The scattered Rust-test / cross-lang-test / build-host steps are replaced by one `pnpm run verify:phase-a3` step, which internally orchestrates rust tests, cross-language RPC tests, TUI smoke tests, SEA build, and typecheck. Environment variables tell the script where to write reports and where to find the pre-built host binary. Artifacts are uploaded with `if: always()` so failures are debuggable.

**Tech Stack:** GitHub Actions, YAML, pnpm, Node.js, Rust toolchain.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## Dependency Overview

- **Phase D1** (prerequisite): confirm `package.json` has the `verify:phase-a3` script added in Part 1.
- **Phase D2**: rewrite `.github/workflows/rust-host.yml`.
- **Phase D3**: validate workflow YAML syntax and test the script locally.
- **Phase D4**: run a CI smoke test on the PR branch.

Task D1 and D2 are independent. Task D3 depends on D1 and D2. Task D4 depends on D3.

## Risks & Open Questions

- The verification script builds the Rust host binary itself; the workflow must either let the script build it or pre-build it. The design keeps the script self-contained, so the workflow only sets `ODY_HOST_BINARY_PATH` to the expected release path.
- `actionlint` may not be installed in the local environment; the plan includes an install fallback.
- Uploading `rust-ody/target/release/ody-host*` on Linux will also match `ody-host.d` if present; the existing glob is preserved.

---

### Task D1: Verify root `package.json` Phase A3 scripts

**Depends on:** Part 1 (`verification-script.md`: Task A8 added these scripts)

**Files:**
- Modify: `package.json` (root)

Confirm the root `package.json` contains the Phase A3 scripts. If they are missing (e.g. Part 1 was not yet executed), add them now.

- [ ] Read `package.json` and locate the `"scripts"` object.

- [ ] Ensure the following entries exist exactly:
  ```json
  {
    "scripts": {
      "verify:phase-a3": "node scripts/verify-phase-a3.mjs",
      "verify:phase-a3:local": "node scripts/verify-phase-a3.mjs --keep-temp"
    }
  }
  ```

- [ ] Manual verification:
  ```bash
  cd /Users/ranwei/workspace/ody-code && node --run verify:phase-a3 --help 2>&1 | head -20 || true
  ```
  Expected: the script prints its CLI help (or begins execution). If `node --run` is unavailable, use `pnpm run verify:phase-a3 --help`.

- [ ] Commit: `chore(ci): ensure verify:phase-a3 scripts are present`

---

### Task D2: Rewrite `.github/workflows/rust-host.yml`

**Depends on:** Task D1

**Files:**
- Modify: `.github/workflows/rust-host.yml:1-76`

Replace the entire existing workflow with the unified Phase A3 verification workflow. The matrix and triggers stay identical; only the job steps change.

- [ ] Overwrite `.github/workflows/rust-host.yml` with:
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

- [ ] Manual verification:
  ```bash
  cd /Users/ranwei/workspace/ody-code && cat .github/workflows/rust-host.yml | grep -E "verify:phase-a3|ODY_HOST_BINARY_PATH|ODY_CODE_REPORT_DIR|if: always"
  ```
  Expected output shows:
  - `run: pnpm run verify:phase-a3`
  - `ODY_HOST_BINARY_PATH:`
  - `ODY_CODE_REPORT_DIR:`
  - at least two occurrences of `if: always()`

- [ ] Commit: `ci: run unified phase-a3 verification in rust-host workflow`

---

### Task D3: Validate workflow YAML and local script run

**Depends on:** Task D1, Task D2

**Files:** none (verification only)

- [ ] Validate YAML syntax with `actionlint`:
  ```bash
  cd /Users/ranwei/workspace/ody-code
  if ! command -v actionlint &> /dev/null; then
    echo "actionlint not found; install via 'brew install actionlint' or download from https://github.com/rhysd/actionlint"
  fi
  actionlint .github/workflows/rust-host.yml
  ```
  Expected: `actionlint` exits 0 with no output. If `actionlint` is not installed, note the command used to install it and re-run.

- [ ] Run the verification script locally (this may take several minutes; skip on resource-constrained machines, but run at least the help):
  ```bash
  cd /Users/ranwei/workspace/ody-code
  ODY_CODE_REPORT_DIR=/Users/ranwei/workspace/ody-code/.ody-code/reports \
  ODY_HOST_BINARY_PATH=/Users/ranwei/workspace/ody-code/rust-ody/target/release/ody-host \
  pnpm run verify:phase-a3:local
  ```
  Expected: exits 0 and writes `/Users/ranwei/workspace/ody-code/.ody-code/reports/phase-a3-report.json`.

- [ ] Inspect the report:
  ```bash
  cd /Users/ranwei/workspace/ody-code && cat .ody-code/reports/phase-a3-report.json | jq '.success, .summary'
  ```
  Expected: `.success` is `true` and `.summary` lists all steps as passed.

- [ ] Commit: `chore(ci): validate phase-a3 workflow yaml and local run`

---

### Task D4: Run CI smoke test on PR branch

**Depends on:** Task D3

**Files:** none (CI validation only)

- [ ] Push the branch and open a draft PR against `main`.

- [ ] Verify the `Rust Host Smoke` workflow starts and runs three matrix jobs (`rust-host-darwin-arm64`, `rust-host-darwin-x64`, `rust-host-linux-x64`).

- [ ] Wait for all matrix jobs to complete.

- [ ] Manual verification:
  - All jobs are green on success.
  - Each job uploaded artifacts named `phase-a3-report-<target>` and `ody-host-<target>`.
  - Download one `phase-a3-report-<target>` artifact and confirm it contains `phase-a3-report.json` with `"success": true`.

- [ ] To verify failure artifact upload, temporarily introduce a failing step in `scripts/verify-phase-a3.mjs` (for example, make the `tui-smoke` step return a non-zero exit), push, confirm the job fails, and confirm the report/log artifact is still uploaded. Revert the temporary failure immediately afterward.

- [ ] Commit: `ci: confirm phase-a3 workflow passes in pull request`

---

## Self-Review (local to Part D)

- [ ] 1. **Spec-coverage table:**
  | Requirement | Task(s) | Status |
  |---|---|---|
  | Root `package.json` has `verify:phase-a3` script | D1 | covered |
  | Workflow invokes `pnpm run verify:phase-a3` | D2 | covered |
  | Matrix preserves darwin-arm64 / darwin-x64 / linux-x64 | D2 | covered |
  | Workflow fails if verification fails (no `continue-on-error`) | D2 | covered |
  | Report and logs uploaded on success and failure | D2 | covered |
  | `ody-host` binary uploaded on success and failure | D2 | covered |
  | YAML syntax validated | D3 | covered |
  | Local verification run passes | D3 | covered |
  | CI smoke test passes on PR | D4 | covered |
  | Failure artifact upload verified | D4 | covered |

- [ ] 2. **Placeholder scan:** no TODO/TBD; every task shows exact YAML, commands, and expected output.
- [ ] 3. **No phantom tasks:** every task produces a verifiable change or CI observation; no `--allow-empty` commits.
- [ ] 4. **Dependency soundness:** D3 depends on D1 and D2; D4 depends on D3; all other tasks depend on none.
- [ ] 5. **Caller & build soundness:** no shared signatures changed. The workflow only changes CI configuration and does not affect runtime consumers. Task D3 runs a local build/typecheck indirectly through the verification script.
- [ ] 6. **Test-the-risk:** risk is CI misconfiguration (workflow fails silently or artifacts missing). D4 includes a manual failure-injection test to confirm artifacts upload on failure.
- [ ] 7. **Type consistency:** no TypeScript types introduced or modified in this part.
