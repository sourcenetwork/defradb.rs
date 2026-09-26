# GovernanceMerge — does the merge path deliver what the contract promises? (TLA+ design)

[`GovernanceContract.tla`](GovernanceContract_DESIGN.md) states what a merge
validator may assume of the database. This module is the governed-merge path
as coded, at the granularity the contract cares about, and the check that every
behaviour of the mechanism is one the contract allows:

| Layer | Module | Asks |
|---|---|---|
| Specification | `GovernanceContract.tla` | what must be true for a validator's Defer to work |
| Implementation | `GovernanceMerge.tla` | what the merge path actually does |
| Check | `REFINES_Contract` | is every merge-path behaviour one the contract allows? |

`GovernanceMerge.tla` ends with an `INSTANCE GovernanceContract WITH …` that
expresses the contract's abstract state in the mechanism's concrete state, and
`REFINES_Contract == H!Spec` asserts the implication. Because `H!Spec` carries
the contract's fairness, this is a **safety and liveness** refinement: it fails
if the machinery cannot keep the promises a plugin relies on, not merely if it
corrupts state.

## Source anchors

`crates/db/src/merge/governance/` unless said otherwise.

- `validator.rs:39-55` — `MergeValidator::validate` and its purity contract:
  reads through `MergeView` only, no wall clock, randomness or node identity.
  The `Verdict(r, w)` operator, with `Needs`/`Bad` standing in for the plugin.
- `verdict.rs:7-30` — `MergeVerdict::{Accept, Reject, Defer { awaiting }}`;
  `:49-56` `into_outcome`: Accept merges, Reject is `MergeOutcome::rejected`,
  Defer is a *retryable* (non-terminal) skip carrying its await keys. `Apply`.
- `awaited.rs:9-13, :43-44` — `WaitKey::{Composite, ImmutableField}`. The
  `AwaitKeys` knob: `"Cid"` is the index before field keys existed,
  `"CidAndField"` is today. **Both kinds of key are filed in the one
  `DeferredMerges`**, so a field-key waiter is `pendingLogs[r][w]`, counts
  toward `Registered(r)` and the capacity, and dies with the process like a
  CID waiter. An earlier draft released field-key waiters straight from the
  constant `AwaitLogs`, which let them escape both bounds and made
  `Green_Augmented` green for a reason the code does not have; a reviewer
  caught it, and `Released` now releases only what is filed.
- `deferred.rs:12-21` — `MAX_DEFERRED_COMPOSITES` (`PendingCap`),
  `MAX_AWAITED_PER_COMPOSITE`, `REDRIVE_BUDGET`. `:101-108` `defer`: a Defer
  whose `awaiting` is empty, or one arriving at capacity, **is not indexed**:
  `Registered(r)` and the `room` check in `Apply`. `:129` `release` is
  `Released(r, e)`; `:82` `enqueue_ready` is the sweep's entry to the same
  `queued` gate. The index is in memory: `IndexDurable = FALSE` is today.
- `judge.rs:42` `judge_governed` maps the verdict (`PushLog`, `DrainReleased`)
  and `:85` queues what it emitted; `:103` `judge_definition` likewise;
  `:261` `index_deferred` files a defer.
- `emission.rs:51` `queue_emissions`, one queue per handler; `:76`
  `write_emissions`, the drain, which then re-drives; `:105` `emit_one` builds
  the record and queues it for re-drive. `Queue`, `WriteEmissions`.
- `merge_handler/dispatch.rs:170-181` `merge_block_attempt` wraps the attempt
  and writes emissions once it has returned, succeeded or not; `:59` the batch
  path writes at the end of the batch; `:124` `redrive_deferred` drains the
  ready queue through the verifying merge path: `DrainReleased`.
- `sweep.rs:37` `sweep_unmerged_governed` walks `blockstore.get_unmerged()`,
  the store's own to-merge index, and re-drives each governed composite through
  `enqueue_ready` and `redrive_deferred`: `Sweep` with `SweepScope = "Unmerged"`.
  `:210` `SWEEP_INTERVAL` (60 s) and `:215` `run_governance_sweep`, which runs
  one pass at startup (`Crash` followed by `Sweep`) and then on the interval.
  Spawned from `crates/embedded/src/node_p2p.rs:236`,
  `crates/p2p-adapter/src/iroh_peer/peer.rs:235` and
  `crates/cli/src/commands/start/server_p2p/libp2p.rs:200`.
- `crates/p2p/src/sync/replication/handlers.rs:429` — a non-terminal
  `Skipped` is not marked merged, so the composite stays in the unmerged set
  the sweep walks. `Unmerged(r)`.
- `merge_handler/composite.rs:160` `has_merged_composite`, the in-process
  merged set the sweep consults before re-driving: `merged[r]`.

