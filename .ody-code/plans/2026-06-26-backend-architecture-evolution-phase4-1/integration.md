# Part 3 — L1 Golden Integration

本 Part 交付 Phase 4.1.0 的 L1 golden fixtures（路径/环境、文本解码、glob pattern）以及 TS-vs-Rust 的 golden harness，最后接入 CI 作为硬门。

---

### Task 7: Create L1 golden fixtures

**Depends on:** Task 2, Task 3, Task 4, Task 5

**Files:**
- Create: `packages/integration-tests/src/parity/fixtures/kaos/l1-paths.json`
- Create: `packages/integration-tests/src/parity/fixtures/kaos/l1-text-decode.json`
- Create: `packages/integration-tests/src/parity/fixtures/kaos/l1-glob-patterns.json`

**Steps:**

- [ ] Create directory `packages/integration-tests/src/parity/fixtures/kaos`.
- [ ] Write `l1-paths.json`:
  ```json
  {
    "version": 1,
    "cases": [
      {
        "name": "normpath collapses dot segments",
        "op": { "type": "normpath", "input": "/foo/bar/../baz" },
        "expected": { "result": "/foo/baz" }
      },
      {
        "name": "normpath collapses leading dot",
        "op": { "type": "normpath", "input": "/foo/./bar" },
        "expected": { "result": "/foo/bar" }
      },
      {
        "name": "normpath collapses double slashes",
        "op": { "type": "normpath", "input": "foo//bar/../baz" },
        "expected": { "result": "foo/baz" }
      },
      {
        "name": "normpath keeps leading dotdot above root",
        "op": { "type": "normpath", "input": "../../foo" },
        "expected": { "result": "../../foo" }
      },
      {
        "name": "detect environment on posix with bash",
        "op": {
          "type": "detect_environment",
          "platform": "darwin",
          "arch": "arm64",
          "release": "23.0.0",
          "env": {},
          "files": ["/bin/bash"],
          "executables": {}
        },
        "expected": {
          "osKind": "macOS",
          "osArch": "arm64",
          "osVersion": "23.0.0",
          "shellName": "bash",
          "shellPath": "/bin/bash"
        }
      },
      {
        "name": "detect environment on windows with override",
        "op": {
          "type": "detect_environment",
          "platform": "win32",
          "arch": "x86_64",
          "release": "10.0.0",
          "env": { "ODY_SHELL_PATH": "D:\\custom\\bash.exe" },
          "files": ["D:\\custom\\bash.exe"],
          "executables": {}
        },
        "expected": {
          "osKind": "Windows",
          "osArch": "x86_64",
          "osVersion": "10.0.0",
          "shellName": "bash",
          "shellPath": "D:\\custom\\bash.exe"
        }
      },
      {
        "name": "detect environment falls back to sh when bash missing",
        "op": {
          "type": "detect_environment",
          "platform": "linux",
          "arch": "x86_64",
          "release": "6.0.0",
          "env": {},
          "files": [],
          "executables": {}
        },
        "expected": {
          "osKind": "Linux",
          "osArch": "x86_64",
          "osVersion": "6.0.0",
          "shellName": "sh",
          "shellPath": "/bin/sh"
        }
      },
      {
        "name": "detect environment infers git bash from git exe",
        "op": {
          "type": "detect_environment",
          "platform": "win32",
          "arch": "x86_64",
          "release": "10.0.0",
          "env": {},
          "files": ["C:\\Program Files\\Git\\bin\\bash.exe"],
          "executables": { "git.exe": "C:\\Program Files\\Git\\cmd\\git.exe" }
        },
        "expected": {
          "osKind": "Windows",
          "osArch": "x86_64",
          "osVersion": "10.0.0",
          "shellName": "bash",
          "shellPath": "C:\\Program Files\\Git\\bin\\bash.exe"
        }
      }
    ]
  }
  ```
