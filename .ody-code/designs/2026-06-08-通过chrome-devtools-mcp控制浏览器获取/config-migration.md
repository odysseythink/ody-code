# Part: Configuration & MCP Migration

## Purpose

定义新浏览器能力在配置层的表达，以及如何与现有 `chrome-devtools` MCP server 共存/迁移。

## Current State

From `packages/agent-core/src/mcp/built-in/registry.ts:44-51`:

```typescript
isDisabled(name: string, config: KimiConfig): boolean {
  const def = this.definitions.get(name);
  if (def === undefined) return true;
  if (name === 'chrome-devtools') {
    return config.browser?.enabled === false;
  }
  return !def.enabledByDefault;
}
```

Current behavior: `chrome-devtools` MCP server is enabled by default unless `config.browser.enabled === false`.

## Proposed Config Schema Extension

```typescript
// packages/agent-core/src/config/schema.ts

interface BrowserConfig {
  enabled?: boolean;                 // default true (native tools enabled)
  legacyMcpEnabled?: boolean;        // default false; opt-in to old chrome-devtools MCP
  autoLaunch?: boolean;              // default true
  chromePort?: number;               // explicit CDP port
  headless?: boolean;                // default true for launched instances
  executablePath?: string;           // custom Chromium/Chrome binary
  userDataDir?: string;              // launched instance profile dir
  allowedHosts?: string[];           // static URL allowlist
  sensitivePatterns?: string[];      // glob/regex patterns always requiring ask
  defaultTimeoutMs?: number;         // default 30000
  screenshotEnabled?: boolean;       // default false (save tokens)
  extensionBridge?: {
    enabled?: boolean;               // default false (Phase 2)
    wsPort?: number;                 // 0 = random
    wsEndpoint?: string;
  };
}

interface KimiConfig {
  // ... existing fields
  browser?: BrowserConfig;
}
```

## Migration Path

### Phase 1: Native tools as default

- Change `BuiltInMcpRegistry.isDisabled('chrome-devtools')` logic:
  ```typescript
  return config.browser?.legacyMcpEnabled === true || config.browser?.enabled === false;
  ```
- By default, `chrome-devtools` MCP is **disabled**.
- Native browser tools are registered in `ToolManager.initializeBuiltinTools()` when `config.browser?.enabled !== false`.

### Phase 2: Deprecate and remove

- Emit deprecation warning when `legacyMcpEnabled === true`.
- In a future major release, remove the vendored `chrome-devtools` MCP entirely.

## Profile Integration

Default agent profile should expose the high-level browser tools:

```json
{
  "tools": [
    "Read", "Write", "Edit", "Bash",
    "BrowserBrowse", "BrowserExtract", "BrowserAct",
    "mcp__*"
  ]
}
```

Atomic tools are hidden by default. Advanced users can opt in via custom profile:

```json
{
  "tools": [
    "Read", "Write",
    "BrowserBrowse", "BrowserExtract", "BrowserAct",
    "BrowserNavigate", "BrowserSnapshot", "BrowserClick",
    "BrowserFill", "BrowserEvaluate", "BrowserScreenshot"
  ]
}
```

## Backward Compatibility

| Config | Behavior |
|---|---|
| `browser.enabled: true` (default) | Native tools active; `chrome-devtools` MCP disabled. |
| `browser.enabled: false` | No browser tools; `chrome-devtools` MCP disabled. |
| `browser.legacyMcpEnabled: true` | Native tools disabled; old `chrome-devtools` MCP enabled. |
| `browser.enabled: true` + `browser.legacyMcpEnabled: true` | Conflict: native tools take precedence; emit warning. |

## TUI / CLI Considerations

- New settings under `/settings` or `ody config`:
  - `browser.autoLaunch`
  - `browser.chromePort`
  - `browser.allowedHosts`
- TUI status bar could show browser connection state icon (future).
- Connection diagnostics command (future): e.g. `ody browser diagnose`.

## Call-Sites

| Location | File | Lines | Action |
|---|---|---|---|
| MCP disable logic | `packages/agent-core/src/mcp/built-in/registry.ts:44-51` | Update to use `legacyMcpEnabled`. |
| Tool registration gate | `packages/agent-core/src/agent/tool/index.ts:388-462` | Conditionally register native tools based on `config.browser?.enabled`. |
| Config schema | `packages/agent-core/src/config/schema.ts` | [C:INFERRED] Add `BrowserConfig` fields. |
| Default profile | `packages/agent-core/src/profile.ts` or similar | [C:INFERRED] Add `BrowserBrowse`, `BrowserExtract`, `BrowserAct` to default tool list. |

## Test Assertions

1. With default config, `isDisabled('chrome-devtools')` returns `true`.
2. With `legacyMcpEnabled: true`, `isDisabled('chrome-devtools')` returns `false`.
3. With `enabled: false`, native browser tools are not registered.
4. With `enabled: true`, high-level browser tools appear in `ToolManager.data()`.
