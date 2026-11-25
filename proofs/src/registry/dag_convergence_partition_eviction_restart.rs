use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p, db-merge, blockstore";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Head rediscovery is embedder-owned. Only sender retry markers and
        // receiver durable pending records re-drive. A head nobody holds an
        // obligation for (a gossip drop, a publish while offline, an
        // acked-then-dropped head) returns only through an explicit
        // sync_documents/BranchableSync or a replicator re-set. replication.rs
        // converges under a configured replicator; partition.rs converges after
        // an explicit replicator re-set.
        family: "DAG convergence (partition / eviction / restart)",
        name: "INV_Converged — every node receives every delta a retry marker or a durable pending record owes it, under eventual connectivity",
        axis: Tla,
        anchor: "crates/database/db (merge) merge handler; crates/engine/blockstore; crates/database/p2p/src/sync/coordinator/event_handler/mod.rs handle_peer_connected; crates/surfaces/p2p-adapter/src/retry.rs run_retry_pass",
        model_ref: "MC_Conv_{Eventual,RestartEviction,NoHeadRediscovery}.cfg",
        tiers: &[Boundary],
        bound_by: "proofs/tests/behavioral/replication.rs two_node_replication_converges; partition.rs convergence_partition_both_directions_merge, convergence_concurrent_same_doc_writes_merge",
    },
    Property {
        // Only sender retry markers and receiver durable pending records
        // re-drive a head. A head nobody holds an obligation for (a gossip
        // drop, a publish while offline, a head acknowledged then dropped)
        // comes back only through an explicit sync_documents or
        // BranchableSync, or a replicator re-set: there is no on-connect
        // anti-entropy.
        family: "DAG convergence (partition / eviction / restart)",
        name: "INV_Converged assumes fair head rediscovery, which the embedder owns",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/event_handler/mod.rs handle_peer_connected; crates/surfaces/p2p-adapter/src/retry.rs run_retry_pass",
        model_ref: "MC_Conv_NoHeadRediscovery.cfg (RED)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