- [ ] Write `l1-text-decode.json`:
  ```json
  {
    "version": 1,
    "cases": [
      {
        "name": "strict rejects invalid utf8",
        "op": { "type": "decode", "encoding": "utf-8", "mode": "strict", "bytes": [104,101,108,108,111,32,255,32,119,111,114,108,100] },
        "expected": { "error": "decode error" }
      },
      {
        "name": "replace substitutes invalid utf8",
        "op": { "type": "decode", "encoding": "utf-8", "mode": "replace", "bytes": [104,101,108,108,111,32,255,32,119,111,114,108,100] },
        "expected": { "result": "hello \uFFFD world" }
      },
      {
        "name": "ignore drops invalid utf8 but preserves valid replacement char",
        "op": { "type": "decode", "encoding": "utf-8", "mode": "ignore", "bytes": [255,191,189,32,104,101,108,108,111] },
        "expected": { "result": "\uFFFD hello" }
      },
      {
        "name": "strict rejects invalid utf16le surrogate",
        "op": { "type": "decode", "encoding": "utf-16le", "mode": "strict", "bytes": [0,216,65,0] },
        "expected": { "error": "decode error" }
      },
      {
        "name": "replace substitutes invalid utf16le surrogate",
        "op": { "type": "decode", "encoding": "utf-16le", "mode": "replace", "bytes": [0,216,65,0] },
        "expected": { "result": "\uFFFDA" }
      },
      {
        "name": "ignore drops invalid utf16le surrogate",
        "op": { "type": "decode", "encoding": "utf-16le", "mode": "ignore", "bytes": [0,216,65,0] },
        "expected": { "result": "A" }
      }
    ]
  }
  ```
- [ ] Write `l1-glob-patterns.json`:
  ```json
  {
    "version": 1,
    "cases": [
      {
        "name": "star matches any chars except slash",
        "op": { "type": "pattern_to_regex", "pattern": "*.txt", "caseSensitive": true, "inputs": ["a.txt", "a/b.txt"] },
        "expected": { "regex": "^[^/]*\\.txt$", "matches": [true, false] }
      },
      {
        "name": "question matches single char",
        "op": { "type": "pattern_to_regex", "pattern": "file?.log", "caseSensitive": true, "inputs": ["file1.log", "file12.log"] },
        "expected": { "regex": "^file[^/].log$", "matches": [true, false] }
      },
      {
        "name": "char class negation with bang",
        "op": { "type": "pattern_to_regex", "pattern": "[!a].txt", "caseSensitive": true, "inputs": ["b.txt", "a.txt"] },
        "expected": { "regex": "^[^a]\\.txt$", "matches": [true, false] }
      },
      {
        "name": "char class literal caret is escaped",
        "op": { "type": "pattern_to_regex", "pattern": "[a^].txt", "caseSensitive": true, "inputs": ["^.txt", "a.txt"] },
        "expected": { "regex": "^[a\\^]\\.txt$", "matches": [true, true] }
      },
      {
        "name": "backslash escapes metachar",
        "op": { "type": "pattern_to_regex", "pattern": "file\\*.txt", "caseSensitive": true, "inputs": ["file*.txt", "fileA.txt"] },
        "expected": { "regex": "^file\\*.txt$", "matches": [true, false] }
      },
      {
        "name": "case insensitive flag works",
        "op": { "type": "pattern_to_regex", "pattern": "*.TXT", "caseSensitive": false, "inputs": ["a.txt"] },
        "expected": { "regex": "(?i)^[^/]*\\.TXT$", "matches": [true] }
      }
    ]
  }
  ```
- [ ] Validate JSON parses:
  ```bash
  node -e "['l1-paths.json','l1-text-decode.json','l1-glob-patterns.json'].forEach(f => JSON.parse(require('fs').readFileSync('packages/integration-tests/src/parity/fixtures/kaos/' + f)))"
  ```
  Expected: command exits `0` with no output.
- [ ] Commit: `test(integration-tests): add L1 kaos golden fixtures`.

---

### Task 8: Build TS-vs-Rust golden harness

**Depends on:** Task 7

