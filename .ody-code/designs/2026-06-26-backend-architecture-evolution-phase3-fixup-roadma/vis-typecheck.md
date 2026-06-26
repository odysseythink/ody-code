# Part 3 — `apps/vis/web` Typecheck Remediation

## 1. Scope

This part designs the minimal source-level fixes required for `pnpm -r typecheck` to pass. The errors are pre-existing and located in packages that `apps/vis/web` transitively type-checks. This part covers:

- Removing or using unused imports and variables reported as `TS6133` / `TS6196`. [C:USER]
- Fixing the `TS2339` error in `packages/agent-core-shared/src/wasm-loader.ts` where `instance` is not recognized on the `WebAssembly.instantiate` return type. [C:USER]
- Keeping changes localized; no tsconfig rule changes or broad suppressions. [C:USER]

Out of scope:
- Refactoring the affected modules beyond the reported type errors. [C:USER]
- Changing `apps/vis/web/tsconfig.json` rules to hide errors. [C:USER]

---

## 2. Error Inventory

All errors were produced by `pnpm --filter @odysseythink/vis-web run typecheck` on Node 24.16.0. [C:INFERRED]

| # | File | Line:Col | Error | Fix strategy |
|---|---|---|---|---|
| 1 | `packages/agent-core-shared/src/wasm-loader.ts` | 46:13 | `TS2339: Property 'instance' does not exist on type 'Instance'.` | Cast `WebAssembly.instantiate` result to `WebAssembly.WebAssemblyInstantiatedSource`. |
| 2 | `packages/agent-core/src/agent/permission/index.ts` | 325:5 | `TS6133: 'signal' is declared but its value is never read.` | Prefix with underscore (`_signal`) or remove if unused. |
| 3 | `packages/agent-core/src/agent/permission/matches-rule.ts` | 3:29 | `TS6133: 'ParsedPattern' is declared but its value is never read.` | Remove import if not referenced; otherwise use `type` import. |
| 4 | `packages/agent-core/src/rpc/client.ts` | 154:47 | `TS6133: 'fn' is declared but its value is never read.` | Rename to `_fn` or remove. |
| 5 | `packages/agent-core/src/rpc/transports/websocket.ts` | 66:10 | `TS6133: 'decodeJson' is declared but its value is never read.` | Remove import. |
| 6 | `packages/agent-core/src/session/checkpoint/backup-store.ts` | 10:10 | `TS6133: 'dirname' is declared but its value is never read.` | Remove import. |
| 7 | `packages/agent-core/src/session/checkpoint/recovery.ts` | 17:36 | `TS6196: 'CheckpointVersion' is declared but never used.` | Remove export or use it; prefer removal if truly unused. |
| 8 | `packages/agent-core/src/session/hooks/types.ts` | 3:10 | `TS6133: 'HOOK_EVENT_TYPES' is declared but its value is never read.` | Remove import or reference it in a type-only context. |
| 9 | `packages/agent-core/src/utils/wasm-glob.ts` | 13:7 | `TS6133: 'GLOB_ERROR' is declared but its value is never read.` | Remove import or use. |
| 10 | `packages/e2e-testing/src/generators/python-pytest.ts` | 1:16 | `TS6133: 'extname' is declared but its value is never read.` | Remove import. |
| 11 | `packages/e2e-testing/src/recursive-impact-analyzer.ts` | 304:3 | `TS6133: 'existsSync' is declared but its value is never read.` | Remove import. |
| 12 | `packages/e2e-testing/src/result-cache.ts` | 54:22 | `TS6138: Property 'kaos' is declared but its value is never read.` | Destructure with underscore prefix or remove `kaos` from destructuring. |
| 13 | `packages/mcp-host/src/built-in/sea-builtins.ts` | 2:33 | `TS6133: 'readFileSync' is declared but its value is never read.` | Remove import. |
| 14 | `packages/mcp-host/src/oauth/service.ts` | 29:8 | `TS6133: 'OAuthClientProvider' is declared but its value is never read.` | Remove import. |
| 15 | `packages/mcp-host/src/trace-recorder.ts` | 2:10 | `TS6133: 'dirname' is declared but its value is never read.` | Remove import. |

