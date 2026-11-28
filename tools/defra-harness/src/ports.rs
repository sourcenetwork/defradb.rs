use std::net::{TcpListener, UdpSocket};

use crate::error::{Result, WrapErr};

/// Ports assigned to a single node, with guard listeners held until release.
///
/// The guards keep the ports reserved until `release()` is called. This
/// prevents other parallel tests from grabbing the same port between
/// allocation and node startup.
pub struct NodePorts {
    pub http: u16,
    pub p2p: u16,
    guards: Option<Vec<TcpListener>>,
}

impl NodePorts {
    /// Release the port guards. Call immediately before spawning the node
    /// process so the ports are free for it to bind.
    pub fn release(&mut self) {
        self.guards = None;
    }
}

/// Allocate port pairs (http, p2p) for `n` nodes, holding guard listeners.
pub fn allocate_node_ports(n: usize) -> Result<Vec<NodePorts>> {
    let count = n * 2;
    let listeners: Vec<TcpListener> = (0..count)
        .map(|i| {
            TcpListener::bind("127.0.0.1:0")
                .wrap_err_with(|| format!("failed to bind ephemeral port {}/{}", i + 1, count))
        })
        .collect::<Result<_>>()?;

    let mut result = Vec::with_capacity(n);
    let mut iter = listeners.into_iter();
    for _ in 0..n {
        let l1 = iter.next().unwrap();
        let l2 = iter.next().unwrap();
        let http = l1.local_addr()?.port();
        let p2p = l2.local_addr()?.port();
        result.push(NodePorts {
            http,
            p2p,
            guards: Some(vec![l1, l2]),
        });
    }

    Ok(result)
}

/// Ports for a single node running multiple libp2p transports
/// (TCP, QUIC over UDP, WebSocket over TCP) plus the HTTP API.
///
/// All four ports are reserved with bind-hold-release guards until
/// `release()` is called. Call `release()` immediately before spawning
/// the node so the child process can bind them.
pub struct TransportNodePorts {
    pub http: u16,
    pub tcp: u16,
    pub quic: u16,
    pub ws: u16,
    tcp_guards: Option<Vec<TcpListener>>,
    udp_guard: Option<UdpSocket>,
}

impl TransportNodePorts {
    /// Release all port guards. Call right before spawning the node.
    pub fn release(&mut self) {
        self.tcp_guards = None;
        self.udp_guard = None;
    }

    /// Multiaddr list for libp2p: TCP + QUIC + WebSocket, comma-separated.
    pub fn p2p_addr_arg(&self) -> String {
        format!(
            "/ip4/127.0.0.1/tcp/{},/ip4/127.0.0.1/udp/{}/quic-v1,/ip4/127.0.0.1/tcp/{}/ws",
            self.tcp, self.quic, self.ws
        )
    }

    /// QUIC-only multiaddr, useful for dialing tests that target a
    /// single transport.
    pub fn quic_p2p_addr_arg(&self) -> String {
        format!("/ip4/127.0.0.1/udp/{}/quic-v1", self.quic)
    }
}

/// Allocate transport-port quads for `n` nodes.
///
/// Binds all guard listeners (3 TCP + 1 UDP per node) before reading
/// any local addresses, preventing parallel callers from getting the
/// same port.
pub fn allocate_transport_ports(n: usize) -> Result<Vec<TransportNodePorts>> {
    let mut tcp_listeners: Vec<TcpListener> = Vec::with_capacity(n * 3);
    let mut udp_sockets: Vec<UdpSocket> = Vec::with_capacity(n);

    for i in 0..n {
        for kind in ["http", "tcp", "ws"] {
            tcp_listeners.push(
                TcpListener::bind("127.0.0.1:0")
                    .wrap_err_with(|| format!("failed to bind {} guard for node {}", kind, i))?,
            );
        }
        udp_sockets.push(
            UdpSocket::bind("127.0.0.1:0")
                .wrap_err_with(|| format!("failed to bind quic guard for node {}", i))?,
        );
    }

    let mut result = Vec::with_capacity(n);
    let mut tcp_iter = tcp_listeners.into_iter();
    let mut udp_iter = udp_sockets.into_iter();
    for _ in 0..n {
        let http_guard = tcp_iter.next().unwrap();
        let tcp_guard = tcp_iter.next().unwrap();
        let ws_guard = tcp_iter.next().unwrap();
        let udp_guard = udp_iter.next().unwrap();
        let http = http_guard.local_addr()?.port();
        let tcp = tcp_guard.local_addr()?.port();
        let ws = ws_guard.local_addr()?.port();
        let quic = udp_guard.local_addr()?.port();
        result.push(TransportNodePorts {
            http,
            tcp,
            quic,
            ws,
            tcp_guards: Some(vec![http_guard, tcp_guard, ws_guard]),
            udp_guard: Some(udp_guard),
        });
    }

    Ok(result)
}

