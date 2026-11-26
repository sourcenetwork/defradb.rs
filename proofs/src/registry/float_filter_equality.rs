use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query, storage";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // #2965: float equality is exact, both in query filters and in index
        // matching; a tolerance can make `0` match `1e-300`.
        family: "Float filter equality",
        name: "exact_accepts_same / epsilon_accepts_distinct / tolerant_has_rejects",
        axis: Lean,
        anchor: "crates/engine/query/src/mapper/filter/eval/operators.rs values_equal; crates/engine/storage/src/index/matcher/mod.rs values_equal",
        model_ref: "FloatEquality (lake build); MC_FloatEquality_{Green,Red_Epsilon}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/query/src/mapper/filter/eval/operators.rs float_equality_is_exact; crates/engine/storage/src/index/matcher/tests.rs test_float_equality_is_exact; tools/integration-test/tests/testo/query/simple/with_filter/with_eq_float_zero.rs float_zero_equality_is_exact_on_indexed_and_unindexed_fields, float_zero_inequality_is_exact_on_indexed_and_unindexed_fields",
    },
];