**What is anchored and what is not.** The structure above is read from the
code; the knobs name real alternatives (`SweepScope = "Indexed"` is the natural
refactor, `IndexDurable = TRUE` a persisted index that does not exist). The
model is not derived from the Rust by a tool. What ties the two together
executably is `crates/db/tests/merge/governance/sweep.rs`: the three probes
there (restart, index at capacity, a Defer naming nothing) are the
`Red_NoSweep` counterexample's three arrival orders as tests, and they fail
without the sweep exactly as the RED run does.

## The refinement mapping

```
held     <- store                merged/quarantined collapse to one verdict:
arrived  <- pushed               hFinal[r][w] = Accept | Reject | None
final    <- hFinal
awaiting <- pending              the defer index IS the contract's awaiting
queue    <- sweepQ               released waiters awaiting re-merge
restarts <- crashes
queuedRecords <- emitQ           pending_emissions, one queue per handler
records  <- records              what write_emissions wrote
```

and the local write path and the absence lever are mapped off: this module is
the replicated path, and the contract's local-write instance checks the
other.

and the contract's levers are read off the mechanism:

```
FieldAwait <- (AwaitKeys = "CidAndField")     WaitKey::ImmutableField
Recovery   <- IndexDurable                    a persisted index (none today)
MaxIndexed <- PendingCap                      MAX_DEFERRED_COMPOSITES
RetryClock <- TRUE                            (deliberately; see below)
GC         <- FALSE                           no disposition primitive yet
```

`RetryClock` maps to `TRUE` unconditionally because the mechanism **has** a
sweep either way. What `SweepScope` changes is *which* composites the sweep
reaches. Mapping it into a constant would hide the difference inside the
abstraction; left as `TRUE`, the difference has to show up as a liveness
failure of the refinement, which is what we want to observe.

`Recovery <- IndexDurable` rather than `TRUE` for the same reason: today the
index dies with the process, and whether that costs anything must be a result,
not an input. It is result 1: with an unmerged-scoped sweep it costs nothing.

## The nine runs

| Config | Await keys | Sweep scope | Durable index | Emission | States | Verdict |
|---|---|---|---|---|---|---|
| `MC_GovernanceMerge_Today` | CID + field | unmerged | no | none in the instance | 7 072 | GREEN |
| `MC_GovernanceMerge_Green_CidOnly` | CID | unmerged | no | none | 3 872 | GREEN |
| `MC_GovernanceMerge_Green_Augmented` | CID + field | indexed | yes | none | 5 824 | GREEN |
| `MC_GovernanceMerge_Red_NoSweep` | CID + field | indexed | no | none | 7 072 | RED |
| `MC_GovernanceMerge_Red_SweepIndexed` | CID | indexed | yes | none | 5 632 | RED |
| `MC_GovernanceMerge_Red_Mutant` | teeth check: one silent merge | | | | 106 | RED |
| `MC_GovernanceMerge_Green_Emit` | CID + field | unmerged | no | queue, drain at return; no crashes | 13 924 | GREEN |
| `MC_GovernanceMerge_Red_DiscardOnError` | CID + field | unmerged | no | a failed attempt clears the queue | 86 | RED |
| `MC_GovernanceMerge_Red_BatchNoDrain` | CID + field | unmerged | no | the batch path never drains | 20 449 | RED |

The state counts of the first six grew when the module gained the emission
queue, the written records and the drain-owed flag; the verdicts and the
counterexamples did not change.

## Result 1 — the merge path as coded refines the contract

`MC_GovernanceMerge_Today`: field keys, a sweep over unmerged composites, an
in-memory index. **GREEN**, safety and liveness, 2 016 states.

The orphan composite that strands in `MC_GovernanceContract_Red_NoRetry` does
not strand here, and the model says why: the sweep iterates unmerged
composites, so it reaches a composite that was never indexed, and the startup
pass reaches everything the crash forgot.

## Result 2 — the code before the sweep did not

`MC_GovernanceMerge_Red_NoSweep` is today's mechanism minus the sweep: field
keys, re-drive from the index only, in-memory index. **RED** on
`REFINES_Contract` and `DL1`, every invariant passing. This is the state of the
tree before `sweep.rs` existed, when `deferred.rs`'s comments promised "the
replication retry clock" as the fallback: that clock sweeps pending-DAG
*registrations* (roots with missing links), and a governed composite whose links
are complete but whose verdict defers has none.

The route this instance exhibits is the **restart**: the orphan is filed under
its field keys, the crash empties the index, and if every input had already
arrived nothing is left to release it. The other two routes to the same place
need a different instance: the index at capacity is
`MC_GovernanceContract_Red_IndexFull`, and a Defer naming nothing is
`Red_SweepIndexed` below, where there are no field keys to file it under. The
three probe tests in `crates/db/tests/merge/governance/sweep.rs` are those three
routes as code.

## Result 3 — the sweep's scope is the load-bearing choice

`MC_GovernanceMerge_Red_SweepIndexed` is `Green_CidOnly` with the sweep
iterating the **index** instead of unmerged composites, and a durable index
thrown in for good measure. **RED**, every invariant passing.