**Files:**
- Modify: `rust-ody/crates/kaos-rs/Cargo.toml`
- Modify: `rust-ody/crates/kaos-rs/src/lib.rs`
- Create: `rust-ody/crates/kaos-rs/src/golden.rs`
- Create: `rust-ody/crates/kaos-rs/src/bin/golden.rs`
- Create: `rust-ody/crates/kaos-rs/tests/golden.rs`
- Modify: `packages/kaos/package.json`
- Create: `packages/integration-tests/src/parity/kaos-golden.ts`
- Create: `packages/integration-tests/test/parity/kaos/l1-golden.test.ts`
- Modify: `packages/integration-tests/package.json`

**Steps:**

- [ ] Update `rust-ody/crates/kaos-rs/Cargo.toml` to add serde, serde_json and the `kaos-golden` binary:
  ```toml
  [package]
  name = "kaos-rs"
  version = "0.1.0"
  edition = "2021"
  description = "Rust implementation of the KAOS execution environment"
  license = "MIT"

  [dependencies]
  dirs = "5"
  encoding_rs = "0.8"
  path-clean = "1"
  regex = "1"
  serde = { workspace = true }
  serde_json = { workspace = true }
  thiserror = "1"
  tokio = { workspace = true }
  which = "6"

  [dev-dependencies]
  tempfile = "3"
  tokio-test = "0.4"

  [[bin]]
  name = "kaos-golden"
  path = "src/bin/golden.rs"
  ```
- [ ] Add `pub mod golden;` to `rust-ody/crates/kaos-rs/src/lib.rs`:
  ```rust
  pub mod buffered;
  pub mod environment;
  pub mod glob;
  pub mod golden;
  pub mod kaos;
  pub mod path;
  pub mod text;
  ```
- [ ] Write the failing Rust integration test in `tests/golden.rs`:
  ```rust
  use std::path::PathBuf;

  use kaos_rs::golden::{run_fixture_file, FixtureFile};

  fn fixture_path(name: &str) -> PathBuf {
      let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
      path.push("../../packages/integration-tests/src/parity/fixtures/kaos");
      path.push(name);
      path
  }

  fn assert_fixture(name: &str) {
      let path = fixture_path(name);
      let content = std::fs::read_to_string(&path).unwrap();
      let fixture: FixtureFile = serde_json::from_str(&content).unwrap();
      let actual = run_fixture_file(path.to_str().unwrap()).unwrap();
      for case in &fixture.cases {
          let actual_result = actual
              .get(&case.name)
              .unwrap_or_else(|| panic!("missing result for case {}", case.name));
          let actual_value = serde_json::to_value(actual_result).unwrap();
          assert_eq!(
              actual_value, case.expected,
              "fixture {} case '{}' mismatch",
              name, case.name
          );
      }
  }

  #[test]
  fn l1_paths_match_fixture() {
      assert_fixture("l1-paths.json");
  }

  #[test]
  fn l1_text_decode_match_fixture() {
      assert_fixture("l1-text-decode.json");
  }

  #[test]
  fn l1_glob_patterns_match_fixture() {
      assert_fixture("l1-glob-patterns.json");
  }
  ```
  Run:
  ```bash
  cd rust-ody && cargo test -p kaos-rs --test golden
  ```
  Expected: compilation fails because `kaos_rs::golden` and `run_fixture_file` do not exist.
