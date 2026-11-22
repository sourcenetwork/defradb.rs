//! The resolver configuration the DNS layers use: the host's, read the way
//! Go's `dnsReadConfig` reads it (a line or word hickory rejects is skipped,
//! the rest is kept), with Go's local nameservers in place of the host's
//! when it lists none or cannot be read
//! (`proofs/lean/DnsResolverFallback.lean`). It is read once, when the
//! transport is built.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use hickory_resolver::config::NameServerConfigGroup;
#[cfg(unix)]
use hickory_resolver::system_conf::parse_resolv_conf;
use libp2p::dns::{ResolveError, ResolverConfig, ResolverOpts};

/// The nameservers Go's resolver falls back to (`defaultNS` in Go's `net`
/// package), on the DNS port, over UDP and TCP.
pub(crate) const FALLBACK_NAMESERVERS: [IpAddr; 2] = [
    IpAddr::V4(Ipv4Addr::LOCALHOST),
    IpAddr::V6(Ipv6Addr::LOCALHOST),
];

/// The DNS port the fallback nameservers answer on.
pub(crate) const DNS_PORT: u16 = 53;

/// The file [`ResolverSource::System`] reads on unix.
#[cfg(unix)]
const RESOLV_CONF: &str = "/etc/resolv.conf";

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
    /// The host's settings. On unix only a file that cannot be read is an
    /// error: any text gives a configuration with a nameserver.
    pub(crate) fn read(&self) -> Result<(ResolverConfig, ResolverOpts), ResolveError> {
        match self {
            #[cfg(unix)]
            Self::System => read_file(std::path::Path::new(RESOLV_CONF)),
            #[cfg(not(unix))]
            Self::System => hickory_resolver::system_conf::read_system_conf(),
            #[cfg(unix)]
            Self::ResolvConf(text) => Ok(parse_lenient(text)),
        }
    }
}

/// The file at `path`, read as Go's `dnsReadConfig` reads it: as bytes, so
/// only a file that cannot be opened or read is an error. A byte that is not
/// UTF-8 (a Latin-1 comment) becomes U+FFFD, and a line it lands in outside a
/// comment is skipped like any other line hickory rejects.
#[cfg(unix)]
fn read_file(path: &std::path::Path) -> Result<(ResolverConfig, ResolverOpts), ResolveError> {
    Ok(parse_lenient(&String::from_utf8_lossy(&std::fs::read(
        path,
    )?)))
}

/// `text` as Go reads it. hickory rejects a whole file over one bad line, so
/// each line (or each word of a line) hickory rejects is skipped first. A
/// file left with no nameserver gets [`FALLBACK_NAMESERVERS`] and keeps its
/// search domains and options.
#[cfg(unix)]
fn parse_lenient(text: &str) -> (ResolverConfig, ResolverOpts) {
    let kept: String = text
        .lines()
        .filter_map(usable_line)
        .map(|line| line + "\n")
        .collect();
    match parse_resolv_conf(&kept) {
        Ok(read) => read,
        Err(cause) => {
            warn_fallback(&cause);
            let fallback: String = FALLBACK_NAMESERVERS
                .iter()
                .map(|ip| format!("nameserver {ip}\n"))
                .collect();
            parse_resolv_conf(format!("{kept}{fallback}"))
                .unwrap_or_else(|_| (fallback_config(), ResolverOpts::default()))
        }
    }
}

/// `line` as hickory accepts it: whole, or (for `search`, `domain` and
/// `options`) its directive with each argument hickory accepts on its own;
/// `None` when nothing of it is accepted. Words are recovered only from
/// before a `#` or `;` comment, and a rejected `nameserver` line is skipped
/// whole, as Go reads only its first argument.
#[cfg(unix)]
fn usable_line(line: &str) -> Option<String> {
    if accepted(line) {
        return Some(line.to_owned());
    }
    let body = line.split(['#', ';']).next().unwrap_or_default();
    let mut words = body.split_whitespace();
    let directive = words.next()?;
    let args: Vec<&str> = match directive {
        "nameserver" => Vec::new(),
        _ => words
            .filter(|arg| accepted(&format!("{directive} {arg}")))
            .collect(),
    };
    let rebuilt = format!("{directive} {}", args.join(" "));
    let kept = (!args.is_empty() && accepted(&rebuilt)).then_some(rebuilt);
    tracing::warn!(
        line,
        ?kept,
        "skipping what hickory cannot parse in a resolv.conf line"
    );
    kept
}

/// Whether hickory parses `line` next to a valid nameserver.
#[cfg(unix)]
fn accepted(line: &str) -> bool {
    parse_resolv_conf(format!("nameserver {}\n{line}\n", Ipv4Addr::LOCALHOST)).is_ok()
}

fn warn_fallback(cause: &dyn std::fmt::Display) {
    tracing::warn!(
        %cause,
        "no usable DNS nameservers in the host resolver configuration, using 127.0.0.1:53 and [::1]:53"
    );
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
    match read {
        Ok((config, opts)) if !config.name_servers().is_empty() => return (config, opts),
        Ok(_) => warn_fallback(&"no nameservers found in config"),
        Err(error) => warn_fallback(&error),
    }
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
