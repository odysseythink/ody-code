# Part 1 — Crate Foundation + Path/Environment

本 Part 交付 `kaos-rs` crate 的骨架、`Kaos` struct、实例级 `cwd`、路径函数与环境探测，为 Part 2 的纯函数 helper 提供宿主类型。

---

### Task 1: Create kaos-rs crate and register workspace

**Depends on:** none (Phase 4.0 parity framework is a prerequisite listed in the roadmap; no code dependency within this plan).

**Files:**
- Create: `rust-ody/crates/kaos-rs/Cargo.toml`
- Create: `rust-ody/crates/kaos-rs/src/lib.rs`
- Modify: `rust-ody/Cargo.toml:2`

**Steps:**
- [ ] Write `Cargo.toml`:
  ```toml
  [package]
  name = "kaos-rs"
  version = "0.1.0"
  edition = "2021"
  description = "Rust implementation of the KAOS execution environment"
  license = "MIT"

  [dependencies]
  dirs = "5"
  path-clean = "1"
  thiserror = "1"

  [dev-dependencies]
  tempfile = "3"
  tokio-test = "0.4"
  ```
- [ ] Write `src/lib.rs` (empty modules so the crate compiles immediately):
  ```rust
  pub mod buffered;
  pub mod environment;
  pub mod glob;
  pub mod kaos;
  pub mod path;
  pub mod text;
  ```
- [ ] Update `rust-ody/Cargo.toml` workspace members from `members = ["crates/ody-rust", "crates/ody-crypto", "crates/ody-host"]` to `members = ["crates/ody-rust", "crates/ody-crypto", "crates/ody-host", "crates/kaos-rs"]`.
- [ ] Build verification:
  ```bash
  cd rust-ody && cargo check -p kaos-rs
  ```
  Expected: `Finished dev [unoptimized + debuginfo] target(s) in ...` with no errors.
- [ ] Whole-workspace build verification:
  ```bash
  cd rust-ody && cargo check --workspace
  ```
  Expected: workspace still compiles; the new crate must not break `ody-host`/`ody-rust`/`ody-crypto`.
- [ ] Commit: `feat(kaos-rs): bootstrap kaos-rs crate`.

---

