use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p-adapter";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // An explicit document sync is complete only once every document it
        // asked for has merged: merge reports repeat (the merge handler and
        // the replication loop both report a merge, and a second peer's copy
        // of a held head is reported too), so it keeps the set of documents
        // reported rather than a count of reports. It asks for documents,
        // not heads, so it does not wait for a second peer's other head. A
        // timeout still ends it with Ok and documents missing.
        family: "Document sync completion",
        name: "INV_CompleteMeansEveryDocMerged / INV_DoneOnceEveryDocReported: a sync that returns before its timeouts has every asked document merged, and one that has read a report of each waits no longer",
        axis: Tla,
        anchor: "crates/surfaces/p2p-adapter/src/doc_sync/sync.rs sync_documents; crates/surfaces/p2p-adapter/src/replication_events.rs publish_replication_result; crates/surfaces/p2p-adapter/src/iroh.rs IrohP2PAdapter::sync_documents; crates/surfaces/p2p-adapter/src/libp2p.rs P2PAdapter::sync_documents",
        model_ref: "MC_DocSyncCompletion_{Green,Green_OnePeer,Red_CountReports,Red_CountPerPeer,Red_AnyDoc,Red_PeerHeadWitness,Red_OkAfterTimeoutWitness}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/surfaces/p2p-adapter/src/doc_sync/sync.rs a_merge_reported_twice_counts_as_one_document",
    },
];
