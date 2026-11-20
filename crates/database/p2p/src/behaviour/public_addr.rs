//! Which addresses belong in the WAN routing table. Go's dual DHT admits a
//! peer to its WAN table through `PublicRoutingTableFilter`, which asks
//! `isPublicAddr` of the peer's known addresses (go-libp2p-kad-dht v0.40.0
//! `dht_filters.go:38-48`, `87-105`), and keeps only the addresses
//! `manet.IsPublicAddr` accepts (`dual/dual.go:103-107`). Go's peerstore holds
//! the IP a DNS name resolved to, so a peer reached through a public name is
//! admitted. Here the swarm reports the dialed `/dns*` address, not the
//! resolved IP, so a DNS name is judged the way `manet.IsPublicAddr` judges
//! it (go-multiaddr v0.16.1 `net/private.go:63-91`, `144-162`).

use std::net::{Ipv4Addr, Ipv6Addr};

use libp2p::{multiaddr::Protocol, Multiaddr};

/// manet's `Private4` and `Unroutable4` (go-multiaddr v0.16 `net/private.go`),
/// as `(network, prefix length)`.
const NOT_PUBLIC_V4: [([u8; 4], u32); 16] = [
    ([127, 0, 0, 0], 8),
    ([10, 0, 0, 0], 8),
    ([100, 64, 0, 0], 10),
    ([172, 16, 0, 0], 12),
    ([192, 168, 0, 0], 16),
    ([169, 254, 0, 0], 16),
    ([0, 0, 0, 0], 8),
    ([192, 0, 0, 0], 26),
    ([192, 0, 2, 0], 24),
    ([192, 88, 99, 0], 24),
    ([198, 18, 0, 0], 15),
    ([198, 51, 100, 0], 24),
    ([203, 0, 113, 0], 24),
    ([224, 0, 0, 0], 4),
    ([240, 0, 0, 0], 4),
    ([255, 255, 255, 255], 32),
];

/// Whether `address` names a public IP, as Go's `isPublicAddr` decides: an
/// IPv4 address outside every private and unroutable range, or an IPv6
/// address in `2000::/3`, or a DNS name outside the special-use domains
/// manet refuses. A relayed address is not public (Go's `isRelayAddr`).
pub(super) fn is_public(address: &Multiaddr) -> bool {
    if address.iter().any(|p| matches!(p, Protocol::P2pCircuit)) {
        return false;
    }
    match address.iter().next() {
        Some(Protocol::Ip4(ip)) => is_public_v4(ip),
        Some(Protocol::Ip6(ip)) => is_public_v6(ip),
        Some(
            Protocol::Dns(name)
            | Protocol::Dns4(name)
            | Protocol::Dns6(name)
            | Protocol::Dnsaddr(name),
        ) => is_public_name(&name),
        _ => false,
    }
}

/// manet's `localHostDomain`, `unResolvableDomains` and `privateUseDomains`:
/// names that never resolve to a public address.
const NOT_PUBLIC_DOMAINS: [&str; 7] = [
    "localhost",
    "in-addr.arpa",
    "ip6.arpa",
    "invalid",
    "home.arpa",
    "local",
    "test",
];

/// Whether a DNS name is public, as manet's `IsPublicAddr` decides: any name
/// that is not one of the special-use domains or a subdomain of one.
fn is_public_name(name: &str) -> bool {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    !NOT_PUBLIC_DOMAINS
        .iter()
        .any(|domain| name == *domain || name.ends_with(&format!(".{domain}")))
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let ip = u32::from(ip);
    !NOT_PUBLIC_V4.iter().any(|&(network, bits)| {
        let mask = u32::MAX << (32 - bits);
        ip & mask == u32::from_be_bytes(network)
    })
}

/// Go's `public6` range, `2000::/3` (go-libp2p-kad-dht v0.40.0 `dht_filters.go`).
fn is_public_v6(ip: Ipv6Addr) -> bool {
    ip.segments()[0] & 0xe000 == 0x2000
}

#[cfg(test)]
mod tests {
    use super::is_public;

    fn public(address: &str) -> bool {
        is_public(&address.parse().unwrap())
    }

    #[test]
    fn only_internet_reachable_addresses_are_public() {
        for address in [
            "/ip4/8.8.8.8/tcp/4001",
            "/ip4/1.1.1.1/udp/4001/quic-v1",
            "/ip6/2606:4700::1111/udp/4001/quic-v1",
            "/dns4/node.example.com/tcp/4001",
            "/dns/Bootstrap.Example.ORG./tcp/4001",
            "/dnsaddr/bootstrap.libp2p.io",
            "/dns6/localhost.example.com/tcp/4001",
        ] {
            assert!(public(address), "{address} is public");
        }
        for address in [
            "/ip4/127.0.0.1/tcp/4001",
            "/ip4/10.1.2.3/tcp/4001",
            "/ip4/192.168.1.2/tcp/4001",
            "/ip4/172.16.0.1/tcp/4001",
            "/ip4/169.254.1.1/tcp/4001",
            "/ip4/100.64.0.1/tcp/4001",
            "/ip4/0.0.0.0/tcp/4001",
            "/ip4/240.0.0.1/tcp/4001",
            "/ip4/255.255.255.255/tcp/4001",
            "/ip6/::1/tcp/4001",
            "/ip6/fd00::1/tcp/4001",
            "/ip6/fe80::1/tcp/4001",
            "/dns4/localhost/tcp/4001",
            "/dns4/foo.localhost/tcp/4001",
            "/dns4/printer.local/tcp/4001",
            "/dns6/node.home.arpa/tcp/4001",
            "/dns/peer.test/tcp/4001",
            "/dns4/x.invalid/tcp/4001",
            "/dnsaddr/1.2.3.4.in-addr.arpa",
            "/ip4/8.8.8.8/tcp/4001/p2p/12D3KooWD3eckifWpRn9wQpMG9R9hX3sD158z7EqHWmweQAJU5SA/p2p-circuit",
        ] {
            assert!(!public(address), "{address} is not public");
        }
    }

    /// manet's `Unroutable4` ranges Go's WAN table refuses.
    #[test]
    fn go_unroutable_v4_ranges_are_not_public() {
        let leaks: Vec<_> = [
            "192.0.0.1",
            "192.0.2.1",
            "192.88.99.1",
            "198.18.0.1",
            "198.19.255.254",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "239.255.255.255",
        ]
        .into_iter()
        .filter(|ip| public(&format!("/ip4/{ip}/tcp/4001")))
        .collect();
        assert!(leaks.is_empty(), "public here, unroutable in Go: {leaks:?}");
        assert!(
            public("/ip4/192.0.0.64/tcp/4001"),
            "192.0.0.0/26 ends at .63"
        );
        assert!(
            public("/ip4/198.20.0.1/tcp/4001"),
            "198.18.0.0/15 ends at 198.19"
        );
    }
}