### Task 2: Kaos struct + cwd semantics + path operations

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/kaos-rs/src/path.rs`
- Create: `rust-ody/crates/kaos-rs/src/kaos.rs`
- Modify: `rust-ody/crates/kaos-rs/src/lib.rs` (already references the modules; add re-exports if desired)

**Steps:**
- [ ] Write the failing tests first in `src/path.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn path_class_is_posix_or_win32() {
          let cls = path_class();
          assert!(cls == "posix" || cls == "win32");
      }

      #[test]
      fn normpath_resolves_dot_and_dotdot() {
          assert_eq!(normpath("/foo/bar/../baz"), "/foo/baz");
          assert_eq!(normpath("/foo/./bar"), "/foo/bar");
          assert_eq!(normpath("foo//bar/../baz"), "foo/baz");
      }

      #[test]
      fn normpath_preserves_relative_above_root() {
          // Node/pathe behavior: leading .. segments that go above cwd are kept.
          assert_eq!(normpath("../foo"), "../foo");
          assert_eq!(normpath("../../foo"), "../../foo");
      }
  }
  ```
  Run:
  ```bash
  cd rust-ody && cargo test -p kaos-rs --lib path::tests
  ```
  Expected: compilation fails because `path_class`/`normpath` do not exist.
- [ ] Implement `src/path.rs`:
  ```rust
  use std::path::{Path, PathBuf};

  use path_clean::PathClean;

  /// Return the path style used by this environment.
  pub fn path_class() -> &'static str {
      if cfg!(windows) {
          "win32"
      } else {
          "posix"
      }
  }

  /// Normalize the given path string (resolve `.` / `..` segments).
  /// Mirrors `pathe.normalize`: always returns `/`-separated paths.
  pub fn normpath(path: impl AsRef<Path>) -> String {
      let cleaned = path.as_ref().clean();
      cleaned.to_string_lossy().replace('\\', "/")
  }

  /// Return the home directory of the current user.
  pub fn gethome() -> Option<PathBuf> {
      dirs::home_dir()
  }
  ```
- [ ] Write the failing tests for `Kaos` in `src/kaos.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use crate::environment::Environment;

      fn dummy_env() -> Environment {
          Environment {
              os_kind: "macOS".to_string(),
              os_arch: "arm64".to_string(),
              os_version: "23.0.0".to_string(),
              shell_name: "bash".to_string(),
              shell_path: "/bin/bash".to_string(),
          }
      }

      #[test]
      fn with_cwd_creates_independent_instance() {
          let a = Kaos::new(dummy_env(), "/foo");
          let b = a.with_cwd("/bar");
          assert_eq!(a.getcwd(), "/foo");
          assert_eq!(b.getcwd(), "/bar");
      }

      #[test]
      fn chdir_only_mutates_internal_state() {
          let mut kaos = Kaos::new(dummy_env(), "/foo");
          kaos.chdir("/bar");
          assert_eq!(kaos.getcwd(), "/bar");
      }

      #[test]
      fn path_class_delegates_to_host() {
          let kaos = Kaos::new(dummy_env(), "/foo");
          let cls = kaos.path_class();
          assert!(cls == "posix" || cls == "win32");
      }

      #[test]
      fn normpath_resolves_dotdot() {
          let kaos = Kaos::new(dummy_env(), "/foo");
          assert_eq!(kaos.normpath("/foo/bar/../baz"), "/foo/baz");
      }

      #[test]
      fn gethome_returns_some_directory() {
          let kaos = Kaos::new(dummy_env(), "/foo");
          assert!(kaos.gethome().is_some());
      }
  }
  ```
  Run:
  ```bash
  cd rust-ody && cargo test -p kaos-rs --lib kaos::tests
  ```
  Expected: compilation fails because `Kaos` struct and methods do not exist.
- [ ] Implement `src/kaos.rs`:
  ```rust
  use std::path::{Path, PathBuf};

  use crate::environment::Environment;
  use crate::path;

  /// A KAOS implementation that directly interacts with the local filesystem.
  ///
  /// `Kaos` maintains its own per-instance working directory (`cwd`) rather than
  /// mutating the process current directory. This lets multiple `Kaos` instances
  /// coexist with independent cwds.
  #[derive(Debug, Clone)]
  pub struct Kaos {
      name: String,
      env: Environment,
      cwd: PathBuf,
  }

  impl Kaos {
      /// Construct a fresh `Kaos` with the given environment and cwd.
      pub fn new(env: Environment, cwd: impl AsRef<Path>) -> Self {
          Self {
              name: "local".to_string(),
              env,
              cwd: PathBuf::from(path::normpath(cwd)),
          }
      }

      /// Return a new `Kaos` with the given `cwd`.
      pub fn with_cwd(&self, cwd: impl AsRef<Path>) -> Self {
          Self {
              name: self.name.clone(),
              env: self.env.clone(),
              cwd: PathBuf::from(path::normpath(cwd)),
          }
      }

      /// Change the working directory of this instance (internal state only).
      pub fn chdir(&mut self, cwd: impl AsRef<Path>) {
          self.cwd = PathBuf::from(path::normpath(cwd));
      }

      /// Human-readable name for this environment.
      pub fn name(&self) -> &str {
          &self.name
      }

      /// OS / shell probe describing the target environment.
      pub fn env(&self) -> &Environment {
          &self.env
      }

      /// Return the path style used by this environment.
      pub fn path_class(&self) -> &'static str {
          path::path_class()
      }

      /// Normalize the given path string (resolve `.` / `..` segments).
      pub fn normpath(&self, path: impl AsRef<Path>) -> String {
          path::normpath(path)
      }

      /// Return the home directory of the current user.
      pub fn gethome(&self) -> Option<PathBuf> {
          path::gethome()
      }

      /// Return the current working directory of this instance.
      pub fn getcwd(&self) -> String {
          path::normpath(&self.cwd)
      }
  }
  ```
- [ ] Run tests:
  ```bash
  cd rust-ody && cargo test -p kaos-rs --lib
  ```
  Expected: all path + kaos tests pass.
- [ ] Commit: `feat(kaos-rs): Kaos struct, cwd isolation, and path operations`.

---

### Task 3: Environment detection

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/kaos-rs/src/environment.rs`

