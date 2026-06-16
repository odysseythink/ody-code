Generate and run temporary end-to-end (E2E) tests for the current project. Use this tool after completing implementation work to validate that your changes haven't broken the affected builtin tools.

When called without arguments, the tool detects changed files (via git status or the approved plan), analyzes which builtin tools are affected, generates temporary Vitest test files, runs them with pnpm vitest run, parses the JSON output into a report, and returns a markdown summary.

The tool respects the `[e2e]` section in config.toml: disable with `enabled = false`, control failure behaviour with `failure_policy` (`block` / `warn` / `ignore`), and adjust parallelism with `max_concurrency`.
