use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query, datastore, db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Relation-filtered mutation",
        name: "INV_DecisionHoldsAtCommit / INV_AbortOnlyOnRangeWrite",
        axis: Tla,
        anchor: "crates/engine/query/src/runner/mutation_inputs.rs resolve_filter_to_doc_ids; crates/engine/datastore/src/multistore.rs SharedTxn::set_scans_validated, SharedTxn::iterator; crates/database/db/src/read/doc.rs DbDocFetcher::set_scans_validated",
        model_ref: "MC_RelationFilterScan_{Green,Red_Unvalidated}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
