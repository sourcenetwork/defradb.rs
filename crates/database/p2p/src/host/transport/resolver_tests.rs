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
    let read = super::ResolverSource::ResolvConf("nameserver not-an-ip\n".into()).read();
    let error = read.as_ref().err().map(ToString::to_string);
    assert!(
        error
            .as_deref()
            .is_some_and(|error| error.contains("Error parsing resolv.conf")),
        "the text must reach hickory's parse error, got {error:?}"
    );
    let (config, _) = resolver_config(read);
    assert_eq!(servers(&config), localhost_servers());
}

#[cfg(unix)]
#[test]
fn an_empty_resolv_conf_reads_as_no_nameservers() {
    let read = super::ResolverSource::ResolvConf(String::new()).read();
    let error = read.as_ref().err().map(ToString::to_string);
    assert!(
        error
            .as_deref()
            .is_some_and(|error| error.contains("no nameservers found in config")),
        "an empty file must be hickory's no-nameservers error, got {error:?}"
    );
    let (config, _) = resolver_config(read);
    assert_eq!(servers(&config), localhost_servers());
}
