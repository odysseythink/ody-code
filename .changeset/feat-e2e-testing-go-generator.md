---
"@odysseythink/agent-core": minor
---

Add Go support to the E2E testing automation framework (Phase 2). The
`E2ETestGenerator` interface now owns impact analysis and test execution, making
`E2ETestExecutor` a language-agnostic orchestrator instead of being hardcoded to
Vitest. The new Go generator detects `go.mod` projects, classifies them
(HTTP-server / CLI / generic), groups changed `.go` files into per-package
features, and for HTTP servers generates a real black-box `go test` that builds
the binary, spawns it as a subprocess, calls the external HTTP interface and
asserts on the parsed JSON response. Results are parsed from `go test -json`.
Generated Go tests use a `//go:build e2e` tag and a non-dot output directory to
avoid the Go toolchain ignoring `.`-prefixed paths. `go test` runs once per unique
package directory (deduped) so packages are never double-counted, the run honors
the turn's `AbortSignal` (killing the subprocess on cancel), and the `go build`
target is inferred from a root `main` package or a single `./cmd/<name>` layout.
Generated test files are intentionally not auto-deleted (consistent with the
TypeScript generator) since they carry `TODO` markers for further editing.
