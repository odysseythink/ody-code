---
"@odysseythink/agent-core": patch
---

Make the agent reliably test externally-facing interfaces.

Add a system-prompt directive: when the model adds or changes an HTTP
endpoint/handler, RPC method, or CLI command, it must add a test that exercises
it through that interface and asserts on the response (status + parsed body),
not only lower-level unit tests. The injected E2E plan task now carries the same
instruction. This is project- and language-agnostic, so any project ody-code
works on gets handler-level HTTP coverage by default.