/// Guards bound to a set of ports whose numbers are already fixed.
///
/// [`allocate_node_ports`] asks the OS for *any* free port; this reserves
/// *specific* ones. A node that is restarting cannot move to fresh ports —
/// its peers hold its multiaddr and its clients hold its API URL — so the
/// gap between killing it and re-binding must be covered by holding the very
/// same numbers, or a concurrent `allocate_*_ports` can be handed them.
pub struct ReservedPorts {
    tcp: Vec<TcpListener>,
    udp: Vec<UdpSocket>,
}

impl ReservedPorts {
    /// Release the guards. Call immediately before spawning the process that
    /// takes ownership of these ports.
    pub fn release(&mut self) {
        self.tcp.clear();
        self.udp.clear();
    }
}

/// Bind guard sockets on exactly `tcp_ports` and `udp_ports`.
///
/// All-or-nothing: on failure every guard bound so far is dropped, so a caller
/// that retries never holds a partial reservation. Failure means someone still
/// owns one of the ports — possibly the previous owner mid-shutdown, which is
/// why callers retry rather than give up on the first error.
pub fn reserve_ports(tcp_ports: &[u16], udp_ports: &[u16]) -> Result<ReservedPorts> {
    let mut reserved = ReservedPorts {
        tcp: Vec::with_capacity(tcp_ports.len()),
        udp: Vec::with_capacity(udp_ports.len()),
    };
    for port in tcp_ports {
        reserved.tcp.push(
            TcpListener::bind(("127.0.0.1", *port))
                .wrap_err_with(|| format!("failed to reserve tcp port {}", port))?,
        );
    }
    for port in udp_ports {
        reserved.udp.push(
            UdpSocket::bind(("127.0.0.1", *port))
                .wrap_err_with(|| format!("failed to reserve udp port {}", port))?,
        );
    }
    Ok(reserved)
}

