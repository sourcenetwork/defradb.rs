//! The conformance registry: every modeled family's headline property, the
//! model that proves it, the source it is anchored to, and how it is bound to
//! the real binary.
//!
//! This table is the spine: its distinct `family` values are the modeled
//! families. `matrix::readme_lists_exactly_the_modeled_families` asserts that
//! `proofs/README.md`'s *Modeled families* table lists exactly these, so a new
//! model cannot land without declaring how it is kept honest against the code.

/// Which proof tool establishes the property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// TLA+ / TLC temporal-safety model (`proofs/tla/`).
    Tla,
    /// Lean 4 functional/algebraic proof (`proofs/lean/`).
    Lean,
}

/// How a property is checked for conformance with the implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Exercised against the running release binary (`tests/tla_conformance.rs`).
    Behavioral,
    /// A model vocabulary/contract asserted against the live Rust types
    /// (`tests/lean_conformance.rs`) — anti-drift, no binary needed.
    Contract,
    /// An assumed boundary (crypto / connectivity / bounded-N / foreign
    /// substrate): surfaced, never asserted, so a green matrix never reads as
    /// "this was proven against the artifact."
    Boundary,
}

pub struct Property {
    /// Family name — must match a row in `proofs/README.md`'s Modeled families table.
    pub family: &'static str,
    /// The headline invariant or theorem.
    pub name: &'static str,
    pub axis: Axis,
    /// The source the model is grounded to: paths and the symbols in them,
    /// resolved against the tree by `anchor::every_registry_anchor_resolves`.
    pub anchor: &'static str,
    /// The TLC config (`*.cfg`) or Lean theorem that establishes it.
    pub model_ref: &'static str,
    /// How this property is bound to the implementation.
    pub tiers: &'static [Tier],
    /// The tests that bind a Contract or Behavioral property, in anchor
    /// syntax: each file and the test functions in it. Empty for a property
    /// that is only a Boundary.
    pub bound_by: &'static str,
}

use Axis::{Lean, Tla};
use Tier::{Behavioral, Boundary, Contract};

