# E2E Testing Guide

Ody Code includes an automated end-to-end (E2E) testing framework that detects which builtin tools are affected by your changes and generates+executes temporary Vitest tests to validate them.

## Configuration

E2E testing is enabled by default. Configure it in `~/.ody-code/config.toml`:

```toml
[e2e]
enabled = true
strategy = "smart"         # "always" | "smart" | "critical-only"
critical_tools = ["ExitPlanModeTool"]
failure_policy = "warn"    # "block" | "warn" | "ignore"
max_concurrency = 4
test_timeout = 30000       # milliseconds
report_dir = ".ody-code/test-reports"
generated_test_dir = ".ody-code/test-generated/e2e"
```

- **enabled**: Master toggle. Set to `false` to disable all E2E automation.
- **strategy**: When to inject E2E tasks.
  - `always` — inject for every plan.
  - `smart` — inject only when changed files match known tool patterns.
  - `critical-only` — inject only when critical tools are affected.
- **critical_tools**: Tool class names that should always be treated as highest priority.
- **failure_policy**: How to react to test failures.
  - `block` — return an error and stop the turn.
  - `warn` — include failures in the summary but continue.
  - `ignore` — do not change turn behaviour at all.
- **max_concurrency**: Maximum concurrent Vitest processes.
- **test_timeout**: Per-test timeout in milliseconds.
- **report_dir**: Where JSON reports are saved.
- **generated_test_dir**: Where temporary test files are written.

## How It Works

1. **Plan Enrichment** — When you exit plan mode with `ExitPlanMode`, the framework inspects git status (or the plan content) for changed files. If any builtin tool is affected, a new task is appended to the plan:
   ```
   ### Task N: Generate and run E2E tests
   ```

2. **Test Generation** — The `RunE2ETests` tool detects your project stack (currently TypeScript + Vitest) and generates temporary test files under `.ody-code/test-generated/e2e/`.

3. **Test Execution** — Tests are run in chunks of `max_concurrency` via `pnpm vitest run`. Results are parsed from Vitest's JSON reporter.

4. **Reports** — A JSON report is saved to `.ody-code/test-reports/e2e-report-<timestamp>.json`, and a markdown summary is returned to the model.

## Running Tests Manually

You can ask the agent to run E2E tests at any time:

> RunE2ETests with toolId: "ExitPlanModeTool"

## Go projects (Phase 2)

When the detected project root contains a `go.mod`, the framework selects the Go
generator instead of the TypeScript/Vitest one. It works the same way (detect →
generate → run → report) but is tailored to Go backends:

1. **Detection** — `go.mod` is required. The project is classified by scanning
   `go.mod` and source files:
   - `http-server` — uses `gin` / `echo` / `fiber`, or imports `net/http` together
     with `ListenAndServe` / `http.Serve` / `.Run(`.
   - `cli` — has a `main` package / `cmd/` directory.
   - `generic` — anything else.

2. **Impact** — changed `.go` files (excluding `_test.go`) are grouped by package
   directory; each package becomes a tested "feature". `critical_tools` is matched
   against package paths, and `strategy` (`always` / `smart` / `critical-only`)
   applies as usual.

3. **Generation** — for `http-server` projects the generated
   `<package>_e2e_test.go` is a **real black-box test**: it `go build`s the binary,
   launches it as a subprocess on a free port, waits for the port to accept
   connections, issues an `http.Client` request to the external interface, and
   asserts on the parsed JSON response (with `t.Cleanup` killing the process). The
   file carries a `//go:build e2e` tag and `package e2e`. `cli`/`generic` projects
   get a smoke placeholder. All templates include `TODO` markers for the model to
   fill in the real endpoint and assertions.

4. **Execution** — tests run via `go test -json -tags e2e -timeout=<test_timeout>s`
   and the JSON event stream is parsed into the shared report format.

> **Important:** The Go toolchain ignores directories whose name begins with `.`
> or `_`, so the default `generated_test_dir` (`.ody-code/test-generated/e2e`) is
> unusable for Go. The Go generator automatically falls back to a module-internal
> `e2e_generated/` directory; the `//go:build e2e` tag keeps these files out of
> normal `go build` / `go test ./...` runs.

> **Note:** Generated Go test files are **not** auto-deleted after a run (same as
> the TypeScript generator). They contain `TODO` markers and are meant to be
> inspected and iterated on. Add `e2e_generated/` to `.gitignore` if you do not
> want them committed.

## Limitations

- Supported stacks: TypeScript/Vitest and Go (`go test`). Python/Jest are not yet
  generated.
- For Go, the `http-server` template targets the module root (`.`) by default and
  expects a JSON response at `/`; adjust the generated `buildTarget`, endpoint and
  assertions to match the real service.
- Only a static mapping of tool/package-to-file is used; transitive dependencies
  are not analyzed.
- Generated tests are temporary and not committed to source control.
