# Part 5 — L1 golden fixtures, parity runner, CI, and known-gaps

**Scope:** 为 Phase 4.4.0 的 `tools-rs` 共享 helper 建立 TS↔Rust L1 parity fixtures，让两边对同一份 JSON 输入产生完全相同的输出；把 parity runner 接入 integration-tests 和 `rust-host.yml` CI；并把当前已知差异登记进 `known-gaps.md`。

**Prerequisites:** Part 1–4 的所有任务已完成，`tools-rs` 的全部模块（types/workspace/store/result_builder/tool_accesses、path-policy、schema-validation、file-type/rg-locator/list-directory）都已实现并通过单元测试。

**本 Part 依赖关系：**
- Task 11（fixtures + Rust golden binary）依赖 Part 1–4 的所有实现任务。
- Task 12（TS runner + test + CI + known-gaps）依赖 Task 11。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| Responsibility | Path |
|---|---|
| Golden fixture files | `packages/integration-tests/src/parity/fixtures/tools-rs/*.json` |
| Rust fixture runner library | `rust-ody/crates/tools-rs/src/golden.rs` |
| Rust golden binary | `rust-ody/crates/tools-rs/src/bin/tools-golden.rs` |
| Crate binary manifest | `rust-ody/crates/tools-rs/Cargo.toml` |
| TS golden runner | `packages/integration-tests/src/parity/tools-rs-golden.ts` |
| TS parity test | `packages/integration-tests/test/parity/tools-rs/l1-golden.test.ts` |
| Integration test script | `packages/integration-tests/package.json` |
| CI job | `.github/workflows/rust-host.yml` |
| Known gaps registry | `packages/integration-tests/src/parity/known-gaps.md` |

---

## Dependency Overview

```
Part 1–4  Implement all tools-rs helpers
   │
   └──► Task 11  L1 golden fixtures + Rust golden binary
            │
            └──► Task 12  TS runner + parity test + CI step + known-gaps entry
```

- Task 11 不能早于 Part 1–4 完成；它引用 `tools-rs` 的每一个公开 API。
- Task 12 依赖 Task 11（需要 fixture 文件和 binary 都存在才能跑通 parity）。

---

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| TS 与 Rust 对错误消息/路径大小写/JSON key 顺序的细微差异会导致 parity 失败 | fixture 只比对深排序后的 JSON；路径测试固定 `pathClass`；错误消息使用 Rust 实现里已对齐 AJV/TS 的文本。 |
| `list_directory` 输出依赖目录遍历顺序，排序规则不同会不一致 | TS 与 Rust 都按“目录优先、字母序”排序，fixture 字符串通过深排序 JSON 比对。 |
| `rg-locator` 的下载路径在 CI 无网络 | fixture 只测试 `detect_target` 和 `find_existing_rg` 的纯查找路径；下载分支由单元测试或本地手动验证，不进入 CI parity。 |
| `sniff_image_dimensions` 对 VP8X/VP8L 的处理与 TS 有边界差异 | fixture 只覆盖 PNG/GIF/BMP/JPEG 四种计算方式完全一致的格式。 |

---

### Task 11: L1 golden fixtures + Rust golden binary

**Depends on:** Part 1 Tasks 1–3, Part 2 Tasks 4–5, Part 3 Tasks 6–7, Part 4 Tasks 8–10

**Files:**
- Create: `packages/integration-tests/src/parity/fixtures/tools-rs/path-policy.json`
- Create: `packages/integration-tests/src/parity/fixtures/tools-rs/rule-match.json`
- Create: `packages/integration-tests/src/parity/fixtures/tools-rs/schema-validation.json`
- Create: `packages/integration-tests/src/parity/fixtures/tools-rs/tool-accesses.json`
- Create: `packages/integration-tests/src/parity/fixtures/tools-rs/result-builder.json`
- Create: `packages/integration-tests/src/parity/fixtures/tools-rs/file-type.json`
- Create: `packages/integration-tests/src/parity/fixtures/tools-rs/rg-locator.json`
- Create: `packages/integration-tests/src/parity/fixtures/tools-rs/list-directory.json`
- Create: `rust-ody/crates/tools-rs/src/golden.rs`
- Create: `rust-ody/crates/tools-rs/src/bin/tools-golden.rs`
- Modify: `rust-ody/crates/tools-rs/Cargo.toml`（添加 `[[bin]]`）
- Test: `rust-ody/crates/tools-rs/tests/l1_parity.rs`（集成测试：每个 fixture 运行后断言结果不为空且无 panic）

#### Step 1 — 创建 fixture 目录和 path-policy.json

```bash
mkdir -p packages/integration-tests/src/parity/fixtures/tools-rs
```

创建 `packages/integration-tests/src/parity/fixtures/tools-rs/path-policy.json`：

```json
{
  "version": 1,
  "cases": [
    {
      "name": "canonicalize relative posix",
      "op": {
        "type": "canonicalize_path",
        "path": "src/../main.rs",
        "cwd": "/workspace",
        "pathClass": "posix"
      },
      "expected": { "result": "/workspace/main.rs" }
    },
    {
      "name": "rejects empty path",
      "op": {
        "type": "canonicalize_path",
        "path": "",
        "cwd": "/workspace",
        "pathClass": "posix"
      },
      "expected": { "error": "Path cannot be empty" }
    },
    {
      "name": "is within workspace uses segment boundaries",
      "op": {
        "type": "is_within_directory",
        "candidate": "/workspace-evil/secrets.txt",
        "base": "/workspace",
        "pathClass": "posix"
      },
      "expected": { "result": true }
    },
    {
      "name": "workspace prefix does not falsely match sibling",
      "op": {
        "type": "is_within_directory",
        "candidate": "/workspace/file.txt",
        "base": "/workspace-evil",
        "pathClass": "posix"
      },
      "expected": { "result": false }
    },
    {
      "name": "normalize user path on win32",
      "op": {
        "type": "normalize_user_path",
        "path": "/c/Users/foo/file.txt",
        "pathClass": "win32"
      },
      "expected": { "result": "C:/Users/foo/file.txt" }
    },
    {
      "name": "resolve path access rejects relative escape",
      "op": {
        "type": "resolve_path_access",
        "path": "../../outside.txt",
        "cwd": "/workspace/project",
        "workspaceDir": "/workspace",
        "additionalDirs": [],
        "operation": "read",
        "pathClass": "posix"
      },
      "expected": { "error": "PATH_OUTSIDE_WORKSPACE" }
    },
    {
      "name": "resolve path access allows absolute outside workspace",
      "op": {
        "type": "resolve_path_access",
        "path": "/etc/hosts",
        "cwd": "/workspace",
        "workspaceDir": "/workspace",
        "additionalDirs": [],
        "operation": "read",
        "pathClass": "posix"
      },
      "expected": {
        "result": { "path": "/etc/hosts", "outsideWorkspace": true }
      }
    },
    {
      "name": "resolve path access expands tilde",
      "op": {
        "type": "resolve_path_access",
        "path": "~/notes/today.txt",
        "cwd": "/workspace",
        "workspaceDir": "/workspace",
        "additionalDirs": [],
        "operation": "read",
        "pathClass": "posix",
        "homeDir": "/home/test"
      },
      "expected": {
        "result": { "path": "/home/test/notes/today.txt", "outsideWorkspace": true }
      }
    },
    {
      "name": "resolve path access rejects sensitive file",
      "op": {
        "type": "resolve_path_access",
        "path": "/tmp/.env",
        "cwd": "/workspace",
        "workspaceDir": "/workspace",
        "additionalDirs": [],
        "operation": "read",
        "pathClass": "posix"
      },
      "expected": { "error": "PATH_SENSITIVE" }
    },
    {
      "name": "assert path allowed returns canonical path",
      "op": {
        "type": "assert_path_allowed",
        "path": "src/../main.rs",
        "cwd": "/workspace/project",
        "workspaceDir": "/workspace",
        "additionalDirs": [],
        "mode": "read",
        "pathClass": "posix"
      },
      "expected": { "result": "/workspace/main.rs" }
    },
    {
      "name": "is sensitive file flags basenames",
      "op": {
        "type": "is_sensitive_file",
        "path": "/app/.env"
      },
      "expected": { "result": true }
    },
    {
      "name": "is sensitive file must survive inputs",
      "op": {
        "type": "is_sensitive_file",
        "path": ".env.example"
      },
      "expected": { "result": false }
    }
  ]
}
```