/// Extract the `(tcp, udp)` ports named by a comma-separated multiaddr list
/// such as `/ip4/127.0.0.1/tcp/4001,/ip4/127.0.0.1/udp/4002/quic-v1`.
///
/// Unparseable or portless components are skipped: the caller uses this to
/// decide what to guard, and guarding nothing is the pre-existing behavior.
pub fn multiaddr_ports(addr: &str) -> (Vec<u16>, Vec<u16>) {
    let mut tcp = Vec::new();
    let mut udp = Vec::new();
    for entry in addr.split(',') {
        let parts: Vec<&str> = entry.split('/').collect();
        for pair in parts.windows(2) {
            let Ok(port) = pair[1].parse::<u16>() else {
                continue;
            };
            match pair[0] {
                "tcp" => tcp.push(port),
                "udp" => udp.push(port),
                _ => {}
            }
        }
    }
    (tcp, udp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapidhash::{HashSetExt, RapidHashSet};
    use regolith::sync::Mutex;
    use std::net::{TcpListener, UdpSocket};

    static PORT_TEST_GUARD: Mutex<()> = Mutex::new(());

    fn assert_tcp_released(port: u16) {
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(listener) => drop(listener),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
            Err(error) => panic!("tcp port {port} was not released: {error}"),
        }
    }

    fn assert_udp_released(port: u16) {
        match UdpSocket::bind(("127.0.0.1", port)) {
            Ok(socket) => drop(socket),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
            Err(error) => panic!("udp port {port} was not released: {error}"),
        }
    }

    #[tokio::test]
    async fn allocate_transport_ports_returns_unique_ports() {
        let _serial = PORT_TEST_GUARD.lock().await;
        let ports = allocate_transport_ports(2).expect("allocate");
        assert_eq!(ports.len(), 2);

        let mut tcp_seen: RapidHashSet<u16> = RapidHashSet::new();
        let mut udp_seen: RapidHashSet<u16> = RapidHashSet::new();
        for p in &ports {
            assert!(tcp_seen.insert(p.http), "duplicate http port {}", p.http);
            assert!(tcp_seen.insert(p.tcp), "duplicate tcp port {}", p.tcp);
            assert!(tcp_seen.insert(p.ws), "duplicate ws port {}", p.ws);
            assert!(udp_seen.insert(p.quic), "duplicate quic port {}", p.quic);
        }
        assert_eq!(tcp_seen.len(), 6, "expected 6 unique TCP ports for n=2");
        assert_eq!(udp_seen.len(), 2, "expected 2 unique UDP ports for n=2");
    }

    #[tokio::test]
    async fn release_frees_ports_for_rebinding() {
        let _serial = PORT_TEST_GUARD.lock().await;
        let mut p = allocate_transport_ports(1)
            .expect("allocate")
            .pop()
            .unwrap();
        let (http, tcp, ws, quic) = (p.http, p.tcp, p.ws, p.quic);
        p.release();

        assert!(p.tcp_guards.is_none());
        assert!(p.udp_guard.is_none());
        assert_tcp_released(http);
        assert_tcp_released(tcp);
        assert_tcp_released(ws);
        assert_udp_released(quic);
    }

    #[tokio::test]
    async fn reserved_ports_lock_out_a_competing_binder_until_released() {
        let _serial = PORT_TEST_GUARD.lock().await;
        let (http, quic, mut reserved) = (0..16)
            .find_map(|_| {
                let mut allocated = allocate_transport_ports(1).ok()?.pop()?;
                let (http, quic) = (allocated.http, allocated.quic);
                allocated.release();
                reserve_ports(&[http], &[quic])
                    .ok()
                    .map(|reserved| (http, quic, reserved))
            })
            .expect("reserve a just-freed port pair");

        assert!(
            TcpListener::bind(("127.0.0.1", http)).is_err(),
            "a competitor must not be able to take reserved tcp port {}",
            http
        );
        assert!(
            UdpSocket::bind(("127.0.0.1", quic)).is_err(),
            "a competitor must not be able to take reserved udp port {}",
            quic
        );

        reserved.release();

        assert!(reserved.tcp.is_empty());
        assert!(reserved.udp.is_empty());
        assert_tcp_released(http);
        assert_udp_released(quic);
    }

    #[tokio::test]
    async fn failed_reservation_holds_no_ports() {
        let _serial = PORT_TEST_GUARD.lock().await;
        let taken = TcpListener::bind("127.0.0.1:0").expect("bind competitor");
        let taken_port = taken.local_addr().unwrap().port();

        let mut allocated = allocate_transport_ports(1)
            .expect("allocate")
            .pop()
            .unwrap();
        let free_port = allocated.http;
        allocated.release();

        assert!(
            reserve_ports(&[free_port, taken_port], &[]).is_err(),
            "reserving a port owned by someone else must fail"
        );

        assert_tcp_released(free_port);
    }

    #[test]
    fn multiaddr_ports_reads_tcp_and_udp_components() {
        let (tcp, udp) = multiaddr_ports(
            "/ip4/127.0.0.1/tcp/4001,/ip4/127.0.0.1/udp/4002/quic-v1,/ip4/127.0.0.1/tcp/4003/ws",
        );
        assert_eq!(tcp, vec![4001, 4003]);
        assert_eq!(udp, vec![4002]);

        let (tcp, udp) = multiaddr_ports("/ip4/127.0.0.1/tcp/0");
        assert_eq!(tcp, vec![0]);
        assert!(udp.is_empty());

        let (tcp, udp) = multiaddr_ports("/dns4/example.com/tcp/notaport");
        assert!(tcp.is_empty() && udp.is_empty());
    }

    #[test]
    fn p2p_addr_arg_lists_all_three_transports() {
        let p = TransportNodePorts {
            http: 1,
            tcp: 2,
            quic: 3,
            ws: 4,
            tcp_guards: None,
            udp_guard: None,
        };
        assert_eq!(
            p.p2p_addr_arg(),
            "/ip4/127.0.0.1/tcp/2,/ip4/127.0.0.1/udp/3/quic-v1,/ip4/127.0.0.1/tcp/4/ws"
        );
        assert_eq!(p.quic_p2p_addr_arg(), "/ip4/127.0.0.1/udp/3/quic-v1");
    }
}
