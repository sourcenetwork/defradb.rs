# P2PDelivery: one model across the P2P delivery seams

`P2PDelivery.tla` composes the pieces that separate models check one at a time: the
sender's marker registrations and retry markers, the receiver's admission, its durable and
in-memory pending records, the fetch slot pool and its permits, the resync sweep, the
hub's merge-and-forward, startup order, and how connections come up. Each separate
model collapses its seam into one atomic step or a scalar version and grants fairness
to each root. Here those seams are separate steps a crash can fall between, heads form
a DAG with siblings, and fairness goes only to tasks, so a root reaches its receiver
only if the scheduling policy gets it there.

The GREEN configurations check a **target contract**, not main. Every RED is main's
behaviour: main breaks the contract on all nine knobs, and the gents rc1 pin
(`b50c667e9`) fixes one of them (`ConnMode`).

Code basis: `origin/main` at `d23a8cf47`. Line numbers below are for that commit.

## Promises

| Property | Plain statement | On main |
|---|---|---|
| `INV_ObligationOwned` | Every head merged on a sender stays owed over each of its edges: a marker, a registration still running, or a durable record on the sender whose DAG brings it, or the receiver has merged it or holds a durable record whose DAG brings it. Ancestry decides on both sides. | Violated: F1, F2, F3, F4, F5 |
| `INV_ReverseBorrowServed` | No node sends a request on a connection its peer does not serve. | Violated: F27 (the pin holds it) |
| `LIVE_HeadDelivered` | A deliverable head written on a node is eventually merged on every node its replicator edges lead to, through any hub. | Violated: F8, F9 |

`INV_ObligationOwned` subsumes the ack contracts the two audit specs stated
separately (`INV_AckBackedByAncestry`, `INV_AckedHeadReachable`, `INV_AckBacked`): an
acknowledgement clears the marker, so the only owner left is the receiver, which must
hold the head in `Reach(merged ∪ pendDur)`.

Every owner is state the code holds. `outbox` is the registration still running in
memory: the commit's broadcast task, and on main the forward that runs after
`mark_as_merged`. A crash loses all of it. A sender's own durable record owes the head
downstream because a hub forwards a root when it merges it, and with the forward
before the mark, the record stands until the forward's markers exist.

`INV_Bounded` (slots, permits, the map, the quota and the durable cap hold, and every
resident root has a durable record) is checked in every configuration as a sanity check
of the model's bookkeeping. Every action enforces those bounds by construction and no
knob breaks them, so it is not a registered property.

A head is deliverable unless its author is `Dead` (never connects) or it is in
`Trickle` (its provider sends blocks for ever and never finishes the DAG). Those two
classes are the environment; the target contract has to deliver everything else
around them.

## Shapes

One module, two shapes, as the workflow report's §5.8 suggests: one GREEN per shape
keeps every TLC run well under the 60 s rule. Both GREENs use the same knob values,
and each RED flips exactly one knob of its shape's GREEN.

| Shape | Module | What it holds | GREEN size |
|---|---|---|---|
| Receiver | `MC_P2PDelivery_Receiver.tla` | Hub `h` just restarted with durable records for `x` (from live `a`), `t` (from trickling `c`) and `u` (from dead `d`); backlog `{u, x}` fits the 2-place map, `{u, t, x}` does not. One fetch slot, two global permits, one per provider. Advisory pull fetches from `c`, merge-channel storms, two local continuations, one dropped connection. | 4,728 states, ~4 s |
| E2E | `MC_P2PDelivery_E2E.tla` | `a → h → b` and `h → a`. Every node has merged `g`; `a` writes `ya`, `h` writes `yb`, siblings at priority 2. One crash of `a` or `h`, one failed registration or forward, every pair starts disconnected. | 12,711 states, ~11 s on one worker |

## Knobs and configurations

Status is the GREEN value's: implemented on main@`d23a8cf47`, on the gents rc1 pin
(`b50c667e9`) only, or proposed, with the open PR that implements it.