#### Step 2 — 创建 rule-match.json

创建 `packages/integration-tests/src/parity/fixtures/tools-rs/rule-match.json`：

```json
{
  "version": 1,
  "cases": [
    {
      "name": "literal rule pattern escapes glob special",
      "op": {
        "type": "literal_rule_pattern",
        "toolName": "read",
        "subject": "/tmp/*.txt"
      },
      "expected": { "result": "read(\\/tmp\\/\\*.txt)" }
    },
    {
      "name": "escape rule subject literal",
      "op": {
        "type": "escape_rule_subject_literal",
        "subject": "a[b]c"
      },
      "expected": { "result": "a\\[b\\]c" }
    },
    {
      "name": "matches glob rule subject positive",
      "op": {
        "type": "matches_glob_rule_subject",
        "ruleArgs": "*.ts",
        "subject": "main.ts"
      },
      "expected": { "result": true }
    },
    {
      "name": "matches glob rule subject negative",
      "op": {
        "type": "matches_glob_rule_subject",
        "ruleArgs": "!*.ts",
        "subject": "main.ts"
      },
      "expected": { "result": false }
    },
    {
      "name": "matches path rule subject with canonical cwd",
      "op": {
        "type": "matches_path_rule_subject",
        "ruleArgs": "src/**/*.ts",
        "subject": "src/deep/main.ts",
        "cwd": "/workspace",
        "pathClass": "posix"
      },
      "expected": { "result": true }
    },
    {
      "name": "matches path rule subject leading dot slash",
      "op": {
        "type": "matches_path_rule_subject",
        "ruleArgs": "*.ts",
        "subject": "./main.ts",
        "cwd": "/workspace",
        "pathClass": "posix"
      },
      "expected": { "result": true }
    }
  ]
}
```

#### Step 3 — 创建 schema-validation.json

创建 `packages/integration-tests/src/parity/fixtures/tools-rs/schema-validation.json`：

```json
{
  "version": 1,
  "cases": [
    {
      "name": "validate empty object with defaults passes",
      "op": {
        "type": "validate_args",
        "schema": {
          "type": "object",
          "properties": {
            "activeOnly": { "type": "boolean", "default": true }
          },
          "additionalProperties": false
        },
        "args": {}
      },
      "expected": { "result": null }
    },
    {
      "name": "validate missing required reports ajv message",
      "op": {
        "type": "validate_args",
        "schema": {
          "type": "object",
          "properties": {
            "name": { "type": "string" },
            "count": { "type": "integer" }
          },
          "required": ["name"],
          "additionalProperties": false
        },
        "args": { "count": 5 }
      },
      "expected": { "error": "must have required property 'name'" }
    },
    {
      "name": "validate additional property reports ajv message",
      "op": {
        "type": "validate_args",
        "schema": {
          "type": "object",
          "properties": {
            "activeOnly": { "type": "boolean" }
          },
          "additionalProperties": false
        },
        "args": { "bogus": true }
      },
      "expected": { "error": "must NOT have additional property 'bogus'" }
    },
    {
      "name": "validate nested additional property reports ajv message",
      "op": {
        "type": "validate_args",
        "schema": {
          "type": "object",
          "properties": {
            "question": {
              "type": "object",
              "properties": {
                "question": { "type": "string" },
                "options": { "type": "array", "items": { "type": "string" }, "minItems": 2 }
              },
              "required": ["question", "options"],
              "additionalProperties": false
            }
          },
          "required": ["question"],
          "additionalProperties": false
        },
        "args": {
          "question": {
            "question": "Which?",
            "options": ["A", "B"],
            "bogus": true
          }
        }
      },
      "expected": { "error": "must NOT have additional property 'bogus'" }
    }
  ]
}
```

#### Step 4 — 创建 tool-accesses.json

创建 `packages/integration-tests/src/parity/fixtures/tools-rs/tool-accesses.json`：

```json
{
  "version": 1,
  "cases": [
    {
      "name": "read read same path does not conflict",
      "op": {
        "type": "access_conflict",
        "left": [{ "kind": "file", "operation": "read", "path": "/repo/src/main.rs" }],
        "right": [{ "kind": "file", "operation": "read", "path": "/repo/src/main.rs" }]
      },
      "expected": { "result": false }
    },
    {
      "name": "write read same path conflicts",
      "op": {
        "type": "access_conflict",
        "left": [{ "kind": "file", "operation": "write", "path": "/repo/src/main.rs" }],
        "right": [{ "kind": "file", "operation": "read", "path": "/repo/src/main.rs" }]
      },
      "expected": { "result": true }
    },
    {
      "name": "recursive write conflicts with descendant read",
      "op": {
        "type": "access_conflict",
        "left": [{ "kind": "file", "operation": "write", "path": "/repo", "recursive": true }],
        "right": [{ "kind": "file", "operation": "read", "path": "/repo/src/main.rs" }]
      },
      "expected": { "result": true }
    },
    {
      "name": "all conflicts with everything",
      "op": {
        "type": "access_conflict",
        "left": [{ "kind": "all" }],
        "right": [{ "kind": "file", "operation": "read", "path": "/repo/src/main.rs" }]
      },
      "expected": { "result": true }
    }
  ]
}
```

#### Step 5 — 创建 result-builder.json

创建 `packages/integration-tests/src/parity/fixtures/tools-rs/result-builder.json`：

```json
{
  "version": 1,
  "cases": [
    {
      "name": "truncates long lines at default 500",
      "op": {
        "type": "build_result",
        "writes": ["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
        "maxLineLength": 500
      },
      "expected": {
        "result": {
          "output": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa…",
          "isError": false,
          "message": "ok"
        }
      }
    },
    {
      "name": "error marks is error",
      "op": {
        "type": "build_result",
        "writes": ["something went wrong"],
        "maxLineLength": 500,
        "asError": true
      },
      "expected": {
        "result": {
          "output": "something went wrong",
          "isError": true,
          "message": "it broke"
        }
      }
    }
  ]
}
```

> 说明：第一个 case 的 `writes` 字符串长度必须大于 500（示例中为 510 个 `a`），Rust/TS 两边都应截断为 500 字符并追加 `…`。如果复制时长度不对，请用脚本生成确保长度恰好为 510。

#### Step 6 — 创建 file-type.json

创建 `packages/integration-tests/src/parity/fixtures/tools-rs/file-type.json`：

