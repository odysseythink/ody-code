# Integrate Multi-Provider Web Search

## Scope

### In Scope

1. Add 11 HTTP-based web search providers ported from anything-llm [C:UPSTREAM]:
   - DuckDuckGo, SerpApi, SearchApi, Serper.dev, Bing Search, Baidu Search, Serply.io, SearXNG, Tavily, Exa, Perplexity.
2. Keep the existing `services.moonshotSearch` config and alias it into the new provider model as `moonshot` [C:USER].
3. Introduce `services.webSearch` config with exactly two provider slots: `primary` and `secondary` [C:USER].
4. Implement a composite `FallbackWebSearchProvider` that tries `primary` then `secondary` [C:USER].
5. Implement a `WebSearchProviderRegistry` that maps provider names to factory functions [C:INFERRED].
6. Normalize every provider response to `WebSearchResult` while preserving the upstream raw object in `raw` [C:USER].
7. Wire the new runtime into `KimiCore.resolveRuntime` / `createRuntimeConfig` so `ToolServices.webSearcher` is the composite provider [C:INFERRED].
8. Add unit tests for the registry, fallback logic, and a representative provider (DuckDuckGo) [C:INFERRED].

### Out of Scope

1. **Parallel (MCP-only from opencode)** — deferred to a future MCP-native provider design; this design covers only HTTP APIs [C:USER].
2. **TUI/CLI live provider switching** — deferred; provider selection is config-only in this phase [C:USER].
3. **Automatic fallback to unconfigured free providers** — fallback only occurs between configured `primary` and `secondary` [C:USER].
4. **Concurrent multi-provider search / result merging** — only serial fallback is supported [C:USER].
5. **Provider-specific advanced parameters beyond the documented `options` schemas** — users can pass extra keys via `options`, but only documented keys are validated and typed [C:USER].

## Prior Art

- **opencode** (`packages/opencode/src/tool/websearch.ts`, `mcp-websearch.ts`) uses an Exa/Parallel dual-provider model selected by env var and runtime flags, communicating via MCP `tools/call` over HTTP [C:UPSTREAM].
- **anything-llm** (`server/utils/agents/aibitat/plugins/web-browsing.js`) implements 11 HTTP search providers with a simple `switch(provider)` dispatcher and explicit error messages when API keys are missing [C:UPSTREAM].
- **ody-code today** (`packages/agent-core/src/tools/builtin/web/web-search.ts`, `providers/moonshot-web-search.ts`) already defines a `WebSearchProvider` interface and a `WebSearchTool`; `KimiCore` injects the provider via `ToolServices.webSearcher` [C:INFERRED].

## Architecture

```
config.services.webSearch
        │
        ▼
KimiCore.createRuntimeConfig() ────────► WebSearchProviderRegistry
        │                                          │
        │    ┌─────────────────┐   ┌─────────────────┐
        └──► │ primary factory │   │ secondary factory│
             │  (optional)     │   │  (optional)     │
             └────────┬────────┘   └────────┬────────┘
                      │                     │
                      ▼                     ▼
              FallbackWebSearchProvider ◄───┘
                      │
                      ▼
              WebSearchTool (existing)
                      │
                      ▼
                 LLM context
```

Data transformations at each arrow:

