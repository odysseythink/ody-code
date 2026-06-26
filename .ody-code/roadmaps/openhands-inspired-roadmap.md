# OpenHands-Inspired Engineering-Practice Roadmap for ody-code

**Document Type**: Product Roadmap (research-derived)
**Last Updated**: 2026-06-18
**Status**: ACTIVE — T1-A ✅ COMPLETED (all 4 phases)
**Source Study**: OpenHands 1.8.0 (`~/Downloads/OpenHands-1.8.0`)
**Epic Owner**: TBD

---

## 📋 Executive Summary

**Objective**: Extract design ideas from OpenHands 1.8.0 that measurably improve **software-engineering
practice** inside ody-code (a local TypeScript coding-agent CLI), and sequence them into an
actionable, tiered roadmap.

**Why now**: A concrete pain motivated this study — when asked to build three page modules, ody-code's
AI duplicated highly-similar front/back-end components three times instead of reusing or extending the
existing ones. The agent had no durable, repo-specific memory of "what already exists and how we reuse
it." OpenHands solves a family of these problems with lightweight, **keyword-triggered repo knowledge**,
**repo bootstrap/verification hooks**, **loop detection**, and **risk-tiered execution** — all of which
map cleanly onto ody-code's existing architecture.

**Value Proposition**:
- ✅ **DONE — T1-A** Stop silent duplication — encode reuse conventions & repo facts the agent auto-recalls.
- ✅ Close the engineering loop — bootstrap the environment and run project checks before committing.
- ✅ Cut wasted cost — detect and break stuck/looping behavior instead of burning tokens.
- ✅ Safer autonomy — risk-tiered confirmation and optional containerized execution.

**Filtering criterion**: ody-code is a **local CLI**. OpenHands' large enterprise/SaaS surface
(multi-tenant org routing, webhooks, hosted runtimes) is explicitly **out of scope** (see Non-Goals).
We keep only ideas that help a single developer working in a single repo.

**Timeline**: ~3–5 months for Tier 1; Tier 2/3 continuous.
**Priority**: Tier 1 = High, Tier 2 = Medium, Tier 3 = Exploratory.

---

## 🔬 Methodology & Source

OpenHands 1.8.0 is structured in two layers:

| Layer | Location in checkout | Relevance to ody-code |
|-------|----------------------|------------------------|
| **App/Enterprise server** | `openhands/app_server/`, `enterprise/` | Mostly **not relevant** — REST proxy, org routing, webhooks, multi-tenant SaaS, Redis caching. |
| **Agent runtime (SDK)** | pinned deps `openhands-sdk==1.27.0`, `openhands-agent-server==1.27.0` (not vendored in-tree) | **Relevant** — controller, condenser, stuck detection, security analyzer. Studied via OpenHands' documented behavior + the artifacts the SDK consumes (microagents, hooks). |
| **Repo-facing artifacts** | `skills/*.md`, `.openhands/{setup.sh,pre-commit.sh,microagents/}` | **Directly relevant & vendored** — concrete, copyable patterns. |

The strongest, best-grounded transferable ideas are the **repo-facing artifacts**, because they live in
the checkout and translate almost 1:1 to ody-code's existing skill/injection systems.

**Evidence grounding (paths under `~/Downloads/OpenHands-1.8.0`):**
- Knowledge microagent w/ keyword triggers: `skills/github.md` (frontmatter `type: knowledge`,
  `triggers: [github, git]`), `skills/security.md`, `skills/code-review.md`, `skills/fix_test.md`.
- Repo-local microagents: `.openhands/microagents/documentation.md`, `.openhands/microagents/glossary.md`.
- Environment bootstrap: `.openhands/setup.sh` (installs `pre-commit`, runs `pre-commit install`).
- Commit-time verification: `.openhands/pre-commit.sh`.

**ody-code current state (gap framing)** — confirmed strong: session checkpoints
(`packages/agent-core/src/session/checkpoint/`), plan/design modes
(`packages/agent-core/src/agent/injection/`), compaction (`packages/agent-core/src/agent/compaction/`),
MCP (`packages/agent-core/src/mcp/`), sub-agents
(`packages/agent-core/src/tools/builtin/collaboration/agent.ts`), ripgrep search, E2E-testing phase 1
(`packages/agent-core/src/e2e-testing/`), **repo knowledge microagents (T1-A ✅)**. Confirmed **missing**:
repo bootstrap/verify hooks, loop detection, action risk scoring, container sandbox, GitHub issue→PR
resolver, semantic/repo-map understanding.

