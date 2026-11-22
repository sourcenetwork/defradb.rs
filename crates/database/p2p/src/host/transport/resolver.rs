//! The resolver configuration the DNS layers use.

use libp2p::dns::{ResolveError, ResolverConfig, ResolverOpts};

use crate::error::{Error, Result};

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
    pub(crate) fn read(&self) -> std::result::Result<(ResolverConfig, ResolverOpts), ResolveError> {
        match self {
            Self::System => hickory_resolver::system_conf::read_system_conf(),
            #[cfg(unix)]
            Self::ResolvConf(text) => hickory_resolver::system_conf::parse_resolv_conf(text),
        }
    }
}

/// The configuration a read of the host's resolver settings yields.
pub(crate) fn resolver_config<E>(
    read: std::result::Result<(ResolverConfig, ResolverOpts), E>,
) -> Result<(ResolverConfig, ResolverOpts)>
where
    E: std::fmt::Display,
{
    read.map_err(|e| Error::Transport(e.to_string()))
}
