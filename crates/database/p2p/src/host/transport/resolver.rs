//! The resolver configuration the DNS layers use: the host's, or Go's local
//! nameservers when the host's lists none or cannot be read
//! (`proofs/lean/DnsResolverFallback.lean`).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use hickory_resolver::config::NameServerConfigGroup;
use libp2p::dns::{ResolveError, ResolverConfig, ResolverOpts};

/// The nameservers Go's resolver falls back to (`defaultNS` in Go's `net`
/// package), on the DNS port, over UDP and TCP.
pub(crate) const FALLBACK_NAMESERVERS: [IpAddr; 2] = [
    IpAddr::V4(Ipv4Addr::LOCALHOST),
    IpAddr::V6(Ipv6Addr::LOCALHOST),
];

/// The DNS port the fallback nameservers answer on.
pub(crate) const DNS_PORT: u16 = 53;

/// Where the DNS layers read the host's resolver configuration from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ResolverSource {
    /// The operating system's configuration (`/etc/resolv.conf` on unix).
    #[default]
    System,
    /// This `resolv.conf` text, in place of the system's file.
    #[cfg(unix)]
    ResolvConf(String),
}

impl ResolverSource {
    pub(crate) fn read(&self) -> Result<(ResolverConfig, ResolverOpts), ResolveError> {
        match self {
            Self::System => hickory_resolver::system_conf::read_system_conf(),
            #[cfg(unix)]
            Self::ResolvConf(text) => hickory_resolver::system_conf::parse_resolv_conf(text),
        }
    }
}

/// The configuration the DNS layers use for a read of the host's settings:
/// the read itself when it names a nameserver, otherwise
/// [`FALLBACK_NAMESERVERS`] with default options, so a host without DNS
/// settings fails only `/dns*` dials, never the node's start. Never
/// `ResolverConfig::default()`, which is a public DNS service.
pub(crate) fn resolver_config<E>(
    read: Result<(ResolverConfig, ResolverOpts), E>,
) -> (ResolverConfig, ResolverOpts)
where
    E: std::fmt::Display,
{
    let cause = match read {
        Ok((config, opts)) if !config.name_servers().is_empty() => return (config, opts),
        Ok(_) => "no nameservers found in config".to_owned(),
        Err(error) => error.to_string(),
    };
    tracing::warn!(
        %cause,
        "no usable DNS nameservers in the host resolver configuration, using 127.0.0.1:53 and [::1]:53"
    );
    (fallback_config(), ResolverOpts::default())
}

/// [`FALLBACK_NAMESERVERS`] as a resolver configuration.
pub(crate) fn fallback_config() -> ResolverConfig {
    ResolverConfig::from_parts(
        None,
        Vec::new(),
        NameServerConfigGroup::from_ips_clear(&FALLBACK_NAMESERVERS, DNS_PORT, false),
    )
}

#[cfg(test)]
#[path = "resolver_tests.rs"]
mod tests;
