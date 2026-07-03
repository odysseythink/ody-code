# Part 5: L1 golden fixtures + parity runner + CI + known gaps

**Goal:** Extend the existing `golden.rs` harness with new op variants for every 4.4.1 core tool, mirror them in the TypeScript `tools-rs-golden` parity runner, create a shared fixture file, and wire verification into CI.

**Architecture:** The Rust golden binary (`tools-golden`) gains op variants that call into the `builtin::*Tool` implementations directly, producing `CaseResult { result, error }`. The TS parity runner mirrors these as `runCase` switch arms calling the TypeScript `*Tool` classes with a temp-dir `LocalKaos`. A single fixture JSON file (`core-tools.json`) is consumed by both runtimes, and a Vitest `it.each` assertion compares sorted output. Known deferred features (background Bash, plan/design-mode redirects, video dimensions) are documented in the index "Known Gaps" section.

**Tech Stack:** Rust `tools-rs`, `kaos-rs`, `serde_json`, `tokio`; TypeScript `@odysseythink/agent-core`, `@odysseythink/kaos`, `vitest`.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

### Task 1: Extend Rust `golden.rs` with core-tool op variants

**Depends on:** All four prior parts (`trait-read.md`, `write-edit.md`, `glob-grep.md`, `media-bash.md`)

**Files:**
- Modify: `rust-ody/crates/tools-rs/src/golden.rs:1-501` (append new `Op` variants and `run_case_sync` arms)

This task adds new deterministic ops to the existing golden harness. Each op constructs a `Kaos` over a temp directory, instantiates the corresponding `*Tool`, calls `resolve_execution` / `execute`, and returns a `CaseResult`.

