# Convergence Proofs

The proof split is intentional:

- TLA+ (`proofs/tla/Convergence.tla`) models distributed delivery: partitions,
  reconnects, head rediscovery, synced-CID eviction, and restart.
- Lean (`proofs/lean/`) models the local merge/apply state machine after delivery.

## Lean Theorems

Run:

```bash
cd proofs/lean
lake build
```

The model proves:

| Theorem | Meaning | Rust source |
|---|---|---|
| `lwwState_merge_comm`, `lwwState_merge_assoc`, `lwwState_merge_idem` | LWW is a join over the deterministic priority/value key. | `crates/crdt/src/field/lww.rs` |
| `word64Add_comm`, `word64Add_assoc` | Int64 counter accumulation is order-independent under wrapping addition. | `crates/crdt/src/field/counter.rs` |
| `word64Add_not_idempotent` | Raw counter merge is not idempotent. Duplicate suppression is not local to `CounterField::merge`. | `crates/crdt/src/field/counter.rs` |
| `appliedSet_merge_*` | The durable merged-CID/applied set is the idempotent layer used for duplicate suppression. | `crates/db/src/merge/merge_handler/field_ops.rs` |
| `composite_merge_*` | Composite local state merges componentwise from the LWW and applied-set components. | `crates/crdt/src/stored.rs` |
| `CrdtField.swap` / `two_converge` / `three_converge` | Generic reusable core (`DefraConvergence/CrdtField.lean`): for ANY commutative-associative merge, the fold is order-independent (replicas applying the same multiset converge). | — (field-agnostic) |
| `CrdtField.dup_absorb` / `nonidem_has_dup_witness` | Idempotence is the dedup dividing line: idempotent ⇒ re-delivery-safe (no dedup); ¬idempotent ⇒ a duplicate changes the result (must dedup). | — |
| `CounterReconcile.counterCM` + `counter_not_idempotent`; `PriorityReconcile.lwwCM` + `lww_idempotent` | Both fields fully instantiate the core: the counter (op-based, `Int +`, **not** idempotent ⇒ must apply each delta once — the algebraic root of the #4935 double-apply) and LWW (state-based join, **idempotent** ⇒ re-delivery-safe). | `crates/crdt/src/field/{counter,lww}.rs` |
| `MixedField.mixedCM` + `mixed_two_converge` / `mixed_three_converge` + `mixed_not_idempotent` | Same-document `Counter × LWW` is a componentwise product of the two #1041 field instances. It converges when both components receive the same operations, but the product is still non-idempotent because the counter component still needs exactly-once dedup. The cross-field stale whole-document materialization hazard is modeled red/green by `MixedFieldMaterialization.tla`. | `crates/crdt/src/stored.rs`; `crates/crdt/src/field/{counter,lww}.rs` |
| `DiskAnnCodes.get_returns_inserted` / `get_in_bounds` / `insert_refuses_other_length` | Resident DiskANN codes in one flat array: with equal lengths and distinct ids each read returns its own code, every slot's slice is inside the array, and a code of another length is refused. | `crates/db/src/index/vector/engine/diskann/cache.rs` |
| `DocumentMaterialization.active_age_after_delete_keeps_deleted` / `delete_active_age_converge` | Document status is a component of materialization: active field rematerialization may update retained bytes, but cannot clear a tombstone. | `crates/db/src/merge/merge_handler/composite_persist.rs`; `crates/defra-core/src/block_delta.rs` |
| `RetrySchedule.new_valid` / `new_accepts_valid` / `cap_mem` / `cap_positive` / `cap_follows_ladder` / `cap_repeats_last` / `default_accepted` | A replicator retry ladder is accepted exactly when it is non-empty and positive, kept as given, and every retry waits one of its rungs: rung `n` for attempt `n`, then the last rung for every attempt past it. | `crates/storage/src/stores/retry_info.rs` |
| `ProviderHealth.rank_perm` / `rank_keeps_healthy_source_first` / `rank_all_healthy` / `next_delay_in_range` / `stale_starts_over` / `forget_keeps_fresh` / `forget_drops_stale` | Ranking a root's fetch providers only reorders them and keeps a healthy announcer first; a fast-failure cool-down stays within 2..30 s, starts over once stale, and pruning drops exactly the stale entries. | `crates/p2p/src/sync/coordinator/provider_health.rs` |
| `Arbitration.roll_le_bound` / `troll_le_bound` / `record_first` / `troll_full` / `iter_monotone_up` / `iter_stays_below` / `decay_geometric` / `troll_decay_up` / `troll_decay_down` / `dither_mean` / `expected_useful_le` | The arbitration estimators stay within what they were fed (no overflow), take their first sample as is, approach a steady input without overshoot, decay geometrically (per sample, or by `2^(-Δt/H)` per unit of time), move by their exact share on average under the dither (so a sub-unit share never freezes them), and the expected time to a useful fetch is at most 64 fetches' cost. | `crates/p2p/src/sync/arbitration/` |
| `Arbitration.bucket_order_any_score` / `bucket_apart_keeps_order` / `inversion_within_bucket` / `same_score_keeps_due_order` / `rearmed_root_goes_behind` | Ordering due roots by (1 s bucket, score, due time) keeps bucket order for any score: a root is passed only by roots due less than one bucket after it (`ACT_AttemptsStartInDueOrder` at bucket resolution), and a root re-armed a backoff of at least a bucket still queues behind (`ACT_EndedAttemptQueuesBehind`). | `crates/p2p/src/sync/arbitration/due_order.rs` |
| `Arbitration.table_is_floor` / `table_decreasing` / `table_wraps` / `weight_le_one` / `weight_full_after_silence` / `weight_zero_gap` / `weight_first_ms` and the side conditions on the constants | The Q16 weight table is exactly `⌊65536 · 2^(-i/64)⌋` and falls step by step, the interpolated Q32 weight never exceeds one, a long silence starts the estimate over, a zero gap (a clock gone backwards) moves nothing and a 1 ms gap already moves something; the side conditions hold on the values `Conformance.lean` binds to the Rust constants. | `crates/p2p/src/sync/arbitration/{mod,ewma}.rs` |

`#print axioms` status checked with Lean 4.18:

Provider ranking (`ProviderHealth.Ranking`):

- `ProviderHealth.rank_perm`, `rank_keeps_healthy_source_first`, `stale_starts_over`, `forget_keeps_fresh`, `forget_drops_stale`: `[propext]`
- `ProviderHealth.rank_all_healthy`, `next_delay_in_range`: `[propext, Classical.choice, Quot.sound]`

DAG fetch arbitration (`Arbitration`, 48 theorems):

- `table_shape`, `table_is_floor` (by `decide +kernel`), `table_decreasing`, `table_wraps`, `weight_le_one`, `weight_first_ms`, `packed_word_fits`, `ppm_fits_value`, `troll_product_fits`, `expected_useful_fits`, `timestamp_lasts`, `tie_bucket_le_backoff_floor`, `useful_floor_is_one_64th`, `occupancy_half_life_ticks`, `lex_keeps_due_order`, `bucket_order_any_score`, `same_score_keeps_due_order`: *(no axioms)*
- `div_mono`, `record_first`, `expected_useful_le`: `[propext]`
- `keep_mono`, `div_add_le`, `share_le`, `share_ge`, `troll_full`, `sum_shift`, `dither_mean`, `decay_step`, `decay_geometric`, `weight_full_after_silence`, `weight_zero_gap`, `bucket_keeps_order`, `inversion_within_quantum`, `rearmed_root_goes_behind`, `bucket_apart_keeps_order`, `inversion_within_bucket`: `[propext, Quot.sound]`
- `roll_between_up`, `roll_between_down`, `roll_le_bound`, `troll_between_up`, `troll_between_down`, `troll_le_bound`, `troll_decay_up`, `troll_decay_down`, `iter_stays_below`, `iter_monotone_up`, `decay_step_up`, `decay_step_down`: `[propext, Classical.choice, Quot.sound]`

Retry ladder (`RetrySchedule.Ladder`):

- `RetrySchedule.new_keeps`, `new_valid`, `new_accepts_valid`: `[propext, Quot.sound]`
- `RetrySchedule.cap_mem`, `cap_positive`: `[propext]`
- `RetrySchedule.cap_follows_ladder`, `cap_repeats_last`: `[propext, Quot.sound]`
- `RetrySchedule.default_accepted`: `[propext, Quot.sound]`

Convergence:

- `lwwState_merge_assoc`: `[propext]`
- `word64Add_assoc`: `[propext, Quot.sound]`
- `word64Add_not_idempotent`: `[Lean.ofReduceBool]`
- `composite_merge_assoc`: `[propext]`
- `reconciled_merge_ge_committed`: `[propext, Quot.sound]`
- `unreconciled_merge_can_clobber`: *(no axioms)*

Generic `CrdtField` core:

- `CrdtField.swap`: *(no axioms)*
- `CrdtField.two_converge`: *(no axioms)*
- `CrdtField.three_converge`: *(no axioms)*
- `CrdtField.dup_absorb`: *(no axioms)*
- `CrdtField.nonidem_has_dup_witness`: `[propext, Classical.choice, Quot.sound]`

Field instantiations:

- `CounterReconcile.counterCM`: `[propext]`
- `CounterReconcile.counter_not_idempotent`: `[propext, Quot.sound]`
- `CounterReconcile.counter_two_converge`: `[propext]`
- `CounterReconcile.counter_three_converge`: `[propext]`
- `PriorityReconcile.lwwCM`: `[propext, Classical.choice, Quot.sound]`
- `PriorityReconcile.lww_idempotent`: `[propext, Classical.choice, Quot.sound]`
- `PriorityReconcile.lww_dup_safe`: `[propext, Classical.choice, Quot.sound]`
- `PriorityReconcile.lww_two_converge`: `[propext, Classical.choice, Quot.sound]`
- `PriorityReconcile.lww_three_converge`: `[propext, Classical.choice, Quot.sound]`
- `MixedField.mixedCM`: `[propext, Classical.choice, Quot.sound]`
- `MixedField.mixed_two_converge`: `[propext, Classical.choice, Quot.sound]`
- `MixedField.mixed_three_converge`: `[propext, Classical.choice, Quot.sound]`
- `MixedField.mixed_not_idempotent`: `[propext, Classical.choice, Quot.sound]`
- `MixedField.mixed_lww_component_dup_safe`: `[propext, Classical.choice, Quot.sound]`
- `DocumentMaterialization.active_age_after_delete_keeps_deleted`: *(no axioms)*
- `DocumentMaterialization.delete_active_age_converge`: *(no axioms)*
- `DiskAnnCodes.get_returns_inserted`: `[propext, Quot.sound]`
- `DiskAnnCodes.get_in_bounds`: `[propext, Quot.sound]`
- `DiskAnnCodes.insert_refuses_other_length`: `[propext]`

`nonidem_has_dup_witness` is the generic core's only classical lemma (it pulls in
`Classical.choice` via `Classical.byContradiction`), and it is the trivial
existential dual of the `Idempotent` definition — it is NOT consumed by any field
convergence or dedup result. The counter results (`counterCM`,
`counter_not_idempotent`, `counter_*_converge`) are fully constructive
(`Classical.choice`-free). The LWW results also depend on `Classical.choice`, but
independently: it enters through the `lwwMerge` case analysis, not through
`nonidem_has_dup_witness`. The mixed-field product inherits that same footprint
from its LWW component.