| Knob | GREEN (target) | Status | RED (main) | Config | Fails | Counterexample TLC finds |
|---|---|---|---|---|---|---|
| `CoverRule` | `Ancestry` | proposed (#2557) | `Priority`: `(priority, CID)` order, `version <= current` (`head_scope.rs:104`) | `Red_F1_PriorityCover` | `INV_ObligationOwned` | `b` records `ya`; `yb` sorts after it, retires `ya`'s record, `h` clears its marker. |
| `ShutdownOrder` | `DrainFirst`: `Node::shutdown` drains requests, their marker registrations included, before it stops P2P | proposed (#2559) | `StopFirst`: `stop_transport` (`lib.rs:849`) before `network.drain` (`:855`), while a commit's markers are registered by a task spawned after the commit (`txn_broadcaster.rs:68-81`) | `Red_F2_MarkerAfterCommit` | `INV_ObligationOwned` | `a` commits `ya`, stops before the recorder registers it. |
| `ForwardMode` | `BeforeMark`: the forward's markers are registered before `mark_as_merged` discharges the record; a failed forward leaves the root unmarked and its re-drive forwards (`ForwardFail`) | proposed (#2556) | `AfterDischarge`: `mark_as_merged` (`handlers.rs:363`) before the forward (`:379`) | `Red_F3_DischargeBeforeForward` | `INV_ObligationOwned` | `h` merges `ya` (record discharged), crashes before registering the forward. |
| `StartupOrder` | `RestoreFirst` | proposed | `LoopFirst`: loops start at `peer.rs:233-332`, restore at `:341` | `Red_F4_LoopFirst` | `INV_ObligationOwned` | `h` restarts, `a` pushes `ya`, `h` merges it before its replicator list is restored and forwards to nobody. |
| `FailedIntent` | `Kept`: a failed registration is retried for its own peer | proposed | `Dropped`: the error is logged, nobody retries (`txn_broadcaster.rs:103-127`) | `Red_F5_FailedDropped` | `INV_ObligationOwned` | `a`'s one registration, for `h`, fails and `ya` is owed by nobody. |
| `FanoutMode` | `PerPeer`: a failure touches no other peer | proposed (#2553) | `AbortOnFirstFail`: the fan-out returns on the first `None` (`broadcast.rs:219-235`) | `Red_F5_FanoutAbort` | `INV_ObligationOwned` | `h` commits `yb`; its registration for `a` fails and drops `b`'s intent. |
| `ConnMode` | `Served`: every new connection is adopted on both sides | pin only | `HalfRegistered`: `connect_with_cache` does not adopt what it dials; the acceptor caches it (`endpoint_streams.rs:91`) | `Red_F27_HalfRegistered` | `INV_ReverseBorrowServed` | `h` dials `{a, h}` while `a` holds a marker for `h`: `a` pushes on a connection `h` does not serve. |
| `SlotBeforePermit` | `FALSE`: slot and permits are taken together | proposed (#2569) | `TRUE`: slot reserved (`mod.rs:1091`), permits awaited later (`dag_fetcher.rs:232`) | `Red_F8_SlotBeforePermit` | `LIVE_HeadDelivered` | A pull fetch holds `c`'s one permit for ever; `t` takes the slot and parks on it, and stays parked once its dropped connection is back. |
| `DispatchPath` | `Direct` | proposed | `MergeFifo`: `try_reserve` on the merge loop's channel (`mod.rs:1096`) | `Red_F9_MergeFifo` | `LIVE_HeadDelivered` | Storms keep the channel full whenever the clock looks, so it never dispatches. |

Every counterexample was read to confirm it fails through the planted mechanism.

### Receiver rules from #2548

These are implemented on main@`d23a8cf47` and are fixed in the model, not knobs. Their
REDs are main's, in `MC_SyncOwnership_Dispatch` and `MC_SyncOwnership_Memory`; the
composition does not check them a second time. It keeps them so that the target
contract is checked with main's receiver in place.

| Rule | Model | Code | RED on main |
|---|---|---|---|
| A clock dispatch makes one attempt and gives its slot back | `Stall`, `Preempt` → `EndAttempt` | `poll_fetch_dag` (`dag_fetcher.rs:201-205`), `DagFetchContext::finish_clock_attempt` | `MC_SyncOwnership_Red_HeldSlot` |
| Its root is due a backoff after the attempt ends | `EndAttempt(_, _, "back")` | `PendingDagLease::finish_attempt` (`pending_dag.rs:80`) | `MC_SyncOwnership_Red_ClaimTimeRearm` |
| A productive attempt ends at its quantum, due again at once | `Preempt` | `CLOCK_ATTEMPT_QUANTUM` (`dag_retry.rs:42`), `FetchTaskEnd::Preempted` | `MC_SyncOwnership_Red_UnboundedProductive` |
| An acknowledged entry never expires | no `Expire` step | `PendingDag::is_expired` (`pending.rs:102`) | `MC_SyncOwnership_Red_AckedTtl` |
| A root holding a contended place gives it up; the sweep resumes after the root it admitted last | `EndAttempt`, `Contended`, `Pick` | `finish_attempt`, `records_after_resync_cursor` (`pending_dag.rs:1188`) | `MC_SyncOwnership_Red_RetainedOverflow`, `Red_KeyOrderResync` |
| A local continuation ends at its first idle turn | `EndCont` | `consume_pending_merge_continuation` (`pending_dag.rs:679`), `MergeTurn` | `MC_SyncOwnership_Red_ImmortalContinuation` |

#0b and F10 need no RED here. The pending TTL never deletes a durable record:
`evict_expired_pending_dags` (`pending_dag.rs:174`) drops only the in-memory entry.
A durable record leaves only on a merge's discharge (`mod.rs:300`), quarantine, a
head covered by a newer one, or supersession (`pending_dag.rs:1046-1152`).
Without a pending store there is no durable record to lose and the sweep does not run
(`resync_persisted_pending_dags` returns 0), and every runtime installs a store. So
with a store, the pre-#2548 TTL was a liveness case (F10), not a lost obligation, and
on main an acknowledged entry no longer expires at all. A TTL that cancels a
productive fetch needs time and DAGs of more than one block, which this model does
not have.

## Fairness

Fairness goes to tasks, never to a root waiting for a slot or a place:

- the dispatch clock (`Schedule`), one condition per receiver;
- the backoff timer (`BecomeDue`), one condition per receiver. A backoff ends when its
  deadline passes, whatever the clock dispatches meanwhile, and it ends the oldest
  backoff, not a root of the timer's choosing;
- the resync sweep, the merge loop (continuation turns), each sender's recorder and
  startup, and each edge's retry sweep, one condition each;
- a fetch task that already holds its slot, one condition per task. Once a dispatch
  has spawned it, the task runs like any spawned task, and its provider, if it serves,
  sends blocks. This is not dispatch fairness. Who gets a slot is decided by the
  clock's first-due order, the quantum and the rotation alone;
- a request's timeout (`RetireTimedOut`), one condition per pair.

Eventual connectivity is weak fairness on `Reconnect` for every pair without a dead
node. It is assumed, not proven, and registered as Boundary.

Three coarser choices were tried and rejected. Each counterexample was a fairness
artifact, not code behaviour:

- one condition per receiver for every pool step: dead `u` cycling
  Schedule→Stall→BecomeDue satisfied it for ever, so the sweep never ran. In the code
  the sweep is its own periodic task.
- one condition for all fetch tasks of a receiver: with two slots, `u`'s stalls
  satisfied it while `x`'s served fetch never took a step.
- one condition for `Schedule ∨ BecomeDue`: with a 3-place map and one dropped
  connection, `t`'s dispatch and quantum cut satisfied it for ever while `x` waited
  out a backoff that never ended.

No strong fairness anywhere. The separate models' per-root `SF ClaimFetch`,
`SF CompleteFetch` and `WF StartFetch` are exactly what hid #0.

## Abstractions and what they cost

- **Untimed.** There is no clock. A timer (backoff, watchdog, quantum, request
  timeout) is an action that can fire. Bounded-wait properties (`INV_FreshWaitBounded`
  in the workflow report) become starvation lassos under `LIVE_HeadDelivered`.
- **F27 is a latency defect, checked as safety.** On main a half-registered
  connection does not stay: the acceptor's request times out after
  `REQUEST_RESPONSE_TIMEOUT` (30 s) and `retire_timed_out_connection` closes it on both
  sides, so each hit costs a stall of at least 30 s and a redial. With that retire fair,
  `LIVE_HeadDelivered` under `HalfRegistered` still fails, but only through lassos in
  which the 30 s timeout always fires before the dialer's own push. That is the untimed
  model, not the code, so `Red_F27_HalfRegistered` checks `INV_ReverseBorrowServed`
  and no liveness.
- **One block per non-trickle DAG.** A served fetch completes in one `Progress` step,
  so whether a cancelled CAR loses its partial blocks is not modeled.
- **Rotation after one dispatch.** `finish_attempt` gives up a contended place after
  `PENDING_ROTATE_AFTER_DISPATCHES` (3) dispatches without a merge. Here it takes one.
- **Backoffs end in the order they began.** `MC_SyncOwnership_Dispatch` explores every
  order.
- **Admission and acknowledgement are one step.** Losing an ack only delays a marker
  clear, so splitting them removes no unsafe interleaving.
- **A stop under `DrainFirst` never falls between a commit and its markers.** The
  drain lets a write in flight finish, its marker registration included. A kill
  inside that window, or a write that outlives the 30 s drain deadline, still loses
  the head: no durable state covers it, and the model leaves it out.
- **A hub's forward registers every marker or none.** A forward that registers some
  peers and fails on another, or a crash between the forward and the mark, leaves the
  record standing with more markers than `ForwardFail` does, so neither is modeled on
  its own.
- **One pull fetch, from one provider.** The receiver GREEN has two global permits and
  one per provider, so one of them is always out of the pull fetch's reach. The code's
  `DagFetchLimiter::new` gives 4 global and `global - 1` = 3 per provider
  (`mod.rs:330`), and pulls from two providers can take all four. That is the finding
  below.
- **One document, one scope per sender and receiver.** Sender-side coalescing
  (PushCoalescing), push-worker starvation (F24, F25), the retry rung (F22), marker
  growth (F21), gossip and head rediscovery (S-H), configuration changes under an
  obligation (S-G), multi-hub cycles, schema skew, storage faults beyond one failed
  registration, identity rotation and client suspend are outside this model.

## Finding: advisory pull fetches can starve a due durable root

Run on the receiver shape with one global permit instead of two, the target contract
fails `LIVE_HeadDelivered`. Advisory pull fetches (doc sync, branchable sync) take
permits with no slot and no durable obligation. While `x` is due, a pull fetch can
hold the permit, or end and start again before the clock looks, so `x` is never
dispatched. Neither main nor any proposed fix gives a due durable root the permit
ahead of an advisory fetch. That is the scout report's contract item "durable
obligations have priority over advisory fetches".

`MC_P2PDelivery_Finding_AdvisoryPermits.cfg` records the gap until a design closes it.

Robustness probes of the receiver and E2E GREENs, run once and not committed:

| Probe | Result |
|---|---|
| two fetch slots | holds |
| a 3-place map | holds |
| per-sender quota 2 | holds |
| E2E: two crashes | holds |
| E2E: one disconnect | holds |
| E2E: two slots | holds |

Two probes fail, both for the advisory-permit reason: one global permit (committed as
the finding), and a 1-place map that one provider's root fills while it waits for that
provider's permit.

## Code map (origin/main `d23a8cf47`)

| Action | Code |
|---|---|
| `Commit` | `crates/db/src/merge/txn_broadcaster.rs:68` `SyncTxnBroadcaster::submit_update` → `crates/p2p/src/sync/coordinator/broadcast.rs:108` `broadcast_local_update`; the node is returned only after `crates/defra-node/src/assemble.rs:234` awaits `setup_iroh` |
| `Register`, `RegFail` | `broadcast.rs:213` `admit_replicator_push` (returns on the first `None`, `:219-235`); `crates/p2p-adapter/src/retry.rs:89` `record_failure`; `crates/storage/src/stores/peerstore.rs:856` `register_scope_marker`; the failure is logged at `txn_broadcaster.rs:103-127` |
| `Push` (sender) | `crates/p2p-adapter/src/retry.rs:374` `run_retry_pass`; `crates/db/src/merge/push_docs.rs:833` `retry_doc`; ack clear `peerstore.rs:1323` `complete_retry_scope` |
| `Push` (admission) | `crates/p2p/src/sync/manager/process/pushlog.rs:241` `process_block_inner`: merged check, capacity shed, durable covered check (`:445`), `try_insert_pending_dag` (`:470`), durable register (`:568`) |
| `Covered`, `Superseded` | `crates/p2p/src/sync/head_scope.rs:100` `ScopeDecision::of` |
| `Schedule` | `crates/p2p/src/sync/coordinator/mod.rs:1082` `dispatch_due_pending_dag_fetches`: slot reserve (`:1091`, `reserve_pending_dag_fetch` `:604`) before the `try_reserve` on the event channel (`:1096`); `crates/p2p/src/sync/manager/process/pending_dag.rs:722` `due_pending_dag_retries` (oldest due first) |
| `Acquire` | `crates/p2p/src/sync/coordinator/dag_fetcher.rs:232` awaits `mod.rs:363` `DagFetchLimiter::acquire` (FIFO, global and per peer) before the attempt and its quantum start (`:245`) |
| `Progress`, `Stall`, `Preempt` | `dag_fetcher.rs:138` `poll_fetch_dag` (defers before the limiter when no provider is connected, `:189`; one attempt when clock-owned, `:201-205`), `:505` `fetch_dag_attempt`; quantum `crates/p2p/src/sync/coordinator/dag_retry.rs:42` `CLOCK_ATTEMPT_QUANTUM` |
| `EndAttempt` | `crates/p2p/src/sync/coordinator/dag_context.rs:125` `DagFetchContext::finish_clock_attempt` → `pending_dag.rs:80` `PendingDagLease::finish_attempt` (backoff, preempted due at once, rotation after `PENDING_ROTATE_AFTER_DISPATCHES`, `crates/p2p/src/sync/manager/pending.rs:353`) |
| `Progress` (merge, discharge, forward), `ForwardFail` | `crates/p2p/src/sync/replication/handlers.rs:254` `handle_block_received`: `mark_as_merged` (`:363`, discharges the record via `crates/p2p/src/sync/manager/process/mod.rs:300` `discharge_merged`) then `push_to_replicators_with_creator` (`:379`), whose failure is `MergedButBroadcastFailed`; #2556 swaps the two and leaves a failed forward's root unmarked |
| `BecomeDue` | `pending_dag.rs:562` `try_claim_pending_dag_dispatch`; `next_retry_at` passes |
| `Resync` | `pending_dag.rs:955` `resync_persisted_pending_dags`, `:1188` `records_after_resync_cursor` (resumes after the root admitted last, cap and quota skip) |
| `StartCont`, `EndCont` | `pending_dag.rs:609` `continue_merge`, `:679` `consume_pending_merge_continuation` (`MergeTurn`) |
| `PullStart`, `PullEnd` | `crates/p2p/src/sync/coordinator/event_handler/doc_sync.rs:214` `handle_doc_sync_reply` (shares `DagFetchLimiter`, no slot) |
| `Crash`, `RestoreReplicators` | `crates/defra-node/src/lib.rs:824` `Node::shutdown`: `stop_transport` (`:849`) before `network.drain` (`:855`); `crates/p2p-adapter/src/iroh_peer/peer.rs:70` `IrohPeer::assemble`: replication loop (`:233`), resync (`:258`) and retry loop (`:326`) start before `restore_iroh_p2p_state` (`:341`) |
| `Reconnect` | `crates/p2p/src/iroh/endpoint_rpc.rs:392` `connect_with_cache` (caches, does not adopt); `crates/p2p/src/iroh/endpoint_streams.rs:35` `handle_incoming` (the acceptor caches it for its own requests, `:91`) |
| `RetireTimedOut` | `endpoint_rpc.rs:52` `REQUEST_RESPONSE_TIMEOUT`, `:327` `retire_timed_out_connection` (evicts and closes, peer-map clones included) |

## Conformance binding

`INV_ObligationOwned`, `INV_ReverseBorrowServed` and `LIVE_HeadDelivered` are Boundary,
in the registry family "P2P delivery liveness", alongside the two ack-backing models
that had no row (`PendingDagRestart` `INV_AckBacked`, `PushLogAdmission`
`INV_SuccessImpliesRegisteredOrMerged`), whose witnesses are integration tests outside
the conformance binary. Eventual connectivity and the bounded instance are assumed.
Trace validation (below) is the family's Behavioral row: recorded runs of the binary
replay as behaviours of the model.

The scenario leg is pending. It needs defra-harness to pause and throttle a node,
CLI flags for `max_concurrent_dag_fetches` and `pending_dag_ttl_secs`, and HTTP-readable
marker counts and `SyncStatus`. With those it covers: delivery within a bound after a
hub restart under catch-up, an acked head delivered past the pending TTL, sibling heads
both delivered by replay, and an acked drop rediscovered on reconnect. The pending TTL
reads `web_time::Instant`, so a paused tokio clock cannot drive it; those scenarios
need the TTL flag with real time, or an injected clock. Eventual connectivity and the
bounded instance stay Boundary for good.

## Trace validation

`Trace_P2PDelivery.tla` replays a run of the real binary through this model.
A `defra` built with `--features model-trace` logs one JSON event per action
under the `model_trace` log target (`crates/p2p/src/model_trace.rs`); without
the feature every site compiles to nothing. `proofs/src/model_trace.rs` orders
the nodes' events by time, names the nodes and the heads of one document, and
writes the constants and the trace; the replay must take each event as a model
step under the fixed design's knobs, keep `INV_TypeOK`, `INV_Bounded` and
`INV_ObligationOwned`, and end settled: every intent registered, every head
merged where its edges lead. `proofs/README.md` says how to run it and how to
add a scenario.

The node that logs an event is the acting node. From the events the replay
reconstructs every variable: `merged`, `owed`, `outbox`, `pendDur`, `queue`,
`back`, `slot`, `cursor`, `known`, `conn`, `pull`, `flood`, `contLive`,
`contMade`, and the counters `crashes`, `regFails`, `discs` (each fault budget
is the number of its events).

| Event | Parameters | Logged at | Replays as |
|---|---|---|---|
| `Commit` | doc, head, parents | `coordinator/broadcast.rs` `push_document_to_replicators_with_creator`; `merge/txn_broadcaster.rs` `broadcast`, `merge/broadcast_mutator/batch.rs` `register_pending_static` and `merge/broadcast_mutator/mod.rs` delete, for a write without document JSON | `Commit`; `parents` gives the constant `Parent` |
| `Register` | doc, receiver, head | `broadcast.rs` `admit_replicator_push`, once the recorder returned the marker's generation | `Register`; for a hub's forward of a head it merged, no step: the merge step lays that marker (`BeforeMark`) |
| `RegFail` | doc, receiver, head, dropped | `admit_replicator_push`, when the recorder returned none; `dropped` is empty, since each peer's registration stands alone (F5) | `RegFail`; for a hub's forward, `ForwardFail` once per attempt |
| `Admit` | doc, sender, head, outcome, superseded | `manager/process/pushlog.rs` `process_block_inner`: `registered` just before the clock can see the record, `merged` and `covered` at their returns; `nacked` in `process_pushlog_with_provider_evidence` | `registered`: `Push` taking the record branch, retiring only what `Superseded` allows; `merged`/`covered`: `Push`'s ack branch, with the model agreeing the head is merged or covered by ancestry; `nacked`: no step |
| `Ack` | doc, receiver, head | `coordinator/push_worker.rs` `run_push_job` after `report_push_ack`; `merge/push_docs.rs` `retry_doc` per acknowledged head | no step; the marker is already gone |
| `Schedule` | doc, head, `run`/`wait` | `coordinator/mod.rs` `dispatch_due_pending_dag_fetches`, after the claim; the clock took the source peer's permit with the slot (F8), and the mode says whether a global permit was free | `Schedule`, of this head, in this mode |
| `Acquire` | head | `dag_fetcher.rs` `poll_fetch_dag`, a clock dispatch's attempt, after `DagFetchLimiter::acquire` | `Acquire` from `wait`; no step on a slot that runs |
| `Progress` | head | `fetch_dag_attempt`, a partial rooted CAR | `Progress` for a `Trickle` head; no step otherwise |
| `Merge` | doc, head | `replication/handlers.rs` `handle_block_received` after `forward_then_mark` (the forward, then `mark_as_merged`), and the batch path after `mark_batch_as_merged` | the completing `Progress`: merge, the forward's markers, discharge (`BeforeMark`); the hub's `Register` events before it are that forward |
| `Stall` | head, `backoff` | `poll_fetch_dag`, a clock attempt that ended without the DAG, or a deferral before it took its permits | `StallStep` |
| `Preempt` | head | `poll_fetch_dag`, a clock attempt cut at `CLOCK_ATTEMPT_QUANTUM` | `Preempt` |
| `BecomeDue` | head | `dispatch_due_pending_dag_fetches`, for a root dispatched before | `BecomeDue`; no step for a root due already |
| `Resync` | head | `pending_dag.rs` `resync_persisted_pending_dags_inner`, after admission | `Resync` of this head |
| `Expire` | head, record removed | `pending_dag.rs` `evict_expired` and `retry_pending_dag` | no step for an entry with no durable record; an acknowledged root never expires |
| `PullStart`, `PullEnd` | provider | `poll_fetch_dag` around an attempt without a pending lease (doc sync, branchable sync) | `PullStart` from `PullPeer`, `PullEnd` |
| `Crash` | node | `p2p-adapter/src/iroh_peer/peer.rs` `assemble`, at every start | `Crash`; also names the node's peer ID |
| `RestoreReplicators` | | `assemble`, after `restore_iroh_p2p_state` | `RestoreReplicators`, or no step when replicators were restored first |
| `Reconnect` | peer, `full`/`dialer` | `iroh/endpoint_commands.rs` `handle_dial` (`full`), `iroh/endpoint_rpc.rs` `connect_with_cache` for a new connection (`dialer`) | `Reconnect` of a down pair; no step for a pair already up |
| `Disconnect` | peer | `iroh/endpoint_streams.rs`, the last connection to the peer closed | `Disconnect`; no step for a pair already down |

The vocabulary also has `FloodOn`, `FloodOff`, `StartCont` and `EndCont`, which
the code logs nowhere: the clock dispatches on a lane of its own, and local
continuations start only for merges larger than a scenario writes.

Two parts of the replay are looser than the model's own steps, both stated in
`Trace_P2PDelivery.tla`: a traced `Stall` does not need the provider to be
unreachable (the one-block DAG abstracts a window that fails while its provider
serves, which is why `Stall` is split into its guard and `StallStep`), and a
duplicate `Admit` of a head already acknowledged changes nothing.

The fixed design's knobs are the contract the trace is held to. A run of a
behaviour the design rules out fails the replay: a fetch that keeps its slot
across retries fails at its `Stall{hold}` (#0a), a TTL expiry of an
acknowledged root at its `Expire` (#0b, F10), a sibling acknowledged as covered
at its `Admit` (F1). For F1, setting `CoverRule` to main's value in the
generated configuration confirms the run is the known defect.

| Configuration | Run | Verdict |
|---|---|---|
| `MC_P2PDelivery_Trace_TwoNode` | `a → b`: create, then update once b has it | GREEN |
| `MC_P2PDelivery_Trace_HubRelay` | `a → h → b`: the same writes through the hub | GREEN |
| `MC_P2PDelivery_Trace_Red_Unscheduled` | the hub relay run without its `Schedule` events | deadlock at the first `Acquire` |
| `MC_P2PDelivery_Trace_Red_PushBeforeMarker` | a's first marker registered after h admitted the push | deadlock at the `Admit` |
| `MC_P2PDelivery_Trace_Red_ExpireAfterAck` | the TTL drops h1 and its record right after h acknowledged it (#0b) | deadlock at the `Expire` |
| `MC_P2PDelivery_Trace_Red_CoveredByNothing` | h acknowledges h1 as covered by nothing (F1 class) | deadlock at the `Admit` |
| `MC_P2PDelivery_Trace_Red_Unsettled` | the run cut after the hub merged h2 | deadlock past the end |
