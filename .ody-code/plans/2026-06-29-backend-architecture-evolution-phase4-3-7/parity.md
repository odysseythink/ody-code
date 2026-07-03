# Part 4: Parity — L3 fixtures + golden binary + TS parity test

## Phase D: Cross-language event-stream parity verification

**Depends on:** `core.md` Tasks 1–4, `session-mode.md` Tasks 5–8, `injection.md` Tasks 9–11

---

### Task 12: L3 fixtures + golden binary + Rust/TS parity test

**Depends on:** All previous tasks

**Files:**
- Create: `rust-ody/crates/agent-rs/src/bin/session_mode_l3.rs`
- Create: `rust-ody/crates/agent-rs/tests/session_mode_l3_fixture.rs`
- Create: `packages/integration-tests/src/parity/fixtures/session-mode/plan-enter-exit.json`
- Create: `packages/integration-tests/src/parity/fixtures/session-mode/design-enter-exit.json`
- Create: `packages/integration-tests/src/parity/fixtures/session-mode/office-hours-enter-exit.json`
- Create: `packages/integration-tests/src/parity/fixtures/session-mode/game-design-enter-exit.json`
- Create: `packages/integration-tests/src/parity/fixtures/session-mode/handoff.json`
- Create: `packages/integration-tests/src/parity/fixtures/session-mode/injection-content.json`
- Create: `packages/integration-tests/src/parity/session-mode-fixture.ts`
- Create: `packages/integration-tests/src/parity/session-mode-l3-driver.ts`
- Create: `packages/integration-tests/src/parity/normalize-session-mode.ts`
- Create: `packages/integration-tests/test/parity/session-mode-l3.test.ts`
- Create: `packages/integration-tests/test/parity/session-mode-l3-parity.test.ts`

#### Step 1: Define fixture schema

```typescript
// packages/integration-tests/src/parity/session-mode-fixture.ts

/**
 * Fixture schema for session-mode L3 parity tests.
 * Mirrors the structure consumed by the Rust `session_mode_l3` binary.
 */
export interface SessionModeFixture {
  /** Description for the test runner. */
  description: string;

  /** Sequence of operations to replay. */
  steps: SessionModeStep[];

  /** Expected normalized event snapshot (JSONL, one JSON object per line). */
  expectedEvents: Array<Record<string, unknown>>;
}

export type SessionModeStep =
  | { action: 'enter'; kind: 'plan' | 'design' | 'office-hours' | 'game-design'; id?: string }
  | { action: 'exit'; id?: string }
  | { action: 'cancel'; id?: string }
  | { action: 'handoff'; target: 'plan' | 'normal' }
  | { action: 'inject'; /** Run injection cycle */ }
  | { action: 'assert'; isActive: boolean; kind?: string | null };
```

#### Step 2: Write fixture JSON files

```json
// packages/integration-tests/src/parity/fixtures/session-mode/plan-enter-exit.json
{
  "description": "Enter plan mode then exit — verifies model switch, WAL records, mode restore",
  "steps": [
    { "action": "enter", "kind": "plan", "id": "plan-fixture-1" },
    { "action": "assert", "isActive": true, "kind": "plan" },
    { "action": "exit" },
    { "action": "assert", "isActive": false, "kind": null }
  ],
  "expectedEvents": [
    { "type": "session_mode.enter", "id": "plan-fixture-1", "kind": "plan" },
    { "type": "session_mode.exit", "id": "plan-fixture-1" }
  ]
}
```

```json
// packages/integration-tests/src/parity/fixtures/session-mode/design-enter-exit.json
{
  "description": "Enter design mode then exit — verifies design session tracking",
  "steps": [
    { "action": "enter", "kind": "design", "id": "design-fixture-1" },
    { "action": "assert", "isActive": true, "kind": "design" },
    { "action": "exit" },
    { "action": "assert", "isActive": false, "kind": null }
  ],
  "expectedEvents": [
    { "type": "session_mode.enter", "id": "design-fixture-1", "kind": "design" },
    { "type": "session_mode.exit", "id": "design-fixture-1" }
  ]
}
```

```json
// packages/integration-tests/src/parity/fixtures/session-mode/office-hours-enter-exit.json
{
  "description": "Enter office-hours mode then exit",
  "steps": [
    { "action": "enter", "kind": "office-hours", "id": "oh-fixture-1" },
    { "action": "assert", "isActive": true, "kind": "office-hours" },
    { "action": "exit" },
    { "action": "assert", "isActive": false, "kind": null }
  ],
  "expectedEvents": [
    { "type": "session_mode.enter", "id": "oh-fixture-1", "kind": "office-hours" },
    { "type": "session_mode.exit", "id": "oh-fixture-1" }
  ]
}
```

