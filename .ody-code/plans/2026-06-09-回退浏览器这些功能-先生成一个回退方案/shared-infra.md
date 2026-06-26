# Phase B: Revert shared infrastructure files

**Scope:** Revert browser-related changes to shared files (Agent, ToolManager, Session, config, permission policies, MCP registry, builtin tools index).

**Depends on:** Phase A (delete new browser files — imports to deleted files must be removed)

## Task B1: Revert Agent index.ts — remove browserConnection

**Depends on:** Phase A
**Files:** Modify `packages/agent-core/src/agent/index.ts`

Steps:

- [ ] Remove the `BrowserConnectionManager` import (line 67):

```typescript
// Delete this line:
import { BrowserConnectionManager } from '../browser';
```

- [ ] Remove the `browserConnection` field (line 132):

```typescript
// Delete this line:
readonly browserConnection?: BrowserConnectionManager;
```

- [ ] Remove the browserConnection initialization from constructor (lines 180-187):

```typescript
// Delete these lines:
if (this.config.hasProvider && this.kimiConfig?.browser?.enabled !== false) {
  this.browserConnection = new BrowserConnectionManager({
    chromePort: this.kimiConfig?.browser?.chromePort,
    autoLaunch: this.kimiConfig?.browser?.autoLaunch,
    headless: this.kimiConfig?.browser?.headless,
    executablePath: this.kimiConfig?.browser?.executablePath,
    log: this.log,
  });
}
```

- [ ] Verify the import was removed:

```bash
grep "BrowserConnectionManager" packages/agent-core/src/agent/index.ts
# Expected: no output
```

- [ ] Commit:

```bash
git add packages/agent-core/src/agent/index.ts && git commit -m "revert(agent): remove browserConnection from Agent class"
```

## Task B2: Revert ToolManager — remove browser tool registrations

**Depends on:** Phase A
**Files:** Modify `packages/agent-core/src/agent/tool/index.ts` lines ~458-467

Steps:

- [ ] Remove the 9 browser tool registration lines from `initializeBuiltinTools()`:

```typescript
// Delete these lines (the 9 browser tool entries that use this.agent.browserConnection):
this.agent.browserConnection && new b.BrowserBrowseTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserExtractTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserActTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserNavigateTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserSnapshotTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserClickTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserFillTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserEvaluateTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserScreenshotTool(this.agent.browserConnection),
```

- [ ] Verify browser tools no longer referenced:

```bash
grep "Browser\w*Tool\|browserConnection" packages/agent-core/src/agent/tool/index.ts
# Expected: no output
```

- [ ] Commit:

```bash
git add packages/agent-core/src/agent/tool/index.ts && git commit -m "revert(tool): remove browser tool registrations from ToolManager"
```

## Task B3: Revert permission/policies/index.ts — remove BrowserHostPermissionPolicy

**Depends on:** Phase A
**Files:** Modify `packages/agent-core/src/agent/permission/policies/index.ts`

Steps:

- [ ] Remove the BrowserHostPermissionPolicy import:

```typescript
// Delete this line:
import { BrowserHostPermissionPolicy } from './browser-host';
```

- [ ] Remove the BrowserHostPermissionPolicy from the chain, and restore the original comment:
  Delete these lines:
  ```
  // Browser automation: per-host authorization
  new BrowserHostPermissionPolicy(agent),
  ```

- [ ] Verify import and usage removed:

```bash
grep "BrowserHost" packages/agent-core/src/agent/permission/policies/index.ts
# Expected: no output
```

- [ ] Commit:

```bash
git add packages/agent-core/src/agent/permission/policies/index.ts && git commit -m "revert(permission): remove BrowserHostPermissionPolicy from chain"
```

## Task B4: Revert permission/policies/browser-tool-ask.ts — remove Browser* matching

**Depends on:** Phase A
**Files:** Modify `packages/agent-core/src/agent/permission/policies/browser-tool-ask.ts`

Steps:

- [ ] Change line 12 to remove the `Browser` prefix check. Replace:

```typescript
if (!name.startsWith('mcp__chrome-devtools__') && !name.startsWith('Browser')) return;
```

with the original:

```typescript
if (!name.startsWith('mcp__chrome-devtools__')) return;
```

- [ ] Verify the line content:

```bash
grep "name\.startsWith" packages/agent-core/src/agent/permission/policies/browser-tool-ask.ts
# Expected: if (!context.toolCall.name.startsWith('mcp__chrome-devtools__')) return;
```

- [ ] Commit:

```bash
git add packages/agent-core/src/agent/permission/policies/browser-tool-ask.ts && git commit -m "revert(permission): remove Browser* matching from BrowserToolAskPermissionPolicy"
```

## Task B5: Revert config/schema.ts — remove browser config additions

