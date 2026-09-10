//! Public configuration types for Defra's iroh transport.

use std::collections::HashSet;

/// Inbound-connection authorization for an iroh endpoint.
///
/// The iroh transport otherwise accepts every inbound QUIC connection: gossip
/// topic ids are derived from collection ids, which are content-derived and
/// therefore guessable by anyone running the same schema. `AcceptAll` keeps
/// today's behavior (fine on a private network); `Explicit` restricts inbound
/// connections to a known set of iroh endpoint ids, which matters once a node
/// is reachable from the internet (relay-enabled).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum IrohAllowlistConfig {
    /// Accept an inbound connection from any peer. Matches the transport's
    /// behavior before this allowlist existed.
    #[default]
    AcceptAll,
    /// Accept an inbound connection only from the listed iroh endpoint ids
    /// (each the string form of an `iroh::EndpointId`).
    Explicit(HashSet<String>),
}

/// Relay configuration for an iroh endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum IrohRelayModeConfig {
    /// Use iroh's default relay behavior.
    #[default]
    Default,
    /// Disable relay-assisted connectivity entirely.
    Disabled,
    /// Use a custom set of relay URLs.
    Custom(Vec<String>),
}

/// Address lookup / discovery configuration for an iroh endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum IrohDiscoveryConfig {
    /// Use iroh's default Number 0 discovery stack (pkarr publisher + DNS lookup).
    #[default]
    N0,
    /// Disable address lookup and publishing.
    Disabled,
    /// Use a custom DNS origin and pkarr relay for publishing / lookup.
    CustomDns {
        origin_domain: String,
        pkarr_relay_url: String,
    },
}
