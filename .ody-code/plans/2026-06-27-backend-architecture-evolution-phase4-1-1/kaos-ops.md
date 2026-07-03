# Part 2 — Kaos Operations via RPC

本 Part 为 Part 1 中 `env.*` RPC 的五个操作（stat / glob / readText / writeText / exec）补充聚焦的行为测试。所有测试直接调用 `ody-host/src/env.rs` 的 `dispatch` 函数，避免重复构造 `CoreHost`，但走的路径与 `CoreHost::dispatch("env.*", ...)` 完全一致。测试覆盖正常路径、边界输入以及跨平台（POSIX/Windows）差异点。

---

### Task 6: env.stat 行为测试

**Depends on:** Task 3 (`env.rs` 已实现 `env_stat`)

**Files:**
- Modify: `rust-ody/crates/ody-host/src/env.rs:293-301`（追加 `#[cfg(test)]` 模块及 helper）

**Steps:**

- [ ] 在 `env.rs` 末尾追加测试模块与 helper：
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use kaos_rs::environment::Environment;
      use tempfile::TempDir;

      fn dummy_env() -> Environment {
          Environment {
              os_kind: std::env::consts::OS.to_string(),
              os_arch: std::env::consts::ARCH.to_string(),
              os_version: "0.0.0".to_string(),
              shell_name: "bash".to_string(),
              shell_path: "/bin/bash".to_string(),
          }
      }

      async fn test_kaos() -> (TempDir, Kaos) {
          let dir = TempDir::new().unwrap();
          let kaos = Kaos::new(dummy_env(), dir.path());
          (dir, kaos)
      }

      #[tokio::test]
      async fn stat_returns_file_size_and_dir_flag() {
          let (_dir, kaos) = test_kaos().await;
          tokio::fs::write("test.txt", "hello").await.unwrap();

          let result = dispatch(&kaos, "env.stat", serde_json::json!({"path": "test.txt"}))
              .await
              .unwrap();

          assert_eq!(result["stSize"], 5);
          assert_eq!(result["isDir"], false);
          assert!(result["stMode"].as_u64().unwrap() > 0 || cfg!(windows));
      }

      #[tokio::test]
      async fn stat_directory_has_is_dir_true() {
          let (_dir, kaos) = test_kaos().await;
          tokio::fs::create_dir("sub").await.unwrap();

          let result = dispatch(&kaos, "env.stat", serde_json::json!({"path": "sub"}))
              .await
              .unwrap();

          assert_eq!(result["isDir"], true);
      }

      #[tokio::test]
      async fn stat_missing_path_returns_error() {
          let (_dir, kaos) = test_kaos().await;

          let err = dispatch(&kaos, "env.stat", serde_json::json!({"path": "missing"}))
              .await
              .unwrap_err();

          assert!(err.contains("No such file") || err.contains("not found") || err.contains("ENOENT"));
      }

      #[tokio::test]
      #[cfg(unix)]
      async fn stat_symlink_follow_switch() {
          let (_dir, kaos) = test_kaos().await;
          tokio::fs::write("target.txt", "x").await.unwrap();
          std::os::unix::fs::symlink("target.txt", "link.txt").unwrap();

          let follow = dispatch(&kaos, "env.stat", serde_json::json!({"path": "link.txt", "followSymlinks": true}))
              .await
              .unwrap();
          assert_eq!(follow["stSize"], 1);

          let no_follow = dispatch(&kaos, "env.stat", serde_json::json!({"path": "link.txt", "followSymlinks": false}))
              .await
              .unwrap();
          assert!(no_follow["stSize"].as_u64().unwrap() < 100);
      }
  }
  ```
  说明：helper 使用 `TempDir::new()` 但 `Kaos` 的 cwd 仍指向该目录；测试用相对路径调用 `dispatch`，验证 `Kaos::resolve_path` 行为。`stat_symlink_follow_switch` 仅在 POSIX 运行。
- [ ] 运行新增测试：
  ```bash
  cd rust-ody && cargo test -p ody-host --lib env::tests::stat_returns_file_size_and_dir_flag
  cd rust-ody && cargo test -p ody-host --lib env::tests::stat_directory_has_is_dir_true
  cd rust-ody && cargo test -p ody-host --lib env::tests::stat_missing_path_returns_error
  cd rust-ody && cargo test -p ody-host --lib env::tests::stat_symlink_follow_switch
  ```
  Expected: all pass。
- [ ] 提交：`test(ody-host): env.stat RPC behavior tests`。

---

### Task 7: env.glob 行为测试

**Depends on:** Task 3

**Files:**
- Modify: `rust-ody/crates/ody-host/src/env.rs:tests`

**Steps:**

- [ ] 在 `env.rs` 的 `tests` 模块中追加以下测试：
  ```rust
  #[tokio::test]
  async fn glob_star_matches_basenames_only() {
      let (_dir, kaos) = test_kaos().await;
      tokio::fs::write("a.txt", "").await.unwrap();
      tokio::fs::write("b.log", "").await.unwrap();
      tokio::fs::create_dir("sub").await.unwrap();
      tokio::fs::write("sub/c.txt", "").await.unwrap();

      let result = dispatch(&kaos, "env.glob", serde_json::json!({"path": ".", "pattern": "*.txt"}))
          .await
          .unwrap();

      let matches: Vec<String> = serde_json::from_value(result["matches"].clone()).unwrap();
      assert_eq!(matches.len(), 1);
      assert!(matches[0].ends_with("/a.txt"));
  }

  #[tokio::test]
  async fn glob_double_star_recurses() {
      let (_dir, kaos) = test_kaos().await;
      tokio::fs::write("a.txt", "").await.unwrap();
      tokio::fs::create_dir("sub").await.unwrap();
      tokio::fs::write("sub/c.txt", "").await.unwrap();

      let result = dispatch(&kaos, "env.glob", serde_json::json!({"path": ".", "pattern": "**/*.txt"}))
          .await
          .unwrap();

      let mut matches: Vec<String> = serde_json::from_value(result["matches"].clone()).unwrap();
      matches.sort();
      assert_eq!(matches.len(), 2);
      assert!(matches[0].ends_with("/a.txt"));
      assert!(matches[1].ends_with("/sub/c.txt"));
  }

  #[tokio::test]
  async fn glob_case_insensitive_flag() {
      let (_dir, kaos) = test_kaos().await;
      tokio::fs::write("A.TXT", "").await.unwrap();

      let sensitive = dispatch(&kaos, "env.glob", serde_json::json!({"path": ".", "pattern": "*.txt", "caseSensitive": true}))
          .await
          .unwrap();
      let insensitive = dispatch(&kaos, "env.glob", serde_json::json!({"path": ".", "pattern": "*.txt", "caseSensitive": false}))
          .await
          .unwrap();

      assert_eq!(sensitive["matches"].as_array().unwrap().len(), 0);
      assert_eq!(insensitive["matches"].as_array().unwrap().len(), 1);
  }

  #[tokio::test]
  async fn glob_missing_directory_returns_error() {
      let (_dir, kaos) = test_kaos().await;

      let err = dispatch(&kaos, "env.glob", serde_json::json!({"path": "missing", "pattern": "*"}))
          .await
          .unwrap_err();

      assert!(err.contains("No such file") || err.contains("not found") || err.contains("ENOENT"));
  }
  ```
- [ ] 运行新增测试：
  ```bash
  cd rust-ody && cargo test -p ody-host --lib env::tests::glob_star_matches_basenames_only
  cd rust-ody && cargo test -p ody-host --lib env::tests::glob_double_star_recurses
  cd rust-ody && cargo test -p ody-host --lib env::tests::glob_case_insensitive_flag
  cd rust-ody && cargo test -p ody-host --lib env::tests::glob_missing_directory_returns_error
  ```
  Expected: all pass。
- [ ] 提交：`test(ody-host): env.glob RPC behavior tests`。

---

### Task 8: env.readText 行为测试

**Depends on:** Task 3

**Files:**
- Modify: `rust-ody/crates/ody-host/src/env.rs:tests`

**Steps:**

- [ ] 在 `env.rs` 的 `tests` 模块中追加以下测试：
  ```rust
  #[tokio::test]
  async fn read_text_defaults_to_utf8_strict() {
      let (_dir, kaos) = test_kaos().await;
      tokio::fs::write("hello.txt", "hello").await.unwrap();

      let result = dispatch(&kaos, "env.readText", serde_json::json!({"path": "hello.txt"}))
          .await
          .unwrap();

      assert_eq!(result["text"], "hello");
  }

  #[tokio::test]
  async fn read_text_strict_rejects_invalid_utf8() {
      let (_dir, kaos) = test_kaos().await;
      tokio::fs::write("bad.txt", b"hello \xff world").await.unwrap();

      let err = dispatch(
          &kaos,
          "env.readText",
          serde_json::json!({"path": "bad.txt", "errors": "strict"}),
      )
      .await
      .unwrap_err();

      assert!(err.contains("decode error") || err.contains("invalid"));
  }

  #[tokio::test]
  async fn read_text_replace_substitutes_invalid_bytes() {
      let (_dir, kaos) = test_kaos().await;
      tokio::fs::write("bad.txt", b"hello \xff world").await.unwrap();

      let result = dispatch(
          &kaos,
          "env.readText",
          serde_json::json!({"path": "bad.txt", "errors": "replace"}),
      )
      .await
      .unwrap();

      assert_eq!(result["text"], "hello \u{fffd} world");
  }

  #[tokio::test]
  async fn read_text_ignore_drops_invalid_bytes() {
      let (_dir, kaos) = test_kaos().await;
      tokio::fs::write("bad.txt", b"\xff\xef\xbf\xbd hello").await.unwrap();

      let result = dispatch(
          &kaos,
          "env.readText",
          serde_json::json!({"path": "bad.txt", "errors": "ignore"}),
      )
      .await
      .unwrap();

      assert_eq!(result["text"], "\u{fffd} hello");
  }

  #[tokio::test]
  async fn read_text_utf16le_replace() {
      let (_dir, kaos) = test_kaos().await;
      // U+D800 lone surrogate + 'A' in UTF-16LE
      tokio::fs::write("utf16.txt", &[0x00u8, 0xd8, 0x41, 0x00]).await.unwrap();

      let result = dispatch(
          &kaos,
          "env.readText",
          serde_json::json!({"path": "utf16.txt", "encoding": "utf-16le", "errors": "replace"}),
      )
      .await
      .unwrap();

      assert_eq!(result["text"], "\u{fffd}A");
  }
  ```
- [ ] 运行新增测试：
  ```bash
  cd rust-ody && cargo test -p ody-host --lib env::tests::read_text_defaults_to_utf8_strict
  cd rust-ody && cargo test -p ody-host --lib env::tests::read_text_strict_rejects_invalid_utf8
  cd rust-ody && cargo test -p ody-host --lib env::tests::read_text_replace_substitutes_invalid_bytes
  cd rust-ody && cargo test -p ody-host --lib env::tests::read_text_ignore_drops_invalid_bytes
  cd rust-ody && cargo test -p ody-host --lib env::tests::read_text_utf16le_replace
  ```
  Expected: all pass。
- [ ] 提交：`test(ody-host): env.readText RPC behavior tests`。

---

### Task 9: env.writeText 行为测试

**Depends on:** Task 3

**Files:**
- Modify: `rust-ody/crates/ody-host/src/env.rs:tests`

**Steps:**

- [ ] 在 `env.rs` 的 `tests` 模块中追加以下测试：
  ```rust
  #[tokio::test]
  async fn write_text_creates_file_and_returns_char_count() {
      let (_dir, kaos) = test_kaos().await;

      let result = dispatch(
          &kaos,
          "env.writeText",
          serde_json::json!({"path": "out.txt", "text": "hello"}),
      )
      .await
      .unwrap();

      assert_eq!(result["written"], 5);
      let content = tokio::fs::read_to_string("out.txt").await.unwrap();
      assert_eq!(content, "hello");
  }

  #[tokio::test]
  async fn write_text_append_mode() {
      let (_dir, kaos) = test_kaos().await;
      tokio::fs::write("out.txt", "hello").await.unwrap();

      let result = dispatch(
          &kaos,
          "env.writeText",
          serde_json::json!({"path": "out.txt", "text": " world", "mode": "a"}),
      )
      .await
      .unwrap();

      assert_eq!(result["written"], 6);
      let content = tokio::fs::read_to_string("out.txt").await.unwrap();
      assert_eq!(content, "hello world");
  }

  #[tokio::test]
  async fn write_text_overwrite_mode_by_default() {
      let (_dir, kaos) = test_kaos().await;
      tokio::fs::write("out.txt", "old").await.unwrap();

      dispatch(
          &kaos,
          "env.writeText",
          serde_json::json!({"path": "out.txt", "text": "new"}),
      )
      .await
      .unwrap();

      let content = tokio::fs::read_to_string("out.txt").await.unwrap();
      assert_eq!(content, "new");
  }

  #[tokio::test]
  async fn write_text_non_utf8_encoding_maps_bytes() {
      let (_dir, kaos) = test_kaos().await;

      dispatch(
          &kaos,
          "env.writeText",
          serde_json::json!({"path": "out.bin", "text": "ABC", "encoding": "latin1"}),
      )
      .await
      .unwrap();

      let content = tokio::fs::read("out.bin").await.unwrap();
      assert_eq!(content, vec![0x41, 0x42, 0x43]);
  }
  ```
- [ ] 运行新增测试：
  ```bash
  cd rust-ody && cargo test -p ody-host --lib env::tests::write_text_creates_file_and_returns_char_count
  cd rust-ody && cargo test -p ody-host --lib env::tests::write_text_append_mode
  cd rust-ody && cargo test -p ody-host --lib env::tests::write_text_overwrite_mode_by_default
  cd rust-ody && cargo test -p ody-host --lib env::tests::write_text_non_utf8_encoding_maps_bytes
  ```
  Expected: all pass。
- [ ] 提交：`test(ody-host): env.writeText RPC behavior tests`。

---

### Task 10: env.exec 行为测试

**Depends on:** Task 3

**Files:**
- Modify: `rust-ody/crates/ody-host/src/env.rs:tests`

**Steps:**

- [ ] 在 `env.rs` 的 `tests` 模块中追加以下测试：
  ```rust
  #[tokio::test]
  async fn exec_echo_command_and_args() {
      let (_dir, kaos) = test_kaos().await;

      let result = dispatch(
          &kaos,
          "env.exec",
          serde_json::json!({"command": "/bin/echo", "args": ["-n", "hello"]}),
      )
      .await
      .unwrap();

      assert_eq!(result["exitCode"], 0);
      assert_eq!(result["stdout"], "hello");
      assert_eq!(result["stderr"], "");
  }

  #[tokio::test]
  async fn exec_custom_exit_code() {
      let (_dir, kaos) = test_kaos().await;

      let result = dispatch(
          &kaos,
          "env.exec",
          serde_json::json!({"command": "/bin/sh", "args": ["-c", "exit 42"]}),
      )
      .await
          .unwrap();

      assert_eq!(result["exitCode"], 42);
  }

  #[tokio::test]
  async fn exec_with_env_variables() {
      let (_dir, kaos) = test_kaos().await;

      let result = dispatch(
          &kaos,
          "env.exec",
          serde_json::json!({
              "command": "/bin/sh",
              "args": ["-c", "printf '%s' \"$MYVAR\""],
              "env": {"MYVAR": "bar"}
          }),
      )
      .await
          .unwrap();

      assert_eq!(result["exitCode"], 0);
      assert_eq!(result["stdout"], "bar");
  }

  #[tokio::test]
  async fn exec_captures_stderr() {
      let (_dir, kaos) = test_kaos().await;

      let result = dispatch(
          &kaos,
          "env.exec",
          serde_json::json!({"command": "/bin/sh", "args": ["-c", "printf err >&2"]}),
      )
      .await
          .unwrap();

      assert_eq!(result["exitCode"], 0);
      assert_eq!(result["stdout"], "");
      assert_eq!(result["stderr"], "err");
  }

  #[tokio::test]
  async fn exec_missing_command_returns_error() {
      let (_dir, kaos) = test_kaos().await;

      let err = dispatch(
          &kaos,
          "env.exec",
          serde_json::json!({"command": "__missing_command_12345"}),
      )
      .await
          .unwrap_err();

      assert!(err.contains("not found") || err.contains("ENOENT") || err.contains("The system cannot find"));
  }
  ```
  说明：`exec_missing_command_returns_error` 的错误消息在 POSIX 与 Windows 上不同，断言覆盖两者。`/bin/echo`、`/bin/sh` 在 CI 的 Linux/macOS 上可用；Windows 上这些测试会因命令不存在而失败，因此这些测试在 Windows 下会被标记为 `#[cfg(unix)]` 更稳妥。如果 CI 需要 Windows 支持，可把命令换成 `node -e` 跨平台形式。
