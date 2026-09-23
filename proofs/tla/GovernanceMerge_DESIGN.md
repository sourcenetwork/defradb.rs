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
  `"CidAndField"` is today.
- `deferred.rs:12-21` — `MAX_DEFERRED_COMPOSITES` (`PendingCap`),
  `MAX_AWAITED_PER_COMPOSITE`, `REDRIVE_BUDGET`. `:101-108` `defer`: a Defer
  whose `awaiting` is empty, or one arriving at capacity, **is not indexed**:
  `Registered(r)` and the `room` check in `Apply`. `:129` `release` is
  `Released(r, e)`; `:82` `enqueue_ready` is the sweep's entry to the same
  `queued` gate. The index is in memory: `IndexDurable = FALSE` is today.
- `judge.rs:32` `judge_governed` maps the verdict (`PushLog`, `DrainReleased`);
  `:194` `index_deferred` files it.
- `merge_handler/dispatch.rs:121` `redrive_deferred` drains the ready queue
  through the verifying merge path: `DrainReleased`.
- `sweep.rs:29-33` `sweep_unmerged_governed` walks `blockstore.get_unmerged()`,
  the store's own to-merge index, and re-drives each governed composite through
  `enqueue_ready` and `redrive_deferred`: `Sweep` with `SweepScope = "Unmerged"`.
  `:124` `SWEEP_INTERVAL` (60 s) and `:129` `run_governance_sweep`, which runs
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
```

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

## The six runs

| Config | Await keys | Sweep scope | Durable index | States | Verdict |
|---|---|---|---|---|---|
| `MC_GovernanceMerge_Today` | CID + field | unmerged | no | 1 792 | GREEN |
| `MC_GovernanceMerge_Green_CidOnly` | CID | unmerged | no | 1 152 | GREEN |
| `MC_GovernanceMerge_Green_Augmented` | CID + field | indexed | yes | 1 560 | GREEN |
| `MC_GovernanceMerge_Red_NoSweep` | CID + field | indexed | no | 1 560 | RED |
| `MC_GovernanceMerge_Red_SweepIndexed` | CID | indexed | yes | 1 632 | RED |
| `MC_GovernanceMerge_Red_Mutant` | teeth check: one silent merge | | | 43 | RED |

## Result 1 — the merge path as coded refines the contract

`MC_GovernanceMerge_Today`: field keys, a sweep over unmerged composites, an
in-memory index. **GREEN**, safety and liveness, 1 792 states.

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
are complete but whose verdict defers has none. Three routes reach the RED: a
Defer naming nothing, the index at capacity, and a restart. The three probe
tests in `crates/db/tests/merge/governance/sweep.rs` are those routes.

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
   and retry exhaustion. Nothing here distinguishes a batch from a sequence of
   pushes.
3. **Two collections.** Everything here is one governed collection. The
   fail-closed rule (a claimed collection with no validator defers everything)
   only becomes interesting with more than one.
4. **The replication policy.** Withholding is modelled as `Deliverable`, a
   property of content, not as a policy decision at a peer. That hook is not in
   this PR.
5. **Disposition.** `Forget` exists in the contract module so the obligation on
   a future primitive is stated; the mechanism module maps it off.
