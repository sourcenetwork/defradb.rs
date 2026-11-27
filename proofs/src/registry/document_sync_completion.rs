use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p-adapter";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // An explicit document sync is complete only once every document it
        // asked for has merged: merge reports repeat (the merge handler and
        // the replication loop both report a merge, and a second peer's copy
        // of a held head is reported too), so it keeps the set of documents
        // reported rather than a count of reports. A peer's answer whose
        // every head of a document is merged here already reports that
        // document, so it does not hold the sync. It asks for documents,
        // not heads, so it does not wait for a second peer's other head. A
        // timeout still ends it with Ok and documents missing.
        family: "Document sync completion",
        name: "INV_CompleteMeansEveryDocMerged / INV_DoneOnceEveryDocReported / INV_NoWaitOnceEveryAnswerHandled: a sync that returns before its timeouts has every asked document merged, one that has read a report of each waits no longer, and a document already merged here does not keep it waiting",
        axis: Tla,
        anchor: "crates/surfaces/p2p-adapter/src/doc_sync/sync.rs sync_documents; crates/surfaces/p2p-adapter/src/replication_events.rs publish_replication_result; crates/database/p2p/src/sync/coordinator/event_handler/doc_sync.rs SyncCoordinator::handle_doc_sync_reply; crates/database/p2p/src/sync/head_provider.rs DocumentHeadProvider::get_document_heads; crates/database/db/src/merge/merge_handler/composite.rs process_composite_delta; crates/database/p2p/src/sync/replication/handlers.rs process_event; crates/surfaces/p2p-adapter/src/iroh.rs IrohP2PAdapter::sync_documents; crates/surfaces/p2p-adapter/src/libp2p.rs P2PAdapter::sync_documents",
        model_ref: "MC_DocSyncCompletion_{Green,Green_OnePeer,Green_Held,Red_CountReports,Red_CountPerPeer,Red_AnyDoc,Red_HeldUnreported,Red_RefusedReport,Red_PeerHeadWitness,Red_OkAfterTimeoutWitness}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/surfaces/p2p-adapter/src/doc_sync/sync.rs a_merge_reported_twice_counts_as_one_document; crates/database/p2p/tests/unit/doc_sync_held.rs a_reply_with_every_head_merged_reports_the_document, a_head_owned_by_another_document_does_not_report_this_document; crates/database/db/tests/merge/merge_handler_tests.rs remote_composite_merge_rejects_immutable_field_change",
    },
];