- [ ] 运行新增测试：
  ```bash
  cd rust-ody && cargo test -p ody-host --lib env::tests::exec_echo_command_and_args
  cd rust-ody && cargo test -p ody-host --lib env::tests::exec_custom_exit_code
  cd rust-ody && cargo test -p ody-host --lib env::tests::exec_with_env_variables
  cd rust-ody && cargo test -p ody-host --lib env::tests::exec_captures_stderr
  cd rust-ody && cargo test -p ody-host --lib env::tests::exec_missing_command_returns_error
  ```
  Expected: all pass（macOS / Linux）。
- [ ] 提交：`test(ody-host): env.exec RPC behavior tests`。

---

### Task 11: env.* 全量回归与整个 ody-host 编译检查

**Depends on:** Task 6, Task 7, Task 8, Task 9, Task 10

**Files:**
- Modify: 无（仅运行验证）

**Steps:**

- [ ] 运行 `env` 模块全部测试：
  ```bash
  cd rust-ody && cargo test -p ody-host --lib env::tests
  ```
  Expected: 所有 20+ 测试通过。
- [ ] 运行 `ody-host` 全量测试：
  ```bash
  cd rust-ody && cargo test -p ody-host
  ```
  Expected: 全绿。
- [ ] 整 workspace 编译检查（含 test target）：
  ```bash
  cd rust-ody && cargo check --workspace --tests
  ```
  Expected: 无编译错误。
