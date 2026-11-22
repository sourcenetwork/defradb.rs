use std::net::SocketAddr;

use hickory_resolver::config::{NameServerConfig, NameServerConfigGroup};
use hickory_resolver::proto::xfer::Protocol;
use libp2p::dns::{ResolverConfig, ResolverOpts};

use super::{fallback_config, resolver_config, DNS_PORT, FALLBACK_NAMESERVERS};

/// Each nameserver as its address and protocol, the part the fallback fixes.
fn servers(config: &ResolverConfig) -> Vec<(SocketAddr, Protocol)> {
    config
        .name_servers()
        .iter()
        .map(|server| (server.socket_addr, server.protocol))
        .collect()
}

fn localhost_servers() -> Vec<(SocketAddr, Protocol)> {
    let mut expected = Vec::new();
    for ip in FALLBACK_NAMESERVERS {
        expected.push((SocketAddr::new(ip, DNS_PORT), Protocol::Udp));
        expected.push((SocketAddr::new(ip, DNS_PORT), Protocol::Tcp));
    }
    expected
}

#[test]
fn a_failed_read_falls_back_to_the_localhost_nameservers() {
    let read: Result<(ResolverConfig, ResolverOpts), std::io::Error> =
        Err(std::io::Error::other("no nameservers found in config"));
    let (config, opts) = resolver_config(read);
    assert_eq!(servers(&config), localhost_servers());
    assert_eq!(opts.ndots, ResolverOpts::default().ndots);
}

#[test]
fn a_read_without_nameservers_falls_back_to_the_localhost_nameservers() {
    let empty = ResolverConfig::from_parts(None, Vec::new(), NameServerConfigGroup::new());
    let mut opts = ResolverOpts::default();
    opts.ndots = 5;
    let (config, opts) = resolver_config(Ok::<_, std::io::Error>((empty, opts)));
    assert_eq!(servers(&config), localhost_servers());
    assert_eq!(opts.ndots, ResolverOpts::default().ndots);
}

#[test]
fn a_read_with_nameservers_passes_through_unchanged() {
    let server = NameServerConfig::new("10.0.0.2:53".parse().unwrap(), Protocol::Udp);
    let read_config =
        ResolverConfig::from_parts(None, Vec::new(), NameServerConfigGroup::from(vec![server]));
    let mut read_opts = ResolverOpts::default();
    read_opts.ndots = 5;
    let (config, opts) = resolver_config(Ok::<_, std::io::Error>((
        read_config.clone(),
        read_opts.clone(),
    )));
    assert_eq!(servers(&config), servers(&read_config));
    assert_eq!(opts.ndots, read_opts.ndots);
}

#[test]
fn the_fallback_is_never_a_public_dns_service() {
    let public = servers(&ResolverConfig::default());
    for server in servers(&fallback_config()) {
        assert!(server.0.ip().is_loopback(), "{server:?} is not local");
        assert!(!public.contains(&server));
    }
}

/// The fallback nameservers the Lean model `DnsResolverFallback.defaultNS`
/// names are the ones the code uses.
#[test]
fn the_fallback_matches_the_lean_model() {
    let model = include_str!("../../../../../../proofs/lean/DnsResolverFallback.lean");
    let line = model
        .lines()
        .find(|line| line.starts_with("def defaultNS"))
        .expect("the model defines defaultNS");
    let modeled: Vec<String> = line
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect();
    let code: Vec<String> = FALLBACK_NAMESERVERS
        .iter()
        .map(|ip| SocketAddr::new(*ip, DNS_PORT).to_string())
        .collect();
    assert_eq!(modeled, code);
}

#[cfg(unix)]
#[test]
fn an_unparseable_resolv_conf_falls_back_to_the_localhost_nameservers() {
    let text = "nameserver not-an-ip\n";
    let read = hickory_resolver::system_conf::parse_resolv_conf(text);
    let error = read.as_ref().err().map(ToString::to_string);
    assert!(
        error
            .as_deref()
            .is_some_and(|error| error.contains("Error parsing resolv.conf")),
        "hickory alone must reject the text, got {error:?}"
    );
    let (config, _) = from_text(text);
    assert_eq!(servers(&config), localhost_servers());
}

