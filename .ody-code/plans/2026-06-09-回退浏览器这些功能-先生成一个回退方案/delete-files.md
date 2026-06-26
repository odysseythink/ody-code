# Phase A: Delete new browser files

**Scope:** Delete all files that were created exclusively for the native browser feature. These files have no non-browser content and can be safely removed.

**Depends on:** none

## Task A1: Delete browser source and support files

**Files:** DELETE the following:

```
packages/agent-core/src/browser/connection.ts
packages/agent-core/src/browser/index.ts
packages/agent-core/src/browser/types.ts
packages/agent-core/src/tools/builtin/browser/_utils.ts
packages/agent-core/src/tools/builtin/browser/act.ts
packages/agent-core/src/tools/builtin/browser/browse.ts
packages/agent-core/src/tools/builtin/browser/click.ts
packages/agent-core/src/tools/builtin/browser/evaluate.ts
packages/agent-core/src/tools/builtin/browser/extract.ts
packages/agent-core/src/tools/builtin/browser/fill.ts
packages/agent-core/src/tools/builtin/browser/index.ts
packages/agent-core/src/tools/builtin/browser/navigate.ts
packages/agent-core/src/tools/builtin/browser/screenshot.ts
packages/agent-core/src/tools/builtin/browser/snapshot.ts
packages/agent-core/src/agent/permission/policies/browser-host.ts
packages/agent-core/src/tools/support/browser-rule-match.ts
```

Steps:

- [ ] Delete the files:

```bash
cd /Users/ranwei/workspace/ody-code
rm packages/agent-core/src/browser/connection.ts
rm packages/agent-core/src/browser/index.ts
rm packages/agent-core/src/browser/types.ts
rm packages/agent-core/src/tools/builtin/browser/_utils.ts
rm packages/agent-core/src/tools/builtin/browser/act.ts
rm packages/agent-core/src/tools/builtin/browser/browse.ts
rm packages/agent-core/src/tools/builtin/browser/click.ts
rm packages/agent-core/src/tools/builtin/browser/evaluate.ts
rm packages/agent-core/src/tools/builtin/browser/extract.ts
rm packages/agent-core/src/tools/builtin/browser/fill.ts
rm packages/agent-core/src/tools/builtin/browser/index.ts
rm packages/agent-core/src/tools/builtin/browser/navigate.ts
rm packages/agent-core/src/tools/builtin/browser/screenshot.ts
rm packages/agent-core/src/tools/builtin/browser/snapshot.ts
rm packages/agent-core/src/agent/permission/policies/browser-host.ts
rm packages/agent-core/src/tools/support/browser-rule-match.ts
```

- [ ] Remove empty directories:

```bash
rmdir packages/agent-core/src/browser 2>/dev/null || true
rmdir packages/agent-core/src/tools/builtin/browser 2>/dev/null || true
```

- [ ] Verify files are gone:

```bash
ls packages/agent-core/src/browser/connection.ts 2>&1 | grep "No such file"
ls packages/agent-core/src/agent/permission/policies/browser-host.ts 2>&1 | grep "No such file"
```

- [ ] Commit:

```bash
git add -A && git commit -m "revert(browser): remove all new browser source files"
```

## Task A2: Delete browser test files

**Files:** DELETE the following:

```
packages/agent-core/test/browser/connection.test.ts
packages/agent-core/test/browser/tools.test.ts
packages/agent-core/test/agent/permission/browser-host.test.ts
packages/agent-core/test/config/browser-config.test.ts
```

Steps:

- [ ] Delete the files:

```bash
cd /Users/ranwei/workspace/ody-code
rm packages/agent-core/test/browser/connection.test.ts
rm packages/agent-core/test/browser/tools.test.ts
rm packages/agent-core/test/agent/permission/browser-host.test.ts
rm packages/agent-core/test/config/browser-config.test.ts
```

- [ ] Remove empty directory:

```bash
rmdir packages/agent-core/test/browser 2>/dev/null || true
```

- [ ] Verify files are gone:

```bash
ls packages/agent-core/test/browser/tools.test.ts 2>&1 | grep "No such file"
ls packages/agent-core/test/agent/permission/browser-host.test.ts 2>&1 | grep "No such file"
```

- [ ] Commit:

```bash
git add -A && git commit -m "revert(browser): remove all new browser test files"
```

## Phase A Self-Review

- [ ] 1. Spec-coverage: delete-files covers all new browser source + test files. ✓
- [ ] 2. Placeholder scan: no TODO/TBD, all commands are concrete. ✓
- [ ] 3. No phantom tasks: both tasks produce actual file deletions. ✗ — Phase A has 2 tasks (A1, A2), both meaningful.
- [ ] 4. Dependency soundness: no inter-task dependencies. ✓
- [ ] 5. Caller & build soundness: deletion-only — no signature changes, later phases will handle the imports that reference these deleted files. At THIS point, typecheck WILL FAIL (expected — imports in agent/ etc. reference deleted files). This is resolved in Phase B. ✓
- [ ] 6. Test-the-risk: deletion is a pure file removal; verification is `ls` confirms absence. ✓
- [ ] 7. Type consistency: no types changed, only files removed. ✓
