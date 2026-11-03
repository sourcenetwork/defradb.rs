# TwoStoreCounter — design & anchors

Models the #1021 two-store counter materialization clobber: under concurrent
same-document increments, every node ends with an IDENTICAL, fully-converged commit
DAG (identical `_commits` set on every node) yet a DIVERGENT materialized counter
value. The cause is the two-store split — local increments RMW the materialized blob
key while merges touch a separate accumulation key, so the storage layer's
transactional conflict detection never fires between a local write and a same-doc
merge, and a merge's stale reconciled snapshot clobbers an interleaved local
increment. Evidence: the asserting in-suite test
`proofs/tests/behavioral/partition.rs::convergence_concurrent_same_doc_merge_storm`
(and the rest of the `convergence_concurrent_counter_*` family), which converge to the
exact expected value after the fix.

(A SEPARATE, still-open issue — a load-dependent gossip/DAG-delivery under-count where
the DAG itself fails to fully converge under a burst — is probed by the non-asserting,
`#[ignore]`'d `proofs/tests/behavioral/bughunt.rs::bughunt_same_doc_merge_storm`. That
is NOT the #1021 bug modeled here and is NOT fixed by this PR.)

## What it abstracts

| Model element | Code |
|---|---|
| `blob` (materialized value, queries read it; local writes RMW it) | the CBOR document blob written by `db` crate `doc_mutator` (local increments) and by `db-merge` composite materialization |
| `acc` (accumulation store) | the pre-#1021 separate counter key; the current code keeps the value only in the counter's part of the document value |
| `MergeReconcile` (`acc := blob`) | the pre-#1021 reconcile; in the Unified code the part operand reads the stored part when the engine merges it |
| `MergeCommit` (`acc += δ; blob := acc`) | `apply_field_op_once` in `crates/db/src/merge/merge_handler/field_ops.rs`: one blind operand on the part, summed by `CounterField::merge` (`crates/crdt/src/field/counter.rs`) |
| `LocalApply` (`blob += 1`, no lock, no `acc`) | local increment via the `db` crate; in the Split (pre-#1021) abstraction it does NOT acquire the shared per-doc guard |
| `MergeRedeliver` (re-delivery; inline `Dedup` branch) | a delta delivered twice; applied key (`field_ops.rs`) and `is_merged(cid)` merged-set guard (`composite.rs`) (suppresses when `Dedup="On"`) |

## The knobs

The model has two orthogonal axes, each with a CONSTANT knob.

### `StoreMode` — the lost-update / two-store-split axis

- `StoreMode = "Split"` [RED]: the merge commits its stale reconciled snapshot+delta
  with no cross-store conflict check. A local increment that interleaves between a
  merge's reconcile and commit is overwritten → `INV_NoLoss` violated
  (`blob < doneCount`). This is the Rust two-store split: local writes touch the
  blob key, merges touch the accumulation key, so the storage layer's transactional
  conflict detection never fires between them.
- `StoreMode = "Unified"` [GREEN]: the merge's RMW is conflict-checked — it commits
  only if `blob` is unchanged since reconcile, otherwise re-reconciles against the
  fresh value and retries. This is Go's design (counter.go `incrementValue` RMW on a
  single `key.WithValueFlag()`, where a concurrent local write and merge conflict at
  commit and the loser retries) and equivalently a per-doc lock spanning both the
  local-write and merge paths.

### `Dedup` — the double-apply / idempotency axis (added by this PR)

Orthogonal to the store split: even with a unified store, a remote counter delta can
be DELIVERED twice (a field block arriving as its own PushLog head, then again via its
composite — upstream `sourcenetwork/defradb#4935`). Go's `coreblock.ProcessBlock`
applies the delta unconditionally; idempotency rests entirely on the blockstore
merged-set / `is_merged(cid)` guard. The `MergeRedeliver` action models the re-delivery
and branches INLINE on `Dedup` (it is a no-op transition, not a disabled action, so the
re-delivery path is exercised in BOTH settings):

- `Dedup = "On"` [GREEN]: the merged-set guard finds the block already merged and
  SUPPRESSES the re-apply — `MergeRedeliver` is a no-op (`UNCHANGED <<blob, acc>>`).
  This active no-op is what makes GREEN non-vacuous on the double-apply axis.
- `Dedup = "Off"` [RED]: no guard → the +1 is applied a second time (`blob`/`acc` climb)
  with no new increment → `blob > doneCount`, violating `INV_NoDoubleApply`. The Lean
  twin is `CounterReconcile.counter_not_idempotent`.

### `Defer` — the lost-deferral axis (PR #2192)

A counter block can arrive on its own, as a standalone field block, before its
document has a value it can be an operand on: the document is not materialized yet,
or its value is on another schema version. Only the composite that links the block
writes that value. `LateOps` are such increments; `materialized` says whether the
document has a value, and `applied` holds each one's applied key
(`HeadstorePriorityKey` in `field_ops.rs`).

- `StandaloneArrive` is `apply_field_op_once` called standalone (`merge_field_in_txn`
  from `process_block_in_txn` in `batch.rs`). With a value, the increment is an
  operand, applied once under its applied key.
- `CompositeArrive` is the composite merge (`composite_fields.rs` through
  `merge_field_delta_in_txn`). It writes the value and adds the increment unless the
  applied key is claimed. It may arrive again after the increment finished.
- `Defer = "On"` [GREEN]: a standalone block with no value to add to is a terminal
  skip that changes nothing (a no-op transition, so GREEN exercises it).
- `Defer = "Off"` [RED]: the standalone merge claims the applied key, drops the value
  it computed and reports the block merged; the composite then skips the increment →
  `blob < doneCount`, violating `INV_NoLoss`.

## Invariants

The real definitions in `TwoStoreCounter.tla`:

- `INV_NoLoss == blob >= doneCount` — no increment dropped (the **Split** axis breaks
  this: a clobbered local write makes `blob < doneCount`; so does **Defer=Off**: a
  deferred increment the composite skips).
- `INV_NoDoubleApply == blob <= doneCount` — no increment counted twice (the **Dedup=Off**
  axis breaks this: a re-applied delta makes `blob > doneCount`).
- `INV_Exact == blob = doneCount` — the headline: the materialized value equals the
  number of committed increments (oracle independent of the stores). Equivalent to
  `INV_NoLoss /\ INV_NoDoubleApply`.

The GREEN config (`MC_TwoStoreCounter_Green.cfg`, `StoreMode="Unified"`, `Dedup="On"`)
checks `INV_Exact` and holds it over all reachable states. The RED configs each check the
one directional invariant their bug violates.

## Scenarios / configs / verdicts

| Config | Knobs | Invariant | Expected |
|---|---|---|---|
| `MC_TwoStoreCounter_Green` | Unified + On, `Defer="On"`, two `LateOps` | `INV_Exact` | GREEN |
| `MC_TwoStoreCounter_Red_Split` | Split + On | `INV_NoLoss` | RED (`blob < doneCount`) |
| `MC_TwoStoreCounter_Red_DoubleApply` | Unified + Off | `INV_NoDoubleApply` | RED (`blob > doneCount`) |
| `MC_TwoStoreCounter_Red_LostDeferral` | Unified + On, `Defer="Off"` | `INV_NoLoss` | RED (`blob < doneCount`) |

## Fix status

**Landed.** Rust's counter value behaves as one store under concurrency: local
increments and merges use engine conflict validation and counter merge operands, while
reconcile is init-if-absent (PCounter migrate-via-max), so a local write and a same-doc
merge cannot silently overwrite each other's RMW.
Validated by the asserting in-suite test
`partition::convergence_concurrent_same_doc_merge_storm` (driven by
`support::run_counter_storm`) and the rest of the `convergence_concurrent_counter_*`
family, which converge to the exact expected value. The GREEN `Unified` mode here
abstracts that fix as a conflict-checked RMW. Engine OCC remains outside this model.
