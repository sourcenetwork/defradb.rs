# PushCoalescing — latest-head retirement (TLA+ design)

Models issue **#1102**'s safety/liveness boundary across the live outbound queue and
the durable retry marker for document and collection scopes. A head can already be
active when a newer local head arrives;
that old send may finish, but its failure must not recreate a stale retry obligation.

The model uses monotonically increasing natural numbers for `(priority, CID)` head
versions. The Rust implementation uses the real block priority with `Cid::cmp` as a
deterministic tie-break across every layer. `LatestOnly` atomically removes queued and
persisted predecessors before installing the new head. `CurrentOnly` checks the version again on
failure, so superseded active work is retired instead of persisted.

The green configuration proves:

- `INV_OneLiveHead`: at most one queued head per `(scope, peer)`.
- `INV_OnePersistedHead`: at most one durable retry head per `(scope, peer)`.
- `INV_NoStaleRetry`: a persisted retry is always the newest known head.
- `INV_NewestRetained`: retirement never drops the newest obligation; it is queued,
  active, persisted, or already acknowledged.
- `INV_ClearedScopeDelivered`: once a scope's marker is clear, every head offered to
  the peer is delivered or an ancestor of a delivered head.

`MC_PushCoalescing_Red_AppendEvery.cfg` demonstrates the live-queue failure, while
`MC_PushCoalescing_Red_StaleRetry.cfg` demonstrates the active-send race that recreates
a superseded persisted retry.

## Sibling heads and the marker (F1)

Heads carry a `Parent` relation. A hub forwards every merged remote head, so two heads
written concurrently on different peers reach the live path for one `(scope, peer)` as
siblings. The live path still sends only one of them: the backlog retires the queued
older version, and the worker's admission check fences any job whose marker
generation is no longer the newest. Retiring a sibling from the volatile queue loses
nothing by itself; the durable marker still stands for it. The loss is the ack: the
ack of the head holding the newest generation cleared the marker, and with it the
sibling that was never sent (`CompleteRule = "Generation"`).

The code's rule (`CompleteRule = "Frontier"`): the worker reads the scope's current
heads (`DocumentHeadProvider`) and the ack carries the marker generation only when the
acked head is the whole frontier, so every head registered before it is one of its
ancestors (`push_worker.rs` `is_whole_frontier`). Otherwise the marker stays set and
the retry ladder sends every current head and clears it once they are acked and
unchanged (`Ladder`; `retry_doc`, `complete_document_retry_if_current`). A failed head
lookup leaves the marker set. One writer's linear heads always have a one-head
frontier, so their acks clear the marker as before.

| Config | Parents | `CompleteRule` | Verdict |
|--------|---------|----------------|---------|
| `MC_PushCoalescing_Green.cfg` | linear | `Frontier` | GREEN: all six invariants |
| `MC_PushCoalescing_Red_AppendEvery.cfg` | linear | `Frontier` | RED: `INV_OneLiveHead` |
| `MC_PushCoalescing_Red_StaleRetry.cfg` | linear | `Frontier` | RED: `INV_NoStaleRetry` |
| `MC_PushCoalescing_Green_Siblings.cfg` | 2 and 3 over 1 | `Frontier` | GREEN: all six invariants |
| `MC_PushCoalescing_Red_SiblingClear.cfg` | 2 and 3 over 1 | `Generation` | RED: `INV_ClearedScopeDelivered` (3 retires queued 2, 3's ack clears the marker) |

The model makes `Fail` and its current-version check atomic. The runtime check and
peerstore write are separated by an asynchronous recorder channel: a newer-head
observation can be processed while no record exists, followed by an already-checked
older failure that creates one stale retry. This narrow interleaving is not covered by
`INV_NoStaleRetry`. Stage 3 closes it with a per-peer serialized marker transition and
an in-process head-ack fence. The model's `persisted` head is a ghost witness for which
head dirtied a scope; Rust persists only the presence marker and rederives current heads.

The model intentionally excludes payload encoding and elapsed time. Each admitted job
contains one head block, while exponential backoff is a deterministic pure function
covered by unit tests.

The runtime's update coalescers use a 250 ms trailing-edge quiet period with a hard
1 second maximum delay. The quiet period absorbs short sequential HTTP bursts; the
maximum forces progress for a continuously written scope and bounds the lifetime
of follower tasks. This deliberately accepts up to 250 ms of latency for an isolated
fan-out or gossip obligation. Transactional document push and gossip are awaited in
order inside the detached broadcaster task, so their independent quiet periods can
stack to about 500 ms before gossip is published. Collection commits use an empty
document ID and therefore share one collection-scoped coalescing key rather than
bypassing the current-head fence. A dropped leader removes its window and wakes
followers to re-admit the latest buffered payload, so cancellation cannot wedge the
scope key.

Durable document and collection records are presence-only dirty markers registered
before send. They share `/rep/retry/id/{peer}` and the exact Go ladder (30 seconds,
then 1, 2, 4, 8, 16, and 32 minutes capped). A retry rederives current heads, and a
success clears a marker only while the serialized scope/head attempt remains current.

If a locally produced head cannot be decoded to obtain its priority, it uses a
CID-scoped volatile obligation key and bypasses scope coalescing; an unproven version
is never retired as an older scope head. The CID is not persisted.

Latest-head retirement in the volatile queue keeps one head per `(scope, peer)`.
Sibling heads do enter this live path: the post-merge replicator fan-out forwards
every merged remote head. Their delivery rests on the marker, which only an ack of the
whole frontier, or the retry ladder, clears (see above). Replay and field/KMS DAG paths
bypass the outbound backlog, and `rebroadcast_on_merge` (gossip) is false at every
production construction site.