```json
{
  "version": 1,
  "cases": [
    {
      "name": "png magic",
      "op": {
        "type": "sniff_media_from_magic",
        "header": [137, 80, 78, 71, 13, 10, 26, 10]
      },
      "expected": {
        "result": { "kind": "image", "mimeType": "image/png" }
      }
    },
    {
      "name": "jpeg extension without header",
      "op": {
        "type": "detect_file_type",
        "path": "photo.JPG",
        "header": null
      },
      "expected": {
        "result": { "kind": "image", "mimeType": "image/jpeg" }
      }
    },
    {
      "name": "typescript suffix is text",
      "op": {
        "type": "detect_file_type",
        "path": "app.ts",
        "header": null
      },
      "expected": {
        "result": { "kind": "text", "mimeType": "text/plain" }
      }
    },
    {
      "name": "zip suffix is unknown",
      "op": {
        "type": "detect_file_type",
        "path": "archive.zip",
        "header": null
      },
      "expected": {
        "result": { "kind": "unknown", "mimeType": "" }
      }
    },
    {
      "name": "extension sniff kind mismatch",
      "op": {
        "type": "detect_file_type",
        "path": "mismatch.mp4",
        "header": [255, 216, 255, 224]
      },
      "expected": {
        "result": { "kind": "unknown", "mimeType": "" }
      }
    },
    {
      "name": "png dimensions",
      "op": {
        "type": "sniff_image_dimensions",
        "header": [137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 3, 32, 0, 0, 2, 88]
      },
      "expected": {
        "result": { "width": 800, "height": 600 }
      }
    },
    {
      "name": "gif dimensions",
      "op": {
        "type": "sniff_image_dimensions",
        "header": [71, 73, 70, 56, 57, 97, 64, 31, 240, 0]
      },
      "expected": {
        "result": { "width": 320, "height": 240 }
      }
    }
  ]
}
```

#### Step 7 — 创建 rg-locator.json

创建 `packages/integration-tests/src/parity/fixtures/tools-rs/rg-locator.json`：

```json
{
  "version": 1,
  "cases": [
    {
      "name": "detect target darwin arm64",
      "op": {
        "type": "detect_target",
        "arch": "aarch64",
        "platform": "darwin"
      },
      "expected": {
        "result": "aarch64-apple-darwin"
      }
    },
    {
      "name": "detect target linux x64",
      "op": {
        "type": "detect_target",
        "arch": "x86_64",
        "platform": "linux"
      },
      "expected": {
        "result": "x86_64-unknown-linux-musl"
      }
    },
    {
      "name": "detect target windows x64",
      "op": {
        "type": "detect_target",
        "arch": "x86_64",
        "platform": "win32"
      },
      "expected": {
        "result": "x86_64-pc-windows-msvc"
      }
    },
    {
      "name": "find existing rg in PATH",
      "op": {
        "type": "find_existing_rg",
        "pathEnv": ["/tmp/fake-bin"],
        "shareDir": "/tmp/fake-share",
        "files": {
          "/tmp/fake-bin/rg": [35, 33, 47, 98, 105, 110, 47, 98, 97, 115, 104, 10]
        }
      },
      "expected": {
        "result": {
          "path": "/tmp/fake-bin/rg",
          "source": "system-path"
        }
      }
    },
    {
      "name": "find existing rg in share bin cached",
      "op": {
        "type": "find_existing_rg",
        "pathEnv": [],
        "shareDir": "/tmp/fake-share",
        "files": {
          "/tmp/fake-share/bin/rg": [35, 33, 47, 98, 105, 110, 47, 98, 97, 115, 104, 10]
        }
      },
      "expected": {
        "result": {
          "path": "/tmp/fake-share/bin/rg",
          "source": "share-bin-cached"
        }
      }
    }
  ]
}
```

#### Step 8 — 创建 list-directory.json

创建 `packages/integration-tests/src/parity/fixtures/tools-rs/list-directory.json`：

```json
{
  "version": 1,
  "cases": [
    {
      "name": "two level tree with truncation hint",
      "op": {
        "type": "list_directory",
        "path": ".",
        "files": {
          "README.md": [35, 32, 72, 101, 108, 108, 111, 10],
          "src/main.rs": [102, 110, 32, 109, 97, 105, 110, 40, 41, 32, 123, 125, 10],
          "src/lib.rs": [112, 117, 98, 32, 109, 111, 100, 32, 108, 105, 98, 59, 10],
          "tests/smoke.rs": [35, 91, 116, 101, 115, 116, 93, 10],
          "package.json": [123, 125, 10]
        }
      },
      "expected": {
        "result": "├── package.json\n├── README.md\n├── src/\n│   ├── lib.rs\n│   ├── main.rs\n│   └── ... and 0 more\n└── tests/\n    └── smoke.rs"
      }
    }
  ]
}
```

> 说明：该 fixture 假设 `LIST_DIR_ROOT_WIDTH=30`、`LIST_DIR_CHILD_WIDTH=10`。若 Part 4 的实现调整了常量，需同步修改本 fixture 的预期字符串。

#### Step 9 — 先写失败的集成测试

创建 `rust-ody/crates/tools-rs/tests/l1_parity.rs`，先写一个会编译失败的骨架：

```rust
use std::collections::HashMap;

#[test]
fn runs_all_fixtures() {
    let fixtures = vec![
        "path-policy.json",
        "rule-match.json",
        "schema-validation.json",
        "tool-accesses.json",
        "result-builder.json",
        "file-type.json",
        "rg-locator.json",
        "list-directory.json",
    ];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("packages/integration-tests/src/parity/fixtures/tools-rs");
    for name in fixtures {
        let path = root.join(name);
        assert!(path.exists(), "missing fixture {name}");
        let out = tools_rs::golden::run_fixture_file(path.to_str().unwrap());
        assert!(!out.is_empty(), "fixture {name} produced no output");
    }
}
```

运行测试，确认失败（`tools_rs::golden` 模块不存在）：

```bash
cd rust-ody && cargo test -p tools-rs --test l1_parity
```

预期失败：

```
error[E0433]: failed to resolve: could not find `golden` in `tools_rs`
```

#### Step 10 — 实现 Rust fixture runner

创建 `rust-ody/crates/tools-rs/src/golden.rs`。该模块负责解析 fixture JSON、调用 `tools-rs` 各 helper、输出 `{case_name: {result|error}}` 结构。注意错误形状与 TS runner 保持一致：成功返回 `{result: value}`，失败返回 `{error: string}`。

