# 4.2.1 Part 2 — L1 golden 测试工具与 fixtures

**Goal:** 在 Rust `kosong-rs` 中新增 `kosong-utils-golden` 二进制，并在 `packages/integration-tests` 中补充 fixtures + TS harness + 测试，使 4.2.1 实现的纯函数模块与 TS 实现保持逐 case 行为一致。

**Architecture:** 延续已有的 generate-loop golden 模式：fixture 只含输入，TS harness 与 Rust binary 各自计算输出，测试文件逐 case 严格比较。utils golden 专注于纯函数，不依赖网络或 mock provider，因此单独一个 binary，避免污染现有的 `kosong-golden`。

**Tech Stack:** Rust (`kosong-rs`, `serde_json`, `anyhow`), TypeScript/Vitest (`packages/integration-tests`).

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

```
rust-ody/crates/kosong-rs/
├── Cargo.toml                          # 追加 [[bin]] kosong-utils-golden
└── src/bin/utils_golden.rs             # 新增：纯函数 golden runner

packages/integration-tests/
├── src/parity/
│   ├── kosong-utils-golden.ts          # 新增：TS 侧 utils golden harness
│   └── fixtures/kosong-utils/
│       ├── tool-call-id.json
│       ├── request-auth.json
│       ├── capability-registry.json
│       └── catalog.json
└── test/parity/kosong/
    └── l1-utils-golden.test.ts         # 新增：TS vs Rust 逐 fixture 比较
```

## Dependency Overview

```
Task 1 (Rust binary)
    │
    ▼
Task 2 (Fixtures) ──▶ Task 3 (TS harness) ──▶ Task 4 (L1 test)
```

- Task 1 仅依赖 `core.md` 已完成的模块（`tool_call_id`、`request_auth`、`capability_registry`、`catalog`、`ProviderType`）。
- Task 2 的 fixture 输入/输出形状必须与 Task 1 的 binary 和 Task 3 的 TS harness 完全一致。
- Task 3 导入 `@odysseythink/kosong` 及其 `providers/*` 子路径。
- Task 4 调用 cargo build 与 vitest，验证整段链路。

## Risks & Open Questions

- **序列化形状漂移：** Rust `ModelCapability` 与 `CatalogModel` 的 serde 字段名（camelCase）必须与 TS 类型完全一致； fixture 中 `tool_call` 字段名与两边接口一致。
- **错误表示差异：** TS `requireProviderApiKey` 抛 `ChatProviderError`，Rust 返回 `Result<_, ChatProviderError::MissingApiKey>`；golden 输出统一为 `{ error: string | null }`。
- **Closure 不可序列化：** `normalizeToolCallIdsForProvider` 的 policy 不能放进 fixture，因此 fixture 传 `provider` 字符串，binary/harness 各自按 provider 构造 policy（maxLength=64，openai_responses 用 `sanitizeOpenAIResponsesCallId`）。

---

### Task 6: 新增 `kosong-utils-golden` Rust binary

