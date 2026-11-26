use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, query, storage, p2p, p2p-adapter";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Retry input and marker fencing",
        name: "INV_ProviderOncePerContext / INV_NewRegistrationSurvivesOldAck / ACT_NewRegistrationSurvivesOldClear",
        axis: Tla,
        anchor: "crates/engine/query/src/error.rs QueryError::NeedsInput; crates/database/db/src/write/autocommit/create.rs create_many_with_resolved_inputs; update.rs update_with_projected_resolved_inputs; delete.rs delete_with_resolved_inputs; crates/database/db/src/write/create.rs embed, compute_many; crates/engine/query/src/prepared.rs; crates/engine/storage/src/stores/peerstore.rs register/complete/clear retry scope",
        model_ref: "MC_RetryPreparedInputs_{Green,Red_*}.cfg / MC_RetryMarkerGeneration_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_SweepReadsBounded / LIVE_VanishedMarkerDischarged — a sweep tick reads one probe and at most one page of markers; a purged document's marker is discharged under scheduler fairness only",
        axis: Tla,
        anchor: "crates/surfaces/p2p-adapter/src/retry.rs run_retry_pass; crates/engine/storage/src/stores/peerstore.rs get_retry_page, load_scope_markers, reschedule_retry_peer_for_incarnation; crates/database/db/src/merge/push_docs.rs retry_doc; crates/database/db/src/merge/push_docs_common.rs complete_document_retry_if_absent",
        model_ref: "MC_RetryMarkerGeneration_{Green,Red_UnboundedReads,Red_KeepVanished,Red_NoRotation}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_QueuedPushOwnsLiveMarker — admission requires current durable ownership and elapsed backpressure",
        axis: Tla,
        anchor: "crates/engine/storage/src/stores/peerstore.rs admit_push_scope; crates/surfaces/p2p-adapter/src/retry.rs spawn_failure_recorder; crates/database/p2p/src/sync/coordinator/push_worker.rs run_push_job",
        model_ref: "MC_RetryPushAdmission_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "ACT_RegistrationCannotCrossFence — allocated registrations preserve peer incarnation and marker order",
        axis: Tla,
        anchor: "crates/engine/storage/src/stores/peerstore.rs RegisterScopeMarkerAttempt; create_replicator",
        model_ref: "MC_RetryRegistrationFence_{Green,Red_NoEpoch,Red_NoOrder}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_FanOutPeerLocal — a write's registrations run at once, one replicator's rejected registration leaves every other replicator's marker and job, and the write queues only once every registration is decided",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/broadcast.rs admit_replicator_push; broadcast_tests.rs fanout_registration_failure_is_peer_local, fanout_registrations_run_at_once",
        model_ref: "MC_RetryRegistrationFence_{Green,Red_FanOutAbort,Red_NoJoin}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_OnePeerAtATime / INV_PeerOrder / INV_AnsweredOnlyWhenDurable / LIVE_EveryRecordAnswered — the failure recorder runs a peer's records in its peer's lane, one transaction per peer per round and in the order sent, answers a sender only once its record is durable, and answers every record",
        axis: Tla,
        anchor: "crates/surfaces/p2p-adapter/src/retry.rs spawn_failure_recorder, lane_of, run_lane, record_peer; crates/engine/storage/src/stores/peerstore/scope_batch.rs Peerstore::record_scopes",
        model_ref: "MC_RetryRecorderLanes_{Green,Red_ScopeLanes,Red_RecordPerTxn,Red_AnswerEarly}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_EveryMarkerScheduled — an ack or empty-peer clear deletes a peer's schedule row only when a validated scan found no marker, and a registration that saw the row is aborted by its validated row read, so a registration that commits meanwhile is never left unscheduled",
        axis: Tla,
        anchor: "crates/engine/storage/src/stores/peerstore.rs scope_markers_left, walk_scope_markers, CompleteMarker, ClearPeer, RegisterScopeMarkerAttempt; crates/engine/storage/src/stores/peerstore_schedule_race_tests.rs",
        model_ref: "MC_RetryPeerSchedule_{Green,Red_PlainScan,Red_UnvalidatedRowRead}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // One retry clock per peer serves all of its markers. An acknowledged
        // delivery, live, fenced by a newer marker generation, or replayed,
        // returns it to the first rung, never later than it was, so the next
        // failure charges the second rung; a new marker and a reconnect keep
        // the rung, so a peer that keeps failing climbs the ladder however
        // often local writes touch it; and the sweep reads a deadline past
        // the longest rung, left by a backward clock step, as due.
        family: "Retry input and marker fencing",
        name: "ACT_DeliveryRestarts / ACT_SilentPeerBacksOff / INV_HeldWithinLongestRung — a delivery restarts the peer clock, nothing else does, and no deadline holds past the longest rung",
        axis: Tla,
        anchor: "crates/engine/storage/src/stores/retry_info.rs RetryInfo::restart_after_delivery, RetryInfo::bounded_by, RetryInfo::bump_with_schedule; crates/engine/storage/src/stores/peerstore.rs PeerstoreRetryAttempt::{CompleteMarker, Reschedule, Restart, Activate}, RegisterScopeMarkerAttempt, Peerstore::load_scope_markers; crates/surfaces/p2p-adapter/src/retry.rs run_retry_pass, record_failure; crates/database/p2p/src/sync/coordinator/push_worker.rs run_push_job; crates/database/db/src/merge/push_docs_common.rs complete_document_retry_if_current",
        model_ref: "MC_RetrySchedule_{Green,Red_KeepRung,Red_SupersededKeepsRung,Red_ReplayKeepsRung,Red_RegisterRestarts,Red_ActivateRestarts,Red_ClockBackHolds}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "restart_rung_le_one / restart_never_later / restart_within_first_rung / restart_respects_backpressure / bump_climbs",
        axis: Lean,
        anchor: "crates/engine/storage/src/stores/retry_info.rs RetryInfo::restart_after_delivery, RetryInfo::bump_with_schedule",
        model_ref: "RetrySchedule (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
