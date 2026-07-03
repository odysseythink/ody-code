# 4.2.5 Chat-Completions 兼容三兄弟（Kimi / DeepSeek / GLM）Implementation Plan

**Goal:** 将 `packages/kosong/src/providers/` 下的 Kimi、DeepSeek、GLM 三家 OpenAI-Compatible provider 迁移到 `kosong-rs`，并建立 L1 golden fixtures + TS↔Rust parity 测试，确保 Rust 实现与 TS 逐值一致。

**Architecture:** 复用 4.2.2 已落地的 `chat_completions_stream` 共享解析器与 `openai_common` 工具函数；Kimi 因 `reasoning_content` 专用 usage 位置与工具参数归一化需要少量扩展；DeepSeek 直接封装 `OpenAILegacyChatProvider`；GLM 作为独立 Chat-Completions provider 实现。每家 provider 最终提供 `ChatProvider` trait 实现，并通过独立的 golden binary + TS runner 做 L1 对照。

**Tech Stack:** Rust (`kosong-rs` crate, `reqwest`, `httwest`, `serde_json`), TypeScript (vitest parity fixtures)。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

新增/修改文件清单（按子系统分组）：

```
rust-ody/crates/kosong-rs/
  Cargo.toml                                    # 新增 kimi/deepseek/glm golden binaries
  src/
    capability_registry.rs                      # 新增三家 capability 查询函数
    chat_completions_stream.rs                  # 新增 usage extractor 扩展点（Kimi 需要）
    providers/
      mod.rs                                    # 导出 kimi / deepseek / glm 模块
      kimi.rs                                   # KimiChatProvider + KimiFiles
      deepseek.rs                               # DeepSeekChatProvider
      glm.rs                                    # GLMChatProvider
    bin/
      kimi_golden.rs                            # Kimi L1 golden 二进制
      deepseek_golden.rs                        # DeepSeek L1 golden 二进制
      glm_golden.rs                             # GLM L1 golden 二进制

packages/integration-tests/
  src/parity/
    kosong-kimi-golden.ts                       # Kimi TS golden runner
    kosong-deepseek-golden.ts                   # DeepSeek TS golden runner
    kosong-glm-golden.ts                        # GLM TS golden runner
    fixtures/
      kosong-kimi/                              # Kimi fixtures
      kosong-deepseek/                          # DeepSeek fixtures
      kosong-glm/                               # GLM fixtures
  test/parity/kosong/
    l1-kimi-golden.test.ts                      # Kimi L1 parity test
    l1-deepseek-golden.test.ts                  # DeepSeek L1 parity test
    l1-glm-golden.test.ts                       # GLM L1 parity test
```

---

## Dependency Overview

按执行顺序分为 5 个 Phase。Phase 1 是硬前置；Phase 2/3/4 彼此独立，可并行开发；Phase 5 依赖前三家 provider 全部完成。

