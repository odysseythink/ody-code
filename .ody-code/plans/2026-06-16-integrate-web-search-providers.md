# Integrate Multi-Provider Web Search Implementation Plan

**Goal:** Add 11 HTTP-based web search providers plus the existing Moonshot provider behind a new `services.webSearch` config with primary/secondary slots and serial fallback, while preserving backward compatibility with `services.moonshotSearch`.

**Architecture:** A new `WebSearchProviderRegistry` maps provider names to factory functions. `KimiCore.createRuntimeConfig` resolves `config.services.webSearch` (or the legacy `moonshotSearch` alias) into a `FallbackWebSearchProvider` that tries the primary provider and, on retryable errors, the secondary provider. Each provider normalizes its response into `WebSearchResult` (with the upstream raw object preserved in `raw`) and reuses the existing `WebSearchTool` unchanged.

**Tech Stack:** TypeScript, Zod (config schemas), Vitest (unit tests), `globalThis.fetch` with `AbortController` timeouts, `smol-toml` for TOML round-trips.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

| Path | Responsibility |
|---|---|
| `packages/agent-core/src/config/schema.ts:122-140` | New `WebSearchProviderName`, per-provider option schemas, `WebSearchProviderConfig`, `WebSearchConfig`; extend `ServicesConfigSchema` and `ServicesConfigPatchSchema`. |
| `packages/agent-core/src/config/toml.ts:244-258, 425-452` | Read/write TOML transforms for `services.web_search`. |
| `packages/agent-core/src/config/web-search.ts` (new) | Backward-compat `resolveWebSearchConfig` helper. |
| `packages/agent-core/src/tools/builtin/web/web-search.ts:21-27` | Add optional `raw?: unknown` to `WebSearchResult`. |
| `packages/agent-core/src/tools/providers/web-search/types.ts` (new) | Shared `WebSearchProvider` re-export, normalization helpers. |
| `packages/agent-core/src/tools/providers/web-search/http.ts` (new) | `buildUrl`, `getJson`, `postJson`, `authHeaderForProvider`, `httpError`, timeout plumbing. |
| `packages/agent-core/src/tools/providers/web-search/duckduckgo.ts` (new) | DuckDuckGo HTML scraping provider. |
| `packages/agent-core/src/tools/providers/web-search/serpapi.ts` (new) | SerpApi provider. |
| `packages/agent-core/src/tools/providers/web-search/searchapi.ts` (new) | SearchApi.io provider. |
| `packages/agent-core/src/tools/providers/web-search/serper.ts` (new) | Serper.dev provider. |
| `packages/agent-core/src/tools/providers/web-search/bing.ts` (new) | Bing Search provider. |
| `packages/agent-core/src/tools/providers/web-search/baidu.ts` (new) | Baidu AppBuilder search provider. |
| `packages/agent-core/src/tools/providers/web-search/serply.ts` (new) | Serply.io provider. |
| `packages/agent-core/src/tools/providers/web-search/searxng.ts` (new) | SearXNG provider. |
| `packages/agent-core/src/tools/providers/web-search/tavily.ts` (new) | Tavily provider. |
| `packages/agent-core/src/tools/providers/web-search/exa.ts` (new) | Exa provider. |
| `packages/agent-core/src/tools/providers/web-search/perplexity.ts` (new) | Perplexity provider. |
| `packages/agent-core/src/tools/providers/web-search/moonshot.ts` (new) | Factory adapter for existing `MoonshotWebSearchProvider`. |
| `packages/agent-core/src/tools/providers/web-search/registry.ts` (new) | `WebSearchProviderRegistry` and `createDefaultRegistry`. |
| `packages/agent-core/src/tools/providers/web-search/fallback.ts` (new) | `FallbackWebSearchProvider` and `isRetryableError`. |
| `packages/agent-core/src/tools/providers/web-search/runtime.ts` (new) | `resolveWebSearchRuntime` glue. |
| `packages/agent-core/src/tools/providers/web-search/index.ts` (new) | Public barrel export. |
| `packages/agent-core/src/rpc/core-impl.ts:821-849` | Replace direct `MoonshotWebSearchProvider` construction with `resolveWebSearchRuntime`. |
| `packages/agent-core/test/config/web-search.test.ts` (new) | Config schema + backward-compat tests. |
| `packages/agent-core/test/tools/providers/web-search/*.test.ts` (new) | Provider, registry, fallback, runtime tests. |
| `packages/agent-core/test/harness/runtime.test.ts:91-180` | Update runtime config tests for new `webSearch` shape. |

## Dependency Overview

