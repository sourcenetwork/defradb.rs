use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Collection schema-version OCC",
        name: "INV_NoStaleSchemaWrite — a writer cannot commit against an obsolete structural version",
        axis: Tla,
        anchor: "crates/database/db/src/txn/mod.rs validate_collection_for_write; crates/database/db/src/write/autocommit/{create,update,delete}.rs; crates/database/db/src/definition structural updates",
        model_ref: "MC_CollectionVersionOCC_Green.cfg / MC_CollectionVersionOCC_Red_NoValidation.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
