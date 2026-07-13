---
"@odysseythink/agent-core": minor
"@odysseythink/agent-core-shared": minor
"ody-code": minor
---

Add an experimental session memory pipeline that writes a per-session summary on Stop/SessionEnd and injects the latest prior-session summary at startup, gated behind the `session-memory` flag and configurable via `[sessionMemory]` in config.toml.
