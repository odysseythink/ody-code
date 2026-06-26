# Phase C: Revert test files, native build check, and dependencies

**Scope:** Revert browser-related changes to test files, the native build check script, and remove the `puppeteer-core` dependency.

**Depends on:** Phase B (shared infrastructure reverted — shared signatures match ace5ba1)

## Task C1: Revert test/agent/permission.test.ts — remove 'browser-host' from chain

**Depends on:** Phase B
**Files:** Modify `packages/agent-core/test/agent/permission.test.ts`

Steps:

- [ ] Remove the `'browser-host'` entry added to the expected policy chain. Delete this line:

```typescript
'browser-host',
```

This line appears between `'user-configured-allow'` and `'browser-tool-ask'` in the policy chain test assertion.

- [ ] Verify removal:

```bash
grep "browser-host" packages/agent-core/test/agent/permission.test.ts
# Expected: no output
```

- [ ] Commit:

```bash
git add packages/agent-core/test/agent/permission.test.ts && git commit -m "revert(test): remove browser-host from permission policy chain test"
```

## Task C2: Revert test/agent/permission/browser-tool-ask.test.ts — remove Browser* tests

**Depends on:** Phase B
**Files:** Modify `packages/agent-core/test/agent/permission/browser-tool-ask.test.ts`

Steps:

- [ ] Remove the three Browser* test cases that were added after the original 5 tests:
  - `'returns ask for native BrowserBrowse tool'` (test block)
  - `'returns ask for native BrowserSnapshot tool'` (test block)
  - The expanded `'returns undefined for non-browser builtin tools'` test block that now checks `Browser*` tools — restore to original which only checks `Read` and `Write`.

Current state has:

```typescript
it('returns undefined for non-browser builtin tools', () => {
  expect(policy.evaluate(policyContext('Read'))).toBeUndefined();
  expect(policy.evaluate(policyContext('Write'))).toBeUndefined();
});
```

This is already the correct original state (no change needed for this test block).

Delete the two added test blocks after the `'returns undefined for Write tool'` block:

```typescript
// Remove these two test blocks:
it('returns ask for native BrowserBrowse tool', () => {
  const result = policy.evaluate(policyContext('BrowserBrowse'));
  expect(result).toEqual({
    kind: 'ask',
    reason: { tool: 'BrowserBrowse' },
  });
});

it('returns ask for native BrowserSnapshot tool', () => {
  const result = policy.evaluate(policyContext('BrowserSnapshot'));
  expect(result).toEqual({
    kind: 'ask',
    reason: { tool: 'BrowserSnapshot' },
  });
});

it('returns undefined for non-browser builtin tools', () => {
  expect(policy.evaluate(policyContext('Read'))).toBeUndefined();
  expect(policy.evaluate(policyContext('Write'))).toBeUndefined();
});
```

- [ ] Verify only 5 original tests remain:

```bash
grep -c "it(" packages/agent-core/test/agent/permission/browser-tool-ask.test.ts
# Expected: 5
```

- [ ] Commit:

```bash
git add packages/agent-core/test/agent/permission/browser-tool-ask.test.ts && git commit -m "revert(test): remove Browser* native tool tests from browser-tool-ask policy tests"
```

## Task C3: Revert test/mcp/built-in/registry.test.ts — restore original chrome-devtools behavior

**Depends on:** Phase B (Task B7)
**Files:** Modify `packages/agent-core/test/mcp/built-in/registry.test.ts`

Steps:

- [ ] Revert test "isDisabled returns true for chrome-devtools by default" back to "isDisabled returns false for chrome-devtools by default":

Change the test name and assertion:

```typescript
// Change from:
it('isDisabled returns true for chrome-devtools by default', () => {
  // ...
  expect(registry.isDisabled('chrome-devtools', config)).toBe(true);
});

// To:
it('isDisabled returns false for chrome-devtools by default', () => {
  // ...
  expect(registry.isDisabled('chrome-devtools', config)).toBe(false);
});
```

- [ ] Remove the added test for legacyMcpEnabled:

```typescript
// Delete this entire test block:
it('isDisabled returns false for chrome-devtools when legacyMcpEnabled is true', () => {
  const registry = new BuiltInMcpRegistry();
  registry.register({
    name: 'chrome-devtools',
    displayName: 'Chrome DevTools',
    enabledByDefault: true,
    config: { transport: 'stdio' as const, command: 'node' },
  });
  const config: KimiConfig = { providers: {}, browser: { legacyMcpEnabled: true } };
  expect(registry.isDisabled('chrome-devtools', config)).toBe(false);
});
```

