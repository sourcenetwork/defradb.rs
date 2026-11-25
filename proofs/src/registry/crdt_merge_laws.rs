use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "CRDT merge laws",
        // Generic CrdtField core (DefraConvergence.CrdtField): comm+assoc => the merge
        // fold is order-independent (every field). Idempotence is the dividing line:
        // LWW (lwwMerge) is idempotent => a join-semilattice, re-delivery-safe, no dedup
        // (lww_dup_safe); the counter (Int +, including PNCounter negative deltas)
        // is NOT idempotent (counter_not_idempotent) => it must apply each delta
        // exactly once, the algebraic root of the #4935 double-apply. Counter, LWW,
        // and the mixed Counter×LWW product fully instantiate the core (counterCM /
        // lwwCM / mixedCM). PNCounter is the same signed-delta algebra with
        // decrement enabled, covered by singleStore_pncounter_converges and exact
        // live/restart/storm behavioral tests. The mixed product's cross-field
        // materialization hazard is checked by MixedFieldMaterialization.
        name: "CrdtField: comm+assoc => order-independent; PNCounter refines to signed Counter; mixed Counter×LWW inherits dedup",
        axis: Lean,
        anchor: "crates/building-blocks/crdt/src/field/lww.rs LwwField::merge; crates/building-blocks/crdt/src/field/counter.rs CounterField::merge; crates/building-blocks/crdt/src/stored.rs StoredDocument::apply componentwise field merge",
        model_ref: "DefraConvergence.MixedField (lake build); MC_MixedFieldMaterialization_Green.cfg",
        tiers: &[Contract, Behavioral],
        bound_by: "proofs/tests/lean_conformance.rs lean_merge_result_vocab_matches_rust; proofs/tests/behavioral/partition.rs convergence_concurrent_mixed_lww_and_counter_fields_merge, convergence_concurrent_pncounter_signed_deltas_sum",
    },
    Property {
        family: "CRDT merge laws",
        name: "PCounterGuard: the positive counter's op accepts only non-negative values, so its value never decreases under any run of accepted or refused writes whose sum stays inside i64 (an i64 sum wraps, as in Go)",
        axis: Lean,
        anchor: "crates/building-blocks/crdt/src/field/counter.rs CounterField::op; crates/building-blocks/crdt/tests/field_contract.rs only_the_positive_counter_refuses_a_negative_value",
        model_ref: "DefraConvergence.PCounterGuard.run_ge (lake build)",
        tiers: &[Contract],
        bound_by: "crates/building-blocks/crdt/tests/field_contract.rs only_the_positive_counter_refuses_a_negative_value",
    },
];
