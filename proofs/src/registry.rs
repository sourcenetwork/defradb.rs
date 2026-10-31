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
    /// `file:symbol` the model is grounded to.
    pub anchor: &'static str,
    /// The TLC config (`*.cfg`) or Lean theorem that establishes it.
    pub model_ref: &'static str,
    /// How this property is bound to the implementation.
    pub tiers: &'static [Tier],
}

use Axis::{Lean, Tla};
use Tier::{Behavioral, Boundary, Contract};

pub const PROPERTIES: &[Property] = &[
    Property {
        family: "HTTP router composition",
        name: "WholeRouter / SingleConstructor / NoBypass / ForwardingPreserved",
        axis: Tla,
        anchor: "crates/http/src/server.rs Server::router, Server::serve; router_transform_tests::forwarding_filter_matches_executed_model; crates/defra-node/src/router_transform_tests.rs",
        model_ref: "MC_HttpRouterComposition_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "HTTP router composition",
        name: "configured_order / constructor_once / absent_preserves / forwarding_preserves / refuses_every_inner_status",
        axis: Lean,
        anchor: "crates/http/src/server.rs Server::router; router_transform_tests::forwarding_filter_matches_executed_model",
        model_ref: "HttpRouterComposition (lake build)",
        tiers: &[Boundary],
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
    },
    Property {
        family: "Peer connection observation",
        name: "ExplicitOnly / ExactAddress / RequestedPort / DefaultPreserved / UnavailableRefused / MappingScope / MappingDefaultsPreserved",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/endpoint_config.rs apply_bind_config; endpoint_config/bind_tests.rs native_bindings_match_executed_model, native_port_mapping_configuration_matches_executed_model, loopback_endpoint_does_not_open_wildcard_gateway_discovery_listeners",
        model_ref: "MC_EndpointBind_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Peer connection observation",
        name: "EndpointBind.explicit_replaces / explicit_preserves / absent_preserves_defaults / port_only_preserves_dual_stack / specific_address_disables_mapping / absent_address_preserves_mapping / wildcard_preserves_mapping",
        axis: Lean,
        anchor: "crates/p2p/src/iroh/endpoint_config.rs apply_bind_config; endpoint_config/bind_tests.rs model_cases, native_port_mapping_configuration_matches_executed_model",
        model_ref: "EndpointBind (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Selective CAR capacity: observed completed grants yield to active sends; in-flight scope and quiescent limits are preserved",
        axis: Tla,
        anchor: "crates/p2p/src/sync/coordinator/selective_car_access.rs register, reclaim_completed and allows_root",
        model_ref: "MC_SelectiveCarCapacity_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "admitted_bounded / warm_does_not_refuse / reclaim_preserves_active / authorized_scope / expired_completed_refused",
        axis: Lean,
        anchor: "crates/p2p/src/sync/coordinator/selective_car_access.rs register, reclaim_completed and allows_root",
        model_ref: "SelectiveCarCapacity (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Selected transfer owns its acknowledgements independently of subscriptions; replay stays fenced, scoped and grant-bound",
        axis: Tla,
        anchor: "crates/db/src/merge/push_docs.rs push_existing_docs_by_id",
        model_ref: "MC_DocumentPush_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Selected exact peer/root authority survives cache loss, restart and time until explicit revocation",
        axis: Tla,
        anchor: "crates/storage/src/stores/peerstore/selected_car.rs; crates/p2p/src/sync/coordinator/event_handler/car.rs",
        model_ref: "MC_SelectedCarAuthority_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Selected pushes ignore subscription filters and retry schedules, retain hint ownership and drain all started tasks",
        axis: Tla,
        anchor: "crates/db/src/merge/push_docs.rs push_existing_docs_by_id",
        model_ref: "MC_SelectedPushIsolation_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "revoked_denied / other_preserved / exact_scope",
        axis: Lean,
        anchor: "crates/storage/src/stores/peerstore/selected_car.rs",
        model_ref: "SelectedCarAuthority (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "selection_owns_acknowledgements / absent_replay_is_refused / selected_failure_is_not_deferred / selected_retains_heads / absent_peer_has_no_observation",
        axis: Lean,
        anchor: "crates/db/src/merge/push_docs.rs; crates/db/tests/merge/push_docs_selection.rs",
        model_ref: "DocumentPush.lean; selected_document_transfers_match_executed_model",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Rooted authorization: one producer, exact grants, bounded optional retention, fenced eviction and cancellation wakeup",
        axis: Tla,
        anchor: "crates/p2p/src/sync/car_authorization.rs walk_with_limits; car_authorization_registry.rs; crates/p2p/tests/unit/car_authorization_limits.rs",
        model_ref: "MC_RootedAuthorization_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "reserve_bounded / release_restores / replacement_survives / grants_independent_of_retention",
        axis: Lean,
        anchor: "crates/p2p/src/sync/car_authorization_budget.rs; car_authorization_registry.rs",
        model_ref: "RootedAuthorization (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Replicator installs validate their row, settle before completion, and retain interrupted outcomes",
        axis: Tla,
        anchor: "crates/p2p-adapter/src/replicator_install_store.rs; crates/p2p-adapter/src/replicator_installs.rs",
        model_ref: "MC_ReplicatorInstall_{Green,Red_*}.cfg; MC_ReplicatorIncarnation_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Install completion is identity-fenced and idempotent; stale allocations re-decide without regressing the incarnation",
        axis: Lean,
        anchor: "crates/p2p-adapter/src/replicator_install_store.rs finish",
        model_ref: "ReplicatorInstall (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_Released — an early reconciliation return releases its queued continuation slot",
        axis: Tla,
        anchor: "crates/p2p/src/sync/replication/handlers.rs::process_one_event; crates/p2p/src/sync/replication/continuation_tests.rs::already_merged_duplicate_continuation_releases_reservation_without_handler",
        model_ref: "MC_ContinuationSlot_{Green,Red_LateClaim}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "owned_slot_released — dropping the continuation owner releases its slot",
        axis: Lean,
        anchor: "crates/p2p/src/sync/coordinator/mod.rs::claim_pending_dag_merge; crates/p2p/src/sync/replication/handlers.rs::process_one_event",
        model_ref: "HistoryTurns.owned_slot_released (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Rejected-page cleanup compares the cursor incarnation before removing it",
        axis: Tla,
        anchor: "crates/db/src/merge/merge_handler/history.rs::reject_history; crates/db/tests/merge/history_cursor.rs",
        model_ref: "MC_RetryMarkerGeneration_{Green,Red_StaleAck,Red_Readd}.cfg (incarnation-fenced retirement abstraction)",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "History turns commit cursor and writes together; reclaim preserves publications",
        axis: Tla,
        anchor: "crates/db/src/merge/merge_handler/history.rs; history_registry.rs; dispatch.rs; crates/db/src/merge/replication.rs; crates/db/tests/merge/collection_history_control.rs",
        model_ref: "MC_HistoryTurns_{Green,Red_*}.cfg; MC_HistoryDriver_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "Ancestor publication is idempotent; cleanup and collection carriers preserve driver ownership",
        axis: Lean,
        anchor: "crates/db/src/merge/merge_handler/history.rs",
        model_ref: "HistoryTurns (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Storage failure classification",
        name: "ClassificationPreserved / NoReplayAuthority",
        axis: Tla,
        anchor: "crates/storage/src/corekv/errors.rs; crates/db/src/error/mod.rs; crates/query/src/runner/turn.rs; crates/query/src/executor.rs; crates/http/src/error.rs",
        model_ref: "MC_StorageFailure_Green.cfg / MC_StorageFailure_Red_LostCause.cfg / MC_StorageFailure_Red_Text.cfg / MC_StorageFailure_Red_Retry.cfg",
        tiers: &[Contract],
    },
    Property {
        family: "Storage failure classification",
        name: "StorageFailure.preserves / full_code / other_code / opaque_code",
        axis: Lean,
        anchor: "crates/storage/src/corekv/io_failure.rs; crates/storage/src/backends/regolith/transaction.rs map_txn_error; crates/query/src/executor.rs QueryResponseError::from_query_error; crates/query/tests/storage_failure.rs",
        model_ref: "StorageFailure.lean",
        tiers: &[Contract],
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "Owner and visibility observations retain read gates and the transaction snapshot",
        axis: Tla,
        anchor: "crates/query/src/runner/owners.rs; crates/db/src/read/lensed/fetcher/mod.rs",
        model_ref: "MC_OwnerObservation_Green.cfg / MC_OwnerObservation_Red_*.cfg",
        tiers: &[Contract],
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "Acp.OwnerObservation.exact_snapshot, unreadable_observation_hidden and unregistered_read_keeps_visibility",
        axis: Lean,
        anchor: "crates/query/src/runner/owners.rs",
        model_ref: "Acp.OwnerObservation",
        tiers: &[Contract],
    },
    Property {
        family: "Peer connection observation",
        name: "NoSpuriousDenial — queued lifecycle notification cannot hide a connected peer",
        axis: Tla,
        anchor: "crates/p2p/src/sync/coordinator/authorizer.rs RuntimeAuthorizer::peer_connected; access_tests.rs live_connection_allows_pull_before_lifecycle_notification_without_granting_push",
        model_ref: "MC_PeerConnection_Green.cfg / MC_PeerConnection_Red_StaleObservation.cfg",
        tiers: &[Contract],
    },
    Property {
        family: "Peer connection observation",
        name: "PeerConnection.live_is_connected / connection_does_not_grant_push",
        axis: Lean,
        anchor: "crates/p2p/src/sync/coordinator/authorizer.rs RuntimeAuthorizer::peer_connected",
        model_ref: "PeerConnection.lean",
        tiers: &[Contract],
    },
    Property {
        family: "Peer connection observation",
        name: "Reverse RPC reuses inbound and legacy paths; stale eviction cannot hide replacements from disconnect",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/endpoint_streams.rs handle_incoming; endpoint_rpc.rs remember_connection, connect_with_cache and evict_connection; crates/p2p/tests/iroh/shared_connection.rs",
        model_ref: "MC_SharedRpcConnection_{Green,Red_*}.cfg; MC_LegacyRpcCache_{Green,Red_*}.cfg; MC_RpcCacheEviction_{Green,Red_*}.cfg",
        tiers: &[Contract],
    },
    Property {
        family: "Peer connection observation",
        name: "SharedRpc replacement identity, legacy reuse, modern preference, revoked refusal and stable-id eviction",
        axis: Lean,
        anchor: "crates/p2p/src/iroh/endpoint_rpc.rs remember_connection, connect_with_cache and evict_connection; endpoint_streams.rs handle_incoming",
        model_ref: "PeerConnection.SharedRpc",
        tiers: &[Contract],
    },
    Property {
        family: "Schema publication completion",
        name: "VisibleBeforeReply — committed schemas are published before their callers resume",
        axis: Tla,
        anchor: "crates/storage/src/corekv/callbacks.rs OnCommit::OrderedAndWait; crates/db/tests/collection/schema_publication.rs",
        model_ref: "MC_SchemaPublication_Green.cfg / MC_SchemaPublication_Red_EarlyReply.cfg",
        tiers: &[Contract],
    },
    Property {
        family: "Schema publication completion",
        name: "SchemaPublication.preserves",
        axis: Lean,
        anchor: "crates/storage/src/corekv/callbacks.rs OnCommit::OrderedAndWait; crates/db/tests/collection/schema_publication.rs",
        model_ref: "SchemaPublication.lean",
        tiers: &[Contract],
    },
    Property {
        family: "DiskANN pending cache",
        name: "INV_NoHiddenPending — a search using the in-memory pending list never misses a vector its snapshot or its own transaction has pending",
        axis: Tla,
        anchor: "crates/db/src/index/vector/engine/diskann/graph.rs pending_candidates; crates/db/src/index/vector/engine/diskann/pending_cache.rs PendingCache; crates/db/src/index/vector/engine/diskann/mod.rs insert, delete; crates/db/src/index/vector/engine/diskann/merge.rs merge_plan, merge",
        model_ref: "MC_PendingCache_Green.cfg (GREEN) / MC_PendingCache_Red_NoGeneration.cfg / MC_PendingCache_Red_OwnWrites.cfg / MC_PendingCache_Red_TruncateThroughHead.cfg (RED)",
        tiers: &[Contract],
    },
    Property {
        family: "DiskANN pending cache",
        name: "prefix_covered — a cache loaded once and caught up on every logged insert holds every vector a snapshot of its generation has pending, and a merge that cuts the log anywhere loses no live vector",
        axis: Lean,
        anchor: "crates/db/src/index/vector/engine/diskann/pending_cache.rs PendingCache; crates/db/src/index/vector/engine/diskann/mod.rs insert; crates/db/src/index/vector/engine/diskann/merge.rs merge_plan, merge",
        model_ref: "PendingCache.Covers.prefix_covered / catch_up_in_steps / merge_keeps_every_vector / truncate_through_head_loses",
        tiers: &[Contract],
    },
    Property {
        family: "DiskANN pending cache",
        name: "INV_ScoresLatest — a search scores each pending vector by the newest entry its snapshot or its own transaction holds, never an obsolete one or one from a cache ahead of it",
        axis: Tla,
        anchor: "crates/db/src/index/vector/engine/diskann/graph.rs pending_candidates, snapshot_pending; crates/db/src/index/vector/engine/diskann/pending_cache.rs Copy::for_each_latest_through, Copy::caught_up, Segment::push, Segment::supersede",
        model_ref: "MC_PendingCache_Green.cfg (GREEN) / MC_PendingCache_Red_EveryVersion.cfg / MC_PendingCache_Red_FutureVersion.cfg / MC_PendingCache_Red_HeadBlindMarks.cfg / MC_PendingCache_Red_NoGapCheck.cfg / MC_PendingCache_Red_NoGeneration.cfg (RED)",
        tiers: &[Contract],
    },
    Property {
        family: "DiskANN pending cache",
        name: "scored_is_last — the code kept for a vector is that of its last entry visited, and a copy cut at a search's head scores as the search's own log does",
        axis: Lean,
        anchor: "crates/db/src/index/vector/engine/diskann/graph.rs pending_candidates; crates/db/src/index/vector/engine/diskann/pending_cache.rs Copy::for_each_latest_through",
        model_ref: "PendingCache.Latest.scored_is_last / scored_append / cut_scores_snapshot / uncut_scores_future",
        tiers: &[Contract],
    },
    Property {
        family: "DiskANN pending cache",
        name: "kept_is_newest — a search that skips an entry only when its superseded marker is at or before the search's head keeps exactly each vector's newest entry in reach",
        axis: Lean,
        anchor: "crates/db/src/index/vector/engine/diskann/pending_cache.rs Segment::push, Segment::supersede, Copy::for_each_latest_through",
        model_ref: "PendingCache.Marks.kept_is_newest / newest_is_kept / one_kept_per_vector / next_decides / head_blind_marker_hides",
        tiers: &[Contract],
    },
    Property {
        family: "DiskANN pending cache",
        name: "Evict — forgetting the pending cache at any moment, as an index or collection drop does, keeps every search's candidates complete and scored by their newest entry",
        axis: Tla,
        anchor: "crates/db/src/index/vector/engine/diskann/cache.rs CodeCaches::evict, evict_collection; crates/db/src/collection/ops/delete.rs delete_collection_as; crates/db/src/collection/ops/version.rs delete_collection_version_as",
        model_ref: "MC_PendingCache_Green_Evict.cfg (GREEN)",
        tiers: &[Contract],
    },
    Property {
        family: "DiskANN shared quantizer",
        name: "INV_EncodesWithItsCodebooks — a write sharing the node's quantizer encodes only with its own snapshot's codebooks, across rebuilds and retried trainings",
        axis: Tla,
        anchor: "crates/db/src/index/vector/engine/diskann/mod.rs DiskAnn::quantizer; crates/db/src/index/vector/engine/diskann/cache.rs CodeCache::quantizer, CodeCache::share_quantizer; crates/db/src/index/vector/engine/diskann/build.rs train_and_build",
        model_ref: "MC_SharedQuantizer_Green.cfg (GREEN) / MC_SharedQuantizer_Red_NoTagCheck.cfg / MC_SharedQuantizer_Red_TagPerRequest.cfg / MC_SharedQuantizer_Red_OneSlot.cfg (RED)",
        tiers: &[Contract],
    },
    Property {
        family: "DiskANN shared quantizer",
        name: "decode_encode — the trained record reads back as written in both encodings, and the shared quantizer answers a tag only with that tag's codebooks",
        axis: Lean,
        anchor: "crates/db/src/index/vector/engine/diskann/codec.rs encode_trained, decode_trained; crates/db/src/index/vector/engine/diskann/cache.rs CodeCache::quantizer, CodeCache::share_quantizer",
        model_ref: "SharedQuantizer.Tagged.decode_encode / lookup_is_books / share_keeps_consistent / reused_tag_serves_other_books / share_keeps_previous / one_slot_evicts",
        tiers: &[Contract],
    },
    Property {
        family: "RobustPrune blocked covered checks",
        name: "blocked_eq_sequential — checking candidates in blocks keeps exactly the edges the one-at-a-time walk keeps, for any cut into blocks",
        axis: Lean,
        anchor: "crates/db/src/index/vector/engine/select/robust_prune.rs RobustPrune::select; crates/defra-core/src/vector/kernel/mod.rs squared_euclidean_block",
        model_ref: "RobustPruneBlocks.blocked_eq_sequential / no_since_keeps_a_covered_edge",
        tiers: &[Contract],
    },
    Property {
        family: "DiskANN back-edges in place",
        name: "in_place_eq_decoded — adding back-edges to an encoded record in place leaves the entries and count that decoding, adding and encoding would",
        axis: Lean,
        anchor: "crates/db/src/index/vector/engine/diskann/codec.rs add_edges; crates/db/src/index/vector/engine/diskann/graph.rs add_back_edges",
        model_ref: "BackEdgesInPlace.in_place_eq_decoded / stale_search_duplicates_an_edge",
        tiers: &[Contract],
    },
    Property {
        family: "Vector index link maintenance",
        name: "INV_NoLostLinks — blind additions preserve links and stale structural pruning aborts",
        axis: Tla,
        anchor: "crates/storage/src/backends/regolith/merge/vector.rs; crates/db/src/index/vector/engine/hnsw/insert.rs; crates/db/tests/vector/concurrent_hnsw.rs",
        model_ref: "MC_VectorLinks_Green.cfg / MC_VectorLinks_Red_Puts.cfg / MC_VectorLinks_Red_Prune.cfg",
        tiers: &[Contract],
    },
    Property {
        // Two writers appending to one collection without seeing each other must
        // both land, leaving two tips for a later merge: that is what makes the
        // head set a CRDT rather than a last-writer-wins cell. The eager-delete
        // strategy makes them write the same key, which regolith refuses at
        // every isolation level, so the RED configuration is the defect and the
        // GREEN one is the derived-head fix.
        family: "Concurrent collection-head transitions",
        name: "INV_NoWriteConflict — concurrent appends to one collection both commit and form siblings",
        axis: Tla,
        anchor: "crates/db/src/block/builder/collection.rs write_collection_block; crates/storage/src/backends/regolith/transaction.rs",
        model_ref: "MC_HeadSet_Green.cfg (GREEN) / MC_HeadSet_Red_EagerDelete.cfg (RED) / MC_HeadSet_Red_PerKeyScan.cfg (RED)",
        tiers: &[Behavioral],
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
    },
    Property {
        // The algebra the temporal model rests on: every key a writer writes
        // names that writer, so two writers cannot collide (disjointness), and
        // the eager strategy demonstrably does collide (non-vacuity). Appending
        // a block makes it a head and un-heads the parents it named, which is
        // what the delete used to achieve.
        family: "Concurrent collection-head transitions",
        name: "derived_writeSets_disjoint / eager_writeSets_overlap / applyDerived_parents_not_head",
        axis: Lean,
        anchor: "crates/db/src/block/heads/document.rs iterator/supersede; crates/db/src/block/heads.rs live_collection_heads; crates/db/src/block/builder/collection.rs write_collection_block",
        model_ref: "HeadSet.Core (lake build)",
        tiers: &[Contract],
    },
    Property {
        family: "Commit-ordered document arrivals",
        name: "dense positions / commit order / at-most-once / snapshot prefix",
        axis: Tla,
        anchor: "crates/db/src/event/arrivals.rs record; crates/storage/src/backends/regolith/transaction.rs append; crates/db/tests/read/arrival_transactions.rs",
        model_ref: "regolith@e844baefe838f3af76403afd46c4b42f1a0b98a5 proofs/tla/CommitOrderedAppend.tla (GREEN Groups/Pipeline/Faults, RED Counter/OnceFromView/AssignBeforeValidation)",
        tiers: &[Behavioral],
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_ProviderOncePerContext / INV_NewRegistrationSurvivesOldAck / INV_NewRegistrationSurvivesOldClear",
        axis: Tla,
        anchor: "crates/db/src/write/autocommit/{create,update,delete}.rs NeedsInput resolver; crates/db/src/write/create.rs NeedsInput waves; crates/query/src/prepared.rs; crates/storage/src/stores/peerstore.rs register/complete/clear retry scope",
        model_ref: "MC_RetryPreparedInputs_{Green,Red_*}.cfg / MC_RetryMarkerGeneration_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_QueuedPushOwnsLiveMarker — admission requires current durable ownership and elapsed backpressure",
        axis: Tla,
        anchor: "crates/storage/src/stores/peerstore.rs admit_push_scope; crates/p2p-adapter/src/retry.rs spawn_failure_recorder; crates/p2p/src/sync/coordinator/push_worker.rs run_push_job",
        model_ref: "MC_RetryPushAdmission_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Retry input and marker fencing",
        name: "INV_RegistrationCannotCrossFence — allocated registrations preserve peer incarnation and marker order",
        axis: Tla,
        anchor: "crates/storage/src/stores/peerstore.rs RegisterScopeMarkerAttempt; create_replicator",
        model_ref: "MC_RetryRegistrationFence_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Collection schema-version OCC",
        name: "INV_NoStaleSchemaWrite — a writer cannot commit against an obsolete structural version",
        axis: Tla,
        anchor: "crates/db/src/txn/mod.rs validate_collection_for_write; crates/db/src/write/autocommit/{create,update,delete}.rs; crates/db/src/definition structural updates",
        model_ref: "MC_CollectionVersionOCC_Green.cfg / MC_CollectionVersionOCC_Red_NoValidation.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Explicit transaction statement atomicity",
        name: "INV_AbortedEffectsRemoved / INV_SuccessfulEffectsRetained / INV_NoCommitWhileCleanupPending",
        axis: Tla,
        anchor: "crates/query/src/runner/mutation.rs explicit transaction mutation execution; crates/db/src/txn/mod.rs statement savepoint; crates/storage/src/backends/regolith/transaction.rs savepoint callbacks",
        model_ref: "MC_StatementAtomicity_Green.cfg / MC_StatementAtomicity_Red_*.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "B3 filtered replication",
        name: "INV_DagComplete — filtered push still converges per-document",
        axis: Tla,
        anchor: "crates/p2p filtered replication; crates/db (merge)",
        model_ref: "MC_S4_ModelB.cfg / M1Convergence.cfg",
        tiers: &[Behavioral],
    },
    Property {
        // Two behavioral legs: live-forward convergence (replication.rs) and
        // convergence across a real partition (partition.rs) — node1 is restarted
        // to sever the link, each side writes independently, and both converge to
        // hold every write once reconnected.
        family: "DAG convergence (partition / eviction / restart)",
        name: "INV_Converged — every node receives every delta under eventual connectivity",
        axis: Tla,
        anchor: "crates/db (merge) merge handler; crates/blockstore",
        model_ref: "MC_Conv_Eventual.cfg",
        tiers: &[Behavioral],
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
    },
    Property {
        family: "CRDT merge laws",
        name: "PCounterGuard: the positive counter's op accepts only non-negative values, so its value never decreases under any run of accepted or refused writes whose sum stays inside i64 (an i64 sum wraps, as in Go)",
        axis: Lean,
        anchor: "crates/crdt/src/field/counter.rs CounterField::op; crates/crdt/tests/field_contract.rs only_the_positive_counter_refuses_a_negative_value",
        model_ref: "DefraConvergence.PCounterGuard.run_ge (lake build)",
        tiers: &[Contract],
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
        anchor: "crates/p2p/src/replicator.rs; crates/db/src/merge/push_docs_transport.rs",
        model_ref: "MC_Replicator_Resumable_Green.cfg",
        tiers: &[Behavioral],
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
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_EventualCurrency / LIVE_EventualReceiverQuiescence — document scope",
        axis: Tla,
        anchor: "crates/p2p/src/sync coordinator + pending DAG clock; crates/db (merge) document marker rederive",
        model_ref: "MC_SyncOwnership_Green_Liveness_Doc.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "LIVE_EventualCurrency / LIVE_EventualReceiverQuiescence — collection scope",
        axis: Tla,
        anchor: "crates/p2p/src/sync coordinator + pending DAG clock; crates/db (merge) collection marker rederive",
        model_ref: "MC_SyncOwnership_Green_Liveness_Collection.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_FetchHasQualifiedProvider / LIVE_EventualCurrency — immutable origin plus durable same-root alternate",
        axis: Tla,
        anchor: "crates/p2p/src/sync/manager/process/pushlog.rs; crates/p2p/src/sync/pending_store.rs; crates/p2p/src/sync/coordinator/mod.rs",
        model_ref: "MC_SyncOwnership_Green_Liveness_IrohRelay.cfg",
        tiers: &[Boundary],
    },
    Property {
        // A second PushLog for a CID already being processed joins that
        // processing instead of being nacked: it waits, bounded by a deadline,
        // looks again once the owner lets go, and either acks a block already
        // merged or registered, or takes the guard and processes its own copy.
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "INV_SuccessImpliesDurable / INV_OneProcessor / LIVE_EveryJoinEnds: same-CID PushLog joins the in-flight owner",
        axis: Tla,
        anchor: "crates/p2p/src/sync/manager/process/pushlog.rs process_pushlog_with_provider_evidence, join_in_flight; crates/p2p/src/sync/queue.rs ProcessQueue::try_acquire",
        model_ref: "MC_PushLogJoin_{Green,Red_NoRecheck,Red_NoGuard,Red_NoDeadline}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Sync ownership transfer (head hint / receiver pull)",
        name: "acquire_exclusive / join_acks_only_durable / join_processes_only_unowned: the CID guard is exclusive and a join acks only a safe block",
        axis: Lean,
        anchor: "crates/p2p/src/sync/queue.rs ProcessQueue::try_acquire, release_sync; crates/p2p/src/sync/manager/process/pushlog.rs join_in_flight; crates/p2p/src/sync/coordinator/mod.rs DagFetchLimiter::claim_root, RootClaim::drop",
        model_ref: "SingleFlight.Join (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Multi-instance claim",
        name: "INV_EventualUnique — claim CAS converges to a single winner",
        axis: Tla,
        anchor: "gents:crates/gents/src/lifecycle/claim.rs:claim_inner (foreign substrate, not the defradb.rs binary)",
        model_ref: "MC_Claim_Filtered_Eventual.cfg",
        tiers: &[Boundary],
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
        anchor: "crates/db/src/merge/push_docs.rs; crates/db/src/merge/push_docs_transport.rs; crates/crdt/src/field/lww.rs",
        model_ref: "MC_EncryptedLwwReplay_Green.cfg",
        tiers: &[Behavioral],
    },
    Property {
        // The model and focused KMS/merge unit tests cover the retry mechanics,
        // but the external harness cannot deterministically hold a DEK request
        // unavailable between ciphertext acknowledgement and receiver restart.
        family: "Encrypted LWW restart/replay",
        name: "INV_AckBacked — acknowledged encrypted replay remains re-drivable across transient KMS failure and restart",
        axis: Tla,
        anchor: "crates/db/src/merge/merge_handler/composite_fields.rs; crates/p2p/src/sync/pending_store.rs",
        model_ref: "MC_EncryptedLwwReplay_Green.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Management-channel auth (NAC gate)",
        name: "INV_OnlyAuthorizedManages — management ops require a valid scoped token",
        axis: Tla,
        anchor: "crates/http manage channel; crates/db (nac)",
        model_ref: "MC_Auth_Green.cfg",
        tiers: &[Behavioral],
    },
    Property {
        family: "ACP soundness + revocation + dual-path commits",
        name: "INV_RevocationConsistent + both User and _commits paths gated",
        axis: Tla,
        anchor: "crates/acp; crates/zanzibar; crates/query dag-scan",
        model_ref: "MC_Acp_Green.cfg / MC_Commits_Green.cfg",
        tiers: &[Behavioral, Contract],
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
        anchor: "crates/vera/src/vera_rs/provider/verified.rs verified, ATTEMPTS; crates/vera/src/vera_rs/provider.rs create_policy, query_policy, query_object_owner, verify_access, create_access_decision; acp-light-client HeaderChain::accept_response",
        model_ref: "MC_VerifiedRead_{Green,Green_Exhausted,Red_AcceptStale,Red_NoRetry,Red_Uncapped}.cfg",
        tiers: &[Boundary],
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
    },
    Property {
        // Listing lenses returns every module of every lens, in order, as
        // Go's `model.Lens`; it used to keep only the first.
        family: "Schema-evolution operations in the core",
        name: "LensListing.listed_keeps_modules / listed_length",
        axis: Lean,
        anchor: "crates/lens/src/wasm.rs WasmLensStore::list; crates/lens/src/store.rs list; crates/db/src/txn/registry/lifecycle.rs list_lenses_as",
        model_ref: "LensListing (lake build)",
        tiers: &[Boundary],
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
    },
    Property {
        family: "Storage transaction validation",
        name: "Validated point reads reject crossing writes",
        axis: Tla,
        anchor: "crates/storage/src/backends/regolith/transaction.rs; proofs/tests/behavioral/ssi.rs",
        model_ref: "regolith@e844baefe838f3af76403afd46c4b42f1a0b98a5 proofs/tla/DefraLevel.tla",
        tiers: &[Behavioral, Boundary],
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
    },
    Property {
        family: "NAC lifecycle privilege-escalation",
        name: "INV_NoPrivEsc — no escalation across enable/disable/restart",
        axis: Tla,
        anchor: "crates/db (nac) lifecycle",
        model_ref: "MC_Nac_Green.cfg",
        tiers: &[Behavioral],
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
    },
    Property {
        family: "Transaction & merge-queue concurrency",
        name: "exactly_one_wins / one_lowering_per_raise / caller_win_means_owner_lowers / uncancelled_owner_wins: the two swaps settle each fence once",
        axis: Lean,
        anchor: "crates/db/src/txn/fence.rs Fence::abandon, Fence::finish",
        model_ref: "CommitFence.Swap (lake build)",
        tiers: &[Boundary],
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
    },
    Property {
        family: "Engine ticker export",
        name: "failed_attempt_changes_nothing / run_gain / run_total_eq_gains / publishAlone_comm / closed_export_adds_exactly: a mark moved by compare-and-swap adds each count once",
        axis: Lean,
        anchor: "crates/telemetry/src/engine_tickers.rs Source::publish",
        model_ref: "EngineTickers.Mark, EngineTickers.Race (lake build)",
        tiers: &[Boundary],
    },
    Property {
        // A placed pool claims its CPUs in the process's ledger, one counter
        // per CPU: alone, moving each counter from 0 to 1 with one
        // compare-and-swap and giving back what it took when one CPU is
        // held; or shared, adding one whatever the counter was. After its
        // capped claims alone fail it shares and says so. Its Hold gives back
        // exactly what it raised once the pool and every thread placed
        // through it are gone.
        family: "Worker pool CPU ledger",
        name: "INV_AloneTakesDisjoint / INV_AloneRunsAlone / INV_TakenAlone / INV_AllOrNothing / INV_HeldNeverFree / INV_CountExact / INV_DoneLeavesNothing / LIVE_StartEnds: placed pools never share a CPU unless one of them says it shares",
        axis: Tla,
        anchor: "crates/workers/src/ledger.rs Ledger::hold, Ledger::share, Hold::drop; crates/workers/src/placement/reserve.rs reserve, shared; crates/workers/src/pool/placer.rs Placer::holding; crates/workers/src/pool/start.rs Pool::start_in",
        model_ref: "MC_CpuLedger_{Green,Red_SplitTake,Red_NoRollback,Red_ReleaseLoser,Red_ShareUncounted,Red_NoAttemptCap,Red_ReleaseEarly,Red_RunOutsideHold,Red_DoubleRelease,Red_ForgetHold}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Worker pool CPU ledger",
        name: "hold_all_or_nothing / rollback_restores / hold_won_alone / hold_step / share_step / release_alone_step / release_shared_step: a claim alone takes all or nothing, the counters stay exact, Holds claimed alone stay apart",
        axis: Lean,
        anchor: "crates/workers/src/ledger.rs Ledger::hold, Ledger::share, Hold::drop",
        model_ref: "CpuLedger.{Hold,Count,Exclusive} (lake build)",
        tiers: &[Boundary],
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
    },
    Property {
        family: "Worker default pool",
        name: "first_running_wins / stopped_never_selected / later_registrations_change_nothing / first_calls_agree / registered_wins / stop_deadline_elsewhere: the first running registered pool wins, a stopped one never does, every caller gets one started pool",
        axis: Lean,
        anchor: "crates/workers/src/pool/default.rs DefaultPool::register, DefaultPool::get",
        model_ref: "DefaultPool.{Slot,Cell,Get} (lake build)",
        tiers: &[Boundary],
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
    },
    Property {
        family: "CID content-addressing determinism + Block canonicalization",
        name: "cid_injective_mod_hash — same normal-form content yields same CID",
        axis: Lean,
        anchor: "crates/defra-core/src/block.rs Block::generate_cid",
        model_ref: "Cid (lake build)",
        tiers: &[Behavioral, Contract],
    },
    Property {
        family: "Deferred-ACP overlay consistency",
        name: "INV_FailClosedActive — txn-local ACP projection gates as committed state would",
        axis: Tla,
        anchor: "crates/query/src/txn/context.rs; crates/db/src/write/autocommit/batch.rs",
        model_ref: "MC_DeferredAcp_Green.cfg",
        tiers: &[Behavioral],
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
    },
    Property {
        family: "DiskANN walk cutoff",
        name: "cut_step_eq_step — skipping neighbours ranked after a full frontier's last place leaves the frontier the full step would",
        axis: Lean,
        anchor: "crates/db/src/index/vector/engine/diskann/graph.rs walk, each_edge, by_distance",
        model_ref: "WalkCutoff.cut_step_eq_step / distance_only_cutoff_drops_a_tie",
        tiers: &[Contract],
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
    },
    Property {
        family: "Filter relation paths and scalar leaves",
        name: "newValidate_false_iff / newRun_eq_mainRun — validation rejects exactly a sub-selection on a non-relation field, and the queries that run are main's",
        axis: Lean,
        anchor: "crates/query/src/runner/plan_validation.rs validate_select; crates/query/src/planner/joins/selection_join.rs Planner::apply_selection_relation_join",
        model_ref: "JsonSubPathCut.leafOk_false_iff / newValidate_false_iff / newValidate_of_main / newRun_eq_mainRun",
        tiers: &[Contract],
    },
    Property {
        family: "PQ distance four sums",
        name: "four_eq_sum — four running sums over a code's lookups add up to the one running sum, in exact arithmetic",
        axis: Lean,
        anchor: "crates/db/src/index/vector/quantize/pq.rs ProductQuantizer::distance",
        model_ref: "PqFourSums.four_eq_sum / dropped_tail_loses_lookups",
        tiers: &[Contract],
    },
    Property {
        family: "Query fragment nesting bound",
        name: "reach_le / walk_none_iff — the fragment walk never calls itself deeper than max_fragment_nesting, and refuses exactly the queries that nest deeper",
        axis: Lean,
        anchor: "crates/query/src/query_parse/limits.rs FragmentPath::check_nesting; crates/query/src/subscription.rs collect_root_fields",
        model_ref: "FragmentNesting.reach_le / walk_none_iff / off_by_one_accepts_too_deep",
        tiers: &[Contract],
    },
    Property {
        family: "Index-maintenance consistency",
        name: "onDocumentUpdate_correct / _no_stale / _none_missing",
        axis: Lean,
        anchor: "crates/db (index) index maintenance",
        model_ref: "IndexMaintenance (lake build)",
        tiers: &[Behavioral, Contract],
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
    },
    Property {
        // A node's shutdown aborts every task it holds and waits for each to
        // let go of storage before the pool stops and regolith closes; the
        // drain gives up at its deadline; a refused telemetry flush moves to
        // a thread of its own instead of the caller.
        family: "Node shutdown order",
        name: "INV_NoStorageAfterClose / INV_NothingDetached / INV_CallerNeverJoinsExporter / LIVE_ShutdownTerminates: no task reaches storage after regolith closes, none outlives shutdown, shutdown returns",
        axis: Tla,
        anchor: "crates/defra-node/src/lib.rs Node::shutdown; crates/defra-node/src/node_acp.rs gate_external_acp; crates/defra-node/src/managed_p2p.rs ShutdownHandle::stop, stop_services; crates/edge/src/task.rs Service::stopped; crates/edge/src/handle.rs Scope::drain",
        model_ref: "MC_NodeShutdown_{Green,Red_RedriveDetached,Red_ServicesNotAwaited,Red_NoDrainDeadline,Red_FlushOnCaller}.cfg",
        tiers: &[Boundary],
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
    },
    Property {
        family: "Node startup",
        name: "new_keeps / new_valid / new_accepts_valid / cap_mem / cap_positive / cap_follows_ladder / cap_repeats_last / default_accepted",
        axis: Lean,
        anchor: "crates/storage/src/stores/retry_info.rs RetrySchedule::new, RetrySchedule::cap, RETRY_INTERVALS_SECS",
        model_ref: "RetrySchedule (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Inbound stream admission",
        name: "INV_ResidentWithinBudget / INV_ShareBound / LIVE_HealthyHandled / LIVE_WaitEnds",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/endpoint_streams.rs admit_stream; crates/p2p/src/iroh/inbound_budget.rs InboundBudget::admit, InboundBudget::peer_share, InboundDeadlines::body; crates/p2p/src/sync/dispatch_queue.rs dispatch_queue; crates/p2p/src/sync/event_dispatcher.rs run_event_dispatcher",
        model_ref: "MC_InboundAdmission_Green.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Inbound stream admission",
        name: "acquire_conserves / release_all_restores / admit_failure_keeps_nothing",
        axis: Lean,
        anchor: "crates/p2p/src/iroh/inbound_budget.rs InboundBudget::admit, InboundLease",
        model_ref: "ByteBudget (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Peer fetch stats",
        name: "roll_ratio_stays_a_ratio / rolling_over_samples / bump_never_overflows",
        axis: Lean,
        anchor: "crates/p2p/src/iroh/fetch_stats.rs PeerCounters::record, roll",
        model_ref: "FetchStats (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Gossip connection ownership",
        name: "INV_RevokedHasNoGossip / INV_PathOnLiveConnection / INV_GossipNeverClosesShared / LIVE_HealthyReached / LIVE_AskAnswered",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/gossip/driver.rs Driver::send, Driver::connected, Driver::forget; crates/p2p/src/iroh/gossip/mod.rs GossipHandle::connected; crates/p2p/src/iroh/endpoint_rpc.rs connect_with_cache; crates/p2p/src/iroh/endpoint_commands.rs handle_deny_peer",
        model_ref: "MC_GossipOwnership_Green.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Gossip connection ownership",
        name: "LIVE_RequestAnswered",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/gossip/inbox.rs Inbox::push, Inbox::offer, Inbox::next; crates/p2p/src/iroh/gossip/driver.rs read_stream, Driver::emit; crates/p2p/src/iroh/records/task.rs RecordsHandle::push_change, RecordsHandle::push, RecordsHandle::next",
        model_ref: "MC_GossipInbox_Green.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Gossip connection ownership",
        name: "decode_encode / decode_all_encode_all / decode_rejects_oversized / read_tag_write_tag",
        axis: Lean,
        anchor: "crates/p2p/src/iroh/gossip/frame.rs write_gossip_frame, read_gossip_frame; crates/p2p/src/iroh/protocols.rs write_stream_tag, read_stream_tag",
        model_ref: "GossipFrame (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Iroh service records",
        name: "INV_SeqNeverGoesBack / INV_NoExpiredChosen / LIVE_NewestReached",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/records/book.rs Book::offer, Book::live; crates/p2p/src/iroh/records/refresh.rs Refresh::check; crates/p2p/src/iroh/records/task.rs Records::renew, Records::on_input, Records::rotate",
        model_ref: "MC_ServiceRecords_Green.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Reply-driven DAG fetch",
        name: "INV_OneFetchPerRoot / INV_NoReplyLost",
        axis: Tla,
        anchor: "crates/p2p/src/sync/coordinator/mod.rs DagFetchLimiter::claim_root, RootKey, RootClaim; crates/p2p/src/sync/coordinator/event_handler/doc_sync.rs handle_doc_sync_reply; crates/p2p/src/sync/coordinator/event_handler/branchable_sync.rs handle_branchable_sync_reply",
        model_ref: "MC_RootSingleFlight_Green.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Iroh service records",
        name: "INV_AskOnlyWhenEmpty / LIVE_Found",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/records/task.rs Records::providers, Records::on_message",
        model_ref: "MC_ProviderLookup_Green.cfg",
        tiers: &[Boundary],
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
    },
    Property {
        family: "Document write pipeline",
        name: "INV_NoStoreBeforeComposite / INV_NoMintInAttempt / INV_OneKeyPerField / INV_CommittedWhole",
        axis: Tla,
        anchor: "crates/db/src/write/create.rs embed, identity, seed_identity, memo_identity, prepare_keys, compute, compute_many, persist; crates/db/src/write/autocommit/create.rs create_many_with_resolved_inputs, create_impl, run_create_attempt; crates/db/src/write/autocommit/batch.rs BatchMutator::create_as (with a memo); crates/db/src/write/doc.rs DbDocMutator::create_as, next_create_identity; crates/db/src/write/update.rs write_local, write_blocks; crates/db/src/write/autocommit/update.rs run_update_attempt; crates/db/src/write/autocommit/batch.rs BatchMutator::update_as (with a memo); crates/db/src/write/doc.rs DbDocMutator::update_as; crates/db/src/block/builder/write.rs build_document_blocks, encrypt_with_new_key",
        model_ref: "MC_WritePipeline_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Add collection",
        name: "INV_NoCollectionUnlessAllChecksPass / INV_AllOrNone",
        axis: Tla,
        anchor: "crates/db/src/collection/ops/add/mod.rs DB::add_collection, prepare_new_collections, check_policies, check_collection_patch; crates/db/src/txn/registry/schemas.rs add_collection_in_txn_as; crates/db/src/collection/ops/create.rs create_collections_atomic_inner",
        model_ref: "MC_AddCollection_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Collection truncate",
        name: "INV_NoRemovalWithoutPermission / INV_FilteredNeverOnBranchable / INV_FilteredRemovesOnlyCommittedMatches / INV_BadFilterRemovesNothing / INV_UnfilteredRemovesAll",
        axis: Tla,
        anchor: "crates/db/src/collection/ops/truncate/mod.rs DB::truncate_collection_as, DB::truncate_parsed_as, DB::truncate_target, conditions, DB::truncate; crates/db/src/collection/ops/truncate/all.rs DB::truncate_all; crates/db/src/collection/ops/truncate/filtered.rs DB::truncate_matching",
        model_ref: "MC_Truncate_{Green,Red_*}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Iroh solicited replies",
        name: "INV_NoUnsolicitedAdmitted / INV_SolicitedAdmitted / INV_NoReplyStreamOnV2",
        axis: Tla,
        anchor: "crates/p2p/src/iroh/solicited.rs Solicited::expect, Solicited::take, Solicited::prune; crates/p2p/src/iroh/endpoint_streams.rs admit_stream, dispatch_stream; crates/p2p/src/iroh/endpoint_commands.rs handle_command; crates/p2p/src/iroh/endpoint_rpc.rs handle_reply_on_stream",
        model_ref: "MC_SolicitedReplies_Green.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Iroh service records",
        name: "validate_sound / validate_complete / offer_never_lowers / offer_replaces_expired / offer_keeps_only_valid / live_not_expired / encodeKey_injective / no_secret_not_advertised / distinct_ids_distinct_keys / parse_print / providers_exclude_self / providers_nodup / providers_from_book / providers_within_cap / providers_sorted",
        axis: Lean,
        anchor: "crates/p2p/src/iroh/records/verify.rs verify; crates/p2p/src/iroh/records/book.rs Book::offer, Book::live; crates/p2p/src/iroh/records/key.rs blind, advertised_key, CollectionKey::encode; crates/p2p/src/iroh/records/pointer.rs Pointer::print, Pointer::parse; crates/p2p/src/iroh/records/task.rs Records::providers, rank",
        model_ref: "ServiceRecord (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Document save",
        name: "INV_NoFork / INV_DeletedStaysDeleted / INV_RefusedOnlyDeleted",
        axis: Tla,
        anchor: "crates/query/src/documents/mod.rs save; crates/defra-node/src/ops/document.rs Node::save_doc; tools/integration-test/tests/mutation/update/with_delete.rs update_save_deleted_doc_does_nothing",
        model_ref: "MC_DocumentSave_{Green,Red_NoDeletedCheck,Red_SplitTxn}.cfg",
        tiers: &[Boundary],
    },
    Property {
        family: "Document save",
        name: "decide_refuses_deleted / refuse_only_deleted / save_never_forks / save_under_given_id",
        axis: Lean,
        anchor: "crates/query/src/documents/mod.rs save; crates/query/src/plan/mutation/upsert.rs UpsertNode::next",
        model_ref: "DocumentSave (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Mutation value checks",
        name: "Rfc3339Nano.no_trailing_zero / trim_restores / all_zero_trims_to_nothing / nonzero_keeps_a_digit",
        axis: Lean,
        anchor: "crates/document/src/encoding.rs trim_rfc3339_trailing_zeros, format_time_rfc3339_nano",
        model_ref: "Rfc3339Nano (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Mutation value checks",
        name: "DocIdText.length_refusal_only_for_unaccepted / accepted_length_never_says_length / digits_length / parse_accepts",
        axis: Lean,
        anchor: "crates/defra-core/src/doc_id.rs parse_uuid, ParsedDocId::from_string",
        model_ref: "DocIdText (lake build)",
        tiers: &[Boundary],
    },
    Property {
        family: "Mutation value checks",
        name: "SecondaryRelation.primary_never_refused / secondary_id_refused / same_answer_for_relation_and_id / checks_agree",
        axis: Lean,
        anchor: "crates/query/src/runner/mutation_inputs.rs normalize_mutation_input_fields, input_field_name; crates/document/src/document.rs Document::set",
        model_ref: "SecondaryRelation (lake build)",
        tiers: &[Boundary],
    },
];
