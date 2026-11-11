# DAG Convergence - Delivery And Local Merge

This slice factors convergence into two proof obligations:

1. **TLA+ delivery:** under partitions, reconnects and finite restarts, every node
   eventually receives and merges the causal history of every head that a retry
   marker or a durable pending record owes it.
2. **Lean local state machine:** once those deltas are delivered, local merge/apply
   is order-independent for the modeled CRDT state.

The tools are intentionally complementary. Lean does not model network delivery;
TLA+ does not prove byte/value merge algebra. Fetch contention (slots, permits, the
pending map's bound, the dispatch clock) is `P2PDelivery.tla`'s.

## Source-Grounded Facts

| Fact | Source |
|---|---|
| A sender owes a head to a replicator through a durable retry marker; the retry sweep pushes it and the acknowledgement clears it. | `crates/storage/src/stores/peerstore.rs` `register_scope_marker`, `complete_retry_scope`; `crates/p2p-adapter/src/retry.rs` `run_retry_pass` |
| A receiver persists its pending record before it acknowledges a PushLog. Without an installed pending store the record is in memory only. | `crates/p2p/src/sync/manager/process/pushlog.rs` `process_block_inner` |
| The resync sweep brings durable records back into the in-memory map, at startup and on connect. | `crates/p2p/src/sync/manager/process/pending_dag.rs` `resync_persisted_pending_dags` |
| On connect a node re-drives its own pending records and nothing else: no head rediscovery. | `crates/p2p/src/sync/coordinator/event_handler/mod.rs` `handle_peer_connected` |
| DocSync and BranchableSync discover heads only when the embedder calls them. | `crates/p2p/src/sync/coordinator/event_handler/doc_sync.rs` `handle_doc_sync_reply`, `branchable_sync.rs` `handle_branchable_sync_reply` |
| A fetch emits `DagReady` only when no missing links remain; the merge discharges the root's record. | `crates/p2p/src/sync/coordinator/dag_fetcher.rs` `fetch_dag_attempt`; `crates/p2p/src/sync/manager/process/mod.rs` `discharge_merged` |
| Composite merge merges parent heads before the current block; durable merged sets skip duplicate replay. | `crates/db/src/merge/merge_handler/composite.rs`, `crates/db/src/merge/merge_handler/mod.rs` |

## TLA+ Abstraction

| Spec symbol | Meaning |
|---|---|
| `have[n]` | Durable blockstore contents. |
| `merged[n]` | Durable merged blocks. |
| `owed[n]` | The creator's durable retry markers for `n`. |
| `pendDur[n]` | `n`'s durable pending records. |
| `wanted[n]` | Roots in `n`'s in-memory pending map. A restart clears it; the resync sweep refills it. |
| `connected` | Current directed peer reachability. |
| `Heads` | Accepted heads whose causal history must converge everywhere. |

The key liveness property is:

```tla
CONV_EventualConnectivity == EventuallyConnected => <>[]AllConverged
```

where `EventuallyConnected == <>[]FullyConnected`. Fairness goes to each node's tasks:
the creator's retry sweep toward it, its resync sweep, its fetches and its merge loop.
`Replicated = TRUE` gives the creator a retry marker per node for every head. The red
config turns it off: the heads were only published, so nothing owes them and
reconnecting does not bring them.

## TLA+ Properties And Verdicts

| Run | Verdict | Meaning |
|---|---|---|
| `MC_Conv_Eventual.cfg` | GREEN | With retry markers owing the heads, a node that starts partitioned eventually receives and merges every accepted head history. |
| `MC_Conv_RestartEviction.cfg` | GREEN | Same property with one restart per node. A restart evicts the in-memory map; durable records and markers bring the roots back. |
| `MC_Conv_NoHeadRediscovery.cfg` | RED | Counterexample: peers reconnect, but nothing owes the heads and nothing rediscovers them on connect, so convergence fails. |

Safety invariant:

- `INV_DurableMerge`: `merged` is a subset of durable `have`.

## Conformance

Boundary. `replication.rs` converges under a configured replicator; `partition.rs`
converges after an explicit replicator re-set. A head nobody owes (a gossip drop, a
publish while offline) returns only through an explicit `sync_documents`,
BranchableSync or a replicator re-set, which the embedder owns.

## Lean Local Model

Lean lives under `proofs/lean/` and models local merge/apply behavior:

- LWW is a join over a resolved total-order key derived from decoded priority and
  deterministic value/tombstone tie-break (`crates/crdt/src/field/lww.rs`
  `LwwField::merge`).
- Int64 counter accumulation is wrapping addition, so it is commutative and
  associative, but raw counter merge is not idempotent
  (`crates/crdt/src/field/counter.rs` `CounterField::merge`).
- Counter idempotency is modeled at the durable merged-CID/applied-set layer, which
  is the contract documented above the merge handler
  (`crates/db/src/merge/merge_handler/field_ops.rs` `apply_field_op_once`).
- A document value merges componentwise over field-local state
  (`crates/crdt/src/stored.rs` `StoredDocument::apply`).

The Lean README records `#print axioms` status. No theorem uses `sorry` or custom
axioms. Float32/Float64 counter laws are intentionally not claimed because IEEE-754
addition is not generally associative.

## Result

Convergence holds under eventual connectivity for every head a retry marker or a
durable pending record owes (TLA+), given the local merge state machine is
order-independent for the modeled CRDT components (Lean).
