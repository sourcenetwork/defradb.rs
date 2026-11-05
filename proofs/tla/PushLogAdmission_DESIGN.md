# PushLogAdmission — hub PushLog admission control (TLA+ design)

Models the hub-side admission decision for incoming PushLog pushes for issue **#1088**
(slice 1, W1) and for sibling heads of one sender scope (audit finding F1), abstracting
`crates/p2p/src/sync/manager/process/pushlog.rs` (`process_block_inner`,
`assert_pushlog_left_durable_state`), the scope rule in
`crates/p2p/src/sync/head_scope.rs` (`ScopeDecision::of`), the in-memory and durable
registrations (`pending_dag.rs` `try_insert_pending_dag`, `pending_store/store.rs`
`register`) and the reply seams in
`crates/p2p/src/sync/coordinator/event_handler/pushlog.rs` (gossip-token and two-stream
paths). Companion to `Replicator.tla`, which models the pusher's resumable retry
lifecycle over a source→target edge; this slice isolates the **hub's reply decision**
when its bounded pending-DAG map is full.

> **Status: the RED config preserves the historical regression.** The hub-side
> backpressure nacks delivered for #592 were removed by `fa4a84f7` ("align iroh replay
> and shutdown with go model", 2026-04-18). The GREEN config fences the restored nack
> behavior; sender redrive now belongs solely to the durable scope-marker ladder.

## Mechanism

A pushed head block whose DAG has missing links must be **registered** in the hub's
bounded pending-DAG map (`SyncConfig::max_pending_dags`) so Bitswap completion is
tracked and the DAG eventually merges. The pusher drives its **persisted retry ladder**
off the `PushLogReply`: a success reply clears the sender's document/collection marker
only after the receiver has merged or durably registered the head. Go's direct
replicator channel has the same shape: its
retry ladder is driven by **error replies** (Go `replicator.go` retryInterval ladder),
so nack-on-overload is the Go-aligned behavior; overload nacks are orthogonal to the
trust/ACP bypasses that `fa4a84f7` was aligning.

