Independently audit the unit tests you just wrote, using a SEPARATE adversarial model (judge ≠ athlete). Use this after writing or changing tests — because the same model wrote both the tests and the code under test, its blind spots are correlated, so a tautological, mock-only, or happy-path-only test can pass while proving nothing.

When called, the tool detects changed test files (via git status), gathers them plus their implementation files, and sends them to the configured reviewer model (`mode_models.test_review`, falling back to `mode_models.review`, then `default_model`). The reviewer attacks the tests for tautology, mock theatre, missing must-reject cases, and weak assertions, and returns severity-tagged findings PLUS up to three executable "mutation probes".

A mutation probe is a one-line break (negate a condition, change a constant, early-return) the reviewer believes a sound test must catch. The reviewer cannot run code, so YOU must run each probe: apply the break, run the named test, confirm it turns red, then revert. A test that stays green under its probe is vacuous — fix it. Report caught/missed for each probe.

Takes an optional `projectRoot` (defaults to the workspace root). Returns a markdown report of findings and probes.
