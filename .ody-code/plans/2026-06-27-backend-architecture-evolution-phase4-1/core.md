# Part 1 — Rust `kaos-rs` 进程实现

> Scope: `rust-ody/crates/kaos-rs` 中新增 `process.rs`，为 `Kaos` 提供 `exec` / `exec_with_env` / `Process`（stdout/stderr/exitCode/wait/kill），覆盖 POSIX 进程组 kill 与 Windows taskkill fallback。

---

### Task A1: `Process` 结构 + `exec` 基本能力

**Depends on:** none（前置：Phase 4.1.0 已交付 `kaos-rs` crate 骨架、`Kaos` struct、实例级 `cwd`、golden harness）

**Files:**
- Create: `rust-ody/crates/kaos-rs/src/process.rs`
- Modify: `rust-ody/crates/kaos-rs/src/lib.rs:11` 追加 `pub mod process;`
- Modify: `rust-ody/crates/kaos-rs/src/kaos.rs:85-180` 追加 `exec` / `exec_with_env` 方法
- Test: `cargo test -p kaos-rs process::tests::exec_echo -- --nocapture`

- [ ] **Write the failing test。** 在 `process.rs` 底部加入：
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use crate::environment::Environment;
      use crate::Kaos;

      fn dummy_env() -> Environment {
          Environment {
              os_kind: "macOS".to_string(),
              os_arch: "arm64".to_string(),
              os_version: "23.0.0".to_string(),
              shell_name: "bash".to_string(),
              shell_path: "/bin/bash".to_string(),
          }
      }

      #[tokio::test]
      async fn exec_echo() {
          let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
          let proc = kaos.exec(&["/bin/echo", "-n", "hello"]).await.unwrap();
          let code = proc.wait().await;
          let out = proc.stdout().await;
          assert_eq!(out, b"hello");
          assert_eq!(code, 0);
      }
  }
  ```
- [ ] **Run it and verify it FAILS。**
  ```bash
  cargo test -p kaos-rs process::tests::exec_echo -- --nocapture
  ```
  预期：编译失败，`kaos` 没有 `exec` 方法，且 `process` 模块不存在。
- [ ] **Write the minimal implementation。**
  - 新建 `rust-ody/crates/kaos-rs/src/process.rs`：
    ```rust
    use std::io;
    use std::path::Path;
    use std::sync::Arc;

    use tokio::io::AsyncReadExt;
    use tokio::process::Command;
    use tokio::sync::watch;

    /// A running process spawned by a `Kaos` environment.
    #[derive(Debug)]
    pub struct Process {
        pid: u32,
        stdout: Arc<tokio::sync::Mutex<Vec<u8>>>,
        stderr: Arc<tokio::sync::Mutex<Vec<u8>>>,
        exit_rx: watch::Receiver<Option<i32>>,
    }

    impl Process {
        /// OS process id.
        pub fn pid(&self) -> u32 {
            self.pid
        }

        /// Exit code if the process has already terminated.
        pub fn exit_code(&self) -> Option<i32> {
            *self.exit_rx.borrow()
        }

        /// Wait for the process to exit and return its exit code.
        pub async fn wait(&self) -> i32 {
            let mut rx = self.exit_rx.clone();
            loop {
                if let Some(code) = *rx.borrow() {
                    return code;
                }
                if rx.changed().await.is_err() {
                    return -1;
                }
            }
        }

        /// Return all stdout bytes captured so far.
        pub async fn stdout(&self) -> Vec<u8> {
            self.stdout.lock().await.clone()
        }

        /// Return all stderr bytes captured so far.
        pub async fn stderr(&self) -> Vec<u8> {
            self.stderr.lock().await.clone()
        }
    }

    pub(crate) async fn spawn(
        cwd: &Path,
        args: &[&str],
        env: Option<&[(&str, &str)]>,
    ) -> Result<Process, io::Error> {
        if args.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "exec requires at least one argument",
            ));
        }

        let mut cmd = Command::new(args[0]);
        cmd.args(&args[1..]);
        cmd.current_dir(cwd);
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        if let Some(vars) = env {
            for (k, v) in vars {
                cmd.env(k, v);
            }
        }

        #[cfg(unix)]
        {
            // Make the child a process-group leader so POSIX kill can signal
            // the whole tree (direct child + grandchildren).
            cmd.process_group(0);
        }

        let mut child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0);

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let stdout_buf = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let stderr_buf = Arc::new(tokio::sync::Mutex::new(Vec::new()));

        let stdout_clone = stdout_buf.clone();
        let stdout_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            let mut reader = stdout;
            let _ = reader.read_to_end(&mut buf).await;
            *stdout_clone.lock().await = buf;
        });

        let stderr_clone = stderr_buf.clone();
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            let mut reader = stderr;
            let _ = reader.read_to_end(&mut buf).await;
            *stderr_clone.lock().await = buf;
        });

        let (tx, rx) = watch::channel(None);
        tokio::spawn(async move {
            let status = child.wait().await.ok();
            let _ = tokio::join!(stdout_task, stderr_task);
            let code = status.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
            let _ = tx.send(Some(code));
        });

        Ok(Process {
            pid,
            stdout: stdout_buf,
            stderr: stderr_buf,
            exit_rx: rx,
        })
    }
    ```
  - 在 `rust-ody/crates/kaos-rs/src/lib.rs` 追加：
    ```rust
    pub mod process;
    ```
  - 在 `rust-ody/crates/kaos-rs/src/kaos.rs` 的 `impl Kaos` 中追加：
    ```rust
    /// Spawn a process with the given arguments.
    pub async fn exec(&self, args: &[&str]) -> Result<crate::process::Process, std::io::Error> {
        crate::process::spawn(&self.cwd, args, None).await
    }

    /// Spawn a process with explicit environment variables.
    pub async fn exec_with_env(
        &self,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Result<crate::process::Process, std::io::Error> {
        crate::process::spawn(&self.cwd, args, Some(env)).await
    }
    ```
- [ ] **Run it and verify it PASSES。**
  ```bash
  cargo test -p kaos-rs process::tests::exec_echo -- --nocapture
  ```
  预期：`test result: ok`，`out == b"hello"`，`code == 0`。
- [ ] **Commit。**
  ```bash
  git add rust-ody/crates/kaos-rs/src/process.rs rust-ody/crates/kaos-rs/src/lib.rs rust-ody/crates/kaos-rs/src/kaos.rs
  git commit -m "feat(kaos-rs): Process struct and exec entry points"
  ```

---

### Task A2: `cwd` 继承 + `exec_with_env`

**Depends on:** Task A1

**Files:**
- Modify: `rust-ody/crates/kaos-rs/src/kaos.rs:85-180`（`exec_with_env` 已在 A1 写入；本任务确认存在即可）
- Modify: `rust-ody/crates/kaos-rs/src/process.rs` 底部测试模块追加用例
- Test: `cargo test -p kaos-rs process::tests::exec_with_env_sees_cwd_and_variable -- --nocapture`

- [ ] **Write the failing test。** 在 `process.rs` 的 `tests` 模块追加：
  ```rust
  #[tokio::test]
  async fn exec_with_env_sees_cwd_and_variable() {
      let tmp = tempfile::tempdir().unwrap();
      let kaos = Kaos::new(dummy_env(), tmp.path()).with_cwd(tmp.path());

      let proc = kaos
          .exec_with_env(&["/bin/sh", "-c", "printf '%s' \"$PWD\""], &[])
          .await
          .unwrap();
      assert_eq!(proc.wait().await, 0);
      let out = String::from_utf8(proc.stdout().await).unwrap();
      assert_eq!(std::path::Path::new(&out), tmp.path());

      let proc2 = kaos
          .exec_with_env(&["/bin/sh", "-c", "printf '%s' \"$MYVAR\""], &[("MYVAR", "bar")])
          .await
          .unwrap();
      assert_eq!(proc2.wait().await, 0);
      assert_eq!(proc2.stdout().await, b"bar");
  }
  ```
- [ ] **Run it and verify it FAILS。**
  ```bash
  cargo test -p kaos-rs process::tests::exec_with_env_sees_cwd_and_variable -- --nocapture
  ```
  预期：编译失败（若 A1 未写入 `exec_with_env`）或测试失败（若环境变量未传入）。
- [ ] **Write the minimal implementation。** 已在 A1 的 `kaos.rs` 中写入 `exec_with_env`，并已在 `process::spawn` 中读取 `env` 参数；本任务只需确认其存在。若测试仍失败，检查 `spawn` 中是否正确调用 `cmd.env(k, v)`。
- [ ] **Run it and verify it PASSES。**
  ```bash
  cargo test -p kaos-rs process::tests::exec_with_env_sees_cwd_and_variable -- --nocapture
  ```
  预期：`cwd` 输出等于 tmpdir，`MYVAR` 输出等于 `bar`。
- [ ] **Commit。**
  ```bash
  git add rust-ody/crates/kaos-rs/src/process.rs rust-ody/crates/kaos-rs/src/kaos.rs
  git commit -m "feat(kaos-rs): exec_with_env inherits cwd and isolates env"
  ```

---

### Task A3: stdout/stderr 缓冲 + wait-before-read

**Depends on:** Task A1

**Files:**
- Modify: `rust-ody/crates/kaos-rs/src/process.rs` 底部测试模块追加用例
- Test: `cargo test -p kaos-rs process::tests -- --nocapture`

- [ ] **Write the failing test。** 在 `process.rs` 的 `tests` 模块追加：
  ```rust
  #[tokio::test]
  async fn wait_then_read_keeps_stdout_and_stderr() {
      let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
      let proc = kaos
          .exec(&["/bin/sh", "-c", "printf out; printf err >&2"])
          .await
          .unwrap();
      assert_eq!(proc.wait().await, 0);
      assert_eq!(proc.stdout().await, b"out");
      assert_eq!(proc.stderr().await, b"err");
  }

  #[tokio::test]
  async fn large_stdout_does_not_deadlock() {
      let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
      let n = 200_000;
      let proc = kaos
          .exec(&["node", "-e", &format!("process.stdout.write('A'.repeat({}))", n)])
          .await
          .unwrap();
      assert_eq!(proc.wait().await, 0);
      let out = proc.stdout().await;
      assert_eq!(out.len(), n);
      assert!(out.iter().all(|&b| b == b'A'));
  }
  ```
- [ ] **Run it and verify it FAILS。**
  ```bash
  cargo test -p kaos-rs process::tests::wait_then_read_keeps_stdout_and_stderr -- --nocapture
  ```
  预期：如果 A1 的实现已经在 `wait()` 之前启动 reader task，测试应通过；若失败，说明 reader 与 `child.wait()` 的顺序或并发性有问题。
- [ ] **Write the minimal implementation。** A1 的 `spawn` 已满足：先 `take()` stdout/stderr，再 `tokio::spawn` reader task，然后在另一个 task 中 `child.wait()` 并 `tokio::join!` reader task。若测试失败，调整顺序确保 reader task 在 `child.wait()` 之前启动。
- [ ] **Run it and verify it PASSES。**
  ```bash
  cargo test -p kaos-rs process::tests -- --nocapture
  ```
  预期：两个新测试均通过，`large_stdout_does_not_deadlock` 在数秒内完成。
- [ ] **Commit。**
  ```bash
  git add rust-ody/crates/kaos-rs/src/process.rs
  git commit -m "feat(kaos-rs): buffered stdout/stderr readable after wait"
  ```

---

### Task A4: exit code 与命令不存在

**Depends on:** Task A1

**Files:**
- Modify: `rust-ody/crates/kaos-rs/src/process.rs` 底部测试模块追加用例
- Test: `cargo test -p kaos-rs process::tests::false_returns_one process::tests::custom_exit_code process::tests::missing_command_returns_not_found -- --nocapture`

- [ ] **Write the failing test。** 在 `process.rs` 的 `tests` 模块追加：
  ```rust
  #[tokio::test]
  async fn false_returns_one() {
      let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
      let proc = kaos.exec(&["/bin/false"]).await.unwrap();
      assert_eq!(proc.wait().await, 1);
      assert_eq!(proc.exit_code(), Some(1));
  }

  #[tokio::test]
  async fn custom_exit_code() {
      let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
      let proc = kaos.exec(&["/bin/sh", "-c", "exit 42"]).await.unwrap();
      assert_eq!(proc.wait().await, 42);
  }

  #[tokio::test]
  async fn missing_command_returns_not_found() {
      let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
      let err = kaos.exec(&["__missing_command_12345"]).await.unwrap_err();
      assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
  }
  ```
- [ ] **Run it and verify it FAILS。**
  ```bash
  cargo test -p kaos-rs process::tests::false_returns_one -- --nocapture
  ```
  预期：若实现正确则直接通过；若 `exit_code()` 未正确传播，则失败。
- [ ] **Write the minimal implementation。** 已在 A1 的 `spawn` 中通过 `watch::channel` 传播退出码；`Command::spawn` 自然返回 `NotFound`。确认 `Process::exit_code()` 读取 `watch::Receiver` 当前值即可。
- [ ] **Run it and verify it PASSES。**
  ```bash
  cargo test -p kaos-rs process::tests::false_returns_one process::tests::custom_exit_code process::tests::missing_command_returns_not_found -- --nocapture
  ```
  预期：三个测试均通过。
- [ ] **Commit。**
  ```bash
  git add rust-ody/crates/kaos-rs/src/process.rs
  git commit -m "feat(kaos-rs): exit code propagation and spawn-not-found semantics"
  ```

---

### Task A5: POSIX 进程组 kill

**Depends on:** Task A1, Task A2

**Files:**
- Modify: `rust-ody/crates/kaos-rs/Cargo.toml` 追加 Unix-only `nix` 依赖
- Modify: `rust-ody/crates/kaos-rs/src/process.rs` 追加 `kill` 方法与测试
- Test: `cargo test -p kaos-rs process::tests::kill_terminates_long_running_process process::tests::kill_tree_terminates_grandchildren -- --nocapture`

- [ ] **Write the failing test。** 在 `process.rs` 的 `tests` 模块追加：
  ```rust
  #[tokio::test]
  async fn kill_terminates_long_running_process() {
      let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
      let proc = kaos.exec(&["/bin/sleep", "30"]).await.unwrap();
      proc.kill(None).await.unwrap();
      let code = proc.wait().await;
      assert_ne!(code, 0); // killed by signal
  }

  #[tokio::test]
  async fn kill_tree_terminates_grandchildren() {
      let tmp = tempfile::tempdir().unwrap();
      let marker = tmp.path().join("pids.txt");
      let marker_str = marker.to_string_lossy().to_string();
      let script = format!(
          "echo $$ >> {}; sleep 30 & echo $! >> {}; wait",
          marker_str, marker_str
      );
      let kaos = Kaos::new(dummy_env(), tmp.path());
      let proc = kaos.exec(&["/bin/sh", "-c", &script]).await.unwrap();

      // Give the shell time to fork the background sleep.
      tokio::time::sleep(std::time::Duration::from_millis(300)).await;

      proc.kill(None).await.unwrap();
      proc.wait().await;

      let content = tokio::fs::read_to_string(&marker).await.unwrap();
      for pid_str in content.trim().split_whitespace() {
          let pid: i32 = pid_str.parse().unwrap();
          let still_running = std::process::Command::new("kill")
              .args(["-0", &pid.to_string()])
              .status()
              .map(|s| s.success())
              .unwrap_or(false);
          assert!(!still_running, "pid {} still running", pid);
      }
  }
  ```
- [ ] **Run it and verify it FAILS。**
  ```bash
  cargo test -p kaos-rs process::tests::kill_terminates_long_running_process -- --nocapture
  ```
  预期：编译失败，`Process` 没有 `kill` 方法。
- [ ] **Write the minimal implementation。**
  - 在 `rust-ody/crates/kaos-rs/Cargo.toml` 追加：
    ```toml
    [target.'cfg(unix)'.dependencies]
    nix = { version = "0.29", features = ["process", "signal"] }
    ```
  - 在 `process.rs` 的 `impl Process` 中追加：
    ```rust
    /// Send a signal to the process (defaults to SIGTERM).
    /// On POSIX the whole process group is signalled.
    pub async fn kill(&self, signal: Option<&str>) -> io::Result<()> {
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, killpg, Signal};
            use nix::unistd::Pid;

            let sig = match signal {
                Some("SIGKILL") => Signal::SIGKILL,
                Some("SIGINT") => Signal::SIGINT,
                _ => Signal::SIGTERM,
            };

            match killpg(Pid::from_raw(self.pid as i32), sig) {
                Ok(()) => Ok(()),
                Err(nix::errno::Errno::ESRCH) => Ok(()), // already gone
                Err(nix::errno::Errno::EPERM) => {
                    // Fall back to signalling the direct child only.
                    match kill(Pid::from_raw(self.pid as i32), sig) {
                        Ok(()) => Ok(()),
                        Err(nix::errno::Errno::ESRCH) => Ok(()),
                        Err(e) => Err(io::Error::new(io::ErrorKind::PermissionDenied, e)),
                    }
                }
                Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
            }
        }

        #[cfg(not(unix))]
        {
            // Windows implementation is added in Task A6.
            Ok(())
        }
    }
    ```
  - 确认 A1 的 `spawn` 中已调用 `cmd.process_group(0)`（POSIX），使子进程成为进程组组长。
- [ ] **Run it and verify it PASSES。**
  ```bash
  cargo test -p kaos-rs process::tests::kill_terminates_long_running_process process::tests::kill_tree_terminates_grandchildren -- --nocapture
  ```
  预期：两个测试均在 1-2 秒内结束，子进程与孙进程均不再存在。
- [ ] **Run crate-level checks。**
  ```bash
  cargo test -p kaos-rs
  cargo clippy -p kaos-rs -- -D warnings
  ```
  预期：所有 kaos-rs 测试通过，clippy 无警告。
- [ ] **Commit。**
  ```bash
  git add rust-ody/crates/kaos-rs/Cargo.toml rust-ody/crates/kaos-rs/src/process.rs
  git commit -m "feat(kaos-rs): POSIX process-group kill with ESRCH/EPERM fallback"
  ```

---

### Task A6: Windows 进程树 taskkill fallback

**Depends on:** Task A5

**Files:**
- Modify: `rust-ody/crates/kaos-rs/src/process.rs` 中 `kill` 方法的 `#[cfg(not(unix))]` 分支
- Test: 手动验证（macOS/Linux 无法运行 Windows 分支；CI windows job 会编译验证）

