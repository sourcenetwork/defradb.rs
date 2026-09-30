# GovernanceContract — what a merge validator may assume of the database (TLA+ design)

The plugin interface in `crates/db/src/merge/governance/` lets an application
narrow what the database merges: `MergeValidator::validate` returns Accept,
Reject or Defer for every composite of a governed collection, reading only
through `MergeView`. The doc comments on that trait state a purity contract
and a set of promises the host makes in return. This module is those promises
as a specification, and [`GovernanceMerge.tla`](GovernanceMerge_DESIGN.md) is
the merge path checked to implement it.

**Status: the mechanism as coded refines this contract** (`MC_GovernanceMerge_Today`,
GREEN on safety and liveness). Five of the six RED runs here are liveness-only:
every safety invariant passes while a composite never settles, which is why the
model is written in TLA+ rather than as invariants alone.

## Why a liveness model

A validator's Defer is the interesting verdict. A composite that defers is left
unmerged and must be judged again when what it lacks arrives. Whether that
*eventually* happens is a progress claim, and no state predicate can express it:
the obligation "a deferred composite whose named inputs have all arrived is
queued for re-drive" (`INV_NoStuck`) is vacuously true of a Defer that named
nothing, and four RED runs below pass it while a composite sits forever.

## What is abstracted, and why it is sound

The validator is **not** modelled. It collapses to three constants:

| Constant | Stands for | Why the collapse is sound for liveness |
|---|---|---|
| `Needs[w]` | the inputs a verdict over `w` must read to accept | Liveness fails exactly when a replica never re-judges `w` after the arrival that completes its reads. Which inputs those are is the validator's business; that there *is* such a set is all progress depends on. |
| `Bad` | the composites that reject on present bytes | By the purity contract a reject never rests on absence, so membership is a property of the composite alone, decided on first sight. Rejects are terminal and so not a liveness question. |
| `Deliverable` | the inputs replication will actually deliver | An input outside it is withheld by policy or does not exist. Without this, "some composite never settles" would name both the bug and the correct behaviour. |

`Self` and `Refs` together abstract what `MergeVerdict::Defer { awaiting }` can
name: an unheld input is nameable iff the composite pins it (`Self[w]`) or a
held input references it (`Refs[f]`). A composite that pins nothing and holds
nothing referencing what it needs names nothing, which is the case the sweep
exists for. Under `FieldAwait` a Defer can also name field keys
(`AwaitLogs[w]`); those are filed in the same index (`awaitingLogs`), count
toward its capacity and are lost with it on a restart, exactly as CID keys are.
A field key is a slot, not a standing subscription.

**The abstraction is generous in one direction and that is deliberate.** Every
settleable composite is acceptable once its reads are held, so the model never
defers for a reason the real validator would have. A liveness failure here is
therefore a failure of the *host machinery*, not of the validator.

### What it does not model

- The validator itself. A plugin author models that separately; see
  "Using this as a base model" below.
- The numeric per-drain budget (`REDRIVE_BUDGET`, 256). It is abstracted to
  *whether the leftovers are ever drained*: `DeliverEntry` queues waiters and
  `DrainOne` spends the budget, so dropping `DrainOne`'s fairness models a
  budget that ran out with nothing coming back. The index bound
  (`MAX_DEFERRED_COMPOSITES`) *is* modelled, as `MaxIndexed`, because it changes
  behaviour rather than timing.
- Batches, multiple collections, and the local-write path, which
  `WriteValidator` covers before any block is built.

## The instances

| Module | The case |
|---|---|
| `MC_GovernanceContract_Orphan` | `w1` needs two inputs and pins neither. A held "anchor" names its target "seal", so a replica holding the anchor *first* can name the seal; TLC explores that order too. The gap is a property of the arrival order, not of the composite. |
| `MC_GovernanceContract_Nameable` | the order the defer index was written for: the composite pins its first input, each held input names the next. Each arrival re-drives and names the next gap. |
| `MC_GovernanceContract_TwoDeferred` | two composites each occupying one index slot, for the capacity bound. |
| `MC_GovernanceContract_Withheld` | an input that is never delivered, so one composite is permanently unsettleable *for a content reason*. |

