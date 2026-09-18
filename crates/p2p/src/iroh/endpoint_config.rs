//! Configuration helpers for the iroh endpoint.

use rapidhash::fast::RandomState;
use std::sync::Arc;

use iroh::{EndpointId, SecretKey};
use kovan_map::HopscotchMap;

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
/// Mirrors [`IrohAllowlistConfig`] but keeps the explicit set concurrent so
/// [`IrohCommand::AllowPeer`](super::command::IrohCommand::AllowPeer) can add
/// a newly authorized peer while the endpoint is running, without a restart.
pub(super) enum AllowlistState {
    AcceptAll,
    Explicit(HopscotchMap<EndpointId, (), RandomState>),
}

impl AllowlistState {
    /// Whether an inbound connection from `id` may proceed.
    pub(super) fn is_allowed(&self, id: &EndpointId) -> bool {
        match self {
            Self::AcceptAll => true,
            Self::Explicit(ids) => ids.contains_key(id),
        }
    }

    /// Add `id` to the explicit set. A no-op under `AcceptAll`: every peer is
    /// already accepted, so there is nothing to widen.
    pub(super) fn allow(&self, id: EndpointId) {
        if let Self::Explicit(ids) = self {
            ids.insert_if_absent(id, ());
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
            Self::Explicit(ids) => ids.remove(id).is_some(),
        }
    }

    /// Widen like [`Self::allow`], but report whether this call is the one
    /// that actually added `id`.
    ///
    /// Needed so a caller that has to undo its own widening can remove only
    /// what it put there, and never an entry a concurrent caller added.
    fn allow_reporting(&self, id: EndpointId) -> bool {
        match self {
            Self::AcceptAll => false,
            Self::Explicit(ids) => ids.insert_if_absent(id, ()).is_none(),
        }
    }
}

/// What a caller is permitted to do to this endpoint's peer admission state.
///
/// Carried down to the state change rather than checked before it, on purpose.
/// Admission is a state machine with exactly one transition that UNDOES a
/// security decision (revoked -> admitted), and only a caller allowed to make
/// that decision may reverse it. Resolving the caller's authority up here and
/// then applying it inside the same lock as the transition is what makes that
/// a lockstep: a revoke landing concurrently is either entirely before the
/// transition, in which case the transition sees it and refuses, or entirely
/// after, in which case it re-bars the peer. There is no window in which an
/// authority decision made against one state is applied to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmissionAuthority {
    may_admit: bool,
    may_revoke: bool,
}

impl AdmissionAuthority {
    /// `may_admit` is the authority to widen who may connect in; `may_revoke`
    /// is the authority to bar a peer outright. Lifting an existing
    /// revocation needs BOTH, because it is an admission that reverses a
    /// revocation.
    pub fn new(may_admit: bool, may_revoke: bool) -> Self {
        Self {
            may_admit,
            may_revoke,
        }
    }

    /// A caller that may admit a peer but may not revoke one, and therefore
    /// may not lift an existing revocation either.
    pub fn admit_only() -> Self {
        Self::new(true, false)
    }

