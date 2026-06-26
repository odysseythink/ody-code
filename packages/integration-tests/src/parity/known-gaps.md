# Parity Known Gaps

> Last reviewed: 2026-06-26
> Phase: 4.0.4

| Scenario | Layer | Reason |
|---|---|---|
| hello-world | L3 | Rust 后端 mock provider 未 emit `turn.ended`，scenario 等待超时 |
| mock-prompt | L3 | Rust 后端 mock provider 未 emit `turn.ended`，scenario 等待超时 |
| file-edit | L3 | Rust 后端 mock provider 未 emit `turn.ended`，scenario 等待超时 |
| multi-turn-tool | L3 | Rust 后端 mock provider 未 emit `turn.ended`，scenario 等待超时 |
| session-lifecycle | L3 | Rust 后端 `session.created` / `session.closed` 事件与 TS `agent.status.updated` 事件类型不一致 |
| set-model | L2 | Rust 后端 `createSession` 返回的 `sessionId` 无前缀，TS 返回 `session_<id>` |
| set-model | L3 | Rust 后端 `session.created` / `session.closed` 事件与 TS `agent.status.updated` 事件类型不一致 |
| * | L4 | records 持久化 4.3 才迁移 |
