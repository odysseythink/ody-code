---
"@odysseythink/agent-core": patch
"@odysseythink/kimi-code": patch
---

Fix `/compact` failing with context overflow on long-context models by capping completion tokens against the remaining context window.
