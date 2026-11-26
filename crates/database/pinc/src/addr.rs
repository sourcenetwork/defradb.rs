//! Peer ids and addresses as text.
//!
//! A pinc node id is the lowercase hex of its Ed25519 public key. An
//! address is pinc's one-line form, `<id>` or `<id>@<ip:port>,<ip:port>`,
//! optionally behind `pinc://`.

use std::net::{IpAddr, SocketAddr};

use pinc::{NodeAddr, NodeId};

use p2p::error::{Error, Result};
use p2p::transport::{PeerAddr, PeerId};

const SCHEME: &str = "pinc://";

pub fn node_to_peer_id(node: &NodeId) -> PeerId {
    PeerId::new(pinc::node_to_string(node))
}

pub fn node_id(peer_id: &PeerId) -> Result<NodeId> {
    pinc::node_from_str(&peer_id.as_str().trim().to_ascii_lowercase())
        .map_err(|error| Error::InvalidPeerId(format!("invalid pinc peer ID: {error}")))
}

/// `peer_id` in its canonical spelling, or as it is when it is not a pinc id.
pub fn canonical_peer_id(peer_id: &PeerId) -> PeerId {
    node_id(peer_id).map_or_else(|_| peer_id.clone(), |node| node_to_peer_id(&node))
}

pub fn parse_canonical_peer_id(value: &str) -> Result<PeerId> {
    node_id(&PeerId::new(value.to_owned())).map(|node| node_to_peer_id(&node))
}

/// The peer an address names and the direct addresses it carries.
pub fn parse_public_peer_addr(addr: &str) -> Result<(PeerId, Vec<PeerAddr>)> {
    let parsed = parse_node_addr(addr)?;
    let direct = parsed
        .direct
        .iter()
        .map(|socket| PeerAddr::new(socket.to_string()))
        .collect();
    Ok((node_to_peer_id(&parsed.node), direct))
}

pub fn parse_node_addr(addr: &str) -> Result<NodeAddr> {
    let trimmed = addr.trim();
    let text = trimmed.strip_prefix(SCHEME).unwrap_or(trimmed);
    if text.is_empty() {
        return Err(Error::InvalidPeerId("empty pinc address".to_owned()));
    }
    NodeAddr::parse(&text.to_ascii_lowercase())
        .map_err(|error| Error::InvalidPeerId(format!("invalid pinc address {addr:?}: {error}")))
}

/// The socket addresses among `addrs`, which the transport reports as
/// `ip:port` strings or as whole addresses.
pub fn socket_addrs(addrs: &[PeerAddr]) -> Vec<SocketAddr> {
    addrs
        .iter()
        .flat_map(|addr| match addr.as_str().parse::<SocketAddr>() {
            Ok(socket) => vec![socket],
            Err(_) => parse_node_addr(addr.as_str())
                .map(|parsed| parsed.direct)
                .unwrap_or_default(),
        })
        .collect()
}

/// The addresses a node lists for peers: one `<id>@<ip:port>` per address.
pub fn format_public_listen_addrs(peer_id: &PeerId, raw_addrs: &[PeerAddr]) -> Vec<String> {
    let Ok(node) = node_id(peer_id) else {
        return raw_addrs.iter().map(ToString::to_string).collect();
    };
    let mut out: Vec<String> = Vec::new();
    for socket in socket_addrs(raw_addrs) {
        let text = NodeAddr::new(node).with_direct(socket).ticket();
        if !out.contains(&text) {
            out.push(text);
        }
    }
    if out.is_empty() {
        out.push(pinc::node_to_string(&node));
    }
    out
}

/// The one address to share: the first that is not loopback. A node bound
/// to a wildcard lists no loopback address (`advertised`), so this is
/// loopback only for a node bound to loopback on purpose, and `None` for a
/// node with no address another host can reach.
pub fn best_shareable_public_addr(peer_id: &PeerId, raw_addrs: &[PeerAddr]) -> Option<String> {
    let node = node_id(peer_id).ok()?;
    let sockets = socket_addrs(raw_addrs);
    let best = sockets
        .iter()
        .find(|socket| !socket.ip().is_loopback() && !socket.ip().is_unspecified())
        .or_else(|| sockets.first())?;
    Some(NodeAddr::new(node).with_direct(*best).ticket())
}

