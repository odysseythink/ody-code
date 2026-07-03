# Part 1 — Host Integration

本 Part 把 `kaos-rs` 接入 `ody-host`：添加依赖、让 `CoreHost` 持有 `Arc<Kaos>`、定义内部 `env.*` RPC 的请求/响应类型，并在 `CoreHost.dispatch` 中把 `env.*` 方法路由到 `kaos-rs`。最后用一个单元测试证明 `env.getcwd` 走通整个 RPC plumbing。

---

### Task 1: Add kaos-rs dependency to ody-host

**Depends on:** none (upstream Phase 4.1.0–4.1.3 already produced `kaos-rs`).

**Files:**
- Modify: `rust-ody/crates/ody-host/Cargo.toml:12-30`

**Steps:**

- [ ] Add `kaos-rs = { path = "../kaos-rs" }` to `[dependencies]` immediately after `toml = { workspace = true }`:
  ```toml
  [dependencies]
  tokio = { workspace = true }
  reqwest = { workspace = true }
  serde = { workspace = true }
  serde_json = { workspace = true }
  tracing = { workspace = true }
  tracing-subscriber = { workspace = true }
  clap = { workspace = true }
  toml = { workspace = true }
  kaos-rs = { path = "../kaos-rs" }
  uuid = { version = "1", features = ["v4", "v7", "serde"] }
  sha2 = "0.10"
  regex = "1"
  dirs = "5"
  async-trait = "0.1"
  futures-util = { version = "0.3", default-features = false, features = ["std"] }
  ```
- [ ] Build verification:
  ```bash
  cd rust-ody && cargo check -p ody-host
  ```
  Expected: `Finished dev [unoptimized + debuginfo] target(s) in ...` with no errors.
- [ ] Whole-workspace build verification:
  ```bash
  cd rust-ody && cargo check --workspace
  ```
  Expected: all workspace crates compile.
- [ ] Commit: `feat(ody-host): depend on kaos-rs`.

---

