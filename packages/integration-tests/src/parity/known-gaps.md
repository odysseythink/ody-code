# Parity Known Gaps

> Last reviewed: 2026-07-01
> Phase: 4.5.0
> Decision authority: Phase 4 Rust-host migration review

This document is the single source of truth for gaps discovered during Phase 4 parity testing. Every gap has a final-state decision:

- **A** — Migrate in Phase 4.5.x (blocks TS dual deletion until complete).
- **B** — Permanent TS callback (TS implementation stays; Rust host routes or defers to TS).
- **C** — Out of Phase 4 (tracked elsewhere; does not block deletion).

## Gap Registry

| ID | Module | Gap | Layer | Decision | Owner | Acceptance Criteria |
|---|---|---|---|---|---|---|
| G1 | kaos | SSH remote execution not implemented in Rust | L1/L2 | B | ts-core-team | Hybrid-backend path documented and tested; permanent callback. |
| G2 | kaos | `readText` strict/replace/ignore error-mode alignment | L1 | A | rust-host-team | `fixtures/kaos/l1-text-decode.json` passes TS↔Rust; divergent cases downgraded to B. |
| G3 | kosong | KimiFiles / video uploader | L1/L2 | A | rust-host-team | L1 fixture green or explicitly scoped to C. |
| G4 | kosong | Provider-specific unresolved gaps (per-provider) | L1 | A/B/C per provider | rust-host-team | Each provider green L1; failures become B or C. |
| G5 | agent | Compaction tokenizer alignment | L1/L3 | C | phase-1-a-team | Tracked in Phase 1-A; Phase 4 uses mock/snapshot fallback. |
| G6 | agent | `SessionGoalStore` not split in agent-rs | L2/L3 | A | rust-host-team | Ported to agent-rs; goal/state tools pass L2/L3. |
| G7 | tools | `RequestCodeReviewTool` in external `@odysseythink/code-review` | L1/L3 | B | ts-core-team | Remains TS; Rust host can invoke via TS worker RPC. |
| G8 | tools | Design/product/game-design artifact sync (`rpc.openExternal`, gbrain CLI) | L3 | B | ts-core-team | Host exposes required RPC bridge; TS tools remain canonical. |
| G9 | parity | L4 cross-host resume not exercised end-to-end | L4 | A | rust-host-team | `resume-cross-host.ts` passes TS→Rust→TS and joins CI. |
| G10 | parity | Session lifecycle event type mismatch (`session.created`/`session.closed` vs `agent.status.updated`) | L3 | A | rust-host-team | Event streams match after normalization. |
| G11 | parity | Rust mock provider missing `turn.ended` | L3 | A | rust-host-team | `turn.ended` emitted; hello-world/mock-prompt/file-edit/multi-turn-tool pass. |
| G12 | parity | Background/cron host RPC path not wired | L3 | A | rust-host-team | `background-cron` L3 scenario passes. |
| G13 | tools | Bash tool-call scenario ends with text instead of calling Bash | L3 | A | rust-host-team | Bash tool-call L3 scenario passes. |
| G14 | tools | Web-search user registration path not fully wired | L3 | A/B | rust-host-team | Wired in Rust or documented TS fallback. |
| G15 | tools | `tools-rs/list-directory` error text divergence on unreadable dir | L1 | C | rust-host-team | Documented delta; no fixture required. |
| G16 | tools | `tools-rs/rg-locator` download branch not covered in CI | L1 | C | rust-host-team | Documented limitation; fixture stays local-lookup. |

## Summary by Decision

| Decision | Count | Gap IDs |
|---|---|---|
| A (migrate in 4.5.x) | 9 | G2, G3, G4-A, G6, G9, G10, G11, G12, G13 |
| B (permanent TS callback) | 4 | G1, G4-B, G7, G8 |
| C (out of Phase 4) | 4 | G5, G4-C, G15, G16 |

## Legacy Skip List (harness-readable)

> The table below is intentionally kept in the old `| Scenario | Layer | Reason |` format so that `known-gaps.ts::parseKnownGaps` can still read it. These rows correspond to the L3 scenarios that are currently skipped by the parity harness.

| Scenario | Layer | Reason |
|---|---|---|
| hello-world | L3 | Rust backend mock provider does not emit `turn.ended`, scenario waits until timeout |
| mock-prompt | L3 | Rust backend mock provider does not emit `turn.ended`, scenario waits until timeout |
| file-edit | L3 | Rust backend mock provider does not emit `turn.ended`, scenario waits until timeout |
| multi-turn-tool | L3 | Rust backend mock provider does not emit `turn.ended`, scenario waits until timeout |
| session-lifecycle | L3 | Rust backend `session.created` / `session.closed` events differ from TS `agent.status.updated` event types |
| set-model | L3 | Rust backend `session.created` / `session.closed` events differ from TS `agent.status.updated` event types |
| host-config | L3 | Rust backend `session.created` / `session.closed` events differ from TS `agent.status.updated` event types |
| session-mode-handoff | L3 | Rust backend session-mode enter/exit is a prototype stub; plan state is always null |
| background-cron | L3 | Rust backend background/cron subsystem host RPC path not wired; `getBackground` returns empty array |
| web-search | L3 | Rust user-registered tool execution path not fully wired; TS side echoed by parity harness |
| bash-tool-call | L3 | Rust mock provider ends tool-call turn with text instead of invoking Bash; TS side executes Bash normally |
| * | L4 | Records cross-host resume end-to-end not yet exercised in CI |
| ssh backend | L1/L2 | SSH remote execution deferred to post-Phase-4; local process/env/kaos backends only |
| tools-rs/list-directory | L1 | Error text may differ when directory is unreadable; ordering already aligned |
| tools-rs/rg-locator | L1 | `find_existing_rg` download branch not covered in CI; fixture covers local-lookup path only |