- [ ] Implement `src/golden.rs`:
  ```rust
  use std::collections::HashMap;

  use serde::{Deserialize, Serialize};
  use serde_json::Value;

  use crate::environment::{detect_environment, Environment, EnvironmentDeps};
  use crate::glob::glob_pattern_to_regex;
  use crate::path::normpath;
  use crate::text::{decode_text_with_errors, ErrorMode};

  #[derive(Debug, Deserialize)]
  pub struct FixtureFile {
      pub version: u32,
      pub cases: Vec<Case>,
  }

  #[derive(Debug, Deserialize)]
  pub struct Case {
      pub name: String,
      pub op: Op,
      pub expected: Value,
  }

  #[derive(Debug, Deserialize)]
  #[serde(tag = "type", rename_all = "snake_case")]
  pub enum Op {
      Normpath { input: String },
      DetectEnvironment {
          platform: String,
          arch: String,
          release: String,
          #[serde(default)]
          env: HashMap<String, String>,
          #[serde(default)]
          files: Vec<String>,
          #[serde(default)]
          executables: HashMap<String, String>,
      },
      Decode {
          encoding: String,
          mode: Mode,
          bytes: Vec<u8>,
      },
      PatternToRegex {
          pattern: String,
          #[serde(rename = "caseSensitive")]
          case_sensitive: bool,
          inputs: Vec<String>,
      },
  }

  #[derive(Debug, Deserialize, Clone, Copy)]
  #[serde(rename_all = "lowercase")]
  pub enum Mode {
      Strict,
      Replace,
      Ignore,
  }

  impl From<Mode> for ErrorMode {
      fn from(m: Mode) -> Self {
          match m {
              Mode::Strict => ErrorMode::Strict,
              Mode::Replace => ErrorMode::Replace,
              Mode::Ignore => ErrorMode::Ignore,
          }
      }
  }

  #[derive(Debug, Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct CaseResult {
      #[serde(skip_serializing_if = "Option::is_none")]
      pub result: Option<Value>,
      #[serde(skip_serializing_if = "Option::is_none")]
      pub error: Option<String>,
  }

  pub fn run_case(case: &Case) -> CaseResult {
      match &case.op {
          Op::Normpath { input } => CaseResult {
              result: Some(Value::String(normpath(input))),
              error: None,
          },
          Op::DetectEnvironment {
              platform,
              arch,
              release,
              env,
              files,
              executables,
          } => {
              let file_set: std::collections::HashSet<&str> = files.iter().map(|s| s.as_str()).collect();
              let deps = EnvironmentDeps {
                  platform: platform.clone(),
                  arch: arch.clone(),
                  release: release.clone(),
                  env: env.clone(),
                  is_file: Box::new(move |p| file_set.contains(p)),
                  find_executable: Box::new({
                      let executables = executables.clone();
                      move |name| executables.get(name).cloned()
                  }),
              };
              let e = detect_environment(&deps);
              CaseResult {
                  result: Some(serde_json::to_value(EnvOutput::from(e)).unwrap()),
                  error: None,
              }
          }
          Op::Decode {
              encoding,
              mode,
              bytes,
          } => match decode_text_with_errors(bytes, encoding, (*mode).into()) {
              Ok(s) => CaseResult {
                  result: Some(Value::String(s)),
                  error: None,
              },
              Err(_) => CaseResult {
                  result: None,
                  error: Some("decode error".to_string()),
              },
          },
          Op::PatternToRegex {
              pattern,
              case_sensitive,
              inputs,
          } => {
              let re = glob_pattern_to_regex(pattern, *case_sensitive);
              let matches: Vec<bool> = inputs.iter().map(|i| re.is_match(i)).collect();
              CaseResult {
                  result: Some(serde_json::json!({
                      "regex": re.as_str(),
                      "matches": matches,
                  })),
                  error: None,
              }
          }
      }
  }

  #[derive(Serialize)]
  #[serde(rename_all = "camelCase")]
  struct EnvOutput {
      os_kind: String,
      os_arch: String,
      os_version: String,
      shell_name: String,
      shell_path: String,
  }

  impl From<Environment> for EnvOutput {
      fn from(e: Environment) -> Self {
          Self {
              os_kind: e.os_kind,
              os_arch: e.os_arch,
              os_version: e.os_version,
              shell_name: e.shell_name,
              shell_path: e.shell_path,
          }
      }
  }

  pub fn run_fixture_file(
      path: &str,
  ) -> Result<HashMap<String, CaseResult>, Box<dyn std::error::Error>> {
      let content = std::fs::read_to_string(path)?;
      let fixture: FixtureFile = serde_json::from_str(&content)?;
      let mut out = HashMap::new();
      for case in &fixture.cases {
          out.insert(case.name.clone(), run_case(case));
      }
      Ok(out)
  }
  ```
