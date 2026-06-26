#!/usr/bin/env python3
"""
External Python client PoC for `ody serve`.

Demonstrates the Ody Code wire protocol over TCP:
  1. Spawn `ody serve --host 127.0.0.1 --port 0`.
  2. Parse the ready JSON line from stderr to obtain host/port/token.
  3. Perform TCP handshake with length-prefixed framing.
  4. Call CoreAPI methods (createSession, prompt).
  5. Handle server-to-client SDKAPI requests (emitEvent, requestApproval, ...)
     in a background reader thread.
  6. Print streamed events until the turn ends or an error occurs.

Run from repository root:
    python3 .ody-code/poc/external-client.py

Requirements:
    - Python 3.10+
    - A built Ody Code CLI at ./apps/ody-code/dist/main.mjs (or `ody` on PATH).
    - Optional: a configured LLM provider if you want the prompt to actually run.
      Without a provider the prompt will fail with a clear error event, which is
      still enough to verify the wire protocol.
"""

from __future__ import annotations

import json
import os
import socket
import struct
import subprocess
import sys
import threading
import time
import uuid
from pathlib import Path
from typing import Any, Callable


# ---------------------------------------------------------------------------
# Protocol helpers
# ---------------------------------------------------------------------------


def u32le(value: int) -> bytes:
    return struct.pack("<I", value)


def read_u32le(data: bytes, offset: int = 0) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def encode_json(value: Any) -> bytes:
    return json.dumps(value, separators=(",", ":")).encode("utf-8")


def decode_json(data: bytes) -> Any:
    return json.loads(data.decode("utf-8"))


def _bytes_from_field(field: Any) -> bytes:
    """Convert a Uint8Array-as-JSON into bytes.

    Node.js may serialize Uint8Array either as a list of integers or as an
    object with numeric string keys (e.g. {"0": 123, "1": 34, ...}).
    """
    if isinstance(field, list):
        return bytes(int(b) for b in field)
    if isinstance(field, dict):
        pairs = [(int(k), int(v)) for k, v in field.items() if k.isdigit()]
        pairs.sort(key=lambda item: item[0])
        return bytes(v for _k, v in pairs)
    return b""


# ---------------------------------------------------------------------------
# Wire transport
# ---------------------------------------------------------------------------


