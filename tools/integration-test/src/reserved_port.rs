//! A Go node's P2P port, held while the node restarts.
//!
//! A restarted node listens on the P2P address it had, so its peers' stored
//! addresses still reach it. A Go node runs in its own process: once the old
//! process exits, the port is free until the new one binds it, and any
//! ephemeral bind on the machine can take it in between. A socket bound to
//! the address with `SO_REUSEPORT` alone, never listening, closes that gap.
//! The kernel skips a port bound this way for every ephemeral bind and
//! connect, while go-libp2p, which binds its TCP listener with
//! `SO_REUSEPORT` as the same user, still binds and listens on it.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use socket2::{Domain, Protocol, Socket, Type};

/// A port held bound until dropped.
pub struct ReservedPort {
    _socket: Socket,
}

impl ReservedPort {
    /// Bind `address` alongside its current owner, which must have bound it
    /// with `SO_REUSEPORT` as this user.
    pub fn hold(address: SocketAddr) -> io::Result<Self> {
        let socket = Socket::new(
            Domain::for_address(address),
            Type::STREAM,
            Some(Protocol::TCP),
        )?;
        // No SO_REUSEADDR: with it, the kernel would let another bind share
        // the port as soon as no one listens on it.
        socket.set_reuse_port(true)?;
        socket.bind(&address.into())?;
        Ok(Self { _socket: socket })
    }
}

/// Hold the port of every TCP address in `listen`, the P2P addresses of a
/// Go node that is still running.
pub fn hold_all(listen: &[String]) -> Vec<Arc<ReservedPort>> {
    listen
        .iter()
        .filter_map(|address| tcp_address(address.as_str()))
        .map(|address| {
            ReservedPort::hold(address)
                .map(Arc::new)
                .unwrap_or_else(|error| {
                    panic!(
                        "hold the Go node's P2P port {address} for its restart: {error}; \
                     its libp2p must listen with SO_REUSEPORT (LIBP2P_TCP_REUSEPORT unset)"
                    )
                })
        })
        .collect()
}

/// The socket address a `/ip4/<ip>/tcp/<port>` or `/ip6/<ip>/tcp/<port>`
/// multiaddr names; `None` for any other address.
pub fn tcp_address(multiaddr: &str) -> Option<SocketAddr> {
    let mut parts = multiaddr.split('/').skip(1);
    let ip: IpAddr = match (parts.next()?, parts.next()?) {
        ("ip4" | "ip6", ip) => ip.parse().ok()?,
        _ => return None,
    };
    match (parts.next()?, parts.next()?) {
        ("tcp", port) => Some(SocketAddr::new(ip, port.parse().ok()?)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::net::{SocketAddr, TcpListener, TcpStream};

    use socket2::{Domain, Protocol, Socket, Type};

    use super::{tcp_address, ReservedPort};

    /// A listener bound as go-libp2p binds its TCP listener.
    fn libp2p_listener(address: SocketAddr) -> std::io::Result<Socket> {
        let socket = Socket::new(
            Domain::for_address(address),
            Type::STREAM,
            Some(Protocol::TCP),
        )?;
        socket.set_reuse_address(true)?;
        socket.set_reuse_port(true)?;
        socket.bind(&address.into())?;
        socket.listen(16)?;
        Ok(socket)
    }

    #[test]
    fn holds_the_port_against_other_binds_and_lets_the_node_take_it_back() {
        let first = libp2p_listener("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = first.local_addr().unwrap().as_socket().unwrap();
        let reserved = ReservedPort::hold(address).unwrap();
        drop(first);

        let taken = TcpListener::bind(address).map(|_| ());
        assert_eq!(
            taken.map_err(|error| error.kind()),
            Err(ErrorKind::AddrInUse)
        );

        let restarted = libp2p_listener(address).unwrap();
        let _client = TcpStream::connect(address).unwrap();
        restarted.accept().unwrap();
        drop(reserved);
    }

    #[test]
    fn reads_tcp_multiaddrs_only() {
        assert_eq!(
            tcp_address("/ip4/127.0.0.1/tcp/42697"),
            Some("127.0.0.1:42697".parse().unwrap())
        );
        assert_eq!(
            tcp_address("/ip6/::1/tcp/9171"),
            Some("[::1]:9171".parse().unwrap())
        );
        assert_eq!(tcp_address("/ip4/127.0.0.1/udp/9171/quic-v1"), None);
        assert_eq!(tcp_address("/dns4/localhost/tcp/9171"), None);
        assert_eq!(tcp_address("/ip4/127.0.0.1/tcp/port"), None);
    }
}
