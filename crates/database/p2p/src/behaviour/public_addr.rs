//! Which addresses belong in the WAN routing table: Go's dual DHT keeps only
//! public ones there (`manet.IsPublicAddr`, applied by go-libp2p-kad-dht v0.38
//! `dual/dual.go:103-107`).

use std::net::{Ipv4Addr, Ipv6Addr};

use libp2p::{multiaddr::Protocol, Multiaddr};

/// Whether `address` can be reached from the public internet: not a private,
/// loopback, link-local, shared or reserved IP, not `localhost`, and not
/// through a relay.
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
        ) => name != "localhost" && !name.ends_with(".localhost"),
        _ => false,
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    let shared = a == 100 && (64..128).contains(&b);
    let reserved = a >= 240;
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_documentation()
        || a == 0
        || shared
        || reserved)
}

/// Go's `public6` range, `2000::/3` (go-libp2p-kad-dht v0.38 `dht_filters.go:33`).
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
            "/ip6/2606:4700::1111/udp/4001/quic-v1",
            "/dns4/node.example.com/tcp/4001",
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
            "/ip6/::1/tcp/4001",
            "/ip6/fd00::1/tcp/4001",
            "/ip6/fe80::1/tcp/4001",
            "/dns4/localhost/tcp/4001",
            "/ip4/8.8.8.8/tcp/4001/p2p/12D3KooWD3eckifWpRn9wQpMG9R9hX3sD158z7EqHWmweQAJU5SA/p2p-circuit",
        ] {
            assert!(!public(address), "{address} is not public");
        }
    }
}
