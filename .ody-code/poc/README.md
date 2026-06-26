# External client PoC

This directory contains a self-contained Python client that talks to `ody serve` over the raw TCP wire protocol. It demonstrates that an external process can:

1. Spawn the Ody Code headless server (`ody serve --host 127.0.0.1 --port 0`).
2. Parse the ready message emitted on stderr to discover host/port/token.
3. Handshake with length-prefixed framing.
4. Call `CoreAPI` methods (`getCoreInfo`, `createSession`, `prompt`, `closeSession`).
5. Handle server-to-client `SDKAPI` requests (`emitEvent`, `requestApproval`, `requestQuestion`, `openExternal`, `toolCall`, `chatStreamInit`, `chatStreamCancel`) in a background reader thread.
6. Print streamed events until the turn ends or an error occurs.

## Run

From the repository root:

```bash
python3 .ody-code/poc/external-client.py
```

The script will first try to use the local built CLI at `./apps/ody-code/dist/main.mjs`. You can override this with the `ODY_BINARY` environment variable, e.g.:

```bash
ODY_BINARY=ody python3 .ody-code/poc/external-client.py
```

## Notes

- Python 3.10+ is required (uses `str | None` union syntax).
- If no LLM provider is configured, the `prompt` call will fail with a clear `error` event. This is still sufficient to verify the wire protocol, handshake, and RPC dispatch.
- The server accepts only one concurrent client connection; the script owns the single connection for its lifetime.