---

## 📊 Feature Catalog (ranked)

| # | Candidate | OpenHands evidence | ody-code gap (integration path) | Eng value | Transfer difficulty |
|---|-----------|--------------------|--------------------------------|-----------|---------------------|
| **T1-A** | **Repo Knowledge Microagents** (keyword-triggered facts/conventions) | `skills/github.md`, `.openhands/microagents/*.md` | ✅ **COMPLETED** — parser, scanner, trigger matcher, injection, budgeting, `/microagent` authoring UX, starter pack, docs | ★★★★★ | Low–Med |
| **T1-B** | **Repo Bootstrap & Verification Hooks** | `.openhands/setup.sh`, `.openhands/pre-commit.sh` | No per-repo bootstrap/commit-verify → session start + `permission/` + Bash | ★★★★☆ | Low–Med |
| **T1-C** | **Stuck / Loop Detection** | SDK `StuckDetector` | No loop detection (only max-step) → `loop/run-turn.ts` | ★★★★☆ | Med |
| **T1-D** | **Risk-Tiered Confirmation + Sandbox** | `ConfirmRisky`, LLM security analyzer, Docker runtime | Permission policies but no risk scoring / host-process only → `permission/`, `packages/kaos` | ★★★★☆ | Med–High |
| **T2-A** | **GitHub Issue→PR Resolver / `CreatePR` tool** | `skills/github.md` `create_pr`, resolver svc | Plugin download only → new builtin tool | ★★★☆☆ | Med |
| **T2-B** | **Event-Stream Replay / Reproducible Debugging** | Action/Observation event store | `AgentRecords` append-log exists → extend to deterministic replay | ★★★☆☆ | Med–High |
| **T2-C** | **Richer Condenser Strategies** | `LLMSummarizingCondenser` | Compaction exists → add summarizing variants | ★★☆☆☆ | Low |
| **T3-A** | **Semantic Code Search / Repo-Map** | repo understanding / ranking | Ripgrep only, no symbol/AST map | ★★★☆☆ | High |
| **T3-B** | **Multi-Agent Supervision** | `AgentDelegateAction`, agenthub | Sub-agents exist, no supervisor/parallel coordination | ★★☆☆☆ | High |
| **T3-C** | **Network Isolation** | sandboxed runtime egress control | `FetchURL` unrestricted | ★★☆☆☆ | High |

★ = relative engineering-practice impact, not effort.

---

## 🥇 Tier 1 — Detailed, Phased Designs

### T1-A — Repo Knowledge Microagents (anti-duplication memory)

**Problem it solves**: The motivating bug. The agent has no durable, repo-scoped knowledge of "these
components already exist; reuse/extend them." ody-code **skills** are *procedures* (how to do a task);
microagents are *facts/conventions* injected **only when relevant keywords appear**, keeping token cost
low.

**Design**:
- New artifact: `.ody-code/microagents/*.md` with YAML frontmatter:
  ```markdown
  ---
  name: reuse-conventions
  type: knowledge
  triggers: [component, page, module, 组件, 页面, 复用]
  ---
  Before creating a new UI component, search `src/components/` and `src/shared/`.
  Prefer extending `<DataTable>` / `<FormShell>` over re-implementing. ...
  ```
- A **trigger matcher** scans the latest user/assistant turn for any `triggers` keyword (case-insensitive,
  word-ish boundaries) and injects matched bodies as a transient context block.
- **Reuse existing infra**:
  - `packages/agent-core/src/skill/registry.ts` already discovers/parses `.md` files with frontmatter —
    extend the parser to recognize `type: knowledge` + `triggers`.
  - `packages/agent-core/src/agent/injection/` is the established mechanism for dynamic, mode-aware
    context injection — add a microagent injector here (de-duplicate so the same microagent isn't
    re-injected every turn).
- **Ship a starter pack**: a `reuse-conventions` microagent that directly mitigates the motivating bug,
  plus optional `glossary` and `testing` examples (mirroring `.openhands/microagents/`).