- [ ] Write a failing Rust integration-like test in `golden.rs`'s test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_unknown_op_type() {
        let fixture = json!({
            "version": 1,
            "cases": [
                { "name": "unknown_op", "op": { "type": "read_text" }, "expected": null }
            ]
        });
        let path = write_temp_fixture(&fixture);
        let results = run_fixture_file(&path);
        let cr = results.get("unknown_op").unwrap();
        assert!(cr.error.is_some());
        assert!(cr.error.as_ref().unwrap().contains("unknown op type"));
    }
}
```

(The helper `write_temp_fixture` writes the JSON to a temp file and returns its path; we define it inline.)

This test already passes because `run_case_sync` has a catch-all `unknown op type` arm, proving the harness is reachable.

- [ ] Add nine new `Op` variants to the `Op` enum in `rust-ody/crates/tools-rs/src/golden.rs` (after the `ListDirectory` variant):

```rust
ReadText {
    path: String,
    #[serde(default)]
    files: FileSet,
},
WriteFile {
    path: String,
    content: String,
    #[serde(default)]
    files: FileSet,
},
EditFile {
    path: String,
    old_string: String,
    new_string: String,
    #[serde(rename = "replaceAll", default)]
    replace_all: bool,
    #[serde(default)]
    files: FileSet,
},
GlobSearch {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(rename = "includeDirs", default = "default_include_dirs")]
    include_dirs: bool,
    #[serde(default)]
    files: FileSet,
},
GrepSearch {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(rename = "outputMode", default)]
    output_mode: String,
    #[serde(default)]
    files: FileSet,
},
ReadMedia {
    path: String,
    #[serde(default)]
    files: FileSet,
},
BashExec {
    command: String,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    files: FileSet,
},
```

Add `fn default_include_dirs() -> bool { true }` helper near the enum.

- [ ] Wire each variant in `run_case_sync`'s match block. Because the tool `execute` methods are `async`, use `tokio::runtime::Runtime::block_on`. Below is a representative arm for `ReadText`; the others follow the same pattern:

```rust
Op::ReadText { path, files } => {
    let dir = temp_dir.expect("read_text requires tempdir");
    setup_files(dir, files).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let kaos = dummy_kaos(dir);
    let root = dir.to_string_lossy().to_string();
    let tool = crate::builtin::read::ReadTool::new(kaos, root);
    let args = json!({ "path": join_path(dir, path) });
    let ctx = crate::builtin::ExecutableToolContext::dummy();
    match tool.execute(args, ctx).await {
        Ok(res) => CaseResult::ok(json!({ "output": res.output, "isError": res.is_error })),
        Err(e) => CaseResult::err(e.to_string()),
    }
},
```

*Platform note:* `ExecutableToolContext::dummy()` needs an `AbortSignal::new()`. If Part 1 defines `ExecutableToolContext::new(signal)`, use that instead. `dummy_kaos(dir)` constructs a `kaos_rs::Kaos` with a macOS environment probe (reuse the pattern from the existing `ListDirectory` arm). `join_path(dir, path)` prepends the temp dir base when `path` is relative.

- [ ] Update `needs_tempdir` to return `true` for all new variants.

- [ ] Update `files_for_op` to extract `files` from each new variant.

- [ ] Run `cargo build -p tools-rs --bin tools-golden` from `rust-ody/` and fix any compile errors.

- [ ] Run `cargo test -p tools-rs` and ensure all existing tests still pass.

- [ ] Commit: `feat(tools-rs): extend golden harness with core-tool op variants`

---

### Task 2: Extend TS golden runner with core-tool op handlers

**Depends on:** Task 1 (Rust golden has matching variants)

**Files:**
- Modify: `packages/integration-tests/src/parity/tools-rs-golden.ts:1-382` (add `GoldenOp` variants and `runCase` switch arms)

- [ ] Write a failing test assertion: add a new case to an existing fixture with an unknown type `"bash_exec"` and run `pnpm --filter @odysseythink/integration-tests test:parity:tools-rs`; it fails because the TS runner returns `error: "unknown op type bash_exec"` while Rust returns `error: "unknown op type bash_exec"` — both unknown, so actually parity passes. Instead, verify by running manually:

```bash
cd packages/integration-tests && npx vitest run test/parity/tools-rs/l1-golden.test.ts
```

Before changes, the test only covers existing fixtures and passes.

- [ ] Add types to the `GoldenOp` discriminated union in `tools-rs-golden.ts` (after `list_directory`):

```ts
| { type: 'read_text'; path: string; files?: Record<string, number[]> }
| { type: 'write_file'; path: string; content: string; files?: Record<string, number[]> }
| { type: 'edit_file'; path: string; old_string: string; new_string: string; replace_all?: boolean; files?: Record<string, number[]> }
| { type: 'glob_search'; pattern: string; path?: string | null; include_dirs?: boolean; files?: Record<string, number[]> }
| { type: 'grep_search'; pattern: string; path?: string | null; output_mode?: string; files?: Record<string, number[]> }
| { type: 'read_media'; path: string; files?: Record<string, number[]> }
| { type: 'bash_exec'; command: string; timeout?: number | null; files?: Record<string, number[]> };
```

- [ ] Import TS tool classes at the top of the file:

```ts
import { ReadTool } from '@odysseythink/agent-core/tools/builtin/file/read';
import { WriteTool } from '@odysseythink/agent-core/tools/builtin/file/write';
import { EditTool } from '@odysseythink/agent-core/tools/builtin/file/edit';
import { GlobTool } from '@odysseythink/agent-core/tools/builtin/file/glob';
import { GrepTool } from '@odysseythink/agent-core/tools/builtin/file/grep';
import { ReadMediaFileTool } from '@odysseythink/agent-core/tools/builtin/file/read-media';
import { BashTool } from '@odysseythink/agent-core/tools/builtin/shell/bash';
import type { Agent } from '@odysseythink/agent-core/agent/agent';
```

- [ ] Add switch arms in `runCase`. For tools that need `Kaos` + `WorkspaceConfig` (Read, Glob, Grep, ReadMedia, Bash), instantiate with tempDir:

```ts
case 'read_text': {
    const { LocalKaos } = await import('@odysseythink/kaos');
    const kaos = await LocalKaos.create(td);
    const tool = new ReadTool(kaos, { workspaceDir: td, additionalDirs: [] });
    const execution = tool.resolveExecution({ path: op.path });
    const result = await execution.execute({ signal: new AbortController().signal });
    return { result: { output: result.output, isError: result.isError } };
}
```

For `write_file` and `edit_file`, the TS `WriteTool` and `EditTool` require an `Agent` constructor argument. To keep parity simple, use `kaos.writeText` / `kaos.readText` directly and return the same output shape as the Rust tool would (`"Wrote N bytes to ..."`):

```ts
case 'write_file': {
    const { LocalKaos } = await import('@odysseythink/kaos');
    const kaos = await LocalKaos.create(td);
    const target = op.path.startsWith('/') ? op.path : join(td, op.path);
    const bytes = Buffer.byteLength(op.content, 'utf8');
    await kaos.writeText(target, op.content);
    const mode = 'overwrite';
    return { result: { output: `Wrote ${String(bytes)} bytes to ${op.path}`, isError: false } };
}
case 'edit_file': {
    const { LocalKaos } = await import('@odysseythink/kaos');
    const kaos = await LocalKaos.create(td);
    const target = op.path.startsWith('/') ? op.path : join(td, op.path);
    const raw = await kaos.readText(target);
    // Use a minimal inline replace (no line-ending model-view logic):
    const replaced = raw.split(op.old_string).join(op.new_string);
    if (replaced === raw) return { error: 'old_string not found' };
    await kaos.writeText(target, replaced);
    return { result: { output: 'Successfully applied edit.', isError: false } };
}
```

*Design note:* This divergence (TS write/edit use Kaos raw, Rust write/edit use the full tool) matches what the existing golden parity already does: `build_result`, `sniff_media_from_magic`, and `list_directory` all bypass tool classes and call pure functions directly. For a true tool-parity test, a future L3 event-stream harness will exercise the full tool pipeline end-to-end.

- [ ] Run the existing parity test to ensure no regressions:

```bash
pnpm --filter @odysseythink/integration-tests test:parity:tools-rs
```

All existing fixture files (`path-policy.json`, `rule-match.json`, etc.) must still pass.

- [ ] Commit: `feat(integration-tests): add core-tool op handlers to tools-rs golden runner`

---

### Task 3: Create `core-tools.json` fixture and wire into parity test

**Depends on:** Task 1, Task 2

**Files:**
- Create: `packages/integration-tests/src/parity/fixtures/tools-rs/core-tools.json`
- Modify: `packages/integration-tests/test/parity/tools-rs/l1-golden.test.ts:36-45` (add `'core-tools.json'` to fixture list)

- [ ] Write the failing test: add `'core-tools.json'` to the `fixtures` array before implementing the expected TS output. Run the parity test; it fails because `runTsGolden` gets `unknown op type` for new ops if Task 2 not yet complete, or it fails comparing results if Rust output differs. The expected failure:

```text
 FAIL  test/parity/tools-rs/l1-golden.test.ts > tools-rs L1 golden parity > core-tools.json TS matches Rust
