---
"@odysseythink/agent-core": patch
---

Expose the `RunE2ETests` and `ReviewTests` tools to the model.

Both tools were registered as builtins but were absent from every agent
profile's tool list, so the model never saw them — an auto-injected "Generate and
run E2E tests" task could be planned but never executed (the model would report
"there is no RunE2ETests tool available" and fall back to a plain `go test`).
They are now enabled on the `agent` and `coder` profiles, with a profile
regression test guarding against silent removal.