**Steps:**
- [ ] Write the failing tests first in `src/environment.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use std::collections::HashMap;

      fn deps_with_files(files: &[&'static str]) -> EnvironmentDeps {
          let set: std::collections::HashSet<&'static str> = files.iter().copied().collect();
          EnvironmentDeps {
              platform: "win32".to_string(),
              arch: "x86_64".to_string(),
              release: "10.0.0".to_string(),
              env: HashMap::new(),
              is_file: Box::new(move |p| set.contains(p)),
              find_executable: Box::new(|_| None),
          }
      }

      #[test]
      fn detect_on_windows_uses_override_shell_path() {
          let mut env = HashMap::new();
          env.insert("ODY_SHELL_PATH".to_string(), "D:\\custom\\bash.exe".to_string());
          let deps = EnvironmentDeps {
              platform: "win32".to_string(),
              arch: "x86_64".to_string(),
              release: "10.0.0".to_string(),
              env,
              is_file: Box::new(|p| p == "D:\\custom\\bash.exe"),
              find_executable: Box::new(|_| None),
          };
          let e = detect_environment(&deps);
          assert_eq!(e.os_kind, "Windows");
          assert_eq!(e.shell_path, "D:\\custom\\bash.exe");
      }

      #[test]
      fn detect_on_windows_falls_back_to_git_bash_candidates() {
          let deps = deps_with_files(&["C:\\Program Files\\Git\\bin\\bash.exe"]);
          let e = detect_environment(&deps);
          assert_eq!(e.os_kind, "Windows");
          assert_eq!(e.shell_path, "C:\\Program Files\\Git\\bin\\bash.exe");
      }

      #[test]
      fn detect_on_posix_prefers_bash() {
          let deps = EnvironmentDeps {
              platform: "darwin".to_string(),
              arch: "arm64".to_string(),
              release: "23.0.0".to_string(),
              env: HashMap::new(),
              is_file: Box::new(|p| p == "/bin/bash"),
              find_executable: Box::new(|_| None),
          };
          let e = detect_environment(&deps);
          assert_eq!(e.os_kind, "macOS");
          assert_eq!(e.shell_name, "bash");
          assert_eq!(e.shell_path, "/bin/bash");
      }

      #[test]
      fn detect_on_posix_falls_back_to_sh() {
          let deps = EnvironmentDeps {
              platform: "linux".to_string(),
              arch: "x86_64".to_string(),
              release: "6.0.0".to_string(),
              env: HashMap::new(),
              is_file: Box::new(|_| false),
              find_executable: Box::new(|_| None),
          };
          let e = detect_environment(&deps);
          assert_eq!(e.os_kind, "Linux");
          assert_eq!(e.shell_name, "sh");
          assert_eq!(e.shell_path, "/bin/sh");
      }

      #[test]
      #[should_panic(expected = "Git Bash was not found")]
      fn detect_on_windows_panics_without_git_bash() {
          let deps = deps_with_files(&[]);
          let _ = detect_environment(&deps);
      }
  }
  ```
  Run:
  ```bash
  cd rust-ody && cargo test -p kaos-rs --lib environment::tests
  ```
  Expected: compilation fails because `Environment`, `EnvironmentDeps`, and `detect_environment` do not exist.
