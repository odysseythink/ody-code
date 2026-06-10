---
"@odysseythink/agent-core": patch
---

Fix design/plan file names getting a doubled date prefix (e.g. `2026-06-10-2026-06-10-foo.md`) when the model's requested path already includes a `YYYY-MM-DD-` prefix.