/// The addresses a node bound at `local` tells peers: `public` first, then
/// `local` itself, or, for a wildcard bind, the address of every interface
/// of its family (both families for `[::]`, which carries IPv4 as mapped
/// addresses) on the bound port. A wildcard bind lists no loopback or
/// IPv6 link-local address: neither is one another host can dial.
pub(super) fn advertised(local: SocketAddr, public: &[SocketAddr]) -> Vec<SocketAddr> {
    let interfaces = if local.ip().is_unspecified() {
        match if_addrs::get_if_addrs() {
            Ok(interfaces) => interfaces.into_iter().map(|iface| iface.ip()).collect(),
            Err(error) => {
                tracing::warn!(%error, %local, "could not list the host's interfaces: the node lists only its public addresses");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    advertised_from(local, public, &interfaces)
}

fn advertised_from(
    local: SocketAddr,
    public: &[SocketAddr],
    interfaces: &[IpAddr],
) -> Vec<SocketAddr> {
    let mut out: Vec<SocketAddr> = Vec::new();
    let mut push = |addr: SocketAddr| {
        if !out.contains(&addr) {
            out.push(addr);
        }
    };
    public.iter().copied().for_each(&mut push);
    if !local.ip().is_unspecified() {
        push(local);
        return out;
    }
    for ip in interfaces {
        let reachable = match ip {
            IpAddr::V4(v4) => !v4.is_loopback() && !v4.is_unspecified(),
            IpAddr::V6(v6) => {
                local.is_ipv6()
                    && !v6.is_loopback()
                    && !v6.is_unspecified()
                    && !v6.is_unicast_link_local()
            }
        };
        if reachable {
            push(SocketAddr::new(*ip, local.port()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "abababababababababababababababababababababababababababababababab";

    #[test]
    fn an_address_round_trips_with_or_without_the_scheme() {
        let text = format!("{ID}@127.0.0.1:4433");
        for spelling in [text.clone(), format!("pinc://{text}"), text.to_uppercase()] {
            let (peer, direct) = parse_public_peer_addr(&spelling).unwrap();
            assert_eq!(peer.as_str(), ID);
            assert_eq!(direct, vec![PeerAddr::new("127.0.0.1:4433".into())]);
        }
    }

    #[test]
    fn a_bare_id_has_no_direct_address() {
        let (peer, direct) = parse_public_peer_addr(ID).unwrap();
        assert_eq!(peer.as_str(), ID);
        assert!(direct.is_empty());
    }

    #[test]
    fn listen_addresses_name_the_node() {
        let peer = PeerId::new(ID.to_owned());
        let listed = format_public_listen_addrs(&peer, &[PeerAddr::new("127.0.0.1:9".into())]);
        assert_eq!(listed, vec![format!("{ID}@127.0.0.1:9")]);
        assert_eq!(
            best_shareable_public_addr(
                &peer,
                &[
                    PeerAddr::new("127.0.0.1:9".into()),
                    PeerAddr::new("10.0.0.7:9".into())
                ]
            ),
            Some(format!("{ID}@10.0.0.7:9"))
        );
    }

    #[test]
    fn garbage_is_refused() {
        assert!(parse_public_peer_addr("").is_err());
        assert!(parse_public_peer_addr("not-an-id").is_err());
        assert!(parse_canonical_peer_id("12").is_err());
    }

    #[test]
    fn a_wildcard_bind_lists_every_interface_but_loopback_and_public_first() {
        let interfaces: Vec<IpAddr> = ["127.0.0.1", "10.0.0.7", "::1", "fe80::1", "2001:db8::7"]
            .iter()
            .map(|ip| ip.parse().unwrap())
            .collect();
        let public: SocketAddr = "203.0.113.9:4000".parse().unwrap();
        let v4 = advertised_from("0.0.0.0:9".parse().unwrap(), &[public], &interfaces);
        assert_eq!(v4, vec![public, "10.0.0.7:9".parse().unwrap()]);
        let v6 = advertised_from("[::]:9".parse().unwrap(), &[], &interfaces);
        assert_eq!(
            v6,
            vec![
                "10.0.0.7:9".parse().unwrap(),
                "[2001:db8::7]:9".parse().unwrap()
            ]
        );
        let explicit = advertised_from("127.0.0.1:9".parse().unwrap(), &[], &interfaces);
        assert_eq!(explicit, vec!["127.0.0.1:9".parse().unwrap()]);
        // Loopback only: nothing another host can reach.
        let lone = advertised_from("0.0.0.0:9".parse().unwrap(), &[], &interfaces[..1]);
        assert!(lone.is_empty());
        let peer = PeerId::new(ID.to_owned());
        assert_eq!(best_shareable_public_addr(&peer, &[]), None);
    }
}
