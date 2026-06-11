---
"@odysseythink/agent-core": patch
"ody-code": patch
---

Fix the plan→normal handoff in manual/approval mode. Previously, approving a plan through the review surface exited plan mode without running the tool's handoff, so the approved plan content and filename never reached the normal context (the normal partition was left empty). The approval policy now lets the tool execute the handoff, and the selected approach (for multi-option plans) is carried into the normal partition.
