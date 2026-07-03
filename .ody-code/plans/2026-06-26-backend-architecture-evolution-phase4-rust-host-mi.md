# Phase 4.1.1 目录操作 (stat/iterdir/glob/mkdir) 迁移实施计划

**Goal:** 在 `rust-ody/crates/kaos-rs` 中实现 `stat`/`iterdir`/`glob`/`mkdir`,与 TS `LocalKaos` 逐字段/逐行为对齐,并通过 `packages/integration-tests/src/parity/` 的 L1 golden fixture 完成 TS↔Rust 对照,为 4.1.4 的 `CoreHost` 集成提供目录 I/O 能力。

**Architecture:** 在 `kaos-rs` 新增 `dir.rs` 模块承载所有目录操作,新增 `errors.rs` 提供 `KaosFileExistsError`;`Kaos` struct 暴露同名 async 方法。L1 对照复用现有 golden harness:Rust 侧扩展 `kaos-golden` binary 与单元测试,TS 侧扩展 `kaos-golden.ts` runner, fixture 为同一份 JSON,最终由 `test/parity/kaos/l1-golden.test.ts` 做 TS↔Rust 结构化 diff。

**Tech Stack:** Rust (tokio::fs, std::path, regex), TypeScript / Vitest, `@odysseythink/kaos`, `packages/integration-tests`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| Path | Responsibility |
|---|---|
| `rust-ody/crates/kaos-rs/src/errors.rs` | `KaosFileExistsError` 类型 |
| `rust-ody/crates/kaos-rs/src/dir.rs` | `stat`/`iterdir`/`glob`/`mkdir` 实现 + 单元测试 |
| `rust-ody/crates/kaos-rs/src/kaos.rs` | 为 `Kaos` struct 绑定目录操作方法 |
| `rust-ody/crates/kaos-rs/src/lib.rs` | 导出 `dir`/`errors` 模块 |
| `rust-ody/crates/kaos-rs/src/golden.rs` | 扩展 golden runner 支持 directory ops |
| `rust-ody/crates/kaos-rs/tests/golden.rs` | 注册 `l1-directory-ops.json` fixture 测试 |
| `packages/integration-tests/src/parity/fixtures/kaos/l1-directory-ops.json` | 目录操作 L1 golden fixture |
| `packages/integration-tests/src/parity/kaos-golden.ts` | TS 侧 golden runner 扩展 |
| `packages/integration-tests/test/parity/kaos/l1-golden.test.ts` | parity 测试入口,加入新 fixture |

---

## Dependency Overview

```
Task 1 (errors.rs + dir.rs scaffold)
  │
  ▼
Task 2 (stat)
  │
  ▼
Task 3 (iterdir)
  │
  ▼
Task 4 (glob)
  │
  ▼
Task 5 (mkdir)
  │
  ▼
Task 6 (Rust golden runner + fixture)
  │
  ▼
Task 7 (TS golden runner + parity test)
```

- 所有任务均依赖 4.1.0 已完成(`Kaos` struct / `path::normpath` / `glob_pattern_to_regex` / `KaosIoError` / golden harness 骨架)。
- 4.1.1 内部按顺序串行,因为每个任务都在同一个 `dir.rs`/`golden.rs` 文件上累加,并行会产生大量合并冲突。

---

## Risks & Open Questions

| # | Risk | Mitigation |
|---|---|---|
| R1 | `StatResult` 中 `st_mode`/`st_uid` 等字段在 Windows 上不存在,fixture 跨平台失败 | fixture 只断言 `isDir` + `size`,runner 把完整 `StatResult` 归一为 `{ isDir, size }` |
| R2 | 时间戳非确定导致 stat fixture 不稳定 | runner 将 `stAtime`/`stMtime`/`stCtime` 归零后再序列化 |
| R3 | `glob` 返回的绝对路径含临时目录,fixture 无法写死 | Rust/TS runner 都把结果裁剪为相对于 tempdir 的相对路径 |
| R4 | `glob` symlink 循环处理与 TS `_globWalk` 不一致 | 单测显式构造循环 symlink,断言不会无限递归且结果有限 |
| R5 | Node `KaosFileExistsError` 与 Rust 错误消息形状不同,TS↔Rust 对照失败 | 两侧 runner 都把 I/O 错误归一化为 `not found`/`already exists`/`permission denied` 等 canonical 字符串 |
| R6 | `glob` 大小写敏感/字符类 `[!a]` 与 TS 行为漂移 | fixture 覆盖大小写、`[!a]`、`**` 递归、隐藏文件用例 |

---

## Spec-Coverage Table

| 设计 § | Requirement | 覆盖 Task(s) | 状态 |
|---|---|---|---|
| 4.1.1.1 实现 `stat` | 对齐 `StatResult` 字段;`followSymlinks` 开关 | Task 2, Task 6/7 fixture | covered |
| 4.1.1.2 实现 `iterdir` | 返回规范化路径,根目录 trailing slash 处理 | Task 3, Task 6/7 fixture | covered |
| 4.1.1.3 实现 `glob` | 复刻 `globPatternToRegex` + `_globWalk`;循环检测 | Task 4, Task 6/7 fixture | covered |
| 4.1.1.4 实现 `mkdir` | `parents` + `existOk` 语义;`KaosFileExistsError` | Task 1/5, Task 6/7 fixture | covered |
| 4.1.1.5 L1 golden 目录 fixture | 构造已知 tmpdir 树,比对返回列表/字段 | Task 6, Task 7 | covered |
| G4-1-1 门 | 目录操作 L1 对照 100% 绿,含 glob symlink 循环 | Task 7 最终验证 | covered |

---

### Task 1: `KaosFileExistsError` + `dir.rs` 骨架