**Depends on:** Phase A
**Files:** Modify `packages/agent-core/src/config/schema.ts`

Steps:

- [ ] Remove the 4 new fields from BrowserConfigSchema. Replace lines 191-196:

```typescript
  autoLaunch: z.boolean().optional(),
  headless: z.boolean().optional(),
  executablePath: z.string().optional(),
  legacyMcpEnabled: z.boolean().optional(),
  allowedHosts: z.array(z.string()).optional(),
  sensitivePatterns: z.array(z.string()).optional(),
```

Delete these 6 lines, keeping only:

```typescript
export const BrowserConfigSchema = z.object({
  enabled: z.boolean().optional(),
  chromePort: z.number().int().min(1).max(65535).optional(),
  traceEnabled: z.boolean().optional(),
  traceRetentionDays: z.number().int().min(1).optional(),
});
```

- [ ] Verify the schema:

```bash
grep -A 10 "BrowserConfigSchema" packages/agent-core/src/config/schema.ts
# Expected output shows only: enabled, chromePort, traceEnabled, traceRetentionDays
```

- [ ] Commit:

```bash
git add packages/agent-core/src/config/schema.ts && git commit -m "revert(config): remove browser config schema additions"
```

## Task B6: Revert session/index.ts — remove browser cleanup

**Depends on:** Phase A
**Files:** Modify `packages/agent-core/src/session/index.ts`

Steps:

- [ ] Remove the `closeBrowserConnections()` call from `close()`. Replace:

```typescript
finally {
  try {
    await this.closeBrowserConnections();
    await this.mcp.shutdown();
  } finally {
    await this.logHandle?.close();
  }
}
```

with:

```typescript
finally {
  try {
    await this.mcp.shutdown();
  } finally {
    await this.logHandle?.close();
  }
}
```

- [ ] Remove the `closeBrowserConnections()` method definition:

```typescript
// Delete these lines:
private async closeBrowserConnections(): Promise<void> {
  await Promise.allSettled(
    Array.from(this.agents.values(), (agent) => agent.browserConnection?.closeAll()),
  );
}
```

- [ ] Verify removal:

```bash
grep "closeBrowserConnections\|browserConnection" packages/agent-core/src/session/index.ts
# Expected: no output
```

- [ ] Commit:

```bash
git add packages/agent-core/src/session/index.ts && git commit -m "revert(session): remove browser connection cleanup from Session.close()"
```

## Task B7: Revert mcp/built-in/registry.ts — restore chrome-devtools behavior

**Depends on:** Phase A
**Files:** Modify `packages/agent-core/src/mcp/built-in/registry.ts`

Steps:

- [ ] Replace the chrome-devtools default-disable logic. Change:

```typescript
if (name === 'chrome-devtools') {
  // Legacy MCP: disabled by default; only enabled when legacyMcpEnabled === true
  return config.browser?.legacyMcpEnabled !== true;
}
```

back to the original:

```typescript
if (name === 'chrome-devtools') {
  return config.browser?.enabled === false;
}
```

- [ ] Verify the line:

```bash
grep -A 2 "name === 'chrome-devtools'" packages/agent-core/src/mcp/built-in/registry.ts
# Expected: return config.browser?.enabled === false;
```

- [ ] Commit:

```bash
git add packages/agent-core/src/mcp/built-in/registry.ts && git commit -m "revert(mcp): restore chrome-devtools enabled-by-default behavior"
```

## Task B8: Revert tools/builtin/index.ts — remove browser export

**Depends on:** Phase A
**Files:** Modify `packages/agent-core/src/tools/builtin/index.ts`

Steps:

- [ ] Remove the browser export line:

```typescript
// Delete this line:
export * from './browser/index';
```

- [ ] Verify removal:

```bash
grep "browser" packages/agent-core/src/tools/builtin/index.ts
# Expected: no output
```

- [ ] Commit:

```bash
git add packages/agent-core/src/tools/builtin/index.ts && git commit -m "revert(tools): remove browser tools export from builtin index"
```

## Phase B Self-Review

- [ ] 1. Spec-coverage: B1-B8 cover all shared infrastructure reversion. ✓
- [ ] 2. Placeholder scan: no TODO/TBD. ✓
- [ ] 3. No phantom tasks: each task produces concrete file edits. ✓
- [ ] 4. Dependency soundness: each task depends on Phase A (files deleted), not on each other. ✓
- [ ] 5. Caller & build soundness: Each task removes the dependency on deleted Phase A files. B1 removes `import ... from '../browser'`. After Phase B, all references to deleted files are gone — code will compile (may still have unused imports). Typecheck at this point validates correctness. ✓
- [ ] 6. Test-the-risk: These are pure deletion/reversion tasks — verified by grep for absence. ✓
- [ ] 7. Type consistency: no new types introduced, only removed. ✓