#[cfg(unix)]
#[test]
fn an_empty_resolv_conf_reads_as_no_nameservers() {
    let read = hickory_resolver::system_conf::parse_resolv_conf("");
    let error = read.as_ref().err().map(ToString::to_string);
    assert!(
        error
            .as_deref()
            .is_some_and(|error| error.contains("no nameservers found in config")),
        "an empty file must be hickory's no-nameservers error, got {error:?}"
    );
    let (config, _) = from_text("");
    assert_eq!(servers(&config), localhost_servers());
}

/// What the DNS layers use for this `resolv.conf` text.
#[cfg(unix)]
fn from_text(text: &str) -> (ResolverConfig, ResolverOpts) {
    resolver_config(super::ResolverSource::ResolvConf(text.into()).read())
}

#[cfg(unix)]
fn udp_tcp(ip: &str) -> Vec<(SocketAddr, Protocol)> {
    let addr = SocketAddr::new(ip.parse().unwrap(), DNS_PORT);
    vec![(addr, Protocol::Udp), (addr, Protocol::Tcp)]
}

#[cfg(unix)]
fn search(config: &ResolverConfig) -> Vec<String> {
    config.search().iter().map(ToString::to_string).collect()
}

/// Go's `dnsReadConfig` ignores an option it does not know; the nameserver
/// next to it is still used.
#[cfg(unix)]
#[test]
fn an_unknown_option_keeps_the_hosts_nameserver() {
    let (config, opts) = from_text("nameserver 10.0.0.2\noptions ndots:3 bogus\n");
    assert_eq!(servers(&config), udp_tcp("10.0.0.2"));
    assert_eq!(opts.ndots, 3);
}

/// Go skips a `nameserver` line that is not an IP and keeps the others.
#[cfg(unix)]
#[test]
fn a_bad_nameserver_line_keeps_the_good_ones() {
    let (config, _) = from_text("nameserver not-an-ip\nnameserver 10.0.0.2\n");
    assert_eq!(servers(&config), udp_tcp("10.0.0.2"));
}

/// Go skips a directive it does not know.
#[cfg(unix)]
#[test]
fn an_unknown_directive_keeps_the_hosts_nameserver() {
    let (config, _) = from_text("nameserver 10.0.0.2\nfoo bar\n");
    assert_eq!(servers(&config), udp_tcp("10.0.0.2"));
}

/// A search domain hickory rejects is dropped; the rest of the line stays.
#[cfg(unix)]
#[test]
fn a_bad_search_domain_keeps_the_others() {
    let long = "a".repeat(64);
    let (config, _) = from_text(&format!(
        "nameserver 10.0.0.2\nsearch {long}.example corp.example\n"
    ));
    assert_eq!(servers(&config), udp_tcp("10.0.0.2"));
    assert_eq!(search(&config), vec!["corp.example".to_owned()]);
}

/// Go sets only `servers = defaultNS` when the file names no nameserver; its
/// search domains and options stay.
#[cfg(unix)]
#[test]
fn a_file_without_nameservers_keeps_its_search_and_options() {
    let (config, opts) = from_text("search corp.example\noptions ndots:3 timeout:9\n");
    assert_eq!(servers(&config), localhost_servers());
    assert_eq!(search(&config), vec!["corp.example".to_owned()]);
    assert_eq!(opts.ndots, 3);
    assert_eq!(opts.timeout, std::time::Duration::from_secs(9));
}

/// The settings real hosts ship parse as they are.
#[cfg(unix)]
#[test]
fn a_systemd_resolved_stub_passes_through() {
    let (config, opts) = from_text("nameserver 127.0.0.53\noptions edns0 trust-ad\nsearch .\n");
    assert_eq!(servers(&config), udp_tcp("127.0.0.53"));
    assert!(opts.edns0);
}