class OdyWireClient:
    """Low-level length-prefixed RPC client for the Ody serve TCP wire protocol."""

    def __init__(
        self,
        host: str,
        port: int,
        token: str | None,
        on_request: Callable[[str, Any], Any],
    ) -> None:
        self.host = host
        self.port = port
        self.token = token
        self.on_request = on_request
        self._sock: socket.socket | None = None
        self._pending: dict[str, threading.Event] = {}
        self._results: dict[str, bytes] = {}
        self._reader_thread: threading.Thread | None = None
        self._shutdown = threading.Event()
        self._buffer = b""
        self._lock = threading.Lock()

    def connect(self) -> None:
        self._sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._sock.connect((self.host, self.port))

        handshake = {"framing": "length-prefixed"}
        if self.token is not None:
            handshake["token"] = self.token
        self._sock.sendall(encode_json(handshake) + b"\n")

        self._reader_thread = threading.Thread(target=self._reader_loop, daemon=True)
        self._reader_thread.start()

    def close(self) -> None:
        self._shutdown.set()
        if self._sock is not None:
            try:
                self._sock.close()
            except OSError:
                pass
        if self._reader_thread is not None:
            self._reader_thread.join(timeout=2.0)

    def call(self, method: str, payload: Any, timeout: float = 30.0) -> Any:
        req_id = str(uuid.uuid4())
        inner = encode_json({"method": method, "args": [payload]})
        frame = {
            "kind": "request",
            "reqId": req_id,
            "bytes": list(inner),
        }

        event = threading.Event()
        with self._lock:
            self._pending[req_id] = event

        raw = encode_json(frame)
        prefixed = u32le(len(raw)) + raw
        if self._sock is None:
            raise RuntimeError("not connected")
        self._sock.sendall(prefixed)

        if not event.wait(timeout=timeout):
            with self._lock:
                self._pending.pop(req_id, None)
            raise TimeoutError(f"RPC call timed out: {method}")

        with self._lock:
            response_bytes = self._results.pop(req_id)
        response = decode_json(response_bytes)
        if not response.get("ok"):
            error = response.get("error", {})
            raise RuntimeError(f"RPC error ({method}): {error.get('message', error)}")
        return response.get("value")

    def _reader_loop(self) -> None:
        assert self._sock is not None
        try:
            while not self._shutdown.is_set():
                chunk = self._sock.recv(64 * 1024)
                if not chunk:
                    break
                self._buffer += chunk
                self._drain()
        except OSError:
            pass
        finally:
            # Wake up any pending callers.
            with self._lock:
                for event in self._pending.values():
                    event.set()

    def _drain(self) -> None:
        while True:
            if len(self._buffer) < 4:
                return
            length = read_u32le(self._buffer)
            if length > 64 * 1024 * 1024:
                raise ValueError(f"frame too large: {length}")
            if len(self._buffer) < 4 + length:
                return

            payload = self._buffer[4 : 4 + length]
            self._buffer = self._buffer[4 + length :]
            self._handle_frame(payload)

    def _handle_frame(self, payload: bytes) -> None:
        frame = decode_json(payload)
        kind = frame.get("kind")
        req_id = frame.get("reqId")

        if kind == "response":
            response_bytes = _bytes_from_field(frame.get("bytes"))
            with self._lock:
                self._results[req_id] = response_bytes
                event = self._pending.pop(req_id, None)
            if event is not None:
                event.set()
            return

        if kind == "request":
            request_bytes = _bytes_from_field(frame.get("bytes"))

            rpc = decode_json(request_bytes)
            method = rpc.get("method", "")
            args = rpc.get("args", [])
            try:
                value = self.on_request(method, args[0] if args else {})
                response_inner = {"ok": True, "value": value}
            except Exception as exc:  # noqa: BLE001
                response_inner = {"ok": False, "error": {"message": str(exc)}}

            response_frame = {
                "kind": "response",
                "reqId": req_id,
                "bytes": list(encode_json(response_inner)),
            }
            raw = encode_json(response_frame)
            if self._sock is not None:
                self._sock.sendall(u32le(len(raw)) + raw)
            return


# ---------------------------------------------------------------------------
# SDKAPI method handlers invoked by the server
# ---------------------------------------------------------------------------


class OdyClientHandlers:
    def __init__(self) -> None:
        self.events: list[dict[str, Any]] = []
        self._lock = threading.Lock()

    def on_request(self, method: str, payload: Any) -> Any:
        if method == "emitEvent":
            with self._lock:
                self.events.append(payload)
            self._print_event(payload)
            return None

        if method == "requestApproval":
            print("[server] approval request -> auto-approve")
            return {"decision": "approved"}

        if method == "requestQuestion":
            print("[server] question request -> no answer")
            return None

        if method == "openExternal":
            print("[server] openExternal request -> decline")
            return {"opened": False, "error": "No external handler in PoC"}

        if method == "toolCall":
            print(f"[server] toolCall request -> unsupported ({payload.get('toolCallId')})")
            return {"output": "SDK custom tool calls are not supported", "isError": True}

        if method == "chatStreamInit":
            print(f"[server] chatStreamInit -> accept stream {payload.get('streamId')}")
            return {"streamId": payload.get("streamId")}

        if method == "chatStreamCancel":
            print(f"[server] chatStreamCancel -> {payload.get('streamId')}")
            return None

        print(f"[server] unhandled SDKAPI method: {method}")
        return None

    @staticmethod
    def _print_event(event: dict[str, Any]) -> None:
        event_type = event.get("type", "unknown")
        if event_type == "text":
            print(f"[event:text] {event.get('text', '')}", end="")
        elif event_type == "thinking":
            print(f"[event:thinking] {event.get('text', '')}", end="")
        elif event_type == "tool_call":
            print(f"[event:tool_call] {event.get('name')}({event.get('arguments')})")
        elif event_type == "tool_result":
            print(f"[event:tool_result] {event.get('toolCallId')}: {event.get('output', '')[:200]}")
        elif event_type == "turn.ended":
            print("[event:turn.ended]")
        elif event_type == "error":
            print(f"[event:error] {event.get('message')}")
        else:
            print(f"[event:{event_type}] {json.dumps(event, ensure_ascii=False)[:300]}")