**Depends on:** 4.1.0

**Files:**
- Create: `rust-ody/crates/kaos-rs/src/errors.rs`
- Create: `rust-ody/crates/kaos-rs/src/dir.rs`
- Modify: `rust-ody/crates/kaos-rs/src/lib.rs:1-8`

- [ ] **Write the failing test** 在 `rust-ody/crates/kaos-rs/src/errors.rs` 底部:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn file_exists_error_display_matches_message() {
          let e = KaosFileExistsError::new("/tmp/foo already exists");
          assert_eq!(format!("{}", e), "/tmp/foo already exists");
          assert!(e.source().is_none());
      }
  }
  ```
- [ ] **Run it and verify it FAILS**:
  ```bash
  cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p kaos-rs file_exists_error_display_matches_message
  ```
  预期:编译失败 `cannot find type KaosFileExistsError in this scope`。
- [ ] **Write the minimal implementation**:
  - `rust-ody/crates/kaos-rs/src/errors.rs`:
    ```rust
    use std::fmt;

    /// Equivalent to Python's `FileExistsError` and TS `KaosFileExistsError`.
    #[derive(Debug)]
    pub struct KaosFileExistsError {
        message: String,
    }

    impl KaosFileExistsError {
        pub fn new(message: impl Into<String>) -> Self {
            Self { message: message.into() }
        }
    }

    impl fmt::Display for KaosFileExistsError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl std::error::Error for KaosFileExistsError {}
    ```
  - `rust-ody/crates/kaos-rs/src/dir.rs` 骨架:
    ```rust
    //! Directory operations for kaos-rs: stat, iterdir, glob, mkdir.
    use std::io;

    pub use crate::errors::KaosFileExistsError;

    #[cfg(test)]
    mod tests {
        use super::*;
    }
    ```
  - `rust-ody/crates/kaos-rs/src/lib.rs` 新增:
    ```rust
    pub mod dir;
    pub mod errors;
    ```
- [ ] **Run it and verify it PASSES**:
  ```bash
  cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p kaos-rs file_exists_error_display_matches_message
  ```
  预期:1 passed。
- [ ] **Commit**:
  ```bash
  git add rust-ody/crates/kaos-rs/src/errors.rs rust-ody/crates/kaos-rs/src/dir.rs rust-ody/crates/kaos-rs/src/lib.rs
  git commit -m "feat(kaos-rs): add KaosFileExistsError and dir module scaffold for 4.1.1"
  ```

---

### Task 2: 实现 `stat`

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/kaos-rs/src/dir.rs`
- Modify: `rust-ody/crates/kaos-rs/src/kaos.rs:85-133`

- [ ] **Write the failing test** 在 `rust-ody/crates/kaos-rs/src/dir.rs` 的 `tests` 模块:
  ```rust
  use tempfile::TempDir;

  async fn temp_dir() -> (TempDir, String) {
      let d = TempDir::new().unwrap();
      let s = d.path().to_string_lossy().to_string();
      (d, s)
  }

  fn is_dir(st_mode: u32) -> bool {
      (st_mode & 0o170000) == 0o040000
  }

  #[tokio::test]
  async fn stat_distinguishes_file_and_directory() {
      let (_d, root) = temp_dir().await;
      let file = format!("{}/file.txt", root);
      tokio::fs::write(&file, "hello").await.unwrap();

      let f = stat(&file, true).await.unwrap();
      assert_eq!(f.st_size, 5);
      assert!(!is_dir(f.st_mode));

      let d = stat(&root, true).await.unwrap();
      assert!(is_dir(d.st_mode));
  }

  #[tokio::test]
  async fn stat_follow_symlinks_switch() {
      let (_d, root) = temp_dir().await;
      let target = format!("{}/target.txt", root);
      let link = format!("{}/link.txt", root);
      tokio::fs::write(&target, "x").await.unwrap();
      #[cfg(unix)]
      {
          std::os::unix::fs::symlink(&target, &link).unwrap();
          let with_follow = stat(&link, true).await.unwrap();
          assert_eq!(with_follow.st_size, 1);
          let no_follow = stat(&link, false).await.unwrap();
          assert!(no_follow.st_size < 100);
      }
  }
  ```
- [ ] **Run it and verify it FAILS**:
  ```bash
  cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p kaos-rs stat_distinguishes
  ```
  预期:编译失败 `cannot find function stat`。
- [ ] **Write the minimal implementation**:
  - `rust-ody/crates/kaos-rs/src/dir.rs`:
    ```rust
    use serde::Serialize;

    #[derive(Debug, Serialize, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct StatResult {
        pub st_mode: u32,
        pub st_ino: u64,
        pub st_dev: u64,
        pub st_nlink: u64,
        pub st_uid: u32,
        pub st_gid: u32,
        pub st_size: u64,
        pub st_atime: f64,
        pub st_mtime: f64,
        pub st_ctime: f64,
    }

    pub async fn stat(path: &str, follow_symlinks: bool) -> Result<StatResult, io::Error> {
        let meta = if follow_symlinks {
            tokio::fs::metadata(path).await?
        } else {
            tokio::fs::symlink_metadata(path).await?
        };
        Ok(build_stat_result(&meta))
    }

    fn build_stat_result(meta: &std::fs::Metadata) -> StatResult {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            StatResult {
                st_mode: meta.mode(),
                st_ino: meta.ino(),
                st_dev: meta.dev(),
                st_nlink: meta.nlink(),
                st_uid: meta.uid(),
                st_gid: meta.gid(),
                st_size: meta.len(),
                st_atime: meta.atime() as f64,
                st_mtime: meta.mtime() as f64,
                st_ctime: meta.ctime() as f64,
            }
        }
        #[cfg(not(unix))]
        {
            StatResult {
                st_mode: 0,
                st_ino: 0,
                st_dev: 0,
                st_nlink: 0,
                st_uid: 0,
                st_gid: 0,
                st_size: meta.len(),
                st_atime: 0.0,
                st_mtime: 0.0,
                st_ctime: 0.0,
            }
        }
    }
    ```
  - `rust-ody/crates/kaos-rs/src/kaos.rs` 在 `write_text` 后添加:
    ```rust
    /// Return stat metadata for `path`.
    pub async fn stat(
        &self,
        path_str: &str,
        follow_symlinks: bool,
    ) -> Result<crate::dir::StatResult, std::io::Error> {
        let resolved = self.resolve_path(path_str);
        crate::dir::stat(&resolved, follow_symlinks).await
    }
    ```