- [ ] Implement `src/environment.rs`:
  ```rust
  use std::collections::HashMap;

  /// Cross-platform probe of OS / shell.
  #[derive(Debug, Clone)]
  pub struct Environment {
      pub os_kind: String,
      pub os_arch: String,
      pub os_version: String,
      pub shell_name: String,
      pub shell_path: String,
  }

  /// Injected dependencies for `detect_environment`. Tests supply mock
  /// implementations; production callers use `detect_environment_from_node`.
  pub struct EnvironmentDeps {
      pub platform: String,
      pub arch: String,
      pub release: String,
      pub env: HashMap<String, String>,
      pub is_file: Box<dyn Fn(&str) -> bool>,
      pub find_executable: Box<dyn Fn(&str) -> Option<String>>,
  }

  #[derive(Debug, thiserror::Error)]
  #[error("Git Bash was not found on this Windows host. Install Git for Windows from https://gitforwindows.org/ or set ODY_SHELL_PATH to a bash.exe. Checked: {checked}.")]
  pub struct KaosShellNotFoundError {
      checked: String,
  }

  fn resolve_os_kind(platform: &str) -> String {
      match platform {
          "darwin" => "macOS".to_string(),
          "linux" => "Linux".to_string(),
          "win32" => "Windows".to_string(),
          _ => platform.to_string(),
      }
  }

  /// Detect environment from injected probes.
  pub fn detect_environment(deps: &EnvironmentDeps) -> Environment {
      let os_kind = resolve_os_kind(&deps.platform);
      let os_arch = deps.arch.clone();
      let os_version = deps.release.clone();

      if deps.platform == "win32" {
          let shell_path = locate_windows_git_bash(deps);
          return Environment {
              os_kind,
              os_arch,
              os_version,
              shell_name: "bash".to_string(),
              shell_path,
          };
      }

      let candidates = ["/bin/bash", "/usr/bin/bash", "/usr/local/bin/bash"];
      for p in candidates {
          if (deps.is_file)(p) {
              return Environment {
                  os_kind,
                  os_arch,
                  os_version,
                  shell_name: "bash".to_string(),
                  shell_path: p.to_string(),
              };
          }
      }

      Environment {
          os_kind,
          os_arch,
          os_version,
          shell_name: "sh".to_string(),
          shell_path: "/bin/sh".to_string(),
      }
  }

  fn locate_windows_git_bash(deps: &EnvironmentDeps) -> String {
      let mut checked: Vec<String> = Vec::new();

      if let Some(override_path) = deps.env.get("ODY_SHELL_PATH").map(|s| s.trim()).filter(|s| !s.is_empty()) {
          checked.push(override_path.to_string());
          if (deps.is_file)(override_path) {
              return override_path.to_string();
          }
      }

      if let Some(git_exe) = (deps.find_executable)("git.exe") {
          if let Some(inferred) = infer_git_bash_from_git_exe(&git_exe) {
              for path in inferred {
                  checked.push(path.clone());
                  if (deps.is_file)(&path) {
                      return path;
                  }
              }
          }
      }

      let mut candidates = vec![
          "C:\\Program Files\\Git\\bin\\bash.exe".to_string(),
          "C:\\Program Files\\Git\\usr\\bin\\bash.exe".to_string(),
          "C:\\Program Files (x86)\\Git\\bin\\bash.exe".to_string(),
          "C:\\Program Files (x86)\\Git\\usr\\bin\\bash.exe".to_string(),
      ];
      if let Some(local_app_data) = deps.env.get("LOCALAPPDATA").map(|s| s.trim()).filter(|s| !s.is_empty()) {
          candidates.push(format!("{}\\Programs\\Git\\bin\\bash.exe", local_app_data));
          candidates.push(format!("{}\\Programs\\Git\\usr\\bin\\bash.exe", local_app_data));
      }
      for candidate in candidates {
          checked.push(candidate.clone());
          if (deps.is_file)(&candidate) {
              return candidate;
          }
      }

      panic!(
          "{}",
          KaosShellNotFoundError {
              checked: checked.join(", "),
          }
      );
  }

  fn infer_git_bash_from_git_exe(git_exe: &str) -> Option<Vec<String>> {
      let sep = if git_exe.contains('\\') { '\\' } else { '/' };
      let mut parts: Vec<&str> = git_exe.split(sep).collect();
      for i in (0..parts.len().saturating_sub(1)).rev() {
          if parts[i] == "cmd" || parts[i] == "bin" {
              let root = parts[..i].join(&sep.to_string());
              let prefix = if root.is_empty() {
                  String::new()
              } else {
                  format!("{}{}", root, sep)
              };
              return Some(vec![
                  format!("{}bin{}bash.exe", prefix, sep),
                  format!("{}usr{}bin{}bash.exe", prefix, sep),
              ]);
          }
      }
      None
  }

  /// Production convenience — derive the deps bag from the host OS.
  pub fn detect_environment_from_node() -> Environment {
      use std::fs;
      let mut env = HashMap::new();
      for (k, v) in std::env::vars() {
          env.insert(k, v);
      }
      detect_environment(&EnvironmentDeps {
          platform: std::env::consts::OS.to_string(),
          arch: std::env::consts::ARCH.to_string(),
          release: std::env::consts::OS.to_string(), // placeholder; exact OS version obtained in 4.5 gap if needed
          env,
          is_file: Box::new(|p| fs::metadata(p).map(|m| m.is_file()).unwrap_or(false)),
          find_executable: Box::new(|name| which::which(name).ok().map(|p| p.to_string_lossy().to_string())),
      })
  }
  ```
  Note: `which` crate is added to `Cargo.toml` under `[dependencies]`:
  ```toml
  which = "6"
  ```