`MC_GovernanceContract_NoDrain` extends `Nameable` with a `Spec` that drops
`DrainOne`'s fairness. Instances that can reject carry a second composite
`w2 ∈ Bad` so the reject properties stay non-vacuous. Two replicas throughout,
so `INV_NoSplit` has something to say.

## The nineteen runs

| Config | Instance | Levers | States | Verdict | Fails |
|---|---|---|---|---|---|
| `MC_GovernanceContract_Green` | Orphan | re-drive + sweep | 1 632 | GREEN | — |
| `MC_GovernanceContract_Green_Nameable` | Nameable | re-drive only | 2 916 | GREEN | — |
| `MC_GovernanceContract_Green_FieldAwait` | Orphan | re-drive + field keys | 576 | GREEN | — |
| `MC_GovernanceContract_Green_Recovery` | Nameable | restart, recovery on | 7 560 | GREEN | — |
| `MC_GovernanceContract_Green_IndexFull` | TwoDeferred | index full, sweep on | 2 304 | GREEN | — |
| `MC_GovernanceContract_Green_Withheld` | Withheld | an input never delivered | 400 | GREEN | — |
| `MC_GovernanceContract_Red_NoRetry` | Orphan | re-drive only | 484 | RED | `L1` `L2` `L3` |
| `MC_GovernanceContract_Red_NoDrain` | NoDrain | queue never drained | 2 916 | RED | `L1` `L2` `L3` |
| `MC_GovernanceContract_Red_NoRecovery` | Nameable | restart, recovery off | 7 560 | RED | `L1` `L2` |
| `MC_GovernanceContract_Red_IndexFull` | TwoDeferred | index full, sweep off | 1 089 | RED | `L1` `L2` |
| `MC_GovernanceContract_Red_Absence` | Nameable | **purity off**: reject on absence | 8 | RED | `INV_RejectIntrinsic` `INV_NoSplit` `ACT_NoFlip` `L3` |
| `MC_GovernanceContract_Green_GC` | Nameable | disposition, floor on | 4 248 | GREEN | — |
| `MC_GovernanceContract_Red_GCNoFloor` | Nameable | disposition, **floor off** | 37 080 | RED | `L1` `L2` `L3` |
| `MC_GovernanceContract_Green_Emit` | Emit | emission: a reject record, a fork receipt found during a defer; no restarts | 7 056 | GREEN |
| `MC_GovernanceContract_Red_EmitLostOnRestart` | Emit | a queued record lost to a restart; the write never re-judged | 28 560 | RED `L_FactsWritten` |
| `MC_GovernanceContract_Red_EmitOnAbsence` | Emit | a defer emits "not approved" | 242 | RED `INV_NoRecordContradictsVerdict` |
| `MC_GovernanceContract_Green_LocalWrite` | LocalWrite | own document hidden, the transaction's documents visible | 32 | GREEN |
| `MC_GovernanceContract_Red_OwnDocCounted` | LocalWrite | the judge counts the document the write creates | 224 | RED `L_LocalAcceptSettlesEverywhere` |
| `MC_GovernanceContract_Red_SnapshotView` | LocalWrite | the judge reads a fresh snapshot | 32 | RED `L_LocalAcceptsWhatPeersWould` |

## Result 1 — the sweep is load-bearing, and no invariant can see why

`MC_GovernanceContract_Red_NoRetry`, 484 states. Core of the counterexample, at
replica `q`:

```
  1. seal and anchor arrive at p         (p settles w1 normally)
  2. seal arrives at q                   held[q] = {seal}
  3. w1 arrives at q                     anchor is missing. It is not pinned by
                                         w1 (Self = {}) and not referenced by
                                         seal (Refs), so the Defer names NOTHING.
  4. anchor arrives at q                 no composite named it -> no waiter
                                         -> w1 is never re-judged at q.
```