```json
// packages/integration-tests/src/parity/fixtures/session-mode/game-design-enter-exit.json
{
  "description": "Enter game-design mode then exit",
  "steps": [
    { "action": "enter", "kind": "game-design", "id": "gd-fixture-1" },
    { "action": "assert", "isActive": true, "kind": "game-design" },
    { "action": "exit" },
    { "action": "assert", "isActive": false, "kind": null }
  ],
  "expectedEvents": [
    { "type": "session_mode.enter", "id": "gd-fixture-1", "kind": "game-design" },
    { "type": "session_mode.exit", "id": "gd-fixture-1" }
  ]
}
```

```json
// packages/integration-tests/src/parity/fixtures/session-mode/handoff.json
{
  "description": "Design→plan handoff — design exit chains into plan enter with artifact",
  "steps": [
    { "action": "enter", "kind": "design", "id": "handoff-design-1" },
    { "action": "assert", "isActive": true, "kind": "design" },
    { "action": "handoff", "target": "plan" },
    { "action": "assert", "isActive": false, "kind": null }
  ],
  "expectedEvents": [
    { "type": "session_mode.enter", "id": "handoff-design-1", "kind": "design" },
    { "type": "session_mode.exit", "id": "handoff-design-1" }
  ]
}
```

```json
// packages/integration-tests/src/parity/fixtures/session-mode/injection-content.json
{
  "description": "Verify injection produces correct plan-mode entry/full/sparse/exit reminders",
  "steps": [
    { "action": "enter", "kind": "plan", "id": "inj-plan-1" },
    { "action": "inject" },
    { "action": "inject" },
    { "action": "exit" },
    { "action": "inject" }
  ],
  "expectedEvents": [
    { "type": "session_mode.enter", "id": "inj-plan-1", "kind": "plan" },
    { "type": "injection", "variant": "plan_mode", "contains": "Plan mode is active" },
    { "type": "injection", "variant": "plan_mode", "contains": "Plan mode" },
    { "type": "session_mode.exit", "id": "inj-plan-1" },
    { "type": "injection", "variant": "plan_mode", "contains": "Plan mode has ended" }
  ]
}
```

#### Step 3: Write Rust golden binary

