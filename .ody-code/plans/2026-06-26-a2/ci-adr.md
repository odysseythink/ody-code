# Phase A2 — CI / ADR Part: Strict Cross-Language Test and Documentation

**Scope:** Make the cross-language test step in `.github/workflows/rust-host.yml` strict (remove `continue-on-error`) and upload failure logs. Update `docs/designs/rust-host-reversal-adr.md` to record the new test result and document known limitations.

**Prerequisite:** Phase B cross-language tests exist and pass locally.

## Task C1: Make the CI cross-language test strict and upload failure logs

**Depends on:** Task B3

**Files:**
- Modify: `.github/workflows/rust-host.yml:57-69`

### Steps

- [ ] Write the complete updated workflow code.

  Replace the cross-language test step block (lines 57–69) with:

  ```yaml
        # Cross-language connector test (requires packages/node-sdk/test/rust-host-connect.test.ts).
        - name: Run cross-language connector test
          run: pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts
          shell: bash

        - name: Upload cross-language failure logs
          if: failure()
          uses: actions/upload-artifact@v4
          with:
            name: rust-host-cross-lang-logs-${{ matrix.target }}
            path: |
              /tmp/ody-rust-host-*/**
              rust-ody/target/release/ody-host
  ```

  The rest of the file stays unchanged.

- [ ] Run the manual verification step.

  Validate YAML syntax with Python:

  ```bash
  python3 -c "import yaml; yaml.safe_load(open('.github/workflows/rust-host.yml')); print('yaml ok')"
  ```

  Expected output:

  ```text
  yaml ok
  ```

- [ ] Commit.

  ```bash
  git add .github/workflows/rust-host.yml
  git commit -m "ci(rust-host): enforce cross-language test and upload failure logs"
  ```

## Task C2: Update the Rust host reversal ADR

**Depends on:** Task B3

**Files:**
- Modify: `docs/designs/rust-host-reversal-adr.md:37` (Cross-language RPC test row)
- Modify: `docs/designs/rust-host-reversal-adr.md` after line 51 (add Known Limitations section)

### Steps

- [ ] Write the complete updated ADR content.

  Change the Cross-language RPC test row from:

  ```markdown
  | Cross-language RPC test | N/A | `packages/node-sdk/test/rust-host-connect.test.ts` not yet created |
  ```

  to:

  ```markdown
  | Cross-language RPC test | PASS | `pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts` passes on macOS + Linux |
  ```

  Add the following section immediately after the Build & Packaging table (after line 51, before `## Recommendation`):

  ```markdown
  ### A2 Known Limitations

  - `createSession` now supports an optional `id` field; when omitted the host generates a UUID v7 session id.
  - The TCP transport test uses a fixed port range (`19090–19099`) with `EADDRINUSE` retry logic. If the CI runner exhausts this range the test fails and the range must be widened or replaced with dynamic port allocation.
  - Cross-language tests cover only session lifecycle RPC (`createSession`, `listSessions`, `closeSession`). They do not exercise LLM/chat paths because no API key is provided in CI.
  - UDS socket paths are created inside a per-test temp directory. On platforms with short `sun_path` limits (e.g., macOS ~104 bytes) an unusually long `TMPDIR` can cause `ENAMETOOLONG`.
  - Each test case starts a fresh `ody-host` process with an isolated `homeDir`, so no persistent session state is left behind after `client.close()` and temp cleanup.
  ```

- [ ] Run the manual verification step.

  Confirm the updated row and new section are present:

  ```bash
  grep -A1 "Cross-language RPC test" docs/designs/rust-host-reversal-adr.md
  grep -A6 "A2 Known Limitations" docs/designs/rust-host-reversal-adr.md
  ```

  Expected output contains:

  ```text
  | Cross-language RPC test | PASS | `pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts` passes on macOS + Linux |
  ### A2 Known Limitations
  - `createSession` now supports an optional `id` field
  ```

- [ ] Commit.

  ```bash
  git add docs/designs/rust-host-reversal-adr.md
  git commit -m "docs(adr): record A2 cross-language test results and limitations"
  ```

## Local Self-Review

- [ ] 1. Spec coverage: CI strictness + log upload (C1), ADR results update + Known Limitations (C2) — both covered.
- [ ] 2. Placeholder scan: no TODO/TBD; all YAML and Markdown blocks are complete.
- [ ] 3. No phantom tasks: C1 modifies CI YAML and validates syntax; C2 modifies ADR and verifies content with grep.
- [ ] 4. Dependency soundness: C1 and C2 depend only on B3 (cross-language tests exist and pass).
- [ ] 5. Caller & build soundness: no code signatures are changed. The workflow step name and artifact path match the existing matrix variable `${{ matrix.target }}`.
- [ ] 6. Test-the-risk: the CI change is wiring, not state mutation; the risk (test failures masked by `continue-on-error`) is mitigated by removing the flag and uploading logs for post-failure inspection.
- [ ] 7. Type consistency: not applicable for YAML/Markdown; no new types introduced.
