use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Composes the sender's markers, the receiver's admission and durable
        // records, the fetch pool and the hub's forward. The GREENs are the
        // target contract, not main: main violates this today, and REDs
        // F1, F2, F3, F4 and both halves of F5 are main's behaviour.
        // Assumes no kill between a local commit and its marker registration:
        // the drain before P2P stops covers a graceful stop only, and no
        // durable state covers that window.
        family: "P2P delivery liveness",
        name: "INV_ObligationOwned — every head merged on a sender stays owed over each edge until the receiver merged it or holds a durable record whose DAG brings it, judged by ancestry",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/head_scope.rs ScopeDecision::of; crates/database/p2p/src/sync/manager/process/pushlog.rs process_block_inner; crates/database/p2p/src/sync/coordinator/broadcast.rs admit_replicator_push; crates/database/p2p/src/sync/replication/handlers.rs handle_block_received; crates/database/db/src/merge/txn_broadcaster.rs SyncTxnBroadcaster::submit_update; crates/entry-points/defra-node/src/lib.rs Node::shutdown; crates/surfaces/p2p-adapter/src/iroh_peer/peer.rs IrohPeer::assemble",
        model_ref: "MC_P2PDelivery_{Green_Receiver,Green_E2E,Red_F1_PriorityCover,Red_F2_MarkerAfterCommit,Red_F3_DischargeBeforeForward,Red_F4_LoopFirst,Red_F5_FanoutAbort,Red_F5_FailedDropped}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Fairness goes to the dispatch clock, the backoff timer, the resync
        // sweep, the recorder, the retry sweep and slotted fetch tasks, never
        // to a waiting root.
        // The GREENs are the target contract, not main: main violates this
        // today through F8 and F9, which are main's behaviour. The receiver
        // rules #2548 put on main are fixed in the model, and their REDs live
        // in MC_SyncOwnership_Dispatch and MC_SyncOwnership_Memory. The
        // receiver GREEN keeps one of its two global permits out of the pull
        // fetch's reach, which the code's limiter does not;
        // Finding_AdvisoryPermits is that gap. Eventual connectivity and the
        // bounded instance are assumed.
        family: "P2P delivery liveness",
        name: "LIVE_HeadDelivered — a deliverable head reaches every node its replicator edges lead to, through any hub, under task-level fairness only",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/mod.rs dispatch_due_pending_dag_fetches, DagFetchLimiter; crates/database/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag, fetch_dag_attempt; crates/database/p2p/src/sync/coordinator/dag_retry.rs CLOCK_ATTEMPT_QUANTUM; crates/database/p2p/src/sync/coordinator/dag_context.rs DagFetchContext::finish_clock_attempt; crates/database/p2p/src/sync/manager/process/pending_dag.rs PendingDagLease::finish_attempt, due_pending_dag_retries, resync_persisted_pending_dags, records_after_resync_cursor, consume_pending_merge_continuation; crates/database/p2p/src/sync/manager/pending.rs PendingDag::is_expired, PENDING_ROTATE_AFTER_DISPATCHES; crates/surfaces/p2p-adapter/src/retry.rs run_retry_pass; crates/database/db/src/merge/push_docs.rs retry_doc",
        model_ref: "MC_P2PDelivery_{Green_Receiver,Green_E2E,Red_F8_SlotBeforePermit,Red_F9_MergeFifo,Finding_AdvisoryPermits}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Main violates this (F27): connect_with_cache does not adopt the
        // connection it dials, while the accepting side caches it for its own
        // requests, which then wait 30 s and retire it. The gents rc1 pin
        // (b50c667e9) already adopts every new connection.
        family: "P2P delivery liveness",
        name: "INV_ReverseBorrowServed — no node sends a request on a connection its peer does not serve",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_rpc.rs connect_with_cache, retire_timed_out_connection, REQUEST_RESPONSE_TIMEOUT; crates/database/p2p/src/iroh/endpoint_streams.rs handle_incoming",
        model_ref: "MC_P2PDelivery_{Green_E2E,Red_F27_HalfRegistered}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The binary built with `model-trace` logs one event per model
        // action; TLC replays a recorded run through the model under the
        // fixed design's knobs. Two recorded runs and eleven planted RED
        // configurations, one per replay guard, run in run-all.sh; the live
        // replay needs DEFRA_TRACE_BINARY and a JDK, and runs in the
        // Conformance job.
        family: "P2P delivery liveness",
        name: "TRACE_P2PDelivery — a recorded run of the binary is a behaviour of P2PDelivery: every logged event is a model step from the state the earlier ones left, the replayed states keep INV_ObligationOwned and INV_Bounded, and the run settles",
        axis: Tla,
        anchor: "crates/database/p2p/src/model_trace.rs Event; proofs/src/model_trace.rs Trace::abstracted, replay; proofs/tests/behavioral/model_trace.rs two_node_push_replays_through_p2p_delivery, hub_relay_replays_through_p2p_delivery",
        model_ref: "Trace_P2PDelivery.tla; MC_P2PDelivery_Trace_{TwoNode,HubRelay,Red_*}.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/model_trace.rs two_node_push_replays_through_p2p_delivery, hub_relay_replays_through_p2p_delivery",
    },
    Property {
        // The witnesses are integration tests outside this conformance
        // binary: p2p_admission_restart and issue1154_repro. Assumes a pending
        // store is installed: without one the registration, and so the ack, is
        // backed in memory only. Every production embedding installs one.
        family: "P2P delivery liveness",
        name: "INV_AckBacked — a success ack stays backed across a hub restart by a merge, a resident registration or a durable record",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pushlog.rs process_block_inner; crates/database/p2p/src/sync/manager/process/pending_dag.rs resync_persisted_pending_dags; tools/integration-test/tests/legacy/p2p_admission_restart.rs",
        model_ref: "MC_PendingDagRestart_{Green,Red_ProcessLocal}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The witness is the p2p_admission integration test, outside this
        // conformance binary.
        family: "P2P delivery liveness",
        name: "INV_SuccessImpliesRegisteredOrMerged — a PushLog at pending capacity is nacked, never acked with its registration dropped",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/manager/process/pushlog.rs process_block_inner; tools/integration-test/tests/legacy/p2p_admission.rs fan_in_pushlog_admission_no_silent_divergence",
        model_ref: "MC_PushLogAdmission_{Green,Red_SuccessOnFull}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
