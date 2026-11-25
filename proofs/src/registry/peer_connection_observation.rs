use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Disconnect answers only once the peer's last connection closed, so
        // ActivePeers right after it no longer lists the peer, as in Go.
        family: "Peer connection observation",
        name: "INV_AnsweredMeansClosed / PROP_CallerAnswered",
        axis: Tla,
        anchor: "crates/database/p2p/src/host/command_handler/network.rs handle_disconnect; crates/database/p2p/src/host/p2p_host/swarm.rs handle_connection_closed; tools/integration-test/tests/testo/net/info/disconnect_peers.rs",
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
        anchor: "crates/database/p2p/src/iroh/endpoint_commands.rs handle_disconnect; crates/surfaces/p2p-adapter/src/iroh.rs connected_peers, connect_peer",
        model_ref: "MC_PeerDisconnect_Red_EarlyAck.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Peer connection observation",
        name: "INV_NoRedialAfterDeliberateClose / INV_RemoteExplicitDialAccepted",
        axis: Tla,
        anchor: "crates/database/p2p/src/behaviour/disconnect_gate.rs DisconnectGate, handle_established_inbound_connection; crates/database/p2p/src/host/p2p_host/mod.rs run_kademlia_bootstrap; crates/database/p2p/src/host/command_handler/network.rs handle_dial, handle_explicit_dial; crates/surfaces/p2p-adapter/src/libp2p.rs connect_peer; crates/database/p2p/src/host/command_handler/mod.rs handle_command; tools/integration-test/tests/testo/net/info/disconnect_reconnect.rs",
        model_ref: "MC_ExplicitDisconnect_{Green_Libp2p,Red_Libp2p,Red_ClearOnDial,Red_Inbound,Red_BackgroundInbound,Red_RefuseAllowed}.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/net/info/disconnect_reconnect.rs net_info_disconnect_sticks_until_explicit_reconnect",
    },
    Property {
        family: "Peer connection observation",
        name: "ExplicitDisconnectGate.blocked_peer_is_refused / explicit_reconnect_is_admitted / other_peer_is_unchanged",
        axis: Lean,
        anchor: "crates/database/p2p/src/behaviour/disconnect_gate.rs check, block, unblock",
        model_ref: "ExplicitDisconnectGate (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/behaviour/disconnect_gate.rs disconnected_peer_is_denied_until_unblocked",
    },
    Property {
        family: "Peer connection observation",
        name: "ExplicitOnly / ExactAddress / RequestedPort / DefaultPreserved / UnavailableRefused / MappingScope / MappingDefaultsPreserved",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_config.rs apply_bind_config; endpoint_config/bind_tests.rs native_bindings_match_executed_model, native_port_mapping_configuration_matches_executed_model, loopback_endpoint_does_not_open_wildcard_gateway_discovery_listeners",
        model_ref: "MC_EndpointBind_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Peer connection observation",
        name: "EndpointBind.explicit_replaces / explicit_preserves / absent_preserves_defaults / port_only_preserves_dual_stack / specific_address_disables_mapping / absent_address_preserves_mapping / wildcard_preserves_mapping",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/endpoint_config.rs apply_bind_config; endpoint_config/bind_tests.rs model_cases, native_port_mapping_configuration_matches_executed_model",
        model_ref: "EndpointBind (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Peer connection observation",
        name: "NoSpuriousDenial — queued lifecycle notification cannot hide a connected peer",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/authorizer.rs RuntimeAuthorizer::peer_connected; access_tests.rs live_connection_allows_pull_before_lifecycle_notification_without_granting_push",
        model_ref: "MC_PeerConnection_Green.cfg / MC_PeerConnection_Red_StaleObservation.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/src/sync/coordinator/access_tests.rs live_connection_allows_pull_before_lifecycle_notification_without_granting_push",
    },
    Property {
        family: "Peer connection observation",
        name: "PeerConnection.live_is_connected / connection_does_not_grant_push",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/coordinator/authorizer.rs RuntimeAuthorizer::peer_connected",
        model_ref: "PeerConnection.lean",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/src/sync/coordinator/access_tests.rs live_connection_allows_pull_before_lifecycle_notification_without_granting_push",
    },
    Property {
        family: "Peer connection observation",
        name: "Reverse RPC reuses inbound and legacy paths; stale eviction cannot hide replacements from disconnect",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_streams.rs adopt_new_connection; endpoint_rpc.rs remember_connection, connect_with_cache and evict_connection; crates/database/p2p/tests/iroh/shared_connection.rs",
        model_ref: "MC_SharedRpcConnection_{Green,Red_*}.cfg; MC_LegacyRpcCache_{Green,Red_*}.cfg; MC_RpcCacheEviction_{Green,Red_*}.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/tests/iroh/shared_connection.rs disconnect_closes_replacement_despite_stale_cleanup, legacy_only_peer_reuses_and_refreshes_its_shared_rpc_path, accepted_inbound_connection_serves_reverse_rpc_without_another_dial, authenticated_reconnection_replaces_an_open_old_rpc_path",
    },
    Property {
        // When the cached RPC handle closes, its slot is left empty and the
        // next RPC dials rather than promoting an open sibling. That is cheap
        // only because every dialled connection is adopted and served; a
        // sibling of unknown liveness may be a connection whose peer is gone.
        family: "Peer connection observation",
        name: "A closed cached RPC connection is replaced by a fresh dial, never by promoting a sibling",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_rpc.rs connect_with_cache, evict_connection; crates/database/p2p/src/iroh/endpoint_streams.rs handle_connection_streams",
        model_ref: "MC_SharedRpcConnection_{Green,Red_NoAdoption}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Peer connection observation",
        name: "Every new connection is served, announced by its pump and released once; an allowlist miss keeps the sibling",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_streams.rs adopt_new_connection and finish_connection; endpoint_rpc.rs connect_with_cache and restore_displaced_connection; endpoint_commands.rs handle_dial; crates/database/p2p/tests/iroh/shared_connection.rs",
        model_ref: "MC_ConnectionAdoption_{Green,Red_*}.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/tests/iroh/shared_connection.rs repeated_cache_hits_reuse_one_adopted_connection, closed_registry_refuses_adoption_and_balances_the_peer_map, cancelled_adopt_still_announces_and_releases_its_connection, allowlist_miss_refuses_only_the_new_connection_and_spares_the_sibling",
    },
    Property {
        family: "Peer connection observation",
        name: "Open RPC connections stay counted after cache displacement and close on revoke",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_rpc.rs close_peer_connections; crates/database/p2p/tests/iroh/shared_connection.rs displaced_rpc_dialed_connection_is_closed_by_revoke",
        model_ref: "MC_ConnectionAdoption_{Green,Red_*}.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/tests/iroh/shared_connection.rs displaced_rpc_dialed_connection_is_closed_by_revoke",
    },
    Property {
        family: "Peer connection observation",
        name: "Refused connection is evicted, a closed displaced connection is not restored, and a newer connection keeps the slot",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_rpc.rs restore_displaced_connection, evict_connection",
        model_ref: "MC_DisplacedConnectionRestore_{Green,Red_NoEvict,Red_Closed,Red_EvictAny,Red_Overwrite}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/iroh/endpoint_rpc.rs a_closed_displaced_handle_evicts_the_refused_connection, a_closed_displaced_restore_spares_a_concurrent_replacement",
    },
    Property {
        family: "Peer connection observation",
        name: "SharedRpc replacement identity, legacy reuse, modern preference, revoked refusal, stable-id eviction and refused-publish restore",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/endpoint_rpc.rs remember_connection, restore_displaced_connection, connect_with_cache and evict_connection; endpoint_streams.rs adopt_new_connection",
        model_ref: "PeerConnection.SharedRpc",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/tests/iroh/shared_connection.rs authenticated_reconnection_replaces_an_open_old_rpc_path, legacy_only_peer_reuses_and_refreshes_its_shared_rpc_path, legacy_mux_does_not_replace_the_modern_shared_rpc_handle, disconnect_closes_replacement_despite_stale_cleanup",
    },
    Property {
        // A generic dial cannot clear an explicit disconnect marker.
        family: "Peer connection observation",
        name: "INV_GenericKeepsMarker",
        axis: Tla,
        anchor: "crates/database/p2p/src/host/command_handler/network.rs P2PHost::handle_dial, handle_explicit_dial, handle_disconnect",
        model_ref: "MC_GenericDisconnectDial_{Green,Red_Clear}.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/net/info/disconnect_reconnect.rs a_replicator_retry_does_not_redial_a_disconnected_peer",
    },
    Property {
        // Rejected connections do not emit unmatched disconnect events.
        family: "Peer connection observation",
        name: "INV_NoSpuriousDisconnect",
        axis: Tla,
        anchor: "crates/database/p2p/src/host/p2p_host/swarm.rs P2PHost::handle_connection_closed",
        model_ref: "MC_PeerDisconnectEvents_{Green,Red_Unannounced}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/host/p2p_host/swarm.rs refused_connection_does_not_emit_a_disconnect",
    },
    Property {
        // A disconnect is emitted only for a previously announced peer.
        family: "Peer connection observation",
        name: "PeerDisconnectEvents.unannounced_is_silent / announced_is_reported",
        axis: Lean,
        anchor: "crates/database/p2p/src/host/p2p_host/swarm.rs P2PHost::handle_connection_closed",
        model_ref: "PeerDisconnectEvents (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/host/p2p_host/swarm.rs refused_connection_does_not_emit_a_disconnect, peer_disconnect_only_on_last_connection_close",
    },
];