```
Phase A — Config
  Task 1: Schema
      │
      ▼
  Task 2: TOML read/write
      │
      ▼
  Task 3: Backward-compat resolver + tests

Phase B — Providers (parallel groups after Task 3 is not strictly required, but uses no runtime config)
  Task 4: WebSearchResult.raw + normalization types
      │
      ▼
  Task 5: HTTP helpers
      │
      ├── Task 6: DuckDuckGo provider
      ├── Task 7: SerpApi / SearchApi / Serper providers
      ├── Task 8: Bing / Baidu / Serply providers
      └── Task 9: SearXNG / Tavily / Exa / Perplexity providers
      │
      ▼
  Task 10: Registry

Phase C — Runtime
  Task 3 ──────┐
               ▼
  Task 10 ──► Task 11: Fallback provider
                  │
                  ▼
              Task 12: resolveWebSearchRuntime
                  │
                  ▼
              Task 13: Wire KimiCore + runtime tests
                  │
                  ▼
              Task 14: Whole-tree typecheck + lint
```

Phase B provider tasks 6-9 can run in parallel once Task 5 is done. Everything else is sequential.

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| DuckDuckGo HTML layout changes | Capture HTML snapshots in tests; fallback to secondary provider. |
| API keys leak into logs | Never log `apiKey`; log only provider name, result count, and error category. |
| Fallback masks persistent failures | Debug logs record every attempt; tool surfaces the secondary error when both fail. |
| Serial 25s timeouts can sum to 50s | Document in schema comments; user can lower `timeout_ms`. |
| `moonshotSearch` alias migration incorrect | Explicit tests for alias-only, override, and apiKey passthrough. |

## Spec Coverage

| Design Requirement | Task(s) | Status |
|---|---|---|
| 11 HTTP providers (DuckDuckGo, SerpApi, SearchApi, Serper, Bing, Baidu, Serply, SearXNG, Tavily, Exa, Perplexity) | Task 6-9 | covered |
| Keep `services.moonshotSearch` as alias `moonshot` | Task 1, 3, 13 | covered |
| `services.webSearch` with `primary`/`secondary` slots | Task 1-3 | covered |
| Composite `FallbackWebSearchProvider` | Task 11 | covered |
| `WebSearchProviderRegistry` | Task 10 | covered |
| Normalize responses to `WebSearchResult` with `raw` | Task 4, 6-9 | covered |
| Wire into `KimiCore.resolveRuntime` / `createRuntimeConfig` | Task 13 | covered |
| Unit tests for registry, fallback, DuckDuckGo | Task 6, 10-12 | covered |
| Out of scope: MCP-only Parallel provider | — | no-op |
| Out of scope: TUI live switching | — | no-op |
| Out of scope: automatic fallback to unconfigured free providers | — | no-op |
| Out of scope: concurrent multi-provider search / merging | — | no-op |
| Out of scope: provider-specific advanced parameters beyond documented options | — | no-op |

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | [2026-06-16-integrate-web-search-providers/config.md](2026-06-16-integrate-web-search-providers/config.md) | Config schema, TOML round-trip, backward compatibility | done |
| 2 | [2026-06-16-integrate-web-search-providers/providers.md](2026-06-16-integrate-web-search-providers/providers.md) | Provider types, HTTP helpers, 11 provider implementations, registry | done |
| 3 | [2026-06-16-integrate-web-search-providers/runtime.md](2026-06-16-integrate-web-search-providers/runtime.md) | Fallback provider, runtime wiring, integration tests, final typecheck | done |

## Self-Review

- [ ] 1. Spec-coverage table: every design requirement maps to at least one task; no GAPs remain. (See Spec Coverage table above.)
- [ ] 2. Placeholder scan: no `TODO`, `TBD`, "implement later", or dead-code placeholders exist in any task or part file.
- [ ] 3. No phantom tasks: every task creates/modifies files and ends with a test run and a commit; no `--allow-empty` or "already done in Task N" steps.
- [ ] 4. Dependency soundness: every `Depends on:` refers to an earlier task; no task references a symbol defined only in a later task. (Config → Providers → Runtime.)
- [ ] 5. Caller & build soundness:
   - The shared `WebSearchResult` interface is changed only in Task 4; `WebSearchTool` is the runtime consumer and it reads only optional fields, so no caller update is required.
   - The shared `createRuntimeConfig` signature is unchanged; Task 13 only replaces its body.
   - Task 14 runs the full workspace typecheck (`pnpm typecheck`) and lint (`pnpm lint`).
- [ ] 6. Test-the-risk:
   - Config backward-compat: Task 3 asserts alias-only, webSearch precedence, and apiKey passthrough.
   - Fallback: Task 11 asserts primary success, retryable fallback, no-fallback-on-auth, and secondary failure propagation.
   - Provider normalization: every provider task asserts request shape and normalized output; DuckDuckGo uses captured HTML.
   - Retryable classifier: Task 11 enumerates must-survive inputs (`401`, `403`, `unauthorized`, `AbortError`) and confirms they are non-retryable.
- [ ] 7. Type consistency: `WebSearchProviderName`, `WebSearchConfig`, `WebSearchProviderConfig`, `WebSearchProvider`, `WebSearchResult`, `ProviderFactoryDeps`, and `resolveWebSearchRuntime` names and shapes match across schema, providers, registry, fallback, runtime, and tests.