`Classical.choice`, `propext`, and `Quot.sound` are the three built-in axioms of
Lean 4's core logic, not project-defined axioms. No theorem uses `sorry` or any
custom (project-defined) axiom. Float32/Float64 counter laws are not claimed here
because IEEE-754 addition is not generally associative.

### PNCounter representation decision (#1051)

Rust models PNCounter as the existing signed-delta counter with decrement enabled,
not as separately stored positive and negative components. A product PN history
refines to this representation by mapping increments to positive `Int` deltas and
decrements to negative deltas. `CounterReconcile.counterCM` is therefore the
authoritative algebra, `singleStore_pncounter_converges` covers the negative-delta
case, and `TwoStoreCounter.tla` already covers the shared exactly-once concurrency
obligation. A separate PN algebra or TLA+ model would not correspond to additional
Rust state or behavior.

## Adding a new CRDT field

This PR is the reference template. A new field with a distinct merge algebra
follows the same recipe — the canonical exemplars are the counter
(op-based, `DefraConvergence/CounterReconcile.lean`), LWW (state-based,
`DefraConvergence/PriorityReconcile.lean`), and the first follow-up product
instance (`DefraConvergence/MixedField.lean`):

1. **Define your merge and prove its two laws.** Implement the field's binary
   merge and prove it commutative + associative, then package it:
   `def fooCM : CrdtField.CommMerge V := { merge := …, comm := …, assoc := … }`.
