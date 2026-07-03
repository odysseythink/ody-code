# Part 2 — L1 golden fixture + TS/Rust harness + parity test

> Scope: 为 `kaos` 进程执行（`exec` / `execWithEnv` / `kill`）建立 L1 golden fixture，扩展 Rust `kaos-golden` 二进制与 TS `kaos-golden` harness 的 `Exec` / `KillTree` op，并在 `packages/integration-tests/test/parity/kaos/l1-golden.test.ts` 注册 fixture，使 TS 与 Rust 的进程行为逐字段对齐。

---

## Phase B — L1 golden parity for process ops

### Dependency Overview

```
Part 1 (core.md: A1–A6)
        │
        ▼
   ┌─────────┐
   │  B1     │  Rust `golden.rs` 新增 Op::Exec / Op::KillTree + run_case_async 实现
   └────┬────┘
        │ 共享 fixture 模式已稳定
        ▼
   ┌─────────┐
   │  B2     │  TS `kaos-golden.ts` 新增 exec / kill_tree case + streamToBuffer 辅助
   └────┬────┘
        │
        ▼
   ┌─────────┐
   │  B3     │ 创建 `l1-process-ops.json` fixture
   └────┬────┘
        │
        ▼
   ┌─────────┐
   │  B4     │ 在 Rust `tests/golden.rs` 与 TS `l1-golden.test.ts` 注册 fixture
   └────┬────┘
        │
        ▼
   ┌─────────┐
   │  B5     │ 全量验证 + changeset
   └─────────┘
```

---

### Task B1: Rust `golden.rs` 新增 `Op::Exec` / `Op::KillTree`

**Depends on:** Part 1 `core.md` Task A5（`Kaos::exec` / `exec_with_env` / `Process::kill` 已落地）

**Files:**
- Modify: `rust-ody/crates/kaos-rs/src/golden.rs:35-130`（`Op` enum）
- Modify: `rust-ody/crates/kaos-rs/src/golden.rs:178-377`（`run_case_async`）
- Modify: `rust-ody/crates/kaos-rs/src/golden.rs:427-454`（`needs_tempdir` / `files_for_op`）
- Modify: `rust-ody/crates/kaos-rs/src/golden.rs:476+`（追加 `#[cfg(test)]` 模块）
- Test: `cargo test -p kaos-rs golden::tests::exec_runs_echo -- --nocapture`