pub const PROPERTIES: &[Property] = &[
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LocalFetchPermit.released_before_send / release_preserves_queue",
        axis: Lean,
        anchor: "crates/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag",
        model_ref: "LocalFetchPermit (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/p2p/src/sync/coordinator/dag_fetcher_tests.rs local_complete_fetch_releases_peer_permit_before_waiting_to_send",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "NoPermitWhileSending / OnePermit — locally complete fetch releases its peer permit before queueing merge",
        axis: Tla,
        anchor: "crates/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag; crates/p2p/src/sync/coordinator/dag_fetcher_tests.rs local_complete_fetch_releases_peer_permit_before_waiting_to_send",
        model_ref: "MC_LocalFetchPermit_{Green,Red_Hold}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/p2p/src/sync/coordinator/dag_fetcher_tests.rs local_complete_fetch_releases_peer_permit_before_waiting_to_send",
    },
    Property {
        family: "HTTP router composition",
        name: "WholeRouter / SingleConstructor / NoBypass / ForwardingPreserved",
        axis: Tla,
        anchor: "crates/http/src/server.rs Server::router, Server::serve; router_transform_tests.rs forwarding_filter_matches_executed_model; crates/defra-node/src/router_transform_tests.rs",
        model_ref: "MC_HttpRouterComposition_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "HTTP router composition",
        name: "configured_order / constructor_once / absent_preserves / forwarding_preserves / refuses_every_inner_status",
        axis: Lean,
        anchor: "crates/http/src/server.rs Server::router; router_transform_tests.rs forwarding_filter_matches_executed_model",
        model_ref: "HttpRouterComposition (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Disconnect answers only once the peer's last connection closed, so
        // ActivePeers right after it no longer lists the peer, as in Go.
        family: "Peer connection observation",
        name: "INV_AnsweredMeansClosed / PROP_CallerAnswered",
        axis: Tla,
        anchor: "crates/p2p/src/host/command_handler/network.rs handle_disconnect; crates/p2p/src/host/p2p_host/swarm.rs handle_connection_closed; tools/integration-test/tests/net/info/disconnect_peers.rs",
        model_ref: "MC_PeerDisconnect_{Green,Red_EarlyAck}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // iroh answers Disconnect once the close has started, the Red_EarlyAck
        // shape: connected_peers lists the peer until its stream task observes
        // the close, so a connect_peer in that window does nothing. Callers
        // that need the peer gone poll connected_peers.
        family: "Peer connection observation",
        name: "INV_AnsweredMeansClosed binds libp2p only: iroh answers Disconnect before the peer leaves connected_peers",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/endpoint_commands.rs handle_disconnect; crates/p2p-adapter/src/iroh.rs connected_peers, connect_peer",
        model_ref: "MC_PeerDisconnect_Red_EarlyAck.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Peer connection observation",
        name: "ExplicitOnly / ExactAddress / RequestedPort / DefaultPreserved / UnavailableRefused / MappingScope / MappingDefaultsPreserved",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/endpoint_config.rs apply_bind_config; endpoint_config/bind_tests.rs native_bindings_match_executed_model, native_port_mapping_configuration_matches_executed_model, loopback_endpoint_does_not_open_wildcard_gateway_discovery_listeners",
        model_ref: "MC_EndpointBind_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Peer connection observation",
        name: "EndpointBind.explicit_replaces / explicit_preserves / absent_preserves_defaults / port_only_preserves_dual_stack / specific_address_disables_mapping / absent_address_preserves_mapping / wildcard_preserves_mapping",
        axis: Lean,
        anchor: "crates/p2p/src/iroh/endpoint_config.rs apply_bind_config; endpoint_config/bind_tests.rs model_cases, native_port_mapping_configuration_matches_executed_model",
        model_ref: "EndpointBind (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Selective CAR capacity: observed completed grants yield to active sends; in-flight scope and quiescent limits are preserved",
        axis: Tla,
        anchor: "crates/p2p/src/sync/coordinator/selective_car_access.rs register, reclaim_completed and allows_root",
        model_ref: "MC_SelectiveCarCapacity_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "admitted_bounded / warm_does_not_refuse / reclaim_preserves_active / authorized_scope / expired_completed_refused",
        axis: Lean,
        anchor: "crates/p2p/src/sync/coordinator/selective_car_access.rs register, reclaim_completed and allows_root",
        model_ref: "SelectiveCarCapacity (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Selected transfer owns its acknowledgements independently of subscriptions; replay stays fenced, scoped and grant-bound",
        axis: Tla,
        anchor: "crates/db/src/merge/push_docs.rs push_existing_docs_by_id",
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
        anchor: "crates/storage/src/stores/peerstore/selected_car.rs revoke_selected_car_root; crates/p2p/src/sync/coordinator/event_handler/car.rs",
        model_ref: "MC_SelectedCarAuthority_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Selected pushes ignore subscription filters and retry schedules, retain hint ownership and drain all started tasks",
        axis: Tla,
        anchor: "crates/db/src/merge/push_docs.rs push_existing_docs_by_id",
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
        anchor: "crates/storage/src/stores/peerstore/selected_car.rs",
        model_ref: "SelectedCarAuthority (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "selection_owns_acknowledgements / absent_replay_is_refused / selected_failure_is_not_deferred / selected_retains_heads / absent_peer_has_no_observation",
        axis: Lean,
        anchor: "crates/db/src/merge/push_docs.rs; crates/db/tests/merge/push_docs_selection.rs",
        model_ref: "DocumentPush.lean; selected_document_transfers_match_executed_model",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Rooted authorization: one producer, exact grants, bounded optional retention, fenced eviction and cancellation wakeup",
        axis: Tla,
        anchor: "crates/p2p/src/sync/car_authorization.rs; car_authorization_registry.rs walk_with_limits; crates/p2p/tests/unit/car_authorization_limits.rs",
        model_ref: "MC_RootedAuthorization_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "reserve_bounded / release_restores / replacement_survives / grants_independent_of_retention",
        axis: Lean,
        anchor: "crates/p2p/src/sync/car_authorization_budget.rs; car_authorization_registry.rs",
        model_ref: "RootedAuthorization (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Replicator installs validate their row, settle before completion, and retain interrupted outcomes",
        axis: Tla,
        anchor: "crates/p2p-adapter/src/replicator_install_store.rs; crates/p2p-adapter/src/replicator_installs.rs",
        model_ref: "MC_ReplicatorInstall_{Green,Red_*}.cfg; MC_ReplicatorIncarnation_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Install completion is identity-fenced and idempotent; stale allocations re-decide without regressing the incarnation",
        axis: Lean,
        anchor: "crates/p2p-adapter/src/replicator_install_store.rs finish",
        model_ref: "ReplicatorInstall (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_Released — an early reconciliation return releases its queued continuation slot",
        axis: Tla,
        anchor: "crates/p2p/src/sync/replication/handlers.rs::process_one_event; crates/p2p/src/sync/replication/continuation_tests.rs::already_merged_duplicate_continuation_releases_reservation_without_handler",
        model_ref: "MC_ContinuationSlot_{Green,Red_LateClaim}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "owned_slot_released — dropping the continuation owner releases its slot",
        axis: Lean,
        anchor: "crates/p2p/src/sync/coordinator/mod.rs::claim_pending_dag_merge; crates/p2p/src/sync/replication/handlers.rs::process_one_event",
        model_ref: "HistoryTurns.owned_slot_released (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Rejected-page cleanup compares the cursor incarnation before removing it",
        axis: Tla,
        anchor: "crates/db/src/merge/merge_handler/history.rs::reject_history; crates/db/tests/merge/history_cursor.rs",
        model_ref: "MC_RetryMarkerGeneration_{Green,Red_StaleAck,Red_Readd}.cfg (incarnation-fenced retirement abstraction)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "History turns commit cursor and writes together; reclaim preserves publications",
        axis: Tla,
        anchor: "crates/db/src/merge/merge_handler/history.rs; history_registry.rs; dispatch.rs; crates/db/src/merge/replication.rs; crates/db/tests/merge/collection_history_control.rs",
        model_ref: "MC_HistoryTurns_{Green,Red_*}.cfg; MC_HistoryDriver_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Ancestor publication is idempotent; cleanup and collection carriers preserve driver ownership",
        axis: Lean,
        anchor: "crates/db/src/merge/merge_handler/history.rs",
        model_ref: "HistoryTurns (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Storage failure classification",
        name: "ClassificationPreserved / NoReplayAuthority",
        axis: Tla,
        anchor: "crates/storage/src/corekv/errors.rs; crates/db/src/error/mod.rs; crates/query/src/runner/turn.rs; crates/query/src/executor.rs; crates/http/src/error.rs",
        model_ref: "MC_StorageFailure_Green.cfg / MC_StorageFailure_Red_LostCause.cfg / MC_StorageFailure_Red_Text.cfg / MC_StorageFailure_Red_Retry.cfg",
        tiers: &[Contract],
        bound_by: "crates/query/tests/storage_failure.rs storage_failure_classification_matches_executed_model",
    },
    Property {
        family: "Storage failure classification",
        name: "StorageFailure.preserves / full_code / other_code / opaque_code",
        axis: Lean,
        anchor: "crates/storage/src/corekv/io_failure.rs; crates/storage/src/backends/regolith/transaction.rs map_txn_error; crates/query/src/executor.rs QueryResponseError::from_query_error; crates/query/tests/storage_failure.rs",
        model_ref: "StorageFailure.lean",
        tiers: &[Contract],
        bound_by: "crates/query/tests/storage_failure.rs storage_failure_classification_matches_executed_model",
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "Owner and visibility observations retain read gates and the transaction snapshot",
        axis: Tla,
        anchor: "crates/query/src/runner/owners.rs; crates/db/src/read/lensed/fetcher/mod.rs",
        model_ref: "MC_OwnerObservation_Green.cfg / MC_OwnerObservation_Red_*.cfg",
        tiers: &[Contract],
        bound_by: "crates/db/tests/read/owner_observation.rs owner_observation_rejects_a_claim_without_a_bound_transaction_view; crates/query/src/runner/owners_tests.rs owner_observation_rejects_a_fetcher_without_a_transaction_view",
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "Acp.OwnerObservation.exact_snapshot, unreadable_observation_hidden and unregistered_read_keeps_visibility",
        axis: Lean,
        anchor: "crates/query/src/runner/owners.rs",
        model_ref: "Acp.OwnerObservation",
        tiers: &[Contract],
        bound_by: "crates/db/tests/read/owner_observation.rs owner_observation_rejects_a_claim_without_a_bound_transaction_view; crates/query/src/runner/owners_tests.rs owner_observation_rejects_a_fetcher_without_a_transaction_view",
    },
    Property {
        family: "Peer connection observation",
        name: "NoSpuriousDenial — queued lifecycle notification cannot hide a connected peer",
        axis: Tla,
        anchor: "crates/p2p/src/sync/coordinator/authorizer.rs RuntimeAuthorizer::peer_connected; access_tests.rs live_connection_allows_pull_before_lifecycle_notification_without_granting_push",
        model_ref: "MC_PeerConnection_Green.cfg / MC_PeerConnection_Red_StaleObservation.cfg",
        tiers: &[Contract],
        bound_by: "crates/p2p/src/sync/coordinator/access_tests.rs live_connection_allows_pull_before_lifecycle_notification_without_granting_push",
    },
    Property {
        family: "Peer connection observation",
        name: "PeerConnection.live_is_connected / connection_does_not_grant_push",
        axis: Lean,
        anchor: "crates/p2p/src/sync/coordinator/authorizer.rs RuntimeAuthorizer::peer_connected",
        model_ref: "PeerConnection.lean",
        tiers: &[Contract],
        bound_by: "crates/p2p/src/sync/coordinator/access_tests.rs live_connection_allows_pull_before_lifecycle_notification_without_granting_push",
    },
    Property {
        family: "Peer connection observation",
        name: "Reverse RPC reuses inbound and legacy paths; stale eviction cannot hide replacements from disconnect",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/endpoint_streams.rs handle_incoming; endpoint_rpc.rs remember_connection, connect_with_cache and evict_connection; crates/p2p/tests/iroh/shared_connection.rs",
        model_ref: "MC_SharedRpcConnection_{Green,Red_*}.cfg; MC_LegacyRpcCache_{Green,Red_*}.cfg; MC_RpcCacheEviction_{Green,Red_*}.cfg",
        tiers: &[Contract],
        bound_by: "crates/p2p/tests/iroh/shared_connection.rs disconnect_closes_replacement_despite_stale_cleanup, legacy_only_peer_reuses_and_refreshes_its_shared_rpc_path, accepted_inbound_connection_serves_reverse_rpc_without_another_dial, authenticated_reconnection_replaces_an_open_old_rpc_path",
    },
    Property {
        // When the cached RPC handle closes, its slot is left empty and the
        // next RPC dials rather than promoting an open sibling. That is cheap
        // only because every dialled connection is adopted and served; a
        // sibling of unknown liveness may be a connection whose peer is gone.
        family: "Peer connection observation",
        name: "A closed cached RPC connection is replaced by a fresh dial, never by promoting a sibling",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/endpoint_rpc.rs connect_with_cache, evict_connection; crates/p2p/src/iroh/endpoint_streams.rs handle_connection_streams",
        model_ref: "MC_SharedRpcConnection_{Green,Red_NoAdoption}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Peer connection observation",
        name: "SharedRpc replacement identity, legacy reuse, modern preference, revoked refusal and stable-id eviction",
        axis: Lean,
        anchor: "crates/p2p/src/iroh/endpoint_rpc.rs remember_connection, connect_with_cache and evict_connection; endpoint_streams.rs handle_incoming",
        model_ref: "PeerConnection.SharedRpc",
        tiers: &[Contract],
        bound_by: "crates/p2p/tests/iroh/shared_connection.rs authenticated_reconnection_replaces_an_open_old_rpc_path, legacy_only_peer_reuses_and_refreshes_its_shared_rpc_path, legacy_mux_does_not_replace_the_modern_shared_rpc_handle, disconnect_closes_replacement_despite_stale_cleanup",
    },
    Property {
        family: "Schema publication completion",
        name: "VisibleBeforeReply — committed schemas are published before their callers resume",
        axis: Tla,
        anchor: "crates/storage/src/corekv/callbacks.rs OnCommit::OrderedAndWait; crates/db/tests/collection/schema_publication.rs",
        model_ref: "MC_SchemaPublication_Green.cfg / MC_SchemaPublication_Red_EarlyReply.cfg",
        tiers: &[Contract],
        bound_by: "crates/db/tests/collection/schema_publication.rs schema_success_waits_for_its_ordered_cache_publication",
    },
    Property {
        family: "Schema publication completion",
        name: "SchemaPublication.preserves",
        axis: Lean,
        anchor: "crates/storage/src/corekv/callbacks.rs OnCommit::OrderedAndWait; crates/db/tests/collection/schema_publication.rs",
        model_ref: "SchemaPublication.lean",
        tiers: &[Contract],
        bound_by: "crates/db/tests/collection/schema_publication.rs schema_success_waits_for_its_ordered_cache_publication",
    },
    Property {
        family: "DiskANN pending cache",
        name: "INV_NoHiddenPending — a search using the in-memory pending list never misses a vector its snapshot or its own transaction has pending",
        axis: Tla,
        anchor: "crates/db/src/index/vector/engine/diskann/graph.rs pending_candidates; crates/db/src/index/vector/engine/diskann/pending_cache.rs PendingCache; crates/db/src/index/vector/engine/diskann/mod.rs insert, delete; crates/db/src/index/vector/engine/diskann/merge.rs merge_plan, merge",
        model_ref: "MC_PendingCache_Green.cfg (GREEN) / MC_PendingCache_Red_NoGeneration.cfg / MC_PendingCache_Red_OwnWrites.cfg / MC_PendingCache_Red_TruncateThroughHead.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/db/tests/vector/diskann_pending_cache.rs later_inserts_catch_the_copy_up_without_a_reload, a_transaction_finds_its_own_uncommitted_insert, a_merge_truncates_the_log_it_absorbed, an_insert_during_a_merge_stays_pending",
    },
    Property {
        family: "DiskANN pending cache",
        name: "prefix_covered — a cache loaded once and caught up on every logged insert holds every vector a snapshot of its generation has pending, and a merge that cuts the log anywhere loses no live vector",
        axis: Lean,
        anchor: "crates/db/src/index/vector/engine/diskann/pending_cache.rs PendingCache; crates/db/src/index/vector/engine/diskann/mod.rs insert; crates/db/src/index/vector/engine/diskann/merge.rs merge_plan, merge",
        model_ref: "PendingCache.Covers.prefix_covered / catch_up_in_steps / merge_keeps_every_vector / truncate_through_head_loses",
        tiers: &[Contract],
        bound_by: "crates/db/tests/vector/diskann_pending_cache.rs later_inserts_catch_the_copy_up_without_a_reload, a_catch_up_past_the_cap_is_refused, a_merge_truncates_the_log_it_absorbed",
    },
    Property {
        family: "DiskANN pending cache",
        name: "INV_ScoresLatest — a search scores each pending vector by the newest entry its snapshot or its own transaction holds, never an obsolete one or one from a cache ahead of it",
        axis: Tla,
        anchor: "crates/db/src/index/vector/engine/diskann/graph.rs pending_candidates, snapshot_pending; crates/db/src/index/vector/engine/diskann/pending_cache.rs Copy::for_each_latest_through, Copy::caught_up, Segment::push, Segment::supersede",
        model_ref: "MC_PendingCache_Green.cfg (GREEN) / MC_PendingCache_Red_EveryVersion.cfg / MC_PendingCache_Red_FutureVersion.cfg / MC_PendingCache_Red_HeadBlindMarks.cfg / MC_PendingCache_Red_NoGapCheck.cfg / MC_PendingCache_Red_NoGeneration.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/db/tests/vector/diskann_pending_cache.rs an_updated_vector_is_scored_by_its_latest_code, a_transaction_scores_its_own_update_by_its_latest_code, a_snapshot_behind_the_copy_scores_the_codes_it_can_see, a_superseded_marker_respects_the_readers_head",
    },
    Property {
        family: "DiskANN pending cache",
        name: "scored_is_last — the code kept for a vector is that of its last entry visited, and a copy cut at a search's head scores as the search's own log does",
        axis: Lean,
        anchor: "crates/db/src/index/vector/engine/diskann/graph.rs pending_candidates; crates/db/src/index/vector/engine/diskann/pending_cache.rs Copy::for_each_latest_through",
        model_ref: "PendingCache.Latest.scored_is_last / scored_append / cut_scores_snapshot / uncut_scores_future",
        tiers: &[Contract],
        bound_by: "crates/db/tests/vector/diskann_pending_cache.rs an_updated_vector_is_scored_by_its_latest_code, a_snapshot_behind_the_copy_scores_the_codes_it_can_see",
    },
    Property {
        family: "DiskANN pending cache",
        name: "kept_is_newest — a search that skips an entry only when its superseded marker is at or before the search's head keeps exactly each vector's newest entry in reach",
        axis: Lean,
        anchor: "crates/db/src/index/vector/engine/diskann/pending_cache.rs Segment::push, Segment::supersede, Copy::for_each_latest_through",
        model_ref: "PendingCache.Marks.kept_is_newest / newest_is_kept / one_kept_per_vector / next_decides / head_blind_marker_hides",
        tiers: &[Contract],
        bound_by: "crates/db/tests/vector/diskann_pending_cache.rs a_superseded_marker_respects_the_readers_head, racing_catch_ups_agree_on_markers, a_folded_copy_keeps_one_entry_per_vector",
    },
    Property {
        family: "DiskANN pending cache",
        name: "Evict — forgetting the pending cache at any moment, as an index or collection drop does, keeps every search's candidates complete and scored by their newest entry",
        axis: Tla,
        anchor: "crates/db/src/index/vector/engine/diskann/cache.rs CodeCaches::evict, evict_collection; crates/db/src/collection/ops/delete.rs delete_collection_as; crates/db/src/collection/ops/version.rs delete_collection_version_as",
        model_ref: "MC_PendingCache_Green_Evict.cfg (GREEN)",
        tiers: &[Contract],
        bound_by: "crates/db/tests/vector/diskann_pending_cache.rs evicting_drops_only_the_named_caches",
    },
    Property {
        family: "DiskANN shared quantizer",
        name: "INV_EncodesWithItsCodebooks — a write sharing the node's quantizer encodes only with its own snapshot's codebooks, across rebuilds and retried trainings",
        axis: Tla,
        anchor: "crates/db/src/index/vector/engine/diskann/mod.rs DiskAnn::quantizer; crates/db/src/index/vector/engine/diskann/cache.rs CodeCache::quantizer, CodeCache::share_quantizer; crates/db/src/index/vector/engine/diskann/build.rs train_and_build",
        model_ref: "MC_SharedQuantizer_Green.cfg (GREEN) / MC_SharedQuantizer_Red_NoTagCheck.cfg / MC_SharedQuantizer_Red_TagPerRequest.cfg / MC_SharedQuantizer_Red_OneSlot.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/db/tests/vector/diskann_shared_quantizer.rs a_write_shares_the_quantizer_under_its_training, a_quantizer_under_another_training_is_not_used, writers_straddling_a_rebuild_keep_both_quantizers",
    },
    Property {
        family: "DiskANN shared quantizer",
        name: "decode_encode — the trained record reads back as written in both encodings, and the shared quantizer answers a tag only with that tag's codebooks",
        axis: Lean,
        anchor: "crates/db/src/index/vector/engine/diskann/codec.rs encode_trained, decode_trained; crates/db/src/index/vector/engine/diskann/cache.rs CodeCache::quantizer, CodeCache::share_quantizer",
        model_ref: "SharedQuantizer.Tagged.decode_encode / lookup_is_books / share_keeps_consistent / reused_tag_serves_other_books / share_keeps_previous / one_slot_evicts",
        tiers: &[Contract],
        bound_by: "crates/db/tests/vector/diskann_shared_quantizer.rs trained_record_reads_back_with_and_without_its_training, a_quantizer_under_another_training_is_not_used",
    },
    Property {
        family: "RobustPrune blocked covered checks",
        name: "blocked_eq_sequential — checking candidates in blocks keeps exactly the edges the one-at-a-time walk keeps, for any cut into blocks",
        axis: Lean,
        anchor: "crates/db/src/index/vector/engine/select/robust_prune.rs RobustPrune::select; crates/defra-core/src/vector/kernel/mod.rs squared_euclidean_block",
        model_ref: "RobustPruneBlocks.blocked_eq_sequential / no_since_keeps_a_covered_edge",
        tiers: &[Contract],
        bound_by: "crates/db/tests/vector/edge_selection.rs robust_prune_keeps_the_edges_one_at_a_time_checking_keeps",
    },
    Property {
        family: "DiskANN back-edges in place",
        name: "in_place_eq_decoded — adding back-edges to an encoded record in place leaves the entries and count that decoding, adding and encoding would",
        axis: Lean,
        anchor: "crates/db/src/index/vector/engine/diskann/codec.rs add_edges; crates/db/src/index/vector/engine/diskann/graph.rs add_back_edges",
        model_ref: "BackEdgesInPlace.in_place_eq_decoded / stale_search_duplicates_an_edge",
        tiers: &[Contract],
        bound_by: "crates/db/tests/vector/diskann_back_edges.rs adding_edges_in_place_writes_what_decoding_and_encoding_would",
    },
    Property {
        family: "Vector index link maintenance",
        name: "INV_NoLostLinks — blind additions preserve links and stale structural pruning aborts",
        axis: Tla,
        anchor: "crates/storage/src/backends/regolith/merge/vector.rs; crates/db/src/index/vector/engine/hnsw/insert.rs; crates/db/tests/vector/concurrent_hnsw.rs",
        model_ref: "MC_VectorLinks_Green.cfg / MC_VectorLinks_Red_Puts.cfg / MC_VectorLinks_Red_Prune.cfg",
        tiers: &[Contract],
        bound_by: "crates/db/tests/vector/concurrent_hnsw.rs overlapping_inserts_keep_shared_links_and_metadata_counts, structural_pruning_conflicts_with_a_new_link",
    },
    Property {
        // Two writers appending to one collection without seeing each other must
        // both land, leaving two tips for a later merge: that is what makes the
        // head set a CRDT rather than a last-writer-wins cell. The eager-delete
        // strategy makes them write the same key, which regolith refuses at
        // every isolation level, so the RED configuration is the defect. The
        // GREEN one is the per-key design that preceded the head-set
        // operator; the tree reaches the same conclusion by blind merges into
        // one key, and overlapping_appends_both_commit checks it.
        family: "Concurrent collection-head transitions",
        name: "INV_NoWriteConflict — concurrent appends to one collection both commit and form siblings",
        axis: Tla,
        anchor: "crates/db/src/block/builder/collection.rs write_collection_block; crates/db/src/block/heads.rs publish_collection_block; crates/storage/src/backends/regolith/transaction.rs; crates/db/tests/block/heads.rs overlapping_appends_both_commit",
        model_ref: "MC_HeadSet_Green.cfg (GREEN) / MC_HeadSet_Red_EagerDelete.cfg (RED) / MC_HeadSet_Red_PerKeyScan.cfg (RED)",
        tiers: &[Behavioral],
        bound_by: "crates/db/tests/block/heads.rs overlapping_appends_both_commit",
    },
    Property {
        // A replicated block is published into its document's head set once:
        // its priority index entry records the publication, so a replay never
        // makes a superseded head live again, and a parent held ahead is read
        // by its own part of the set, so the parent's concurrent publication
        // (and nothing unrelated) refuses that decision. No guard key exists.
        family: "Concurrent collection-head transitions",
        name: "INV_LiveExact / INV_AheadExact / INV_NoUnrelatedAbort — document head publication without guard keys",
        axis: Tla,
        anchor: "crates/db/src/block/heads/document.rs publish/was_published; crates/storage/src/backends/regolith/merge/head_set.rs part/touches; crates/db/tests/block/document_heads.rs, crates/db/tests/merge/head_publication.rs",
        model_ref: "MC_HeadPublication_Green.cfg (GREEN) / MC_HeadPublication_Red_{NoRecord,OwnUnvalidated,NoPartRead,WholeSet}.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/db/tests/block/document_heads.rs replay_of_a_superseded_parent_cannot_resurrect_it, holding_a_parent_ahead_conflicts_with_its_publication, an_unrelated_publication_does_not_turn_away_a_hold; crates/db/tests/merge/head_publication.rs a_replayed_create_after_a_local_update_stays_superseded",
    },
    Property {
        // The algebra the temporal model rests on, for the per-key design:
        // every key a writer writes names that writer, so two writers cannot
        // collide (disjointness), and the eager strategy demonstrably does
        // collide (non-vacuity). The tree keeps none of those keys; it binds to
        // this model only at reads, where head_queries_match_executable_lean
        // compares derivedHeads with live_collection_heads.
        family: "Concurrent collection-head transitions",
        name: "derivedHeads reads as live_collection_heads; derived_writeSets_disjoint / eager_writeSets_overlap / applyDerived_parents_not_head are about the per-key design",
        axis: Lean,
        anchor: "crates/db/src/block/heads.rs live_collection_heads, publish_collection_block; crates/db/src/block/builder/collection.rs write_collection_block; crates/db/tests/block/head_conformance.rs head_queries_match_executable_lean",
        model_ref: "HeadSet.Core (lake build)",
        tiers: &[Contract],
        bound_by: "crates/db/tests/block/head_conformance.rs head_queries_match_executable_lean",
    },
    Property {
        family: "Commit-ordered document arrivals",
        name: "dense positions / commit order / at-most-once / snapshot prefix",
        axis: Tla,
        anchor: "crates/db/src/event/arrivals.rs record; crates/storage/src/backends/regolith/transaction.rs append; crates/db/tests/read/arrival_transactions.rs",
        model_ref: "regolith@e844baefe838f3af76403afd46c4b42f1a0b98a5 proofs/tla/CommitOrderedAppend.tla (GREEN Groups/Pipeline/Faults, RED Counter/OnceFromView/AssignBeforeValidation)",
        tiers: &[Behavioral],
        bound_by: "crates/db/tests/read/arrival_transactions.rs reversed_concurrent_commits_receive_cursors_in_commit_order, rollback_is_atomic_and_duplicate_arrivals_do_not_advance, cursor_survives_database_reopen",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_ProviderOncePerContext / INV_NewRegistrationSurvivesOldAck / ACT_NewRegistrationSurvivesOldClear",
        axis: Tla,
        anchor: "crates/query/src/error.rs QueryError::NeedsInput; crates/db/src/write/autocommit/create.rs create_many_with_resolved_inputs; update.rs update_with_projected_resolved_inputs; delete.rs delete_with_resolved_inputs; crates/db/src/write/create.rs embed, compute_many; crates/query/src/prepared.rs; crates/storage/src/stores/peerstore.rs register/complete/clear retry scope",
        model_ref: "MC_RetryPreparedInputs_{Green,Red_*}.cfg / MC_RetryMarkerGeneration_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_SweepReadsBounded / LIVE_VanishedMarkerDischarged — a sweep tick reads one probe and at most one page of markers; a purged document's marker is discharged under scheduler fairness only",
        axis: Tla,
        anchor: "crates/p2p-adapter/src/retry.rs run_retry_pass; crates/storage/src/stores/peerstore.rs get_retry_page, load_scope_markers, reschedule_retry_peer_for_incarnation; crates/db/src/merge/push_docs.rs retry_doc; crates/db/src/merge/push_docs_common.rs complete_document_retry_if_absent",
        model_ref: "MC_RetryMarkerGeneration_{Green,Red_UnboundedReads,Red_KeepVanished,Red_NoRotation}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_QueuedPushOwnsLiveMarker — admission requires current durable ownership and elapsed backpressure",
        axis: Tla,
        anchor: "crates/storage/src/stores/peerstore.rs admit_push_scope; crates/p2p-adapter/src/retry.rs spawn_failure_recorder; crates/p2p/src/sync/coordinator/push_worker.rs run_push_job",
        model_ref: "MC_RetryPushAdmission_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "ACT_RegistrationCannotCrossFence — allocated registrations preserve peer incarnation and marker order",
        axis: Tla,
        anchor: "crates/storage/src/stores/peerstore.rs RegisterScopeMarkerAttempt; create_replicator",
        model_ref: "MC_RetryRegistrationFence_{Green,Red_NoEpoch,Red_NoOrder}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_FanOutPeerLocal — one replicator's rejected registration leaves every other replicator's marker and job",
        axis: Tla,
        anchor: "crates/p2p/src/sync/coordinator/broadcast.rs admit_replicator_push; broadcast_tests.rs fanout_registration_failure_is_peer_local",
        model_ref: "MC_RetryRegistrationFence_{Green,Red_FanOutAbort}.cfg",
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
        anchor: "crates/storage/src/stores/retry_info.rs RetryInfo::restart_after_delivery, RetryInfo::bounded_by, RetryInfo::bump_with_schedule; crates/storage/src/stores/peerstore.rs PeerstoreRetryAttempt::{CompleteMarker, Reschedule, Restart, Activate}, RegisterScopeMarkerAttempt, Peerstore::load_scope_markers; crates/p2p-adapter/src/retry.rs run_retry_pass, record_failure; crates/p2p/src/sync/coordinator/push_worker.rs run_push_job; crates/db/src/merge/push_docs_common.rs complete_document_retry_if_current",
        model_ref: "MC_RetrySchedule_{Green,Red_KeepRung,Red_SupersededKeepsRung,Red_ReplayKeepsRung,Red_RegisterRestarts,Red_ActivateRestarts,Red_ClockBackHolds}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Retry input and marker fencing",
        name: "restart_rung_le_one / restart_never_later / restart_within_first_rung / restart_respects_backpressure / bump_climbs",
        axis: Lean,
        anchor: "crates/storage/src/stores/retry_info.rs RetryInfo::restart_after_delivery, RetryInfo::bump_with_schedule",
        model_ref: "RetrySchedule (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Collection schema-version OCC",
        name: "INV_NoStaleSchemaWrite — a writer cannot commit against an obsolete structural version",
        axis: Tla,
        anchor: "crates/db/src/txn/mod.rs validate_collection_for_write; crates/db/src/write/autocommit/{create,update,delete}.rs; crates/db/src/definition structural updates",
        model_ref: "MC_CollectionVersionOCC_Green.cfg / MC_CollectionVersionOCC_Red_NoValidation.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Explicit transaction statement atomicity",
        name: "INV_AbortedEffectsRemoved / INV_SuccessfulEffectsRetained / INV_NoCommitWhileCleanupPending",
        axis: Tla,
        anchor: "crates/query/src/runner/mutation.rs explicit transaction mutation execution; crates/db/src/txn/mod.rs statement savepoint; crates/storage/src/backends/regolith/transaction.rs savepoint callbacks",
        model_ref: "MC_StatementAtomicity_Green.cfg / MC_StatementAtomicity_Red_*.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "B3 filtered replication",
        name: "INV_DagComplete — filtered push still converges per-document",
        axis: Tla,
        anchor: "crates/p2p filtered replication; crates/db (merge)",
        model_ref: "MC_S4_ModelB.cfg / M1Convergence.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/replication.rs two_node_replication_converges",
    },
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
        anchor: "crates/db (merge) merge handler; crates/blockstore; crates/p2p/src/sync/coordinator/event_handler/mod.rs handle_peer_connected; crates/p2p-adapter/src/retry.rs run_retry_pass",
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
        anchor: "crates/p2p/src/sync/coordinator/event_handler/mod.rs handle_peer_connected; crates/p2p-adapter/src/retry.rs run_retry_pass",
        model_ref: "MC_Conv_NoHeadRediscovery.cfg (RED)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "CRDT merge laws",
        // Generic CrdtField core (DefraConvergence.CrdtField): comm+assoc => the merge
        // fold is order-independent (every field). Idempotence is the dividing line:
        // LWW (lwwMerge) is idempotent => a join-semilattice, re-delivery-safe, no dedup
        // (lww_dup_safe); the counter (Int +, including PNCounter negative deltas)
        // is NOT idempotent (counter_not_idempotent) => it must apply each delta
        // exactly once, the algebraic root of the #4935 double-apply. Counter, LWW,
        // and the mixed Counter×LWW product fully instantiate the core (counterCM /
        // lwwCM / mixedCM). PNCounter is the same signed-delta algebra with
        // decrement enabled, covered by singleStore_pncounter_converges and exact
        // live/restart/storm behavioral tests. The mixed product's cross-field
        // materialization hazard is checked by MixedFieldMaterialization.
        name: "CrdtField: comm+assoc => order-independent; PNCounter refines to signed Counter; mixed Counter×LWW inherits dedup",
        axis: Lean,
        anchor: "crates/crdt/src/field/lww.rs LwwField::merge; crates/crdt/src/field/counter.rs CounterField::merge; crates/crdt/src/stored.rs StoredDocument::apply componentwise field merge",
        model_ref: "DefraConvergence.MixedField (lake build); MC_MixedFieldMaterialization_Green.cfg",
        tiers: &[Contract, Behavioral],
        bound_by: "proofs/tests/lean_conformance.rs lean_merge_result_vocab_matches_rust; proofs/tests/behavioral/partition.rs convergence_concurrent_mixed_lww_and_counter_fields_merge, convergence_concurrent_pncounter_signed_deltas_sum",
    },
    Property {
        family: "CRDT merge laws",
        name: "PCounterGuard: the positive counter's op accepts only non-negative values, so its value never decreases under any run of accepted or refused writes whose sum stays inside i64 (an i64 sum wraps, as in Go)",
        axis: Lean,
        anchor: "crates/crdt/src/field/counter.rs CounterField::op; crates/crdt/tests/field_contract.rs only_the_positive_counter_refuses_a_negative_value",
        model_ref: "DefraConvergence.PCounterGuard.run_ge (lake build)",
        tiers: &[Contract],
        bound_by: "crates/crdt/tests/field_contract.rs only_the_positive_counter_refuses_a_negative_value",
    },
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
        anchor: "crates/p2p/src/replicator.rs; crates/db/src/merge/push_docs.rs push_existing_docs, retry_doc",
        model_ref: "MC_Replicator_Resumable_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/replicator_lifecycle.rs replicator_backfill_no_loss, replicator_resume_across_restart",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_ObligationConservation / INV_PendingServiceable / INV_PendingHasAuthenticatedProvider / INV_SingleFlight / INV_SingleMergeWriter / INV_SenderMarkersOnly",
        axis: Tla,
        anchor: "crates/p2p/src/sync coordinator + pending DAG clock; crates/db (merge) push replay; crates/storage peerstore retry markers",
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
        anchor: "crates/db/src/merge/txn_broadcaster.rs SyncTxnBroadcaster::submit_update; crates/db/src/event/emission.rs register_update_event_callback; crates/defra-node/src/lib.rs Node::shutdown",
        model_ref: "MC_SyncOwnership_{Green_MarkerHandoff,Red_UndrainedMarker}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_EventualCurrency / LIVE_EventualReceiverQuiescence — document scope",
        axis: Tla,
        anchor: "crates/p2p/src/sync coordinator + pending DAG clock; crates/db (merge) document marker rederive",
        model_ref: "MC_SyncOwnership_Green_Liveness_Doc.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_EventualCurrency / LIVE_EventualReceiverQuiescence — collection scope",
        axis: Tla,
        anchor: "crates/p2p/src/sync coordinator + pending DAG clock; crates/db (merge) collection marker rederive",
        model_ref: "MC_SyncOwnership_Green_Liveness_Collection.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_FetchHasQualifiedProvider / LIVE_EventualCurrency — immutable origin plus durable same-root alternate",
        axis: Tla,
        anchor: "crates/p2p/src/sync/manager/process/pushlog.rs; crates/p2p/src/sync/pending_store/mod.rs; crates/p2p/src/sync/coordinator/mod.rs",
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
        anchor: "crates/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag, FetchTaskEnd; crates/p2p/src/sync/coordinator/dag_retry.rs CLOCK_ATTEMPT_QUANTUM; crates/p2p/src/sync/replication/handlers.rs handle_dag_needs_fetch; crates/p2p/src/sync/coordinator/dag_context.rs DagFetchContext::finish_clock_attempt; crates/p2p/src/sync/manager/process/pending_dag.rs try_claim_pending_dag_dispatch, PendingDagLease::finish_attempt, due_pending_dag_retries; crates/p2p/src/sync/manager/pending.rs PendingDag::is_expired; crates/p2p/src/sync/manager/config.rs SyncConfig::pending_dag_ttl; crates/p2p/src/sync/manager/pending.rs PENDING_DAG_TTL; crates/p2p/src/sync/coordinator/mod.rs dispatch_due_pending_dag_fetches, DagFetchLimiter::try_reserve_peer, DagFetchLimiter::acquire",
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
        anchor: "crates/p2p/src/sync/manager/event_lanes.rs DispatchLane, SyncEventReceiver::recv; crates/p2p/src/sync/coordinator/mod.rs dispatch_due_pending_dag_fetches; crates/p2p/src/sync/manager/process/pending_dag.rs retry_pending_dag",
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
        anchor: "crates/p2p/src/sync/manager/process/pending_dag.rs PendingDagLease::finish_attempt, records_after_resync_cursor, resync_persisted_pending_dags_inner, try_insert_pending_dag, max_pending_dags_per_peer, continue_merge, consume_pending_merge_continuation, MergeTurn; crates/p2p/src/sync/manager/pending.rs PendingOrigin, PENDING_ROTATE_AFTER_DISPATCHES; crates/p2p/src/sync/replication/handlers.rs handle_block_received, process_merge_batch",
        model_ref: "MC_SyncOwnership_{Green_Memory,Green_Memory_Quota,Red_RetainedOverflow,Red_KeyOrderResync,Red_QuotaPassOver,Red_ImmortalContinuation}.cfg",
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
        anchor: "crates/p2p/src/sync/replication/handlers.rs forward_merged, forward_owed, handle_block_received, process_merge_batch; crates/defra-core/src/merge.rs MergeOutcome::skip_reason_is_applied; crates/p2p/src/sync/replication/forward_tests.rs; crates/db/tests/merge/head_publication.rs",
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
        anchor: "crates/p2p/src/sync/push_backlog.rs BacklogOwner::enqueue, pop_eligible, park_peer, park_peer_unreachable, peer_answered, retire_idle_cooldowns, take_queued_for_peer; crates/p2p/src/sync/coordinator/push_worker.rs spawn_push_workers, run_push_job",
        model_ref: "MC_PushBacklog_{Green,Green_TwoDead,Green_SharedCap,Red_TwoDead,Red_DemoteEveryTimeout,Red_NoPeerCap,Red_SpawnPerItem,Red_PermitLeak,Red_RetainHandles}.cfg",
        tiers: &[Boundary],
        bound_by: "",
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
        anchor: "crates/p2p/src/sync/manager/process/pushlog.rs process_block_inner; crates/p2p/src/sync/manager/process/pending_dag.rs durable_share_admits, admits_durable_only, max_persisted_pending_per_peer, try_insert_pending_dag; crates/p2p/src/sync/pending_store/store.rs take_new",
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
        anchor: "crates/p2p/src/sync/pending_store/store.rs register, take_new; crates/p2p/src/sync/pending_store/shares.rs; crates/p2p/src/sync/manager/process/pending_dag.rs restored_root_fits, resync_persisted_pending_dags_inner",
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
        anchor: "crates/p2p/src/sync/manager/process/pushlog.rs process_pushlog_with_provider_evidence, assert_pushlog_left_durable_state, process_block_inner, Processed; crates/p2p/src/sync/pending_store/store.rs register; crates/p2p/src/sync/pending_store/view.rs PersistedView::registered",
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
        anchor: "crates/p2p/src/sync/pending_store/store.rs register, take_new; crates/p2p/src/sync/pending_store/shares.rs held, charge, hand_over, give_back; crates/p2p/src/sync/manager/process/pending_dag.rs durable_share_admits",
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
        anchor: "crates/p2p/src/sync/manager/process/pushlog.rs process_pushlog_with_provider_evidence; join.rs join_in_flight, needs_nothing_more; crates/p2p/src/sync/queue.rs ProcessQueue::try_acquire",
        model_ref: "MC_PushLogJoin_{Green,Red_NoRecheck,Red_NoGuard,Red_NoDeadline}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "acquire_exclusive / join_acks_only_durable / join_processes_only_unowned: the CID guard is exclusive and a join acks only a safe block",
        axis: Lean,
        anchor: "crates/p2p/src/sync/queue.rs ProcessQueue::try_acquire, release_sync; crates/p2p/src/sync/manager/process/join.rs join_in_flight; crates/p2p/src/sync/coordinator/mod.rs DagFetchLimiter::claim_root, RootClaim::drop",
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
        anchor: "crates/p2p/src/sync/head_scope.rs ScopeDecision::of; crates/p2p/src/sync/manager/process/pushlog.rs process_block_inner, assert_pushlog_left_durable_state; crates/p2p/src/sync/manager/process/pending_dag.rs try_insert_pending_dag; crates/p2p/src/sync/pending_store/store.rs register",
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
        anchor: "crates/p2p/src/sync/push_backlog.rs BacklogOwner::enqueue; crates/p2p/src/sync/coordinator/push_worker.rs run_push_job, is_whole_frontier; crates/storage/src/stores/peerstore.rs refresh_push_failure_with_generation, complete_retry_scope, record_acknowledged_heads",
        model_ref: "MC_PushCoalescing_{Green,Green_Siblings,Red_AppendEvery,Red_StaleRetry,Red_SiblingClear,Red_UnfencedAck}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Rejected is assumed content-determined. Under strict (Vera) ACP a
        // denial depends on the local relationship view, so an update whose
        // grant has not arrived yet is quarantined for good, after its
        // sender's marker was discharged. Local ACP cannot reach this.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_NoSilentDrop / LIVE_SoundEventuallyMerged / LIVE_PoisonQuiesces assume a rejection is content-determined",
        axis: Tla,
        anchor: "crates/db/src/merge/acp_merge_handler.rs on_protected_update; crates/p2p/src/sync/coordinator/merge_state.rs quarantine_pending_dag",
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
        anchor: "crates/p2p/src/sync/manager/process/pushlog.rs process_block_inner, extend_registered_root; crates/p2p/src/sync/pending_store/mod.rs",
        model_ref: "MC_PendingDagRestart_{Green,Red_ProcessLocal}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Per-field LWW is the DefraDB data model: fields are not atomic across
        // a record, so a converged claim can mix different claimants' fields.
        // Consumers fence on one field (gents: execution_generation).
        family: "Multi-instance claim",
        name: "INV_EventualUnique per field — each AgentRequest field converges to one LWW winner (priority, then bytes)",
        axis: Tla,
        anchor: "crates/crdt/src/field/lww.rs LwwField::merge; gents:crates/gents/src/request/lifecycle/claim.rs claim_inner (foreign substrate, not the defradb.rs binary)",
        model_ref: "MC_Claim_Filtered_Eventual.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Verify-before-merge fires only on the P2P merge path; injecting a
        // forged/unsigned block needs an adversarial peer, not reachable through
        // the public CLI. Rust makes signature verification mandatory (model->code
        // audit) and EUF-CMA is a named crypto boundary, so this is structural.
        family: "Block integrity / signatures",
        name: "INV_OnlyVerifiedMerged — verify-before-merge, author bound to verified DID",
        axis: Tla,
        anchor: "crates/db (merge) sig verify; crates/defra-core/src/batch_signing.rs",
        model_ref: "MC_Integrity_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The unauthorized-denied leg is now observable: a serve-side
        // `tracing::warn!("DEK release DENIED")` at the KMS policy check makes the
        // previously-silent denial loggable, and the behavioral test reads node0's
        // log to confirm node0 denies the DEK to an unauthorized node1's fetch.
        // ECIES secrecy of a released envelope remains a crypto Boundary (proven
        // by crates/kms ecies_envelope unit tests).
        family: "KMS key distribution",
        name: "INV_AuthorizedEventuallyGets / no-unauthorized-usable",
        axis: Tla,
        anchor: "crates/kms/src/defra_kms.rs serve_request; crates/kms/src/nac_dac_policy.rs",
        model_ref: "MC_Kms_Green.cfg",
        tiers: &[Behavioral, Boundary],
        bound_by: "proofs/tests/behavioral/kms.rs kms_unauthorized_dek_release_denied",
    },
    Property {
        // A merge that needs a DEK fetches it inline on the serial merge
        // executor, blocking unrelated merges for up to RESPONSE_TIMEOUT (5 s,
        // plus up to SUBSCRIBER_WAIT_TIMEOUT, 10 s, when no subscriber is
        // known). KeyUnavailable is retryable, so an unauthorized replica pays
        // this on every re-drive.
        family: "KMS key distribution",
        name: "LIVE_UnrelatedMergeProgress is not claimed: a merge awaiting a DEK holds the merge executor",
        axis: Tla,
        anchor: "crates/db/src/merge/merge_handler/dispatch.rs merge_with_retries; crates/p2p/src/kms/pubsub_transport.rs RESPONSE_TIMEOUT, SUBSCRIBER_WAIT_TIMEOUT",
        model_ref: "MC_EncryptedLwwReplay_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A write asks the KMS once per key scope: every field under document
        // encryption shares the document key, and a field key is never it.
        // Exercised by the testo scenarios
        // encryption::field_commit::*_should_use_dedicated_enc_key_for_individual_fields.
        family: "KMS key distribution",
        name: "DocKeyScopes.same_scope_same_key / different_scope_different_key",
        axis: Lean,
        anchor: "crates/db/src/write/prepare.rs KMS key preparation (`generated` memo)",
        model_ref: "DocKeyScopes (lake build)",
        tiers: &[Behavioral],
        bound_by: "tools/integration-test/tests/encryption_scenarios/field_commit.rs doc_encryption_field_with_doc_and_field_encryption_should_use_dedicated_enc_key_for_individual_fields",
    },
    Property {
        // Encryption reuses PriorityReconcile.lwwCM unchanged: ciphertext and
        // key timing affect when a value can materialize, not which value wins.
        // The behavioral legs preserve encrypted fields selected for filtered
        // replication and receive both encrypted siblings after a real restart,
        // proving identical DAGs and the decrypted winner.
        family: "Encrypted LWW restart/replay",
        name: "INV_NoFilteredLoss / INV_LwwWinner — encrypted delivery remains complete and LWW-convergent",
        axis: Tla,
        anchor: "crates/db/src/merge/push_docs.rs; crates/p2p/src/sync/coordinator/selective_car_access.rs; crates/crdt/src/field/lww.rs",
        model_ref: "MC_EncryptedLwwReplay_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/partition.rs convergence_encrypted_lww_restart_merge",
    },
    Property {
        // The model and focused KMS/merge unit tests cover the retry mechanics,
        // but the external harness cannot deterministically hold a DEK request
        // unavailable between ciphertext acknowledgement and receiver restart.
        family: "Encrypted LWW restart/replay",
        name: "INV_AckBacked — acknowledged encrypted replay remains re-drivable across transient KMS failure and restart",
        axis: Tla,
        anchor: "crates/db/src/merge/merge_handler/composite_fields.rs; crates/p2p/src/sync/pending_store/mod.rs",
        model_ref: "MC_EncryptedLwwReplay_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Management-channel auth (NAC gate)",
        name: "INV_OnlyAuthorizedManages — management ops require a valid scoped token",
        axis: Tla,
        anchor: "crates/http manage channel; crates/db (nac)",
        model_ref: "MC_Auth_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/nac.rs nac_management_requires_admin",
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_RevocationConsistent + both User and _commits paths gated",
        axis: Tla,
        anchor: "crates/acp; crates/zanzibar; crates/query dag-scan",
        model_ref: "MC_Acp_Green.cfg / MC_Commits_Green.cfg",
        tiers: &[Behavioral, Contract],
        bound_by: "proofs/tests/behavioral/acp.rs acp_dual_path_gating_and_revocation; proofs/tests/lean_conformance.rs lean_relation_expression_vocab_matches_rust",
    },
    Property {
        // The light client checks an answer at the newest root it has
        // authenticated, and refuses one that a block finalized during the
        // round trip overtook. The vera provider asks such a read again, at
        // most ATTEMPTS times: the refusal reaches a caller only after a block
        // overtook every ask, and a stale answer is never accepted.
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_NeverStale / INV_FailsOnlyWhenEveryAskOvertaken / INV_AttemptsBounded / LIVE_Answers: a proven read the chain overtook is asked again, bounded, never accepted stale",
        axis: Tla,
        anchor: "crates/vera/src/vera_rs/provider/verified.rs verified, ATTEMPTS; crates/vera/src/vera_rs/provider.rs create_policy, query_policy, query_object_owner, verify_access, create_access_decision; backbone:acp-light-client HeaderChain::accept_response",
        model_ref: "MC_VerifiedRead_{Green,Green_Exhausted,Red_AcceptStale,Red_NoRetry,Red_Uncapped}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Registrations of one document that race each write their own owner
        // key, so only the commit can refuse all but one. The deciding prefix
        // scan is a validated scan: a registration that commits under the
        // prefix after another's snapshot makes the other conflict, and it
        // returns false. The sentinel byte it replaces never conflicted on
        // regolith, which commits a write that leaves a key as it was.
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_OneOwner / INV_WinnersOwn / INV_SomeoneWins: racing registrations of one document leave exactly one owner",
        axis: Tla,
        anchor: "crates/acp/src/persistent.rs PersistentAcpStore::register_doc_atomic, delete_doc_tuples; crates/acp/src/store.rs AcpStore::register_doc_atomic",
        model_ref: "MC_RegisterDocOnce_{Green,Red_NoValidation,Red_Sentinel}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Peers that merged the same blocks show the same text, a peer's text
        // built one block at a time equals a replay from scratch, every edit
        // lands at its position in its own parent version, and a run typed at
        // one spot is never split by a concurrent one; a write of null reads
        // null exactly while every head writes null. The crdt tests check the
        // same invariants on random DAGs; the db tests merge real blocks
        // between two nodes.
        family: "Text CRDT convergence",
        name: "INV_Converge / INV_MatchesReplay / INV_Intent / INV_PlacedAsIntended / INV_NoInterleave / INV_CriticalReplay / INV_NullConverge / INV_NullMatchesReplay / INV_NullEmpty: Fugue text converges, keeps every edit where its author put it, never interleaves runs, replays a concurrent merge from its critical block, and reads null only when every head writes null",
        axis: Tla,
        anchor: "crates/crdt/src/fugue.rs Tree::apply, Tree::attach, replay, merge_text; crates/crdt/src/field/text.rs TextField (block_data, CausalCrdt diff, block_op, replay, merge_from, check_version, nil_heads); crates/db/src/merge/merge_handler/field_causal.rs merge_field_causal, critical_block, causal_history; crates/db/src/write/autocommit/helpers.rs apply_field_ops; crates/db/src/definition/migration/helpers.rs changes_block_state; crates/crdt/src/fugue_tests.rs, crates/crdt/src/field/testing.rs check_causal, crates/db/tests/merge/text_merge.rs, crates/db/tests/definition/migration_suite/text_lens.rs",
        model_ref: "MC_TextCrdt_{Green,Red_ArrivalOrder,Red_NoCausalBuffer,Red_CurrentText,Red_NextNode,Red_CommonAncestor,Red_NullLast}.cfg",
        tiers: &[Contract],
        bound_by: "crates/db/tests/merge/text_merge.rs concurrent_edits_of_one_text_keep_both_on_every_node; crates/crdt/src/fugue_tests.rs concurrent_inserts_at_one_spot_keep_each_run_whole_in_id_order, an_edit_concurrent_with_a_delete_still_finds_its_spot",
    },
    Property {
        // Fugue's rule puts an inserted character right after its left origin
        // and moves nothing else; a delete hides only its character; two
        // concurrent characters hung in either order give the same tree; a
        // placeholder run kept as one span and cut where an edit lands is the
        // same tree as its chain of characters.
        family: "Text CRDT convergence",
        name: "place_visible / delete_visible / attach_comm / split_run: intention preservation and arrival-order independence of the Fugue tree, and span cuts that change nothing",
        axis: Lean,
        anchor: "crates/crdt/src/fugue.rs Tree::apply, Tree::attach, Tree::traverse, Tree::split",
        model_ref: "TextCrdt.{Fugue,Span} (lake build)",
        tiers: &[Contract],
        bound_by: "crates/crdt/src/fugue_tests.rs a_linear_history_is_its_ops_applied_in_turn, concurrent_inserts_at_one_spot_keep_each_run_whole_in_id_order, concurrent_deletes_of_one_character_remove_it_once",
    },
    Property {
        // The JSON CRDT (#2129). Each path is a register: racing edits resolve
        // by priority, then block ID; an edit wipes only what it had seen below
        // it, so an update beats a concurrent delete; only a set below a path
        // makes it a map, so a delete creates nothing; the conflicts query
        // lists exactly the racers that lost. The defra-core tests check the
        // same invariants on block DAGs; the integration tests merge real
        // blocks between two nodes.
        family: "JSON CRDT per-path merge",
        name: "INV_Converge / INV_MatchesReplay / INV_DisjointKept / INV_SamePathLww / INV_UpdateWins / INV_TypeClash / INV_ConflictsAreLosers / INV_DeleteCreatesNothing / INV_WriteLandsAsRead: per-path JSON merge converges, keeps every edit nothing saw, never lets a delete create a path, keeps two signers' equal edits apart by their nonces, and lands every write, several paths at once included, as its writer reads it",
        axis: Tla,
        anchor: "crates/defra-core/src/json_crdt.rs Frontier::delta, Frontier::compose, Frontier::materialize, Frontier::conflicts, replay; crates/crdt/src/field/json.rs JsonField (TaggedCrdt: write_data, effect, replay), block_ops, settle_earlier, apply_ops, landed, mismatch; crates/defra-core/src/json_crdt.rs encode_write, decode_ops; crates/crdt/src/field/tagged.rs local_write, land; crates/db/src/merge/merge_handler/field_tagged.rs merge_field_tagged, tagged_past; crates/db/src/write/autocommit/helpers.rs prepare_tagged_writes; crates/db/src/definition/migration/helpers.rs changes_block_state; crates/defra-core/src/json_crdt_tests.rs, crates/crdt/tests/json_field.rs, crates/db/tests/merge/json_merge.rs, tools/integration-test/tests/p2p/json_crdt.rs",
        model_ref: "MC_JsonCrdt_{Green_FirstValueOrMap,Green_FirstListOrDelete,Green_MultiPath,Red_DeleteWins,Red_NoCid,Red_DeleteImpliesMap,Red_SharedId,Red_NoAnchor,Red_NoSettle,Red_DeleteMakesParent,Red_MapKeepsBelow}.cfg",
        tiers: &[Contract],
        bound_by: "crates/db/tests/merge/json_merge.rs equal_concurrent_json_edits_from_two_signers_converge_in_any_order; crates/crdt/tests/json_field.rs json_meets_the_contract; crates/defra-core/src/json_crdt_tests.rs disjoint_keys_both_survive_in_any_order, an_update_beats_a_concurrent_delete_above_it",
    },
    Property {
        // A path's register keeps the same winner whatever order edits arrive
        // in, or if one arrives twice; a path's key reads back as the path, and
        // one key starts another only when its path is above the other's; the
        // keys under a path are one stretch of the frontier's sorted entries.
        family: "JSON CRDT per-path merge",
        name: "join_comm / join_assoc / join_idem / decode_encode / encode_prefix / prefix_convex: order-free registers, nesting path keys, and one range per subtree",
        axis: Lean,
        anchor: "crates/defra-core/src/json_crdt.rs beats, encode_path, decode_path, Frontier::under, Frontier::at",
        model_ref: "JsonCrdt.{Lww,Path} (lake build)",
        tiers: &[Contract],
        bound_by: "crates/defra-core/src/json_crdt_tests.rs diff_then_merge_gives_the_new_value, disjoint_keys_both_survive_in_any_order, one_path_resolves_by_priority_then_block_and_lists_the_loser",
    },
    Property {
        // Custom CRDT types (#2183). A node without a registered type merges
        // the rest of each block, keeps the custom block and a record of it,
        // and acknowledges it; built again with the type, it merges each
        // parked block once, removing its record in the same transaction. A
        // block whose only part was parked still makes the document, and a
        // replay that fails for now (a store error, a key this node does not
        // hold) leaves its block parked. A node storing a value of a type
        // refuses to start without it. The defra-node custom_crdt tests run
        // the same story between real nodes.
        family: "Custom CRDT types",
        name: "INV_NoBlindMerge / INV_MergeOnce / INV_OtherFieldsExact / INV_ConvergeWhenDone / LIVE_Converge: an unregistered type is never merged, a parked block merges exactly once, other fields stay exact, and replicas converge once the type is registered",
        axis: Tla,
        anchor: "crates/crdt/src/field/registry.rs Crdts::delta, Crdts::field, Crdts::part; crates/db/src/merge/merge_handler/field.rs merge_field_delta_in_txn; crates/db/src/merge/merge_handler/composite_fields.rs process_linked_field_blocks; crates/db/src/merge/merge_handler/composite_persist.rs persist_merged_document; crates/db/src/merge/merge_handler/parked.rs park, replay_parked, replay_in_txn, DB::start_parked_replay; crates/db/src/merge/merge_handler/error.rs MergeError::fails_block, MergeError::needs_key; crates/db/src/merge/merge_handler/merge_keys.rs with_local_history_keys; crates/db/src/database/stored_crdts.rs record, DB::check_stored_crdts; crates/defra-node/tests/custom_crdt/replication.rs, crates/db/tests/merge/parked_errors.rs",
        model_ref: "MC_UnknownCrdt_{Green,Red_DropDeferral,Red_DoubleReplay,Red_Lww,Red_RemoveStored,Red_NoDocument,Red_FailStall}.cfg",
        tiers: &[Contract],
        bound_by: "crates/defra-node/tests/custom_crdt/replication.rs a_node_without_the_type_parks_its_blocks_until_it_has_the_type; crates/db/tests/merge/parked_errors.rs a_parked_block_fails_only_for_an_error_of_its_own",
    },
    Property {
        // Every existing type keeps its definition bytes (code, no name), so
        // definition CIDs do not move; a custom type is code 8 plus its name;
        // reading back what a type writes gives that type. The defra-node
        // definition_cids golden test pins the CIDs themselves.
        family: "Custom CRDT types",
        name: "native_unchanged / custom_not_native / decode_encode / encode_injective / old_codes_read_back: custom types leave native definitions byte-identical and round-trip",
        axis: Lean,
        anchor: "crates/schema/src/ctype.rs CType::to_u8, CType::custom_name, CType::from_definition; crates/schema/src/cid.rs field_to_delta_with_priority; crates/defra-core/src/block_delta.rs FieldDefinitionDeltaPayload; crates/defra-node/tests/definition_cids.rs",
        model_ref: "CustomCrdtCode (lake build)",
        tiers: &[Contract],
        bound_by: "crates/defra-node/tests/definition_cids.rs native_definitions_keep_their_ids",
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
        anchor: "crates/p2p/src/ack_scope.rs AckScope::new; crates/p2p/src/replicator.rs ReplicatorInfo::ack_scope; crates/p2p/src/sync/coordinator/broadcast.rs SyncCoordinator::push_jobs; crates/db/src/merge/replication_ack.rs replication_acknowledged",
        model_ref: "MC_ReplicationAckScope_{Green,Red_MainRule,Red_PushToFiltered}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A transaction ID travels in the request context. Only the caller
        // that opened the transaction runs in it, a read-only transaction
        // refuses writes, a finished transaction runs nothing, a discard
        // leaves nothing in the store, and a commit that clashes with a
        // newer commit fails.
        family: "Explicit transactions in the request context",
        name: "INV_OnlyOwnerRuns / INV_ReadOnlyNeverWrites / INV_FinishedNeverRuns / INV_DiscardLeavesNothing / INV_NoLostUpdate",
        axis: Tla,
        anchor: "crates/db/src/txn/registry/lifecycle.rs begin_as, run_as, commit_as, discard_as; crates/db/src/txn/owner/mod.rs Owners::run, Owners::finish; crates/defra-node/src/ops/request.rs txn_begin, txn_commit, txn_discard; crates/query/src/runner/turn.rs read-only rejection; tools/integration-test/tests/basic/txn_context.rs",
        model_ref: "MC_TxnContext_{Green,Red_NoOwnerCheck,Red_ReadOnlyWrites,Red_RunAfterEnd,Red_DiscardLeaks,Red_NoConflictCheck}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Collection patch, set-active, set-migration, add/list lens and get
        // collections each have one core function every edge calls: it
        // checks NAC before any effect, runs in the transaction the context
        // carries or else autocommits, and refuses a patch inside one.
        family: "Schema-evolution operations in the core",
        name: "INV_NoEffectWithoutPermission / INV_TxnNeverAutocommits / INV_PatchInTxnChangesNothing",
        axis: Tla,
        anchor: "crates/db/src/txn/registry/lifecycle.rs get_collections_as, patch_collection_as, set_active_collection_version_as, set_migration_as, add_lens_as, list_lenses_as; crates/defra-node/src/ops/collection.rs; crates/http/src/handlers/{collections,lens}.rs",
        model_ref: "MC_CoreDispatch_{Green,Red_NoNac,Red_IgnoreTxn}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Listing lenses returns every module of every lens, in order, as
        // Go's `model.Lens`; it used to keep only the first.
        family: "Schema-evolution operations in the core",
        name: "LensListing.listed_keeps_modules / listed_length",
        axis: Lean,
        anchor: "crates/lens/src/wasm.rs WasmTransformStore::list; crates/lens/src/store.rs list; crates/db/src/txn/registry/lifecycle.rs list_lenses_as",
        model_ref: "LensListing (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // An explicit transaction's ID is `sequence * workers + worker`. The
        // owner is the ID modulo the worker count and the sequence is the
        // quotient, so the round trip is exact for every worker below the
        // count, the owner is always a real worker, and two different
        // (sequence, worker) pairs never share an ID.
        family: "Explicit transactions in the request context",
        name: "TxnId.owner_encode / sequence_encode / owner_lt / encode_injective / decode_encode",
        axis: Lean,
        anchor: "crates/db/src/txn/owner/id.rs TxnId::new, TxnId::owner; crates/db/src/txn/owner/tests.rs",
        model_ref: "TxnId (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Storage transaction validation",
        name: "Validated point reads reject crossing writes",
        axis: Tla,
        anchor: "crates/storage/src/backends/regolith/transaction.rs; proofs/tests/behavioral/ssi.rs",
        model_ref: "regolith@e844baefe838f3af76403afd46c4b42f1a0b98a5 proofs/tla/DefraLevel.tla",
        tiers: &[Behavioral, Boundary],
        bound_by: "proofs/tests/behavioral/ssi.rs ssi_write_skew_prevented",
    },
    Property {
        // The replay-capability gate lives inside the P2P replication wire
        // protocol; presenting a forged/expired capability needs a custom peer,
        // not the public CLI. Rust-only hardening Go lacks; structural.
        family: "P2P explicit-replay capability gate",
        name: "INV_OnlyLegitAccepted — forged/expired/wrong-target capability rejected",
        axis: Tla,
        anchor: "crates/p2p capability replay gate",
        model_ref: "MC_Capability_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The pending entry, its durable record across a restart, and the
        // merge (the collection/creator check and the strict-ACP bypass) reuse
        // the admission verdict without re-checking expiry or revocation.
        // Only KMS key release verifies again. Revocations live in process
        // memory and are lost on restart.
        family: "P2P explicit-replay capability gate",
        name: "INV_TtlCapped / INV_RevokedNeverAccepted hold at PushLog admission only",
        axis: Tla,
        anchor: "crates/db/src/merge/merge_handler/authorization.rs validate_explicit_replay_authorization; crates/p2p/src/explicit_replay.rs",
        model_ref: "MC_Capability_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "NAC lifecycle privilege-escalation",
        name: "INV_NoPrivEsc — no escalation across enable/disable/restart",
        axis: Tla,
        anchor: "crates/db (nac) lifecycle",
        model_ref: "MC_Nac_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/nac_lifecycle.rs nac_lifecycle_no_escalation",
    },
    Property {
        // No-loss / no-double-apply under concurrent same-document mutation is now
        // BEHAVIORAL: `partition::convergence_concurrent_same_doc_merge_storm`
        // storms one PCounter doc from a 3-node mesh and asserts the exact sum
        // (below => a delta dropped, above => double-applied). Mixed-field legs
        // assert the product state (LWW name + counter views) after live, restart,
        // and 3-node full-mesh replay; `MC_MixedFieldMaterialization` proves the
        // stale whole-document commit hazard RED/GREEN.
        // The process-local document queue and counter-finalize gate are gone.
        // Ordinary application markers validate exactly-once decisions; counter
        // operands fold in the engine. Materialized mirrors retain validated
        // reads. The adapter test `interactive_counter_increment_conflicts_with_concurrent_same_doc_merge`
        // checks conflict/rollback/retry convergence. Engine OCC is a boundary;
        // the historical serialized MergeQueue model is deliberately retired.
        family: "Transaction & merge-queue concurrency",
        name: "INV_NoLoss / INV_NoDoubleApply under concurrent same-doc mutation",
        axis: Tla,
        anchor: "crates/db/src/merge/merge_handler/field_ops.rs apply_field_op_once; crates/db/src/write/doc.rs; crates/db/tests/merge/merge_handler_tests.rs; crates/db txn registry",
        model_ref: "MC_TxnRegistry_Green.cfg / MC_TwoStoreCounter_Green.cfg / MC_MixedFieldMaterialization_Green.cfg",
        tiers: &[Behavioral, Boundary],
        bound_by: "proofs/tests/behavioral/partition.rs convergence_concurrent_same_doc_merge_storm, convergence_concurrent_pncounter_same_doc_merge_storm, convergence_concurrent_mixed_lww_and_counter_fields_merge",
    },
    Property {
        // A counter block that arrives on its own before its document has a
        // value on the block's schema version is a terminal skip: no applied
        // key, no value, no merged mark. The composite that writes the value
        // applies it once. The db merge tests
        // `counter_merge_of_an_unmaterialized_document_waits_for_its_composite`
        // and `counter_merge_marks_cid_merged` check both arrival orders.
        family: "Transaction & merge-queue concurrency",
        name: "INV_NoLoss: a standalone counter block that arrives before its document's value is applied once, by its composite",
        axis: Tla,
        anchor: "crates/db/src/merge/merge_handler/field_ops.rs apply_field_op_once; crates/db/src/merge/merge_handler/batch.rs process_block_in_txn; crates/db/src/merge/merge_handler/composite_fields.rs; crates/db/tests/merge/merge_handler_tests.rs",
        model_ref: "MC_TwoStoreCounter_{Green,Red_LostDeferral}.cfg",
        tiers: &[Contract],
        bound_by: "crates/db/tests/merge/merge_handler_tests.rs counter_merge_of_an_unmaterialized_document_waits_for_its_composite, counter_merge_marks_cid_merged",
    },
    Property {
        // A commit begins when it is handed to its transaction's owner, or,
        // if a request is running the transaction then, when the owner takes
        // it; a caller that stops waiting before that abandons it. One that
        // began still lands after its caller stops waiting. The caller raises
        // the database's fence count
        // and swaps the commit's state word; the owner swaps it once it is
        // finished, and whichever swap loses lowers the count. Every new
        // transaction and read waits in DB::new_txn while the count is above
        // zero, so it sees the commit.
        family: "Transaction & merge-queue concurrency",
        name: "INV_ReaderSeesCancelledCommit / INV_NeverNegative / INV_UncancelledUntouched / INV_SettledExact / INV_BegunEndRuns / INV_AbandonedEndDiscarded / LIVE_NoFenceLeak / LIVE_ReaderProceeds: a cancelled commit that began is fenced until it lands; one that had not begun is abandoned",
        axis: Tla,
        anchor: "crates/db/src/txn/fence.rs CommitFences, Fence::abandon, Fence::finish; crates/db/src/txn/owner/mod.rs Owners::run, Owners::finish, FenceOnDrop; crates/db/src/txn/owner/actor.rs own; crates/db/src/database/mod.rs DB::new_txn",
        model_ref: "MC_CommitFence_{Green,Red_RaiseAfterSwap,Red_OwnerNoSwap,Red_ReaderNoWait,Red_ForgetFailed,Red_UndoNoWake,Red_SkipBegun,Red_RunAbandoned}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A request reaches an explicit transaction when its owner key is the
        // one the transaction began with. An HTTP registry keys a request by
        // its identity; the embedded node's registry gives every request one
        // key, since the embedding process is its only caller (#2335). Either
        // way the work runs as the request's own identity.
        family: "Transaction & merge-queue concurrency",
        name: "INV_FfiReaches / INV_HttpOwnerOnly / INV_ActsAsCaller: the embedder reaches its transactions as any identity, an HTTP caller only its own, and work runs as the request",
        axis: Tla,
        anchor: "crates/db/src/txn/registry/mod.rs TxnOwnership, DbTransactionRegistry::caller, DbTransactionRegistry::in_txn; crates/db/src/txn/registry/lifecycle.rs run_as; crates/db/src/txn/owner/table.rs Table::admits; crates/ffi/src/node.rs build_node_state; crates/defra-node/src/node_builder.rs NodeBuilder::with_txn_ownership",
        model_ref: "MC_TxnAdmission_{Green,Red_IdentityOnFfi,Red_EmbedderOnHttp,Red_ActsAsOwner}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Transaction & merge-queue concurrency",
        name: "embedder_admits_every / byIdentity_admits_iff / byIdentity_turns_away_stranger / admits_beginner / default_is_byIdentity",
        axis: Lean,
        anchor: "crates/db/src/txn/registry/mod.rs TxnOwnership, DbTransactionRegistry::caller; crates/db/src/txn/owner/table.rs Table::admits",
        model_ref: "TxnAdmission (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Transaction & merge-queue concurrency",
        name: "exactly_one_wins / one_lowering_per_raise / caller_win_means_owner_lowers / uncancelled_owner_wins: the two swaps settle each fence once",
        axis: Lean,
        anchor: "crates/db/src/txn/fence.rs Fence::abandon, Fence::finish",
        model_ref: "CommitFence.Swap (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Every open store exports its engine's commit and policy tickers. A
        // read of the totals, or the store's close, adds what the engine
        // counted past the export's mark by swapping the mark forward with one
        // compare-and-swap; only the winner adds, exactly the stretch it
        // claimed. The close adds before it leaves the map. The finding
        // configuration shows the claim and the add are two steps: a read can
        // briefly miss a closed store's counts that another read claimed and
        // has not added yet. The race test exercises the code on real threads;
        // nothing drives this model against the binary.
        family: "Engine ticker export",
        name: "INV_Accounted / INV_NeverOvercount / INV_ClosedComplete / INV_LeftClaimed / INV_QuiescentExact / PROP_TotalNeverBack / PROP_MarkNeverBack / LIVE_EveryCallEnds: every count reaches the exported total once, and the total never goes back",
        axis: Tla,
        anchor: "crates/telemetry/src/engine_tickers.rs Source::publish, EngineTickerExport::drop, engine_ticker_totals; crates/storage/src/backends/shared.rs TransactionMetrics",
        model_ref: "MC_EngineTickers_{Green,Green_TwoSources,Red_Backward,Red_BlindStore,Red_GiveUp,Red_StaleSeen,Red_RemoveFirst,Red_WeakClose,Red_Reset,Red_SharedStats,Red_CountAfterClose,Finding_ClaimGap}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Engine ticker export",
        name: "failed_attempt_changes_nothing / run_gain / run_total_eq_gains / publishAlone_comm / closed_export_adds_exactly: a mark moved by compare-and-swap adds each count once",
        axis: Lean,
        anchor: "crates/telemetry/src/engine_tickers.rs Source::publish",
        model_ref: "EngineTickers.Mark, EngineTickers.Race (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A placed pool claims its CPUs in the process's ledger, one counter
        // per CPU: alone, moving each counter from 0 to 1 with one
        // compare-and-swap and giving back what it took when one CPU is
        // held; or shared, adding one whatever the counter was. After its
        // capped claims alone fail it shares and says so. Its Hold gives back
        // exactly what it raised once the pool and every thread placed
        // through it are gone. A network runtime sized from the machine
        // claims whole cores the same way, alone only, and gives up holding
        // nothing instead of sharing.
        family: "Worker pool CPU ledger",
        name: "INV_AloneTakesDisjoint / INV_AloneRunsAlone / INV_TakenAlone / INV_AllOrNothing / INV_HeldNeverFree / INV_CountExact / INV_DoneLeavesNothing / LIVE_StartEnds: placed pools never share a CPU unless one of them says it shares",
        axis: Tla,
        anchor: "crates/workers/src/ledger.rs Ledger::hold, Ledger::share, Hold::drop; crates/workers/src/placement/reserve.rs reserve, shared; crates/workers/src/placement/edge.rs EdgeCores::reserve_on, EdgeCores::reserve; crates/workers/src/pool/placer.rs Placer::holding; crates/workers/src/pool/start.rs Pool::start_in; crates/edge/src/platform/native.rs build",
        model_ref: "MC_CpuLedger_{Green,Green_Edge,Red_SplitTake,Red_NoRollback,Red_ReleaseLoser,Red_ShareUncounted,Red_NoAttemptCap,Red_ReleaseEarly,Red_RunOutsideHold,Red_DoubleRelease,Red_ForgetHold,Red_EdgeNoCap}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Worker pool CPU ledger",
        name: "hold_all_or_nothing / rollback_restores / hold_won_alone / hold_step / share_step / release_alone_step / release_shared_step: a claim alone takes all or nothing, the counters stay exact, Holds claimed alone stay apart",
        axis: Lean,
        anchor: "crates/workers/src/ledger.rs Ledger::hold, Ledger::share, Hold::drop",
        model_ref: "CpuLedger.{Hold,Count,Exclusive} (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // `default_pool()` answers the first registered pool that still runs,
        // else one pool started once for the process. Registration moves the
        // slot only off a pool that stopped, with one compare-and-swap; the
        // started pool is built under a one-permit turn and published once; a
        // worker thread never waits for that start; `Pool::stop` never times
        // its own deadline on the pool it stops.
        family: "Worker default pool",
        name: "INV_NeverHandsOutClosing / INV_FirstRunningWins / INV_RegisteredWins / INV_OneStart / INV_OneDefault / INV_ErrorHonest / INV_StopDeadlineElsewhere / INV_MakeDefaultTellsTruth / INV_WorkerNeverWaits / INV_RegistrationKeepsNothingAlive / LIVE_EveryCallReturns / LIVE_EveryRegisterReturns: one default pool, never a closing one",
        axis: Tla,
        anchor: "crates/workers/src/pool/default.rs default_pool, Pool::make_default, DefaultPool::register, DefaultPool::get, DefaultPool::started; crates/workers/src/pool/mod.rs Pool::stop; crates/workers/src/pool/deadline.rs delay_on",
        model_ref: "MC_DefaultPool_{Green,Red_AnswerYes,Red_BlindRegister,Red_FailKeepsTurn,Red_NoClosingCheck,Red_NoRecheck,Red_NoRetry,Red_NoTurn,Red_RacyCell,Red_RegisterClosing,Red_RegisterOverwrites,Red_ShutStarted,Red_StartedFirst,Red_StopAsksFirst,Red_StrongSlot,Red_WorkerRefusedFirst,Red_WorkerWaits}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Worker default pool",
        name: "first_running_wins / stopped_never_selected / later_registrations_change_nothing / first_calls_agree / registered_wins / stop_deadline_elsewhere: the first running registered pool wins, a stopped one never does, every caller gets one started pool",
        axis: Lean,
        anchor: "crates/workers/src/pool/default.rs DefaultPool::register, DefaultPool::get",
        model_ref: "DefaultPool.{Slot,Cell,Get} (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // `partition::convergence_delete_update_race_preserves_tombstone`
        // partitions a replicated doc, deletes on one side, updates a mutable field
        // on the other, then heals and asserts the merged materialized view stays
        // tombstoned on both replicas. The TLA red leg captures the bad policy:
        // committing an active update from a stale whole-document snapshot clears
        // the delete marker.
        family: "Document materialization status convergence",
        name: "INV_DeletedMarkerAbsorbs — active rematerialization never clears a tombstone",
        axis: Tla,
        anchor: "crates/db/src/merge/merge_handler/composite_persist.rs handle_deletion / persist_merged_document; crates/defra-core/src/block_delta.rs CompositeDeltaPayload status",
        model_ref: "MC_DocumentMaterialization_Green.cfg; DefraConvergence.DocumentMaterialization.delete_active_age_converge",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/partition.rs convergence_delete_update_race_preserves_tombstone",
    },
    Property {
        // The DID-binding half (a distinct identity cannot impersonate another)
        // IS observed behaviorally by the ACP test (ungranted bob denied). The
        // signer-binding half — forging a malformed / alg-confused token, or one
        // whose iss/signer DID mismatches its signature — is unreachable through
        // the public CLI (the client always signs correctly), so it is Boundary.
        family: "JWT issuer / algorithm binding",
        name: "INV_TokenBindsGenuineDid — alg/iss/sig bound to did(pubkey)",
        axis: Tla,
        anchor: "crates/identity token verification",
        model_ref: "MC_Jwt_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "CID content-addressing determinism + Block canonicalization",
        name: "cid_injective_mod_hash — same normal-form content yields same CID",
        axis: Lean,
        anchor: "crates/defra-core/src/block.rs Block::generate_cid",
        model_ref: "Cid (lake build)",
        tiers: &[Behavioral, Contract],
        bound_by: "proofs/tests/behavioral/cid.rs cid_determinism_same_content_same_docid",
    },
    Property {
        family: "Deferred-ACP overlay consistency",
        name: "INV_FailClosedActive — txn-local ACP projection gates as committed state would",
        axis: Tla,
        anchor: "crates/query/src/txn/context.rs; crates/db/src/write/autocommit/batch.rs",
        model_ref: "MC_DeferredAcp_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/deferred_acp.rs deferred_acp_txn_local_gating",
    },
    Property {
        // Lean marker values (mNull=0..mIntMax=253) verified to match the live
        // `crates/storage/src/encoding/mod.rs` constants by inspection, but those
        // are `pub(crate)` (no static assert without widening prod visibility) and
        // a plain ordered query may sort in memory rather than exercise the index
        // key encoding — so neither tier is cleanly realizable. Boundary.
        family: "Order-preserving key encoding",
        name: "asc_strictly_order_preserving_* — encoding is a strict-order embedding",
        axis: Lean,
        anchor: "crates/storage/src/encoding/mod.rs type-marker constants",
        model_ref: "OrderEncoding (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // `sort_in_key_order` is private to the planner, so no contract test
        // reaches it; crates/defra-node/tests/in_filter_order.rs runs the
        // queries it orders over an ASC, a DESC and no index. Boundary.
        family: "Order-preserving key encoding",
        name: "lookups_strictly_increasing / reversed_lookups_strictly_decreasing — `_in` point lookups run in key order, each once",
        axis: Lean,
        anchor: "crates/query/src/planner/index_selection/filter_to_scan.rs filter_to_index_scan, sort_in_key_order",
        model_ref: "OrderEncoding.InLookup (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The flat code table is a pure in-memory structure loaded once per
        // graph generation; its laws are proven, and the unit tests in
        // crates/db/tests/vector/diskann_resident.rs exercise the Rust side.
        family: "DiskANN resident code table",
        name: "get_returns_inserted / get_in_bounds / insert_refuses_other_length: each slot reads back its own code, in bounds",
        axis: Lean,
        anchor: "crates/db/src/index/vector/engine/diskann/cache.rs Codes::insert, Codes::get",
        model_ref: "DiskAnnCodes (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // No test binds graph.rs walk to WalkCutoff.
        family: "DiskANN walk cutoff",
        name: "cut_step_eq_step — skipping neighbours ranked after a full frontier's last place leaves the frontier the full step would",
        axis: Lean,
        anchor: "crates/db/src/index/vector/engine/diskann/graph.rs walk, each_edge, by_distance",
        model_ref: "WalkCutoff.cut_step_eq_step / distance_only_cutoff_drops_a_tie",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Pure functions over the schema; the Rust side is exercised by
        // crates/query/src/planner/joins/relation_paths_tests.rs and the
        // validate_select tests in crates/query/src/runner/plan_validation.rs.
        family: "Filter relation paths and scalar leaves",
        name: "cut_eq_longest / longest_max / cut_idem / relationPaths_of_allRel — a filter path is cut at its first non-relation field, idempotently, and all-relation paths are unchanged",
        axis: Lean,
        anchor: "crates/query/src/planner/joins/relation_paths.rs Planner::relation_filter_paths, Planner::relation_prefix",
        model_ref: "JsonSubPathCut.cut_eq_longest / longest_allRel / longest_max / cut_idem / cut_of_allRel / relationPaths_of_allRel",
        tiers: &[Contract],
        bound_by: "crates/query/src/planner/joins/relation_paths_tests.rs json_sub_path_is_not_a_relation_path, relation_path_stops_at_json_field, json_sub_path_filter_through_relation_plans",
    },
    Property {
        family: "Filter relation paths and scalar leaves",
        name: "newValidate_false_iff / newRun_eq_mainRun — validation rejects exactly a sub-selection on a non-relation field, and the queries that run are main's",
        axis: Lean,
        anchor: "crates/query/src/runner/plan_validation.rs validate_select; crates/query/src/planner/joins/selection_join.rs Planner::apply_selection_relation_join",
        model_ref: "JsonSubPathCut.leafOk_false_iff / newValidate_false_iff / newValidate_of_main / newRun_eq_mainRun",
        tiers: &[Contract],
        bound_by: "crates/query/src/planner/joins/relation_paths_tests.rs sub_selection_on_json_field_is_go_validation_error, json_sub_path_filter_plans_without_index",
    },
    Property {
        family: "PQ distance four sums",
        name: "four_eq_sum — four running sums over a code's lookups add up to the one running sum, in exact arithmetic",
        axis: Lean,
        anchor: "crates/db/src/index/vector/quantize/pq.rs ProductQuantizer::distance",
        model_ref: "PqFourSums.four_eq_sum / dropped_tail_loses_lookups",
        tiers: &[Contract],
        bound_by: "crates/db/tests/vector/quantize_pq.rs the_lookup_table_agrees_with_the_reconstruction",
    },
    Property {
        family: "Query fragment nesting bound",
        name: "reach_le / walk_none_iff — the fragment walk never calls itself deeper than max_fragment_nesting, and refuses exactly the queries that nest deeper",
        axis: Lean,
        anchor: "crates/query/src/query_parse/limits.rs FragmentPath::check_nesting; crates/query/src/subscription.rs collect_root_fields",
        model_ref: "FragmentNesting.reach_le / walk_none_iff / off_by_one_accepts_too_deep",
        tiers: &[Contract],
        bound_by: "crates/query/tests/fragment_stack.rs a_long_fragment_chain_is_refused_on_a_small_stack, a_chain_at_the_default_limit_still_parses",
    },
    Property {
        family: "Index-maintenance consistency",
        name: "onDocumentUpdate_correct / _no_stale / _none_missing",
        axis: Lean,
        anchor: "crates/db (index) index maintenance",
        model_ref: "IndexMaintenance (lake build)",
        tiers: &[Behavioral, Contract],
        bound_by: "proofs/tests/behavioral/index.rs index_no_stale_no_missing",
    },
    Property {
        // `index::index_reconciles_lww_merge_after_restart` drives a restart
        // partition where two replicas write different values for the same indexed
        // LWW field. Once the DAG converges, indexed filters must expose exactly
        // the winning value and no stale seed/loser keys.
        family: "Index-maintenance consistency",
        name: "INV_IndexMatchesWinner — indexed filters equal the converged CRDT value",
        axis: Tla,
        anchor: "crates/db (merge) LWW materialization; crates/db (index) on_document_update",
        model_ref: "MC_IndexReconciliation_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/index.rs index_reconciles_lww_merge_after_restart",
    },
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
        anchor: "crates/defra-node/src/lib.rs Node::shutdown, Node::as_request; crates/defra-node/src/ops/request.rs Node::txn_begin, Node::txn_commit; crates/defra-node/src/network.rs NodeEdge::drain_requests; crates/http/src/server.rs Server::router; crates/defra-node/src/node_acp.rs gate_external_acp; crates/defra-node/src/managed_p2p.rs ShutdownHandle::stop, stop_services; crates/edge/src/task.rs Service::stopped; crates/edge/src/handle.rs Scope::drain; crates/net/src/server.rs serve, until_closing",
        model_ref: "MC_NodeShutdown_{Green,Red_RedriveDetached,Red_ServicesNotAwaited,Red_NoDrainDeadline,Red_FlushOnCaller,Red_RequestsDuringP2P,Red_CommitNotAdmitted,Red_OpenTransaction}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A signed query or transformed route keeps a request-scope permit
        // while its started worker writes, so P2P cannot stop first.
        family: "Node shutdown order",
        name: "INV_CommittedMarked: a signed query or transformed route commits with its durable head marker",
        axis: Tla,
        anchor: "crates/defra-node/src/signed_query/mod.rs execute_with_signing_context; crates/http/src/server.rs Server::router; crates/query/src/pooled/mod.rs PooledExecutor::execute_prepared; crates/workers/src/pool/detached.rs run_on; crates/defra-node/src/network.rs NodeEdge::drain_requests",
        model_ref: "MC_SignedRequestDrain_{Green,Red_TaskDeadline,Red_Transform}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/defra-node/src/signed_query/tests.rs a_signed_query_remains_admitted_past_the_edge_deadline; crates/http/tests/request_admission.rs a_route_added_by_router_transform_stays_admitted_until_it_ends",
    },
    Property {
        // A transaction guard follows its admitted commit task rather than a
        // caller that can stop waiting while the task still runs.
        family: "Node shutdown order",
        name: "INV_CommitNotAbandoned: cancelling the waiter does not abandon an edge-admitted commit in flight",
        axis: Tla,
        anchor: "crates/defra-node/src/transaction.rs NodeTransaction::commit; crates/query/src/txn/guard.rs TransactionGuard::drop; crates/db/src/txn/owner/table.rs Table::abandon",
        model_ref: "MC_GuardedCommit_{Green,Red_CallerOwns}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/defra-node/src/transaction/cancellation_tests.rs cancelling_a_commit_waiter_does_not_abandon_the_running_commit",
    },
    Property {
        // Request admission follows the worker; transaction ownership follows
        // its commit task when the caller gives up waiting.
        family: "Node shutdown order",
        name: "start_keeps_permit / finish_keeps_permit / dropped_waiter_keeps_permit / expired_task_loses_permit / task_guard_survives_cancellation / caller_guard_is_abandoned / begin_open_has_owner / abandoned_begin_is_closed",
        axis: Lean,
        anchor: "crates/defra-node/src/signed_query/mod.rs execute_with_signing_context; crates/defra-node/src/transaction.rs NodeTransaction::commit; crates/workers/src/pool/detached.rs run_on",
        model_ref: "RequestDrain (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/defra-node/src/signed_query/tests.rs a_signed_query_remains_admitted_past_the_edge_deadline; crates/http/tests/request_admission.rs a_route_added_by_router_transform_stays_admitted_until_it_ends; crates/defra-node/src/transaction/cancellation_tests.rs cancelling_a_commit_waiter_does_not_abandon_the_running_commit, cancelling_a_begin_waiter_abandons_the_unreturned_transaction",
    },
    Property {
        // The result of a detached begin abandons its transaction if its
        // caller leaves before receiving the handle.
        family: "Node shutdown order",
        name: "INV_OpenHasOwner: an edge-admitted begin cannot leave an unowned transaction open",
        axis: Tla,
        anchor: "crates/defra-node/src/lib.rs Node::begin_transaction; crates/defra-node/src/ops/request.rs Node::txn_begin; crates/query/src/txn/guard.rs TransactionGuard::drop",
        model_ref: "MC_DetachedBegin_{Green,Red_NoResultGuard}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/defra-node/src/transaction/cancellation_tests.rs cancelling_a_begin_waiter_abandons_the_unreturned_transaction",
    },
    Property {
        // A network task's completion says how the work ended: its own
        // handle's abort is Cancelled, the runtime's stop is Closed, a value
        // is a value. The handle records its abort before it cancels, and the
        // runtime marks itself stopped before it ends its tasks.
        family: "Edge completion",
        name: "INV_AbortSaysCancelled / INV_StopSaysClosed / INV_DoneSaysValue: a completion its handle aborted says Cancelled, one the runtime's stop cut off says Closed, one that finished says its value",
        axis: Tla,
        anchor: "crates/edge/src/task.rs Task::poll, Task::abort; crates/edge/src/platform/native.rs the shutdown; crates/edge/src/shared.rs Shared::stop",
        model_ref: "MC_EdgeCompletion_{Green,Red_NoMap,Red_CancelBeforeFlag,Red_CancelBeforeStopMark}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/edge/src/tests/task_set.rs aborting_a_set_stops_its_tasks_and_hands_each_back_cancelled; shutdown.rs shutdown_stops_accepting_drains_then_stops_the_rest; spawn.rs a_spawned_task_hands_back_its_value",
    },
    Property {
        // Work that continues a request already admitted (an outbound
        // connection's driver) is admitted after the gate closes while
        // admitted work is in flight; the drain seals before its last look, so
        // it never says drained while a continuation still runs.
        family: "Edge gate continuation",
        name: "INV_NothingAfterDrain / INV_InFlightContinues / INV_NoContinuationWhenIdle: nothing holds a permit once drained, a request in flight gets its continuation, and none starts from an idle closed gate",
        axis: Tla,
        anchor: "crates/edge/src/gate.rs Gate::admit, Gate::admit_continuation, Gate::drained; crates/edge/src/handle.rs Edge::spawn_continuation, drain_gate; crates/net/src/rt.rs ClientExecutor::execute",
        model_ref: "MC_EdgeGateContinuation_{Green,Red_PlainSpawn,Red_NoSeal,Red_NoCas}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/edge/tests/continuation.rs a_continuation_is_admitted_after_close_while_admitted_work_runs, a_continuation_is_refused_after_close_when_nothing_is_in_flight, the_drain_waits_for_a_running_continuation, a_continuation_is_refused_after_the_drain_completed_even_with_plain_spawns_racing; crates/net/tests/server.rs a_closing_client_finishes_the_request_in_flight_and_frees_its_idle_connections, a_request_in_flight_at_close_opens_a_fresh_connection_and_finishes",
    },
    Property {
        // An iroh dial answers from its connection's outcome: a refusal that
        // already closed the connection is answered Unauthorized, and
        // connect_peer ends with the dial's answer instead of polling the
        // connected set until a timeout.
        family: "Iroh dial outcome",
        name: "INV_RefusalKnownIsSaid / LIVE_DialAnswered: a dial never answers ok on a connection the peer's refusal already closed, and every dial is answered",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/endpoint_commands.rs handle_dial; crates/p2p-adapter/src/iroh.rs IrohP2PAdapter::connect_peer; crates/p2p/src/iroh/transport.rs poll_until_connected",
        model_ref: "MC_IrohDialOutcome_{Green,Green_Admitted,Red_Poll,Red_NoCheck}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/p2p/src/iroh/allowlist_tests.rs closed_by_refusal_reads_the_remote_close_code, poll_until_connected_wakes_on_the_connection_and_times_out_without_one",
    },
    Property {
        // A server's connections are tasks the drain waits for: a request in
        // flight when shutdown begins finishes and is answered, a connection
        // looks at closing only between requests, and the drain's deadline
        // stops what is left.
        family: "Server drain",
        name: "INV_NoDropBeforeDeadline / INV_NoNewWorkWhenClosing / LIVE_ServerReturns: a request in flight is only stopped at the drain's deadline, no request starts once closing began, the server returns",
        axis: Tla,
        anchor: "crates/net/src/server.rs serve, until_closing; crates/net/src/rt.rs Executor::execute; crates/pg-compat/src/lib.rs PgServer::run; crates/pg-compat/src/wire/process.rs serve; crates/edge/src/handle.rs Scope::drain",
        model_ref: "MC_ServerDrain_{Green,Red_Service,Red_CheckInside,Red_NoCheck,Red_NoDeadline}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/net/tests/server.rs a_closing_server_finishes_the_requests_in_flight_then_returns; crates/pg-compat/tests/wire.rs a_query_in_flight_when_the_edge_starts_closing_completes_and_the_connection_closes, an_idle_connection_closes_at_once_when_the_edge_starts_closing",
    },
    Property {
        // An update on a document's topic is applied only while the node
        // follows the document or its collection: an unsubscribe clears the
        // set the gossip handler reads before it returns, so updates queued or
        // sent after it are dropped.
        family: "Document topic ingress",
        name: "INV_NoApplyAfterUnsubscribe / INV_FollowedApplies: no update is applied after the document's unsubscribe returned unless its collection is followed, and none taken while followed is dropped",
        axis: Tla,
        anchor: "crates/p2p/src/sync/coordinator/event_handler/gossip.rs handle_gossip_message; crates/p2p/src/sync/coordinator/access.rs is_followed_document; crates/p2p-adapter/src/libp2p.rs P2PAdapter::add_documents, remove_documents; crates/p2p-adapter/src/iroh.rs IrohP2PAdapter::add_documents, remove_documents; crates/defra-node/src/libp2p_recovery.rs restore_libp2p_documents; crates/p2p-adapter/src/iroh_restore.rs restore_iroh_p2p_state",
        model_ref: "MC_DocTopicIngress_{Green,Red_NoCheck,Red_OwnSet,Red_NoCollection}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/p2p/src/sync/coordinator/access_tests.rs gossip_document_topic_dropped_after_document_unfollowed; crates/p2p-adapter/tests/libp2p/followed_documents.rs remove_documents_clears_the_shared_followed_set",
    },
    Property {
        // A node checks its config before it opens anything, hands
        // its retry ladder to every retry site of either transport, stops a
        // host whose listen failed, enables NAC owned by the node identity,
        // and dials bootstrap peers only once NAC is in place, logging a peer
        // that does not answer.
        family: "Node startup",
        name: "INV_FailedHoldsNothing / INV_OneSchedule / INV_NacAsAsked / INV_NoPeerBeforeNac / INV_BootstrapNonFatal / LIVE_StartupEnds: a failed start holds nothing, a started node is configured as asked",
        axis: Tla,
        anchor: "crates/defra-node/src/assemble.rs assemble; crates/defra-node/src/libp2p_runtime.rs setup_libp2p, assemble_libp2p; crates/defra-node/src/p2p_runtime.rs setup_iroh; crates/defra-node/src/node_acp.rs create_nac_manager",
        model_ref: "MC_NodeStartup_{Green,Red_IrohDefaultLadder,Red_ListenLeak,Red_LateNacCheck,Red_NacUnowned,Red_DialBeforeNac,Red_BootstrapFatal}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Node startup",
        name: "new_keeps / new_valid / new_accepts_valid / cap_mem / cap_positive / cap_follows_ladder / cap_repeats_last / default_accepted",
        axis: Lean,
        anchor: "crates/storage/src/stores/retry_info.rs RetrySchedule::new, RetrySchedule::cap, RETRY_INTERVALS_SECS",
        model_ref: "RetrySchedule (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Inbound stream admission",
        name: "INV_ResidentWithinBudget / INV_ShareBound / LIVE_HealthyHandled / LIVE_WaitEnds",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/endpoint_streams.rs admit_stream; crates/p2p/src/iroh/inbound_budget.rs InboundBudget::admit, InboundBudget::peer_share, InboundDeadlines::body; crates/p2p/src/sync/dispatch_queue.rs dispatch_queue; crates/p2p/src/sync/event_dispatcher.rs run_event_dispatcher",
        model_ref: "MC_InboundAdmission_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A request shed while every rejection worker is busy is dropped. Its
        // stream closes empty, so the requester sees Refused (CAR) or
        // Transport (PushLog) instead of Busy, and spends free refusals, stall
        // budget and an attempt. Retries stay bounded by DagFetchBudget and the
        // per-root clock.
        family: "Inbound stream admission",
        name: "A request shed past the rejection workers is answered by a closed stream, not Busy",
        axis: Tla,
        anchor: "crates/p2p/src/sync/event_dispatcher.rs run_event_dispatcher, MAX_ACTIVE_REJECTIONS; crates/p2p/src/iroh/endpoint_rpc.rs CarFetchOutcome",
        model_ref: "MC_InboundAdmission_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Inbound stream admission",
        name: "acquire_conserves / release_all_restores / admit_failure_keeps_nothing",
        axis: Lean,
        anchor: "crates/p2p/src/iroh/inbound_budget.rs InboundBudget::admit, InboundLease",
        model_ref: "ByteBudget (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Peer fetch stats",
        name: "roll_ratio_stays_a_ratio / rolling_over_samples / bump_never_overflows",
        axis: Lean,
        anchor: "crates/p2p/src/iroh/fetch_stats.rs PeerCounters::record, roll",
        model_ref: "FetchStats (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "DAG fetch provider ranking",
        name: "INV_WaitBounded — one attempt waits at most a rotation of stalls plus its free refusals, however many batches",
        axis: Tla,
        anchor: "crates/p2p/src/sync/coordinator/dag_fetcher.rs fetch_dag_attempt, poll_fetch_blocks_rotating, window_outcome; crates/p2p/src/sync/coordinator/dag_retry.rs FAST_REFUSAL, MAX_FREE_REFUSALS",
        model_ref: "MC_DagFetchBudget_{Green,Red_SlowIsRefusal,Red_Uncapped,Red_NoCheck}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "DAG fetch provider ranking",
        name: "rank_perm / rank_keeps_healthy_source_first / rank_all_healthy / next_delay_in_range / stale_starts_over / forget_keeps_fresh / forget_drops_stale",
        axis: Lean,
        anchor: "crates/p2p/src/sync/coordinator/provider_health.rs ProviderHealth::rank, record_fast_failure, forget_stale, Cooldown::is_stale",
        model_ref: "ProviderHealth (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "DAG fetch arbitration (EWMA)",
        name: "ArbitrationConstants: VALUE_BITS, TIME_BITS, WEIGHT_ONE, the half-lives, shifts, tick, TIE_BUCKET, BACKOFF_FLOOR, units, floor and WEIGHT_TABLE, with table_is_floor / table_decreasing / table_wraps / packed_word_fits / tie_bucket_le_backoff_floor / useful_floor_is_one_64th / occupancy_half_life_ticks checked on exactly these values",
        axis: Lean,
        anchor: "crates/p2p/src/sync/arbitration/mod.rs VALUE_BITS, TIME_BITS, WEIGHT_ONE, WEIGHT_TABLE, PROVIDER_HALF_LIFE; crates/p2p/src/sync/arbitration/ewma.rs weight_q32; proofs/tests/lean_conformance.rs lean_arbitration_constants_match_rust",
        model_ref: "Arbitration.Constants (lake build); Conformance.lean ArbitrationConstants",
        tiers: &[Contract],
        bound_by: "proofs/tests/lean_conformance.rs lean_arbitration_constants_match_rust",
    },
    Property {
        family: "DAG fetch arbitration (EWMA)",
        name: "roll_le_bound / troll_le_bound / record_first / troll_full / iter_monotone_up / iter_stays_below / decay_geometric / troll_decay_up / troll_decay_down / dither_mean / weight_le_one / weight_zero_gap / weight_first_ms / expected_useful_le / bucket_order_any_score / inversion_within_bucket / rearmed_root_goes_behind",
        axis: Lean,
        anchor: "crates/p2p/src/sync/arbitration/ewma.rs roll, troll, record_word, PackedEwma; root.rs RootEstimates::observe; occupancy.rs advance; provider.rs ProviderEstimates, expected_useful; due_order.rs tie_break_order; tests.rs; tests/unit/estimator_trace_replay.rs; tests/unit/estimators_observe_only.rs",
        model_ref: "Arbitration (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Gossip connection ownership",
        name: "INV_RevokedHasNoGossip / INV_PathOnLiveConnection / INV_GossipNeverClosesShared / LIVE_HealthyReached / LIVE_AskAnswered",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/gossip/driver.rs Driver::send, Driver::connected, Driver::forget; crates/p2p/src/iroh/gossip/mod.rs GossipHandle::connected; crates/p2p/src/iroh/endpoint_rpc.rs connect_with_cache; crates/p2p/src/iroh/endpoint_commands.rs handle_deny_peer",
        model_ref: "MC_GossipOwnership_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Gossip connection ownership",
        name: "LIVE_RequestAnswered",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/gossip/inbox.rs Inbox::push, Inbox::offer, Inbox::next; crates/p2p/src/iroh/gossip/driver.rs read_stream, Driver::emit; crates/p2p/src/iroh/records/task.rs RecordsHandle::push_change, RecordsHandle::push, RecordsHandle::next",
        model_ref: "MC_GossipInbox_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Gossip connection ownership",
        name: "decode_encode / decode_all_encode_all / decode_rejects_oversized / read_tag_write_tag",
        axis: Lean,
        anchor: "crates/p2p/src/iroh/gossip/frame.rs write_gossip_frame, read_gossip_frame; crates/p2p/src/iroh/protocols.rs write_stream_tag, read_stream_tag",
        model_ref: "GossipFrame (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Iroh service records",
        name: "INV_SeqNeverGoesBack / INV_NoExpiredChosen / LIVE_NewestReached",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/records/book.rs Book::offer, Book::live; crates/p2p/src/iroh/records/refresh.rs Refresh::check; crates/p2p/src/iroh/records/task.rs Records::renew, Records::on_input, Records::rotate",
        model_ref: "MC_ServiceRecords_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A node publishes a service record only after its embedder sets a
        // ServiceProfile, and only the CLI does. For NodeBuilder and FFI
        // nodes LIVE_NewestReached is vacuous.
        family: "Iroh service records",
        name: "LIVE_NewestReached and LIVE_Found assume the embedder sets a ServiceProfile",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/records/task.rs Records::renew; crates/cli/src/commands/start/server_p2p/iroh.rs",
        model_ref: "MC_ServiceRecords_Green.cfg / MC_ProviderLookup_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Depth-cut DAG fetch",
        name: "INV_HistoryNotResent / INV_RoundsLogarithmic / LIVE_Completes",
        axis: Tla,
        anchor: "crates/p2p/src/sync/car.rs collect_dag_blocks_within; crates/p2p/src/sync/coordinator/closure_depth.rs ClosureDepth::widen; crates/p2p/src/sync/coordinator/dag_fetcher.rs fetch_dag_attempt",
        model_ref: "MC_DepthCutFetch_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Depth-cut DAG fetch",
        name: "within_iff_level / deep_block_not_served / within_le / cut_le_cap / cut_ge_pow / cut_reaches",
        axis: Lean,
        anchor: "crates/p2p/src/sync/car.rs collect_dag_blocks_within; crates/p2p/src/sync/coordinator/closure_depth.rs ClosureDepth::initial, ClosureDepth::widen",
        model_ref: "CarClosureDepth (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // libp2p CAR bodies have one fixed total read deadline. A response
        // slower than CAR_MAX_BYTES over that deadline fails, and the fetch
        // falls back to Bitswap.
        family: "Depth-cut DAG fetch",
        name: "LIVE_Completes on libp2p assumes a CAR body arrives within the stream read deadline",
        axis: Tla,
        anchor: "crates/p2p/src/two_stream/handler/car.rs read_stream; crates/p2p/src/two_stream/runner.rs TwoStreamRunner::new",
        model_ref: "MC_DepthCutFetch_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Clock and PushLog fetches single-flight per root in
        // PendingDagFetchTask, separately, and may overlap a reply fetch of the
        // same root. CAR ingest serializes the overlap (StorageContended
        // defers it); it costs bandwidth and a limiter permit, never a head.
        family: "Reply-driven DAG fetch",
        name: "INV_OneFetchPerRoot per claim key (root, replier's sorted explicit-replicator collection set), reply-driven fetches only / INV_NoReplyLost",
        axis: Tla,
        anchor: "crates/p2p/src/sync/coordinator/mod.rs DagFetchLimiter::claim_root, RootKey, RootClaim, PendingDagFetchTask; crates/p2p/src/sync/coordinator/event_handler/doc_sync.rs handle_doc_sync_reply; crates/p2p/src/sync/coordinator/event_handler/branchable_sync.rs handle_branchable_sync_reply; crates/p2p/src/sync/coordinator/event_handler/car.rs CarIngestDisposition::StorageContended; crates/p2p/src/sync/manager/process/mod.rs try_acquire_car_storage_owners",
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
        anchor: "crates/p2p/src/sync/coordinator/dag_context.rs DagFetchContext::with_alternate_providers; crates/p2p/src/sync/pending_store/mod.rs MAX_PENDING_DAG_ALTERNATE_PROVIDERS; crates/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag",
        model_ref: "MC_RootSingleFlight_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Iroh service records",
        name: "INV_AskOnlyWhenEmpty / LIVE_Found",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/records/task.rs Records::providers, Records::on_message",
        model_ref: "MC_ProviderLookup_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A lookup asks only when the book has no live record for the
        // collection key. Records name holders of a collection, not of a root,
        // so a non-holder's live record suppresses Who.
        family: "Iroh service records",
        name: "LIVE_Found assumes a live record's peer holds the root, not only the collection",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/records/task.rs Records::providers, Records::on_message",
        model_ref: "MC_ProviderLookup_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Dropping an index while its backfill runs: the drop's commit takes
        // the backfill record with it, a backfill failure is recorded only
        // while the index is defined, entries go in pages after the
        // definition commits, the drop retires the index's id so a new
        // index never shares its keys, and a restart finishes an unfinished
        // drop.
        family: "Index drop during backfill",
        name: "INV_NoRecordForGoneIndex / INV_NoOrphanEntries / INV_NewIndexIntact / LIVE_DropFinishes",
        axis: Tla,
        anchor: "crates/db/src/index/drop.rs DB::drop_index, DropIndexAttempt, DropPage, DB::remove_dropped_entries, DB::resume_index_drops; crates/db/src/index/manager/mod.rs index_id_floor; crates/db/src/index/backfill.rs BackfillPage::index, DB::backfill_index, FailBackfillAttempt, DB::resume_index_backfills",
        model_ref: "MC_IndexDropBackfill_{Green,Red_KeepRecord,Red_BlindFail,Red_SameTxn,Red_Forget,Red_ReuseId}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Purging documents by DocID: values, index entries, the primary
        // key, heads and DocID aliases go in one transact attempt; outside a
        // branchable collection every block only the purged documents own
        // goes too, decided by a validated owner scan; in a branchable
        // collection the history stays and no owner edge names a purged
        // document; the count is the distinct stored documents, taken at
        // commit; a sync re-storing a walked block re-runs the attempt.
        family: "Purge by DocID",
        name: "INV_OthersIntact / INV_CollectionIntact / INV_Gone / INV_NoStaleOwner / INV_Pruned / INV_Count / PROP_Commits",
        axis: Tla,
        anchor: "crates/db/src/collection/ops/purge.rs DB::purge_by_doc_ids_as, DB::purge_documents; crates/db/src/txn/attempt.rs DB::transact_db; crates/db/src/collection/ops/truncate/filtered.rs DB::truncate_filtered_chunk; crates/db/src/txn/registry/purge.rs DbTransactionRegistry::purge_by_doc_ids_as; crates/db/src/block/cleanup.rs delete_owned_dag_for_owners, drop_dag_owners; crates/db/src/docid/map.rs block_has_owners",
        model_ref: "MC_Purge_{Green_Prune,Green_Branchable,Red_Blind,Red_PruneBranchable,Red_KeepEdges,Red_KeepIndex,Red_CountRequests,Red_CountAttempt,Red_Fail,Red_PlainScan}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The orphan block sweep: a fetched block's to-merge mark
        // holds its fetch time and a refetch leaves it alone; the sweep nominates
        // expired marks from a snapshot and decides each in a transaction
        // that reads the mark and the owners again, deleting an unowned
        // block, clearing the mark of an owned one, and never touching an
        // untimed mark; a merge clears and owns its signature block too.
        family: "Orphan block sweep",
        name: "INV_NoMergedLoss / INV_NoOwnedLoss / INV_MergedUnmarked / INV_SigOwned / INV_LegacyKept / PROP_Reclaims",
        axis: Tla,
        anchor: "crates/storage/src/stores/blockstore.rs BlockWritesAttempt::run, marker_value, marker_time; crates/db/src/block/orphan_gc.rs DB::reclaim_orphan_blocks, DB::decide_orphans; crates/db/src/merge/merge_handler/doc_identity.rs record_block_ownership",
        model_ref: "MC_OrphanGc_{Green,Red_SnapshotRead,Red_NoOwnerCheck,Red_BlindRefresh,Red_ReclaimLegacy,Red_SigMarked,Red_SigUnowned}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The sweep does not read the pending-DAG store, so it reclaims the
        // fetched blocks of a durable obligation stalled past the TTL. The
        // model's weak fairness on Fetch assumes such a block can be fetched
        // again.
        family: "Orphan block sweep",
        name: "PROP_Reclaims assumes a reclaimed unmerged block can be fetched again",
        axis: Tla,
        anchor: "crates/db/src/block/orphan_gc.rs DB::decide_orphans, OrphanSweepConfig::TTL",
        model_ref: "MC_OrphanGc_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Orphan block sweep",
        name: "time_value / time_short / time_long / time_legacy",
        axis: Lean,
        anchor: "crates/storage/src/stores/blockstore.rs marker_value, marker_time",
        model_ref: "OrphanMarker (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The replication filter (#2451): asked between the merge's phases,
        // so a refused document stores nothing; a refusal skips only its own
        // event, for good, and a later commit of a refused document is refused
        // too; a branchable collection is not filtered. A non-genesis commit
        // that finds no live heads is taken to follow a refusal: that is sound
        // only while a refusal is the only thing that leaves a document
        // headless, and a purge or a predecessor skipped non-terminally also do.
        family: "Replication filter",
        name: "INV_RefusedNothingStored / INV_KeptMerged / INV_MergedOnce / INV_BranchableUnfiltered",
        axis: Tla,
        anchor: "crates/db/src/merge/merge_handler/composite_fields.rs process_linked_field_blocks; crates/db/src/replication_filter.rs add_field_values; crates/db/src/merge/merge_handler/batch.rs try_batch_merge",
        model_ref: "MC_ReplicationFilter_{Green,Green_Branchable,Red_AfterPersist,Red_WholeBatch,Red_Retry,Red_FilterBranchable,Red_Alone}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Document write pipeline",
        name: "INV_NoStoreBeforeComposite / INV_NoMintInAttempt / INV_OneKeyPerField / INV_CommittedWhole",
        axis: Tla,
        anchor: "crates/db/src/write/create.rs embed, identity, seed_identity, memo_identity, prepare_keys, compute, compute_many, persist; crates/db/src/write/autocommit/create.rs create_many_with_resolved_inputs, create_impl, run_create_attempt; crates/db/src/write/autocommit/batch.rs BatchMutator::create_as (with a memo); crates/db/src/write/doc.rs DbDocMutator::create_as, next_create_identity; crates/db/src/write/update.rs write_local, write_blocks; crates/db/src/write/autocommit/update.rs run_update_attempt; crates/db/src/write/autocommit/batch.rs BatchMutator::update_as (with a memo); crates/db/src/write/doc.rs DbDocMutator::update_as; crates/db/src/block/builder/write.rs build_document_blocks, encrypt_with_new_key",
        model_ref: "MC_WritePipeline_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Add collection",
        name: "INV_NoCollectionUnlessAllChecksPass / INV_AllOrNone",
        axis: Tla,
        anchor: "crates/db/src/collection/ops/add/mod.rs DB::add_collection, prepare_new_collections, check_policies, check_collection_patch; crates/db/src/txn/registry/schemas.rs add_collection_in_txn_as; crates/db/src/collection/ops/create.rs create_collections_atomic_inner",
        model_ref: "MC_AddCollection_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Collection truncate",
        name: "INV_NoRemovalWithoutPermission / INV_FilteredNeverOnBranchable / INV_FilteredRemovesOnlyCommittedMatches / INV_BadFilterRemovesNothing / INV_UnfilteredRemovesAll",
        axis: Tla,
        anchor: "crates/db/src/collection/ops/truncate/mod.rs DB::truncate_collection_as, DB::truncate_parsed_as, DB::truncate_target, conditions, DB::truncate; crates/db/src/collection/ops/truncate/all.rs DB::truncate_all; crates/db/src/collection/ops/truncate/filtered.rs DB::truncate_matching",
        model_ref: "MC_Truncate_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Add DAC policy",
        name: "INV_NoPolicyWithoutPermission / INV_NoPolicyWithoutCreator / INV_NoInvalidPolicy / INV_NoPolicyWithoutAcp / INV_ReturnedIsKeptLocally / INV_NoSilentFailure / INV_GoOrder",
        axis: Tla,
        anchor: "crates/db/src/access/dac/mod.rs DB::add_dac_policy_as, DB::add_policy, parse, build; crates/db/src/access/dac/policies.rs DacPolicies::keep, DacPolicies::get; crates/acp/src/policy_lookup.rs RemotePolicies::add_policy",
        model_ref: "MC_DacPolicyAdd_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Iroh solicited replies",
        name: "INV_NoUnsolicitedAdmitted / INV_SolicitedAdmitted / INV_NoReplyStreamOnV2",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/solicited.rs Solicited::expect, Solicited::take, Solicited::prune; crates/p2p/src/iroh/endpoint_streams.rs admit_stream, dispatch_stream; crates/p2p/src/iroh/endpoint_commands.rs handle_command; crates/p2p/src/iroh/endpoint_rpc.rs handle_reply_on_stream",
        model_ref: "MC_SolicitedReplies_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Reply expectations are keyed by (peer, tag) across all of the peer's
        // connections, and are taken before the size, budget and body checks.
        // A concurrent mux 0.1 connection can therefore open up to that many
        // unowed reply streams (still budgeted and matched by message id), and
        // a reply reset for room or size loses its expectation.
        family: "Iroh solicited replies",
        name: "INV_NoUnsolicitedAdmitted counts expectations per (peer, tag), not per connection or reply",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/solicited.rs Solicited::expect, Solicited::take; crates/p2p/src/iroh/endpoint_streams.rs admit_stream",
        model_ref: "MC_SolicitedReplies_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Iroh service records",
        name: "validate_sound / validate_complete / offer_never_lowers / offer_replaces_expired / offer_keeps_only_valid / live_not_expired / encodeKey_injective / no_secret_not_advertised / distinct_ids_distinct_keys / parse_print / providers_exclude_self / providers_nodup / providers_from_book / providers_within_cap / providers_sorted",
        axis: Lean,
        anchor: "crates/p2p/src/iroh/records/verify.rs verify; crates/p2p/src/iroh/records/book.rs Book::offer, Book::live; crates/p2p/src/iroh/records/key.rs blind, advertised_key, CollectionKey::encode; crates/p2p/src/iroh/records/pointer.rs Pointer::print, Pointer::parse; crates/p2p/src/iroh/records/task.rs Records::providers, rank",
        model_ref: "ServiceRecord (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Text chunking",
        name: "covers / bounded / deterministic / local_edit",
        axis: Lean,
        anchor: "crates/chunk-split/src/split/mod.rs split; segments.rs fence_before; atoms.rs atoms; pack.rs pack; overlap.rs with_overlap",
        model_ref: "ChunkSplit (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Text chunking",
        name: "no_chunks_keeps_its_identity / different_settings_different_identity / field_order_does_not_change_identity / peer_holds_the_same_settings_as_the_author / buggy_identity_ignores_settings",
        axis: Lean,
        anchor: "crates/schema/src/cid.rs build_collection_delta; crates/query/src/sdl_parse/chunk.rs synthesize_chunk_types; crates/db/src/merge/merge_handler/definition.rs process_collection_definition_delta",
        model_ref: "ChunkSplit (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // One task serves a pg-compat connection: one deadline read at the
        // accept bounds the peek, the SSL and GSS refusals and the startup;
        // an extended-protocol error skips to the Sync; a fatal error or a
        // cancel request closes it and the loop returns. A copy's CopyDone
        // leaves the copy, so an Execute copy's Sync reaches on_sync.
        family: "Postgres wire connection loop",
        name: "INV_StartupWithinDeadline / INV_StartedNeverTimedOut / INV_NoQueryBeforeStartup / INV_SkipUntilSync / INV_SyncResumes / INV_CopyEnds / INV_NothingAfterEnd / LIVE_Ends: no connection outlives the startup deadline unstarted, nothing but Sync is handled during a skip, nothing is read after the end",
        axis: Tla,
        anchor: "crates/pg-compat/src/wire/process.rs serve, run, refuse_ssl, dispatch, report; crates/pg-compat/src/wire/connection.rs Connection::next, Connection::is_closed, Connection::poll_close; crates/pg-compat/src/handler/auth.rs DIDAuthHandler::on_startup",
        model_ref: "MC_PgWire_{Green_Startup,Green_Protocol,Red_PeekNoDeadline,Red_SslNoDeadline,Red_StartupNoDeadline,Red_FreshDeadline,Red_DeadlineAfterStartup,Red_NoAwaitingSync,Red_HandleWhileSkipping,Red_SyncKeepsSkipping,Red_CopyFlagFromMessage,Red_CopyDoneStays,Red_ExtendedCopyDoneStays,Red_LoopAfterClose,Red_LoopAfterCloseRuns,Red_LoopAfterCloseHangs,Red_FatalStaysOpen,Red_StartupErrorNotFatal}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/pg-compat/tests/wire.rs ssl_is_refused_then_queries_run_and_a_fatal_error_closes_the_connection, a_client_that_starts_tls_directly_is_closed, a_client_that_sends_nothing_is_closed_at_the_startup_deadline",
    },
    Property {
        family: "HTTP client redirects and body deadline",
        name: "credentials_stay_with_their_origin / at_most_limit_plus_one_exchanges / endless_redirects_fail / same_origin_redirect_keeps_credentials / cross_origin_redirect_strips_credentials: a credential from the request or the defaults reaches only the request's origin, a proxy credential only its own hop, within eleven exchanges",
        axis: Lean,
        anchor: "crates/net/src/http/client.rs Client::execute, Client::follow, Client::exchange, redirect, REDIRECT_LIMIT",
        model_ref: "HttpClient.Redirect (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/net/tests/client.rs redirects_are_followed_and_change_the_method_as_the_status_says, credentials_follow_a_redirect_only_to_the_same_origin",
    },
    Property {
        family: "HTTP client redirects and body deadline",
        name: "ends_after_timeout / at_most_one_timeout / deadline_wins_over_a_ready_piece / never_waits_past_the_deadline / no_timeout_before_the_deadline / no_limit_no_timeout: past the limit a body stream times out once, even with a piece ready, then gives its end and nothing more",
        axis: Lean,
        anchor: "crates/net/src/http/response.rs BytesStream::poll_next, Response::bytes_stream",
        model_ref: "HttpClient.BodyDeadline (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/net/tests/body_deadline.rs a_stalled_body_stream_gives_one_timeout_then_ends, every_chunk_read_past_the_limit_times_out_at_once, a_piece_ready_past_the_limit_gives_the_timeout",
    },
    Property {
        family: "Connect fallback race",
        name: "INV_OneSocket / INV_FallbackWaits / INV_NeverIdle / INV_ErrorOnlyWhenAllFail / INV_LastError / INV_WithinLimit / LIVE_Ends: one socket comes back, the other family starts after the delay or a failure, an error only once every family failed",
        axis: Tla,
        anchor: "crates/net/src/connect.rs first_connected, Connector::socket, FALLBACK_DELAY",
        model_ref: "MC_ConnectFallback_{Green,Green_NoTimeout,Green_Single,Red_NoDelay,Red_WaitAfterFailure,Red_FirstErrorWins,Red_KeepFirstError,Red_KeepLoser,Red_TimeoutMissesRace,Red_TimeoutMissesRaceHangs}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/net/tests/connect.rs the_other_family_connects_when_the_first_is_silent_past_the_delay, the_other_family_is_tried_when_the_first_refuses, when_both_families_refuse_the_last_refusal_comes_back",
    },
    Property {
        // The first shutdown caller to swap the flag builds the one drain,
        // which owns the only completion sender; a closed edge hands the
        // drain back and the winner runs it in place. The drain waits for
        // pruners, takes the tracked set, and aborts what outlives its
        // deadline, so no tracked task keeps the coordinator alive. The
        // GREEN configurations assume prune_finished_tasks reads the flag
        // after announcing itself; Red_PrunerIgnoresFlag is the code without
        // that read.
        family: "Coordinator shutdown drain",
        name: "INV_NoLeak / INV_OneDrain / INV_ErrMeansCancelled / LIVE_CallersReturn: one drain runs, no tracked task is left running in the set once it returns, every shutdown caller returns",
        axis: Tla,
        anchor: "crates/p2p/src/sync/coordinator/mod.rs SyncShutdownHandle::shutdown, begin_shutdown, drain_background_tasks, spawn_task, spawn_here, register_task, prune_finished_tasks; crates/p2p/src/network.rs Network::try_spawn; crates/edge/src/handle.rs Edge::try_spawn",
        model_ref: "MC_CoordinatorShutdown_{Green,Green_TwoSpawns,Red_DropRefused,Red_CheckThenSpawn,Red_NoAbort,Red_SenderOutsideDrain,Red_TwoWinners,Red_NoPrunerWait,Red_PrunerIgnoresFlag,Red_NoRecheck}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/p2p/tests/coordinator/shutdown.rs concurrent_shutdown_callers_wait_for_task_completion, shutdown_rejects_new_tasks_without_spawning_them, task_registration_racing_shutdown_does_not_leave_resources_alive, shutdown_on_a_closed_edge_runs_the_drain_and_releases_tracked_tasks",
    },
    Property {
        family: "Per-peer stream cap",
        name: "INV_WithinCap / INV_SlotsFollowStreams / INV_ForgottenAreIdle",
        axis: Tla,
        anchor: "crates/p2p/src/two_stream/peer_streams.rs PeerStreams::admit, StreamAdmission; crates/p2p/src/two_stream/runner.rs TwoStreamRunner::run",
        model_ref: "MC_PeerStreamCap_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // Replies to this node's own requests arrive as inbound streams and
        // share the per-peer cap and the max_p2p_tasks pool with requests, as
        // in Go's rcmgr. A reply past the cap is reset and shows up as a reply
        // timeout. libp2p only.
        family: "Per-peer stream cap",
        name: "INV_WithinCap counts replies with requests: a reply past the cap is reset",
        axis: Tla,
        anchor: "crates/p2p/src/two_stream/runner.rs TwoStreamRunner::run; crates/p2p/src/two_stream/peer_streams.rs PeerStreams::admit",
        model_ref: "MC_PeerStreamCap_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The receive gate: a replication filter asked when a push arrives,
        // before its head is stored, and again on a fetched CAR, before it is
        // imported; the merge asks again as the backstop.
        family: "Receive gate",
        name: "INV_ArrivalRefusalStoresNothing / INV_FetchRefusalImportsNothing / INV_RefusedNeverMerged / INV_KeptIsKept",
        axis: Tla,
        anchor: "crates/p2p/src/sync/coordinator/receive_gate.rs SyncCoordinator::gate_arrival, SyncCoordinator::gate_fetched; crates/db/src/receive_gate.rs DbReceiveGate::allows",
        model_ref: "MC_ReceiveGate_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
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
        anchor: "crates/p2p/src/sync/head_scope.rs ScopeDecision::of; crates/p2p/src/sync/manager/process/pushlog.rs process_block_inner; crates/p2p/src/sync/coordinator/broadcast.rs admit_replicator_push; crates/p2p/src/sync/replication/handlers.rs handle_block_received; crates/db/src/merge/txn_broadcaster.rs SyncTxnBroadcaster::submit_update; crates/defra-node/src/lib.rs Node::shutdown; crates/p2p-adapter/src/iroh_peer/peer.rs IrohPeer::assemble",
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
        anchor: "crates/p2p/src/sync/coordinator/mod.rs dispatch_due_pending_dag_fetches, DagFetchLimiter; crates/p2p/src/sync/coordinator/dag_fetcher.rs poll_fetch_dag, fetch_dag_attempt; crates/p2p/src/sync/coordinator/dag_retry.rs CLOCK_ATTEMPT_QUANTUM; crates/p2p/src/sync/coordinator/dag_context.rs DagFetchContext::finish_clock_attempt; crates/p2p/src/sync/manager/process/pending_dag.rs PendingDagLease::finish_attempt, due_pending_dag_retries, resync_persisted_pending_dags, records_after_resync_cursor, consume_pending_merge_continuation; crates/p2p/src/sync/manager/pending.rs PendingDag::is_expired, PENDING_ROTATE_AFTER_DISPATCHES; crates/p2p-adapter/src/retry.rs run_retry_pass; crates/db/src/merge/push_docs.rs retry_doc",
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
        anchor: "crates/p2p/src/iroh/endpoint_rpc.rs connect_with_cache, retire_timed_out_connection, REQUEST_RESPONSE_TIMEOUT; crates/p2p/src/iroh/endpoint_streams.rs handle_incoming",
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
        anchor: "crates/p2p/src/model_trace.rs Event; proofs/src/model_trace.rs Trace::abstracted, replay; proofs/tests/behavioral/model_trace.rs two_node_push_replays_through_p2p_delivery, hub_relay_replays_through_p2p_delivery",
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
        anchor: "crates/p2p/src/sync/manager/process/pushlog.rs process_block_inner; crates/p2p/src/sync/manager/process/pending_dag.rs resync_persisted_pending_dags; tools/integration-test/tests/p2p_admission_restart.rs",
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
        anchor: "crates/p2p/src/sync/manager/process/pushlog.rs process_block_inner; tools/integration-test/tests/p2p_admission.rs fan_in_pushlog_admission_no_silent_divergence",
        model_ref: "MC_PushLogAdmission_{Green,Red_SuccessOnFull}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // gate_fetched judges only roots with a resident pending record. Pulls
        // and expired unacknowledged entries take the Walk path: their CAR is
        // imported unchecked and refused at merge.
        family: "Receive gate",
        name: "INV_FetchRefusalImportsNothing holds for pushed roots only",
        axis: Tla,
        anchor: "crates/p2p/src/sync/coordinator/receive_gate.rs SyncCoordinator::gate_fetched",
        model_ref: "MC_ReceiveGate_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
