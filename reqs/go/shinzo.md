# `go.shinzo`: Behaviour Shinzo adds on Go `shinzo-develop`

- **Parents:** ["go"]

Derived from every test that Go `shinzo-develop` commit `2a899b860` (2026-10-26) adds or changes against `develop` commit `b160f4130` (merge base `3de014849`). Integration tests are cited by their path below `tests/integration/`, like the rest of `reqs/go`; unit tests next to the new code are cited by their path from the Go repo root. Every one of those tests is cited by exactly one leaf below, or by name in the `Go-Tests` line of the `defra` requirement that now holds the behaviour (purge, save-many, orphan sweep, replication filter field values, unique-index errors, truncate ownership, index range filters, neighbour updates, typed arrays, collection IDs, cyclic fragments), or listed in `reqs/not-applying/shinzo.md` with the reason it does not apply to defradb.rs.

These requirements come from Go Shinzo, not Go `develop`: every leaf carries `Go-Ref: "shinzo-develop 2a899b860"`, the commit its Go tests were read at. Their tests run against defradb.rs; none starts a Go Shinzo node.

A leaf with an **Interop** line describes bytes a Rust node shares with Go Shinzo: they must match `shinzo-develop` exactly, and its test checks them against bytes Go Shinzo wrote.