### Task 2: CoreHost holds Arc<Kaos>

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/ody-host/src/host.rs:1-58`

**Steps:**

- [ ] Add imports at the top of `host.rs`:
  ```rust
  use std::sync::Arc;
  use kaos_rs::kaos::Kaos;
  use kaos_rs::environment::detect_environment_from_node;
  ```
  (Existing `use std::sync::Arc;` at line 3 can be kept or merged; add kaos imports.)
- [ ] Add `kaos` field to `CoreHost`:
  ```rust
  pub struct CoreHost {
      pub config: HostConfig,
      pub session_manager: SessionManager,
      tool_registry: ToolRegistry,
      provider: Box<dyn LlmProvider>,
      sink: Box<dyn EventSink>,
      turn_counter: AtomicI64,
      kaos: Arc<Kaos>,
  }
  ```
- [ ] In `CoreHost::new`, after `let store = SessionStoreAdapter::new(...);`, construct the host-level Kaos instance. Use `config.home_dir` as the initial cwd so the host starts in a well-known directory:
  ```rust
  let env = detect_environment_from_node();
  let kaos = Arc::new(Kaos::new(env, &config.home_dir));
  ```
- [ ] Include `kaos` in the struct literal returned by `CoreHost::new`:
  ```rust
  Ok(Self {
      session_manager: SessionManager::new(store),
      tool_registry,
      provider,
      sink,
      config,
      turn_counter: AtomicI64::new(0),
      kaos,
  })
  ```
- [ ] Add an accessor so later tasks and tools can clone the Arc:
  ```rust
  impl CoreHost {
      /// Return a clone of the host-level `Arc<Kaos>`.
      pub fn kaos(&self) -> Arc<Kaos> {
          Arc::clone(&self.kaos)
      }
      // ... existing methods
  }
  ```
- [ ] Build verification:
  ```bash
  cd rust-ody && cargo check -p ody-host
  ```
  Expected: compiles; `main.rs` does not need changes because `CoreHost::new` signature is unchanged.
- [ ] Run host unit tests to ensure no regression:
  ```bash
  cd rust-ody && cargo test -p ody-host --lib host::tests
  ```
  Expected: existing tests pass.
- [ ] Commit: `feat(ody-host): host-level Arc<Kaos>`.

---

### Task 3: Define env.* RPC request/response types and dispatch

**Depends on:** Task 2

**Files:**
- Create: `rust-ody/crates/ody-host/src/env.rs`
- Modify: `rust-ody/crates/ody-host/src/lib.rs`

**Steps:**

- [ ] Create `rust-ody/crates/ody-host/src/env.rs` with the following full implementation:
  ```rust
  use std::sync::Arc;

  use kaos_rs::kaos::Kaos;
  use kaos_rs::text::ErrorMode;

  /// Dispatch an internal `env.*` method to `kaos-rs`.
  ///
  /// These methods are NOT part of the public CoreAPI contract; they exist
  /// only for parity testing and internal tooling.
  pub async fn dispatch(
      kaos: &Kaos,
      method: &str,
      payload: serde_json::Value,
  ) -> Result<serde_json::Value, String> {
      match method {
          "env.getcwd" => Ok(serde_json::json!({ "cwd": kaos.getcwd() })),
          "env.stat" => env_stat(kaos, payload).await,
          "env.glob" => env_glob(kaos, payload).await,
          "env.readText" => env_read_text(kaos, payload).await,
          "env.writeText" => env_write_text(kaos, payload).await,
          "env.exec" => env_exec(kaos, payload).await,
          _ => Err(format!("unknown env method: {method}")),
      }
  }

  async fn env_stat(kaos: &Kaos, payload: serde_json::Value) -> Result<serde_json::Value, String> {
      let path = payload
          .get("path")
          .and_then(|v| v.as_str())
          .ok_or("missing path")?;
      let follow_symlinks = payload
          .get("followSymlinks")
          .and_then(|v| v.as_bool())
          .unwrap_or(true);
      let stat = kaos
          .stat(path, follow_symlinks)
          .await
          .map_err(|e| e.to_string())?;
      Ok(serde_json::json!({
          "stMode": stat.st_mode,
          "stIno": stat.st_ino,
          "stDev": stat.st_dev,
          "stNlink": stat.st_nlink,
          "stUid": stat.st_uid,
          "stGid": stat.st_gid,
          "stSize": stat.st_size,
          "stAtime": stat.st_atime,
          "stMtime": stat.st_mtime,
          "stCtime": stat.st_ctime,
          "isDir": stat.is_dir(),
      }))
  }

  async fn env_glob(kaos: &Kaos, payload: serde_json::Value) -> Result<serde_json::Value, String> {
      let path = payload
          .get("path")
          .and_then(|v| v.as_str())
          .ok_or("missing path")?;
      let pattern = payload
          .get("pattern")
          .and_then(|v| v.as_str())
          .ok_or("missing pattern")?;
      let case_sensitive = payload
          .get("caseSensitive")
          .and_then(|v| v.as_bool())
          .unwrap_or(true);
      let matches = kaos
          .glob(path, pattern, case_sensitive)
          .await
          .map_err(|e| e.to_string())?;
      Ok(serde_json::json!({ "matches": matches }))
  }

  async fn env_read_text(
      kaos: &Kaos,
      payload: serde_json::Value,
  ) -> Result<serde_json::Value, String> {
      let path = payload
          .get("path")
          .and_then(|v| v.as_str())
          .ok_or("missing path")?;
      let encoding = payload.get("encoding").and_then(|v| v.as_str());
      let errors = payload
          .get("errors")
          .and_then(|v| v.as_str())
          .map(parse_error_mode)
          .transpose()?;
      let text = kaos
          .read_text(path, encoding, errors)
          .await
          .map_err(|e| e.to_string())?;
      Ok(serde_json::json!({ "text": text }))
  }

  async fn env_write_text(
      kaos: &Kaos,
      payload: serde_json::Value,
  ) -> Result<serde_json::Value, String> {
      let path = payload
          .get("path")
          .and_then(|v| v.as_str())
          .ok_or("missing path")?;
      let text = payload
          .get("text")
          .and_then(|v| v.as_str())
          .ok_or("missing text")?;
      let mode = payload.get("mode").and_then(|v| v.as_str());
      let encoding = payload.get("encoding").and_then(|v| v.as_str());
      let written = kaos
          .write_text(path, text, mode, encoding)
          .await
          .map_err(|e| e.to_string())?;
      Ok(serde_json::json!({ "written": written }))
  }

  async fn env_exec(kaos: &Kaos, payload: serde_json::Value) -> Result<serde_json::Value, String> {
      let command = payload
          .get("command")
          .and_then(|v| v.as_str())
          .ok_or("missing command")?;
      let args: Vec<String> = payload
          .get("args")
          .and_then(|v| v.as_array())
          .map(|arr| {
              arr.iter()
                  .filter_map(|v| v.as_str().map(String::from))
                  .collect()
          })
          .unwrap_or_default();
      let env: Option<Vec<(String, String)>> = payload
          .get("env")
          .and_then(|v| v.as_object())
          .map(|obj| {
              obj.iter()
                  .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                  .collect()
          });

      let all_args: Vec<&str> = std::iter::once(command)
          .chain(args.iter().map(|s| s.as_str()))
          .collect();

      let proc = if let Some(vars) = env {
          let pairs: Vec<(&str, &str)> = vars
              .iter()
              .map(|(k, v)| (k.as_str(), v.as_str()))
              .collect();
          kaos.exec_with_env(&all_args, &pairs).await
      } else {
          kaos.exec(&all_args).await
      }
      .map_err(|e| e.to_string())?;

      let exit_code = proc.wait().await;
      let stdout = String::from_utf8_lossy(&proc.stdout().await).to_string();
      let stderr = String::from_utf8_lossy(&proc.stderr().await).to_string();
      Ok(serde_json::json!({
          "exitCode": exit_code,
          "stdout": stdout,
          "stderr": stderr,
      }))
  }

  fn parse_error_mode(s: &str) -> Result<ErrorMode, String> {
      match s {
          "strict" => Ok(ErrorMode::Strict),
          "replace" => Ok(ErrorMode::Replace),
          "ignore" => Ok(ErrorMode::Ignore),
          _ => Err(format!("invalid errors mode: {s}")),
      }
  }
  ```
- [ ] Add `pub mod env;` to `rust-ody/crates/ody-host/src/lib.rs` (append after existing `pub mod transport;`).
- [ ] Build verification:
  ```bash
  cd rust-ody && cargo check -p ody-host
  ```
  Expected: compiles; note that `exec_with_env` takes `&[(&str, &str)]` and `exec` takes `&[&str]` per `kaos-rs/src/kaos.rs:177-188`.
- [ ] Commit: `feat(ody-host): env.* RPC dispatch types and kaos wiring`.

---

### Task 4: Wire env.* into CoreHost::dispatch

**Depends on:** Task 3

**Files:**
- Modify: `rust-ody/crates/ody-host/src/host.rs:61-89`

**Steps:**

- [ ] In `CoreHost::dispatch`, before the `_ => Err(...)` arm, add an `env.` prefix match:
  ```rust
  pub async fn dispatch(&self, method: &str, payload: serde_json::Value) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
      match method {
          "getCoreInfo" => Ok(self.get_core_info()),
          // ... existing methods ...
          "getMcpStartupMetrics" => Ok(self.get_mcp_startup_metrics()),
          method if method.starts_with("env.") => {
              crate::env::dispatch(&self.kaos, method, payload)
                  .await
                  .map_err(|e| e.into())
          }
          _ => Err(format!("unknown method: {method}").into()),
      }
  }
  ```
- [ ] Build verification:
  ```bash
  cd rust-ody && cargo check -p ody-host
  ```
  Expected: compiles.
- [ ] Commit: `feat(ody-host): route env.* methods to kaos-rs`.

---

### Task 5: Unit test env.* dispatch wiring

**Depends on:** Task 4

**Files:**
- Modify: `rust-ody/crates/ody-host/src/host.rs:508-859` (test module)

**Steps:**

- [ ] Add a test that `env.getcwd` returns the configured home directory:
  ```rust
  #[tokio::test]
  async fn env_getcwd_returns_home_dir() {
      let host = make_host();
      let result = host
          .dispatch("env.getcwd", serde_json::json!({}))
          .await
          .unwrap();
      assert!(result["cwd"].is_string());
      // make_host uses a tempdir as home_dir; the returned cwd should be normalized.
      assert!(!result["cwd"].as_str().unwrap().is_empty());
  }
  ```
- [ ] Add a test that `env.stat` returns metadata for a file created in the host's home dir:
  ```rust
  #[tokio::test]
  async fn env_stat_returns_file_metadata() {
      let host = make_host();
      let home = host.config.home_dir.to_string_lossy().to_string();
      let file_path = format!("{}/test.txt", home);
      tokio::fs::write(&file_path, "hello").await.unwrap();

      let result = host
          .dispatch(
              "env.stat",
              serde_json::json!({"path": "test.txt", "followSymlinks": true}),
          )
          .await
          .unwrap();
      assert_eq!(result["stSize"], 5);
      assert_eq!(result["isDir"], false);
  }
  ```
- [ ] Add a test that unknown env methods still return an error:
  ```rust
  #[tokio::test]
  async fn env_unknown_method_returns_error() {
      let host = make_host();
      let err = host
          .dispatch("env.nosuch", serde_json::json!({}))
          .await
          .unwrap_err();
      assert!(err.to_string().contains("unknown env method"));
  }
  ```
- [ ] Run the new tests:
  ```bash
  cd rust-ody && cargo test -p ody-host --lib host::tests::env_getcwd_returns_home_dir
  cd rust-ody && cargo test -p ody-host --lib host::tests::env_stat_returns_file_metadata
  cd rust-ody && cargo test -p ody-host --lib host::tests::env_unknown_method_returns_error
  ```
  Expected: all pass.
- [ ] Run the full ody-host test suite:
  ```bash
  cd rust-ody && cargo test -p ody-host
  ```
  Expected: all pass.
- [ ] Commit: `test(ody-host): env.* dispatch wiring`.

---

## Part 1 Local Self-Review

- [ ] 1. Spec-coverage table: 4.1.4.1/4.1.4.2/4.1.4.3 的核心接入（依赖、CoreHost 字段、env.* 类型与分发）已映射到 T1–T5。
- [ ] 2. Placeholder扫描: `env.rs` 中每个 `env.*` 方法都有完整实现，无 TODO/TBD。
- [ ] 3. No phantom tasks: 每个 Task 均产生可编译代码或测试；无 `--allow-empty`。
- [ ] 4. Dependency soundness: T1→T2→T3→T4→T5，均为前向依赖。
- [ ] 5. Caller & build soundness: T2 增加 `CoreHost` 字段但未改变 `CoreHost::new` 签名，因此 `main.rs` 与现有测试无需改动；T5 新增测试不影响外部调用方；每个 Task 均以 `cargo check/test -p ody-host` 验证。
- [ ] 6. Test-the-risk: T5 断言 `env.getcwd`/`env.stat` 的返回形状与文件系统状态；`env.nosuch` 断言错误路由。
- [ ] 7. Type consistency: `Kaos`/`StatResult` 直接复用 `kaos-rs` 定义；`env.*` 请求字段名（`path`/`followSymlinks`/`pattern`/`caseSensitive`/`text`/`mode`/`encoding`/`command`/`args`/`env`）与 Part 2 的 TS 适配器保持一致。
