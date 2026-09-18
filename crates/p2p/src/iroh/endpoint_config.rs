//! Configuration helpers for the iroh endpoint.

use rapidhash::{HashSetExt, RapidHashSet};
use std::sync::Arc;

use iroh::{EndpointId, SecretKey};

use super::config::{IrohAllowlistConfig, IrohDiscoveryConfig, IrohRelayModeConfig};
use super::gossip_heal::GossipHealConfig;

// iroh 1.0 silently ignores custom values below its internal default plus one.
const MIN_CONCURRENT_MULTIPATH_PATHS: u32 = 9;

/// Configuration for creating an `IrohEndpoint`.
pub struct IrohEndpointConfig {
    pub secret_key: SecretKey,
    /// Optional Defra identity used for the Go-compatible peer identity
    /// challenge. The QUIC endpoint authenticates the requester; the returned
    /// token binds this DID to that requester's endpoint ID.
    pub node_identity: Option<Arc<identity::RawIdentity>>,
    /// Relay behavior for this endpoint.
    pub relay_mode: IrohRelayModeConfig,
    /// Address publishing / lookup behavior for this endpoint.
    pub discovery: IrohDiscoveryConfig,
    /// UDP port for the QUIC listener. `None` = ephemeral (OS-assigned).
    pub bind_port: Option<u16>,
    /// Bind to a specific IP address. When set, IROH only listens on this
    /// interface — prevents advertising unreachable LAN addresses to peers
    /// on different networks. None = 0.0.0.0 (all interfaces).
    pub bind_addr: Option<std::net::IpAddr>,
    /// Maximum QUIC paths that may be open concurrently for one connection.
    /// `None` keeps iroh's default; custom values must be at least 9.
    pub max_concurrent_multipath_paths: Option<u32>,
    /// Gossip send-path healing (#1092).
    pub gossip_heal: GossipHealConfig,
    /// Inbound-connection authorization. Defaults to accepting every peer,
    /// matching the transport's behavior before this allowlist existed.
    pub allowlist: IrohAllowlistConfig,
}

impl Default for IrohEndpointConfig {
    fn default() -> Self {
        Self {
            secret_key: SecretKey::generate(),
            node_identity: None,
            relay_mode: IrohRelayModeConfig::default(),
            discovery: IrohDiscoveryConfig::default(),
            bind_port: None,
            bind_addr: None,
            max_concurrent_multipath_paths: None,
            gossip_heal: GossipHealConfig::default(),
            allowlist: IrohAllowlistConfig::default(),
        }
    }
}

/// Runtime inbound-allowlist state, held by the endpoint for the life of the
/// process.
///
/// Mirrors [`IrohAllowlistConfig`] but keeps the explicit set behind a lock so
/// [`IrohCommand::AllowPeer`](super::command::IrohCommand::AllowPeer) can add
/// a newly authorized peer while the endpoint is running, without a restart.
pub(super) enum AllowlistState {
    AcceptAll,
    Explicit(parking_lot::Mutex<RapidHashSet<EndpointId>>),
}

impl AllowlistState {
    /// Whether an inbound connection from `id` may proceed.
    pub(super) fn is_allowed(&self, id: &EndpointId) -> bool {
        match self {
            Self::AcceptAll => true,
            Self::Explicit(ids) => ids.lock().contains(id),
        }
    }

    /// Add `id` to the explicit set. A no-op under `AcceptAll`: every peer is
    /// already accepted, so there is nothing to widen.
    pub(super) fn allow(&self, id: EndpointId) {
        if let Self::Explicit(ids) = self {
            ids.lock().insert(id);
        }
    }

    /// Drop `id` from the explicit set. Reports whether it was actually
    /// present, so a caller can tell a real withdrawal from one that changed
    /// nothing. A no-op under `AcceptAll`, which holds no set to narrow.
    ///
    /// On its own this is NOT a revocation, and must never be used as one.
    /// It only narrows who may open a NEW inbound connection; it does not
    /// touch a connection already open, and it does not stop this node from
    /// dialling the peer itself. [`PeerAdmission::revoke`] is the operation
    /// that actually bars a peer.
    fn remove(&self, id: &EndpointId) -> bool {
        match self {
            Self::AcceptAll => false,
            Self::Explicit(ids) => ids.lock().remove(id),
        }
    }
}

