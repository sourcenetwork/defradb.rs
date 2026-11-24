use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Two behavioral legs: backfill (a write that pre-dates the replicator is
        // delivered) and full resume across a real disconnect (node1 restarted —
        // its state survives on disk and a post-disconnect write arrives after
        // reconnect). The resume leg needs an actual partition: config-level
        // replicator delete does not gate sync between already-connected peers, so
        // it restarts node1 via the harness `restart_node` (fixed upstream in
        // backbone 025d396 to respawn the configured binary, not the debug path).
        family: "Replicator lifecycle (no-loss / resume)",
        name: "INV_NoLoss — reconnect recomputes the target gap, no block dropped",
        axis: Tla,
        anchor: "crates/database/p2p/src/replicator.rs; crates/database/db/src/merge/push_docs.rs push_existing_docs, retry_doc",
        model_ref: "MC_Replicator_Resumable_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/replicator_lifecycle.rs replicator_backfill_no_loss, replicator_resume_across_restart",
    },
];
