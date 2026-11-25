use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "defra-node, embedded, edge, vera-embedded, acp, http, query, db, storage";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A node's shutdown aborts every task it holds and waits for each to
        // let go of storage before the pool stops and regolith closes; the
        // drain gives up at its deadline; a refused telemetry flush moves to
        // a thread of its own instead of the caller. Requests and in-process
        // writes stop and drain before P2P, so every admitted write registers
        // its head marker; the transactions callers left open end before
        // regolith closes.
        family: "Node shutdown order",
        name: "INV_NoStorageAfterClose / INV_NothingDetached / INV_CallerNeverJoinsExporter / INV_AdmittedWritesMarked / INV_CloseWaitsForNoTransaction / LIVE_ShutdownTerminates: no task reaches storage after regolith closes, none outlives shutdown, every admitted write is marked, the close waits for no transaction left open, shutdown returns",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/lib.rs Node::shutdown, Node::as_request; crates/entry-points/defra-node/src/ops/request.rs Node::txn_begin, Node::txn_commit; crates/entry-points/defra-node/src/network.rs NodeEdge::drain_requests; crates/surfaces/http/src/server.rs Server::router; crates/entry-points/defra-node/src/node_acp.rs gate_external_acp; crates/entry-points/defra-node/src/managed_p2p.rs ShutdownHandle::stop, stop_services; crates/foundation/edge/src/task.rs Service::stopped; crates/foundation/edge/src/handle.rs Scope::drain; crates/building-blocks/net/src/server.rs serve, until_closing",
        model_ref: "MC_NodeShutdown_{Green,Red_RedriveDetached,Red_ServicesNotAwaited,Red_NoDrainDeadline,Red_FlushOnCaller,Red_RequestsDuringP2P,Red_CommitNotAdmitted,Red_OpenTransaction}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A node dropped without shutdown aborts every task its shutdown set
        // holds, the transaction sweep among them, without waiting for any,
        // so its pool's threads and its store are released.
        family: "Node shutdown order",
        name: "LIVE_DroppedNodeReleasesPool / INV_DropNeverWaits / INV_ShutdownStopsEveryTask: a dropped node releases its pool and store, its drop waits for no task, a shutdown stops every task",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/assemble.rs assemble; crates/entry-points/defra-node/src/background_tasks.rs BackgroundTasks; crates/database/db/src/txn/registry/cleanup.rs start_stale_transaction_cleanup; crates/foundation/workers/src/pool/background.rs Background",
        model_ref: "MC_NodeDrop_{Green,Red_SweepOutsideSet,Red_NoAbortOnDrop,Red_DropAwaits}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/tests/node_drop.rs a_dropped_http_node_releases_its_threads_and_descriptors",
    },
    Property {
        // A node that runs its own Vera node stops it after the request
        // drain, waits for it up to a deadline and closes its transports:
        // a remembered decision outlives it by no more than the freshness of
        // the state it was read at, and once it has ended its
        // directory is free for the next node. A Vera node that hangs past
        // the deadline keeps its directory.
        family: "Node shutdown order",
        name: "INV_NoDecisionPastFreshness / INV_AdmittedRequestsDecided / INV_StoppedNodeFreesDirectory / LIVE_ShutdownReturns / LIVE_NextStartReturns: once the embedded Vera node is gone a remembered access decision is repeated no longer than its state stays fresh and no new one is given, shutdown stops it under no admitted request but one the request drain gave up on, a node that shut down holds the directory of an ended Vera node no longer, shutdown returns, the next start over the directory starts or fails at its deadline",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/lib.rs Node::shutdown; crates/entry-points/defra-node/src/embedded_vera.rs start; crates/entry-points/cli/src/commands/start/run.rs Node::run; crates/entry-points/cli/src/commands/start/server_acp_embedded.rs setup; crates/engine/vera/src/vera_rs/provider.rs decision_state; crates/engine/vera-embedded/src/host.rs HostedNode::start, HostedNode::stop; crates/engine/vera-embedded/src/link.rs Link; crates/engine/vera-embedded/src/call.rs call",
        model_ref: "MC_EmbeddedVeraStop_{Green,Red_StaleDecision,Red_StopBeforeRequests,Red_NoStopDeadline,Red_LinkStaysOpen,Red_NoStartDeadline}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/vera-embedded/tests/hosted_node.rs once_the_node_stops_the_provider_verifies_no_new_decision; crates/engine/vera-embedded/tests/stopped_node_decisions.rs a_remembered_decision_is_not_given_past_the_freshness_limit",
    },
    Property {
        // A signed query or transformed route keeps a request-scope permit
        // while its started worker writes, so P2P cannot stop first.
        family: "Node shutdown order",
        name: "INV_CommittedMarked: a signed query or transformed route commits with its durable head marker",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/signed_query/mod.rs execute_with_signing_context; crates/surfaces/http/src/server.rs Server::router; crates/engine/query/src/pooled/mod.rs PooledExecutor::execute_prepared; crates/foundation/workers/src/pool/detached.rs run_on; crates/entry-points/defra-node/src/network.rs NodeEdge::drain_requests",
        model_ref: "MC_SignedRequestDrain_{Green,Red_TaskDeadline,Red_Transform}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/src/signed_query/tests.rs a_signed_query_remains_admitted_past_the_edge_deadline; crates/entry-points/defra-node/tests/unit/verified_create.rs signed_writes_are_refused_once_requests_have_drained; crates/surfaces/http/tests/request_admission.rs a_route_added_by_router_transform_stays_admitted_until_it_ends",
    },
    Property {
        // A transaction guard follows its admitted commit task rather than a
        // caller that can stop waiting while the task still runs.
        family: "Node shutdown order",
        name: "INV_CommitNotAbandoned: cancelling the waiter does not abandon an edge-admitted commit in flight",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/transaction.rs NodeTransaction::commit; crates/engine/query/src/txn/guard.rs TransactionGuard::drop; crates/database/db/src/txn/owner/table.rs Table::abandon",
        model_ref: "MC_GuardedCommit_{Green,Red_CallerOwns}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/src/transaction/cancellation_tests.rs cancelling_a_commit_waiter_does_not_abandon_the_running_commit",
    },
    Property {
        // Request admission follows the worker; transaction ownership follows
        // its commit task when the caller gives up waiting.
        family: "Node shutdown order",
        name: "start_keeps_permit / finish_keeps_permit / dropped_waiter_keeps_permit / expired_task_loses_permit / task_guard_survives_cancellation / caller_guard_is_abandoned / begin_open_has_owner / abandoned_begin_is_closed",
        axis: Lean,
        anchor: "crates/entry-points/defra-node/src/signed_query/mod.rs execute_with_signing_context; crates/entry-points/defra-node/src/transaction.rs NodeTransaction::commit; crates/foundation/workers/src/pool/detached.rs run_on",
        model_ref: "RequestDrain (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/src/signed_query/tests.rs a_signed_query_remains_admitted_past_the_edge_deadline; crates/surfaces/http/tests/request_admission.rs a_route_added_by_router_transform_stays_admitted_until_it_ends; crates/entry-points/defra-node/src/transaction/cancellation_tests.rs cancelling_a_commit_waiter_does_not_abandon_the_running_commit, cancelling_a_begin_waiter_abandons_the_unreturned_transaction",
    },
    Property {
        // The result of a detached begin abandons its transaction if its
        // caller leaves before receiving the handle.
        family: "Node shutdown order",
        name: "INV_OpenHasOwner: an edge-admitted begin cannot leave an unowned transaction open",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/lib.rs Node::begin_transaction; crates/entry-points/defra-node/src/ops/request.rs Node::txn_begin; crates/engine/query/src/txn/guard.rs TransactionGuard::drop",
        model_ref: "MC_DetachedBegin_{Green,Red_NoResultGuard}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/src/transaction/cancellation_tests.rs cancelling_a_begin_waiter_abandons_the_unreturned_transaction",
    },
];