The orphan names no CID, so it is never filed, so an index-scoped sweep never
looks at it. **This is the refactor to refuse:** a sweep written the natural
way, walking the map of things we know we are waiting for, is exactly wrong for
governed collections, because the composites at risk are the ones that could
not be put in that map. It is a plausible change to make on efficiency grounds
without realising what it costs; `sweep_unmerged_governed`'s doc comment says
so, and this run is the reason.

## Result 4 — an unmerged-scoped sweep subsumes everything else

`MC_GovernanceMerge_Green_CidOnly`: no field keys, no durable index, a sweep
over unmerged composites. **GREEN**, 1 152 states. Conversely
`Green_Augmented` shows that field keys plus a *persisted* index would also
have sufficed without the sweep, and `Red_NoSweep` that field keys alone do not.

| Await keys | Sweep scope | Durable index | Refines |
|---|---|---|---|
| CID + field | unmerged | no | ✅ today |
| CID | unmerged | no | ✅ |
| CID + field | indexed | yes | ✅ |
| CID + field | indexed | no | ❌ before the sweep |
| CID | indexed | yes | ❌ |

Read down the table: **an unmerged-scoped sweep is sufficient on its own**, and
it is the only single mechanism in this set that is. Everything else needs a
partner. That is why the sweep, not a persisted index, was the fix: it needs no
new durable state, and its cost follows the unmerged set, which a plugin's
retention bounds.

## Result 5: emission is a queue and a drain, and both halves are load-bearing

`Emitting(r, w)` is what judging `w` emits: the facts in `Facts[w]` whose
`FactNeeds` the replica holds. `Queue` appends them to `emitQ[r]`, which is one
queue per handler, and marks a drain owed; `WriteEmissions` writes the queue
when a drain is owed. Every attempt owes one on return, succeeded or failed:
an emission is a fact about held bytes, not about the attempt's outcome, and a
retried attempt finds the same fact and the same record.

`Red_DiscardOnError` is the design before review: an attempt that fails on a
transaction conflict clears the queue on its way out. The queue is shared, so
a concurrent attempt on another document, which had queued its facts and not
yet drained, loses them; that document merges, is never re-judged, and its
record is never written. RED on the refinement itself, since the contract has
no step that removes a queued record except writing it, and on
`DL_FactsWritten`.

`Red_BatchNoDrain` is `handle_block_batch` judging through its own transaction
and returning without a drain: a batch member's facts sit in the queue until an
unrelated attempt happens to return, and on a node fed only by batches, never.
RED on liveness: the contract's `Emit` is fair, and nothing here enables it.

`Green_Emit` is today's code with `MaxCrashes = 0`. With a crash allowed the
same instance is RED, and honestly so: a fact queued between an attempt's
commit and its drain is lost with the process, and a write already merged or
quarantined is never re-judged. The window is one call wide in the code and
it is real; the contract records it as `Red_EmitLostOnRestart`, and closing
it means writing emissions inside the attempt's transaction, which this PR
does not do.

## Does the refinement check have teeth?

`MC_GovernanceMerge_Red_Mutant` adds one bug to the otherwise-correct mechanism,
a composite merged without the validator being consulted, and asks what catches
it:

| Check | Verdict |
|---|---|
| `DINV_NoSplit` | pass |
| `DINV_RejectIntrinsic` | pass |
| `DINV_CapRespected` | pass |
| `DL1_SettleableSettles` | pass |
| **`REFINES_Contract`** | **FAIL** |

Every invariant stated over the mechanism's own state misses it, and so does
its own liveness property: a silently merged composite has, after all, settled.
Only the comparison against the specification catches it. That is the argument
for keeping the layers separate rather than writing one model with all the
properties in it: a model's own invariants tend to be the ones its author
already believed.

## A note for `PendingDagQuarantine`

That model's `RetryForever` RED says a root re-driven forever without disposing
is a bug. A governed composite whose verdict defers is re-driven by this sweep
at every tick, by design, until its inputs arrive or the plugin's retention
lets it go. The two do not conflict: a verdict Defer is a non-terminal skip on
*absence*, while `PendingDagQuarantine`'s poison root is a terminal rejection on
*content*, and `verdict.rs` keeps the two outcomes distinct. The precondition
is recorded there.

## Not covered

1. **Soundness of deferral.** Assumed by both modules; see the contract note.
2. **The batch path.** `MergeQueue.tla` covers ordered multi-document batches
   and retry exhaustion. Here a batch member differs from a push in one way
   only, whether its return owes a drain (`PushBatchMember`).
3. **Two collections.** Everything here is one governed collection. The
   fail-closed rule (a claimed collection with no validator defers everything)
   only becomes interesting with more than one.
4. **The replication policy.** Withholding is modelled as `Deliverable`, a
   property of content, not as a policy decision at a peer. That hook is not in
   this PR.
5. **Disposition.** `Forget` exists in the contract module so the obligation on
   a future primitive is stated; the mechanism module maps it off.