**Phases**:
| Phase | Scope | Status |
|-------|-------|--------|
| A.1 | Parser: recognize `type: knowledge` + `triggers` frontmatter; load from `.ody-code/microagents/`. | ✅ **Done** — `parser.ts` (`parseSkillText`, `parseTriggers`), `scanner.ts` (scans `.ody-code/microagents/`) |
| A.2 | Trigger matcher + injection (with per-session de-dup) wired into `agent/injection/`. | ✅ **Done** — `knowledge-microagent.ts` (`triggerMatches`, `matchKnowledgeMicroagents`, `KnowledgeMicroagentInjector`) |
| A.3 | Precedence & budgeting: cap injected microagent tokens; project > user > builtin precedence. | ✅ **Done** — `sortBySourcePriority`, `applyBudget`, configurable `microagentBudget.maxTokens` (default 1024), telemetry |
| A.4 | Authoring UX: a `/microagent` helper to scaffold a file; doc + starter `reuse-conventions`. | ✅ **Done** — `/microagent` TUI command with 3-step wizard, 4 starter templates, en/zh docs |

**Success criteria**: typing "add a new component/page" surfaces the reuse convention automatically;
microagent bodies are injected only on keyword match; total injected microagent tokens are capped and
visible in telemetry. — ✅ **All criteria met.**

---

### T1-B — Repo Bootstrap & Verification Hooks (engineering closed-loop)

**Problem it solves**: The agent starts "cold" each session (deps not installed, env not prepared) and
can commit code that fails the project's own checks.

**Design**:
- **`.ody-code/setup.sh`** — if present, auto-run once at session start to prepare the environment
  (install deps, generate types, etc.). Mirrors `.openhands/setup.sh`.
  - **Gate it**: route through `packages/agent-core/src/agent/permission/` — in `manual` mode prompt
    once ("Run this repo's setup script?"), in `auto`/`yolo` run automatically. Never run an unreviewed
    script silently in manual mode.