- `config.services.webSearch` → `createRuntimeConfig`: backward-compat alias `moonshotSearch` is merged into provider configs.
- `Registry` → `FallbackWebSearchProvider`: two `WebSearchProvider` instances are composed.
- `FallbackWebSearchProvider` → `WebSearchTool`: a single `search()` call that may internally retry on the secondary provider.
- `WebSearchTool` → LLM: results are rendered as `Title/Date/URL/Snippet/Content` text blocks.

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | The existing `WebSearchProvider` interface (`search(query, opts) => Promise<WebSearchResult[]>`) can be reused without changing `WebSearchTool` [C:INFERRED] | High | Low — if the interface needs new fields, only the composite provider and new providers change; `WebSearchTool` stays untouched. | Read `packages/agent-core/src/tools/builtin/web/web-search.ts`. |
| 2 | `KimiCore.resolveRuntime` is the only production place that constructs `ToolServices` [C:INFERRED] | High | Medium — missing another construction site would leave that code path without the new providers. | Grep for `ToolServices` construction and `new Session({ toolServices`. |
| 3 | `config.services` can be extended with a new `webSearch` key without breaking existing TOML round-trips [C:INFERRED] | High | Medium — invalid schema would reject user configs on load. | Add schema fields and run existing config tests. |
| 4 | DuckDuckGo HTML scraping is legal and stable enough for our use case [C:INFERRED] | Low | High — parsing may break on DDG layout changes or be blocked. | Implement with unit tests using captured HTML; mark as best-effort. |
| 5 | Users accept storing API keys in the local config file for all new providers [C:USER] | High | Low — user already chose this over env vars. | N/A |
| 6 | A single global `proxyUrl` per provider (DuckDuckGo) is sufficient; per-request proxy auth is not required [C:USER] | Medium | Low — if needed later it can be added as a sub-field without breaking the schema. | N/A |

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | DuckDuckGo HTML layout changes break parsing | Medium | High | Unit-test with captured snapshots; fallback to secondary provider hides single-provider failures. |
| 2 | A provider API silently changes request/response shape | Low | High | Each provider has isolated implementation; tests assert normalized output shape; changes are localized. |
| 3 | API keys leak into logs or telemetry | Low | Critical | Never log `apiKey`; log only provider name, result count, and error category; redact config when serialized for diagnostics. |
| 4 | Fallback masks persistent provider failures | Medium | Medium | Log each fallback attempt with provider name and error category; expose provider name in tool metadata. |
| 5 | Config migration from `moonshotSearch` is incorrect | Low | High | Add explicit unit tests for both alias-only and override scenarios. |
| 6 | Two sequential 25s timeouts can block the tool for 50s | Medium | Medium | Document in config comments; users can lower `timeoutMs`; future work can add a total deadline budget. |

## Self-Review

- **Security** — Checked secret handling and log content. Found: proxy URL may embed credentials (`http://user:pass@proxy`). Fixed by adding a note that proxy auth should use environment variables or a local proxy manager; the provider passes the URL directly to `fetch` without redaction, so credential-in-URL is at user risk. API keys are never logged; only provider name, result count, and error category are logged.
- **Test** — Verified the three most expensive predicates with ephemeral `node -e`: (1) fallback retryable classifier correctly treats 401/403 as non-retryable and 429/5xx/network/timeout as retryable; (2) DuckDuckGo redirect extraction handles encoded URLs, direct URLs, empty strings, and invalid URLs; (3) backward-compat alias gives `webSearch` precedence over `moonshotSearch`. Added concrete must-reject cases (unknown provider, auth error does not fallback) to the test plan.
- **Ops** — Checked latency and identifier collision. Found: serial fallback with per-provider 25s default can sum to 50s. Added risk #6 and mitigation (documented config, user-adjustable timeout). Provider names are enum literals, so no collision risk. No concurrent calls are introduced.
- **Integration** — Verified all hooks exist in code: `WebSearchProvider` interface and `WebSearchTool` in `packages/agent-core/src/tools/builtin/web/web-search.ts`; `ToolServices` in `packages/agent-core/src/tools/support/services.ts`; `ToolManager` registers `WebSearchTool` from `toolServices.webSearcher` at line 458 of `packages/agent-core/src/agent/tool/index.ts`; `KimiCore.createRuntimeConfig` constructs `ToolServices` at lines 821-849 of `packages/agent-core/src/rpc/core-impl.ts`; `MoonshotWebSearchProvider` and `MoonshotServiceConfig` already exist. No silent retargeting: the design lands in these existing locations.
- **Scope** — The design remains one coherent feature (multi-provider web search). It is split into three parts only for readability, not because the pieces are independent products.

## Data Models

See [config.md](2026-06-16-integrate-web-search-providers/config.md) for the full config schemas, provider option schemas, backward-compatibility alias, and TOML round-trip rules.

## Algorithms

See:
- [providers.md](2026-06-16-integrate-web-search-providers/providers.md) for result normalization and DuckDuckGo HTML parsing algorithms.
- [runtime.md](2026-06-16-integrate-web-search-providers/runtime.md) for the fallback and retryable-error-classifier algorithms.

## Error Handling

See [runtime.md](2026-06-16-integrate-web-search-providers/runtime.md) for the error-category table, fallback behavior, and propagation to the model.

## User Final Approval

- [x] Audit level: Deep
- [x] All [C:INFERRED] assumptions accepted via post-write audit gate
- [x] Scope In/Out, Architecture, Data Models, Algorithms, Error Handling, Self-Review completed
- [x] Design ready for `/plan`

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | [config.md](2026-06-16-integrate-web-search-providers/config.md) | Config schema, backward compatibility, data models | done |
| 2 | [providers.md](2026-06-16-integrate-web-search-providers/providers.md) | Provider interfaces, 12 provider implementations, normalization | done |
| 3 | [runtime.md](2026-06-16-integrate-web-search-providers/runtime.md) | Registry, fallback provider, wiring, tests | done |