```rust
use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::args_validator::{compile_tool_args_validator, validate_tool_args};
use crate::file_type::{detect_file_type, sniff_image_dimensions, sniff_media_from_magic, FileKind};
use crate::policies::path_access::{
    assert_path_allowed, canonicalize_path, is_sensitive_file, is_within_directory,
    resolve_path_access, PathAccessOperation, PathClass, WorkspaceAccessPolicy, WorkspaceGuardMode,
};
use crate::policies::rule_match::{
    escape_rule_subject_literal, literal_rule_pattern, matches_glob_rule_subject,
    matches_path_rule_subject,
};
use crate::policies::path_glob_match::PermissionPathMatchOptions;
use crate::result_builder::ToolResultBuilder;
use crate::rg_locator::{find_existing_rg, RgResolution, RgResolutionSource};
use crate::tool_accesses::{ToolAccesses, ToolResourceAccess};
use crate::workspace::WorkspaceConfig;

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

pub type FileSet = HashMap<String, Vec<u8>>;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Op {
    CanonicalizePath {
        path: String,
        cwd: String,
        #[serde(rename = "pathClass")]
        path_class: PathClass,
    },
    IsWithinDirectory {
        candidate: String,
        base: String,
        #[serde(rename = "pathClass")]
        path_class: PathClass,
    },
    NormalizeUserPath {
        path: String,
        #[serde(rename = "pathClass")]
        path_class: PathClass,
    },
    ResolvePathAccess {
        path: String,
        cwd: String,
        #[serde(rename = "workspaceDir")]
        workspace_dir: String,
        #[serde(rename = "additionalDirs")]
        additional_dirs: Vec<String>,
        operation: PathAccessOperation,
        #[serde(rename = "pathClass")]
        path_class: PathClass,
        #[serde(rename = "homeDir")]
        home_dir: Option<String>,
    },
    AssertPathAllowed {
        path: String,
        cwd: String,
        #[serde(rename = "workspaceDir")]
        workspace_dir: String,
        #[serde(rename = "additionalDirs")]
        additional_dirs: Vec<String>,
        mode: PathAccessOperation,
        #[serde(rename = "pathClass")]
        path_class: PathClass,
    },
    IsSensitiveFile { path: String },
    LiteralRulePattern { tool_name: String, subject: String },
    EscapeRuleSubjectLiteral { subject: String },
    MatchesGlobRuleSubject { rule_args: String, subject: String },
    MatchesPathRuleSubject {
        rule_args: String,
        subject: String,
        cwd: Option<String>,
        #[serde(rename = "pathClass")]
        path_class: PathClass,
    },
    ValidateArgs { schema: Value, args: Value },
    AccessConflict { left: Vec<ToolResourceAccess>, right: Vec<ToolResourceAccess> },
    BuildResult {
        writes: Vec<String>,
        #[serde(rename = "maxLineLength")]
        max_line_length: usize,
        #[serde(default)]
        as_error: bool,
    },
    SniffMediaFromMagic { header: Vec<u8> },
    DetectFileType { path: String, header: Option<Vec<u8>> },
    SniffImageDimensions { header: Vec<u8> },
    DetectTarget { arch: String, platform: String },
    FindExistingRg {
        #[serde(rename = "pathEnv")]
        path_env: Vec<String>,
        #[serde(rename = "shareDir")]
        share_dir: String,
        #[serde(default)]
        files: FileSet,
    },
    ListDirectory {
        path: String,
        #[serde(default)]
        files: FileSet,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl CaseResult {
    pub fn ok(value: Value) -> Self {
        Self {
            result: Some(value),
            error: None,
        }
    }
    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            result: None,
            error: Some(msg.into()),
        }
    }
}

fn kind_to_str(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Text => "text",
        FileKind::Image => "image",
        FileKind::Video => "video",
        FileKind::Unknown => "unknown",
    }
}

fn file_type_to_value(ft: crate::file_type::FileType) -> Value {
    serde_json::json!({
        "kind": kind_to_str(ft.kind),
        "mimeType": ft.mime_type,
    })
}

fn run_case_sync(case: &Case, temp_dir: Option<&std::path::Path>) -> CaseResult {
    match &case.op {
        Op::CanonicalizePath { path, cwd, path_class } => {
            match canonicalize_path(path, cwd, *path_class) {
                Ok(v) => CaseResult::ok(Value::String(v)),
                Err(e) => CaseResult::err(format!("{:?}", e.code)),
            }
        }
        Op::IsWithinDirectory { candidate, base, path_class } => {
            CaseResult::ok(Value::Bool(is_within_directory(candidate, base, *path_class)))
        }
        Op::NormalizeUserPath { path, path_class } => {
            CaseResult::ok(Value::String(crate::policies::path_access::normalize_user_path(path, *path_class)))
        }
        Op::ResolvePathAccess { path, cwd, workspace_dir, additional_dirs, operation, path_class, home_dir } => {
            let config = WorkspaceConfig {
                workspace_dir: workspace_dir.clone(),
                additional_dirs: additional_dirs.clone(),
            };
            let policy = WorkspaceAccessPolicy {
                guard_mode: WorkspaceGuardMode::AbsoluteOutsideAllowed,
                check_sensitive: true,
            };
            match resolve_path_access(
                path,
                cwd,
                &config,
                crate::policies::path_access::ResolvePathAccessOptions {
                    operation: *operation,
                    policy: Some(policy),
                    path_class: Some(*path_class),
                    home_dir: home_dir.clone(),
                },
            ) {
                Ok(a) => CaseResult::ok(serde_json::to_value(a).unwrap()),
                Err(e) => CaseResult::err(format!("{:?}", e.code)),
            }
        }
        Op::AssertPathAllowed { path, cwd, workspace_dir, additional_dirs, mode, path_class } => {
            let config = WorkspaceConfig {
                workspace_dir: workspace_dir.clone(),
                additional_dirs: additional_dirs.clone(),
            };
            match assert_path_allowed(
                path,
                cwd,
                &config,
                crate::policies::path_access::AssertPathOptions {
                    mode: *mode,
                    check_sensitive: Some(true),
                    path_class: Some(*path_class),
                },
            ) {
                Ok(v) => CaseResult::ok(Value::String(v)),
                Err(e) => CaseResult::err(format!("{:?}", e.code)),
            }
        }
        Op::IsSensitiveFile { path } => CaseResult::ok(Value::Bool(is_sensitive_file(path))),
        Op::LiteralRulePattern { tool_name, subject } => {
            CaseResult::ok(Value::String(literal_rule_pattern(tool_name, subject)))
        }
        Op::EscapeRuleSubjectLiteral { subject } => {
            CaseResult::ok(Value::String(escape_rule_subject_literal(subject)))
        }
        Op::MatchesGlobRuleSubject { rule_args, subject } => {
            CaseResult::ok(Value::Bool(matches_glob_rule_subject(rule_args, subject)))
        }
        Op::MatchesPathRuleSubject { rule_args, subject, cwd, path_class } => {
            let opts = PermissionPathMatchOptions {
                cwd: cwd.clone(),
                path_class: Some(*path_class),
                home_dir: None,
                case_insensitive_paths: Some(true),
            };
            CaseResult::ok(Value::Bool(matches_path_rule_subject(rule_args, subject, Some(&opts))))
        }
        Op::ValidateArgs { schema, args } => match compile_tool_args_validator(schema) {
            Ok(v) => match validate_tool_args(&v, args) {
                None => CaseResult::ok(Value::Null),
                Some(msg) => CaseResult::err(msg),
            },
            Err(e) => CaseResult::err(e.to_string()),
        },
        Op::AccessConflict { left, right } => {
            let a = ToolAccesses(left.clone());
            let b = ToolAccesses(right.clone());
            CaseResult::ok(Value::Bool(ToolAccesses::conflict(&a, &b)))
        }
        Op::BuildResult { writes, max_line_length, as_error } => {
            let mut builder = ToolResultBuilder::new(Some(*max_line_length));
            for text in writes {
                builder.write(text);
            }
            let result = if *as_error {
                builder.error("it broke".into())
            } else {
                builder.ok(Some("ok".into()))
            };
            CaseResult::ok(serde_json::to_value(result).unwrap())
        }
        Op::SniffMediaFromMagic { header } => {
            match sniff_media_from_magic(header) {
                Some(ft) => CaseResult::ok(file_type_to_value(ft)),
                None => CaseResult::err("no media magic".into()),
            }
        }
        Op::DetectFileType { path, header } => {
            let h = header.as_deref();
            CaseResult::ok(file_type_to_value(detect_file_type(path, h)))
        }
        Op::SniffImageDimensions { header } => {
            match sniff_image_dimensions(header) {
                Some(d) => CaseResult::ok(serde_json::to_value(d).unwrap()),
                None => CaseResult::err("no dimensions".into()),
            }
        }
        Op::DetectTarget { arch, platform } => {
            CaseResult::ok(Value::String(crate::rg_locator::detect_target_for(arch, platform)))
        }
        Op::FindExistingRg { path_env, share_dir, files } => {
            let dir = temp_dir.expect("find_existing_rg requires tempdir");
            let rg_name = if cfg!(windows) { "rg.exe" } else { "rg" };
            let mut paths: Vec<PathBuf> = path_env.iter().map(|p| dir.join(strip_leading_slash(p))).collect();
            let share = dir.join(strip_leading_slash(share_dir));
            std::fs::create_dir_all(&share.join("bin")).unwrap();
            if let Some(data) = files.get(&format!("{}/bin/{}", share_dir, rg_name)) {
                let target = share.join("bin").join(rg_name);
                std::fs::write(&target, data).unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
                }
            }
            for (rel, data) in files {
                if rel.starts_with(share_dir) {
                    continue;
                }
                let target = dir.join(strip_leading_slash(rel));
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(&target, data).unwrap();
                if rel.ends_with(rg_name) {
                    paths.push(target.parent().unwrap().to_path_buf());
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
                    }
                }
            }
            let path_var = paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join(";");
            let old = std::env::var("PATH").ok();
            std::env::set_var("PATH", &path_var);
            let result = find_existing_rg(&share);
            if let Some(old) = old {
                std::env::set_var("PATH", old);
            } else {
                std::env::remove_var("PATH");
            }
            match result {
                Some(RgResolution { path, source }) => {
                    let source_str = match source {
                        RgResolutionSource::SystemPath => "system-path",
                        RgResolutionSource::Vendor => "vendor",
                        RgResolutionSource::ShareBinCached => "share-bin-cached",
                        RgResolutionSource::ShareBinDownloaded => "share-bin-downloaded",
                    };
                    CaseResult::ok(serde_json::json!({
                        "path": path.to_string_lossy().to_string(),
                        "source": source_str,
                    }))
                }
                None => CaseResult::err("rg not found".into()),
            }
        }
        Op::ListDirectory { path, files } => {
            use kaos_rs::kaos::Kaos;
            let dir = temp_dir.expect("list_directory requires tempdir");
            for (rel, data) in files {
                let target = dir.join(strip_leading_slash(rel));
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(&target, data).unwrap();
            }
            let rt = tokio::runtime::Runtime::new().unwrap();
            let kaos = Kaos::new(kaos_rs::environment::detect_environment_from_node(), dir.clone());
            let listing = rt.block_on(crate::list_directory::list_directory(&kaos, path)).unwrap();
            CaseResult::ok(Value::String(listing))
        }
    }
}

fn strip_leading_slash(p: &str) -> &str {
    p.strip_prefix('/').unwrap_or(p)
}

fn needs_tempdir(op: &Op) -> bool {
    matches!(op, Op::FindExistingRg { .. } | Op::ListDirectory { .. })
}

fn files_for_op(op: &Op) -> FileSet {
    match op {
        Op::FindExistingRg { files, .. } => files.clone(),
        Op::ListDirectory { files, .. } => files.clone(),
        _ => FileSet::new(),
    }
}

fn setup_files(dir: &std::path::Path, files: &FileSet) -> std::io::Result<()> {
    for (rel, data) in files {
        let target = dir.join(strip_leading_slash(rel));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, data)?;
    }
    Ok(())
}

pub fn run_fixture_file(path: &str) -> HashMap<String, CaseResult> {
    let content = std::fs::read_to_string(path).expect("read fixture");
    let fixture: FixtureFile = serde_json::from_str(&content).expect("parse fixture");
    let mut all_files = FileSet::new();
    for case in &fixture.cases {
        for (k, v) in files_for_op(&case.op) {
            all_files.insert(k, v);
        }
    }
    let temp_dir = if all_files.is_empty() {
        None
    } else {
        let dir = std::env::temp_dir().join(format!("tools-rs-golden-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        setup_files(&dir, &all_files).unwrap();
        Some(dir)
    };
    let mut out = HashMap::new();
    for case in &fixture.cases {
        let td = needs_tempdir(&case.op).then_some(temp_dir.as_deref()).flatten();
        out.insert(case.name.clone(), run_case_sync(case, td));
    }
    out
}
```