- **Commit-time verification hook** — before the agent finalizes a commit, run the project's checks
  (lint/test/typecheck, e.g. via the repo's `pre-commit` or an `.ody-code/verify.sh`). On failure, feed
  the output back to the agent to fix rather than committing broken work. Mirrors `.openhands/pre-commit.sh`.
  - **Reuse**: the Bash tool (`packages/agent-core/src/tools/builtin/shell/bash.ts`) for execution and
    the existing git-cwd permission policy (`git-cwd-write-approve.ts`) for the commit path.
- **Synergy with E2E roadmap**: the verification hook is the natural place to invoke the existing
  `RunE2ETests` capability (`packages/agent-core/src/e2e-testing/`) — see
  `e2e-testing-automation-roadmap.md`.

**Phases**:
| Phase | Scope |
|-------|-------|
| B.1 | Detect & run `.ody-code/setup.sh` at session start, permission-gated. |
| B.2 | Verification hook on commit path (`.ody-code/verify.sh` or detected `pre-commit`). |
| B.3 | Feedback loop: surface failing check output to the agent + retry budget. |
| B.4 | Config knobs (`config.toml`): enable/disable, timeout, failure policy (warn vs block). |

**Success criteria**: a repo with `.ody-code/setup.sh` prepares itself on first session; a commit that
breaks lint/tests is blocked with the failure surfaced to the agent for repair.

---

### T1-C — Stuck / Loop Detection (reliability & cost control)

**Problem it solves**: ody-code's controller enforces a max-step limit but has **no loop detection**
(confirmed in `packages/agent-core/src/loop/`). An agent can repeat the same failing edit or
read-the-same-file cycle, burning tokens until the cap.

**Design**:
- A lightweight detector in the turn controller (`packages/agent-core/src/loop/run-turn.ts`, leveraging
  the existing `AfterStepHook`) maintaining a short rolling window of recent (tool name + normalized
  args) and (observation signature) pairs.
- **Heuristics (from OpenHands `StuckDetector`)**:
  - N identical tool calls with identical args in a row.
  - Repeating action↔observation cycle (e.g., same failed edit → same error) K times.
  - "Monologue" loops: repeated assistant text with no tool progress.
- **Action on trip**: inject a corrective system note ("You appear stuck repeating X; try a different
  approach or ask the user"), and if it persists, break the loop and escalate via the existing
  user-interaction path. Config-gated thresholds (default conservative to avoid false positives).
- Emit a telemetry event (reuse `packages/agent-core/src/agent/records/` / telemetry) for observability.

**Phases**:
| Phase | Scope |
|-------|-------|
| C.1 | Rolling-window signatures of tool calls + observations in the step hook. |
| C.2 | Identical-call and action/observation-cycle detectors with thresholds. |
| C.3 | Corrective injection + escalation/break; telemetry event. |
| C.4 | Tuning + config (`config.toml`) thresholds; opt-out. |

**Success criteria**: an induced repeat-edit loop is detected within the configured window, the agent is
nudged to change approach, and unbounded repetition no longer reaches the step cap.

---

### T1-D — Risk-Tiered Confirmation & Optional Sandbox (security)

**Problem it solves**: ody-code's permission system is **rule/path-based** but has no notion of *risk
level*, and all execution runs in the **host process**.

**Design**:
- **Risk-tiered confirmation** (`ConfirmRisky` analog): add a policy in the existing decision chain
  (`packages/agent-core/src/agent/permission/policies/`) that classifies a pending action's risk
  (destructive shell ops, writes outside repo, network egress, secret-touching paths) and only prompts
  for **risky** actions — reducing prompt fatigue vs. always-ask while staying safer than auto-approve.
  - Start **rule-based** (cheap, deterministic); optionally add an LLM risk scorer later behind a flag.
  - Reuse `path-access.ts` / `sensitive.ts` signals already present.
- **Optional containerized runtime**: an execution backend behind the **kaos** abstraction
  (`packages/kaos`) that runs Bash/file ops inside a container for untrusted or high-blast-radius work.
  Opt-in; host execution remains the default.

**Phases**:
| Phase | Scope |
|-------|-------|
| D.1 | Rule-based risk classifier + `ConfirmRisky` policy in the permission chain. |
| D.2 | Config: per-mode mapping (manual/auto/yolo) → confirm thresholds. |
| D.3 | (Optional) LLM risk scorer behind a flag. |
| D.4 | (Optional) Containerized kaos backend; opt-in profile for risky sessions. |

**Success criteria**: a destructive command (e.g., `rm -rf` outside repo) triggers a confirm even in
`auto` mode, while routine in-repo edits do not; container backend can run a session end-to-end when
enabled.

---

## 🥈 Tier 2 — Directional

- **T2-A · GitHub Issue→PR Resolver / `CreatePR` tool** — A builtin `CreatePR` tool plus an
  "ingest issue context" helper so ody-code can read an issue, implement, and open a PR. OpenHands wires
  this via `skills/github.md` (`create_pr`) and its resolver. For a local CLI, lean on the `gh` CLI
  rather than reimplementing GraphQL. Value: closes the issue→change→PR loop without leaving the agent.
- **T2-B · Event-Stream Replay / Reproducible Debugging** — OpenHands models every step as a persisted
  Action/Observation event, enabling replay. ody-code already has an append-only `AgentRecords` log
  (`packages/agent-core/src/agent/records/`); extend it toward **deterministic replay** of a session
  for debugging agent behavior and writing regression fixtures.
- **T2-C · Richer Condenser Strategies** — Add an `LLMSummarizingCondenser`-style option to the existing
  compaction strategies (`packages/agent-core/src/agent/compaction/`) for smarter long-session memory.
  Low effort since the compaction framework already exists.

---

## 🥉 Tier 3 — Exploratory

- **T3-A · Semantic Code Search / Repo-Map** — Beyond ripgrep: a symbol/AST index and a generated
  repo-map to improve large-codebase navigation and *reinforce* T1-A's anti-duplication goal (the agent
  can actually find the existing component). High effort (indexing, incremental updates).
- **T3-B · Multi-Agent Supervision** — A supervisor coordinating parallel sub-agents with shared
  context, extending the current single-spawn `Agent` tool.
- **T3-C · Network Isolation** — Egress allow-listing for `FetchURL`/tools, pairing with T1-D's sandbox.

---

## 🧭 Sequencing & Dependencies

```
T1-A  Repo Knowledge Microagents ─┐ (directly fixes the motivating duplication bug; lowest risk)
                                  ├─► T3-A Semantic search amplifies A (find-before-build)
T1-B  Bootstrap & Verify Hooks ───┤
                                  └─► reuses E2E roadmap (RunE2ETests in verify hook)
T1-C  Stuck / Loop Detection ──────► independent; ship anytime (high ROI, isolated to loop/)
T1-D  Risk-Tiered Confirm ─────────► D.1 first (rule-based); D.4 sandbox later, pairs w/ T3-C
```

**Recommended order**: **T1-A ✅ (done) → T1-C → T1-B → T1-D**.
- T1-A first: directly addresses the reported pain, lowest blast radius, reuses skill/injection infra. — ✅ **Completed**.
- T1-C next: isolated to the controller, immediate cost/reliability win.
- T1-B then: introduces script execution, so it benefits from T1-D's risk gating being designed.
- T1-D last in Tier 1: largest surface; start with the cheap rule-based classifier (D.1–D.2), defer the
  container backend (D.4).

**Rough sizing** (engineering-weeks, single dev, indicative):
| Item | Size | Actual |
|------|------|--------|
| T1-A (A.1–A.4) | 2–3 wk | ~2 wk (implemented incrementally across multiple PRs) |
| T1-B (B.1–B.4) | 2–3 wk |
| T1-C (C.1–C.4) | 1.5–2.5 wk |
| T1-D (D.1–D.2) | 1.5–2 wk; D.4 container +2–4 wk |

---

## ⚠️ Risks & Non-Goals

**Non-Goals (explicitly out of scope for a local CLI):**
- Enterprise/SaaS multi-tenancy: org routing (`enterprise/integrations/resolver_org_router.py`),
  webhook automation services, Redis org-claim caching. These solve hosted-platform problems ody-code
  does not have.
- Hosted/cloud runtimes (E2B/Modal) and the app-server↔agent-server REST split.

**Risks & mitigations:**
| Risk | Mitigation |
|------|------------|
| Microagent injection bloats prompt / token cost | **Keyword-gate** injection (only on trigger match), cap total injected tokens, de-dup per session (T1-A.3). |
| `setup.sh`/verify scripts run untrusted code | Route through permission system; prompt in `manual` mode; never auto-run unreviewed scripts there (T1-B.1). |
| Loop detector false positives halt legit iteration | Conservative default thresholds, corrective-nudge-before-break, opt-out config (T1-C.4). |
| Risk classifier over-prompts (fatigue) or under-prompts (unsafe) | Start rule-based + tune per mode; LLM scorer optional behind flag (T1-D). |
| Scope creep into IDE/semantic features | Keep Tier 3 exploratory; gate on Tier 1 outcomes. |

---

## ❓ FAQ

**Q: How is a "microagent" different from an ody-code skill?**
A: Skills are *procedures* the agent chooses to run (how to do X). Microagents are *facts/conventions*
auto-injected when trigger keywords appear (what's true about this repo). They complement each other and
can share the same `.md` + frontmatter parsing.

**Q: Why is the enterprise GitHub/resolver machinery mostly excluded?**
A: It targets a multi-tenant hosted platform. A local CLI gets the same user value far more cheaply by
shelling out to `gh` (T2-A).

**Q: Does this require vendoring OpenHands' SDK?**
A: No. We reimplement the *ideas* (triggered knowledge, hooks, stuck detection, risk tiers) natively on
ody-code's existing skill/injection/permission/loop infrastructure.

**Q: What's the single highest-ROI item?**
A: **T1-A (Repo Knowledge Microagents)** — it directly fixes the motivating duplication bug, has the
lowest blast radius, and reuses infrastructure already present.

---

## 🚀 Next Steps

1. ~~Approve this roadmap (or trim the tier set).~~
2. ~~Run `/plan` on **T1-A** to produce a concrete implementation plan (parser → trigger match → injection
   → starter `reuse-conventions` microagent).~~
3. **T1-A ✅ COMPLETED** — all 4 phases (parser, trigger + injection, budget + precedence, authoring UX) are
   implemented and tested. Gated behind `ODY_CODE_EXPERIMENTAL_REPO_KNOWLEDGE=1`.
4. Sequence **T1-C** and **T1-B** behind it (T1-B benefits from T1-D risk gating).
5. Re-evaluate Tier 2/3 after Tier 1 ships and telemetry is available.

---

## 📖 Related Documents

- `.ody-code/roadmaps/e2e-testing-automation-roadmap.md` — pairs with **T1-B** (verification hook
  invokes `RunE2ETests`).
- Source study: `~/Downloads/OpenHands-1.8.0/skills/`, `~/Downloads/OpenHands-1.8.0/.openhands/`.

---

**Version**: 1.1
**Status**: ACTIVE — T1-A ✅ COMPLETED
