# db-merge — formal-modelability survey

> **Now a module, not a crate.** `db-merge` was folded into `db` as `crates/database/db/src/merge/`. The slice below is unchanged; only its location moved.

## Purpose
DB-side implementation of P2P merge/broadcast/push, formerly behind `#[cfg(p2p)]` in
`db`. Implements the `MergeHandler` trait: decode IPLD blocks, verify signatures,
walk the composite DAG, apply LWW/Counter CRDT deltas, update heads, gate via ACP,
and (re)generate searchable-encryption artifacts. Also wraps the mutator to broadcast
local commits (`broadcast_mutator`) and pushes existing docs to replicators
(`push_docs*`, `replication`).

## State machines
- **Composite DAG merge** (`merge_handler/composite*.rs`): recursive ancestry walk
  bounded by `MAX_MERGE_DEPTH`, with `merged_composites`/`merged_collections` dedup
  sets and a blockstore merged-set for CRDT idempotency. Heads advanced in
  `composite_heads.rs` (delete parent heads, set new head + priority index).
- **Signature gate** (`signature.rs`): verify-before-merge; `Err` ⇒ reject block.
- **ACP register-on-merge** (`acp_merge_handler.rs`): post-commit replicated-doc
  registration + strict replicated-doc-access gate.
- **SE coordinator** (`se/`): artifact generate → store/serve → receive/validate.
- **Broadcast** (`broadcast_mutator/`): fire-and-forget post-commit broadcast.

## Candidates

| name | kind | property | already-modeled | priority |
|------|------|----------|-----------------|----------|
| composite-dag-completeness | TLA+ | no composite merged without its merged parents; dedup set makes re-delivery idempotent | yes (Convergence INV_DagComplete) | low |
| sig-verify-before-merge | TLA+ | invalid signature ⇒ block rejected before any state change | yes (Integrity) | low |
| acp-register-on-merge | TLA+ | replicated doc gated/registered; strict access enforced both paths | yes (Acp, Commits) | low |
| head-advance-monotone | Lean | head update = delete-parents ∪ {new}; priority varint round-trips | yes (db-blocks survey / CRDT-laws) | low |
| se-artifact-roundtrip | either | artifact gen/serve/receive consistency | yes (db-search survey, KMS) | low |

## Verdict
**Model-worthy: no new slice.** This crate implements behaviors already abstracted
by existing slices (Convergence, Integrity, Acp, Commits, db-blocks/db-search). The
former per-document `MergeQueue` is gone, so its proposed concurrency model no
longer describes the code. Broadcast, push_docs and replication wiring remain
plumbing covered by integration tests.