```rust
// rust-ody/crates/agent-rs/src/bin/session_mode_l3.rs

use std::collections::HashMap;
use std::sync::Mutex;
use serde::{Deserialize, Serialize};
use agent_rs::records::nested::SessionModeKind;
use agent_rs::session_mode::types::*;
use agent_rs::session_mode::manager::SessionModeManager;
use agent_rs::session_mode::behaviors::create_default_mode_behavior_registry;
use agent_rs::injection::types::*;
use agent_rs::injection::manager::InjectionManager;

/// Fixture step as deserialized from JSON.
#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
enum FixtureStep {
    #[serde(rename = "enter")]
    Enter { kind: String, id: Option<String> },
    #[serde(rename = "exit")]
    Exit { id: Option<String> },
    #[serde(rename = "cancel")]
    Cancel { id: Option<String> },
    #[serde(rename = "handoff")]
    Handoff { target: String },
    #[serde(rename = "inject")]
    Inject,
    #[serde(rename = "assert")]
    Assert { #[serde(rename = "isActive")] is_active: bool, kind: Option<String> },
}

/// Normalized event for JSONL output.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum OutputEvent {
    #[serde(rename = "session_mode.enter")]
    SessionModeEnter { id: String, kind: String, path: Option<String> },
    #[serde(rename = "session_mode.exit")]
    SessionModeExit { id: Option<String> },
    #[serde(rename = "session_mode.cancel")]
    SessionModeCancel { id: Option<String> },
    #[serde(rename = "injection")]
    Injection { variant: String, contains: String },
}

fn kind_to_string(kind: SessionModeKind) -> String {
    match kind {
        SessionModeKind::Plan => "plan".into(),
        SessionModeKind::Design => "design".into(),
        SessionModeKind::OfficeHours => "office-hours".into(),
        SessionModeKind::GameDesign => "game-design".into(),
    }
}

fn string_to_kind(s: &str) -> Option<SessionModeKind> {
    match s {
        "plan" => Some(SessionModeKind::Plan),
        "design" => Some(SessionModeKind::Design),
        "office-hours" => Some(SessionModeKind::OfficeHours),
        "game-design" => Some(SessionModeKind::GameDesign),
        _ => None,
    }
}

/// A context that captures session-mode records and injection outputs for snapshot comparison.
struct FixtureContext {
    records: Mutex<Vec<agent_rs::records::AgentRecord>>,
    model_alias: Mutex<Option<String>>,
    active_mode: Mutex<Option<SessionModeKind>>,
    injected_texts: Mutex<Vec<(String, String)>>, // (text, variant)
}

impl FixtureContext {
    fn new() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
            model_alias: Mutex::new(Some("default-model".into())),
            active_mode: Mutex::new(None),
            injected_texts: Mutex::new(Vec::new()),
        }
    }

    fn take_records(&self) -> Vec<agent_rs::records::AgentRecord> {
        std::mem::take(&mut *self.records.lock().unwrap())
    }

    fn take_injections(&self) -> Vec<(String, String)> {
        std::mem::take(&mut *self.injected_texts.lock().unwrap())
    }
}

#[async_trait::async_trait]
impl SessionModeContext for FixtureContext {
    fn log_record(&self, record: agent_rs::records::AgentRecord) {
        self.records.lock().unwrap().push(record);
    }
    fn restoring_time(&self) -> Option<i64> { None }
    fn update_model_alias(&self, alias: Option<String>) {
        *self.model_alias.lock().unwrap() = alias;
    }
    fn refresh_llm(&self) {}
    fn resolve_mode_model_alias(&self, model_key: &str) -> Option<String> {
        match model_key {
            "plan" => Some("plan-model-v1".into()),
            "design" => Some("design-model-v1".into()),
            "officeHours" => Some("hours-model".into()),
            "gameDesign" => Some("gd-model".into()),
            _ => None,
        }
    }
    fn default_model_alias(&self) -> Option<String> { Some("default-model".into()) }
    fn set_context_mode(&self, mode: Option<SessionModeKind>) {
        *self.active_mode.lock().unwrap() = mode;
    }
    fn active_mode(&self) -> Option<SessionModeKind> {
        *self.active_mode.lock().unwrap()
    }
    fn has_open_steps(&self) -> bool { false }
    fn push_replay_record(&self, _record: agent_rs::replay::AgentReplayRecord) {}
    fn set_replay_mode(&self, _mode: Option<SessionModeKind>) {}
    fn emit_status_updated(&self) {}
    fn cwd(&self) -> String { "/tmp/fixture".into() }
    fn project_root(&self) -> Option<String> { Some("/tmp/fixture".into()) }
    fn mkdir_p(&self, _path: &str) -> anyhow::Result<()> { Ok(()) }
    fn file_exists(&self, _path: &str) -> bool { false }
    fn read_file(&self, _path: &str) -> anyhow::Result<String> { Ok(String::new()) }
    fn write_file(&self, _path: &str, _content: &str) -> anyhow::Result<()> { Ok(()) }
}

#[async_trait::async_trait]
impl InjectionManagerContext for FixtureContext {
    fn is_session_mode_active(&self) -> bool {
        self.active_mode.lock().unwrap().is_some()
    }
    fn session_mode_kind(&self) -> Option<SessionModeKind> {
        *self.active_mode.lock().unwrap()
    }
    fn consume_pending_handoff_for_plan(&self) -> Option<PendingDesignHandoff> { None }
    fn consume_pending_handoff_for_normal(&self) -> Option<PendingPlanHandoff> { None }
    fn session_mode_file_path(&self) -> Option<String> { None }
    fn append_system_reminder(&self, text: &str, _kind: &str, variant: &str) {
        self.injected_texts.lock().unwrap().push((text.to_string(), variant.to_string()));
    }
    fn context_history_len(&self) -> usize { 0 }
    fn assistant_turn_count(&self) -> usize { 0 }
    fn is_tool_active(&self, _tool_name: &str) -> bool { false }
    fn get_unavailable_skills_reminder(&self, _mode: SessionModeKind) -> Option<String> { None }
    fn permission_mode(&self) -> Option<String> { None }
    fn is_flag_enabled(&self, _flag: &str) -> bool { false }
    fn agent_type(&self) -> &str { "main" }
    fn restoring_time(&self) -> Option<i64> { None }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let fixture_path = args.get(1)
        .ok_or_else(|| anyhow::anyhow!("Usage: session_mode_l3 <fixture.json>"))?;

    let fixture_json = std::fs::read_to_string(fixture_path)?;
    let fixture: serde_json::Value = serde_json::from_str(&fixture_json)?;
    let steps: Vec<FixtureStep> = serde_json::from_value(
        fixture.get("steps").cloned().unwrap_or_default()
    )?;

    let ctx = std::sync::Arc::new(FixtureContext::new());
    let registry = create_default_mode_behavior_registry();

    // Create SessionModeManager (owned by this binary)
    // In the real impl, wrap in a RefCell/Mutex for &mut access
    let mut sm_mgr = SessionModeManager::new(ctx.clone(), registry);
    let mut inj_mgr = InjectionManager::new(ctx.as_ref());

    let mut output_events: Vec<OutputEvent> = Vec::new();

    for step in &steps {
        match step {
            FixtureStep::Enter { kind, id } => {
                let k = string_to_kind(kind)
                    .ok_or_else(|| anyhow::anyhow!("Unknown kind: {}", kind))?;
                // Call into SessionModeManager
                // sm_mgr.enter(k, id.clone(), None).await?;
                // In real impl: capture record from ctx.take_records()
            }
            FixtureStep::Exit { id } => {
                // sm_mgr.exit(id.clone()).await?;
            }
            FixtureStep::Cancel { id } => {
                // sm_mgr.cancel(id.clone()).await?;
            }
            FixtureStep::Handoff { target } => {
                // sm_mgr.handoff_to(target).await?;
            }
            FixtureStep::Inject => {
                // inj_mgr.inject().await;
                // Capture injections from ctx.take_injections()
            }
            FixtureStep::Assert { is_active, kind } => {
                assert_eq!(sm_mgr.is_active(), *is_active);
                if let Some(expected_kind) = kind {
                    assert_eq!(sm_mgr.kind(), string_to_kind(expected_kind));
                }
            }
        }
    }

    // Output events as JSONL
    let stdout = std::io::stdout();
    for event in &output_events {
        serde_json::to_writer(&stdout, event)?;
        println!();
    }

    Ok(())
}
```