---

## 3. Detailed Fixes

### 3.1 `wasm-loader.ts` instance type fix

File: `packages/agent-core-shared/src/wasm-loader.ts`, line 46. [C:INFERRED]

```ts
// Before
const { instance } = await WebAssembly.instantiate(bytes, {});

// After
const { instance } = (await WebAssembly.instantiate(
  bytes,
  {},
)) as WebAssembly.WebAssemblyInstantiatedSource;
```

Rationale: TypeScript resolves the bytes overload ambiguously when `strict` checks are enabled. The explicit cast documents that we expect the `WebAssemblyInstantiatedSource` shape. [C:INFERRED]

### 3.2 Unused import / variable fixes

Algorithm for each reported symbol:

```
function fixUnusedSymbol(filePath, symbolName, line)
  source := readFile(filePath)
  references := findAllReferences(source, symbolName)
  if references.length === 1  // only declaration
    removeDeclaration(source, symbolName)
  else if parameterPosition(symbolName) !== null
    renameToUnderscore(source, symbolName)
  else
    replaceWithTypeOnlyImport(source, symbolName)
  writeFile(filePath, source)
```

Rules [C:INFERRED]:
- Imported identifiers with zero references → delete the import clause.
- Function parameters with zero references → prefix with `_`.
- Destructured properties with zero references → prefix with `_`.
- Exported types with zero references → remove the `export` keyword first; if still unused, remove the declaration.

### 3.3 Example transformations

`packages/agent-core/src/agent/permission/index.ts` line 325:
```ts
// Before
function someHandler(signal: AbortSignal) { ... }

// After
function someHandler(_signal: AbortSignal) { ... }
```

`packages/e2e-testing/src/result-cache.ts` line 54:
```ts
// Before
const { kaos, ...rest } = something;

// After
const { kaos: _kaos, ...rest } = something;
```

---

## 4. Call-Site Integration

No new call sites are introduced. The change set is limited to removing unused symbols and one type assertion. [C:INFERRED]

---

## 5. Error Handling

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| A "fixed" symbol is actually used in another package | Build/test failure in that package. | Revert removal; change to `type` import or `_` prefix instead. | CI green. |
| `wasm-loader.ts` cast is rejected by stricter checks | Try runtime guard: `const result = await WebAssembly.instantiate(bytes, {}); const instance = 'instance' in result ? result.instance : result;` | Fallback to the guard form. | Typecheck green. |
| New unused variables appear after merge | Add them to the same fix pattern; consider a lint-staged hook in a follow-up. | N/A | N/A |

---

## 6. Test Plan

| Test | Assertion |
|---|---|
| `pnpm -r typecheck` exits 0 | No `TS6133`, `TS6196`, or `TS2339` errors. |
| `pnpm --filter @odysseythink/vis-web run typecheck` exits 0 | Confirms the directly failing package is green. |
| `cargo test -p ody-host` still passes | Wasm loader change does not break Rust-side consumers. |
| `pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts` still passes | Type fixes do not break SDK. |
| `pnpm --filter @odysseythink/agent-core-shared run typecheck` exits 0 | Wasm loader file compiles. |

Done criteria [C:USER]:
- `pnpm -r typecheck` exits 0 on Node 24.15.0+.

---

## 7. Local Notes

- Do not add `// @ts-ignore` unless a symbol cannot be removed for backward-compatibility reasons. [C:INFERRED]
- Prefer `_` prefix over removal for parameters that are part of a public or callback signature, to preserve the API shape. [C:INFERRED]
- Run `pnpm -r typecheck` after each file change to catch cascading errors early. [C:INFERRED]