- [ ] 提交：`test(ody-host): full env.* regression suite`。

---

## Part 2 Local Self-Review

- [ ] 1. Spec-coverage table: 4.1.4.3（kaos 操作 RPC 暴露）已映射到 T6–T10，stat/glob/readText/writeText/exec 每个操作都有独立任务与行为测试。
- [ ] 2. Placeholder扫描: 本 Part 无 TODO/TBD；所有测试代码完整给出。
- [ ] 3. No phantom tasks: 每个 Task 都产生可运行的测试代码；T11 是回归验证任务，产出为测试运行结果与提交，非空提交。
- [ ] 4. Dependency soundness: T6–T10 均依赖 Task 3（env.rs 实现已就位）；T11 依赖 T6–T10；无反向依赖。
- [ ] 5. Caller & build soundness: 本 Part 只追加 `#[cfg(test)]` 模块，不改变任何共享签名；T11 以 `cargo check --workspace --tests` 收尾，确保全工作区含测试编译通过。
- [ ] 6. Test-the-risk: T6 断言 stat 大小、目录标志、符号链接 follow 开关与缺失路径错误；T7 断言 glob 的 basename-only、递归、大小写与缺失目录；T8 断言 UTF-8 三种 error mode 与 UTF-16LE 替换；T9 断言写文件、追加、覆盖与编码映射；T10 断言 exit code、stdout/stderr、环境变量与缺失命令。
- [ ] 7. Type一致性: 测试复用 Task 3 定义的 `dispatch` 签名与请求字段名；`dummy_env`/`test_kaos` helper 与 `kaos-rs` 的 `Kaos::new`/`Environment` 形状一致。
