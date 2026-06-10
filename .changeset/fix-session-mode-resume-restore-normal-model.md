---
"@odysseythink/agent-core": patch
---

Fix the normal-mode model not being restored after resuming directly into plan/design mode: switching back to normal in the same session now correctly reverts to `default_model` instead of staying on the resumed mode's model.
