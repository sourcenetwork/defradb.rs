use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Clock and PushLog fetches single-flight per root in
        // PendingDagFetchTask, separately, and may overlap a reply fetch of the
        // same root. CAR ingest serializes the overlap (StorageContended
        // defers it); it costs bandwidth and a limiter permit, never a head.
        family: "Reply-driven DAG fetch",
        name: "INV_OneFetchPerRoot per claim key (root, replier's sorted explicit-replicator collection set), reply-driven fetches only / INV_NoReplyLost",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/mod.rs DagFetchLimiter::claim_root, RootKey, RootClaim, PendingDagFetchTask; crates/database/p2p/src/sync/coordinator/event_handler/doc_sync.rs handle_doc_sync_reply; crates/database/p2p/src/sync/coordinator/event_handler/branchable_sync.rs handle_branchable_sync_reply; crates/database/p2p/src/sync/coordinator/event_handler/car.rs CarIngestDisposition::StorageContended; crates/database/p2p/src/sync/manager/process/mod.rs try_acquire_car_storage_owners",
        model_ref: "MC_RootSingleFlight_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Pull fetches are best-effort. A reply that cannot claim its root is
        // dropped, and the claimant tries only its replier and up to
        // MAX_PENDING_DAG_ALTERNATE_PROVIDERS connected alternates. Deferred,
        // Busy or no provider ends the pull with no re-drive: sync_documents
        // and BranchableSync callers own completion.
        family: "Reply-driven DAG fetch",
        name: "INV_NoReplyLost assumes the running fetch reaches a holder; a pull it fails is not re-driven",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/dag_context.rs DagFetchContext::with_alternate_providers; crates/database/p2p/src/sync/pending_store/mod.rs MAX_PENDING_DAG_ALTERNATE_PROVIDERS; crates/database/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag",
        model_ref: "MC_RootSingleFlight_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
