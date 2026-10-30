# `go.shinzo`: Behaviour Shinzo adds on Go `shinzo-develop`

- **Parents:** ["go"]

Derived from every test that Go `shinzo-develop` commit `2a899b860` (2026-10-26) adds or changes against `develop` commit `b160f4130` (merge base `3de014849`). Integration tests are cited by their path below `tests/integration/`, like the rest of `reqs/go`; unit tests next to the new code are cited by their path from the Go repo root. Every one of those tests is cited by exactly one leaf below, or listed in `reqs/not-applying/shinzo.md` with the reason it does not apply to defradb.rs.

These requirements are verified against Go Shinzo, never Go `develop`: every leaf carries `Go-Ref: "shinzo-develop 2a899b860"`, and its testo test runs under `DEFRA_CLIENT=go-shinzo` (pinned by `GO_SHINZO_COMMIT`, #2443) as well as the Rust clients.

A leaf with an **Interop** line describes bytes or messages a Rust node exchanges with a Go Shinzo node: it must match `shinzo-develop` exactly, and its test runs against a Go `shinzo-develop` node as well as a Rust one.
