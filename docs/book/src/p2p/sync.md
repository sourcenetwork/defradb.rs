# Push and pull

Replication moves blocks (the DAG units), in three shapes.

## Push: the PushLog

Local commits broadcast as signed PushLog messages over gossip topics
(`crates/p2p/src/sync/`): per-collection and per-document topics, plus fixed topics for
branchable sync and encryption-key distribution. A peer that receives a PushLog stores
the blocks it carries, queues anything missing, and emits sync events; the DB-side
merge runs on the pool, routed to the document's worker with the same key affinity as
a local write ([`crates/db/src/merge/pooled.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/db/src/merge/pooled.rs)).

**Replicators** are registered outbound replay targets: a durable intent to push a
collection's history to a specific peer, with filters (`replication-filter`). Push
delivery is durable: durable delivery markers are written before the committing
transaction reports success, and a persisted retry sweep (every 2 s) drains what the
network missed.

## Pull: doc sync and DAG fetch

- **Doc sync**: a request for specific documents' current heads and bodies over
  pubsub-RPC (bounded: at most 1000 IDs per request by default), served by the peer's
  read-gated fetch handlers.
- **DAG sync**: given a head, fetch the missing blocks: store what arrived, extract
  links, ask for what is missing (bitswap on libp2p; iroh streams with the same shape).
  Pending, partial DAGs live in a persisted pending store; a DAG whose root cannot
  eventually be resolved is quarantined (deterministic forensics, the
  `PendingDagQuarantined` event).
- **CAR transfer**: a single-round-trip bounded DAG export/import (10,000 blocks, 16
  MiB caps), with explicit authorization for what a CAR may contain.

A merge is *admitted* only once its blocks and KMS keys are local: the merge handler
never reaches for the network mid-merge; remote inputs are resolved first (the doctrine
again).

## Backpressure

Everything inbound and outbound is bounded: concurrent DAG fetches and push tasks,
push queue capacity in items and bytes, per-peer rate limits, inbound stream budgets
(the next chapter). A slow peer slows itself, not the node.

> **Why push-and-pull instead of either alone.** Push alone loses updates to
> partitions and late joiners; pull alone turns every write into a polling fan-out.
> Together: pushes propagate writes promptly to live peers; doc sync and DAG fetch
> repair gaps when peers (re)connect; durable delivery markers make push intent
> survivable across restarts. The trade is explicit: each mechanism is simple and
> bounded, and the composition is eventually consistent by construction (the DAG
> convergence models in `proofs/`).
