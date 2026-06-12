---
"@odysseythink/agent-core": patch
"@odysseythink/kimi-code": patch
---

Fix `/compact` failing with `400 tool_call_id is not found` when the session history contains an orphaned tool result.