- [ ] **Write the implementation。** 将 Task A5 中 `kill` 方法的 `#[cfg(not(unix))]` 分支替换为：
  ```rust
  #[cfg(windows)]
  {
      // Mirror TS LocalProcess.kill(): taskkill /T kills the whole tree.
      // /F is added only for SIGKILL-equivalent force kill.
      let force = signal == Some("SIGKILL");
      let pid_str = self.pid.to_string();
      let args: Vec<&str> = if force {
          vec!["/T", "/F", "/PID", &pid_str]
      } else {
          vec!["/T", "/PID", &pid_str]
      };

      let mut child = tokio::process::Command::new("taskkill")
          .args(&args)
          .creation_flags(0x08000000) // CREATE_NO_WINDOW
          .spawn()?;
      child.wait().await?;
      Ok(())
  }

  #[cfg(not(any(unix, windows)))]
  {
      Err(io::Error::new(
          io::ErrorKind::Unsupported,
          "kill is not supported on this platform",
      ))
  }
  ```
  注意：需要确保 `signal` 参数在 `kill` 签名中可用；若 A5 的实现已命名为 `signal`，直接复用。
- [ ] **Build check on macOS/Linux（验证非 Windows 分支不变）。**
  ```bash
  cargo check -p kaos-rs
  cargo build -p kaos-rs --bin kaos-golden
  ```
  预期：编译通过，`kaos-golden` binary 在 `rust-ody/target/debug/kaos-golden` 生成。