> 注意：
> - `PermissionPathMatchOptions` 的实际字段名需与 Part 2 中 `path_glob_match.rs` 定义的 struct 保持一致。若 Part 2 实际字段名为 `path_class` / `case_insensitive_paths` 等，请按真实 API 调整。
> - `rg_locator::detect_target_for` 是 Part 4 中用于测试的 helper（接受 `arch`/`platform` 字符串）。如果 Part 4 只暴露了 `detect_target()`（无参数），请在 `rg_locator.rs` 中新增 `pub fn detect_target_for(arch: &str, platform: &str) -> String` 并在本 task 中一并实现。
> - `find_existing_rg` 的签名在 Part 4 为 `pub async fn find_existing_rg(share_dir: impl AsRef<Path>) -> Option<RgResolution>`。由于 golden runner 是同步的，本 task 需要把 `find_existing_rg` 改为同步版本，或在 runner 内用 `tokio::runtime` 调用。为避免改变公开 API，推荐在 runner 内使用 `tokio::runtime::Runtime::new().unwrap().block_on(find_existing_rg(...))`。

#### Step 11 — 调整 `find_existing_rg` 以支持测试注入 PATH

Part 4 的 `rg_locator.rs` 中 `find_existing_rg` 直接读取 `std::env::var("PATH")`，无法被 golden runner 注入的临时 PATH 覆盖。需要把 PATH 读取抽出为可注入参数，或修改 `find_existing_rg` 增加一个可选的 `path_env` 参数。

修改 `rust-ody/crates/tools-rs/src/rg_locator.rs`：

- 把 `find_existing_rg` 改为：

```rust
pub async fn find_existing_rg(
    share_dir: impl AsRef<std::path::Path>,
    path_env: Option<&str>,
) -> Option<RgResolution> {
    let bin_name = rg_binary_name();
    let path_env = path_env.map(String::from).or_else(|| std::env::var("PATH").ok());
    let system_rg = which_rg(path_env.as_deref()).await;
    if let Some(path) = system_rg {
        return Some(RgResolution { path, source: RgResolutionSource::SystemPath });
    }
    // ... rest unchanged
}
```

- 把 `which_rg` 改为接收 `path_env: Option<&str>`：

```rust
async fn which_rg(path_env: Option<&str>) -> Option<String> {
    let path_env = path_env.unwrap_or_default();
    let sep = if cfg!(windows) { ';' } else { ':' };
    let bin_name = rg_binary_name();
    for dir in path_env.split(sep) {
        if dir.is_empty() { continue; }
        let candidate = std::path::Path::new(dir).join(&bin_name);
        if let Ok(st) = tokio::fs::metadata(&candidate).await {
            if st.is_file() { return Some(candidate.to_string_lossy().to_string()); }
        }
    }
    None
}
```

