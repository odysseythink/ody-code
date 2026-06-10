---
"@odysseythink/agent-core": minor
"@odysseythink/kimi-code": minor
---

Plan and design mode now let the model invent its own filename on first write; the host normalizes it with a date prefix and deduplication, and persists the resolved path for session replay.
