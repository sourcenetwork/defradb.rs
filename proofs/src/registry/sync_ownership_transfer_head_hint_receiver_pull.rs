use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p, db-merge, storage, embedded, defra-node";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LocalFetchPermit.released_before_send / release_preserves_queue",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag",
        model_ref: "LocalFetchPermit (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/sync/coordinator/dag_fetcher_tests.rs local_complete_fetch_releases_peer_permit_before_waiting_to_send",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "NoPermitWhileSending / OnePermit — locally complete fetch releases its peer permit before queueing merge",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag; crates/database/p2p/src/sync/coordinator/dag_fetcher_tests.rs local_complete_fetch_releases_peer_permit_before_waiting_to_send",
        model_ref: "MC_LocalFetchPermit_{Green,Red_Hold}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/sync/coordinator/dag_fetcher_tests.rs local_complete_fetch_releases_peer_permit_before_waiting_to_send",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_ReadyImpliesRead / INV_ErrorNeverComplete / INV_AbsentIsMissing / INV_AckedImpliesOwned / INV_RefusedOnlyUnparsed / INV_RegisteredNeverErrors / INV_SweepTotal / INV_ConflictTyped / INV_LocalFaultNotBlamed — a DAG is ready only when its root and every unmerged block were read, a failed walk or root read is the frontier [root], and a local fault is never the provider's failure",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/links.rs find_all_missing_links, frontier_or_unknown; crates/database/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag, fetch_dag_attempt; crates/database/p2p/src/sync/coordinator/event_handler/doc_sync.rs handle_doc_sync_reply; crates/database/p2p/src/sync/coordinator/event_handler/car.rs car_size_limit_disposition; crates/database/p2p/src/sync/manager/process/pushlog.rs process_block_inner; crates/database/p2p/src/sync/manager/process/pending_dag.rs retry_pending_dag, retry_pending_dags_waiting_on, resync_persisted_pending_dags",
        model_ref: "MC_DagReadiness_{Green,Red_SkipNone,Red_DropErr,Red_ErrAsEmpty_*,Red_ErrAbortsSweep,Red_ConflictStringified,Red_RootFaultErrs,Red_CarFaultBlamesPeer}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/tests/unit/dag_readiness.rs a_read_fault_at_final_verification_is_not_complete",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "DagReadiness.ok_nil_implies_all_read / absent_is_missing / failed_fails / unknown_never_empty / error_never_ready / ready_implies_read / retry_ready_implies_read",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/manager/links.rs find_all_missing_links, frontier_or_unknown; crates/database/p2p/src/sync/manager/process/pending_dag.rs retry_pending_dag",
        model_ref: "DagReadiness (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/tests/unit/dag_readiness.rs an_absent_child_is_missing_and_each_child_is_read_once",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Selective CAR capacity: observed completed grants yield to active sends; in-flight scope and quiescent limits are preserved",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/selective_car_access.rs register, reclaim_completed and allows_root",
        model_ref: "MC_SelectiveCarCapacity_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "admitted_bounded / warm_does_not_refuse / reclaim_preserves_active / authorized_scope / expired_completed_refused",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/coordinator/selective_car_access.rs register, reclaim_completed and allows_root",
        model_ref: "SelectiveCarCapacity (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Selected transfer owns its acknowledgements independently of subscriptions; replay stays fenced, scoped and grant-bound",
        axis: Tla,
        anchor: "crates/database/db/src/merge/push_docs.rs push_existing_docs_by_id",
        model_ref: "MC_DocumentPush_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Revoke is API-only: revoke_selected_car_root has no production
        // caller. A grant lasts for the life of the store, through deny/allow,
        // purge and identity rotation, and storage grows with each distinct
        // selected (peer, head). A grant only re-authorizes bytes already sent
        // to that peer.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Selected exact peer/root authority survives cache loss, restart and time until explicit revocation",
        axis: Tla,
        anchor: "crates/engine/storage/src/stores/peerstore/selected_car.rs revoke_selected_car_root; crates/database/p2p/src/sync/coordinator/event_handler/car.rs",
        model_ref: "MC_SelectedCarAuthority_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Selected pushes ignore subscription filters and retry schedules, retain hint ownership, pause and retry the same head on capacity hints within its own first-send deadline, defer a head whose pauses outlast that deadline without stopping the rest, stop scheduling after a stall, disconnect or cancellation, bound late sends and timeouts by the send slots, drain all started tasks and succeed only when every document is acknowledged",
        axis: Tla,
        anchor: "crates/database/db/src/merge/push_docs.rs push_existing_docs_by_id; crates/database/db/src/merge/push_docs/stop.rs SelectedPushStop, stops_selected_transfer; crates/database/db/src/merge/push_docs/selected.rs HeadDeadline; crates/surfaces/p2p-adapter/src/pooled_doc_pusher.rs push_existing_docs_by_id",
        model_ref: "MC_SelectedPushIsolation_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // As above: nothing in production revokes, so revoked_denied is about
        // an API no caller uses.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "revoked_denied / other_preserved / exact_scope",
        axis: Lean,
        anchor: "crates/engine/storage/src/stores/peerstore/selected_car.rs",
        model_ref: "SelectedCarAuthority (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "selection_owns_acknowledgements / absent_replay_is_refused / selected_failure_is_not_deferred / selected_retains_heads / absent_peer_has_no_observation / no_progress_stops / pause_expiry_does_not_stop / expired_head_leaves_the_rest / stopped_admits_nothing / stopped_selection_fails / selected_success_needs_every_acknowledgement",
        axis: Lean,
        anchor: "crates/database/db/src/merge/push_docs.rs; crates/database/db/src/merge/push_docs/stop.rs; crates/database/db/tests/merge/push_docs_selection.rs",
        model_ref: "DocumentPush.lean; selected_document_transfers_match_executed_model",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Rooted authorization: one producer, exact grants, bounded optional retention, fenced eviction and cancellation wakeup",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/car_authorization.rs; car_authorization_registry.rs walk_with_limits; crates/database/p2p/tests/unit/car_authorization_limits.rs",
        model_ref: "MC_RootedAuthorization_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "reserve_bounded / release_restores / replacement_survives / grants_independent_of_retention",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/car_authorization_budget.rs; car_authorization_registry.rs",
        model_ref: "RootedAuthorization (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Replicator installs validate their row, settle before completion, and retain interrupted outcomes",
        axis: Tla,
        anchor: "crates/surfaces/p2p-adapter/src/replicator_install_store.rs; crates/surfaces/p2p-adapter/src/replicator_installs.rs",
        model_ref: "MC_ReplicatorInstall_{Green,Red_*}.cfg; MC_ReplicatorIncarnation_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Deferred replay preserves its install identity, cannot recreate or alter a replaced peer row, flips its own install's row in one transaction whatever same-incarnation rewrite came first, writes retry markers only for its own install, and stops recording failures only when that install is replaced",
        axis: Tla,
        anchor: "crates/database/db/src/merge/push_docs_replay.rs persist_replay_failures; crates/database/db/src/merge/push_docs.rs push_existing_docs_with_config_and_allowlist; crates/engine/storage/src/stores/peerstore.rs update_replicator_for_incarnation, observe_push_head_for_incarnation, record_push_failure_for_incarnation; crates/surfaces/p2p-adapter/src/replicator_status.rs set_persisted_replicator_status_for_incarnation_with_marker_state; crates/surfaces/p2p-adapter/src/replicator_install_store.rs finish",
        model_ref: "MC_ReplicatorIncarnation_{Green,Red_Replay*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "A live write whose marker a newer generation supersedes (a replay's or a second live writer's) is owed by that marker and never reported undurable, and its head stays delivered or marked, because a live ack completes the marker only at its own generation and only while its head is the whole frontier, on the retired branch too",
        axis: Tla,
        anchor: "crates/engine/storage/src/stores/peerstore.rs register_scope_marker, register_scope_marker_at; crates/database/db/src/merge/push_docs.rs push_existing_docs_with_config_and_allowlist; crates/database/p2p/src/sync/coordinator/push_worker.rs run_push_job, is_whole_frontier",
        model_ref: "MC_LiveMarkerGeneration_{Green,Red_Superseded,Red_NoFence,Red_NoHeadFence}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/storage/src/stores/peerstore_tests.rs live_registration_superseded_by_replay_marker_is_owed_by_it; crates/database/p2p/src/sync/coordinator/push_worker_tests.rs retired_older_head_with_newer_generation_leaves_the_marker",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "defer_preserves_incarnation / defer_does_not_resurrect / stale_defer_leaves_replacement / fenced_preserves_incarnation / create_changes_incarnation / create_resurrects / stale_mark_writes_nothing / deleted_mark_writes_nothing / mark_owned / only_replacement_stops / superseded_continues",
        axis: Lean,
        anchor: "crates/database/db/src/merge/push_docs_replay.rs persist_replay_failures; crates/engine/storage/src/stores/peerstore.rs record_push_failure_for_incarnation",
        model_ref: "ReplicatorInstall (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Install completion is identity-fenced and idempotent; stale allocations re-decide without regressing the incarnation",
        axis: Lean,
        anchor: "crates/surfaces/p2p-adapter/src/replicator_install_store.rs finish",
        model_ref: "ReplicatorInstall (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_Released — an early reconciliation return releases its queued continuation slot",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/replication/handlers.rs::process_one_event; crates/database/p2p/src/sync/replication/continuation_tests.rs::already_merged_duplicate_continuation_releases_reservation_without_handler",
        model_ref: "MC_ContinuationSlot_{Green,Red_LateClaim}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_KeyArrivalPromotes / LIVE_PromotedBackoffDrains: a verified key arrival empties every retry backoff into the due queue, and the promoted roots remain available to the clock",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pending_dag.rs promote_pending_retries; crates/database/kms/src/service.rs set_key_arrival_listener",
        model_ref: "MC_PendingKeyArrival_{Green,Red_NoPromotion,Red_LostPromotion}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/p2p/src/sync/manager/process/pending_dag_tests.rs key_arrival_promotes_pending_retries, key_arrival_during_attempt_survives_rearm",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_StoredKeysNotified — a finished or cancelled reply notifies every stored key",
        axis: Tla,
        anchor: "crates/database/kms/src/remote.rs RemoteKeys::fetch_batch, ArrivedKeys::drop",
        model_ref: "MC_KeyBatchNotification_{Green,Red_Cancel}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/kms/src/remote_tests.rs cancelling_a_reply_notifies_keys_stored_before_a_later_put_blocks",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "stored_key_is_notified — flushing includes each stored key",
        axis: Lean,
        anchor: "crates/database/kms/src/remote.rs ArrivedKeys::drop",
        model_ref: "PendingKeyArrival (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/kms/src/remote_tests.rs cancelling_a_reply_notifies_keys_stored_before_a_later_put_blocks",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "arrival_survives_finish — an in-flight key arrival keeps the retry due",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/manager/process/pending_dag.rs promote_pending_retries, PendingDagLease::finish_attempt",
        model_ref: "PendingKeyArrival (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/database/p2p/src/sync/manager/process/pending_dag_tests.rs key_arrival_during_attempt_survives_rearm",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_Capacity / INV_OwnedSlot / INV_NoIdleCapacity / INV_RollbackNeedsCause — only an owner's own generation releases its pending-DAG slot, the release wakes the retry clock, and a full-lane rollback does not",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/mod.rs PendingDagWorkOwner, claim_pending_dag_merge, spawn_pending_dag_fetch, dispatch_due_pending_dag_fetches and run_pending_dag_retry_clock; crates/database/p2p/tests/unit/pending_capacity_wake.rs",
        model_ref: "MC_PendingDagOwner_{Green,Red_NoGeneration,Red_NoWake,Red_RollbackWake}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "owned_slot_released — dropping the continuation owner releases its slot",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/coordinator/mod.rs::claim_pending_dag_merge; crates/database/p2p/src/sync/replication/handlers.rs::process_one_event",
        model_ref: "HistoryTurns.owned_slot_released (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Rejected-page cleanup compares the cursor incarnation before removing it",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/history.rs::reject_history; crates/database/db/tests/merge/history_cursor.rs",
        model_ref: "MC_RetryMarkerGeneration_{Green,Red_StaleAck,Red_Readd}.cfg (incarnation-fenced retirement abstraction)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "History turns commit cursor and writes together; reclaim preserves publications",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/history.rs; history_registry.rs; dispatch.rs; crates/database/db/src/merge/replication.rs; crates/database/db/tests/merge/collection_history_control.rs",
        model_ref: "MC_HistoryTurns_{Green,Red_*}.cfg; MC_HistoryDriver_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Ancestor publication is idempotent; cleanup and collection carriers preserve driver ownership",
        axis: Lean,
        anchor: "crates/database/db/src/merge/merge_handler/history.rs",
        model_ref: "HistoryTurns (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_ObligationConservation / INV_PendingServiceable / INV_PendingHasAuthenticatedProvider / INV_SingleFlight / INV_SingleMergeWriter / INV_SenderMarkersOnly",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync coordinator + pending DAG clock; crates/database/db (merge) push replay; crates/engine/storage peerstore retry markers",
        model_ref: "MC_SyncOwnership_Green.cfg",
        // The deterministic A/B and restart/fanout witnesses live in the
        // integration-test crate, outside this conformance binary's behavioral
        // harness. Keep this binding honest until that harness drives them.
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A transactional write's markers are registered after its commit;
        // the commit returns only once that registration ran, and P2P stops
        // only after the writes the node admitted drained.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "ACT_AckedWriteOwned / INV_ConservedWhileSenderUp — a commit call returns only once its head is owed; while the sender runs every committed head is owed",
        axis: Tla,
        anchor: "crates/database/db/src/merge/txn_broadcaster.rs SyncTxnBroadcaster::submit_update; crates/database/db/src/event/emission.rs register_update_event_callback; crates/entry-points/defra-node/src/lib.rs Node::shutdown",
        model_ref: "MC_SyncOwnership_{Green_MarkerHandoff,Red_UndrainedMarker}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_EventualCurrency / LIVE_EventualReceiverQuiescence — document scope",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync coordinator + pending DAG clock; crates/database/db (merge) document marker rederive",
        model_ref: "MC_SyncOwnership_Green_Liveness_Doc.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_EventualCurrency / LIVE_EventualReceiverQuiescence — collection scope",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync coordinator + pending DAG clock; crates/database/db (merge) collection marker rederive",
        model_ref: "MC_SyncOwnership_Green_Liveness_Collection.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_FetchHasQualifiedProvider / LIVE_EventualCurrency — immutable origin plus durable same-root alternate",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pushlog.rs; crates/database/p2p/src/sync/pending_store/mod.rs; crates/database/p2p/src/sync/coordinator/mod.rs",
        model_ref: "MC_SyncOwnership_Green_Liveness_IrohRelay.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A clock-dispatched fetch gives its slot back after one attempt, its
        // root is due again a backoff after that attempt ends, and an
        // acknowledged head never expires, so after a restart catch-up roots
        // cannot hold every slot while acknowledged heads wait or expire. The
        // model claims only due roots (next_retry_at passed), and a productive
        // attempt gives its slot back at its quantum. Fairness is the
        // scheduler's only: no root has dispatch fairness of its own. The
        // liveness leg assumes a fetch that can complete eventually does
        // (strong fairness on some WComplete): reachability alone does not
        // exclude a provider that answers busy for ever. Dead and Trickle
        // providers' heads are kept but promised no merge, and do not block
        // the others. The clock takes a root's slot together with a fetch
        // permit of its source peer, so no slot is held by a fetch waiting for
        // one while another peer's root could use it; a root whose peer has no
        // permit free, its own fetches' or a pull fetch's, keeps its place.
        // The liveness leg also assumes that pull fetches, which answer this
        // node's own sync requests, at some point stop starting.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "ACT_AttemptsStartInDueOrder / ACT_EndedAttemptQueuesBehind / ACT_AckedRootLeavesOwned / INV_NoSlotWaitsForPermit / LIVE_AckedRootMerged (assumes an eventually serving provider)",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag, FetchTaskEnd; crates/database/p2p/src/sync/coordinator/dag_retry.rs CLOCK_ATTEMPT_QUANTUM; crates/database/p2p/src/sync/replication/handlers.rs handle_dag_needs_fetch; crates/database/p2p/src/sync/coordinator/dag_context.rs DagFetchContext::finish_clock_attempt; crates/database/p2p/src/sync/manager/process/pending_dag.rs try_claim_pending_dag_dispatch, PendingDagLease::finish_attempt, due_pending_dag_retries; crates/database/p2p/src/sync/manager/pending.rs PendingDag::is_expired; crates/database/p2p/src/sync/manager/config.rs SyncConfig::pending_dag_ttl; crates/database/p2p/src/sync/manager/pending.rs PENDING_DAG_TTL; crates/database/p2p/src/sync/coordinator/mod.rs dispatch_due_pending_dag_fetches, DagFetchLimiter::try_reserve_peer, DagFetchLimiter::acquire",
        model_ref: "MC_SyncOwnership_{Green_Dispatch,Green_Dispatch_DeadProvider,Green_Dispatch_Trickle,Green_Dispatch_PeerPermits,Green_Dispatch_PullPermit,Red_HeldSlot,Red_ClaimTimeRearm,Red_AckedTtl,Red_UnboundedProductive,Red_SlotBeforePermit}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The clock hands its dispatches to the merge loop on a lane of their
        // own, one per fetch slot, which the merge loop empties before its
        // channel of merge work: senders waiting for room in that channel
        // are owed every cell it frees, and would otherwise starve the clock.
        // The lane goes first for at most a slot's worth of dispatches in a
        // row, so a finished fetch's DagReady, which it reports through the
        // channel under its slot, still gets its turn. A transport handler,
        // or the merge loop, that completes a root never waits for room; it
        // makes the root due for the clock instead. The storm of merge work
        // is never forced and never made to stop.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_AckedRootMerged under a full merge-loop channel / INV_AdmissionNeverWaitsOnMergeLoop / INV_MergeLoopNeverWaitsOnItself",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/event_lanes.rs DispatchLane, SyncEventReceiver::recv; crates/database/p2p/src/sync/coordinator/mod.rs dispatch_due_pending_dag_fetches; crates/database/p2p/src/sync/manager/process/pending_dag.rs retry_pending_dag",
        model_ref: "MC_SyncOwnership_{Green_Dispatch_Storm,Green_Dispatch_LaneTurns,Red_MergeFifoDispatch,Red_BlockingHandlerSend,Red_LoopWaitsOnItself,Red_LaneAlwaysFirst}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The durable store can hold more acknowledged roots than the
        // in-memory map. A root that had its dispatches gives its entry up
        // while a durable root waits (its record stays), and the resync sweep
        // resumes after the place it admitted at last and gives a peer's
        // places to its roots in turn, so never-serving roots restored first
        // cannot hold the map, or a peer's quota of it, for good, and a peer
        // held at its quota does not hold the others back. A local pull-sync
        // continuation ends at its first turn that does not yield; it is
        // never an acknowledged obligation and is promised no merge.
        // Pull-originated roots have no durable record: a merge that fails, or
        // a yield whose continuation is refused at map or per-peer capacity
        // (reported Failed), is dropped, and its blocks stay unmerged until the
        // next pull or push of the head. Callers own pull completion, and
        // nothing re-drives these at startup.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_DurableRootMerged / LIVE_ContinuationEnds / INV_ContinuationsNeverAcked / INV_MemoryBounded",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pending_dag.rs PendingDagLease::finish_attempt, records_after_resync_cursor, resync_persisted_pending_dags_inner, try_insert_pending_dag, max_pending_dags_per_peer, continue_merge, consume_pending_merge_continuation, MergeTurn; crates/database/p2p/src/sync/manager/pending.rs PendingOrigin, PENDING_ROTATE_AFTER_DISPATCHES; crates/database/p2p/src/sync/replication/handlers.rs handle_block_received, process_merge_batch",
        model_ref: "MC_SyncOwnership_{Green_Memory,Green_Memory_Quota,Red_RetainedOverflow,Red_KeyOrderResync,Red_QuotaPassOver,Red_ImmortalContinuation}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // #2307: the resync sweep sleeps between its 60-second ticks. Whatever
        // frees an in-memory place while a durable root waits for one wakes
        // it: a record leaving, a root giving its entry up after its
        // dispatches, and a local continuation ending. Without the wake a
        // freed place stood empty for a whole tick, and after a write burst,
        // with every place held in turn by roots waiting on another document,
        // the replicas drained a few roots a minute.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_FreedPlaceArmed: a freed in-memory place wakes the resync sweep",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pending_dag.rs PendingDagLease::finish_attempt, MergeTurn; crates/database/p2p/src/sync/manager/process/mod.rs remove_persisted_pending; crates/database/p2p/src/sync/coordinator/mod.rs run_pending_dag_resync",
        model_ref: "MC_SyncOwnership_{Green_Memory,Green_Memory_Quota,Green_TwoTierQuota,Red_RotateNoWake,Red_ContinuationNoWake}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A hub that merges an upstream head forwards it to its own
        // replicators before the merge discharges the upstream pending
        // record; a forward that fails leaves the root unmarked, and any
        // terminal step for a root the database holds and the hub has not
        // marked forwards it, in the same process or after a restart. The
        // witnesses are the p2p crate's forward_tests and the db crate's
        // restart re-drive tests, outside this conformance binary's harness.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_ForwardObligationOwned / LIVE_EventualDownstreamCurrency: the downstream marker is durable before the upstream discharge",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/replication/handlers.rs forward_merged, forward_owed, handle_block_received, process_merge_batch; crates/foundation/defra-core/src/merge.rs MergeOutcome::skip_reason_is_applied; crates/database/p2p/src/sync/replication/forward_tests.rs; crates/database/db/tests/merge/head_publication.rs",
        model_ref: "MC_SyncOwnership_{Green,Green_Liveness_Doc,Red_DischargeBeforeForward,Red_OwedInMemory}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The pusher's outbound head hints wait in one bounded queue drained by
        // a fixed worker pool; one peer holds at most its per-peer cap. A send
        // that times out parks its peer, and peers that have not answered since
        // share one per-peer cap, so any number of dead replicators leave
        // workers for the rest. A peer's queued jobs move to the retry ledger
        // only when a probe (a job started while it was unreachable) times out.
        // Fairness is the scheduler's only; every running send ends by a reply
        // or its send timeout.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_QueueBounded / INV_PermitConservation / INV_PeerLeavesAWorker / INV_UnreachableLeaveAWorker / ACT_DemoteAfterFailedProbe / LIVE_HealthyProgress",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/push_backlog.rs BacklogOwner::enqueue, pop_eligible, park_peer, park_peer_unreachable, peer_answered, retire_idle_cooldowns, take_queued_for_peer; crates/database/p2p/src/sync/coordinator/push_worker.rs spawn_push_workers, run_push_job",
        model_ref: "MC_PushBacklog_{Green,Green_TwoDead,Green_SharedCap,Red_TwoDead,Red_DemoteEveryTimeout,Red_OfflineQueue,Red_NoPeerCap,Red_SpawnPerItem,Red_PermitLeak,Red_RetainHandles}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The owner rechecks connection state before applying delayed offline
        // parks or reconnect notifications, and never clears a send failure's
        // cooldown merely because the peer connected.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_NoStalePark / INV_KeepFailure / INV_ExpiredCanClear",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/push_backlog.rs BacklogOwner::handle, park_peer; crates/database/p2p/src/sync/coordinator/push_worker.rs run_push_job; crates/database/p2p/src/sync/coordinator/event_handler/mod.rs handle_peer_connected",
        model_ref: "MC_OfflineReconnect_{Green,Red_StalePark,Red_StaleConnect,Red_Failure,Red_Expired}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/sync/coordinator/push_worker_tests.rs reconnect_wakes_offline_queue_and_fences_stale_parks, reconnect_preserves_newer_transport_cooldown; crates/database/p2p/src/sync/push_backlog.rs stale_reconnect_does_not_clear_offline_cooldown, reconnect_clears_offline_park_after_expired_capacity_cooldown",
    },
    Property {
        // Admission has two tiers: a head the full map has no entry for is
        // acknowledged on its durable record alone and joins the rotation,
        // instead of being nacked while the store has room, which left the
        // obligation with the sender for as long as the map stayed full.
        // Each peer holds at most its share of the durable records, counted
        // by the store in each registration's own transaction, so one peer's
        // never-serving roots cannot fill the store and leave every other
        // peer nacked. Fairness is per scheduler: each sending peer's retry
        // ledger and the receiver's answers to that peer, never a scope.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_WrittenHeadMerged / INV_DurablePeerQuota: a full map admits on the durable record within each peer's share",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pushlog.rs process_block_inner; crates/database/p2p/src/sync/manager/process/pending_dag.rs durable_share_admits, admits_durable_only, max_persisted_pending_per_peer, try_insert_pending_dag; crates/database/p2p/src/sync/pending_store/store.rs take_new",
        model_ref: "MC_SyncOwnership_{Green_Memory,Green_TwoTierQuota,Red_MemoryOnlyAdmission,Red_NoDurablePeerQuota}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A durable record ends only when its root merges or is quarantined:
        // nothing times one out. A root whose provider never serves (gone or
        // asleep for good, its blocks ACP-filtered, a key never granted, a
        // peer whose identity rotated) keeps its record and its place in its
        // peer's share for good, and that peer's later heads are nacked at
        // the share. The share holds one peer's such roots to a quarter of
        // the durable cap, so the roots of four peers still fill the store,
        // after which every new head is nacked at the cap. Every sweep reads
        // every record; one the map has no place for is skipped before its
        // block is read or its DAG walked.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Durable records have no expiry: never-serving roots hold their peer's share for good",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/pending_store/store.rs register, take_new; crates/database/p2p/src/sync/pending_store/shares.rs; crates/database/p2p/src/sync/manager/process/pending_dag.rs restored_root_fits, resync_persisted_pending_dags_inner",
        model_ref: "MC_SyncOwnership_Green_TwoTierQuota.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A success reply means the head is merged or registered, and a nack
        // means it is not registered: the sender parks the hub on a nack. A
        // head registered on its durable record alone is acknowledged on the
        // record the store committed, since the view of the durable records
        // learns of a commit in receipt order, which an earlier commit's hook
        // holds back past the reply.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_SuccessImpliesRegisteredOrMerged / INV_NackImpliesUnregistered / EventuallyAllMerged: the PushLog ack contract",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pushlog.rs process_pushlog_with_provider_evidence, assert_pushlog_left_durable_state, process_block_inner, Processed; crates/database/p2p/src/sync/pending_store/store.rs register; crates/database/p2p/src/sync/pending_store/view.rs PersistedView::registered",
        model_ref: "MC_PushLogAdmission_{Green,Red_SuccessOnFull,Red_ViewGuard}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The store counts a peer's durable records in the registration's own
        // transaction, so two registrations of the peer that run at once, or
        // that commit while the view of the durable records lags, never both
        // pass its share. A check against the view alone lets them both pass.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_PeerQuota: each peer's share is held in the registration's transaction",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/pending_store/store.rs register, take_new; crates/database/p2p/src/sync/pending_store/shares.rs held, charge, hand_over, give_back; crates/database/p2p/src/sync/manager/process/pending_dag.rs durable_share_admits",
        model_ref: "MC_PushLogAdmission_{Green_PeerShare,Red_ViewShare}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A second PushLog for a CID already being processed joins that
        // processing instead of being nacked: it waits, bounded by a deadline,
        // looks again once the owner lets go, and either acks a block already
        // merged or registered, or takes the guard and processes its own copy.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_SuccessImpliesDurable / INV_OneProcessor / LIVE_EveryJoinEnds: same-CID PushLog joins the in-flight owner",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pushlog.rs process_pushlog_with_provider_evidence; join.rs join_in_flight, needs_nothing_more; crates/database/p2p/src/sync/queue.rs ProcessQueue::try_acquire",
        model_ref: "MC_PushLogJoin_{Green,Red_NoRecheck,Red_NoGuard,Red_NoDeadline}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A gossip announcement refused for rate, saturation or capacity is
        // written as a durable pending record and promoted into a free slot
        // later; gossip has no retry, so dropping it loses the document.
        // Past a per-peer or total byte budget it is dropped and reported.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_HeardIsOwned / INV_HeardIsOwnedOrReported / INV_NothingDropped / LIVE_AllMerged / LIVE_AllSettled: a refused gossip head is kept durably, or reported past its budget",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/event_dispatcher.rs run_event_dispatcher, kept_when_refused; crates/database/p2p/src/sync/coordinator/event_handler/mod.rs handle_transport_event, handle_transport_event_with_admission; crates/database/p2p/src/sync/coordinator/event_handler/gossip.rs handle_gossip_message, defer_refused_gossip; crates/database/p2p/src/sync/manager/process/gossip_deferral.rs defer_gossip_head, drop_gossip_head; crates/database/p2p/src/sync/pending_store/store.rs register; crates/database/p2p/src/sync/manager/process/pending_dag.rs restored_root_may_fit, resync_persisted_pending_dags; crates/database/p2p/src/sync/coordinator/mod.rs run_pending_dag_resync",
        model_ref: "MC_GossipAdmission_{Green,Green_PastCap,Red_SilentShed,Red_Drop,Red_MemoryDefer,Red_NoPromote,Red_LaneDrop}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A committed write's gossip waits for room in a byte-bounded pending
        // set instead of being shed; only replicators hold a durable marker,
        // so a shed announcement never reaches a gossip-only peer. Its
        // coalescing window holds no publish slot. Past a wait deadline it is
        // dropped and reported.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_CommittedIsOwnedOrReported / INV_PendingBounded / INV_WindowHoldsNoSlot / INV_TasksBounded / LIVE_AllSettled: a committed write's gossip is published, or reported past a wait deadline, on one lane task",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/mod.rs spawn_non_authoritative_broadcast_task, publish_slot, acquire_non_authoritative_broadcast_slot; crates/database/p2p/src/sync/coordinator/broadcast_room.rs BroadcastRoom::reserve, BroadcastRoom::run, lane; crates/database/p2p/src/sync/coordinator/broadcast.rs broadcast_local_update_with_creator; crates/database/db/src/merge/broadcast_mutator/broadcast.rs spawn_gossip_or_report, broadcast_with_creator",
        model_ref: "MC_SenderBroadcast_{Green,Red_Shed,Red_SilentExpiry,Red_SlotAcrossWindow,Red_UnboundedPending,Red_TaskPerWrite}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "acquire_exclusive / join_acks_only_durable / join_processes_only_unowned: the CID guard is exclusive and a join acks only a safe block",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/queue.rs ProcessQueue::try_acquire, release_sync; crates/database/p2p/src/sync/manager/process/join.rs join_in_flight; crates/database/p2p/src/sync/coordinator/mod.rs DagFetchLimiter::claim_root, RootClaim::drop",
        model_ref: "SingleFlight.Join (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A success reply ends the pusher's retry record, so every acked head
        // must stay reachable from a pending or merged head. A head retires
        // its scope's current head only when it links it as a parent; no head
        // is acked on another head's strength. Witnessed by
        // pushlog.rs sibling_heads_from_one_sender_are_both_retained and
        // sync_manager_tests.rs sibling_heads_from_one_sender_are_both_durable.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_SuccessImpliesRegisteredOrMerged / INV_AckedHeadReachable / EventuallyAllMerged: overflow nacks, and sibling heads of one sender scope each keep their registration",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/head_scope.rs ScopeDecision::of; crates/database/p2p/src/sync/manager/process/pushlog.rs process_block_inner, assert_pushlog_left_durable_state; crates/database/p2p/src/sync/manager/process/pending_dag.rs try_insert_pending_dag; crates/database/p2p/src/sync/pending_store/store.rs register",
        model_ref: "MC_PushLogAdmission_{Green,Green_Siblings,Red_SuccessOnFull,Red_PriorityCover}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A live ack clears its scope's marker only when the acked head is the
        // scope's whole frontier; with a sibling beside it, the ack is not
        // reported and the retry ladder sends every current head and records
        // them as acked. Witnessed by access_tests.rs
        // live_ack_of_a_sibling_head_does_not_clear_the_scope_marker and
        // db push_docs_common.rs multi_head_doc_reads_acknowledged_after_ladder.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_OneLiveHead / INV_NoStaleRetry / INV_NewestRetained / INV_ClearedScopeDelivered / INV_AckRecordCurrent: latest-head coalescing, a cleared marker never hides an undelivered sibling, and its acked-heads record is the current frontier",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/push_backlog.rs BacklogOwner::enqueue; crates/database/p2p/src/sync/coordinator/push_worker.rs run_push_job, is_whole_frontier; crates/engine/storage/src/stores/peerstore.rs refresh_push_failure_with_generation, complete_retry_scope, record_acknowledged_heads",
        model_ref: "MC_PushCoalescing_{Green,Green_Siblings,Red_AppendEvery,Red_StaleRetry,Red_SiblingClear,Red_UnfencedAck}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Rejected is assumed content-determined. Under strict (Vera) ACP a
        // signed update the provider refuses is no longer Rejected: it is
        // parked by its document and re-driven (ParkedMerge.tla), so it never
        // reaches the quarantine. An unsigned update's refusal stays
        // Rejected, and is content-determined: no grant can cover it.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_NoSilentDrop / LIVE_SoundEventuallyMerged / LIVE_PoisonQuiesces assume a rejection is content-determined",
        axis: Tla,
        anchor: "crates/database/db/src/merge/acp_merge_handler.rs on_protected_update; crates/database/p2p/src/sync/coordinator/merge_state.rs quarantine_pending_dag",
        model_ref: "MC_PendingDagQuarantine_{Green,Red_OvereagerQuarantine,Red_RetryForever}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // With no pending store installed a success ack is backed only in
        // memory, and a restart loses it. Every production embedding installs
        // a store.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_AckBacked holds only with a pending store installed",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pushlog.rs process_block_inner, extend_registered_root; crates/database/p2p/src/sync/pending_store/mod.rs",
        model_ref: "MC_PendingDagRestart_{Green,Red_ProcessLocal}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