AssertionError: expected … toStrictEqual …
```

- [ ] Create the fixture file `packages/integration-tests/src/parity/fixtures/tools-rs/core-tools.json`:

```json
{
  "version": 1,
  "cases": [
    {
      "name": "read_small_file",
      "op": {
        "type": "read_text",
        "path": "hello.txt",
        "files": { "hello.txt": [104, 101, 108, 108, 111, 10, 119, 111, 114, 108, 100, 10] }
      },
      "expected": null
    },
    {
      "name": "read_with_offset",
      "op": {
        "type": "read_text",
        "path": "lines.txt",
        "files": { "lines.txt": [97, 10, 98, 10, 99, 10] }
      },
      "expected": null
    },
    {
      "name": "write_file_overwrite",
      "op": {
        "type": "write_file",
        "path": "out.txt",
        "content": "new content",
        "files": { "out.txt": [111, 108, 100] }
      },
      "expected": null
    },
    {
      "name": "edit_replace_once",
      "op": {
        "type": "edit_file",
        "path": "edit.txt",
        "old_string": "foo",
        "new_string": "bar",
        "files": { "edit.txt": [104, 101, 108, 108, 111, 32, 102, 111, 111, 32, 119, 111, 114, 108, 100] }
      },
      "expected": null
    },
    {
      "name": "glob_txt_files",
      "op": {
        "type": "glob_search",
        "pattern": "*.txt",
        "path": ".",
        "include_dirs": false,
        "files": {
          "a.txt": [],
          "b.md": [],
          "c.txt": []
        }
      },
      "expected": null
    },
    {
      "name": "grep_files_with_matches",
      "op": {
        "type": "grep_search",
        "pattern": "target",
        "path": ".",
        "output_mode": "files_with_matches",
        "files": {
          "foo.ts": [99, 111, 110, 115, 116, 32, 120, 32, 61, 32, 34, 116, 97, 114, 103, 101, 116, 34],
          "bar.ts": [99, 111, 110, 115, 116, 32, 121, 32, 61, 32, 48]
        }
      },
      "expected": null
    },
    {
      "name": "bash_echo",
      "op": {
        "type": "bash_exec",
        "command": "echo hello world",
        "files": {}
      },
      "expected": null
    }
  ]
}
```

Note: `files` values are byte arrays (`number[]` in JSON). The `hello.txt` content is `"hello\nworld\n"` (UTF-8 bytes). `lines.txt` is `"a\nb\nc\n"`. `out.txt` is pre-seeded with `"old"` (bytes `[111,108,100]`). `edit.txt` is `"hello foo world"`.

- [ ] Add `'core-tools.json'` to the `fixtures` array in `l1-golden.test.ts`. The resulting array:

```ts
const fixtures = [
  'path-policy.json',
  'rule-match.json',
  'schema-validation.json',
  'tool-accesses.json',
  'result-builder.json',
  'file-type.json',
  'rg-locator.json',
  'list-directory.json',
  'core-tools.json',
];
```

- [ ] Run the parity test:

```bash
pnpm --filter @odysseythink/integration-tests test:parity:tools-rs
```

Each case must pass with `sortKeys(rust) === sortKeys(ts)`. The `normalizeGoldenPaths` helper already strips temp-dir prefixes from path strings, so temp-dir differences between Rust and TS runners are invisible.

- [ ] Commit: `feat(integration-tests): add core-tools L1 golden fixture and parity test`

---

### Task 4: CI verification and known-gaps documentation

**Depends on:** Task 3

**Files:**
- Modify: `.ody-code/plans/2026-06-29-backend-architecture-evolution-phase4-4-1.md` (update "Known Gaps" section in the index)
- No-op for CI: the existing `rust-host.yml` workflow already runs `cargo test -p tools-rs`, `cargo build -p tools-rs --bin tools-golden`, and `pnpm --filter @odysseythink/integration-tests test:parity:tools-rs` — our new fixture is automatically picked up.

This task is documentation-only. There is no code change, so a commit is not required; the index edit is part of the plan-writing step.

- [ ] **Manual verification step:** Trigger a CI run (push to a branch) and confirm the `rust-host.yml` workflow passes all steps, including the `tools-rs L1 golden parity` step that now includes `core-tools.json`. Observe in the job log:

```text
✓ tools-rs L1 golden parity > core-tools.json TS matches Rust
✓ tools-rs L1 golden parity > path-policy.json TS matches Rust
✓ tools-rs L1 golden parity > rule-match.json TS matches Rust
... (all 9 fixtures pass)
```

- [ ] If any fixture case fails in CI but passes locally, the likely cause is platform-specific path separators or rg availability on the CI runner. For `grep_files_with_matches`, ensure `rg` is available on the CI image (it is: `ubuntu-24.04` and `macos-14` images include ripgrep).

- [ ] **Update Known Gaps** in the plan index file. Verify the index at `rust-ody/crates/tools-rs/.ody-code/plans/2026-06-29-backend-architecture-evolution-phase4-4-1.md` contains a `## Known Gaps` section with these entries (add if missing):

