---
"@odysseythink/agent-core": minor
"ody-code": major
---

Replace `/request-code-review` and `/receive-code-review` with a single `/review` slash command. Running `/review` without arguments opens a picker to request a review or enter receive-feedback mode; running `/review` with arguments (for example `--pr 123`) requests a review directly. The `requesting-code-review` and `receiving-code-review` built-in skills are removed; feedback-handling guidance is now injected by the receive branch of `/review`.
