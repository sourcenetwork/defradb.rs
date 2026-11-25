use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // The replication filter (#2451): asked between the merge's phases,
        // so a refused document stores nothing; a refusal skips only its own
        // event, for good, and a later commit of a refused document is refused
        // too; a branchable collection is not filtered. A non-genesis commit
        // that finds no live heads is taken to follow a refusal: that is sound
        // only while a refusal is the only thing that leaves a document
        // headless, and a purge or a predecessor skipped non-terminally also do.
        family: "Replication filter",
        name: "INV_RefusedNothingStored / INV_KeptMerged / INV_MergedOnce / INV_BranchableUnfiltered",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/composite_fields.rs process_linked_field_blocks; crates/database/db/src/replication_filter.rs add_field_values; crates/database/db/src/merge/merge_handler/batch.rs try_batch_merge",
        model_ref: "MC_ReplicationFilter_{Green,Green_Branchable,Red_AfterPersist,Red_WholeBatch,Red_Retry,Red_FilterBranchable,Red_Alone}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