```
Phase 1: Shared groundwork
  ├─ 1.1 capability_registry 增加 Kimi/DeepSeek/GLM capability
  ├─ 1.2 chat_completions_stream 增加 usage extractor 扩展点
  └─ 1.3 kimi-schema 工具参数归一化

Phase 2: Kimi provider
  ├─ 2.1 KimiChatProvider 构造与 ChatProvider trait 壳
  ├─ 2.2 消息/工具转换与请求体组装
  ├─ 2.3 响应解析 + generate() 完整路径
  └─ 2.4 KimiFiles.uploadVideo

Phase 3: DeepSeek provider
  ├─ 3.1 DeepSeekChatProvider 构造
  ├─ 3.2 capability + generate() 端到端

Phase 4: GLM provider
  ├─ 4.1 GLMChatProvider 构造
  ├─ 4.2 消息/工具转换与请求体组装
  └─ 4.3 generate() 完整路径

Phase 5: L1 golden parity
  ├─ 5.1 Rust golden binaries
  ├─ 5.2 TS golden runners
  ├─ 5.3 fixtures
  └─ 5.4 vitest parity tests
```

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| Kimi `choices[0].usage` 与 top-level `usage` 的解析差异 | 给 `parse_stream_response` 增加 extractor 扩展点，不改变既有签名； fixture 覆盖两种位置 |
| Kimi 工具参数归一化（`normalizeKimiToolSchema`）涉及 JSON Schema dereference，Rust 实现易与 TS 在循环引用/ siblings 合并处漂移 | 单独 L1 fixture，输入真实 MCP 工具 schema，比对输出 JSON |
| DeepSeek 为空 API key 时需避免回退到 `OPENAI_API_KEY` | 单元测试断言 delegate 的 api_key 为 `""` |
| GLM 多媒体内容抛错行为 | fixture 覆盖 `image_url`/`audio_url`/`video_url` 输入，断言 error |
| 三家 provider 的 `with_thinking` 映射不同 | 每个 provider 单独测试 effort → request 字段映射 |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-27-backend-architecture-evolution-phase4-2-5/shared.md` | capability registry + chat-completions usage extractor + kimi-schema | done |
| 2 | `2026-06-27-backend-architecture-evolution-phase4-2-5/kimi.md` | KimiChatProvider + KimiFiles | done |
| 3 | `2026-06-27-backend-architecture-evolution-phase4-2-5/deepseek.md` | DeepSeekChatProvider | done |
| 4 | `2026-06-27-backend-architecture-evolution-phase4-2-5/glm.md` | GLMChatProvider | done |
| 5 | `2026-06-27-backend-architecture-evolution-phase4-2-5/parity.md` | L1 golden fixtures + TS↔Rust parity | done |

---

## Self-Review

- [ ] 1. **Spec-coverage table:**
  | Spec section (4.2.5) | Covered by | Status |
  |---|---|---|
  | Kimi provider migration (`KimiChatProvider`, `KimiFiles.uploadVideo`) | `kimi.md` Tasks 2.1–2.4 | covered |
  | DeepSeek provider migration (`DeepSeekChatProvider`, API-key isolation) | `deepseek.md` Tasks 3.1–3.2 | covered |
  | GLM provider migration (`GLMChatProvider`, empty-text filtering, multimedia rejection) | `glm.md` Tasks 4.1–4.4 | covered |
  | Shared `chat_completions_stream` usage-extractor extension | `shared.md` Task 1.2 | covered |
  | `capability_registry` entries for all three providers | `shared.md` Task 1.1 + `glm.md` Task 4.1 | covered |
  | `kimi-schema` tool parameter normalization | `shared.md` Task 1.3 + `kimi.md` Task 2.2 | covered |
  | L1 golden fixtures + TS↔Rust parity for all three providers | `parity.md` Tasks 5.1–5.4 | covered |
  | Whole-tree Rust + TypeScript typecheck after changes | `kimi.md` Task 2.4 + `glm.md` Task 4.3 + `parity.md` Task 5.4 | covered |

- [ ] 2. **Placeholder scan:** No TODO/TBD/deferred placeholders remain in any part file; every task provides complete code/fixtures/commands.
- [ ] 3. **No phantom tasks:** Every part/task produces a verifiable file change and a passing test/build step; no `--allow-empty` commits.
- [ ] 4. **Dependency soundness:** Phase order is Shared → Kimi/DeepSeek/GLM (independent) → Parity. Each task's `Depends on:` references only earlier tasks or prerequisite parts.
- [ ] 5. **Caller & build soundness:** Shared-signature changes (`HttpClient::post_multipart` in `kimi.md` Task 2.4) explicitly update `ReqwestClient`, `MockHttpClient`, and test-only `CaptureJsonClient`; each signature-changing task ends with `cargo check --workspace --tests` or `pnpm -r typecheck`.
- [ ] 6. **Test-the-risk:** Every provider task includes behavioral tests for message conversion, API-key handling, streaming/non-streaming parsing, and error paths. Parity fixtures assert TS↔Rust equivalence on text, tool-call, and error outputs.
- [ ] 7. **Type consistency:** Provider options, capability functions, and golden binary names match across part files, crate exports, and TS runners.
