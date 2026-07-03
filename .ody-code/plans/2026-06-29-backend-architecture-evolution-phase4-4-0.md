# Phase 4.4.0 — Tool Infrastructure & Shared Support Implementation Plan

**Goal:** Build the shared `tools-rs` crate and host-level support layer that all Phase 4.4 builtin tools will depend on, with every helper verified by TS↔Rust L1 parity fixtures.

**Architecture:** A new `tools-rs` crate sits between `kaos-rs` and `ody-host`/`agent-rs`, exposing pure/policy helpers (path security, schema/validation, result building, file-type sniff, rg resolution) and the common tool information types. `ody-host` keeps its `Tool` trait for host-internal RPC tools, while `agent-rs`’s existing `agent_loop::ExecutableTool` trait remains the execution contract for builtin tools that run inside a turn. Each helper is paired with a JSON fixture that the TS `LocalKaos`/helpers and Rust helpers both consume, so parity is asserted before any concrete tool is migrated.

**Tech Stack:** Rust 2021, `tokio`, `serde_json`, `globset`, `regex`, `jsonschema`, `reqwest` (for rg download), `thiserror`; TS fixtures driven by `packages/integration-tests/src/parity` harness; CI via `.github/workflows/rust-host.yml`.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| Responsibility | Path |
|---|---|
| New crate definition & workspace wiring | `rust-ody/crates/tools-rs/Cargo.toml`, `rust-ody/crates/tools-rs/src/lib.rs`, `rust-ody/Cargo.toml` |
| Common tool types (source, info, collisions, store, workspace) | `rust-ody/crates/tools-rs/src/types.rs`, `rust-ody/crates/tools-rs/src/workspace.rs`, `rust-ody/crates/tools-rs/src/store.rs` |
| Tool result builder | `rust-ody/crates/tools-rs/src/result_builder.rs` |
| Tool access declarations / conflict detection | `rust-ody/crates/tools-rs/src/tool_accesses.rs` |
| Path security policy & sensitive-file detection | `rust-ody/crates/tools-rs/src/policies/path_access.rs`, `rust-ody/crates/tools-rs/src/policies/sensitive.rs` |
| Rule / path matching | `rust-ody/crates/tools-rs/src/policies/rule_match.rs`, `rust-ody/crates/tools-rs/src/policies/path_glob_match.rs` |
| Input JSON-schema builder | `rust-ody/crates/tools-rs/src/schema.rs` |
| Args validator (AJV-compatible messages) | `rust-ody/crates/tools-rs/src/args_validator.rs` |
| File-type / image-dimension sniff | `rust-ody/crates/tools-rs/src/file_type.rs` |
| rg binary locator | `rust-ody/crates/tools-rs/src/rg_locator.rs` |
| 2-level directory lister | `rust-ody/crates/tools-rs/src/list_directory.rs` |
| L1 golden fixtures | `packages/integration-tests/fixtures/tools-rs/**/*.json` |
| Fixture runner / parity tests | `rust-ody/crates/tools-rs/tests/l1_parity.rs`, `packages/integration-tests/src/parity/tools-l1.ts` |
| CI parity job | `.github/workflows/rust-host.yml` |

---

## Dependency Overview

```
Task 1  Create tools-rs crate + wire workspace
   │
   ├──► Task 2  Common tool types / workspace / store / result builder
   │
   ├──► Task 3  ToolAccesses conflict detection
   │
   ├──► Task 4  Path security policy + sensitive files
   │       │
   │       └──► Task 5  Rule / path matching
   │
   ├──► Task 6  Input JSON-schema builder
   │
   ├──► Task 7  Args validator
   │
   ├──► Task 8  File-type sniff + image dimensions
   │
   ├──► Task 9  rg locator
   │
   ├──► Task 10 list-directory helper
   │
   └──► Task 11 L1 golden fixtures + parity runner
            │
            └──► Task 12 CI job + known-gaps entry
```

- Tasks 2–10 can be developed in parallel once Task 1 lands, because they operate on disjoint source files. Each task depends only on types/constants introduced in earlier tasks (Task 1 for crate layout; Task 4 for Task 5 path semantics).
- Task 11 depends on Tasks 2–10 (it needs all helpers to exist to run the combined fixture suite).
- Task 12 depends on Task 11 (CI runs the fixture suite).