2. **Settle the idempotence axis.** A state-based join (componentwise / lexicographic
   max — like LWW) is idempotent: prove `CrdtField.Idempotent fooCM` and inherit
   `dup_absorb` (re-delivery safe, NO dedup). An op-based field (delta accumulation —
   like the counter) is NOT idempotent: prove `¬ CrdtField.Idempotent fooCM`, which
   makes the per-delta dedup obligation explicit (discharged upstream by the
   merged-set / `is_merged` guard).
3. **Derive convergence by instantiation only.** Get `foo_two_converge` from
   `CrdtField.two_converge fooCM` and `foo_three_converge` from
   `CrdtField.three_converge fooCM`. Both fields expose the IDENTICAL convergence
   set — no field-specific reasoning in these theorems. ("Implement `CommMerge`,
   plug in, inherit the same named theorems" is the copyable invariant.)
4. **Concurrency axis (TLA+).** Copy `proofs/tla/TwoStoreCounter.tla`'s structure:
   one `CONSTANT` knob per hazard, so each interleaving hazard is toggled
   independently and shown RED/GREEN.
5. **Behavioral conformance leg.** If the field is numeric/counter-like, reuse
   `run_counter_storm` (`proofs/tests/support.rs`) — its exact-sum oracle is the
   behavioral template for the COUNTER FAMILY. For a non-numeric field
   (LWW/composite/object) reuse only its connect/replicate/seed/round scaffolding
   and supply the field's own convergence predicate (a last-writer-wins assertion,
   componentwise equality, etc.) in place of the numeric exact-sum oracle.
6. **Code plug-point.** The new field's local-write handling plugs into
   `crates/db/src/write/autocommit/helpers.rs` — `apply_local_counter_deltas`
   (update path) and `init_counter_stores_on_create` (create path) are the counter
   exemplars — reached through the single `write_local_update` / `write_local_create`
   chokepoint that ALL local-write mutators (auto-commit, batch, explicit-txn) call.
   Add the field-CRDT logic THERE (the shared chokepoint), NOT in the individual
   per-mutator files, so the single-store invariant stays enforced by construction.

`DefraConvergence/PriorityReconcile.lean` proves the invariant behind the
same-doc convergence bug this project found and fixed: a field's priority lives
in two stores (the headstore, advanced by local writes *and* merges; the
datastore LWW view, advanced only by merges). `reconciled_merge_ge_committed`
shows that re-seeding the view to the committed priority before merging
guarantees no delta can drop the field below its committed priority (no clobber);
`unreconciled_merge_can_clobber` is the constructive witness that, without the
reconcile, the bug is reachable — the live repro, encoded in Lean. The fix lives
in `crates/db/src/merge/merge_handler/field_state.rs` (`reseed_from_document`).