`INV_NoStuck` says *a deferred composite whose named inputs have all arrived
must be queued*. Here `awaiting[q][w1] = {}`, so the antecedent is vacuously
satisfied at every state and the invariant passes while `w1` never settles.
**No state predicate can catch this.** The sweep is not a backstop for rare
cases; it is the only thing standing between an ordinary arrival order and a
composite that never merges.

## Result 2 — immutable-field keys close the gap on their own

`MC_GovernanceContract_Green_FieldAwait` is result 1's run with `FieldAwait` on
and the sweep still **off**. It passes, 576 states.

With `WaitKey::ImmutableField` the composite awaits the field keys of the logs
it depends on, so the arrival of *any* entry of either re-drives it, including
the one nothing had named. Field keys are not a convenience over naming CIDs:
they remove the arrival order in which the sweep is the only mechanism that
works. They remove only that one. A field-key waiter is a slot in the same
bounded, in-memory index as a CID waiter, so the two routes of result 3, a full
index and a restart, still fall back on the sweep with field keys on.

## Result 3 — restart, and the bounded index, both fall back on the sweep

Two more liveness-only REDs, every invariant passing:

- `MC_GovernanceContract_Red_NoRecovery` (7 560 states). A restart drops the
  in-memory defer index. Without something re-driving the unmerged composites
  afterwards, an arrival that would have released a waiter finds nothing
  indexed. `Green_Recovery` is the same run with recovery on: green.
- `MC_GovernanceContract_Red_IndexFull` (1 089 states). With `MaxIndexed = 1`
  and two composites deferring, the second is not indexed at all.
  `Green_IndexFull` confirms the sweep covers it and the RED confirms nothing
  else does.

Together with result 1 these are three independent routes to the same place:
**a composite the index cannot hold is reachable only by the sweep.** The index
is an optimisation; the sweep is the correctness argument. That is what
`sweep_unmerged_governed` is for, and why it walks the blockstore's unmerged
set rather than the index.

## Result 4 — withholding is safe, and defers rather than rejects

`MC_GovernanceContract_Green_Withheld`, 400 states. `w1` needs an input that is
never delivered. `L4_UnsettleableDefers` passes: it stays deferred and is never
quarantined, however long it waits. `w2`, which needs only the delivered input,
still settles: a permanently incomplete replica does not stall the composites
it *can* judge. `L1` is stated over `Settleable` composites precisely so that
`w1`'s permanent defer is not counted as a failure.

## Result 5 — why "never reject on absence" is necessary, not merely tidy

`MC_GovernanceContract_Red_Absence`, 8 states, the only safety RED.
`AbsenceReject` makes a verdict reject a composite it cannot fully read. TLC
needs six states:

```
  1-3. est, seal, anchor all arrive at q
  4.   w1 arrives at q      -> Needs ⊆ held  -> Accept
  5.   w1 arrives at p      -> p holds nothing -> Reject
```

`INV_NoSplit` violated: the same composite is merged at one replica and
quarantined at the other. `ACT_NoFlip` fails separately, and that failure is
about re-push: a remote re-push of a quarantined composite is judged afresh.
That is safe only because a reject cannot rest on absence; otherwise the same
composite re-pushed after more inputs arrived comes back Accept and the
quarantine was a lie. The purity clause and judge-afresh-on-re-push cannot be
adopted independently.

## Result 6 — disposition cannot split replicas, whatever policy is plugged in

The database exposes no primitive for a node to forget an unmerged composite or
a held input; a plugin's retention today is bookkeeping only. `Forget(r, e)`
models such a primitive before it exists, so the obligation on it is stated
first. A forgotten input is **not re-delivered**, which is what makes the floor
load-bearing.

`GCFloorOK(r, e)` is the floor: forget an input only once no composite that
needs it is still unsettled here, *including composites that have not arrived
yet*. Expiry approximates it.

- `MC_GovernanceContract_Green_GC` (floor on), 4 248 states: everything passes.
- `MC_GovernanceContract_Red_GCNoFloor` (floor off), 37 080 states: `L1`, `L2`,
  `L3` fail and **every safety invariant passes**, `INV_NoSplit` among them.

