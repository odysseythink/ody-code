---
"@odysseythink/agent-core": patch
"@odysseythink/kimi-code": patch
---

Fix `[browser]` config section in config.toml being ignored, so `enabled = false` now correctly disables the built-in Chrome DevTools MCP server.
