# ACP (Access Control Policy) Tests

These tests cover ACP behavior over iroh transport, including:
- local ACP replication behavior
- Vera-backed DAC permissioned replication
- Vera-backed document-actor relationship propagation
- NAC trust-boundary behavior

## Files

- `acp.rs` — Local ACP policy enforcement with iroh transport
- `dac.rs` — DAC permissioned replication, including the Go `replicator_with_doc_actor_relationship` and `subscribe_with_doc_actor_relationship` parity cases via Vera-backed relationship propagation
- `nac.rs` — Node Access Control via Vera
- `trust_boundary.rs` — Trust boundary enforcement between iroh peers

## Environment

Some `dac.rs` and `nac.rs` tests require a Vera test environment. The harness still uses the legacy names:
`verad` must be resolvable via `VERA_BINARY`, `VERA_WORKSPACE`, or `PATH`.

Without `verad`, those tests fail at harness startup rather than at ACP assertion time.
