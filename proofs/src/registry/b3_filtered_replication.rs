use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p, db-merge";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "B3 filtered replication",
        name: "INV_DagComplete — filtered push still converges per-document",
        axis: Tla,
        anchor: "crates/database/p2p filtered replication; crates/database/db (merge)",
        model_ref: "MC_S4_ModelB.cfg / M1Convergence.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/replication.rs two_node_replication_converges",
    },
    Property {
        // A write pushes each document's heads to every replicator that should
        // get the document, and a `@branchable` collection's heads only to
        // unfiltered replicators. The acknowledgement read compares at the
        // same scope, through the same AckScope rule, so a filtered
        // replicator's acknowledgements are read per document.
        family: "B3 filtered replication",
        name: "INV_AckedReadsAcked / INV_AckedIsHeld / INV_NeverPushedNeverAcked / INV_FilterRespected: replication acknowledgements are read at the scope they are pushed at",
        axis: Tla,
        anchor: "crates/database/p2p/src/ack_scope.rs AckScope::new; crates/database/p2p/src/replicator.rs ReplicatorInfo::ack_scope; crates/database/p2p/src/sync/coordinator/broadcast.rs SyncCoordinator::push_jobs; crates/database/db/src/merge/replication_ack.rs replication_acknowledged",
        model_ref: "MC_ReplicationAckScope_{Green,Red_MainRule,Red_PushToFiltered}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A replicator added with no collections is stored, listed, and
        // restored on start, as in Go, and receives no pushes until it has a
        // collection (#2702).
        family: "B3 filtered replication",
        name: "INV_ListedAsAdded / INV_NoPushOutsideCollections: a replicator with no collections is listed and pushed nothing",
        axis: Tla,
        anchor: "crates/database/p2p/src/bitswap/registry.rs store_info, ReplicatorRegistry::load_from_infos, remove_peer_collections; crates/database/p2p/src/sync/coordinator/broadcast.rs replicator_in_collection",
        model_ref: "MC_ReplicatorEmptyCollections_{Green,Red_DropOnStore,Red_DropOnLoad,Red_EmptyMeansAll}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Storing or loading a replicator record always leaves its entry, the
        // one with no collections included; an empty entry grants nothing and
        // is picked for no collection's pushes.
        family: "B3 filtered replication",
        name: "ReplicatorEmptyCollections.stored_is_listed / restart_lists_every_record / empty_grants_nothing / empty_gets_no_push / push_only_to_members",
        axis: Lean,
        anchor: "crates/database/p2p/src/bitswap/registry.rs store_info, ReplicatorRegistry::load_from_infos, ReplicatorRegistry::is_any_replicator; crates/database/p2p/src/sync/coordinator/broadcast.rs replicator_in_collection",
        model_ref: "ReplicatorEmptyCollections (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