- [ ] **Manual verification on Windows。**
  1. 在 Windows 开发机上启动 PowerShell。
  2. 运行一个一次性验证脚本（可临时放在 `rust-ody/crates/kaos-rs/examples/windows_kill_check.rs`）：
     ```rust
     use kaos_rs::{environment, Kaos};

     #[tokio::main]
     async fn main() {
         let env = environment::detect_environment_from_node().await.unwrap();
         let kaos = Kaos::new(env, std::env::current_dir().unwrap());
         let proc = kaos
             .exec(&["powershell", "-Command", "Start-Sleep -Seconds 30"])
             .await
             .unwrap();
         let pid = proc.pid();
         proc.kill(None).await.unwrap();
         let code = proc.wait().await;
         println!("pid={} exit={}", pid, code);
     }
     ```
  3. 执行：
     ```powershell
     cargo run -p kaos-rs --example windows_kill_check
     ```
  4. 同时用另一个 PowerShell 窗口观察：
     ```powershell
     Get-Process -Id <pid>
     ```
     预期：示例程序在 1 秒内结束；`Get-Process` 在 kill 后返回错误“Cannot find a process with the process identifier <pid>”，且没有残留的 `powershell -Command Start-Sleep` 进程。
- [ ] **Commit。**
  ```bash
  git add rust-ody/crates/kaos-rs/src/process.rs
  git commit -m "feat(kaos-rs): Windows taskkill /T fallback for process tree kill"
  ```