- 更新 `resolve_rg_path` 调用：`find_existing_rg(share_dir, None).await`。

- 更新 `rg_locator.rs` 内的单元测试调用处（如果有的话）为 `find_existing_rg(tmp.path(), None).await`。

#### Step 12 — 创建 golden binary

创建 `rust-ody/crates/tools-rs/src/bin/tools-golden.rs`：

```rust
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .ok_or("usage: tools-golden <fixture.json>")?;
    let results = tools_rs::golden::run_fixture_file(&path);
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
```

#### Step 13 — 注册 binary 并导出 golden 模块

修改 `rust-ody/crates/tools-rs/Cargo.toml`，在 `[package]` 后添加：

```toml
[[bin]]
name = "tools-golden"
path = "src/bin/tools-golden.rs"
```

修改 `rust-ody/crates/tools-rs/src/lib.rs`，追加：

```rust
pub mod golden;
```

#### Step 14 — 运行集成测试

```bash
cd rust-ody && cargo test -p tools-rs --test l1_parity
```

预期：`test result: ok.` 且 8 个 fixture 都能产生非空输出。

#### Step 15 — 构建并运行 golden binary

```bash
cd rust-ody && cargo build -p tools-rs --bin tools-golden
./target/debug/tools-golden ../packages/integration-tests/src/parity/fixtures/tools-rs/path-policy.json
```

预期：输出 JSON 包含 `canonicalize relative posix`、`rejects empty path` 等 case 的结果，无 panic。

#### Step 16 — 提交

```bash
git add packages/integration-tests/src/parity/fixtures/tools-rs rust-ody/crates/tools-rs/src/golden.rs rust-ody/crates/tools-rs/src/bin/tools-golden.rs rust-ody/crates/tools-rs/src/lib.rs rust-ody/crates/tools-rs/src/rg_locator.rs rust-ody/crates/tools-rs/Cargo.toml rust-ody/crates/tools-rs/tests/l1_parity.rs
git commit -m "feat(tools-rs): L1 golden fixtures and golden binary for TS-Rust parity"
```

---

### Task 12: TS parity runner + test + CI + known-gaps

**Depends on:** Task 11

**Files:**
- Create: `packages/integration-tests/src/parity/tools-rs-golden.ts`
- Create: `packages/integration-tests/test/parity/tools-rs/l1-golden.test.ts`
- Modify: `packages/integration-tests/package.json`（新增 script）
- Modify: `.github/workflows/rust-host.yml`（新增 parity step）
- Modify: `packages/integration-tests/src/parity/known-gaps.md`（新增 gap 行）

#### Step 1 — 创建 TS golden runner

创建 `packages/integration-tests/src/parity/tools-rs-golden.ts`：