- [ ] Verify test count restored:

```bash
grep -c "it(" packages/agent-core/test/mcp/built-in/registry.test.ts
# Expected: 8 (was 9)
```

- [ ] Commit:

```bash
git add packages/agent-core/test/mcp/built-in/registry.test.ts && git commit -m "revert(test): restore chrome-devtools default-enabled test behavior"
```

## Task C4: Revert test/mcp/built-in-integration.test.ts — remove legacyMcpEnabled config

**Depends on:** Phase B (Task B7)
**Files:** Modify `packages/agent-core/test/mcp/built-in-integration.test.ts`

Steps:

- [ ] Revert the test name and remove the config.toml write. Change:

```typescript
// From:
it('injects chrome-devtools server config into new sessions when legacyMcpEnabled is true', async () => {
  tmp = await mkdtemp(join(tmpdir(), 'kimi-core-built-in-'));
  const homeDir = join(tmp, 'home');
  const workDir = join(tmp, 'work');
  await mkdir(homeDir, { recursive: true });
  await mkdir(workDir, { recursive: true });
  await writeFile(
    join(homeDir, 'config.toml'),
    '[browser]\nlegacyMcpEnabled = true\n',
    'utf-8',
  );

// To:
it('injects chrome-devtools server config into new sessions', async () => {
  tmp = await mkdtemp(join(tmpdir(), 'kimi-core-built-in-'));
  const homeDir = join(tmp, 'home');
  const workDir = join(tmp, 'work');
  await mkdir(homeDir, { recursive: true });
  await mkdir(workDir, { recursive: true });
```

- [ ] Verify:

```bash
grep "legacyMcpEnabled" packages/agent-core/test/mcp/built-in-integration.test.ts
# Expected: no output
```

- [ ] Commit:

```bash
git add packages/agent-core/test/mcp/built-in-integration.test.ts && git commit -m "revert(test): remove legacyMcpEnabled from chrome-devtools integration test"
```

## Task C5: Revert native build check-bundle.mjs — remove proxy-agent

**Depends on:** Phase A
**Files:** Modify `apps/ody-code/scripts/native/check-bundle.mjs`

Steps:

- [ ] Remove `'proxy-agent'` from the `optionalRuntimeRequires` set:

```typescript
// Remove this line:
'proxy-agent',
```

- [ ] Verify:

```bash
grep "proxy-agent" apps/ody-code/scripts/native/check-bundle.mjs
# Expected: no output
```

- [ ] Commit:

```bash
git add apps/ody-code/scripts/native/check-bundle.mjs && git commit -m "revert(native): remove proxy-agent from check-bundle optional runtime requires"
```

## Task C6: Remove puppeteer-core dependency

**Depends on:** Phase A
**Files:** Modify `packages/agent-core/package.json`

Steps:

- [ ] Remove the puppeteer-core dependency line from `packages/agent-core/package.json`:

```json
// Remove this line from dependencies:
"puppeteer-core": "^25.1.0",
```

- [ ] Run `pnpm install` to update the lockfile:

```bash
source ~/.nvm/nvm.sh && nvm use 24.16.0 && cd /Users/ranwei/workspace/ody-code && pnpm install
```

- [ ] Verify puppeteer-core is gone:

```bash
grep "puppeteer-core" packages/agent-core/package.json
# Expected: no output
ls packages/agent-core/node_modules/puppeteer-core 2>&1
# Expected: No such file or directory
```

- [ ] Commit:

```bash
git add packages/agent-core/package.json pnpm-lock.yaml && git commit -m "revert(deps): remove puppeteer-core dependency"
```

## Phase C Self-Review

- [ ] 1. Spec-coverage: C1-C6 cover all remaining test reverts + dep removal. ✓
- [ ] 2. Placeholder scan: no TODO/TBD. ✓
- [ ] 3. No phantom tasks: each task produces concrete changes. ✓
- [ ] 4. Dependency soundness: C1-C4 depend on Phase B (shared sigs match ace5ba1); C5-C6 depend on Phase A (no browser code left). ✓
- [ ] 5. Caller & build soundness: C1-C4 are test-only reverts. C5 removes a string allowlist entry. C6 is dep removal. No shared-signature changes. ✓
- [ ] 6. Test-the-risk: Tests are being restored to original state — no new assertions. ✓
- [ ] 7. Type consistency: no new types. ✓
