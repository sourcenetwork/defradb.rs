use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "PQ distance four sums",
        name: "four_eq_sum — four running sums over a code's lookups add up to the one running sum, in exact arithmetic",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/quantize/pq.rs ProductQuantizer::distance",
        model_ref: "PqFourSums.four_eq_sum / dropped_tail_loses_lookups",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/quantize_pq.rs the_lookup_table_agrees_with_the_reconstruction",
    },
];
