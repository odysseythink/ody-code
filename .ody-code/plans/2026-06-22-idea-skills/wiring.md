# Part 3: Wiring & Whole-Tree Verification

Scope: connect `SaveIdeaReportTool` and its auto-approve permission policy into the existing tool/permission systems, update the idea skill markdowns to call the new tool, and verify the entire workspace builds and tests pass.

---

## Task C7: Wire `SaveIdeaReportTool` into `tools/builtin/index.ts` and `ToolManager`

**Depends on:** `2026-06-22-idea-skills/core.md`: Task A1, Task A2  
**Files:**
- Modify: `packages/agent-core/src/tools/builtin/index.ts`
- Modify: `packages/agent-core/src/tools/tool-manager.ts`
- Test: `packages/agent-core/test/tools/idea/save-idea-report.test.ts` (already created in A2; this task runs it)

### Steps

- [ ] Open `packages/agent-core/src/tools/builtin/index.ts` and add the new re-export at the end of the existing list.

```typescript
// packages/agent-core/src/tools/builtin/index.ts
export * from './execute';
export * from './browser';
export * from './shell';
export * from './read-file';
export * from './write-file';
export * from './apply-edit';
export * from './patch';
export * from './search';
export * from './list-files';
export * from './glob';
export * from './grep';
export * from './fetch-url';
export * from './mcp';
export * from './url';
export * from './skill';
export * from './request-permission';
export * from './think';
export * from './idea/save-idea-report'; // ADD
```

- [ ] Open `packages/agent-core/src/tools/tool-manager.ts`. Locate `initializeBuiltinTools()` where instances of builtin tools are created. Add a `SaveIdeaReportTool` instance to the same array/map.

Before change:
```typescript
this.registerBuiltinTool(new RequestPermissionTool(this.permissionManager));
this.registerBuiltinTool(new ThinkTool());
```

After change:
```typescript
this.registerBuiltinTool(new RequestPermissionTool(this.permissionManager));
this.registerBuiltinTool(new ThinkTool());
this.registerBuiltinTool(new SaveIdeaReportTool(this.workspaceRoot)); // ADD
```

Ensure the constructor import is present:
```typescript
import {
  ExecuteTool,
  BrowserTool,
  ShellTool,
  ReadFileTool,
  WriteFileTool,
  ApplyEditTool,
  PatchTool,
  SearchTool,
  ListFilesTool,
  GlobTool,
  GrepTool,
  FetchURLTool,
  MCPTool,
  URLTool,
  SkillTool,
  RequestPermissionTool,
  ThinkTool,
  SaveIdeaReportTool, // ADD
} from './builtin';
```

- [ ] Add a runtime wiring test to `packages/agent-core/test/tools/tool-manager.test.ts` (create the file if it does not exist, otherwise append). The test asserts that after initialization the manager exposes the `SaveIdeaReport` tool under its exact name.

```typescript
// packages/agent-core/test/tools/tool-manager.test.ts
import { describe, it, expect, beforeEach } from 'vitest';
import { tmpdir } from 'node:os';
import { ToolManager } from '../../src/tools/tool-manager';

describe('ToolManager builtin wiring', () => {
  let manager: ToolManager;

  beforeEach(() => {
    manager = new ToolManager(tmpdir());
    manager.initializeBuiltinTools({} as any);
  });

  it('exposes SaveIdeaReportTool under its declared name', () => {
    const tool = manager.getTool('SaveIdeaReport');
    expect(tool).toBeDefined();
    expect(tool?.name).toBe('SaveIdeaReport');
  });
});
```

- [ ] Run the new wiring test and confirm it PASSES.

```bash
cd /Users/ranwei/workspace/ody-code && \
  pnpm --filter @odysseythink/agent-core exec vitest run test/tools/tool-manager.test.ts
```

Expected output (excerpt):
```
 ✓ test/tools/tool-manager.test.ts (1 test) 12ms
   ✓ ToolManager builtin wiring > exposes SaveIdeaReportTool under its declared name

Test Files  1 passed (1)
```

