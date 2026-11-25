use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db-merge, crdt, db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // `partition::convergence_delete_update_race_preserves_tombstone`
        // partitions a replicated doc, deletes on one side, updates a mutable field
        // on the other, then heals and asserts the merged materialized view stays
        // tombstoned on both replicas. The TLA red leg captures the bad policy:
        // committing an active update from a stale whole-document snapshot clears
        // the delete marker.
        family: "Document materialization status convergence",
        name: "INV_DeletedMarkerAbsorbs — active rematerialization never clears a tombstone",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/composite_persist.rs handle_deletion / persist_merged_document; crates/foundation/defra-core/src/block_delta.rs CompositeDeltaPayload status",
        model_ref: "MC_DocumentMaterialization_Green.cfg; DefraConvergence.DocumentMaterialization.delete_active_age_converge",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/partition.rs convergence_delete_update_race_preserves_tombstone",
    },
];