# ---------------------------------------------------------------------------
# Main demonstration
# ---------------------------------------------------------------------------


def find_ody_binary() -> list[str]:
    """Return argv prefix for invoking the Ody CLI."""
    repo_main = Path(__file__).resolve().parents[2] / "apps" / "ody-code" / "dist" / "main.mjs"
    env_binary = os.environ.get("ODY_BINARY")
    if env_binary:
        return env_binary.split()
    if repo_main.exists():
        return ["node", str(repo_main)]
    return ["ody"]


def main() -> int:
    ody_argv = find_ody_binary()
    print(f"[poc] starting server: {' '.join(ody_argv)} serve --host 127.0.0.1 --port 0")

    proc = subprocess.Popen(
        [*ody_argv, "serve", "--host", "127.0.0.1", "--port", "0"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    ready: dict[str, Any] | None = None
    try:
        for line in proc.stderr:  # type: ignore[union-attr]
            line = line.strip()
            if not line:
                continue
            try:
                msg = json.loads(line)
            except json.JSONDecodeError:
                print(f"[server stderr] {line}")
                continue
            if msg.get("type") == "ready":
                ready = msg
                break
            print(f"[server] {line}")
    except Exception as exc:  # noqa: BLE001
        print(f"[poc] failed to read ready message: {exc}")
        proc.terminate()
        return 1

    if ready is None:
        print("[poc] server did not emit ready message")
        proc.terminate()
        return 1

    host = ready.get("host", "127.0.0.1")
    port = ready.get("port", 0)
    token = ready.get("token")
    print(f"[poc] server ready at {host}:{port} (token={'set' if token else 'none'})")

    handlers = OdyClientHandlers()
    client = OdyWireClient(host, port, token, handlers.on_request)

    try:
        client.connect()
        print("[poc] connected and handshake complete")

        # Sanity check: ask for core info.
        core_info = client.call("getCoreInfo", {})
        print(f"[poc] getCoreInfo -> version {core_info.get('version')}")

        # Create a session in the current working directory.
        work_dir = os.getcwd()
        session = client.call("createSession", {"workDir": work_dir})
        session_id = session["id"]
        print(f"[poc] createSession -> {session_id}")

        # Send a simple prompt. The protocol requires agentId and sessionId in the payload.
        prompt_text = "Say hello and stop."
        client.call(
            "prompt",
            {
                "agentId": "main",
                "sessionId": session_id,
                "input": [{"type": "text", "text": prompt_text}],
            },
            timeout=5.0,
        )
        print("[poc] prompt accepted; waiting for events...")

        # Wait for events in the background. Stop on turn.ended or error, or after timeout.
        deadline = time.monotonic() + 30.0
        while time.monotonic() < deadline:
            time.sleep(0.2)
            with handlers._lock:
                if any(e.get("type") == "turn.ended" for e in handlers.events):
                    break
                if any(e.get("type") == "error" for e in handlers.events):
                    break
        else:
            print("[poc] timed out waiting for turn end")

        # Clean up.
        client.call("closeSession", {"sessionId": session_id}, timeout=5.0)
        print("[poc] session closed")
    except Exception as exc:  # noqa: BLE001
        print(f"[poc] error during RPC: {exc}")
        return 1
    finally:
        client.close()
        proc.terminate()
        try:
            proc.wait(timeout=5.0)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()

    print(f"[poc] captured {len(handlers.events)} event(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