- [ ] Run the SaveIdeaReport tool tests from A2 to confirm the tool still passes after re-export.

```bash
cd /Users/ranwei/workspace/ody-code && \
  pnpm --filter @odysseythink/agent-core exec vitest run test/tools/idea/save-idea-report.test.ts
```

Expected: all tests in `save-idea-report.test.ts` pass.

- [ ] Commit.

```bash
cd /Users/ranwei/workspace/ody-code && \
  git add packages/agent-core/src/tools/builtin/index.ts \
          packages/agent-core/src/tools/tool-manager.ts \
          packages/agent-core/test/tools/tool-manager.test.ts && \
  git commit -m "feat(agent-core): wire SaveIdeaReportTool into ToolManager"
```

---

## Task C8: Register `IdeaToolDirectoryApprovePermissionPolicy`

**Depends on:** `2026-06-22-idea-skills/core.md`: Task A3  
**Files:**
- Modify: `packages/agent-core/src/agent/permission/policies/index.ts`
- Test: `packages/agent-core/test/agent/permission/policies/idea-tool-directory.test.ts` (already created in A3)

### Steps

- [ ] Open `packages/agent-core/src/agent/permission/policies/index.ts`. This file typically imports and registers all permission policies. Add the new policy import and instantiate/register it alongside the existing ones.

Example pattern (adjust to the actual surrounding code):
```typescript
// packages/agent-core/src/agent/permission/policies/index.ts
import { WriteFilePolicy } from './write-file';
import { ShellPolicy } from './shell';
import { BrowserPolicy } from './browser';
import { MCPPolicy } from './mcp';
import { URLPolicy } from './url';
import { IdeaToolDirectoryApprovePermissionPolicy } from './idea-tool-directory'; // ADD

export function createBuiltinPolicies(...deps: any[]) {
  return [
    new WriteFilePolicy(),
    new ShellPolicy(),
    new BrowserPolicy(),
    new MCPPolicy(),
    new URLPolicy(),
    new IdeaToolDirectoryApprovePermissionPolicy(), // ADD
  ];
}
```

If the file uses a different registration mechanism (e.g. direct export list), add the class to that list instead:
```typescript
export * from './write-file';
export * from './shell';
export * from './browser';
export * from './mcp';
export * from './url';
export * from './idea-tool-directory'; // ADD
```

- [ ] Verify the policy tests from A3 still pass.

```bash
cd /Users/ranwei/workspace/ody-code && \
  pnpm --filter @odysseythink/agent-core exec vitest run test/agent/permission/policies/idea-tool-directory.test.ts
```

Expected: all policy tests pass.

- [ ] Add an integration-style test that exercises the policy through the permission manager. Create or append to `packages/agent-core/test/agent/permission/policy-registration.test.ts`:

```typescript
// packages/agent-core/test/agent/permission/policy-registration.test.ts
import { describe, it, expect } from 'vitest';
import { createBuiltinPolicies } from '../../../src/agent/permission/policies';
import { IdeaToolDirectoryApprovePermissionPolicy } from '../../../src/agent/permission/policies/idea-tool-directory';

describe('builtin policy registration', () => {
  it('includes the idea-tool-directory auto-approve policy', () => {
    const policies = createBuiltinPolicies();
    const found = policies.some(p => p instanceof IdeaToolDirectoryApprovePermissionPolicy);
    expect(found).toBe(true);
  });
});
```

If `createBuiltinPolicies` does not exist, replace the assertion with a test that imports `IdeaToolDirectoryApprovePermissionPolicy` from the barrel and instantiates it without error, plus a snapshot of the exported policy list.

- [ ] Run the registration test.

```bash
cd /Users/ranwei/workspace/ody-code && \
  pnpm --filter @odysseythink/agent-core exec vitest run test/agent/permission/policy-registration.test.ts
```

Expected output (excerpt):
```
 ✓ test/agent/permission/policy-registration.test.ts (1 test)
   ✓ builtin policy registration > includes the idea-tool-directory auto-approve policy
```