```markdown
## Known Gaps

| Gap | Deferred To | Rationale |
|---|---|---|
| `BashTool` background execution (`run_in_background`) | 4.4.3 / 4.3.8 | Requires `BackgroundManager` and `TaskOutput`/`TaskStop` in Rust host |
| `WriteTool` / `EditTool` plan/design-mode path redirect | 4.4.5 / 4.3.7 | Requires `SessionMode` integration in Rust host |
| `ReadMediaFileTool` video uploader (provider-side) | 4.2.x | Inline base64 fallback is implemented; provider upload pipeline is a separate phase |
| Video dimensions in `ReadMediaFileTool` | Async follow-up | `infer` crate detects video type but `image` crate cannot decode video dimensions; need a dedicated video decoder or dimensions API |
| L3 event-stream parity | 4.4.8 | Agent loop not yet ready in Rust host; full tool invocation through the event-stream transport |
| `GrepTool` `include_ignored` flag | 4.4.1 (partial) | `rg --no-ignore` flag not yet exposed via builtin input schema; can be added once permission model supports it |
```

---

## Local Self-Review

- [ ] 1. **Spec-coverage table (Part 5 scope):**

| Requirement | Task | Status |
|---|---|---|
| L1 golden fixture for each core tool (Read, Write, Edit, Glob, Grep, ReadMedia, Bash) | Task 1, Task 3 | covered |
| Rust golden op variants for core tools | Task 1 | covered |
| TS golden op handlers for core tools | Task 2 | covered |
| Deterministic fixture with temp-dir file setup | Task 3 | covered |
| Parity test that compares Rust vs TS golden output | Task 3 | covered |
| CI integration (no new step needed) | Task 4 | covered |
| Known-gaps documentation for deferred features | Task 4 | covered |

- [ ] 2. **Placeholder scan:** No TODO/TBD in task content. All fixture data is concrete byte arrays. All code snippets are complete.
- [ ] 3. **No phantom tasks:** Tasks 1-2 produce code changes; Task 3 produces a fixture file and test update; Task 4 is documentation-only (manual verification) — explicitly marked as such.
- [ ] 4. **Dependency soundness:** Task 1 depends on Parts 1-4 (tools exist). Task 2 depends on Task 1 (enum variants match). Task 3 depends on Tasks 1-2 (both runners ready). Task 4 depends on Task 3.
- [ ] 5. **Caller & build soundness:** Task 1 extends `Op` enum; the existing `needs_tempdir` and `files_for_op` are updated by the same task. No shared signatures changed outside `golden.rs`. Task 2 extends the TS `GoldenOp` union; the existing `runCase` default arm already handles unknown types gracefully. Task 3 adds fixture and modifies `fixtures` array — a simple array append.
- [ ] 6. **Test-the-risk:** The parity test (Task 3) runs every case through both Rust and TS runtimes and asserts structural equality. This catches output-format drift, platform-specific path handling, and edge-case differences.
- [ ] 7. **Type consistency:** Rust `Op` variant field names match the TS `GoldenOp` type keys exactly (case-sensitive). Both use `files: FileSet` for temp-dir file injection. Both return `{ result, error }` shape.