- [ ] Run tests:
  ```bash
  cd rust-ody && cargo test -p kaos-rs --lib environment::tests
  ```
  Expected: all environment tests pass.
- [ ] Run full crate tests:
  ```bash
  cd rust-ody && cargo test -p kaos-rs
  ```
  Expected: all Part 1 tests pass.
- [ ] Commit: `feat(kaos-rs): environment detection with Windows Git Bash fallback`.

---

## Local Self-Review

- [ ] 1. Spec-coverage: 4.1.0.1 crate/workspace → T1; 4.1.0.2 `Kaos`/`cwd` → T2; 4.1.0.3 path operations → T2; 4.1.0.4 environment detection → T3。无 GAP。
- [ ] 2. Placeholder scan: 本 Part 无 TODO/TBD；`detect_environment_from_node` 的 `release` placeholder 是已知且不影响 L1 的（L1 使用注入 deps），已在注释说明。
- [ ] 3. No phantom tasks: T1 产出可编译 crate；T2/T3 产出带测试的模块；每个 Task 以 commit 收尾。
- [ ] 4. Dependency soundness: T2/T3 均依赖 T1；T2 与 T3 之间无依赖；无反向依赖。
- [ ] 5. Caller & build soundness: T1 修改 `rust-ody/Cargo.toml` workspace members，同一 Task 以 `cargo check --workspace` 验证；T2/T3 仅新增符号，无共享签名变更。
- [ ] 6. Test-the-risk: T2 断言 `with_cwd` 实例隔离与 `chdir` 内部状态变更；T3 断言 Windows override、fallback、bash/sh 分支与 panic 路径。
- [ ] 7. Type consistency: `Environment` 字段名（os_kind/os_arch/os_version/shell_name/shell_path）与 TS `Environment` 对齐；`Kaos` 方法名（path_class/normpath/gethome/getcwd/with_cwd/chdir）与 TS `Kaos` interface 对齐。