---

## Local Self-Review（Part 1）

- [ ] 1. Spec-coverage table：
  | 路线图 4.1.3 条目 | Task(s) | 状态 |
  |---|---|---|
  | 4.1.3.1 `exec / execWithEnv` | A1, A2 | covered |
  | 4.1.3.2 `KaosProcess` 结构 | A1, A3, A4 | covered |
  | 4.1.3.3 POSIX 进程组 kill | A5 | covered |
  | 4.1.3.4 Windows taskkill fallback | A6 | covered |
- [ ] 2. Placeholder scan：Part 1 中没有 TODO/TBD；A6 的 Windows 分支给出完整实现与手动验证步骤。
- [ ] 3. No phantom tasks：每个 Task 都修改/创建文件并通过 `cargo test`/`cargo check` 验证。
- [ ] 4. Dependency soundness：A2/A3/A4 依赖 A1；A5 依赖 A1/A2；A6 依赖 A5；所有依赖均为前置任务。
- [ ] 5. Caller & build soundness：新增 `Kaos.exec` / `exec_with_env` / `Process` 是 `kaos-rs` 的新公共 API，无既有调用方；A5 新增 `nix` 为 Unix-only 依赖；Part 1 结束必须跑 `cargo test -p kaos-rs` + `cargo clippy -p kaos-rs`。
- [ ] 6. Test-the-risk：
  - 进程启动/输出：`exec_echo`、`wait_then_read_keeps_stdout_and_stderr`、`large_stdout_does_not_deadlock` 断言 stdout 字节与长度。
  - 环境/路径：`exec_with_env_sees_cwd_and_variable` 断言 `$PWD` 与 `$MYVAR`。
  - 退出码/错误：`false_returns_one`、`custom_exit_code`、`missing_command_returns_not_found`。
  - kill：`kill_terminates_long_running_process`、`kill_tree_terminates_grandchildren` 断言进程树终止。
- [ ] 7. Type consistency：`Process` 的 `pid() -> u32`、`wait() -> i32`、`stdout()/stderr() -> Vec<u8>`、`kill(Option<&str>) -> io::Result<()>` 在 A1 定义后保持不变；Part 2 的 fixture 与 harness 直接复用这些类型语义。