/// Who this endpoint will exchange connections with, in either direction.
///
/// Two gates, deliberately kept separate rather than folded into the one set:
///
/// - the allowlist answers "may this peer open a connection TO us", which is
///   all [`IrohAllowlistConfig`] has ever meant, and
/// - `revoked` answers "is this peer barred outright", and unlike the
///   allowlist it is consulted on the DIAL path as well.
///
/// Barring only the accept is not a revocation, and the difference is not
/// theoretical. This node dials peers on its own initiative: the replicator
/// reconnect sweep dials exactly those registered peers missing from
/// `connected_peers`, every `PERSISTED_RETRY_SWEEP_INTERVAL`. Cutting a
/// peer's connection is itself what marks it missing, so a peer barred on
/// the accept path alone is re-dialled BY US within seconds and regains full
/// stream service over the connection we opened. Hence a second set that
/// both directions consult, rather than a wider allowlist: only peers
/// someone explicitly revoked change behaviour, and every other peer dials
/// and is dialled exactly as before.
pub(super) struct PeerAdmission {
    allowlist: AllowlistState,
    revoked: parking_lot::Mutex<RapidHashSet<EndpointId>>,
}

impl PeerAdmission {
    pub(super) fn new(allowlist: AllowlistState) -> Self {
        Self {
            allowlist,
            revoked: parking_lot::Mutex::new(RapidHashSet::new()),
        }
    }

    /// Whether an inbound connection from `id` may proceed. Checked on every
    /// accepted connection in `endpoint_streams::handle_incoming`.
    pub(super) fn admits_inbound(&self, id: &EndpointId) -> bool {
        !self.is_revoked(id) && self.allowlist.is_allowed(id)
    }

    /// Whether this node may open a connection TO `id`.
    ///
    /// The allowlist deliberately does not participate: it describes who may
    /// come in, and a node is normally expected to dial peers that were
    /// never on it. Only an explicit revocation bars an outbound dial.
    pub(super) fn admits_outbound(&self, id: &EndpointId) -> bool {
        !self.is_revoked(id)
    }

    pub(super) fn is_revoked(&self, id: &EndpointId) -> bool {
        self.revoked.lock().contains(id)
    }

    /// Authorize `id`, lifting any revocation on it.
    ///
    /// The allowlist is widened first and the revocation lifted second, so
    /// there is no instant in which `id` counts as admissible without
    /// actually being on the list.
    pub(super) fn allow(&self, id: EndpointId) {
        self.allowlist.allow(id);
        self.revoked.lock().remove(&id);
    }

    /// Bar `id` in both directions. Reports whether this changed anything,
    /// so a caller can tell a real revocation from re-revoking a peer that
    /// was already barred; revoking an unknown peer is not an error.
    ///
    /// The revocation is recorded BEFORE the allowlist entry is dropped, so
    /// every check in between already refuses the peer. Under `AcceptAll`
    /// the allowlist step does nothing and the revocation alone carries it,
    /// which is why an `AcceptAll` endpoint can revoke a single peer without
    /// narrowing anyone else.
    ///
    /// This does not close a connection that is already open. That is a
    /// separate step the caller must also take, and the two are ordered:
    /// see `endpoint_commands::handle_deny_peer`.
    pub(super) fn revoke(&self, id: EndpointId) -> bool {
        let newly_revoked = self.revoked.lock().insert(id);
        let was_listed = self.allowlist.remove(&id);
        newly_revoked || was_listed
    }
}

pub(super) fn allowlist_state_from_config(
    config: &IrohAllowlistConfig,
) -> crate::error::Result<AllowlistState> {
    match config {
        IrohAllowlistConfig::AcceptAll => Ok(AllowlistState::AcceptAll),
        IrohAllowlistConfig::Explicit(ids) => {
            let mut parsed = RapidHashSet::with_capacity(ids.len());
            for id in ids {
                let endpoint_id: EndpointId = id.parse().map_err(|e: iroh::KeyParsingError| {
                    crate::error::Error::Transport(format!(
                        "invalid allowlisted iroh endpoint id '{}': {}",
                        id, e
                    ))
                })?;
                parsed.insert(endpoint_id);
            }
            Ok(AllowlistState::Explicit(parking_lot::Mutex::new(parsed)))
        }
    }
}