- [ ] Implement the binary in `src/bin/golden.rs`:
  ```rust
  use std::env;

  fn main() -> Result<(), Box<dyn std::error::Error>> {
      let path = env::args()
          .nth(1)
          .ok_or("usage: kaos-golden <fixture.json>")?;
      let results = kaos_rs::golden::run_fixture_file(&path)?;
      println!("{}", serde_json::to_string_pretty(&results)?);
      Ok(())
  }
  ```
- [ ] Run Rust golden tests:
  ```bash
  cd rust-ody && cargo test -p kaos-rs --test golden
  ```
  Expected: `test result: ok. 3 passed; 0 failed`.
- [ ] Build the golden binary:
  ```bash
  cd rust-ody && cargo build -p kaos-rs --bin kaos-golden
  ```
  Expected: binary at `rust-ody/target/debug/kaos-golden`.
- [ ] Expose `@odysseythink/kaos/internal` for integration tests by updating `packages/kaos/package.json`:
  ```json
  "exports": {
    ".": {
      "types": "./src/index.ts",
      "default": "./src/index.ts"
    },
    "./internal": {
      "types": "./src/internal.ts",
      "default": "./src/internal.ts"
    },
    "./ssh": {
      "types": "./src/ssh.ts",
      "default": "./src/ssh.ts"
    }
  }
  ```
  Find every existing import of `@odysseythink/kaos/internal` to confirm none exist:
  ```bash
  grep -rn "@odysseythink/kaos/internal" packages/ apps/
  ```
  Expected: no matches. The only new caller is the integration test written in the next step.
- [ ] Write the failing TS parity test in `packages/integration-tests/test/parity/kaos/l1-golden.test.ts`:
  ```ts
  import { existsSync } from 'node:fs';
  import { execSync } from 'node:child_process';
  import { dirname, join } from 'pathe';
  import { fileURLToPath } from 'node:url';
  import { beforeAll, describe, expect, it } from 'vitest';
  import {
    loadFixture,
    resolveRustGoldenBinary,
    runRustGolden,
    runTsGolden,
  } from '../../../src/parity/kaos-golden';

  function findProjectRoot(): string {
    let current = dirname(fileURLToPath(import.meta.url));
    while (current !== dirname(current)) {
      if (existsSync(join(current, '.git'))) {
        return current;
      }
      current = dirname(current);
    }
    return process.cwd();
  }

  const rootDir = findProjectRoot();
  let binaryPath: string;

  beforeAll(() => {
    binaryPath = resolveRustGoldenBinary(rootDir);
    if (!existsSync(binaryPath)) {
      execSync('cargo build -p kaos-rs --bin kaos-golden', {
        cwd: join(rootDir, 'rust-ody'),
        stdio: 'inherit',
      });
    }
  });

  const fixtures = [
    'l1-paths.json',
    'l1-text-decode.json',
    'l1-glob-patterns.json',
  ];

  describe('kaos L1 golden parity', () => {
    it.each(fixtures)('%s TS matches Rust', async (name) => {
      const fixture = await loadFixture(name);
      const ts = await runTsGolden(fixture);
      const fixturePath = join(
        rootDir,
        'packages',
        'integration-tests',
        'src',
        'parity',
        'fixtures',
        'kaos',
        name,
      );
      const rust = runRustGolden(fixturePath, binaryPath);
      expect(rust).toEqual(ts);
    });
  });
  ```
  Run:
  ```bash
  pnpm --filter @odysseythink/integration-tests vitest run test/parity/kaos/l1-golden.test.ts
  ```
  Expected: compilation fails because `../../../src/parity/kaos-golden` does not exist.
