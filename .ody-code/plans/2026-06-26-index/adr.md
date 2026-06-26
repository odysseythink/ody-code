# Phase A1 Part 3: CLI 启动约定 ADR

> Scope: 撰写 ADR 记录 Phase A1 最终 CLI 合同，包括 Rust `ody-host` 的全局 flags 与 `serve` 子命令互斥规则、TS SDK 的 spawn argv 格式以及 spawn error 处理约定。

---

### Task 6: 撰写 CLI 启动约定 ADR

**Depends on:** Task 1-5（Rust CLI 与 TS SDK 实现完成后，ADR 才能记录最终合同）

**Files:**
- Create: `docs/designs/cli-serve-subcommand-adr.md`

- [ ] Write the complete ADR

创建 `docs/designs/cli-serve-subcommand-adr.md`，内容如下：

```markdown
# ADR: Ody Host CLI Launch Convention (Phase A1)

## Status

Accepted — implemented in Phase A1.

## Context

`ody-host` originally accepted transport flags as top-level arguments, e.g. `ody-host --stdio`.
As the TS TUI and other clients spawn the binary as an external process, we need a single,
unambiguous launch contract that:

1. Works whether the user types the command manually or a client spawns it.
2. Keeps backward compatibility for existing scripts and documentation that use global flags.
3. Rejects ambiguous combinations of global flags and subcommand flags.
4. Surfaces spawn failures to the client immediately and informatively.

## Decision

Introduce a `serve` subcommand that accepts the same flags as the global form.
The global form remains valid but is mutually exclusive with `serve`.

### Rust host contract

```text
ody-host --stdio                         # valid: global form (backward compatible)
ody-host --socket-path /tmp/ody.sock     # valid: global form
ody-host serve --stdio                   # valid: canonical subcommand form
ody-host serve --socket-path /tmp/ody.sock  # valid: canonical subcommand form
ody-host --stdio serve --socket-path /tmp/ody.sock  # invalid: mixed global + subcommand
```

All recognized flags (global or under `serve`):

- `--stdio`
- `--socket-path <PATH>`
- `--tcp-host <HOST>`
- `--tcp-port <PORT>`
- `--config <PATH>`
- `--home <PATH>`
- `--log-level <debug|info|warn|error>`

Transport precedence inside `HostConfig::from_cli` is unchanged:
`socket_path` > `tcp_host + tcp_port` > `stdio` (default).

### TypeScript SDK contract

`SDKRpcClient.connect` spawns the external binary with `serve` as the first positional argument,
followed by transport flags and optional `--config` / `--home` extras:

```text
<binary> serve --stdio
<binary> serve --socket-path <PATH>
<binary> serve --tcp-host <HOST> --tcp-port <PORT>
```

If `spawn` fails before the host prints its ready message (e.g. `ENOENT`, `EACCES`),
`createExternalTransport` rejects with an error containing:

- the absolute or configured `binaryPath`, and
- the exact `argv` array passed to `spawn`.

Example:

```text
Failed to spawn host /usr/local/bin/ody-host with args ["serve","--stdio"]: spawn ENOENT
```

## Trade-offs

Pros:
- Clear canonical form (`serve`) for programmatic spawning.
- Backward-compatible global form avoids breaking existing CI/docs.
- Explicit rejection of mixed usage prevents silent misconfiguration.
- Spawn error message includes both path and argv, reducing debugging time.

Cons:
- Two equivalent syntaxes increase documentation surface.
- Clients must always prepend `serve`; older clients that spawned `<binary> --stdio` must be updated.

## Migration Notes

- Update any shell scripts or CI steps from `ody-host --stdio` to `ody-host serve --stdio`
  if you prefer the canonical form; the global form continues to work.
- Clients using `SDKRpcClient.connect` already receive the new argv format automatically.
- Do not combine global flags with the `serve` subcommand; the host will exit with a config error.

## Consequences

- `rust-ody/crates/ody-host/src/config.rs` parses `Command::Serve(ServeArgs)` and rejects mixed usage.
- `packages/node-sdk/src/rpc.ts` wraps host spawning in `spawnHost`, which listens for `proc.on('error')`.
- Future transport flags should be added to both `SharedArgs` (Rust) and the `spawnHost` argv builder (TS).
```

- [ ] Manual verification

1. 确认文件存在且 Markdown 语法正确：

```bash
cat /Users/ranwei/workspace/ody-code/docs/designs/cli-serve-subcommand-adr.md | head -20
```

Expected observation：终端输出 ADR 前 20 行，包含 `# ADR: Ody Host CLI Launch Convention (Phase A1)` 与 `## Status`。

2. 确认 ADR 中的示例与 Part 1/Part 2 的实现一致：
   - Rust 拒绝 `ody-host --stdio serve --socket-path ...` → 对应 `rust-cli.md` Task 3 的 `global_flags_conflict_with_serve_rejected`。
   - TS spawn argv 以 `serve` 开头 → 对应 `ts-sdk.md` Task 5 的 mock 断言。
   - spawn error 消息格式 `Failed to spawn host <binaryPath> with args <argv>` → 对应 `ts-sdk.md` Task 4 的 `spawnHost` 模板字符串。

- [ ] Commit

```bash
cd /Users/ranwei/workspace/ody-code
git add docs/designs/cli-serve-subcommand-adr.md
git commit -m "docs: add ADR for ody-host serve subcommand contract"
```

---

## Local Self-Review

- [ ] 1. Spec-coverage table（本 Part）：
  - ADR 记录最终 CLI 合同 → Task 6 covered
- [ ] 2. Placeholder scan：本 Part 无 TODO/TBD/"implement later"。
- [ ] 3. No phantom tasks：Task 6 产生一个可验证的 ADR 文件。
- [ ] 4. Dependency soundness：Task 6 依赖 Part 1 与 Part 2 的最终实现，仅用于记录，不反向依赖。
- [ ] 5. Caller & build soundness：ADR 为纯文档，无签名/调用者变更；无需类型检查。
- [ ] 6. Test-the-risk：ADR 不涉及状态变更；通过人工校验确保示例与实现常量一致。
- [ ] 7. Type consistency：ADR 不引入代码类型；文档中的 flag 名称与 `rust-cli.md`/`ts-sdk.md` 完全一致。
