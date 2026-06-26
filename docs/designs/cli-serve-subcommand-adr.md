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