In the RED behavior, a full map **drops the registration and replies success**: the
sender clears its marker while the hub holds neither a merge nor a registration.
Silent, permanent divergence (#1088 M1).

## Sibling heads (F1)

The hub keeps one current head per sender scope (announcing peer, collection,
document). Deciding by `(priority, CID)` alone — a lower version acked as covered
without a registration, a higher one retiring the current head's registration — is
sound only when the newer head links the older one. A hub forwards every merged remote
head under its own peer ID, and two heads written concurrently on different peers are
siblings: neither links the other, so fetching the survivor never fetches the lost
one, and the sender has already cleared its marker. `CoverRule = "Priority"` is that
rule.

The code's rule (`CoverRule = "Parent"`): a head retires the scope's current head only
when it links it as a parent (its block's `heads`), and nothing is covered — every
other head, older or a sibling, registers on its own. The receiver cannot see deeper ancestry without the blocks in
between, so a stale ancestor hint costs one registration until the head that links it
merges, and its record is then discharged as merged.

The model gives heads a `Parent` relation, a priority and a sender scope, and a
`CoverRule` knob: `"Parent"` is the code, `"Priority"` the old rule.

## Properties

- `INV_SuccessImpliesRegisteredOrMerged` — **a success PushLogReply implies the pushed
  block is either merged or registered as pending on the hub.** No code path may reply
  success after discarding state. Checked on configurations whose heads have no
  parents (a retired parent is legitimately neither).
- `INV_AckedHeadReachable` — **a success PushLogReply implies the head is reachable
  from what the hub holds**: pending or merged, or an ancestor of a head that is.
- `EventuallyAllMerged` (GREEN) — with overflow nacked (`AT_CAPACITY`), every pushed
  head eventually merges. Fairness is scheduler-level only: the pusher keeps sending
  something, the hub keeps answering something, the fetch pool keeps completing
  something. No head is promised a turn of its own.
- `INV_NackImpliesUnregistered` — **a nack implies the hub holds no registration for the
  block.** The sender parks the hub on a nack and demotes its queued pushes, so a nack of
  a head the hub already owns costs the sender for nothing. The ack guard
  (`assert_pushlog_left_durable_state`) can only break this for a head registered on its
  durable record alone (`process_block_inner`, no in-memory entry): the hub's view of its
  durable records (`PersistedView`) learns of a commit when the store's receipt-ordered
  events go out, and an earlier commit's hook on the same store holds them back past the
  reply. `GuardMode = "Committed"` accepts the record the store committed for the push
  (`Processed::DurablyRegistered`); `GuardMode = "View"` reads only the view (`Publish`
  is when it learns of the record) and nacks a head whose record is durable.

`INV_PeerQuota` — **the pushing peer never holds more durable records than its share**
(`max_persisted_pending_per_peer`, `Quota`). The docs are one peer's pushes. `QuotaMode =
"Store"` checks the share against the durable records in the registration's own
transaction (`pending_store/shares.rs`, `store.rs` `take_new`); `QuotaMode = "View"` checks
it against the view and leaves the store to write the record, so two pushes of the peer
both pass it before the view shows either record.

## Abstractions

- **The heads are one peer's pushes**: its share and the global capacity both bound
  them, and other peers' pushes contend only for the global capacity, so they are left
  out. The pusher is one per-head ack/retry record, and heads of different scopes are
  independent.
- **Merging a head merges its DAG**, so the pending records of its ancestors go with it;
  in the code the receiver clock or the resync sweep finds them merged and discharges
  them.
- **How a push comes to have missing links** (single-head-block live update, M3
  send-timeout truncation) is irrelevant to the reply decision: `HubComplete` models a
  complete arrival, `HubAdmit`/`HubOverflow*` an incomplete one.
- **TTL eviction of an admitted entry is not modeled.** Evicting a registered DAG after
  a success reply is the pending map's residual divergence window (#844, #1088 W2/W3
  follow-ups), orthogonal to the reply decision this slice fixes. With eviction the
  invariant would need reply-time phrasing; without it, the state invariant is exact.

## Configs

| Config | Knobs | Heads | Verdict | Meaning |
|--------|-------|-------|---------|---------|
| `MC_PushLogAdmission_Green.cfg` | `NackOnFull`, `Parent`, `Committed`, `Store` | 3 independent, 1 slot | GREEN | W1 fix: overflow nacks; a nack never hits a registered head; the invariants + liveness hold |
| `MC_PushLogAdmission_Red_SuccessOnFull.cfg` | `SuccessOnFull`, `Parent`, `Committed`, `Store` | 3 independent, 1 slot | RED | overflow acks success → `INV_SuccessImpliesRegisteredOrMerged` violated |
| `MC_PushLogAdmission_Green_Siblings.cfg` | `NackOnFull`, `Parent`, `Committed`, `Store` | base + 2 siblings, one scope, 2 slots | GREEN | F1 fix: each sibling keeps its registration; `INV_AckedHeadReachable` + liveness hold |
| `MC_PushLogAdmission_Red_PriorityCover.cfg` | `NackOnFull`, `Priority`, `Committed`, `Store` | base + 2 siblings, one scope, 2 slots | RED | old rule: sibling 3 retires sibling 2 (or 2 is covered by 3) → `INV_AckedHeadReachable` violated |
| `MC_PushLogAdmission_Red_ViewGuard.cfg` | `NackOnFull`, `Parent`, `View`, `Store` | 3 independent, 1 slot | RED | the guard reads only the view: a head registered on its durable record alone is nacked before the view shows the record (`INV_NackImpliesUnregistered`) |
| `MC_PushLogAdmission_Green_PeerShare.cfg` | `NackOnFull`, `Parent`, `Committed`, `Store` | 3 independent, 2 slots, `Quota=1` | GREEN | the store holds the share in the registration's transaction: invariants + liveness hold |
| `MC_PushLogAdmission_Red_ViewShare.cfg` | `NackOnFull`, `Parent`, `Committed`, `View` | 3 independent, 2 slots, `Quota=1` | RED | the share is checked against the lagging view: two pushes of the peer are registered (`INV_PeerQuota`) |

## Conformance fence

The Rust-side fence for the same invariant is the fan-in integration test
(`tools/integration-test/tests/p2p/`, #1088 W5) plus the coordinator unit tests around
the capacity-nack reply — both assert that no document is success-acked on a pusher
while unmerged and unregistered on the hub. For siblings:
`sibling_heads_from_one_sender_are_both_retained` and
`a_head_retires_only_the_scope_head_it_links` (`pushlog.rs`),
`sibling_heads_from_one_sender_are_both_durable` (`crates/p2p/tests/sync_manager_tests.rs`),
and `a_sibling_neither_retires_nor_is_covered_whatever_its_version` (`head_scope.rs`).