**Depends on:** `core.md` 全部任务（Rust 模块已实现并 export）

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/bin/utils_golden.rs`
- Modify: `rust-ody/crates/kosong-rs/Cargo.toml:23`（在 `kosong-golden` 下方追加 `[[bin]]`）

**步骤：**

- [ ] 创建 `src/bin/utils_golden.rs`（完整代码）：
  ```rust
  use std::collections::HashMap;
  use std::env;
  use std::fs;

  use anyhow::{bail, Context, Result};
  use kosong_rs::catalog::{CatalogModelEntry, CatalogProviderEntry};
  use kosong_rs::provider::{ProviderRequestAuth, ProviderType};
  use kosong_rs::tool_call_id::{
      normalize_tool_call_ids_for_provider, sanitize_openai_responses_call_id, sanitize_tool_call_id,
      ToolCallIdPolicy,
  };
  use kosong_rs::{capability_registry, catalog, request_auth};
  use serde::{Deserialize, Serialize};
  use serde_json::Value;

  const TOOL_CALL_ID_MAX_LENGTH: usize = 64;

  #[derive(Debug, Deserialize)]
  struct Fixture {
      operations: Vec<Operation>,
  }

  #[derive(Debug, Deserialize)]
  struct Operation {
      operation: String,
      cases: Vec<Case>,
  }

  #[derive(Debug, Deserialize)]
  struct Case {
      name: String,
      input: Value,
  }

  #[derive(Debug, Serialize)]
  struct GoldenResult {
      name: String,
      #[serde(skip_serializing_if = "Option::is_none")]
      output: Option<Value>,
      #[serde(skip_serializing_if = "Option::is_none")]
      error: Option<String>,
  }

  #[derive(Debug, Serialize)]
  struct GoldenOperation {
      operation: String,
      results: Vec<GoldenResult>,
  }

  #[derive(Debug, Serialize)]
  struct GoldenOutput {
      operations: Vec<GoldenOperation>,
  }

  fn main() -> Result<()> {
      let path = env::args().nth(1).context("fixture path required")?;
      let input = fs::read_to_string(&path)?;
      let fixture: Fixture = serde_json::from_str(&input)?;

      let mut operations = Vec::new();
      let mut all_ok = true;
      for op in fixture.operations {
          let mut results = Vec::new();
          for case in op.cases {
              match run_case(&op.operation, &case.input) {
                  Ok(output) => results.push(GoldenResult {
                      name: case.name,
                      output: Some(output),
                      error: None,
                  }),
                  Err(e) => {
                      all_ok = false;
                      results.push(GoldenResult {
                          name: case.name,
                          output: None,
                          error: Some(format!("{}", e)),
                      });
                  }
              }
          }
          operations.push(GoldenOperation {
              operation: op.operation,
              results,
          });
      }

      println!("{}", serde_json::to_string_pretty(&GoldenOutput { operations })?);
      if all_ok {
          Ok(())
      } else {
          bail!("one or more cases failed");
      }
  }

  fn run_case(operation: &str, input: &Value) -> Result<Value> {
      match operation {
          "sanitizeToolCallId" => {
              let id = input["id"].as_str().context("id must be a string")?;
              let max_length = input.get("maxLength").and_then(|v| v.as_u64()).map(|v| v as usize);
              Ok(Value::String(sanitize_tool_call_id(id, max_length)))
          }
          "sanitizeOpenAIResponsesCallId" => {
              let id = input["id"].as_str().context("id must be a string")?;
              let max_length = input.get("maxLength").and_then(|v| v.as_u64()).map(|v| v as usize);
              Ok(Value::String(sanitize_openai_responses_call_id(id, max_length)))
          }
          "normalizeToolCallIdsForProvider" => {
              let messages: Vec<kosong_rs::message::Message> =
                  serde_json::from_value(input["messages"].clone())
                      .context("messages must be an array of Message")?;
              let provider: ProviderType = serde_json::from_value(input["provider"].clone())
                  .context("provider must be a ProviderType string")?;
              let policy = tool_call_id_policy_for_provider(provider);
              let normalized = normalize_tool_call_ids_for_provider(&messages, &policy);
              Ok(serde_json::to_value(&normalized)?)
          }
          "requireProviderApiKey" => {
              let provider_name =
                  input["providerName"].as_str().context("providerName must be a string")?;
              let auth: Option<ProviderRequestAuth> = input
                  .get("auth")
                  .and_then(|v| serde_json::from_value(v.clone()).ok());
              let default_api_key = input.get("defaultApiKey").and_then(|v| v.as_str());
              let key = request_auth::require_provider_api_key(
                  provider_name,
                  auth.as_ref(),
                  default_api_key,
              )?;
              Ok(Value::String(key))
          }
          "mergeRequestHeaders" => {
              let default_headers: Option<HashMap<String, String>> = input
                  .get("defaultHeaders")
                  .and_then(|v| serde_json::from_value(v.clone()).ok());
              let request_headers: Option<HashMap<String, String>> = input
                  .get("requestHeaders")
                  .and_then(|v| serde_json::from_value(v.clone()).ok());
              let merged = request_auth::merge_request_headers(
                  default_headers.as_ref(),
                  request_headers.as_ref(),
              );
              Ok(serde_json::to_value(&merged)?)
          }
          "getOpenAILegacyModelCapability" => {
              let model_name = input["modelName"].as_str().context("modelName must be a string")?;
              let cap = capability_registry::get_openai_legacy_model_capability(model_name);
              Ok(serde_json::to_value(&cap)?)
          }
          "getOpenAIResponsesModelCapability" => {
              let model_name = input["modelName"].as_str().context("modelName must be a string")?;
              let cap = capability_registry::get_openai_responses_model_capability(model_name);
              Ok(serde_json::to_value(&cap)?)
          }
          "getAnthropicModelCapability" => {
              let model_name = input["modelName"].as_str().context("modelName must be a string")?;
              let cap = capability_registry::get_anthropic_model_capability(model_name);
              Ok(serde_json::to_value(&cap)?)
          }
          "getGoogleGenAIModelCapability" => {
              let model_name = input["modelName"].as_str().context("modelName must be a string")?;
              let cap = capability_registry::get_google_genai_model_capability(model_name);
              Ok(serde_json::to_value(&cap)?)
          }
          "usesOpenAIResponsesDeveloperRole" => {
              let model_name = input["modelName"].as_str().context("modelName must be a string")?;
              Ok(Value::Bool(capability_registry::uses_openai_responses_developer_role(
                  model_name,
              )))
          }
          "inferWireType" => {
              let entry: CatalogProviderEntry = serde_json::from_value(input["entry"].clone())
                  .context("entry must be a CatalogProviderEntry")?;
              let wire = catalog::infer_wire_type(&entry);
              Ok(serde_json::to_value(&wire)?)
          }
          "catalogBaseUrl" => {
              let entry: CatalogProviderEntry = serde_json::from_value(input["entry"].clone())
                  .context("entry must be a CatalogProviderEntry")?;
              let wire: ProviderType = serde_json::from_value(input["wire"].clone())
                  .context("wire must be a ProviderType string")?;
              let url = catalog::catalog_base_url(&entry, wire);
              Ok(serde_json::to_value(&url)?)
          }
          "catalogModelToCapability" => {
              let model: CatalogModelEntry = serde_json::from_value(input["model"].clone())
                  .context("model must be a CatalogModelEntry")?;
              let result = catalog::catalog_model_to_capability(&model);
              Ok(serde_json::to_value(&result)?)
          }
          "catalogProviderModels" => {
              let entry: CatalogProviderEntry = serde_json::from_value(input["entry"].clone())
                  .context("entry must be a CatalogProviderEntry")?;
              let models = catalog::catalog_provider_models(&entry);
              Ok(serde_json::to_value(&models)?)
          }
          _ => bail!("unknown operation: {}", operation),
      }
  }

  fn tool_call_id_policy_for_provider(provider: ProviderType) -> ToolCallIdPolicy {
      match provider {
          ProviderType::OpenAiResponses => ToolCallIdPolicy::new(
              |id| sanitize_openai_responses_call_id(id, Some(TOOL_CALL_ID_MAX_LENGTH)),
              Some(TOOL_CALL_ID_MAX_LENGTH),
          ),
          _ => ToolCallIdPolicy::new(
              |id| sanitize_tool_call_id(id, Some(TOOL_CALL_ID_MAX_LENGTH)),
              Some(TOOL_CALL_ID_MAX_LENGTH),
          ),
      }
  }
  ```

- [ ] 在 `Cargo.toml` 末尾追加：
  ```toml
  [[bin]]
  name = "kosong-utils-golden"
  path = "src/bin/utils_golden.rs"
  ```

- [ ] 编译确认：
  ```bash
  cargo build -p kosong-rs --bin kosong-utils-golden
  ```
  期望：`Finished dev [unoptimized + debuginfo] target(s)`，无 error。

- [ ] 用临时 fixture 手动验证：
  ```bash
  cat > /tmp/utils-smoke.json <<'EOF'
  {
    "operations": [
      {
        "operation": "sanitizeToolCallId",
        "cases": [
          { "name": "replaces unsafe chars", "input": { "id": "a|b/c", "maxLength": 64 } }
        ]
      }
    ]
  }
  EOF
  ./rust-ody/target/debug/kosong-utils-golden /tmp/utils-smoke.json
  ```
  期望输出包含 `"output": "a_b_c"`、`"error": null`，进程退出码 `0`。

- [ ] Commit：`feat(kosong-rs): add kosong-utils-golden binary for utility parity`

---

### Task 7: 创建 fixture JSON 文件

**Depends on:** Task 6（binary 已支持以下 operations 与输入形状）

**Files:**
- Create: `packages/integration-tests/src/parity/fixtures/kosong-utils/tool-call-id.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-utils/request-auth.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-utils/capability-registry.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-utils/catalog.json`

**步骤：**

- [ ] 创建 `tool-call-id.json`：
  ```json
  {
    "operations": [
      {
        "operation": "sanitizeToolCallId",
        "cases": [
          {
            "name": "safe chars pass through",
            "input": { "id": "call_123-abc", "maxLength": 64 }
          },
          {
            "name": "unsafe chars become underscores",
            "input": { "id": "a|b/c@d#e", "maxLength": 64 }
          },
          {
            "name": "truncates to maxLength",
            "input": { "id": "abcdefghij", "maxLength": 5 }
          },
          {
            "name": "empty id stays empty",
            "input": { "id": "", "maxLength": 64 }
          }
        ]
      },
      {
        "operation": "sanitizeOpenAIResponsesCallId",
        "cases": [
          {
            "name": "splits at pipe",
            "input": { "id": "call_123|extra", "maxLength": 64 }
          },
          {
            "name": "truncates after split",
            "input": { "id": "abcdef|gh", "maxLength": 4 }
          },
          {
            "name": "no pipe passes through",
            "input": { "id": "safe-id", "maxLength": 64 }
          }
        ]
      },
      {
        "operation": "normalizeToolCallIdsForProvider",
        "cases": [
          {
            "name": "openai normalizes matching ids",
            "input": {
              "provider": "openai",
              "messages": [
                {
                  "role": "assistant",
                  "content": [],
                  "toolCalls": [
                    { "type": "function", "id": "a|b", "name": "read", "arguments": null }
                  ]
                },
                {
                  "role": "tool",
                  "content": [{ "type": "text", "text": "ok" }],
                  "toolCalls": [],
                  "toolCallId": "a|b"
                }
              ]
            }
          },
          {
            "name": "openai_responses splits pipe",
            "input": {
              "provider": "openai_responses",
              "messages": [
                {
                  "role": "assistant",
                  "content": [],
                  "toolCalls": [
                    { "type": "function", "id": "call_123|extra", "name": "read", "arguments": null }
                  ]
                }
              ]
            }
          },
          {
            "name": "duplicate normalized ids get suffixes",
            "input": {
              "provider": "openai",
              "messages": [
                {
                  "role": "assistant",
                  "content": [],
                  "toolCalls": [
                    { "type": "function", "id": "a|b", "name": "read", "arguments": null }
                  ]
                },
                {
                  "role": "assistant",
                  "content": [],
                  "toolCalls": [
                    { "type": "function", "id": "a/b", "name": "write", "arguments": null }
                  ]
                }
              ]
            }
          },
          {
            "name": "no tool calls returns unchanged",
            "input": {
              "provider": "openai",
              "messages": [
                {
                  "role": "user",
                  "content": [{ "type": "text", "text": "hello" }],
                  "toolCalls": []
                }
              ]
            }
          }
        ]
      }
    ]
  }
  ```

- [ ] 创建 `request-auth.json`：
  ```json
  {
    "operations": [
      {
        "operation": "requireProviderApiKey",
        "cases": [
          {
            "name": "returns request apiKey",
            "input": { "providerName": "openai", "auth": { "apiKey": "sk-req" } }
          },
          {
            "name": "prefers request over default",
            "input": { "providerName": "openai", "auth": { "apiKey": "sk-req" }, "defaultApiKey": "sk-def" }
          },
          {
            "name": "falls back to default",
            "input": { "providerName": "openai", "defaultApiKey": "sk-def" }
          },
          {
            "name": "rejects missing key",
            "input": { "providerName": "openai" }
          }
        ]
      },
      {
        "operation": "mergeRequestHeaders",
        "cases": [
          {
            "name": "merges maps",
            "input": { "defaultHeaders": { "a": "1" }, "requestHeaders": { "b": "2" } }
          },
          {
            "name": "request overrides default",
            "input": { "defaultHeaders": { "a": "1" }, "requestHeaders": { "a": "2" } }
          },
          {
            "name": "empty maps return null",
            "input": { "defaultHeaders": {}, "requestHeaders": {} }
          },
          {
            "name": "single map works",
            "input": { "defaultHeaders": { "a": "1" } }
          }
        ]
      }
    ]
  }
  ```

- [ ] 创建 `capability-registry.json`：
  ```json
  {
    "operations": [
      {
        "operation": "getOpenAILegacyModelCapability",
        "cases": [
          { "name": "gpt-4o vision tool", "input": { "modelName": "gpt-4o-2024-05-13" } },
          { "name": "o3-mini reasoning", "input": { "modelName": "o3-mini" } },
          { "name": "unknown model", "input": { "modelName": "foo-bar" } }
        ]
      },
      {
        "operation": "getOpenAIResponsesModelCapability",
        "cases": [
          { "name": "gpt-4.1 vision", "input": { "modelName": "gpt-4.1" } },
          { "name": "o3-mini reasoning", "input": { "modelName": "o3-mini" } }
        ]
      },
      {
        "operation": "getAnthropicModelCapability",
        "cases": [
          { "name": "claude-3-5 vision", "input": { "modelName": "claude-3-5-sonnet-20241022" } },
          { "name": "claude-opus-4 thinking", "input": { "modelName": "claude-opus-4-20250514" } }
        ]
      },
      {
        "operation": "getGoogleGenAIModelCapability",
        "cases": [
          { "name": "gemini-2.0-flash multimodal", "input": { "modelName": "gemini-2.0-flash-exp" } },
          { "name": "gemini-2.5-pro thinking", "input": { "modelName": "gemini-2.5-pro-preview-05-06" } },
          { "name": "non-catalogued gemini", "input": { "modelName": "gemini-9.0-ultra" } }
        ]
      },
      {
        "operation": "usesOpenAIResponsesDeveloperRole",
        "cases": [
          { "name": "gpt-4.1 true", "input": { "modelName": "gpt-4.1" } },
          { "name": "o3-mini true", "input": { "modelName": "o3-mini" } },
          { "name": "gpt-4o false", "input": { "modelName": "gpt-4o" } }
        ]
      }
    ]
  }
  ```

- [ ] 创建 `catalog.json`：
  ```json
  {
    "operations": [
      {
        "operation": "inferWireType",
        "cases": [
          { "name": "explicit type", "input": { "entry": { "type": "deepseek" } } },
          { "name": "infer from id claude", "input": { "entry": { "id": "claude-3-5-sonnet" } } },
          { "name": "infer from npm google", "input": { "entry": { "npm": "@ai-sdk/google" } } },
          { "name": "vertex from id", "input": { "entry": { "id": "vertex-something" } } },
          { "name": "unknown", "input": { "entry": { "id": "foo" } } }
        ]
      },
      {
        "operation": "catalogBaseUrl",
        "cases": [
          {
            "name": "anthropic strips trailing v1",
            "input": { "entry": { "api": "https://api.anthropic.com/v1" }, "wire": "anthropic" }
          },
          {
            "name": "openai keeps v1",
            "input": { "entry": { "api": "https://api.openai.com/v1" }, "wire": "openai" }
          },
          {
            "name": "missing api returns null",
            "input": { "entry": {}, "wire": "openai" }
          }
        ]
      },
      {
        "operation": "catalogModelToCapability",
        "cases": [
          {
            "name": "embedding skipped",
            "input": {
              "model": {
                "id": "text-embedding-3",
                "family": "embedding",
                "limit": { "context": 8192, "output": 1536 },
                "modalities": { "input": ["text"], "output": ["text"] }
              }
            }
          },
          {
            "name": "valid model parsed",
            "input": {
              "model": {
                "id": "gpt-4o",
                "limit": { "context": 128000, "output": 16384 },
                "tool_call": true,
                "modalities": { "input": ["text", "image"], "output": ["text"] }
              }
            }
          }
        ]
      },
      {
        "operation": "catalogProviderModels",
        "cases": [
          {
            "name": "filters invalid returns valid",
            "input": {
              "entry": {
                "models": {
                  "gpt-4o": {
                    "id": "gpt-4o",
                    "limit": { "context": 128000, "output": 16384 },
                    "tool_call": true,
                    "modalities": { "input": ["text"], "output": ["text"] }
                  },
                  "bad": {
                    "id": "",
                    "limit": { "context": 0 }
                  }
                }
              }
            }
          }
        ]
      }
    ]
  }
  ```

- [ ] 用 binary 跑一遍所有 fixtures（此时 TS harness 还未写，只验证 Rust 侧不 panic 且退出码 0）：
  ```bash
  cargo build -p kosong-rs --bin kosong-utils-golden
  for f in packages/integration-tests/src/parity/fixtures/kosong-utils/*.json; do
    ./rust-ody/target/debug/kosong-utils-golden "$f" > /dev/null || echo "FAIL: $f"
  done
  ```
  期望：无 `FAIL` 输出。

- [ ] Commit：`test(integration): add kosong-utils L1 golden fixtures`

---

### Task 8: 新增 TS utils golden harness

**Depends on:** Task 7（fixtures 已存在，定义了 operation/case 输入形状）

**Files:**
- Create: `packages/integration-tests/src/parity/kosong-utils-golden.ts`

**步骤：**

- [ ] 创建 `kosong-utils-golden.ts`（完整代码）：
  ```ts
  import type {
    CatalogModelEntry,
    CatalogProviderEntry,
    Message,
    ProviderType,
  } from '@odysseythink/kosong';
  import {
    catalogBaseUrl,
    catalogModelToCapability,
    catalogProviderModels,
    inferWireType,
  } from '@odysseythink/kosong';
  import {
    getAnthropicModelCapability,
    getGoogleGenAIModelCapability,
    getOpenAILegacyModelCapability,
    getOpenAIResponsesModelCapability,
    usesOpenAIResponsesDeveloperRole,
  } from '@odysseythink/kosong/providers/capability-registry';
  import {
    mergeRequestHeaders,
    requireProviderApiKey,
  } from '@odysseythink/kosong/providers/request-auth';
  import {
    normalizeToolCallIdsForProvider,
    sanitizeOpenAIResponsesCallId,
    sanitizeToolCallId,
    type ToolCallIdPolicy,
  } from '@odysseythink/kosong/providers/tool-call-id';

  export interface Fixture {
    operations: Array<{
      operation: string;
      cases: Array<{ name: string; input: Record<string, unknown> }>;
    }>;
  }

  export interface GoldenResult {
    name: string;
    output?: unknown;
    error?: string;
  }

  export interface GoldenOperation {
    operation: string;
    results: GoldenResult[];
  }

  export interface GoldenOutput {
    operations: GoldenOperation[];
  }

  const TOOL_CALL_ID_MAX_LENGTH = 64;

  function toolCallIdPolicyForProvider(provider: ProviderType): ToolCallIdPolicy {
    if (provider === 'openai_responses') {
      return {
        normalize: (id: string) => sanitizeOpenAIResponsesCallId(id, TOOL_CALL_ID_MAX_LENGTH),
        maxLength: TOOL_CALL_ID_MAX_LENGTH,
      };
    }
    return {
      normalize: (id: string) => sanitizeToolCallId(id, TOOL_CALL_ID_MAX_LENGTH),
      maxLength: TOOL_CALL_ID_MAX_LENGTH,
    };
  }

  export async function runTsKosongUtilsGolden(fixturePath: string): Promise<GoldenOutput> {
    const { readFileSync } = await import('node:fs');
    const raw = readFileSync(fixturePath, 'utf8');
    const fixture: Fixture = JSON.parse(raw);

    const operations: GoldenOperation[] = [];
    for (const op of fixture.operations) {
      const results: GoldenResult[] = [];
      for (const c of op.cases) {
        try {
          const output = runCase(op.operation, c.input);
          results.push({ name: c.name, output });
        } catch (e) {
          results.push({ name: c.name, error: String(e) });
        }
      }
      operations.push({ operation: op.operation, results });
    }
    return { operations };
  }

  function runCase(operation: string, input: Record<string, unknown>): unknown {
    switch (operation) {
      case 'sanitizeToolCallId': {
        const id = String(input.id);
        const maxLength = input.maxLength === undefined ? undefined : Number(input.maxLength);
        return sanitizeToolCallId(id, maxLength);
      }
      case 'sanitizeOpenAIResponsesCallId': {
        const id = String(input.id);
        const maxLength = input.maxLength === undefined ? undefined : Number(input.maxLength);
        return sanitizeOpenAIResponsesCallId(id, maxLength);
      }
      case 'normalizeToolCallIdsForProvider': {
        const messages = input.messages as Message[];
        const provider = input.provider as ProviderType;
        return normalizeToolCallIdsForProvider(messages, toolCallIdPolicyForProvider(provider));
      }
      case 'requireProviderApiKey': {
        const providerName = String(input.providerName);
        const auth = input.auth as { apiKey?: string } | undefined;
        const defaultApiKey =
          input.defaultApiKey === undefined ? undefined : String(input.defaultApiKey);
        return requireProviderApiKey(providerName, auth, defaultApiKey);
      }
      case 'mergeRequestHeaders': {
        const defaultHeaders = input.defaultHeaders as Record<string, string> | undefined;
        const requestHeaders = input.requestHeaders as Record<string, string> | undefined;
        return mergeRequestHeaders(defaultHeaders, requestHeaders) ?? null;
      }
      case 'getOpenAILegacyModelCapability': {
        return getOpenAILegacyModelCapability(String(input.modelName));
      }
      case 'getOpenAIResponsesModelCapability': {
        return getOpenAIResponsesModelCapability(String(input.modelName));
      }
      case 'getAnthropicModelCapability': {
        return getAnthropicModelCapability(String(input.modelName));
      }
      case 'getGoogleGenAIModelCapability': {
        return getGoogleGenAIModelCapability(String(input.modelName));
      }
      case 'usesOpenAIResponsesDeveloperRole': {
        return usesOpenAIResponsesDeveloperRole(String(input.modelName));
      }
      case 'inferWireType': {
        return inferWireType(input.entry as CatalogProviderEntry) ?? null;
      }
      case 'catalogBaseUrl': {
        return catalogBaseUrl(input.entry as CatalogProviderEntry, input.wire as ProviderType) ?? null;
      }
      case 'catalogModelToCapability': {
        return catalogModelToCapability(input.model as CatalogModelEntry) ?? null;
      }
      case 'catalogProviderModels': {
        return catalogProviderModels(input.entry as CatalogProviderEntry);
      }
      default:
        throw new Error(`unknown operation: ${operation}`);
    }
  }
  ```

- [ ] 类型检查验证：
  ```bash
  pnpm -F @odysseythink/integration-tests typecheck
  ```
  期望：`pnpm` 退出码 `0`，无 TS error。

- [ ] 用临时脚本手动验证 harness 可正确读取 fixture：
  ```bash
  node --input-type=module <<'EOF'
  import { runTsKosongUtilsGolden } from './packages/integration-tests/src/parity/kosong-utils-golden.ts';
  const out = await runTsKosongUtilsGolden('./packages/integration-tests/src/parity/fixtures/kosong-utils/tool-call-id.json');
  console.log(JSON.stringify(out.operations[0].results[0], null, 2));
  EOF
  ```
  期望输出包含 `"output": "call_123-abc"`（第一个 case）。

- [ ] Commit：`feat(integration): add TS harness for kosong-utils golden parity`

---

### Task 9: 新增 `l1-utils-golden.test.ts`

**Depends on:** Task 6（binary）、Task 7（fixtures）、Task 8（TS harness）

**Files:**
- Create: `packages/integration-tests/test/parity/kosong/l1-utils-golden.test.ts`

**步骤：**

- [ ] 创建测试文件（完整代码）：
  ```ts
  import { existsSync, readFileSync } from 'node:fs';
  import { spawnSync } from 'node:child_process';
  import { beforeAll, describe, expect, it } from 'vitest';
  import { dirname, join } from 'pathe';
  import { fileURLToPath } from 'node:url';
  import { runTsKosongUtilsGolden } from '../../../src/parity/kosong-utils-golden';

  function findProjectRoot(): string {
    let current = dirname(fileURLToPath(import.meta.url));
    while (current !== dirname(current)) {
      if (existsSync(join(current, '.git'))) return current;
      current = dirname(current);
    }
    return process.cwd();
  }

  const rootDir = findProjectRoot();
  const fixturesDir = join(
    rootDir,
    'packages',
    'integration-tests',
    'src',
    'parity',
    'fixtures',
    'kosong-utils',
  );
  const fixtures = [
    'tool-call-id.json',
    'request-auth.json',
    'capability-registry.json',
    'catalog.json',
  ];

  function sortKeys(obj: unknown): unknown {
    if (Array.isArray(obj)) return obj.map(sortKeys);
    if (obj !== null && typeof obj === 'object') {
      const sorted: Record<string, unknown> = {};
      for (const key of Object.keys(obj as Record<string, unknown>).sort()) {
        const val = (obj as Record<string, unknown>)[key];
        if (val === undefined) continue;
        sorted[key] = sortKeys(val);
      }
      return sorted;
    }
    return obj;
  }

  describe('kosong utils L1 golden parity', () => {
    beforeAll(() => {
      spawnSync('cargo', ['build', '-p', 'kosong-rs', '--bin', 'kosong-utils-golden'], {
        cwd: join(rootDir, 'rust-ody'),
        stdio: 'inherit',
      });
    });

    const binaryPath = join(rootDir, 'rust-ody', 'target', 'debug', 'kosong-utils-golden');

    it.each(fixtures)('%s TS matches Rust', async (name) => {
      const fixturePath = join(fixturesDir, name);
      const ts = await runTsKosongUtilsGolden(fixturePath);
      const result = spawnSync(binaryPath, [fixturePath], { encoding: 'utf8' });
      if (result.status !== 0) {
        throw new Error(`kosong-utils-golden exited ${result.status}: ${result.stderr}`);
      }
      const rust = JSON.parse(result.stdout);
      expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
    });
  });
  ```

- [ ] 运行并确认通过：
  ```bash
  pnpm -F @odysseythink/integration-tests test:parity -- test/parity/kosong/l1-utils-golden.test.ts
  ```
  或（若 `test:parity` glob 不包含新文件）：
  ```bash
  pnpm -F @odysseythink/integration-tests test -- test/parity/kosong/l1-utils-golden.test.ts
  ```
  期望：4 个 fixture 测试全部 PASS，无 TS 编译错误。

- [ ] 回归验证已有 golden 测试未受影响：
  ```bash
  pnpm -F @odysseythink/integration-tests test:parity
  ```
  期望：`kosong L1 golden parity` 与 `kosong utils L1 golden parity` 均 PASS，无新增失败。

- [ ] Commit：`test(integration): add kosong utils L1 golden parity test`

---

## Part 2 Local Self-Review

- [ ] 1. Spec-coverage：Task 6-9 共同覆盖 4.2.1 的 L1 parity 验证需求；每个 operation 在 fixtures 中至少有一个 case。
- [ ] 2. Placeholder scan：binary、harness、fixtures、test 文件均给出完整代码/内容，无 `TODO`/`TBD`。
- [ ] 3. No phantom tasks：每个 Task 产生可验证的改动（新增文件或类型检查/测试命令）。
- [ ] 4. Dependency soundness：Task 6 依赖 `core.md`；Task 7 依赖 Task 6；Task 8 依赖 Task 7；Task 9 依赖 Task 6-8。
- [ ] 5. Caller & build soundness：parity 部分未修改共享签名；Task 8 以 `pnpm -F @odysseythink/integration-tests typecheck` 结束；Task 9 以 `test:parity` 全量回归结束。
- [ ] 6. Test-the-risk：fixtures 覆盖边界（空 apiKey、embedding 模型、冲突 tool-call id、`/v1` 剥离）；Task 9 用 `toStrictEqual(sortKeys(...))` 逐 case 断言 TS/Rust 输出一致。
- [ ] 7. Type consistency：binary 与 harness 的 operation 名称、fixture 字段名、`ProviderType` 字符串、`Catalog*` 字段名与 `core.md` / TS 源码一致。
