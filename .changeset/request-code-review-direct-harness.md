---
"ody-code": minor
"@odysseythink/agent-core": patch
---

Make `/request-code-review`, `/requesting-code-review`, and `/skill:requesting-code-review` invoke the code review engine directly through the harness instead of relying on the `RequestCodeReview` tool being exposed in the active agent profile.