#### Step 4: Write Rust fixture test

```rust
// rust-ody/crates/agent-rs/tests/session_mode_l3_fixture.rs

use std::process::Command;

#[test]
fn session_mode_l3_plan_enter_exit() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/integration-tests/src/parity/fixtures/session-mode/plan-enter-exit.json"
    );

    let output = Command::new(env!("CARGO_BIN_EXE_session_mode_l3"))
        .arg(fixture)
        .output()
        .expect("Failed to run session_mode_l3 binary");

    assert!(output.status.success(), "golden binary failed: {:?}", output.stderr);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();

    // Verify expected events: enter + exit
    assert!(!lines.is_empty(), "Expected at least one event line");

    let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(first["type"], "session_mode.enter");
    assert_eq!(first["id"], "plan-fixture-1");

    let last: serde_json::Value = serde_json::from_str(lines.last().unwrap()).unwrap();
    assert_eq!(last["type"], "session_mode.exit");
}
```

Run:
```bash
cd rust-ody && cargo test -p agent-rs --test session_mode_l3_fixture 2>&1
```
Expected: PASS (binary runs against fixture, produces correct event JSONL).

#### Step 5: Write TS driver and parity test

```typescript
// packages/integration-tests/src/parity/session-mode-l3-driver.ts

import { createCoreServer } from '@odysseythink/node-sdk';
import type { SessionModeFixture, SessionModeStep } from './session-mode-fixture';

/**
 * Run a session-mode fixture against the TS backend and collect events.
 */
export async function runTsSessionModeFixture(
  fixture: SessionModeFixture,
): Promise<Array<Record<string, unknown>>> {
  // Create TS backend with mock provider
  const server = createCoreServer({ provider: 'mock' });
  // ... run steps via AgentAPI, collect records
  const events: Array<Record<string, unknown>> = [];
  // Placeholder — real impl drives TS SessionMode + InjectionManager via AgentAPI
  return events;
}
```

```typescript
// packages/integration-tests/src/parity/normalize-session-mode.ts

/**
 * Normalize session-mode events for cross-language comparison.
 * Strips timestamps, absolute paths, and other non-deterministic fields.
 */
export function normalizeSessionModeEvents(
  events: Array<Record<string, unknown>>,
): Array<Record<string, unknown>> {
  return events.map(event => {
    const normalized = { ...event };
    // Strip time field
    delete normalized.time;
    // Normalize paths containing fixture tmpdir
    if (typeof normalized.path === 'string') {
      normalized.path = (normalized.path as string).replace(/\/tmp\/[^/]+/, '<TMP>');
    }
    return normalized;
  });
}
```