- [ ] Commit.

```bash
cd /Users/ranwei/workspace/ody-code && \
  git add packages/agent-core/src/agent/permission/policies/index.ts \
          packages/agent-core/test/agent/permission/policy-registration.test.ts && \
  git commit -m "feat(agent-core): register idea-tool-directory permission policy"
```

---

## Task C9: Update Skill Markdowns to Call `SaveIdeaReport`

**Depends on:** `2026-06-22-idea-skills/skills.md`: Task B4, Task B5  
**Files:**
- Modify: `packages/agent-core/src/skill/builtin/idea-generator.md`
- Modify: `packages/agent-core/src/skill/builtin/idea-evaluator.md`

### Steps

- [ ] Open `packages/agent-core/src/skill/builtin/idea-generator.md`. Find the final step that currently instructs the model to write the report to `.ody-code/ideas/`. Replace any raw `write_file` / file-writing instruction with a call to the `SaveIdeaReport` tool.

Example final step (preserve the existing content but update the tool call):
```markdown
## Final step
After completing the analysis, call the tool `SaveIdeaReport` with:
- `title`: a concise, filesystem-safe title for the idea
- `content`: the full Markdown report content (include `#` headings, problem, alternatives, recommendation, next steps)

Do not write the file directly; always use `SaveIdeaReport` so the output is validated and stored under `.ody-code/ideas/`.
```

- [ ] Open `packages/agent-core/src/skill/builtin/idea-evaluator.md`. Apply the same change in the final step.

```markdown
## Final step
After completing the evaluation, call the tool `SaveIdeaReport` with:
- `title`: a concise, filesystem-safe title for the evaluation
- `content`: the full Markdown evaluation content (include `#` headings, criteria, scores, summary)

Do not write the file directly; always use `SaveIdeaReport` so the output is validated and stored under `.ody-code/ideas/`.
```

- [ ] Verify both skill markdowns still load without syntax errors by running the skill tests from B6.

```bash
cd /Users/ranwei/workspace/ody-code && \
  pnpm --filter @odysseythink/agent-core exec vitest run test/skill/builtin-skills.test.ts
```

Expected: `builtin-skills.test.ts` passes and asserts that both `idea-generator` and `idea-evaluator` are present in `BUILTIN_SKILLS`.

- [ ] Commit.

```bash
cd /Users/ranwei/workspace/ody-code && \
  git add packages/agent-core/src/skill/builtin/idea-generator.md \
          packages/agent-core/src/skill/builtin/idea-evaluator.md && \
  git commit -m "docs(agent-core): route idea skill outputs through SaveIdeaReport"
```

---

## Task C10: Whole-Tree Typecheck, Lint, Build, and Test

**Depends on:** Task C7, Task C8, Task C9  
**Files:**
- All files touched in Tasks C7–C9
- Verify: no new files created; only build/test commands run

### Steps

- [ ] Run the agent-core test suite to confirm no regressions.

```bash
cd /Users/ranwei/workspace/ody-code && \
  pnpm --filter @odysseythink/agent-core exec vitest run
```

Expected output (excerpt):
```
Test Files  N passed (N)
Tests       M passed (M)
```

If any test fails, fix the underlying issue (implementation or test) before proceeding. Do not adjust unrelated tests.

- [ ] Run whole-tree typecheck.

```bash
cd /Users/ranwei/workspace/ody-code && \
  pnpm -r typecheck
```

Expected: `pnpm -r typecheck` exits with code 0 and no `tsc` errors in any workspace package. If a shared-signature change surfaces stale callers in test files, fix them in this task.

- [ ] Run lint for the affected package.

```bash
cd /Users/ranwei/workspace/ody-code && \
  pnpm --filter @odysseythink/agent-core lint
```

Expected: lint exits with code 0. Address any new warnings introduced by the changed code.

- [ ] Build the affected package.

```bash
cd /Users/ranwei/workspace/ody-code && \
  pnpm --filter @odysseythink/agent-core build