- [ ] Implement `packages/integration-tests/src/parity/kaos-golden.ts`:
  ```ts
  import { existsSync } from 'node:fs';
  import { readFile } from 'node:fs/promises';
  import { spawnSync } from 'node:child_process';
  import { dirname, join } from 'pathe';
  import { fileURLToPath } from 'node:url';
  import { detectEnvironment, normpath } from '@odysseythink/kaos';
  import { decodeTextWithErrors, globPatternToRegex } from '@odysseythink/kaos/internal';

  const __dirname = dirname(fileURLToPath(import.meta.url));
  const fixturesDir = join(__dirname, '..', '..', 'fixtures', 'kaos');

  export interface FixtureFile {
    version: number;
    cases: GoldenCase[];
  }

  export interface GoldenCase {
    name: string;
    op: GoldenOp;
    expected: unknown;
  }

  export type GoldenOp =
    | { type: 'normpath'; input: string }
    | {
        type: 'detect_environment';
        platform: string;
        arch: string;
        release: string;
        env: Record<string, string>;
        files: string[];
        executables: Record<string, string>;
      }
    | { type: 'decode'; encoding: BufferEncoding; mode: 'strict' | 'replace' | 'ignore'; bytes: number[] }
    | { type: 'pattern_to_regex'; pattern: string; caseSensitive: boolean; inputs: string[] };

  export async function loadFixture(name: string): Promise<FixtureFile> {
    const raw = await readFile(join(fixturesDir, name), 'utf8');
    return JSON.parse(raw) as FixtureFile;
  }

  export async function runTsGolden(fixture: FixtureFile): Promise<Record<string, unknown>> {
    const out: Record<string, unknown> = {};
    for (const c of fixture.cases) {
      out[c.name] = await runTsCase(c);
    }
    return out;
  }

  async function runTsCase(c: GoldenCase): Promise<unknown> {
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
        return env;
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
        return { regex: source, matches };
      }
      default:
        throw new Error(`unknown op type ${(op as { type: string }).type}`);
    }
  }

  export function runRustGolden(fixturePath: string, binaryPath: string): Record<string, unknown> {
    const result = spawnSync(binaryPath, [fixturePath], {
      encoding: 'utf8',
      maxBuffer: 16 * 1024 * 1024,
    });
    if (result.error) {
      throw new Error(`failed to run kaos-golden: ${result.error.message}`);
    }
    if (result.status !== 0) {
      throw new Error(`kaos-golden exited ${result.status}: ${result.stderr}`);
    }
    return JSON.parse(result.stdout) as Record<string, unknown>;
  }

  export function resolveRustGoldenBinary(rootDir: string): string {
    const override = process.env['ODY_KAOS_GOLDEN_BINARY_PATH'];
    if (override !== undefined && override.length > 0) {
      return override;
    }
    return join(rootDir, 'rust-ody', 'target', 'debug', 'kaos-golden');
  }
  ```
- [ ] Add a convenience script to `packages/integration-tests/package.json`:
  ```json
  "test:parity:kaos": "vitest run test/parity/kaos"
  ```
- [ ] Run TS L1 parity test:
  ```bash
  pnpm --filter @odysseythink/integration-tests test:parity:kaos
  ```
  Expected: `3 passed` for `kaos L1 golden parity`.
- [ ] Whole-tree typecheck (covers the new `@odysseythink/kaos/internal` export and its callers):
  ```bash
  pnpm -r typecheck
  ```
  Expected: all workspace packages typecheck cleanly.
- [ ] Commit: `test(integration-tests): TS-vs-Rust kaos L1 golden harness`.

---

### Task 9: Run L1 gate and wire CI

**Depends on:** Task 8

**Files:**
- Modify: `.github/workflows/rust-host.yml`

**Steps:**

- [ ] Run the full `kaos-rs` test suite (unit tests + golden integration tests):
  ```bash
  cd rust-ody && cargo test -p kaos-rs
  ```
  Expected: all tests pass, including the three `golden.rs` fixture tests.
- [ ] Run whole-workspace typecheck and the TS L1 golden parity suite:
  ```bash
  pnpm -r typecheck
  pnpm --filter @odysseythink/integration-tests test:parity:kaos
  ```
  Expected: typecheck clean; `3 passed` for `kaos L1 golden parity`.