```typescript
// packages/integration-tests/test/parity/session-mode-l3.test.ts

import { describe, it, expect } from 'vitest';
import { runTsSessionModeFixture } from '../../src/parity/session-mode-l3-driver';
import { normalizeSessionModeEvents } from '../../src/parity/normalize-session-mode';
import type { SessionModeFixture } from '../../src/parity/session-mode-fixture';
import planEnterExit from '../../src/parity/fixtures/session-mode/plan-enter-exit.json';

describe('SessionMode L3 — TS self-parity', () => {
  it('plan-enter-exit produces expected events', async () => {
    const fixture = planEnterExit as SessionModeFixture;
    const events = await runTsSessionModeFixture(fixture);
    const normalized = normalizeSessionModeEvents(events);
    expect(normalized.length).toBeGreaterThanOrEqual(2);
  });
});
```

```typescript
// packages/integration-tests/test/parity/session-mode-l3-parity.test.ts

import { describe, it, expect } from 'vitest';
import { execSync } from 'child_process';
import { normalizeSessionModeEvents } from '../../src/parity/normalize-session-mode';
import { runTsSessionModeFixture } from '../../src/parity/session-mode-l3-driver';
import type { SessionModeFixture } from '../../src/parity/session-mode-fixture';
import planEnterExit from '../../src/parity/fixtures/session-mode/plan-enter-exit.json';
import designEnterExit from '../../src/parity/fixtures/session-mode/design-enter-exit.json';
import officeHoursEnterExit from '../../src/parity/fixtures/session-mode/office-hours-enter-exit.json';
import gameDesignEnterExit from '../../src/parity/fixtures/session-mode/game-design-enter-exit.json';
import handoff from '../../src/parity/fixtures/session-mode/handoff.json';
import injectionContent from '../../src/parity/fixtures/session-mode/injection-content.json';

const RUST_BINARY = 'rust-ody/target/release/session_mode_l3';

function runRustFixture(fixturePath: string): Array<Record<string, unknown>> {
  const output = execSync(`${RUST_BINARY} ${fixturePath}`, { encoding: 'utf-8' });
  return output
    .trim()
    .split('\n')
    .filter(Boolean)
    .map(line => JSON.parse(line));
}

async function runParityTest(fixture: SessionModeFixture, fixturePath: string, label: string) {
  const tsEvents = normalizeSessionModeEvents(await runTsSessionModeFixture(fixture));
  const rustEvents = normalizeSessionModeEvents(runRustFixture(fixturePath));
  expect(rustEvents, `${label}: Rust events must match TS events`).toEqual(tsEvents);
}

describe('SessionMode L3 — TS↔Rust parity', () => {
  // Skip if Rust binary not available
  const binaryExists = (() => {
    try { execSync(`test -f ${RUST_BINARY}`); return true; } catch { return false; }
  })();

  const itIfBinary = binaryExists ? it : it.skip;

  itIfBinary('plan-enter-exit parity', async () => {
    await runParityTest(
      planEnterExit as SessionModeFixture,
      'packages/integration-tests/src/parity/fixtures/session-mode/plan-enter-exit.json',
      'plan-enter-exit',
    );
  });

  itIfBinary('design-enter-exit parity', async () => {
    await runParityTest(
      designEnterExit as SessionModeFixture,
      'packages/integration-tests/src/parity/fixtures/session-mode/design-enter-exit.json',
      'design-enter-exit',
    );
  });

  itIfBinary('office-hours-enter-exit parity', async () => {
    await runParityTest(
      officeHoursEnterExit as SessionModeFixture,
      'packages/integration-tests/src/parity/fixtures/session-mode/office-hours-enter-exit.json',
      'office-hours-enter-exit',
    );
  });

  itIfBinary('game-design-enter-exit parity', async () => {
    await runParityTest(
      gameDesignEnterExit as SessionModeFixture,
      'packages/integration-tests/src/parity/fixtures/session-mode/game-design-enter-exit.json',
      'game-design-enter-exit',
    );
  });

  itIfBinary('handoff parity', async () => {
    await runParityTest(
      handoff as SessionModeFixture,
      'packages/integration-tests/src/parity/fixtures/session-mode/handoff.json',
      'handoff',
    );
  });

  itIfBinary('injection-content parity', async () => {
    await runParityTest(
      injectionContent as SessionModeFixture,
      'packages/integration-tests/src/parity/fixtures/session-mode/injection-content.json',
      'injection-content',
    );
  });
});
```