```

Expected: build completes without errors and the output directory (`packages/agent-core/dist/` or equivalent) contains the compiled `tools/builtin/idea/save-idea-report.*`, `agent/permission/policies/idea-tool-directory.*`, and skill files.

- [ ] Manual end-to-end verification. In a temporary workspace, run the CLI with the `idea-generator` skill (or a small Node harness) and confirm:
  1. The skill can be invoked.
  2. When the LLM emits a `SaveIdeaReport` tool call, the file appears under `.ody-code/ideas/`.
  3. The permission policy auto-approves the write (no interactive prompt).

Example harness (`/tmp/verify-idea-skills.mjs`, delete after use):
```javascript
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const workspace = join(tmpdir(), 'idea-skill-verify-' + Date.now());

// Use the built SDK / agent-core entry points. Adjust paths to the actual exports.
const { SaveIdeaReportTool } = await import(
  '/Users/ranwei/workspace/ody-code/packages/agent-core/dist/tools/builtin/idea/save-idea-report.js'
);
const { IdeaToolDirectoryApprovePermissionPolicy } = await import(
  '/Users/ranwei/workspace/ody-code/packages/agent-core/dist/agent/permission/policies/idea-tool-directory.js'
);

const tool = new SaveIdeaReportTool(workspace);
const result = await tool.execute({ title: 'Verify Wiring', content: '# Verify\nOK' });
console.log(result);

const policy = new IdeaToolDirectoryApprovePermissionPolicy();
const decision = policy.evaluate?.({
  tool: 'WriteFile',
  args: { path: join(workspace, '.ody-code/ideas/verify.md') },
});
console.log('decision:', decision);
```

Run it:
```bash
node /tmp/verify-idea-skills.mjs
```

Expected observation:
- `result` contains a path ending in `.ody-code/ideas/verify-wiring.md` (or similar sanitized filename) and status `success`.
- `decision` is `approve` or `{ status: 'approve' }` (match the policy's return shape).

- [ ] Clean up the temporary harness file.

```bash
rm /tmp/verify-idea-skills.mjs
```

- [ ] Commit.

```bash
cd /Users/ranwei/workspace/ody-code && \
  git commit --allow-empty -m "chore(agent-core): verify idea skills end-to-end"
```

(Empty commit is acceptable here because the verification produced only transient artifacts; record the passing state in the commit message.)

---

## Part 3 Local Self-Review

- [ ] 1. **Spec-coverage table (Part 3 scope)**

| Requirement | Task | Status |
|---|---|---|
| `SaveIdeaReportTool` exported from tools barrel | C7 | covered |
| `SaveIdeaReportTool` instantiated in `ToolManager` | C7 | covered |
| `IdeaToolDirectoryApprovePermissionPolicy` registered | C8 | covered |
| Skill markdowns route output through `SaveIdeaReport` | C9 | covered |
| Whole-tree typecheck/build/test pass | C10 | covered |

- [ ] 2. **Placeholder scan**: no TODO/TBD in wiring.md.
- [ ] 3. **No phantom tasks**: C10 produces the final verification commit.
- [ ] 4. **Dependency soundness**: C7 depends on A1/A2; C8 depends on A3; C9 depends on B4/B5; C10 depends on C7/C8/C9.
- [ ] 5. **Caller & build soundness**: C7 updates the `ToolManager` caller list and re-exports the tool; C8 updates the policy registration barrel; C10 runs `pnpm -r typecheck`. The `SaveIdeaReportTool` constructor signature used in C7 matches A2's definition; the policy registration in C8 matches A3's class.
- [ ] 6. **Test-the-risk**: C7 tests that `ToolManager.getTool('SaveIdeaReport')` returns the tool; C8 tests that the policy is registered; C10 runs full tests.
- [ ] 7. **Type consistency**: `SaveIdeaReportTool` is imported from `./builtin` using the name defined in A2; `IdeaToolDirectoryApprovePermissionPolicy` is imported from `./idea-tool-directory` using the name defined in A3.