A disposition policy below the floor strands composites, but it **cannot make
two replicas disagree**: a reject rests only on present bytes, so a replica
that has forgotten things can only ever defer *more*, never reject differently.
What a bad policy does destroy is evidence a plugin may need for accountability
(in Fefra's case, the proof that a signer forked), which no verdict property
here can see. That belongs to the plugin's own model.

GC is the first mechanism that takes bytes away, and two properties were split
for it: `ACT_Durable` into `ACT_ArrivedDurable` and `ACT_HeldShrinksOnlyByGC`,
and `INV_AcceptJustified` (a `wasJust` ghost, true at the moment of the verdict)
from `INV_AcceptHeldNow` (still re-derivable now, true only when nothing is
forgotten).

## Result 7: emission, and what may never be emitted

A validator's judgement may emit records beside its verdict, and the contract
carries that as `Facts[w]`, what judging `w` emits, each record resting on the
held bytes `FactNeeds[f]`. Two obligations follow, one on the host and one on
the plugin.

**The host writes what a judgement emitted.** `Judge` queues the emittable facts
and `Emit`, fair, writes them; `L_FactsWritten` says a fact found at a replica is
eventually a record there. `Green_Emit` (7 056 states, `MaxRestarts = 0`) is
GREEN. `Red_EmitLostOnRestart` is the same instance with one restart allowed:
a queued record is in memory, the restart clears it, and the write that found
it is settled and never re-judged, so the record is never written. That is the
one-call window between an attempt's commit and `write_emissions` in the code,
stated as a red run rather than hidden; closing it means writing emissions in
the merge transaction.

**A record rests on present bytes.** `EmitOnAbsence` is the purity violation
for emission, as `AbsenceReject` is for verdicts: a defer emits "not approved",
a claim about what the replica lacks. `Red_EmitOnAbsence` produces a replica
holding that record while another has accepted the write, and
`INV_NoRecordContradictsVerdict` fails in a few hundred states. The rule a
plugin author takes from it: emit a reject and its reason, or two signed
entries at one position, which is a fork whatever else arrives; never "not
yet". A record is evidence, never an input: `Records` and `Entries` are disjoint
by construction, which is the model's way of saying no verdict reads one.

## Result 8: the local write path is a fourth entry point

A node writes composites itself, and judges each before its transaction
commits, through a view of what it holds, what the transaction wrote before
this write, and, unless hidden, the document the write itself creates.
`LocalWrite` accepts and commits; `LocalRefuse` is the refusal, which drops
the transaction so a refused write is never durable and never a composite.
`Authored` writes exist only once a node has written them, and what a
transaction wrote exists only once it committed.

`L_LocalAcceptSettlesEverywhere`: a write this node accepted settles on every
replica that receives it. `Red_OwnDocCounted` breaks it with `HideOwnDoc =
FALSE`: a rule that counts the document the write is creating accepts locally,
and every peer, which holds that document only by merging the write, defers on
it forever. The pending view therefore hides the candidate's own document on a
create, and `local_write.rs:299-309` does exactly that.

`L_LocalAcceptsWhatPeersWould`: a write every peer would accept is not refused
here. `Red_SnapshotView` breaks it with `TxnDocsVisible = FALSE`: the judge
reads a fresh snapshot, does not see the grant the same batch wrote a moment
earlier, refuses the note, and the batch with it, so the grant never exists
either. This was the fourth hole in a review of #1835, and the fix is the
transaction's own stores in the view (`view.rs:123`, `with_stores`). The
property is stated as one eventuality over the constant set of locally
acceptable writes rather than a quantified leads-to: TLC's liveness tableau
mishandles a leads-to whose antecedent is constant-valued under `\A`.

## Fairness

`Spec` assumes weak fairness per `(replica, item)` for deliveries, drains and
sweeps. Fairness is per item, not per action, so no single composite can be
starved by a scheduler that keeps choosing another; a coarser `WF_vars(DrainOne)`
would have let TLC "pass" `Red_NoDrain` for the wrong reason.

`Restart` and `RePushQuarantined` are deliberately unfair: they may happen or
not. `Restart` is bounded by `MaxRestarts` so a behaviour cannot restart forever
and call the resulting lack of progress a liveness failure. Input loss is
modelled as `Deliverable` rather than as unfair delivery, so a permanently
missing input is a statement about content that the properties can quantify
over.

## Using this as a base model for your own validator

A plugin author does not re-model the merge path. They instantiate this module:

1. **Fill the constants.** `Needs`, `Self`, `Refs`, `Bad`, `Deliverable` and
   the field keys describe *your* validator's reads on a small case. The
   `MC_GovernanceContract_*` modules are the pattern; two replicas, two or
   three inputs, two composites is enough to exhibit an arrival order.
2. **Check your properties with the host's levers as they are.** Re-drive,
   field keys, the sweep and judge-afresh-on-re-push are on in the code today;
   `Recovery` is not. It keeps `awaiting` across a `Restart`, and the defer
   index is in memory, so a restart loses it and the sweep is what covers the
   restart. `MC_GovernanceMerge_Today` maps `Recovery <- IndexDurable` and sets
   it `FALSE`, so `Recovery = FALSE, RetryClock = TRUE` is the configuration
   the refinement check actually covers. `GovernanceMerge.tla` is what
   guarantees the levers mean what this module says they mean.
3. **Turn one lever off to see what you depend on.** `RetryClock = FALSE`
   shows whether your defers always name something. `AbsenceReject = TRUE`
   shows what happens if your validator ever rejects for a missing input.
   That last one is the promise the interface asks of you; the RED run is
   what breaking it costs everyone.
4. **Describe what you emit.** `Facts[w]` and `FactNeeds[f]` say what a
   judgement of `w` records and what it rests on. Keep `FactNeeds[f]` inside
   the bytes the verdict read: a reject record rests on the composite's own
   bytes (`FactNeeds = {}`), a fork receipt on the two entries. Then check
   `L_FactsWritten` with `MaxRestarts = 0`, and `EmitOnAbsence = TRUE` to see
   the contradiction a "not yet" record produces.
5. **Describe your own writes.** `Authored` is what your node writes, `Batch[w]`
   what the same transaction wrote before it, `Own[w]` the document the write
   creates. `Red_OwnDocCounted` is the mistake to look for in your rule: a
   lookup that would count the write's own document.

### What this does not establish, and where it has to be checked

**The soundness of deferral is an assumption of this model, not a result of
it.** `Judgement` is defined as a function of `held[r]` alone, monotone in it,
with rejects determined by `Bad`. That definition *is* the soundness property,
written as a premise: the verdict depends only on the reads, an accept once
justified stays justified as `held` grows, a reject is content-only, and
**deferring and re-judging later call the same function.** `INV_NoSplit`,
`ACT_NoFlip` and `ACT_AcceptTerminal` are consequences of that premise rather
than evidence for it; they guard against modelling slips.

Stated properly, soundness of deferral is a path-equivalence claim: the verdict
reached through judge → defer → re-drive equals the verdict a single judge
reaches at that state. Here that is true by construction. In the code the paths
differ: a re-drive reloads bytes from the blockstore and rebuilds the carrier
metadata, and a recovery merge re-derives `SignatureStatus`. Whether *your*
validator agrees with itself across those paths is a question about your
validator, and it needs a model in which the verdict is real rather than
abstracted, plus something anchoring that model to your code. The Fefra layer
(sourcenetwork/fefra, `model/` and `prototype/crates/fefra-oracle`) is one
worked example: a state-machine model of the real verdict driven through
delivery, defer, re-drive and restart, cross-checked against the Rust on random
held sets.

## Other limits

- Bounds are tiny: 2 replicas, 2–3 inputs, 2 composites, ≤1 restart. These are
  existence arguments about arrival orders, not coverage claims.
- Symmetry reduction is not used: TLC's symmetry can be unsound with liveness
  checking, and the state spaces are small enough not to need it.
- The corpus originates in sourcenetwork/fefra `proofs/tla`, where a copy is
  kept with Fefra-specific configurations. This is the anchored one.