pub(super) fn apply_multipath_config(
    builder: iroh::endpoint::Builder,
    max_concurrent: Option<u32>,
) -> crate::error::Result<iroh::endpoint::Builder> {
    let Some(max_concurrent) = max_concurrent else {
        return Ok(builder);
    };

    if max_concurrent < MIN_CONCURRENT_MULTIPATH_PATHS {
        return Err(crate::error::Error::Transport(format!(
            "iroh max concurrent multipath paths must be at least {}, got {}",
            MIN_CONCURRENT_MULTIPATH_PATHS, max_concurrent
        )));
    }

    let transport_config = iroh::endpoint::QuicTransportConfig::builder()
        .max_concurrent_multipath_paths(max_concurrent)
        .build();
    tracing::info!(max_concurrent, "configured iroh multipath path limit");
    Ok(builder.transport_config(transport_config))
}

pub(super) fn relay_mode_from_config(
    config: &IrohRelayModeConfig,
) -> crate::error::Result<iroh::RelayMode> {
    match config {
        IrohRelayModeConfig::Default => Ok(iroh::endpoint::default_relay_mode()),
        IrohRelayModeConfig::Disabled => Ok(iroh::RelayMode::Disabled),
        IrohRelayModeConfig::Custom(urls) => {
            let relay_map = iroh::RelayMap::try_from_iter(urls.iter().map(String::as_str))
                .map_err(|e| {
                    crate::error::Error::Transport(format!("invalid relay URL list: {}", e))
                })?;
            Ok(iroh::RelayMode::Custom(relay_map))
        }
    }
}

/// A browser has no DNS resolver, so `DnsAddressLookup` is compiled out of iroh
/// there. Resolution instead goes back to the pkarr relay over HTTP, which
/// serves the same records the DNS lookup would have answered from.
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub(super) fn apply_discovery_config(
    mut builder: iroh::endpoint::Builder,
    config: &IrohDiscoveryConfig,
) -> crate::error::Result<iroh::endpoint::Builder> {
    use iroh::address_lookup::{PkarrPublisher, PkarrResolver};

    builder = match config {
        IrohDiscoveryConfig::N0 => builder
            .address_lookup(PkarrPublisher::n0_dns())
            .address_lookup(PkarrResolver::n0_dns()),
        IrohDiscoveryConfig::Disabled => builder.clear_address_lookup(),
        IrohDiscoveryConfig::CustomDns {
            origin_domain,
            pkarr_relay_url,
        } => {
            let _ = origin_domain;
            builder
                .address_lookup(PkarrPublisher::builder(
                    pkarr_relay_url
                        .parse()
                        .map_err(|e| invalid_pkarr_relay(pkarr_relay_url, e))?,
                ))
                .address_lookup(PkarrResolver::builder(
                    pkarr_relay_url
                        .parse()
                        .map_err(|e| invalid_pkarr_relay(pkarr_relay_url, e))?,
                ))
        }
    };

    Ok(builder)
}

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub(super) fn apply_discovery_config(
    mut builder: iroh::endpoint::Builder,
    config: &IrohDiscoveryConfig,
) -> crate::error::Result<iroh::endpoint::Builder> {
    use iroh::address_lookup::{DnsAddressLookup, PkarrPublisher};

    builder = match config {
        IrohDiscoveryConfig::N0 => builder
            .address_lookup(PkarrPublisher::n0_dns())
            .address_lookup(DnsAddressLookup::n0_dns()),
        IrohDiscoveryConfig::Disabled => builder.clear_address_lookup(),
        IrohDiscoveryConfig::CustomDns {
            origin_domain,
            pkarr_relay_url,
        } => {
            let pkarr_relay = pkarr_relay_url
                .parse()
                .map_err(|e| invalid_pkarr_relay(pkarr_relay_url, e))?;
            builder
                .address_lookup(PkarrPublisher::builder(pkarr_relay))
                .address_lookup(DnsAddressLookup::builder(origin_domain.clone()))
        }
    };

    Ok(builder)
}

