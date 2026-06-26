# Resume Session Model from Config.toml — Implementation Plan

**Goal:** Replace the session-record fallback in `refreshSessionRuntimeConfig` with config.toml as the sole model source for resumed sessions.

**Architecture:** Single-function rewrite in `packages/agent-core/src/rpc/core-impl.ts:813-847`. Delete the existing try-requested-then-fallback loop (~30 lines), replace with a mode-aware config lookup. No shared-signature changes, no new imports.

**Tech Stack:** TypeScript, pnpm workspace.

> For executing workers: implement this plan task-by-task. Steps use `- [ ]` checkboxes for tracking.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `packages/agent-core/src/rpc/core-impl.ts:813-847` | Modify | Replace `refreshSessionRuntimeConfig` |

## Dependency Overview

```
Task 1 ──→ Task 2
(rewrite     (typecheck
 function)    + build)
```

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| `session.requireMainAgent()` throws if main agent doesn't exist | Guard: design verified this call is safe post-`session.resume()` |
| `main.sessionMode.kind` returns wrong mode after resume | Design assumption #2; verified by test/session-mode.test.ts |

---

### Task 1: Rewrite `refreshSessionRuntimeConfig`

**Depends on:** none

**Files:** Modify `packages/agent-core/src/rpc/core-impl.ts:813-847`

- [ ] **Write the new implementation.** Replace the entire function body:

```ts
  private async refreshSessionRuntimeConfig(
    session: Session,
    config: KimiConfig,
  ): Promise<void> {
    const main = session.requireMainAgent();
    const currentMode = main.sessionMode.isActive ? main.sessionMode.kind : 'normal';
    const fromConfig =
      currentMode === 'plan'
        ? config.modeModels?.plan
        : currentMode === 'design'
          ? config.modeModels?.design
          : config.defaultModel;
    const model = fromConfig?.trim() ?? '';
    if (model.length === 0) return;
    const api = new SessionAPIImpl(session);
    await api.setModel({ agentId: 'main', model });
    await session.flushMetadata();
  }
```

Delete everything from the old function: the `const requested`, `const fallback`, `const candidates`, and the entire for/try/catch loop with its migration comment block.

- [ ] **Build check:** `pnpm --filter agent-core typecheck` — verify exit 0, no type errors.
- [ ] **Manual verification:**
  1. Verify `SessionAPIImpl` import still exists (it was already imported for the old code).
  2. Run `pnpm --filter agent-core test -- --run test/agent/resume.test.ts` to confirm resume tests pass.
  3. Run `pnpm --filter agent-core test -- --run test/harness/model-alias-session.test.ts` to confirm model alias tests pass.
- [ ] **Commit:** `git add packages/agent-core/src/rpc/core-impl.ts && git commit -m "fix: load resumed session model from config.toml"`

---

### Task 2: Whole-workspace typecheck

**Depends on:** Task 1

- [ ] **Run full typecheck:**
  ```bash
  source ~/.nvm/nvm.sh && nvm use 24 > /dev/null 2>&1 && cd /Users/ranwei/workspace/ody-code && pnpm run build:packages && pnpm --filter agent-core run typecheck && pnpm --filter ody-code run typecheck
  ```
  Expected: exit 0. agent-core and ody-code typecheck clean (pre-existing oauth test errors unrelated).
- [ ] **Run build:**
  ```bash
  pnpm --filter ody-code build:native:sea
  ```
  Expected: exit 0. Native binary rebuilt with the fix.

---

## Self-Review

- [ ] 1. **Spec-coverage table:**

| Design Section | Task(s) | Status |
|---|---|---|
| 算法实现 (步骤 1-8) | Task 1 | covered |
| 删除原有 fallback 循环 | Task 1 | covered |
| 全 workspace typecheck | Task 2 | covered |
| 构建 native binary | Task 2 | covered |

- [ ] 2. **Placeholder scan:** No TODO/TBD anywhere. All code shown verbatim.
- [ ] 3. **No phantom tasks:** 2 real tasks, both produce verifiable changes.
- [ ] 4. **Dependency soundness:** Task 2 depends on Task 1 (needs the code change first).
- [ ] 5. **Caller & build soundness:** No shared signatures changed. `refreshSessionRuntimeConfig` is private, called only at line 339 in the same file. Both agent-core and ody-code typecheck in Task 2.
- [ ] 6. **Test-the-risk:** The risk is resume flow breakage. Existing `test/agent/resume.test.ts` and `test/harness/model-alias-session.test.ts` cover resume paths. Manual verification step runs these tests.
- [ ] 7. **Type consistency:** All types (`Session`, `KimiConfig`, `SessionAPIImpl`) are pre-existing. No new types defined.
