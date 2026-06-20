---
"@odysseythink/agent-core": patch
---

Make interface tests authentication-aware.

The interface-test guidance and the generated Go E2E template previously fired a
bare unauthenticated request and asserted 200 — guaranteeing a false failure on
any protected endpoint. Now:

- The system-prompt directive and the injected E2E plan task tell the model to
  supply a valid credential for authenticated interfaces (test token, the
  project's test-auth helper/bypass, or a real login) and to also assert the
  unauthorized (401/403) path.
- The Go http-server template builds the request via `http.NewRequest` + an
  `Authorization` header hook (with a TODO on how to obtain the credential) and
  includes a commented unauthorized-path assertion, instead of a bare GET.
