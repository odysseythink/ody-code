---
"@odysseythink/agent-core": minor
"@odysseythink/kimi-code": minor
---

Store plan and design mode files under `<cwd>/.ody-code/plans/` and `<cwd>/.ody-code/designs/` respectively, with automatic fallback to session-scoped directories when the project path is not writable. Persist the file path in session records so resume restores the exact location.