- [ ] **Run it and verify it PASSES**:
  ```bash
  cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p kaos-rs stat_distinguishes
  ```
  预期:2 passed。
- [ ] **Commit**:
  ```bash
  git add rust-ody/crates/kaos-rs/src/dir.rs rust-ody/crates/kaos-rs/src/kaos.rs
  git commit -m "feat(kaos-rs): implement stat with StatResult parity"
  ```

---

### Task 3: 实现 `iterdir`

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/kaos-rs/src/dir.rs`
- Modify: `rust-ody/crates/kaos-rs/src/kaos.rs`

- [ ] **Write the failing test** 在 `rust-ody/crates/kaos-rs/src/dir.rs` 的 `tests` 模块:
  ```rust
  #[tokio::test]
  async fn iterdir_returns_normalized_full_paths() {
      let (_d, root) = temp_dir().await;
      tokio::fs::write(format!("{}/a.txt", root), "").await.unwrap();
      tokio::fs::write(format!("{}/b.txt", root), "").await.unwrap();

      let mut entries = iterdir(&root).await.unwrap();
      entries.sort();
      assert_eq!(entries.len(), 2);
      assert!(entries[0].ends_with("/a.txt"));
      assert!(entries[1].ends_with("/b.txt"));
      assert!(!entries[0].contains("//"));
  }
  ```
- [ ] **Run it and verify it FAILS**:
  ```bash
  cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p kaos-rs iterdir_returns_normalized
  ```
  预期:编译失败 `cannot find function iterdir`。
- [ ] **Write the minimal implementation**:
  - `rust-ody/crates/kaos-rs/src/dir.rs`:
    ```rust
    pub async fn iterdir(path: &str) -> Result<Vec<String>, io::Error> {
        let mut entries = tokio::fs::read_dir(path).await?;
        let mut out = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let full = crate::path::normpath(&entry.path());
            out.push(full);
        }
        Ok(out)
    }
    ```
  - `rust-ody/crates/kaos-rs/src/kaos.rs` 添加:
    ```rust
    /// Yield entry names in the directory at `path` as normalized full paths.
    pub async fn iterdir(&self, path_str: &str) -> Result<Vec<String>, std::io::Error> {
        let resolved = self.resolve_path(path_str);
        crate::dir::iterdir(&resolved).await
    }
    ```
- [ ] **Run it and verify it PASSES**:
  ```bash
  cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p kaos-rs iterdir_returns_normalized
  ```
  预期:1 passed。
- [ ] **Commit**:
  ```bash
  git add rust-ody/crates/kaos-rs/src/dir.rs rust-ody/crates/kaos-rs/src/kaos.rs
  git commit -m "feat(kaos-rs): implement iterdir returning normalized full paths"
  ```

---

### Task 4: 实现 `glob`

**Depends on:** Task 3

**Files:**
- Modify: `rust-ody/crates/kaos-rs/src/dir.rs`
- Modify: `rust-ody/crates/kaos-rs/src/kaos.rs`

- [ ] **Write the failing test** 在 `rust-ody/crates/kaos-rs/src/dir.rs` 的 `tests` 模块:
  ```rust
  async fn make_tree(root: &str) {
      tokio::fs::write(format!("{}/a.txt", root), "").await.unwrap();
      tokio::fs::write(format!("{}/b.log", root), "").await.unwrap();
      tokio::fs::create_dir(format!("{}/sub", root)).await.unwrap();
      tokio::fs::write(format!("{}/sub/c.txt", root), "").await.unwrap();
      tokio::fs::write(format!("{}/.hidden", root), "").await.unwrap();
  }

  #[tokio::test]
  async fn glob_star_pattern_matches_basename() {
      let (_d, root) = temp_dir().await;
      make_tree(&root).await;
      let mut matches = glob(&root, "*.txt", true).await.unwrap();
      matches.sort();
      assert_eq!(matches.len(), 1);
      assert!(matches[0].ends_with("/a.txt"));
  }

  #[tokio::test]
  async fn glob_double_star_recurses() {
      let (_d, root) = temp_dir().await;
      make_tree(&root).await;
      let mut matches = glob(&root, "**/*.txt", true).await.unwrap();
      matches.sort();
      assert_eq!(matches.len(), 2);
      assert!(matches[0].ends_with("/a.txt"));
      assert!(matches[1].ends_with("/sub/c.txt"));
  }

  #[tokio::test]
  async fn glob_char_class_negation() {
      let (_d, root) = temp_dir().await;
      make_tree(&root).await;
      let mut matches = glob(&root, "[!a].*", true).await.unwrap();
      matches.sort();
      assert_eq!(matches.len(), 1);
      assert!(matches[0].ends_with("/b.log"));
  }

  #[tokio::test]
  async fn glob_case_sensitivity_flag() {
      let (_d, root) = temp_dir().await;
      tokio::fs::write(format!("{}/A.TXT", root), "").await.unwrap();
      assert_eq!(glob(&root, "*.txt", true).await.unwrap().len(), 0);
      assert_eq!(glob(&root, "*.txt", false).await.unwrap().len(), 1);
  }

  #[tokio::test]
  async fn glob_detects_symlink_cycle() {
      let (_d, root) = temp_dir().await;
      tokio::fs::create_dir(format!("{}/loop", root)).await.unwrap();
      #[cfg(unix)]
      {
          std::os::unix::fs::symlink(
              format!("{}/loop", root),
              format!("{}/loop/self", root),
          )
          .unwrap();
          let matches = glob(&root, "loop/**/*", true).await.unwrap();
          assert!(matches.len() <= 2, "cycle should not infinite-loop");
      }
  }
  ```
- [ ] **Run it and verify it FAILS**:
  ```bash
  cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p kaos-rs glob_
  ```
  预期:编译失败 `cannot find function glob`。
- [ ] **Write the minimal实现**:
  - `rust-ody/crates/kaos-rs/src/dir.rs` 顶部新增 import 与实现:
    ```rust
    use std::collections::HashSet;
    use crate::glob::glob_pattern_to_regex;

    pub async fn glob(
        path: &str,
        pattern: &str,
        case_sensitive: bool,
    ) -> Result<Vec<String>, io::Error> {
        let base = crate::path::normpath(path);
        let parts: Vec<String> = pattern.split('/').map(|s| s.to_string()).collect();
        let mut visited = HashSet::new();
        if let Ok(meta) = tokio::fs::metadata(&base).await {
            if let Some(key) = cycle_key(&meta) {
                visited.insert(key);
            }
        }
        let mut results = Vec::new();
        glob_walk(&base, &parts, case_sensitive, &visited, &mut results).await?;
        results.sort();
        Ok(results)
    }

    async fn glob_walk(
        base: &str,
        parts: &[String],
        case_sensitive: bool,
        visited: &HashSet<String>,
        results: &mut Vec<String>,
    ) -> Result<(), io::Error> {
        if parts.is_empty() {
            return Ok(());
        }
        let current = &parts[0];
        let rest = &parts[1..];

        if current == "**" {
            if rest.is_empty() {
                results.push(base.to_string());
            } else {
                Box::pin(glob_walk(base, rest, case_sensitive, visited, results)).await?;
            }

            let mut entries = tokio::fs::read_dir(base).await?;
            while let Some(entry) = entries.next_entry().await? {
                let full = crate::path::normpath(&entry.path());
                let meta = match entry.metadata().await {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if meta.is_dir() {
                    if let Some(key) = cycle_key(&meta) {
                        if visited.contains(&key) { continue; }
                        let mut v2 = visited.clone();
                        v2.insert(key);
                        Box::pin(glob_walk(&full, parts, case_sensitive, &v2, results)).await?;
                    } else {
                        Box::pin(glob_walk(&full, parts, case_sensitive, visited, results)).await?;
                    }
                } else if rest.is_empty() {
                    results.push(full);
                }
            }
        } else {
            let re = glob_pattern_to_regex(current, case_sensitive);
            let mut entries = tokio::fs::read_dir(base).await?;
            while let Some(entry) = entries.next_entry().await? {
                let name = entry.file_name().to_string_lossy().to_string();
                if !re.is_match(&name) { continue; }
                let full = crate::path::normpath(&entry.path());
                if rest.is_empty() {
                    results.push(full);
                } else {
                    let meta = match entry.metadata().await {
                        Ok(m) => m,
                        Err(_) => continue,
                    };
                    if meta.is_dir() {
                        if let Some(key) = cycle_key(&meta) {
                            if visited.contains(&key) { continue; }
                            let mut v2 = visited.clone();
                            v2.insert(key);
                            Box::pin(glob_walk(&full, rest, case_sensitive, &v2, results)).await?;
                        } else {
                            Box::pin(glob_walk(&full, rest, case_sensitive, visited, results)).await?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn cycle_key(meta: &std::fs::Metadata) -> Option<String> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let ino = meta.ino();
            if ino == 0 { return None; }
            Some(format!("{}:{}", meta.dev(), ino))
        }
        #[cfg(not(unix))]
        { None }
    }
    ```
  - `rust-ody/crates/kaos-rs/src/kaos.rs` 添加:
    ```rust
    /// Yield paths matching `pattern` under `path`.
    pub async fn glob(
        &self,
        path_str: &str,
        pattern: &str,
        case_sensitive: bool,
    ) -> Result<Vec<String>, std::io::Error> {
        let resolved = self.resolve_path(path_str);
        crate::dir::glob(&resolved, pattern, case_sensitive).await
    }
    ```
- [ ] **Run it and verify it PASSES**:
  ```bash
  cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p kaos-rs glob_
  ```
  预期:5 passed。
- [ ] **Commit**:
  ```bash
  git add rust-ody/crates/kaos-rs/src/dir.rs rust-ody/crates/kaos-rs/src/kaos.rs
  git commit -m "feat(kaos-rs): implement glob with **, char classes and cycle detection"
  ```

---

### Task 5: 实现 `mkdir`

**Depends on:** Task 4

**Files:**
- Modify: `rust-ody/crates/kaos-rs/src/dir.rs`
- Modify: `rust-ody/crates/kaos-rs/src/kaos.rs`

- [ ] **Write the failing test** 在 `rust-ody/crates/kaos-rs/src/dir.rs` 的 `tests` 模块:
  ```rust
  #[tokio::test]
  async fn mkdir_nested_with_parents() {
      let (_d, root) = temp_dir().await;
      let target = format!("{}/a/b/c", root);
      mkdir(&target, true, false).await.unwrap();
      assert!(tokio::fs::metadata(&target).await.unwrap().is_dir());
  }

  #[tokio::test]
  async fn mkdir_existing_dir_without_exist_ok_fails() {
      let (_d, root) = temp_dir().await;
      let target = format!("{}/existing", root);
      tokio::fs::create_dir(&target).await.unwrap();
      let err = mkdir(&target, false, false).await.unwrap_err();
      assert!(err.to_string().contains("already exists"));
  }

  #[tokio::test]
  async fn mkdir_existing_dir_with_exist_ok_succeeds() {
      let (_d, root) = temp_dir().await;
      let target = format!("{}/existing", root);
      tokio::fs::create_dir(&target).await.unwrap();
      mkdir(&target, false, true).await.unwrap();
  }

  #[tokio::test]
  async fn mkdir_file_collision_with_exist_ok_fails() {
      let (_d, root) = temp_dir().await;
      let target = format!("{}/file.txt", root);
      tokio::fs::write(&target, "x").await.unwrap();
      let err = mkdir(&target, false, true).await.unwrap_err();
      assert!(err.to_string().contains("not a directory"));
  }

  #[tokio::test]
  async fn mkdir_existing_with_parents_and_no_exist_ok_fails() {
      let (_d, root) = temp_dir().await;
      let target = format!("{}/a", root);
      tokio::fs::create_dir(&target).await.unwrap();
      let err = mkdir(&target, true, false).await.unwrap_err();
      assert!(err.to_string().contains("already exists"));
  }
  ```
- [ ] **Run it and verify it FAILS**:
  ```bash
  cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p kaos-rs mkdir_
  ```
  预期:编译失败 `cannot find function mkdir`。
- [ ] **Write the minimal实现**:
  - `rust-ody/crates/kaos-rs/src/dir.rs`:
    ```rust
    pub async fn mkdir(path: &str, parents: bool, exist_ok: bool) -> Result<(), KaosFileExistsError> {
        if parents {
            if !exist_ok {
                match tokio::fs::metadata(path).await {
                    Ok(m) if m.is_dir() => {
                        return Err(KaosFileExistsError::new(format!("{} already exists", path)));
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(_) => {}
                }
            }
            tokio::fs::create_dir_all(path)
                .await
                .map_err(|e| KaosFileExistsError::new(e.to_string()))?;
            Ok(())
        } else {
            match tokio::fs::create_dir(path).await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && exist_ok => {
                    let m = tokio::fs::metadata(path)
                        .await
                        .map_err(|e| KaosFileExistsError::new(e.to_string()))?;
                    if m.is_dir() {
                        Ok(())
                    } else {
                        Err(KaosFileExistsError::new(format!(
                            "{} already exists but is not a directory",
                            path
                        )))
                    }
                }
                Err(e) => Err(KaosFileExistsError::new(e.to_string())),
            }
        }
    }
    ```
  - `rust-ody/crates/kaos-rs/src/kaos.rs` 添加:
    ```rust
    /// Create a directory at `path`.
    pub async fn mkdir(
        &self,
        path_str: &str,
        parents: bool,
        exist_ok: bool,
    ) -> Result<(), crate::errors::KaosFileExistsError> {
        let resolved = self.resolve_path(path_str);
        crate::dir::mkdir(&resolved, parents, exist_ok).await
    }
    ```
- [ ] **Run it and verify it PASSES**:
  ```bash
  cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p kaos-rs mkdir_
  ```
  预期:5 passed。
- [ ] **Commit**:
  ```bash
  git add rust-ody/crates/kaos-rs/src/dir.rs rust-ody/crates/kaos-rs/src/kaos.rs
  git commit -m "feat(kaos-rs): implement mkdir with parents/existOk parity"
  ```

---

### Task 6: Rust golden runner 扩展 + L1 目录 fixture

**Depends on:** Task 5

**Files:**
- Modify: `rust-ody/crates/kaos-rs/src/golden.rs`
- Create: `packages/integration-tests/src/parity/fixtures/kaos/l1-directory-ops.json`
- Modify: `rust-ody/crates/kaos-rs/tests/golden.rs`

- [ ] **Write the failing test** 先加 fixture 与 runner,然后运行:
  ```bash
  cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p kaos-rs l1_directory_ops_match_fixture
  ```
  预期:测试或 fixture 未找到,失败。
- [ ] **Write the minimal实现**:
  - `rust-ody/crates/kaos-rs/src/golden.rs`:
    1. 更新 `setup_files` 以支持目录型 key(以 `/` 结尾):
       ```rust
       pub async fn setup_files(temp_dir: &PathBuf, files: &FileSet) -> Result<(), Box<dyn std::error::Error>> {
           for (rel_path, data) in files {
               let full = temp_dir.join(rel_path);
               if rel_path.ends_with('/') {
                   tokio::fs::create_dir_all(&full).await?;
               } else {
                   if let Some(parent) = full.parent() {
                       tokio::fs::create_dir_all(parent).await?;
                   }
                   tokio::fs::write(&full, data).await?;
               }
           }
           Ok(())
       }
       ```
    2. `Op` enum 新增变体:
       ```rust
       Stat {
           path: String,
           #[serde(default = "default_true")]
           follow_symlinks: bool,
           #[serde(default)]
           files: FileSet,
       },
       Iterdir {
           path: String,
           #[serde(default)]
           files: FileSet,
       },
       Glob {
           path: String,
           pattern: String,
           #[serde(rename = "caseSensitive", default = "default_true")]
           case_sensitive: bool,
           #[serde(default)]
           files: FileSet,
       },
       Mkdir {
           path: String,
           #[serde(default)]
           parents: bool,
           #[serde(rename = "existOk", default)]
           exist_ok: bool,
           #[serde(default)]
           files: FileSet,
       },
       ```
    2. 添加 helper:
       ```rust
       fn default_true() -> bool { true }

       fn canonical_io_error(e: &std::io::Error) -> String {
           match e.kind() {
               std::io::ErrorKind::NotFound => "not found".to_string(),
               std::io::ErrorKind::AlreadyExists => "already exists".to_string(),
               std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
               _ => e.to_string(),
           }
       }

       fn relativize(path: &str, temp_dir: Option<&PathBuf>) -> String {
           let p = crate::path::normpath(path);
           if let Some(td) = temp_dir {
               let base = crate::path::normpath(td);
               if let Some(stripped) = p.strip_prefix(&format!("{}/", base)) {
                   return stripped.to_string();
               }
           }
           p
       }
       ```
    3. `run_case_async` 新增分支:
       ```rust
       Op::Stat { path, follow_symlinks, files: _ } => {
           let p = resolve(temp_dir, path);
           match crate::dir::stat(&p, *follow_symlinks).await {
               Ok(s) => CaseResult::ok(serde_json::json!({
                   "isDir": s.is_dir(),
                   "size": s.st_size,
               })),
               Err(e) => CaseResult::err(canonical_io_error(&e)),
           }
       }
       Op::Iterdir { path, files: _ } => {
           let p = resolve(temp_dir, path);
           match crate::dir::iterdir(&p).await {
               Ok(mut entries) => {
                   entries = entries.into_iter().map(|e| relativize(&e, temp_dir)).collect();
                   entries.sort();
                   CaseResult::ok(serde_json::to_value(entries).unwrap())
               }
               Err(e) => CaseResult::err(canonical_io_error(&e)),
           }
       }
       Op::Glob { path, pattern, case_sensitive, files: _ } => {
           let p = resolve(temp_dir, path);
           match crate::dir::glob(&p, pattern, *case_sensitive).await {
               Ok(mut matches) => {
                   matches = matches.into_iter().map(|m| relativize(&m, temp_dir)).collect();
                   matches.sort();
                   CaseResult::ok(serde_json::to_value(matches).unwrap())
               }
               Err(e) => CaseResult::err(canonical_io_error(&e)),
           }
       }
       Op::Mkdir { path, parents, exist_ok, files: _ } => {
           let p = resolve(temp_dir, path);
           match crate::dir::mkdir(&p, *parents, *exist_ok).await {
               Ok(()) => {
                   match tokio::fs::metadata(&p).await {
                       Ok(m) if m.is_dir() => CaseResult::ok(serde_json::json!({ "created": true })),
                       Ok(_) => CaseResult::err("created path is not a directory"),
                       Err(e) => CaseResult::err(canonical_io_error(&e)),
                   }
               }
               Err(e) => CaseResult::err(e.to_string()),
           }
       }
       ```
       在 `StatResult` 上需要 `is_dir()` 方法:
       ```rust
       impl StatResult {
           pub fn is_dir(&self) -> bool {
               (self.st_mode & 0o170000) == 0o040000
           }
       }
       ```
    4. 更新 `needs_tempdir` 与 `files_for_op` 以包含新增 op。
  - `rust-ody/crates/kaos-rs/tests/golden.rs` 添加:
    ```rust
    #[tokio::test]
    async fn l1_directory_ops_match_fixture() {
        assert_fixture("l1-directory-ops.json").await;
    }
    ```
  - 创建 `packages/integration-tests/src/parity/fixtures/kaos/l1-directory-ops.json`:
    ```json
    {
      "version": 1,
      "cases": [
        {
          "name": "stat file vs directory",
          "op": {
            "type": "stat",
            "path": "file.txt",
            "files": { "file.txt": [104, 101, 108, 108, 111] }
          },
          "expected": { "result": { "isDir": false, "size": 5 } }
        },
        {
          "name": "stat directory",
          "op": {
            "type": "stat",
            "path": "dir",
            "files": { "dir/": [] }
          },
          "expected": { "result": { "isDir": true, "size": 0 } }
        },
        {
          "name": "stat not found",
          "op": {
            "type": "stat",
            "path": "missing.txt"
          },
          "expected": { "error": "not found" }
        },
        {
          "name": "iterdir returns sorted entries",
          "op": {
            "type": "iterdir",
            "path": ".",
            "files": { "a.txt": [], "b.txt": [] }
          },
          "expected": { "result": ["a.txt", "b.txt"] }
        },
        {
          "name": "glob star txt",
          "op": {
            "type": "glob",
            "path": ".",
            "pattern": "*.txt",
            "caseSensitive": true,
            "files": { "a.txt": [], "b.log": [] }
          },
          "expected": { "result": ["a.txt"] }
        },
        {
          "name": "glob double star recursion",
          "op": {
            "type": "glob",
            "path": ".",
            "pattern": "**/*.txt",
            "caseSensitive": true,
            "files": { "a.txt": [], "sub/c.txt": [] }
          },
          "expected": { "result": ["a.txt", "sub/c.txt"] }
        },
        {
          "name": "glob char class negation",
          "op": {
            "type": "glob",
            "path": ".",
            "pattern": "[!a].*",
            "caseSensitive": true,
            "files": { "a.txt": [], "b.log": [] }
          },
          "expected": { "result": ["b.log"] }
        },
        {
          "name": "glob case insensitive",
          "op": {
            "type": "glob",
            "path": ".",
            "pattern": "*.TXT",
            "caseSensitive": false,
            "files": { "a.txt": [] }
          },
          "expected": { "result": ["a.txt"] }
        },
        {
          "name": "mkdir nested with parents",
          "op": {
            "type": "mkdir",
            "path": "a/b/c",
            "parents": true,
            "existOk": false
          },
          "expected": { "result": { "created": true } }
        },
        {
          "name": "mkdir existing without existOk fails",
          "op": {
            "type": "mkdir",
            "path": "existing",
            "parents": false,
            "existOk": false,
            "files": { "existing/": [] }
          },
          "expected": { "error": "already exists" }
        },
        {
          "name": "mkdir file collision with existOk fails",
          "op": {
            "type": "mkdir",
            "path": "file.txt",
            "parents": false,
            "existOk": true,
            "files": { "file.txt": [120] }
          },
          "expected": { "error": "file.txt already exists but is not a directory" }
        }
      ]
    }
    ```
- [ ] **Run it and verify it PASSES**:
  ```bash
  cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p kaos-rs l1_directory_ops_match_fixture
  ```
  预期:1 passed。
- [ ] **Commit**:
  ```bash
  git add rust-ody/crates/kaos-rs/src/golden.rs rust-ody/crates/kaos-rs/tests/golden.rs packages/integration-tests/src/parity/fixtures/kaos/l1-directory-ops.json
  git commit -m "feat(kaos-rs): extend golden runner and add l1-directory-ops fixture"
  ```

---

### Task 7: TS golden runner 扩展 + TS↔Rust 对照

**Depends on:** Task 6

**Files:**
- Modify: `packages/integration-tests/src/parity/kaos-golden.ts`
- Modify: `packages/integration-tests/test/parity/kaos/l1-golden.test.ts`

- [ ] **Write the failing test** 在 `packages/integration-tests/test/parity/kaos/l1-golden.test.ts`:
  ```ts
  const fixtures = [
    'l1-paths.json',
    'l1-glob-patterns.json',
    'l1-directory-ops.json',
  ];
  ```
  运行:
  ```bash
  cd /Users/ranwei/workspace/ody-code && pnpm --filter @odysseythink/integration-tests test:parity:kaos
  ```
  预期:TS runner 不认识 directory op 类型,失败。
- [ ] **Write the minimal实现**:
  - `packages/integration-tests/src/parity/kaos-golden.ts`:
    1. 顶部导入更新:
       ```ts
       import { mkdtemp, mkdir, writeFile } from 'node:fs/promises';
       import { tmpdir } from 'node:os';
       import { dirname, join } from 'pathe';
       import {
         detectEnvironment,
         normpath,
         LocalKaos,
         KaosFileExistsError,
       } from '@odysseythink/kaos';
       import { decodeTextWithErrors, globPatternToRegex } from '@odysseythink/kaos/internal';
       ```
    2. `GoldenOp` 类型扩展:
       ```ts
       export type GoldenOp =
         | { type: 'normpath'; input: string }
         | { type: 'detect_environment'; platform: string; arch: string; release: string; env: Record<string, string>; files: string[]; executables: Record<string, string> }
         | { type: 'decode'; encoding: BufferEncoding; mode: 'strict' | 'replace' | 'ignore'; bytes: number[] }
         | { type: 'pattern_to_regex'; pattern: string; caseSensitive: boolean; inputs: string[] }
         | { type: 'stat'; path: string; followSymlinks?: boolean; files?: Record<string, number[]> }
         | { type: 'iterdir'; path: string; files?: Record<string, number[]> }
         | { type: 'glob'; path: string; pattern: string; caseSensitive?: boolean; files?: Record<string, number[]> }
         | { type: 'mkdir'; path: string; parents?: boolean; existOk?: boolean; files?: Record<string, number[]> };
       ```
    3. 新增 helpers:
       ```ts
       function relativeToTemp(p: string, tempDir: string): string {
         const normalized = p.replace(/\\/g, '/');
         const base = tempDir.replace(/\\/g, '/').replace(/\/$/, '');
         return normalized.startsWith(base + '/') ? normalized.slice(base.length + 1) : normalized;
       }

       function canonicalIoError(e: unknown): string {
         if (e instanceof KaosFileExistsError) {
           return e.message.includes('not a directory') ? e.message : 'already exists';
         }
         if (e && typeof e === 'object' && 'code' in e) {
           const code = (e as { code: string }).code;
           if (code === 'ENOENT') return 'not found';
           if (code === 'EEXIST') return 'already exists';
           if (code === 'EACCES') return 'permission denied';
         }
         return String(e);
       }

       async function setupTempDir(fixture: FixtureFile): Promise<string> {
         const files = collectFiles(fixture);
         const dir = await mkdtemp(join(tmpdir(), 'kaos-golden-'));
         for (const [rel, bytes] of Object.entries(files)) {
           const full = join(dir, rel);
           if (rel.endsWith('/')) {
             await mkdir(full, { recursive: true });
           } else {
             await mkdir(dirname(full), { recursive: true });
             await writeFile(full, Buffer.from(bytes));
           }
         }
         return dir;
       }

       function collectFiles(fixture: FixtureFile): Record<string, number[]> {
         const out: Record<string, number[]> = {};
         for (const c of fixture.cases) {
           const op = c.op as { files?: Record<string, number[]> };
           if (op.files) Object.assign(out, op.files);
         }
         return out;
       }
       ```
    4. 更新 `runTsGolden`:
       ```ts
       export async function runTsGolden(fixture: FixtureFile): Promise<Record<string, unknown>> {
         const tempDir = await setupTempDir(fixture);
         const kaos = await LocalKaos.create();
         await kaos.chdir(tempDir);
         const out: Record<string, unknown> = {};
         for (const c of fixture.cases) {
           out[c.name] = await runTsCase(c, kaos, tempDir);
         }
         return out;
       }
       ```
    5. 更新 `runTsCase` 签名并新增分支:
       ```ts
       async function runTsCase(
         c: GoldenCase,
         kaos: LocalKaos,
         tempDir: string,
       ): Promise<unknown> {
         const op = c.op;
         switch (op.type) {
           case 'normpath':
             return { result: normpath(op.input) };
           case 'detect_environment': {
             const files = new Set(op.files);
             const env = await detectEnvironment({
               platform: op.platform,
               arch: op.arch,
               release: op.release,
               env: op.env,
               isFile: async (p) => files.has(p),
               findExecutable: async (name) => op.executables[name],
             });
             return { result: env };
           }
           case 'decode': {
             const buf = Buffer.from(op.bytes);
             try {
               const result = decodeTextWithErrors(buf, op.encoding, op.mode);
               return { result };
             } catch {
               return { error: 'decode error' };
             }
           }
           case 'pattern_to_regex': {
             const re = globPatternToRegex(op.pattern, op.caseSensitive);
             const matches = op.inputs.map((input) => re.test(input));
             const source = op.caseSensitive ? re.source : `(?i)${re.source}`;
             return { result: { regex: source, matches } };
           }
           case 'stat': {
             try {
               const s = await kaos.stat(op.path, { followSymlinks: op.followSymlinks ?? true });
               return { result: { isDir: s.isDirectory(), size: s.stSize } };
             } catch (e) {
               return { error: canonicalIoError(e) };
             }
           }
           case 'iterdir': {
             try {
               const entries: string[] = [];
               for await (const p of kaos.iterdir(op.path)) {
                 entries.push(relativeToTemp(p, tempDir!));
               }
               entries.sort();
               return { result: entries };
             } catch (e) {
               return { error: canonicalIoError(e) };
             }
           }
           case 'glob': {
             try {
               const matches: string[] = [];
               for await (const p of kaos.glob(op.path, op.pattern, { caseSensitive: op.caseSensitive ?? true })) {
                 matches.push(relativeToTemp(p, tempDir!));
               }
               matches.sort();
               return { result: matches };
             } catch (e) {
               return { error: canonicalIoError(e) };
             }
           }
           case 'mkdir': {
             try {
               await kaos.mkdir(op.path, { parents: op.parents ?? false, existOk: op.existOk ?? false });
               return { result: { created: true } };
             } catch (e) {
               return { error: canonicalIoError(e) };
             }
           }
         }
       }
       ```
- [ ] **Run it and verify it PASSES**:
  ```bash
  cd /Users/ranwei/workspace/ody-code && pnpm --filter @odysseythink/integration-tests test:parity:kaos
  ```
  预期:`l1-directory-ops.json` TS↔Rust 对照通过,3 个 fixture 全绿。
- [ ] **Run whole-tree checks**:
  ```bash
  cd /Users/ranwei/workspace/ody-code/rust-ody && cargo test -p kaos-rs
  cd /Users/ranwei/workspace/ody-code && pnpm -r typecheck
  ```
  预期:`kaos-rs` 全部测试通过;TS 全仓库 `typecheck` 通过。
- [ ] **Commit**:
  ```bash
  git add packages/integration-tests/src/parity/kaos-golden.ts packages/integration-tests/test/parity/kaos/l1-golden.test.ts
  git commit -m "feat(parity): extend TS golden runner and enable l1-directory-ops TS↔Rust parity"
  ```

---

## Global Self-Review

- [ ] 1. **Spec-coverage table**:4.1.1 全部 5 个条目 + G4-1-1 门已映射到 Task 1–7,无 GAP。
- [ ] 2. **Placeholder scan**:所有任务均给出完整代码、命令、断言,无 TODO/TBD/"later"。
- [ ] 3. **No phantom tasks**:每个任务都有文件变更、测试、commit;零 `--allow-empty`。
- [ ] 4. **Dependency soundness**:Task N 仅依赖 Task N-1 或 4.1.0;无 forward reference。
- [ ] 5. **Caller & build soundness**:Task 7 修改共享 TS runner 后运行 `pnpm -r typecheck`;Rust 侧修改 `Kaos` 方法后运行 `cargo test -p kaos-rs`。未改动其他 crate 的公共签名。
- [ ] 6. **Test-the-risk**:stat 区分文件/目录、iterdir 返回路径、glob 循环检测与大小写、mkdir `parents`/`existOk` 组合均有行为断言。
- [ ] 7. **Type consistency**:Rust `StatResult` 字段名、TS `GoldenOp` 变体名、fixture 字段名(`followSymlinks`/`caseSensitive`/`existOk`)在 Task 6/7 中保持一致。
<!-- e2e-enriched -->

### Task 8: Generate and run E2E tests

Based on the changed files, validate the following areas:
- /Users/ranwei/workspace/ody-code/apps/ody-code/src/cli (priority: important)
- /Users/ranwei/workspace/ody-code/apps/ody-code/src (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core-shared/src/errors (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/background (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/compaction (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/config (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/context (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/cron (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/injection (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/permission (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/permission/policies (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/records (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/replay (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/session-mode (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/skill (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/tool (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/turn (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/usage (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/profile (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/rpc (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session/checkpoint (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session/export (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/skill/builtin (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/skill (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/background (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/collaboration (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/file (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/game-design (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/goal (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/idea (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/office-hours (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/planning (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/shell (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/state (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/visual (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/web (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/cron (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/cron/harness (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/harness (priority: important)
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity (priority: important)
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity/fixtures (priority: important)
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity/scenarios (priority: important)
- /Users/ranwei/workspace/ody-code/packages/node-sdk/scripts (priority: important)
- /Users/ranwei/workspace/ody-code/packages/node-sdk/src (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

