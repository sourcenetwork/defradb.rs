use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "storage, db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Schema publication completion",
        name: "VisibleBeforeReply — committed schemas are published before their callers resume",
        axis: Tla,
        anchor: "crates/engine/storage/src/corekv/callbacks.rs OnCommit::OrderedAndWait; crates/database/db/tests/collection/schema_publication.rs",
        model_ref: "MC_SchemaPublication_Green.cfg / MC_SchemaPublication_Red_EarlyReply.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/collection/schema_publication.rs schema_success_waits_for_its_ordered_cache_publication",
    },
    Property {
        family: "Schema publication completion",
        name: "SchemaPublication.preserves",
        axis: Lean,
        anchor: "crates/engine/storage/src/corekv/callbacks.rs OnCommit::OrderedAndWait; crates/database/db/tests/collection/schema_publication.rs",
        model_ref: "SchemaPublication.lean",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/collection/schema_publication.rs schema_success_waits_for_its_ordered_cache_publication",
    },
];