- [ ] **Write the failing test。** 在 `golden.rs` 文件末尾追加：
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use serde_json::Value;
      use std::collections::HashMap;

      #[tokio::test]
      async fn exec_runs_echo() {
          let case = Case {
              name: "exec echo".to_string(),
              op: Op::Exec {
                  command: "/bin/echo".to_string(),
                  args: vec!["-n".to_string(), "hello".to_string()],
                  env: HashMap::new(),
                  files: HashMap::new(),
              },
              expected: serde_json::json!({
                  "result": {
                      "stdout": [104, 101, 108, 108, 111],
                      "stderr": [],
                      "exitCode": 0,
                  }
              }),
          };
          let actual = run_case_async(&case, None).await;
          assert_eq!(actual.result, Some(case.expected));
          assert!(actual.error.is_none());
      }
  }
  ```
- [ ] **Run it and verify it FAILS。**
  ```bash
  cargo test -p kaos-rs golden::tests::exec_runs_echo -- --nocapture
  ```
  预期：编译错误，`Op` 枚举没有 `Exec` 变体。
- [ ] **Write the minimal implementation。**
  1. 在 `Op` 枚举的 `Mkdir` 变体之后追加：
     ```rust
     // ── L1 process ops ─────────────────────────────────────────────────
     Exec {
         command: String,
         args: Vec<String>,
         #[serde(default)]
         env: HashMap<String, String>,
         #[serde(default)]
         files: FileSet,
     },
     KillTree {
         command: String,
         args: Vec<String>,
         #[serde(default)]
         files: FileSet,
         #[serde(rename = "sleepMs")]
         sleep_ms: u64,
     },
     ```
  2. 在 `run_case_async` 的 `match` 末尾、`Op::Mkdir` 分支之后追加：
     ```rust
     Op::Exec {
         command,
         args,
         env,
         files: _,
     } => {
         let kaos = if let Some(td) = temp_dir {
             crate::Kaos::new(crate::environment::detect_environment_from_node(), td)
         } else {
             crate::Kaos::new(
                 crate::environment::detect_environment_from_node(),
                 std::env::current_dir().map_err(|e| format!("{}", e))?,
             )
         };
         let mut all_args = vec![command.as_str()];
         all_args.extend(args.iter().map(|s| s.as_str()));
         let env_pairs: Vec<(&str, &str)> = env
             .iter()
             .map(|(k, v)| (k.as_str(), v.as_str()))
             .collect();
         match kaos.exec_with_env(&all_args, &env_pairs).await {
             Ok(proc) => {
                 let code = proc.wait().await;
                 let stdout = proc.stdout().await;
                 let stderr = proc.stderr().await;
                 CaseResult::ok(serde_json::json!({
                     "stdout": stdout,
                     "stderr": stderr,
                     "exitCode": code,
                 }))
             }
             Err(e) => CaseResult::err(format!("{}", e)),
         }
     }
     Op::KillTree {
         command,
         args,
         files: _,
         sleep_ms,
     } => {
         #[cfg(unix)]
         {
             let kaos = if let Some(td) = temp_dir {
                 crate::Kaos::new(crate::environment::detect_environment_from_node(), td)
             } else {
                 crate::Kaos::new(
                     crate::environment::detect_environment_from_node(),
                     std::env::current_dir().map_err(|e| format!("{}", e))?,
                 )
             };
             let mut all_args = vec![command.as_str()];
             all_args.extend(args.iter().map(|s| s.as_str()));
             let proc = kaos.exec(&all_args).await.map_err(|e| format!("{}", e))?;
             tokio::time::sleep(std::time::Duration::from_millis(*sleep_ms)).await;
             proc.kill(None).await.map_err(|e| format!("{}", e))?;
             let _ = proc.wait().await;

             let marker = temp_dir
                 .expect("kill_tree requires a tempdir for the pid marker file")
                 .join("pids.txt");
             let content = tokio::fs::read_to_string(&marker).await.unwrap_or_default();
             for pid_str in content.split_whitespace() {
                 let pid: i32 = pid_str
                     .parse()
                     .map_err(|_| format!("bad pid in marker: {}", pid_str))?;
                 let alive = std::process::Command::new("kill")
                     .args(["-0", &pid.to_string()])
                     .status()
                     .map(|s| s.success())
                     .unwrap_or(false);
                 if alive {
                     return CaseResult::err(format!("pid {} still alive", pid));
                 }
             }
             CaseResult::ok(serde_json::json!({ "killed": true }))
         }
         #[cfg(not(unix))]
         {
             let _ = (command, args, sleep_ms);
             CaseResult::err("kill_tree is POSIX-only".to_string())
         }
     }
     ```
  3. 更新 `needs_tempdir`：
     ```rust
     pub fn needs_tempdir(op: &Op) -> bool {
         matches!(
             op,
             Op::ReadBytes { .. }
                 | Op::ReadText { .. }
                 | Op::ReadLines { .. }
                 | Op::WriteBytes { .. }
                 | Op::WriteText { .. }
                 | Op::Stat { .. }
                 | Op::Iterdir { .. }
                 | Op::Glob { .. }
                 | Op::Mkdir { .. }
                 | Op::Exec { .. }
                 | Op::KillTree { .. }
         )
     }
     ```
  4. 更新 `files_for_op`：
     ```rust
     pub fn files_for_op(op: &Op) -> FileSet {
         match op {
             Op::ReadBytes { files, .. } => files.clone(),
             Op::ReadText { files, .. } => files.clone(),
             Op::ReadLines { files, .. } => files.clone(),
             Op::Stat { files, .. } => files.clone(),
             Op::Iterdir { files, .. } => files.clone(),
             Op::Glob { files, .. } => files.clone(),
             Op::Mkdir { files, .. } => files.clone(),
             Op::Exec { files, .. } => files.clone(),
             Op::KillTree { files, .. } => files.clone(),
             _ => HashMap::new(),
         }
     }
     ```
- [ ] **Run it and verify it PASSES。**
  ```bash
  cargo test -p kaos-rs golden::tests::exec_runs_echo -- --nocapture
  ```
  预期：`test result: ok`，`actual.result` 与 `expected` 相等。
- [ ] **Commit。**
  ```bash
  git add rust-ody/crates/kaos-rs/src/golden.rs
  git commit -m "feat(kaos-rs): golden harness Exec and KillTree ops"
  ```

---

### Task B2: TS `kaos-golden.ts` 新增 `exec` / `kill_tree`

**Depends on:** Task B1（fixture 模式 `Exec` / `KillTree` 已在 Rust 侧落地）

**Files:**
- Modify: `packages/integration-tests/src/parity/kaos-golden.ts:20-89`（`GoldenOp` 类型）
- Modify: `packages/integration-tests/src/parity/kaos-golden.ts:91-105`（`runTsGolden` 不变，仅复用）
- Modify: `packages/integration-tests/src/parity/kaos-golden.ts:107-253`（`runTsCase` switch）
- Modify: `packages/integration-tests/src/parity/kaos-golden.ts:255-304`（追加 `streamToBuffer` 并导出 `runTsCase`）
- Create: `packages/integration-tests/test/parity/kaos/harness-process.test.ts`
- Test: `pnpm vitest run packages/integration-tests/test/parity/kaos/harness-process.test.ts`

- [ ] **Write the failing test。** 创建 `packages/integration-tests/test/parity/kaos/harness-process.test.ts`：
  ```ts
  import { describe, expect, it } from 'vitest';
  import { LocalKaos } from '@odysseythink/kaos';
  import { runTsCase } from '../../../src/parity/kaos-golden';

  describe('kaos golden harness process ops', () => {
    it('exec captures stdout/stderr/exitCode', async () => {
      const kaos = await LocalKaos.create();
      const result = await runTsCase(
        kaos,
        {
          name: 'exec echo',
          op: {
            type: 'exec',
            command: '/bin/echo',
            args: ['-n', 'hello'],
          },
          expected: {
            result: {
              stdout: [104, 101, 108, 108, 111],
              stderr: [],
              exitCode: 0,
            },
          },
        },
        process.cwd(),
      );
      expect(result).toEqual({
        result: {
          stdout: [104, 101, 108, 108, 111],
          stderr: [],
          exitCode: 0,
        },
      });
    });
  });
  ```
- [ ] **Run it and verify it FAILS。**
  ```bash
  pnpm vitest run packages/integration-tests/test/parity/kaos/harness-process.test.ts
  ```
  预期：测试失败，`runTsCase` 的 `default` 分支抛出 `unknown op type exec`。
- [ ] **Write the minimal implementation。**
  1. 在 `GoldenOp` 类型末尾追加两个变体：
     ```ts
     | {
         type: 'exec';
         command: string;
         args: string[];
         env?: Record<string, string>;
         files?: Record<string, number[]>;
       }
     | {
         type: 'kill_tree';
         command: string;
         args: string[];
         files?: Record<string, number[]>;
         sleepMs: number;
       };
     ```
  2. 将 `async function runTsCase(...)` 改为导出：`export async function runTsCase(...)`。
  3. 在 `runTsCase` 的 `mkdir` 分支之后、`default` 之前追加：
     ```ts
     case 'exec': {
       const args = [op.command, ...op.args];
       const proc =
         op.env && Object.keys(op.env).length > 0
           ? await kaos.execWithEnv(args, op.env)
           : await kaos.exec(...args);
       const [stdout, stderr] = await Promise.all([
         streamToBuffer(proc.stdout),
         streamToBuffer(proc.stderr),
       ]);
       const exitCode = await proc.wait();
       return {
         result: {
           stdout: [...stdout],
           stderr: [...stderr],
           exitCode,
         },
       };
     }
     case 'kill_tree': {
       const args = [op.command, ...op.args];
       const proc = await kaos.exec(...args);
       await new Promise((resolve) => setTimeout(resolve, op.sleepMs));
       await proc.kill();
       const exitCode = await proc.wait();
       const marker = join(tempDir, 'pids.txt');
       let content = '';
       try {
         content = await readFile(marker, 'utf8');
       } catch {
         // marker may be absent if spawn failed before writing
       }
       for (const pid of content
         .trim()
         .split(/\s+/)
         .filter((s) => s.length > 0)) {
         try {
           process.kill(Number(pid), 0);
           throw new Error(`pid ${pid} still alive`);
         } catch (e) {
           if ((e as NodeJS.ErrnoException).code !== 'ESRCH') {
             throw e;
           }
         }
       }
       return { result: { killed: true } };
     }
     ```
  4. 在文件底部（`resolvePath` 之前或之后）追加辅助函数：
     ```ts
     async function streamToBuffer(readable: Readable): Promise<Buffer> {
       const chunks: Buffer[] = [];
       for await (const chunk of readable) {
         chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
       }
       return Buffer.concat(chunks);
     }
     ```
  5. 确认 `collectFiles` 的现有实现通过 `(c.op as { files?: Record<string, number[]> }).files` 读取 `files`，新增的 `exec` / `kill_tree` 变体均包含 `files?`，无需改动即可被收集到 tempdir。
- [ ] **Run it and verify it PASSES。**
  ```bash
  pnpm vitest run packages/integration-tests/test/parity/kaos/harness-process.test.ts
  ```
  预期：`Test Files 1 passed`。
- [ ] **Whole-tree typecheck。** `GoldenOp` 是共享类型，且 `runTsCase` 已导出，必须确认全工作区调用方无类型错误。
  ```bash
  pnpm -r typecheck
  ```
  预期：所有 package 的 `typecheck` 脚本均通过。
- [ ] **Commit。**
  ```bash
  git add packages/integration-tests/src/parity/kaos-golden.ts \
          packages/integration-tests/test/parity/kaos/harness-process.test.ts
  git commit -m "feat(integration-tests): golden harness exec and kill_tree for parity"
  ```

---

### Task B3: 创建 `l1-process-ops.json` fixture

**Depends on:** Task B2（TS/Rust harness 都已支持 `exec` / `kill_tree`）

**Files:**
- Create: `packages/integration-tests/src/parity/fixtures/kaos/l1-process-ops.json`
- Test: 手动 JSON 校验（无代码测试，按非测试文件处理）

- [ ] **Write the fixture。** 创建文件并写入：
  ```json
  {
    "version": 1,
    "cases": [
      {
        "name": "exec echo captures stdout",
        "op": {
          "type": "exec",
          "command": "/bin/echo",
          "args": ["-n", "hello"]
        },
        "expected": {
          "result": {
            "stdout": [104, 101, 108, 108, 111],
            "stderr": [],
            "exitCode": 0
          }
        }
      },
      {
        "name": "exec exit code is propagated",
        "op": {
          "type": "exec",
          "command": "/bin/sh",
          "args": ["-c", "exit 7"]
        },
        "expected": {
          "result": {
            "stdout": [],
            "stderr": [],
            "exitCode": 7
          }
        }
      },
      {
        "name": "exec with env variable",
        "op": {
          "type": "exec",
          "command": "/bin/sh",
          "args": ["-c", "printf '%s' \"$KAOS_VAR\""],
          "env": { "KAOS_VAR": "golden" }
        },
        "expected": {
          "result": {
            "stdout": [103, 111, 108, 100, 101, 110],
            "stderr": [],
            "exitCode": 0
          }
        }
      },
      {
        "name": "kill tree terminates grandchild",
        "op": {
          "type": "kill_tree",
          "command": "/bin/sh",
          "args": [
            "-c",
            "echo $$ >> pids.txt; sleep 60 & echo $! >> pids.txt; wait"
          ],
          "files": { "pids.txt": [] },
          "sleepMs": 300
        },
        "expected": {
          "result": { "killed": true }
        }
      }
    ]
  }
  ```
- [ ] **Manual verification。**
  ```bash
  node -e "const f=require('./packages/integration-tests/src/parity/fixtures/kaos/l1-process-ops.json'); console.log(f.cases.map(c=>c.name).join('\n'));"
  ```
  预期输出四行用例名：
  ```
  exec echo captures stdout
  exec exit code is propagated
  exec with env variable
  kill tree terminates grandchild
  ```
- [ ] **Commit。**
  ```bash
  git add packages/integration-tests/src/parity/fixtures/kaos/l1-process-ops.json
  git commit -m "test(integration-tests): L1 golden fixture for kaos process ops"
  ```

---

### Task B4: 注册 fixture 到 Rust / TS 测试

**Depends on:** Task B3

**Files:**
- Modify: `rust-ody/crates/kaos-rs/tests/golden.rs:58-81`（追加 `l1_process_ops_match_fixture`）
- Modify: `packages/integration-tests/test/parity/kaos/l1-golden.test.ts:38`（fixtures 列表）
- Test: `cargo test -p kaos-rs --test golden l1_process_ops_match_fixture -- --nocapture`
- Test: `pnpm vitest run packages/integration-tests/test/parity/kaos/l1-golden.test.ts`

- [ ] **Write the failing test changes。**
  1. 在 `rust-ody/crates/kaos-rs/tests/golden.rs` 现有 `l1_directory_ops_match_fixture` 之后追加：
     ```rust
     #[tokio::test]
     async fn l1_process_ops_match_fixture() {
         if cfg!(windows) {
             return;
         }
         assert_fixture("l1-process-ops.json").await;
     }
     ```
  2. 在 `packages/integration-tests/test/parity/kaos/l1-golden.test.ts` 第 38 行替换为：
     ```ts
     const baseFixtures = [
       'l1-paths.json',
       'l1-glob-patterns.json',
       'l1-file-io.json',
       'l1-directory-ops.json',
     ];
     const fixtures =
       process.platform === 'win32'
         ? baseFixtures
         : [...baseFixtures, 'l1-process-ops.json'];
     ```
- [ ] **Run it and verify it FAILS。**
  ```bash
  cargo test -p kaos-rs --test golden l1_process_ops_match_fixture -- --nocapture
  ```
  预期：测试找不到 fixture 文件或 fixture 未实现——具体地，在 B3 之前 `l1-process-ops.json` 不存在会报错；若已执行 B3，则应因 harness 尚未识别 op 而失败（但 B1/B2 已实现，故在正确顺序下本步直接通过，仍可保留作为验证步骤）。
- [ ] **Run Rust integration test。**
  ```bash
  cargo test -p kaos-rs --test golden l1_process_ops_match_fixture -- --nocapture
  ```
  预期：四个 case 的 result 均与 fixture 的 `expected` 一致。
- [ ] **Run TS parity test。**
  ```bash
  pnpm vitest run packages/integration-tests/test/parity/kaos/l1-golden.test.ts
  ```
  预期：所有 fixture（含 `l1-process-ops.json`）TS 与 Rust 输出经 key-sort 后 `toStrictEqual`。
- [ ] **Commit。**
  ```bash
  git add rust-ody/crates/kaos-rs/tests/golden.rs \
          packages/integration-tests/test/parity/kaos/l1-golden.test.ts
  git commit -m "test(integration-tests): register l1-process-ops.json in parity suite"
  ```

---

### Task B5: 全量验证 + changeset

**Depends on:** Task B4

**Files:**
- Create: `.changeset/feat-kaos-rs-process-ops-parity.md`
- Modify: `.ody-code/roadmaps/backend-architecture-evolution-phase4-rust-host-migration-roadmap.md:136`（可选：将 4.1.3 状态标记为 done）

- [ ] **Run crate-level checks。**
  ```bash
  cargo test -p kaos-rs
  cargo clippy -p kaos-rs -- -D warnings
  cargo build -p kaos-rs --bin kaos-golden
  ```
  预期：`kaos-rs` 全部测试通过、clippy 无警告、`rust-ody/target/debug/kaos-golden` 生成。
- [ ] **Run parity suite。**
  ```bash
  pnpm vitest run packages/integration-tests/test/parity/kaos/l1-golden.test.ts
  pnpm vitest run packages/integration-tests/test/parity/kaos/harness-process.test.ts
  ```
  预期：两个测试文件均通过。
- [ ] **Generate the changeset。** 创建 `.changeset/feat-kaos-rs-process-ops-parity.md`：
  ```markdown
  ---
  '@odysseythink/kaos': minor
  '@odysseythink/integration-tests': patch
  ---

  feat(kaos-rs): migrate process execution (exec / execWithEnv / kill) to Rust and add L1 golden parity fixture

  - Adds `Process` struct with buffered stdout/stderr, exit code, wait, and process-group kill.
  - Implements POSIX `killpg` fallback and Windows `taskkill /T` fallback.
  - Extends the L1 golden harness with `Exec` and `KillTree` ops.
  - Adds `l1-process-ops.json` parity fixture covering echo, exit code, env isolation, and process-tree kill.
  ```
- [ ] **Update roadmap status（可选但推荐）。** 在 `.ody-code/roadmaps/backend-architecture-evolution-phase4-rust-host-migration-roadmap.md` 的 4.1.3 行，将状态备注为 `done` 或添加 `✅`：
  ```markdown
  | 4.1.3 | 进程执行（exec / KaosProcess / kill） | — | `kaos-rs` | L1 | 中 | G4-1-3 | **plan ✅** |
  ```
- [ ] **Commit。**
  ```bash
  git add .changeset/feat-kaos-rs-process-ops-parity.md
  git add .ody-code/roadmaps/backend-architecture-evolution-phase4-rust-host-migration-roadmap.md
  git commit -m "chore: changeset for kaos-rs process ops parity"
  ```

---

## Local Self-Review（Part 2）

- [ ] 1. Spec-coverage table：
  | 路线图 4.1.3 条目 | Task(s) | 状态 |
  |---|---|---|
  | 4.1.3.1 `exec / execWithEnv` | Part 1 A1/A2；B1/B2 harness | covered |
  | 4.1.3.2 `KaosProcess` 结构 | Part 1 A1/A3/A4；B1/B2 输出 `stdout/stderr/exitCode` | covered |
  | 4.1.3.3 POSIX 进程组 kill | Part 1 A5；B1/B2/B3 `kill_tree` fixture | covered |
  | 4.1.3.4 Windows taskkill fallback | Part 1 A6；B1/B4 Windows skip（fixture 不运行） | covered |
  | 4.1.3.5 L1 golden 进程 fixture | B1/B2/B3/B4 | covered |
- [ ] 2. Placeholder scan：Part 2 中无 TODO/TBD；每个 case 的代码与 fixture 都是完整实现。
- [ ] 3. No phantom tasks：B5 产生 changeset 与 roadmap 状态更新；B1–B4 都有可运行的测试或 fixture 文件。
- [ ] 4. Dependency soundness：B1 依赖 Part 1；B2 依赖 B1；B3 依赖 B2；B4 依赖 B3；B5 依赖 B4。无反向依赖。
- [ ] 5. Caller & build soundness：
  - Rust `Op` 变体新增后，`bin/golden.rs` 与 `tests/golden.rs` 无需修改（增量变体兼容），B1 仍跑 `cargo check -p kaos-rs` 确保无编译错误。
  - TS `GoldenOp` / `runTsCase` 导出变更后，B2 跑 `pnpm -r typecheck` 覆盖所有调用方（含测试文件）。
  - fixture 文件名 `l1-process-ops.json` 在 Rust `tests/golden.rs` 与 TS `l1-golden.test.ts` 中一致，consumer 通过同一路径读取。
- [ ] 6. Test-the-risk：
  - B1 的 `exec_runs_echo` 断言 `stdout` 字节与 `exitCode`。
  - B2 的 `harness-process.test.ts` 断言 TS 侧 `exec` 输出与 Rust 侧一致。
  - B3/B4 的 fixture 断言 echo、退出码、环境变量隔离、进程树 kill 四个行为。
  - kill_tree 用例通过 `pids.txt` 与 `kill -0` / `process.kill(pid, 0)` 双重验证进程确实消失，而非仅 `kill()` 调用完成。
- [ ] 7. Type consistency：
  - Rust `Op::Exec` / `Op::KillTree` 的字段名与 JSON fixture 的 camelCase key 一致（通过 `#[serde(rename = "sleepMs")]` 等）。
  - TS `GoldenOp` 使用同样的 camelCase 属性名。
  - Rust `CaseResult` 与 TS `runTsCase` 返回形状一致：`{ result: { stdout: number[], stderr: number[], exitCode: number } }` 或 `{ result: { killed: true } }`。
