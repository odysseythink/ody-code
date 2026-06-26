# Phase D: Verify — typecheck and test suite

**Scope:** Run whole-tree typecheck and full test suite to confirm the rollback is complete and correct.

**Depends on:** Phase C (all browser code removed, deps cleaned up)

## Task D1: Run typecheck and tests

**Files:** none

Steps:

- [ ] Run whole-tree typecheck:

```bash
source ~/.nvm/nvm.sh && nvm use 24.16.0 && cd /Users/ranwei/workspace/ody-code && pnpm -r typecheck 2>&1
```

Expected: Only the 4 pre-existing `packages/oauth/test/provider-login.test.ts` errors (unrelated). No errors from `packages/agent-core`.

- [ ] Run full test suite for agent-core:

```bash
source ~/.nvm/nvm.sh && nvm use 24.16.0 && cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm test 2>&1
```

Expected: All tests pass. The test count should match the original ace5ba1 baseline (~2600 tests minus browser tests).

- [ ] Verify no browser files remain:

```bash
find packages/agent-core/src -name "*browser*" -not -path "*/mcp/*-devtools*" | head
# Expected: no output (chrome-devtools files are NOT browser-native and should remain)
find packages/agent-core/test -name "*browser*" -not -path "*/mcp/*-devtools*" | head
# Expected: no output
```

- [ ] Verify no browser-related imports remain:

```bash
grep -r "BrowserConnection\|BrowserBrowse\|BrowserExtract\|BrowserAct\|BrowserNavigate\|BrowserSnapshot\|BrowserClick\|BrowserFill\|BrowserEvaluate\|BrowserScreenshot" packages/agent-core/src 2>/dev/null
# Expected: no output
```

- [ ] Run native build to verify it still works:

```bash
source ~/.nvm/nvm.sh && nvm use 24.16.0 && cd /Users/ranwei/workspace/ody-code && pnpm -C apps/ody-code run build:native:sea 2>&1
```

Expected: Build succeeds without proxy-agent or eval errors.

- [ ] Final commit with a summary message:

```bash
git add -A && git commit -m "revert: native browser tools feature - full rollback"
```

## Phase D Self-Review

- [ ] 1. Spec-coverage: D1 verifies all previous phases produced correct state. ✓
- [ ] 2. Placeholder scan: no TODO/TBD. ✓
- [ ] 3. No phantom tasks: D1 is a verification-only step. ✓
- [ ] 4. Dependency soundness: depends on Phase C. ✓
- [ ] 5. Caller & build soundness: typecheck validates cross-file correctness. ✓
- [ ] 6. Test-the-risk: Running the full test suite verifies behavioral correctness after rollback. ✓
- [ ] 7. Type consistency: typecheck validates all types are consistent. ✓
