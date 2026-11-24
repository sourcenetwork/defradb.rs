# ACP (Access Control Policy) Tests

These tests cover ACP behavior over iroh transport, including:
- local ACP replication behavior
- NAC trust-boundary behavior

## Files

- `acp.rs` — Local ACP policy enforcement with iroh transport
- `dac.rs` — DAC permissioned replication under local ACP. The Vera-backed cases, including the Go `replicator_with_doc_actor_relationship` and `subscribe_with_doc_actor_relationship` parity cases, are in `tests/verars/iroh_dac.rs`
- `nac.rs` — Node Access Control
- `trust_boundary.rs` — Trust boundary enforcement between iroh peers