- [ ] Wire the gate into `.github/workflows/rust-host.yml` by inserting three steps after `Install dependencies`:
  ```yaml
      - name: kaos-rs unit tests
        run: cargo test -p kaos-rs
        working-directory: rust-ody

      - name: Build kaos-golden binary
        run: cargo build -p kaos-rs --bin kaos-golden
        working-directory: rust-ody

      - name: kaos L1 golden parity
        run: pnpm --filter @odysseythink/integration-tests test:parity:kaos
        env:
          ODY_KAOS_GOLDEN_BINARY_PATH: ${{ github.workspace }}/rust-ody/target/debug/kaos-golden
  ```
  These steps must appear before the existing `Phase A3 verification` step so a `kaos-rs` regression blocks the host smoke tests early.
- [ ] Run the same commands locally that CI will run, to verify the wiring:
  ```bash
  cd rust-ody && cargo test -p kaos-rs
  cd rust-ody && cargo build -p kaos-rs --bin kaos-golden
  ODY_KAOS_GOLDEN_BINARY_PATH=$(pwd)/rust-ody/target/debug/kaos-golden pnpm --filter @odysseythink/integration-tests test:parity:kaos
  ```
  Expected: same pass counts as the local verification above.
- [ ] Final whole-workspace build guard:
  ```bash
  cd rust-ody && cargo check --workspace
  ```
  Expected: workspace compiles cleanly; the new `kaos-golden` binary and `kaos-rs` crate do not break `ody-host`/`ody-rust`/`ody-crypto`.
- [ ] Commit: `ci: run kaos-rs tests and L1 golden parity in rust-host workflow`.

---

## Local Self-Review

- [ ] 1. Spec-coverage:

  | 设计 § | Requirement | 覆盖 Task | 状态 |
  |---|---|---|---|
  | 4.1.0.6 | L1 golden path fixture | T7 | covered |
  | 4.1.0.6 | L1 golden text-decode fixture | T7 | covered |
  | 4.1.0.6 | L1 golden glob-pattern fixture | T7 | covered |
  | L1 对照 | TS-vs-Rust golden harness | T8 | covered |
  | G4-1-0 | 全 L1 fixture 绿 + crate 编译 | T9 | covered |
  | CI | `cargo test -p kaos-rs` + parity L1 | T9 | covered |

- [ ] 2. Placeholder scan: 本 Part 无 TODO/TBD；所有 JSON fixture、Rust runner、TS harness、CI YAML 均为可直接执行的完整代码。
- [ ] 3. No phantom tasks: T7 产出 3 个 fixture 文件；T8 产出 Rust golden runner/binary、TS harness、测试；T9 产出 CI 步骤与最终验证；每个 Task 以 commit 收尾。
- [ ] 4. Dependency soundness: T7 依赖 Part 1/2 的函数符号（T2–T5）；T8 依赖 T7；T9 依赖 T8；无反向依赖。
- [ ] 5. Caller & build soundness: T8 新增 `@odysseythink/kaos/internal` 子路径导出，同一 Task 创建其唯一调用方 `kaos-golden.ts` 并以 `pnpm -r typecheck` 验证；T9 以 `cargo check --workspace` 验证 workspace 编译；无既有共享签名被修改。
- [ ] 6. Test-the-risk: T7 fixture 显式覆盖 `normpath` 越界、Windows Git Bash 推断、strict/replace/ignore 解码、字符类 `!`/`^` 与大小写开关；T8 的 Rust golden 测试逐 case 断言实际输出与 fixture expected 一致，TS parity 测试断言 Rust 输出与 TS 输出逐字段相等；T9 以 crate 全绿和 L1 parity 绿作为硬门。
- [ ] 7. Type consistency: fixture 中 `detect_environment` 的字段名（`platform`/`arch`/`release`/`env`/`files`/`executables`）与 TS `EnvironmentDeps`、Rust `EnvironmentDeps` 一致；输出对象统一使用 camelCase（Rust 通过 `#[serde(rename_all = "camelCase")]`），与 TS `Environment` 对象字段一致。
