use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db-block-builder, storage";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Two writers appending to one collection without seeing each other must
        // both land, leaving two tips for a later merge: that is what makes the
        // head set a CRDT rather than a last-writer-wins cell. The eager-delete
        // strategy makes them write the same key, which regolith refuses at
        // every isolation level, so the RED configuration is the defect. The
        // GREEN one is the per-key design that preceded the head-set
        // operator; the tree reaches the same conclusion by blind merges into
        // one key, and overlapping_appends_both_commit checks it.
        family: "Concurrent collection-head transitions",
        name: "INV_NoWriteConflict — concurrent appends to one collection both commit and form siblings",
        axis: Tla,
        anchor: "crates/database/db/src/block/builder/collection.rs write_collection_block; crates/database/db/src/block/heads.rs publish_collection_block; crates/engine/storage/src/backends/regolith/transaction.rs; crates/database/db/tests/block/heads.rs overlapping_appends_both_commit",
        model_ref: "MC_HeadSet_Green.cfg (GREEN) / MC_HeadSet_Red_EagerDelete.cfg (RED) / MC_HeadSet_Red_PerKeyScan.cfg (RED)",
        tiers: &[Behavioral],
        bound_by: "crates/database/db/tests/block/heads.rs overlapping_appends_both_commit",
    },
    Property {
        // A replicated block is published into its document's head set once:
        // its priority index entry records the publication, so a replay never
        // makes a superseded head live again, and a parent held ahead is read
        // by its own part of the set, so the parent's concurrent publication
        // (and nothing unrelated) refuses that decision. No guard key exists.
        family: "Concurrent collection-head transitions",
        name: "INV_LiveExact / INV_AheadExact / INV_NoUnrelatedAbort — document head publication without guard keys",
        axis: Tla,
        anchor: "crates/database/db/src/block/heads/document.rs publish/was_published; crates/engine/storage/src/backends/regolith/merge/head_set.rs part/touches; crates/database/db/tests/block/document_heads.rs, crates/database/db/tests/merge/head_publication.rs",
        model_ref: "MC_HeadPublication_Green.cfg (GREEN) / MC_HeadPublication_Red_{NoRecord,OwnUnvalidated,NoPartRead,WholeSet}.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/block/document_heads.rs replay_of_a_superseded_parent_cannot_resurrect_it, holding_a_parent_ahead_conflicts_with_its_publication, an_unrelated_publication_does_not_turn_away_a_hold; crates/database/db/tests/merge/head_publication.rs a_replayed_create_after_a_local_update_stays_superseded",
    },
    Property {
        // The algebra the temporal model rests on, for the per-key design:
        // every key a writer writes names that writer, so two writers cannot
        // collide (disjointness), and the eager strategy demonstrably does
        // collide (non-vacuity). The tree keeps none of those keys; it binds to
        // this model only at reads, where head_queries_match_executable_lean
        // compares derivedHeads with live_collection_heads.
        family: "Concurrent collection-head transitions",
        name: "derivedHeads reads as live_collection_heads; derived_writeSets_disjoint / eager_writeSets_overlap / applyDerived_parents_not_head are about the per-key design",
        axis: Lean,
        anchor: "crates/database/db/src/block/heads.rs live_collection_heads, publish_collection_block; crates/database/db/src/block/builder/collection.rs write_collection_block; crates/database/db/tests/block/head_conformance.rs head_queries_match_executable_lean",
        model_ref: "HeadSet.Core (lake build)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/block/head_conformance.rs head_queries_match_executable_lean",
    },
];