    /// A caller holding both authorities.
    pub fn full() -> Self {
        Self::new(true, true)
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
    revoked: HopscotchMap<EndpointId, (), RandomState>,
}

impl PeerAdmission {
    pub(super) fn new(allowlist: AllowlistState) -> Self {
        Self {
            allowlist,
            revoked: HopscotchMap::with_hasher(RandomState::default()),
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
        self.revoked.contains_key(id)
    }

    /// Authorize `id`, lifting any revocation on it, if `authority` allows.
    ///
    /// Refused when the peer is currently revoked and the caller does not hold
    /// the authority to revoke: a principal that cannot bar a peer must not be
    /// able to un-bar one, or the revocation is only as strong as the weakest
    /// permission anyone holds.
    ///
    /// There is no lock to hold across the decision, so the weaker path is
    /// written as check, widen, then check again, and it undoes its own
    /// widening if a bar appeared in between. That is what keeps it a single
    /// step in effect: a [`Self::revoke`] landing at ANY point is either seen
    /// by the first check, or by the second. It cannot land in a gap and leave
    /// the peer admitted, because `revoke` records the bar before it narrows
    /// the allowlist and [`Self::admits_inbound`] refuses on the bar alone.
    ///
    /// The undo removes only an entry this call added (`allow_reporting`), so
    /// a full-authority admission running concurrently is never rolled back by
    /// a weaker caller losing the race.
    pub(super) fn allow(
        &self,
        id: EndpointId,
        authority: AdmissionAuthority,
    ) -> crate::error::Result<()> {
        if !authority.may_admit {
            return Err(crate::error::Error::Transport(format!(
                "cannot admit peer {id}: caller is not authorized to admit peers"
            )));
        }

        if authority.may_revoke {
            // Allowed to clear a bar, so the two writes need no ordering
            // against each other: this caller is permitted to end in the
            // admitted state either way.
            self.allowlist.allow(id);
            self.revoked.remove(&id);
            return Ok(());
        }

        if self.is_revoked(&id) {
            return Err(Self::revoked_refusal(&id));
        }
        let added = self.allowlist.allow_reporting(id);
        if self.is_revoked(&id) {
            if added {
                self.allowlist.remove(&id);
            }
            return Err(Self::revoked_refusal(&id));
        }
        Ok(())
    }

    fn revoked_refusal(id: &EndpointId) -> crate::error::Error {
        crate::error::Error::Transport(format!(
            "cannot admit peer {id}: the peer is revoked, and lifting a revocation \
             needs the same authority that can revoke one"
        ))
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
        // Same lock order as `allow` (revoked, then allowlist) and held across
        // both, so the two transitions serialise against each other instead of
        // interleaving halfway.
        // The bar is recorded BEFORE the allowlist is narrowed, so every check
        // in between already refuses the peer: `admits_inbound` requires the
        // absence of a bar as well as an allowlist entry, and
        // `admits_outbound` consults the bar alone.
        let newly_revoked = self.revoked.insert_if_absent(id, ()).is_none();
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
            let parsed = HopscotchMap::with_capacity_and_hasher(ids.len(), RandomState::default());
            for id in ids {
                let endpoint_id: EndpointId = id.parse().map_err(|e: iroh::KeyParsingError| {
                    crate::error::Error::Transport(format!(
                        "invalid allowlisted iroh endpoint id '{}': {}",
                        id, e
                    ))
                })?;
                parsed.insert_if_absent(endpoint_id, ());
            }
            Ok(AllowlistState::Explicit(parsed))
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
    use rapidhash::{HashSetExt, RapidHashSet};

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

        admission.allow(peer, AdmissionAuthority::full()).unwrap();
        assert!(
            admission.admits_inbound(&peer),
            "allow must clear the bar, not just re-add the allowlist entry"
        );
        assert!(admission.admits_outbound(&peer));
        assert!(!admission.is_revoked(&peer));
    }

    #[test]
    fn a_caller_without_revoke_authority_cannot_lift_a_revocation() {
        let peer = iroh::SecretKey::generate().public();
        let admission = admission(IrohAllowlistConfig::Explicit(
            [peer.to_string()].into_iter().collect(),
        ));

        admission.revoke(peer);

        let refused = admission.allow(peer, AdmissionAuthority::admit_only());
        assert!(
            refused.is_err(),
            "a caller that cannot revoke must not be able to un-revoke"
        );
        // And the refusal changed nothing: the peer is still barred, both ways.
        assert!(!admission.admits_inbound(&peer));
        assert!(!admission.admits_outbound(&peer));
        assert!(admission.is_revoked(&peer));
    }

    #[test]
    fn a_caller_with_revoke_authority_can_lift_a_revocation() {
        let peer = iroh::SecretKey::generate().public();
        let admission = admission(IrohAllowlistConfig::Explicit(
            [peer.to_string()].into_iter().collect(),
        ));

        admission.revoke(peer);
        admission
            .allow(peer, AdmissionAuthority::full())
            .expect("the authority that revoked may also restore");

        assert!(admission.admits_inbound(&peer));
        assert!(admission.admits_outbound(&peer));
        assert!(!admission.is_revoked(&peer));
    }

    #[test]
    fn admit_only_authority_still_admits_a_peer_that_was_never_revoked() {
        let peer = iroh::SecretKey::generate().public();
        let admission = admission(IrohAllowlistConfig::Explicit(RapidHashSet::new()));

        // The gate is on UNDOING a revocation, not on ordinary admission, so
        // the weaker authority must keep working for the ordinary case.
        admission
            .allow(peer, AdmissionAuthority::admit_only())
            .expect("admitting a peer that was never revoked needs no revoke authority");
        assert!(admission.admits_inbound(&peer));
    }

    #[test]
    fn a_caller_with_no_admit_authority_cannot_admit_at_all() {
        let peer = iroh::SecretKey::generate().public();
        let admission = admission(IrohAllowlistConfig::Explicit(RapidHashSet::new()));

        let refused = admission.allow(peer, AdmissionAuthority::new(false, false));
        assert!(
            refused.is_err(),
            "admission requires the authority to admit"
        );
        assert!(!admission.admits_inbound(&peer));
    }

    #[test]
    fn revoke_then_refused_restore_leaves_the_peer_barred_not_half_admitted() {
        let peer = iroh::SecretKey::generate().public();
        let admission = admission(IrohAllowlistConfig::Explicit(
            [peer.to_string()].into_iter().collect(),
        ));

        admission.revoke(peer);
        let _ = admission.allow(peer, AdmissionAuthority::admit_only());

        // The refused transition must not have half-applied: specifically it
        // must not have re-added the allowlist entry while leaving the bar,
        // which would quietly restore the peer the moment anyone lifted the
        // revocation for an unrelated reason.
        let restored = admission.allow(peer, AdmissionAuthority::full());
        assert!(restored.is_ok());
        assert!(admission.admits_inbound(&peer));

        // Re-revoking still reports a real change, proving the earlier refusal
        // left the state machine exactly where it was.
        assert!(admission.revoke(peer));
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