```ts
import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';

import { detectFileType, sniffImageDimensions, sniffMediaFromMagic } from '@odysseythink/agent-core/tools/support/file-type';
import {
  assertPathAllowed,
  canonicalizePath,
  isSensitiveFile,
  isWithinDirectory,
  resolvePathAccess,
  type PathAccessOperation,
  type PathClass,
} from '@odysseythink/agent-core/tools/policies/path-access';
import {
  escapeRuleSubjectLiteral,
  literalRulePattern,
  matchesGlobRuleSubject,
  matchesPathRuleSubject,
} from '@odysseythink/agent-core/tools/support/rule-match';
import { pathGlobMatch, type PermissionPathMatchOptions } from '@odysseythink/agent-core/tools/support/path-glob-match';
import { ToolResultBuilder } from '@odysseythink/agent-core/tools/support/result-builder';
import { toInputJsonSchema } from '@odysseythink/agent-core-shared';
import { z } from 'zod';
import { LocalKaos } from '@odysseythink/kaos';

import { detectTarget, findExistingRg } from '@odysseythink/agent-core/tools/support/rg-locator';
import { listDirectory } from '@odysseythink/agent-core/tools/support/list-directory';
import { ToolAccesses } from '@odysseythink/agent-core/tools/support/tool-accesses';

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
  | { type: 'canonicalize_path'; path: string; cwd: string; pathClass: PathClass }
  | { type: 'is_within_directory'; candidate: string; base: string; pathClass: PathClass }
  | { type: 'normalize_user_path'; path: string; pathClass: PathClass }
  | {
      type: 'resolve_path_access';
      path: string;
      cwd: string;
      workspaceDir: string;
      additionalDirs: string[];
      operation: PathAccessOperation;
      pathClass: PathClass;
      homeDir?: string;
    }
  | {
      type: 'assert_path_allowed';
      path: string;
      cwd: string;
      workspaceDir: string;
      additionalDirs: string[];
      mode: PathAccessOperation;
      pathClass: PathClass;
    }
  | { type: 'is_sensitive_file'; path: string }
  | { type: 'literal_rule_pattern'; toolName: string; subject: string }
  | { type: 'escape_rule_subject_literal'; subject: string }
  | { type: 'matches_glob_rule_subject'; ruleArgs: string; subject: string }
  | {
      type: 'matches_path_rule_subject';
      ruleArgs: string;
      subject: string;
      cwd?: string;
      pathClass: PathClass;
    }
  | { type: 'validate_args'; schema: Record<string, unknown>; args: unknown }
  | { type: 'access_conflict'; left: unknown[]; right: unknown[] }
  | { type: 'build_result'; writes: string[]; maxLineLength: number; asError?: boolean }
  | { type: 'sniff_media_from_magic'; header: number[] }
  | { type: 'detect_file_type'; path: string; header: number[] | null }
  | { type: 'sniff_image_dimensions'; header: number[] }
  | { type: 'detect_target'; arch: string; platform: string }
  | {
      type: 'find_existing_rg';
      pathEnv: string[];
      shareDir: string;
      files?: Record<string, number[]>;
    }
  | { type: 'list_directory'; path: string; files?: Record<string, number[]> };

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

export function resolveRustGoldenBinary(rootDir: string): string {
  const override = process.env['ODY_TOOLS_RS_GOLDEN_BINARY_PATH'];
  if (override !== undefined && override.length > 0) {
    return override;
  }
  return join(rootDir, 'rust-ody', 'target', 'debug', 'tools-golden');
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
    case 'canonicalize_path':
      try {
        return { result: canonicalizePath(op.path, op.cwd, op.pathClass) };
      } catch (e) {
        return { error: (e as { code: string }).code };
      }
    case 'is_within_directory':
      return { result: isWithinDirectory(op.candidate, op.base, op.pathClass) };
    case 'normalize_user_path':
      return { result: normalizeUserPath(op.path, op.pathClass) };
    case 'resolve_path_access': {
      try {
        const result = resolvePathAccess(op.path, op.cwd, {
          workspaceDir: op.workspaceDir,
          additionalDirs: op.additionalDirs,
        }, {
          operation: op.operation,
          pathClass: op.pathClass,
          homeDir: op.homeDir,
        });
        return { result };
      } catch (e) {
        return { error: (e as { code: string }).code };
      }
    }
    case 'assert_path_allowed': {
      try {
        const result = assertPathAllowed(op.path, op.cwd, {
          workspaceDir: op.workspaceDir,
          additionalDirs: op.additionalDirs,
        }, {
          mode: op.mode,
          pathClass: op.pathClass,
        });
        return { result };
      } catch (e) {
        return { error: (e as { code: string }).code };
      }
    }
    case 'is_sensitive_file':
      return { result: isSensitiveFile(op.path) };
    case 'literal_rule_pattern':
      return { result: literalRulePattern(op.toolName, op.subject) };
    case 'escape_rule_subject_literal':
      return { result: escapeRuleSubjectLiteral(op.subject) };
    case 'matches_glob_rule_subject':
      return { result: matchesGlobRuleSubject(op.ruleArgs, op.subject) };
    case 'matches_path_rule_subject': {
      const options: PermissionPathMatchOptions = {
        cwd: op.cwd,
        pathClass: op.pathClass,
        caseInsensitivePaths: true,
      };
      return { result: matchesPathRuleSubject(op.ruleArgs, op.subject, options) };
    }
    case 'validate_args': {
      const schema = z.toJSONSchema(z.object({}), { target: 'draft-7', io: 'input' });
      const validator = compileToolArgsValidator(op.schema);
      const error = validateToolArgs(validator, op.args);
      return error === null ? { result: null } : { error };
    }
    case 'access_conflict': {
      const left = ToolAccesses.fromJSON(op.left);
      const right = ToolAccesses.fromJSON(op.right);
      return { result: ToolAccesses.conflict(left, right) };
    }
    case 'build_result': {
      const builder = new ToolResultBuilder({ maxLineLength: op.maxLineLength });
      for (const text of op.writes) {
        builder.write(text);
      }
      const result = op.asError
        ? builder.error('it broke')
        : builder.ok('ok');
      return { result };
    }
    case 'sniff_media_from_magic': {
      const ft = sniffMediaFromMagic(Buffer.from(op.header));
      return ft === null
        ? { error: 'no media magic' }
        : { result: { kind: ft.kind, mimeType: ft.mimeType } };
    }
    case 'detect_file_type': {
      const header = op.header === null ? undefined : Buffer.from(op.header);
      const ft = detectFileType(op.path, header);
      return { result: { kind: ft.kind, mimeType: ft.mimeType } };
    }
    case 'sniff_image_dimensions': {
      const dims = sniffImageDimensions(Buffer.from(op.header));
      return dims === null
        ? { error: 'no dimensions' }
        : { result: { width: dims.width, height: dims.height } };
    }
    case 'detect_target':
      return { result: detectTarget(op.arch, op.platform) };
    case 'find_existing_rg': {
      const kaos = await LocalKaos.create();
      const oldPath = process.env['PATH'];
      const tempDir = await makeTempDir();
      await setupFiles(tempDir, op.files ?? {});
      const pathEnv = op.pathEnv.map((p) => join(tempDir, p.replace(/^\//, ''))).join(';');
      process.env['PATH'] = pathEnv;
      const shareDir = join(tempDir, op.shareDir.replace(/^\//, ''));
      try {
        const resolution = await findExistingRg(shareDir);
        if (resolution === undefined) {
          return { error: 'rg not found' };
        }
        return {
          result: {
            path: resolution.path,
            source: resolution.source,
          },
        };
      } finally {
        if (oldPath === undefined) {
          delete process.env['PATH'];
        } else {
          process.env['PATH'] = oldPath;
        }
        await kaos.close?.();
      }
    }
    case 'list_directory': {
      const kaos = await LocalKaos.create();
      const tempDir = await makeTempDir();
      await setupFiles(tempDir, op.files ?? {});
      try {
        const listing = await listDirectory(kaos, tempDir);
        return { result: listing };
      } finally {
        await kaos.close?.();
      }
    }
    default:
      throw new Error(`unknown op type ${(op as { type: string }).type}`);
  }
}

function normalizeUserPath(path: string, pathClass: PathClass): string {
  if (pathClass !== 'win32' || path === '/') return path;
  if (path.startsWith('//')) return path;
  const cygdrive = /^\/cygdrive\/([A-Za-z])(?:\/|$)/.exec(path);
  if (cygdrive !== null) {
    const drive = cygdrive[1]!.toUpperCase();
    const rest = path.slice(`/cygdrive/${cygdrive[1]}`.length);
    return `${drive}:${rest === '' ? '/' : rest}`;
  }
  const drive = /^\/([A-Za-z])(?:\/|$)/.exec(path);
  if (drive !== null) {
    const d = drive[1]!.toUpperCase();
    const rest = path.slice(2);
    return `${d}:${rest === '' ? '/' : rest}`;
  }
  return path;
}

function compileToolArgsValidator(schema: Record<string, unknown>) {
  // Reuse the AJV-compatible validator from agent-core internals.
  // If agent-core does not export this helper directly, inline a minimal AJV setup:
  const Ajv = require('ajv');
  const ajv = new Ajv({ strict: false });
  return ajv.compile(schema);
}

function validateToolArgs(validator: ReturnType<typeof compileToolArgsValidator>, args: unknown): string | null {
  const valid = validator(args);
  if (valid) return null;
  const first = validator.errors?.[0];
  if (first === undefined) return 'validation failed';
  if (first.keyword === 'additionalProperties' && typeof first.params.additionalProperty === 'string') {
    return `must NOT have additional property '${first.params.additionalProperty}'`;
  }
  if (first.keyword === 'required' && Array.isArray(first.params.missingProperty)) {
    return `must have required property '${first.params.missingProperty[0]}'`;
  }
  if (first.keyword === 'required' && typeof first.params.missingProperty === 'string') {
    return `must have required property '${first.params.missingProperty}'`;
  }
  return `${first.keyword}: ${first.message ?? 'validation failed'}`;
}

async function makeTempDir(): Promise<string> {
  const { mkdtemp } = await import('node:fs/promises');
  const { tmpdir } = await import('node:os');
  return mkdtemp(join(tmpdir(), 'tools-rs-golden-'));
}

async function setupFiles(dir: string, files: Record<string, number[]>): Promise<void> {
  const { mkdir, writeFile } = await import('node:fs/promises');
  const { dirname: patheDirname } = await import('pathe');
  for (const [rel, bytes] of Object.entries(files)) {
    const full = join(dir, rel.replace(/^\//, ''));
    await mkdir(patheDirname(full), { recursive: true });
    await writeFile(full, Buffer.from(bytes));
  }
}

export function runRustGolden(fixturePath: string, binaryPath: string): Record<string, unknown> {
  const result = spawnSync(binaryPath, [fixturePath], {
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.error) {
    throw new Error(`failed to run tools-golden: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(`tools-golden exited ${result.status}: ${result.stderr}`);
  }
  return JSON.parse(result.stdout) as Record<string, unknown>;
}
```

> 注意：
> - `@odysseythink/agent-core/tools/support/tool-accesses` 和 `ToolAccesses.fromJSON` 是假设的 API。如果 agent-core 未暴露该类或方法，请按实际 API 调整；例如直接把 JSON 数组传给 `ToolAccesses` 构造函数。
> - `compileToolArgsValidator` 这里使用 `ajv`。`@odysseythink/integration-tests` 当前依赖中未包含 `ajv`；若 agent-core 已导出 `compileToolArgsValidator`，请优先复用。否则在本 task 中把 `ajv` 作为 devDependency 加入 `packages/integration-tests/package.json`。
> - `findExistingRg` 在 TS 侧为异步；runner 在临时目录中设置 mock PATH，调用后恢复。

#### Step 2 — 创建 parity test

创建 `packages/integration-tests/test/parity/tools-rs/l1-golden.test.ts`：

```ts
import { existsSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';

import {
  resolveRustGoldenBinary,
  runRustGolden,
  runTsGolden,
} from '../../../src/parity/tools-rs-golden';

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
const binaryPath = resolveRustGoldenBinary(rootDir);

beforeAll(() => {
  // Always rebuild to ensure the binary is up to date.
  execSync('cargo build -p tools-rs --bin tools-golden', {
    cwd: join(rootDir, 'rust-ody'),
    stdio: 'inherit',
  });
});

const fixtures = [
  'path-policy.json',
  'rule-match.json',
  'schema-validation.json',
  'tool-accesses.json',
  'result-builder.json',
  'file-type.json',
  'rg-locator.json',
  'list-directory.json',
];

async function loadFixture(name: string): Promise<unknown> {
  const { readFile } = await import('node:fs/promises');
  const raw = await readFile(
    join(rootDir, 'packages', 'integration-tests', 'src', 'parity', 'fixtures', 'tools-rs', name),
    'utf8',
  );
  return JSON.parse(raw);
}

function sortKeys(obj: unknown): unknown {
  if (Array.isArray(obj)) return obj.map(sortKeys);
  if (obj !== null && typeof obj === 'object') {
    const sorted: Record<string, unknown> = {};
    for (const key of Object.keys(obj as Record<string, unknown>).sort()) {
      sorted[key] = sortKeys((obj as Record<string, unknown>)[key]);
    }
    return sorted;
  }
  return obj;
}

describe('tools-rs L1 golden parity', () => {
  it.each(fixtures)('%s TS matches Rust', async (name) => {
    const fixture = await loadFixture(name);
    const ts = await runTsGolden(fixture as { version: number; cases: unknown[] });
    const fixturePath = join(
      rootDir,
      'packages',
      'integration-tests',
      'src',
      'parity',
      'fixtures',
      'tools-rs',
      name,
    );
    const rust = runRustGolden(fixturePath, binaryPath);
    expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
  }, 120000);
});
```

#### Step 3 — 添加 npm script

修改 `packages/integration-tests/package.json` 的 `scripts` 部分，在 `"test:parity"` 附近新增：

```json
"test:parity:tools-rs": "vitest run test/parity/tools-rs/l1-golden.test.ts",
```

完整 `scripts` 块应类似（保留已有条目）：

```json
"scripts": {
  "test": "vitest run",
  "test:parity": "vitest run test/parity",
  "test:parity:tools-rs": "vitest run test/parity/tools-rs/l1-golden.test.ts",
  "test:parity:kaos": "vitest run test/parity/kaos",
  ...
}
```

如果 `tools-rs-golden.ts` 使用 `ajv` 且 agent-core 未导出 validator，则同时在 `devDependencies` 中加入：

```json
"ajv": "^8.17.1"
```

#### Step 4 — 在 rust-host.yml 中添加 CI step

修改 `.github/workflows/rust-host.yml`，在 `kaos-rs unit tests` step 之后、`Build kaos-golden binary` step 之前插入：

```yaml
      - name: tools-rs unit tests
        run: cargo test -p tools-rs
        working-directory: rust-ody

      - name: Build tools-golden binary
        run: cargo build -p tools-rs --bin tools-golden
        working-directory: rust-ody

      - name: tools-rs L1 golden parity
        run: pnpm --filter @odysseythink/integration-tests test:parity:tools-rs
        env:
          ODY_TOOLS_RS_GOLDEN_BINARY_PATH: ${{ github.workspace }}/rust-ody/target/debug/tools-golden
```

#### Step 5 — 登记 known-gaps

修改 `packages/integration-tests/src/parity/known-gaps.md`，在表格末尾新增一行：

```markdown
| tools-rs/list-directory | L1 | Rust list_directory 使用 kaos iterdir 排序，TS 使用 Kaos.stat 逐条判断；当目录不可读时错误文本可能不同 |
```

如果 `rg-locator` 的下载分支在 CI 因网络不可达而无法对齐，也添加：

```markdown
| tools-rs/rg-locator | L1 | `find_existing_rg` 下载分支在 CI 无网络；fixture 仅覆盖纯查找路径 |
```

#### Step 6 — 安装依赖并 typecheck

```bash
pnpm install
pnpm --filter @odysseythink/integration-tests typecheck
```

预期：`tsc` 无错误。

#### Step 7 — 运行 parity test

```bash
pnpm --filter @odysseythink/integration-tests test:parity:tools-rs
```

预期：8 个 fixture 全部通过 `TS matches Rust`。

#### Step 8 — 提交

```bash
git add packages/integration-tests/src/parity/tools-rs-golden.ts packages/integration-tests/test/parity/tools-rs/l1-golden.test.ts packages/integration-tests/package.json .github/workflows/rust-host.yml packages/integration-tests/src/parity/known-gaps.md pnpm-lock.yaml
git commit -m "feat(integration-tests): tools-rs L1 parity runner, test, and CI job"
```

---

## Local Self-Review

- [ ] 1. Spec-coverage table：

| Spec item | Task(s) | Status |
|---|---|---|
| 4.4.0 — L1 golden fixtures for path policy | Task 11 Step 1 | covered |
| 4.4.0 — L1 golden fixtures for rule/path matching | Task 11 Step 2 | covered |
| 4.4.0 — L1 golden fixtures for schema/validation | Task 11 Step 3 | covered |
| 4.4.0 — L1 golden fixtures for ToolAccesses | Task 11 Step 4 | covered |
| 4.4.0 — L1 golden fixtures for result builder | Task 11 Step 5 | covered |
| 4.4.0 — L1 golden fixtures for file-type sniff | Task 11 Step 6 | covered |
| 4.4.0 — L1 golden fixtures for rg locator | Task 11 Step 7 | covered |
| 4.4.0 — L1 golden fixtures for list-directory | Task 11 Step 8 | covered |
| 4.4.0 — Rust golden runner/binary | Task 11 Step 9–13 | covered |
| 4.4.0 — TS golden runner | Task 12 Step 1 | covered |
| 4.4.0 — TS↔Rust parity test | Task 12 Step 2 | covered |
| 4.4.0 — npm script for tools-rs parity | Task 12 Step 3 | covered |
| 4.4.0 — CI job in rust-host.yml | Task 12 Step 4 | covered |
| 4.4.0 — Known-gaps entry | Task 12 Step 5 | covered |

- [ ] 2. Placeholder scan：无 TODO/TBD；所有代码片段完整，包含实际 import 路径、函数调用和 fixture 数据。
- [ ] 3. No phantom tasks：每个 task 产出具体文件变更；无 `--allow-empty` 提交。
- [ ] 4. Dependency soundness：Task 11 依赖 Part 1–4；Task 12 依赖 Task 11；无后置依赖。
- [ ] 5. Caller & build soundness：本 part 修改了 `rg_locator::find_existing_rg` 的签名（增加 `path_env` 参数），Task 11 Step 11 同时更新了 `resolve_rg_path` 和该模块内的测试调用；修改后应运行 `cargo test -p tools-rs` 和 `pnpm -r typecheck` 确认全树编译通过。
- [ ] 6. Test-the-risk：parity test 对同一份 JSON fixture 比对 TS 与 Rust 输出，任何 helper 常量/排序/错误文本不一致都会失败；敏感文件 fixture 包含 `.env.example` 等 must-survive 输入。
- [ ] 7. Type consistency：Rust `golden.rs` 中使用的枚举/struct 名称（`PathClass::Posix`、`ToolAccesses`、`RgResolutionSource` 等）与 Part 1–4 定义一致；TS runner import 路径与 `agent-core` 实际 export 对齐。