---

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| `z.toJSONSchema` behavior in TS (input view, `additionalProperties:false`) is hard to replicate exactly in Rust without a full zod port | Limit Task 6 to the schema shapes actually used by builtin tools (object properties, types, descriptions, required, enums) and capture them in fixtures. |
| AJV error-message wording differs from `jsonschema` crate | Task 7 explicitly normalizes messages to AJV shapes (`must have required property`, `must NOT have additional property`) and fixture-tests the mapping. |
| rg locator downloads from internal CDN; CI may not have network | Task 9 tests pure lookup (`find_existing_rg`) with a mocked `$PATH` / tmp share dir; download path is exercised locally, not in CI. |
| Path canonicalization must match `pathe.normalize` across separators and `..` segments | Task 4 fixtures include Windows variants, absolute/relative, `~` expansion, and sensitive-file edge cases. |
| ToolAccesses conflict logic is security-sensitive | Task 3 tests recursive overlap, exact match, `all` wildcard, and non-conflicting read/read pairs. |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-29-backend-architecture-evolution-phase4-4-0/types.md` | Crate, common types, `ToolAccesses`, result builder | done |
| 2 | `2026-06-29-backend-architecture-evolution-phase4-4-0/path-policy.md` | Path security policy, sensitive files, rule/path matching | done |
| 3 | `2026-06-29-backend-architecture-evolution-phase4-4-0/schema-validation.md` | Input schema builder and args validator | done |
| 4 | `2026-06-29-backend-architecture-evolution-phase4-4-0/support.md` | File-type sniff, rg locator, list-directory | done |
| 5 | `2026-06-29-backend-architecture-evolution-phase4-4-0/fixtures-ci.md` | L1 golden fixtures, parity runner, CI, known-gaps | done |

---

## Spec-Coverage Table

| Spec item / requirement | Part(s) / Task(s) | Status |
|---|---|---|
| 4.4.0 — Create `tools-rs` crate and wire workspace | Part 1 Task 1 | covered |
| 4.4.0 — Common tool types (`ToolSource`, `ToolInfo`, collisions) | Part 1 Task 2 | covered |
| 4.4.0 — `WorkspaceConfig` + `ToolStore` | Part 1 Task 2 | covered |
| 4.4.0 — `ToolResultBuilder` with line-length truncation | Part 1 Task 2 | covered |
| 4.4.0 — `ToolAccesses` conflict detector | Part 1 Task 3 | covered |
| 4.4.0 — Path canonicalization / workspace containment | Part 2 Task 4 | covered |
| 4.4.0 — Sensitive-file detector with must-survive inputs | Part 2 Task 4 | covered |
| 4.4.0 — Rule / path glob matching | Part 2 Task 5 | covered |
| 4.4.0 — Input JSON-schema builder | Part 3 Task 6 | covered |
| 4.4.0 — AJV-style args validator | Part 3 Task 7 | covered |
| 4.4.0 — File-type sniff + image dimensions | Part 4 Task 8 | covered |
| 4.4.0 — rg binary locator | Part 4 Task 9 | covered |
| 4.4.0 — 2-level directory lister | Part 4 Task 10 | covered |
| 4.4.0 — L1 golden fixtures for all helpers | Part 5 Task 11 | covered |
| 4.4.0 — TS↔Rust parity runner + test | Part 5 Task 12 | covered |
| 4.4.0 — CI job in `rust-host.yml` | Part 5 Task 12 | covered |
| 4.4.0 — Known-gaps entry | Part 5 Task 12 | covered |

---

## Cross-File Final Review

- [ ] 1. Spec-coverage table maps every Phase 4.4.0 requirement to a part/task; no GAP.
- [ ] 2. Placeholder scan: no TODO/TBD, no deferred-by-dependency excuses across all 5 part files.
- [ ] 3. No phantom tasks: every task produces a verifiable change; no `--allow-empty`.
- [ ] 4. Dependency soundness:
  - Part 1 Task 1 has no deps.
  - Part 1 Tasks 2–3 depend on Task 1.
  - Part 2 Task 4 depends on Part 1 Task 2 (`WorkspaceConfig`).
  - Part 2 Task 5 depends on Task 4.
  - Part 3 Task 6 depends on Part 1 Task 1.
  - Part 3 Task 7 depends on Task 6.
  - Part 4 Tasks 8–10 depend on Part 1 Task 1 (and Task 10 uses `kaos-rs`).
  - Part 5 Task 11 depends on Part 1–4.
  - Part 5 Task 12 depends on Task 11.
- [ ] 5. Caller & build soundness: Part 5 Task 11 Step 11 changes `rg_locator::find_existing_rg` signature; that same step updates `resolve_rg_path` and internal test callers. No other crate calls `find_existing_rg` before this plan (verify with `grep -rn "find_existing_rg" rust-ody/`).
- [ ] 6. Test-the-risk: every state-mutating helper has behavioral asserts in its part file; Part 5 adds cross-implementation parity asserts on the same fixtures.
- [ ] 7. Type consistency: all helper signatures and JSON shapes referenced in Part 5 match the definitions in Parts 1–4.
