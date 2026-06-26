# Phase A2 — Rust Part: Optional `id` in `create_session`

**Scope:** Extend `CoreHost::create_session` so the `"id"` field in the RPC payload is honored when present and non-empty; otherwise keep the existing UUID v7 generation. Add unit tests for provided-id, auto-id, and duplicate-id rejection.

**Prerequisite:** Phase A1 CLI launch convention is already merged — `ody-host serve` subcommand exists and TS `spawnHost` listens for spawn errors.

## Task A1: Extend `create_session` with optional `id` and add unit tests

**Depends on:** none

**Files:**
- Modify: `rust-ody/crates/ody-host/src/host.rs:86-105` (`create_session` implementation)
- Test: `rust-ody/crates/ody-host/src/host.rs` under `#[cfg(test)]`, after the existing `create_session_returns_summary` test (~line 328)

### Steps

- [ ] Write the failing tests.

  Insert these three tests immediately after `create_session_returns_summary` in the existing `#[cfg(test)]` block:

  ```rust
  #[tokio::test]
  async fn create_session_with_provided_id() {
      let host = make_host();
      let work_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
      let result = host
          .dispatch("createSession", serde_json::json!({"workDir": work_dir, "id": "custom-1"}))
          .await
          .unwrap();
      assert_eq!(result["id"], "custom-1");
      assert_eq!(result["workDir"], work_dir);
  }

  #[tokio::test]
  async fn create_session_without_id_uses_uuid() {
      let host = make_host();
      let work_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
      let a = host
          .dispatch("createSession", serde_json::json!({"workDir": work_dir}))
          .await
          .unwrap();
      let b = host
          .dispatch("createSession", serde_json::json!({"workDir": work_dir}))
          .await
          .unwrap();
      assert!(a["id"].as_str().unwrap().len() > 10);
      assert!(b["id"].as_str().unwrap().len() > 10);
      assert_ne!(a["id"], b["id"]);
  }

  #[tokio::test]
  async fn create_session_duplicate_id_fails() {
      let host = make_host();
      let work_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
      let first = host
          .dispatch("createSession", serde_json::json!({"workDir": work_dir, "id": "dup-1"}))
          .await
          .unwrap();
      assert_eq!(first["id"], "dup-1");
      let err = host
          .dispatch("createSession", serde_json::json!({"workDir": work_dir, "id": "dup-1"}))
          .await
          .unwrap_err();
      assert!(err.to_string().contains("already exists"));
  }
  ```

- [ ] Run the new tests and verify they FAIL.

  ```bash
  cd rust-ody && cargo test -p ody-host create_session
  ```

  Expected failures:
  - `create_session_with_provided_id` — assertion fails because the current implementation ignores `"id"` and returns a generated UUID.
  - `create_session_duplicate_id_fails` — the second call does not fail because it also ignores `"id"` and creates a second UUID.

- [ ] Write the minimal implementation.

  Replace `create_session` (`rust-ody/crates/ody-host/src/host.rs:86-105`) with:

  ```rust
  async fn create_session(&self, payload: serde_json::Value) -> Result<serde_json::Value, crate::error::HostError> {
      let work_dir = payload
          .get("workDir")
          .and_then(|v| v.as_str())
          .unwrap_or(".");
      let title = payload.get("title").and_then(|v| v.as_str());
      let id = payload
          .get("id")
          .and_then(|v| v.as_str())
          .filter(|s| !s.is_empty());
      let summary = match id {
          Some(id) => self.session_manager.create_with_id(id, Path::new(work_dir), title).await,
          None => self.session_manager.create(Path::new(work_dir), title).await,
      }
      .map_err(|e| crate::error::HostError::config_invalid(e.to_string()))?;
      self.sink.emit(AgentEvent::SessionCreated {
          session_id: summary.id.clone(),
          work_dir: work_dir.to_string(),
      });
      Ok(serde_json::json!({
          "id": summary.id,
          "workDir": summary.work_dir,
          "title": summary.title,
          "createdAtMs": summary.created_at_ms,
          "updatedAtMs": summary.updated_at_ms,
      }))
  }
  ```

- [ ] Run the full Rust host test suite and verify it PASSES.

  ```bash
  pnpm run test:host
  ```

  Expected output ends with:

  ```text
  test result: ok. 32 passed; 0 failed
  ```

  (The baseline is 29 tests; the three new tests should bring the total to 32.)

- [ ] Commit.

  ```bash
  git add rust-ody/crates/ody-host/src/host.rs
  git commit -m "feat(ody-host): accept optional id in create_session"
  ```

## Local Self-Review

- [ ] 1. Spec coverage: Rust optional `id`, fixed-id assertion, auto-id assertion, duplicate-id rejection — all covered by Task A1.
- [ ] 2. Placeholder scan: no TODO/TBD; every code block is complete.
- [ ] 3. No phantom tasks: Task A1 modifies code, adds tests, and requires a passing `cargo test`.
- [ ] 4. Dependency soundness: Task A1 depends on none; it only uses existing `SessionManager::create_with_id`.
- [ ] 5. Caller & build soundness: `create_session` is a private method; its public caller `dispatch` keeps the same signature. Run `cargo test -p ody-host` (whole-package, including tests) to confirm.
- [ ] 6. Test-the-risk: behavioral asserts on session id mutation (`id == "custom-1"`), UUID generation, and duplicate rejection are present.
- [ ] 7. Type consistency: no new public types; the payload keys `"id"`, `"workDir"`, `"title"` match the existing JSON contract.