fn invalid_pkarr_relay(url: &str, error: impl std::fmt::Display) -> crate::error::Error {
    crate::error::Error::Transport(format!("invalid pkarr relay URL '{}': {}", url, error))
}

/// A browser endpoint owns no UDP socket — it reaches peers only through a
/// relay — so a bind request here is a misconfiguration rather than something
/// to quietly drop.
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub(super) fn apply_bind_config(
    builder: iroh::endpoint::Builder,
    bind_addr: Option<std::net::IpAddr>,
    bind_port: Option<u16>,
) -> crate::error::Result<iroh::endpoint::Builder> {
    if bind_addr.is_some() || bind_port.is_some() {
        return Err(crate::error::Error::Transport(
            "a browser iroh endpoint cannot bind an address or port; it is relay-only".into(),
        ));
    }

    Ok(builder)
}

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub(super) fn apply_bind_config(
    mut builder: iroh::endpoint::Builder,
    bind_addr: Option<std::net::IpAddr>,
    bind_port: Option<u16>,
) -> crate::error::Result<iroh::endpoint::Builder> {
    use iroh::endpoint::BindOpts;

    let bind_error =
        |error| crate::error::Error::Transport(format!("invalid bind addr: {}", error));

    match (bind_addr, bind_port) {
        (Some(ip), port) => {
            builder = builder
                .bind_addr(std::net::SocketAddr::new(ip, port.unwrap_or(0)))
                .map_err(bind_error)?;
        }
        (None, Some(port)) => {
            builder = builder.clear_ip_transports();
            builder = builder
                .bind_addr(std::net::SocketAddr::new(
                    std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
                    port,
                ))
                .map_err(bind_error)?;
            builder = builder
                .bind_addr_with_opts(
                    std::net::SocketAddr::new(
                        std::net::IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
                        port,
                    ),
                    BindOpts::default().set_is_required(false),
                )
                .map_err(bind_error)?;
        }
        (None, None) => {}
    }

    Ok(builder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multipath_limit_rejects_values_iroh_would_ignore() {
        let builder = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal);
        let result =
            apply_multipath_config(builder, Some(super::MIN_CONCURRENT_MULTIPATH_PATHS - 1));

        assert!(matches!(
            result,
            Err(crate::error::Error::Transport(message))
                if message.contains("must be at least 9")
        ));
    }

    #[test]
    fn multipath_limit_accepts_iroh_minimum() {
        let builder = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal);

        assert!(
            apply_multipath_config(builder, Some(super::MIN_CONCURRENT_MULTIPATH_PATHS),).is_ok()
        );
    }

    #[test]
    fn accept_all_allows_any_endpoint_id() {
        let state = allowlist_state_from_config(&IrohAllowlistConfig::AcceptAll).unwrap();
        let id = iroh::SecretKey::generate().public();

        assert!(state.is_allowed(&id));
    }

    #[test]
    fn explicit_allowlist_rejects_unlisted_ids() {
        let listed = iroh::SecretKey::generate().public();
        let unlisted = iroh::SecretKey::generate().public();
        let state = allowlist_state_from_config(&IrohAllowlistConfig::Explicit(
            [listed.to_string()].into_iter().collect(),
        ))
        .unwrap();

        assert!(state.is_allowed(&listed));
        assert!(!state.is_allowed(&unlisted));
    }

    #[test]
    fn explicit_allowlist_rejects_malformed_endpoint_id() {
        let result = allowlist_state_from_config(&IrohAllowlistConfig::Explicit(
            ["not-an-endpoint-id".to_string()].into_iter().collect(),
        ));

        assert!(matches!(
            result,
            Err(crate::error::Error::Transport(message))
                if message.contains("invalid allowlisted iroh endpoint id")
        ));
    }

    #[test]
    fn allow_adds_a_peer_to_an_explicit_allowlist() {
        let newly_allowed = iroh::SecretKey::generate().public();
        let state =
            allowlist_state_from_config(&IrohAllowlistConfig::Explicit(RapidHashSet::new()))
                .unwrap();

        assert!(!state.is_allowed(&newly_allowed));
        state.allow(newly_allowed);
        assert!(state.is_allowed(&newly_allowed));
    }

    #[test]
    fn allow_is_a_no_op_under_accept_all() {
        let id = iroh::SecretKey::generate().public();
        let state = allowlist_state_from_config(&IrohAllowlistConfig::AcceptAll).unwrap();

        state.allow(id);
        assert!(matches!(state, AllowlistState::AcceptAll));
    }

    fn admission(config: IrohAllowlistConfig) -> PeerAdmission {
        PeerAdmission::new(allowlist_state_from_config(&config).unwrap())
    }

    #[test]
    fn revoke_bars_a_peer_in_both_directions() {
        let peer = iroh::SecretKey::generate().public();
        let admission = admission(IrohAllowlistConfig::Explicit(
            [peer.to_string()].into_iter().collect(),
        ));

        assert!(admission.admits_inbound(&peer));
        assert!(admission.admits_outbound(&peer));
        assert!(
            admission.revoke(peer),
            "the peer was admitted, so revoke must report a change"
        );

        assert!(!admission.admits_inbound(&peer));
        // The half that makes this a revocation rather than an allowlist
        // withdrawal: this node must also refuse to DIAL the peer, or its own
        // reconnect sweep undoes the revocation within seconds.
        assert!(!admission.admits_outbound(&peer));
    }

    #[test]
    fn revoke_under_accept_all_bars_only_that_peer() {
        let revoked = iroh::SecretKey::generate().public();
        let other = iroh::SecretKey::generate().public();
        let admission = admission(IrohAllowlistConfig::AcceptAll);

        assert!(admission.revoke(revoked), "revoking must report a change");

        assert!(!admission.admits_inbound(&revoked));
        assert!(!admission.admits_outbound(&revoked));
        // The reason the bar is a separate set rather than a narrowing of the
        // allowlist: an endpoint that accepts everyone can still cut off one
        // peer without turning into an explicit allowlist for everybody else.
        assert!(admission.admits_inbound(&other));
        assert!(admission.admits_outbound(&other));
    }

    #[test]
    fn revoking_an_unknown_peer_bars_it_and_reports_the_change() {
        let never_seen = iroh::SecretKey::generate().public();
        let admission = admission(IrohAllowlistConfig::Explicit(RapidHashSet::new()));

        assert!(
            admission.revoke(never_seen),
            "a peer that was never allowed is still newly barred, which is a change"
        );
        assert!(!admission.admits_inbound(&never_seen));
        assert!(!admission.admits_outbound(&never_seen));

        // Idempotent: re-revoking changes nothing and says so.
        assert!(!admission.revoke(never_seen));
    }

    #[test]
    fn allow_lifts_a_revocation_so_a_device_can_log_back_in() {
        let peer = iroh::SecretKey::generate().public();
        let admission = admission(IrohAllowlistConfig::Explicit(
            [peer.to_string()].into_iter().collect(),
        ));

        admission.revoke(peer);
        assert!(!admission.admits_inbound(&peer));

        admission.allow(peer);
        assert!(
            admission.admits_inbound(&peer),
            "allow must clear the bar, not just re-add the allowlist entry"
        );
        assert!(admission.admits_outbound(&peer));
        assert!(!admission.is_revoked(&peer));
    }

    #[test]
    fn revoke_does_not_widen_who_else_may_connect() {
        let revoked = iroh::SecretKey::generate().public();
        let listed = iroh::SecretKey::generate().public();
        let unlisted = iroh::SecretKey::generate().public();
        let admission = admission(IrohAllowlistConfig::Explicit(
            [revoked.to_string(), listed.to_string()]
                .into_iter()
                .collect(),
        ));

        admission.revoke(revoked);

        assert!(admission.admits_inbound(&listed));
        assert!(!admission.admits_inbound(&unlisted));
    }
}
