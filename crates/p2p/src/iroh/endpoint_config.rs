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

    /// Remove `id` from the explicit set, refusing its next inbound
    /// connection. Returns whether `id` was actually present, so a caller
    /// can tell a real revoke from denying a peer that was never allowed;
    /// denying an absent id is not an error.
    ///
    /// This only changes what [`Self::is_allowed`] returns for the NEXT
    /// accepted connection (checked in `handle_incoming`); it does nothing
    /// to a connection already open. Closing that is a separate step the
    /// caller must also take (see `IrohTransport::deny_peer`).
    ///
    /// Unlike [`Self::allow`], this is NOT a silent no-op under `AcceptAll`.
    /// Authorization can afford to no-op there because widening an
    /// already-total set changes nothing observable. Revocation cannot use
    /// the same excuse: under `AcceptAll` every OTHER peer would stay
    /// connectable, so silently reporting success would tell a caller a
    /// peer was cut off when it was not. This returns an error naming the
    /// situation instead of a false `Ok`.
    pub(super) fn deny(&self, id: &EndpointId) -> crate::error::Result<bool> {
        match self {
            Self::AcceptAll => Err(crate::error::Error::Transport(format!(
                "cannot deny peer {id}: this endpoint accepts every inbound peer (AcceptAll \
                 allowlist), so denying one id would not narrow who may connect"
            ))),
            Self::Explicit(ids) => Ok(ids.lock().remove(id)),
        }
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

    #[test]
    fn deny_removes_a_peer_from_an_explicit_allowlist() {
        let allowed = iroh::SecretKey::generate().public();
        let state = allowlist_state_from_config(&IrohAllowlistConfig::Explicit(
            [allowed.to_string()].into_iter().collect(),
        ))
        .unwrap();

        assert!(state.is_allowed(&allowed));
        assert!(
            state.deny(&allowed).unwrap(),
            "the peer was present, so deny must report true"
        );
        assert!(!state.is_allowed(&allowed));
    }

    #[test]
    fn deny_of_a_never_allowed_peer_is_not_an_error() {
        let never_allowed = iroh::SecretKey::generate().public();
        let state =
            allowlist_state_from_config(&IrohAllowlistConfig::Explicit(RapidHashSet::new()))
                .unwrap();

        // Not an error: the peer ends up in the desired state (denied)
        // either way. `Ok(false)` tells a caller nothing was actually
        // removed, distinct from a real revoke.
        assert!(
            !state.deny(&never_allowed).unwrap(),
            "denying a peer that was never allowed must report false, not error"
        );
        assert!(!state.is_allowed(&never_allowed));
    }

    #[test]
    fn deny_is_an_error_under_accept_all() {
        let id = iroh::SecretKey::generate().public();
        let state = allowlist_state_from_config(&IrohAllowlistConfig::AcceptAll).unwrap();

        let result = state.deny(&id);
        assert!(
            matches!(&result, Err(crate::error::Error::Transport(message)) if message.contains("AcceptAll")),
            "expected an AcceptAll-naming error, got {result:?}"
        );
        // Pin the decision: AcceptAll must not silently report a revoke that
        // did not happen.
        assert!(state.is_allowed(&id));
    }
}