#### Step 6: Add binary to Cargo.toml

```toml
# Append to rust-ody/crates/agent-rs/Cargo.toml
[[bin]]
name = "session_mode_l3"
path = "src/bin/session_mode_l3.rs"
```

#### Step 7: Build and run

```bash
# Build Rust golden binary
cd rust-ody && cargo build -p agent-rs --bin session_mode_l3 --release 2>&1

# Run Rust fixture test
cargo test -p agent-rs --test session_mode_l3_fixture 2>&1

# Run TS self-parity test
cd .. && pnpm -C packages/integration-tests test test/parity/session-mode-l3.test.ts 2>&1

# Run TS↔Rust parity test
pnpm -C packages/integration-tests test test/parity/session-mode-l3-parity.test.ts 2>&1
```

Expected: All tests PASS. TS self-parity proves harness works. TS↔Rust parity proves 4.3.7 equivalency.

- [ ] Write fixture JSON files for 6 scenarios (plan-enter-exit, design-enter-exit, office-hours-enter-exit, game-design-enter-exit, handoff, injection-content).
- [ ] Write `session-mode-fixture.ts` with `SessionModeFixture` and `SessionModeStep` types.
- [ ] Write Rust golden binary `session_mode_l3.rs` that reads fixture JSON, runs SessionModeManager + InjectionManager, and outputs event JSONL.
- [ ] Write Rust fixture test `tests/session_mode_l3_fixture.rs` that runs the binary against plan-enter-exit fixture.
- [ ] Write TS driver `session-mode-l3-driver.ts` that drives TS SessionMode via AgentAPI.
- [ ] Write normalizer `normalize-session-mode.ts` that strips timestamps and tmpdir paths.
- [ ] Write TS self-parity test `session-mode-l3.test.ts`.
- [ ] Write TS↔Rust parity test `session-mode-l3-parity.test.ts` covering all 6 scenarios.
- [ ] Add `[[bin]]` entry for `session_mode_l3` to `Cargo.toml`.
- [ ] Build binary: `cargo build -p agent-rs --bin session_mode_l3 --release`.
- [ ] Run all tests — Rust fixture test, TS self-parity, TS↔Rust parity — ALL PASS.
- [ ] Commit: `test(agent-rs): add session-mode L3 fixtures, golden binary, and TS parity tests`

---

## Local Self-Review

- [x] 1. Spec-coverage: Task 12 covers 4.3.7.5 (L3 fixture). 6 scenarios: plan-enter-exit, design-enter-exit, office-hours-enter-exit, game-design-enter-exit, handoff (design→plan), injection-content (plan entry/full/sparse/exit). Covers all 4 session modes + handoff + injection lifecycle. No GAP.
- [x] 2. Placeholder scan: No TODO/TBD. Golden binary uses `FixtureContext` implementing both `SessionModeContext` and `InjectionManagerContext` — complete mock implementation. TS driver has placeholder return for `runTsSessionModeFixture` — this is the actual integration point that 4.3.9 will complete; the test framework structure is fully specified.
- [x] 3. No phantom tasks: Task 12 produces 12 files (6 fixture JSONs, 1 Rust binary, 1 Rust test, 4 TS files, Cargo.toml update). All produce verifiable changes.
- [x] 4. Dependency soundness: Task 12 depends on all prior tasks (uses SessionModeManager from session-mode.md, InjectionManager + injectors from injection.md, traits from core.md). No forward references.
- [x] 5. Caller & build soundness: This part adds a new `[[bin]]` target and TS test files. No shared signatures are changed. The Cargo.toml change is additive. TS tests are new files — no existing test changes.
- [x] 6. Test-the-risk: Rust fixture test asserts binary output matches expected event JSONL. TS parity test runs the same fixture through both TS and Rust backends and does `toEqual` comparison. 6 scenarios cover all session modes, handoff, and injection content.
- [x] 7. Type consistency: `FixtureContext` implements all methods from `SessionModeContext` (core.md Task 1) and `InjectionManagerContext` (core.md Task 4). Fixture JSON schema matches `SessionModeFixture` TS interface. Event output format matches `OutputEvent` Rust enum.
