//! libp2p's TCP transport on nuclei sockets.
//!
//! The same behaviour as `libp2p-tcp`: `TCP_NODELAY` on, a backlog of 1024,
//! IPv6 sockets only for IPv6, and port reuse when the swarm asks for it:
//! a dial then binds the port of a listener of the same family and scope,
//! so the remote sees the address it can dial back. A listener on an
//! unspecified address reports each interface address as it comes and
//! goes ([`Interfaces`]).

use std::collections::VecDeque;
use std::future::{ready, Future, Ready};
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use futures::stream::{SelectAll, Stream, StreamExt};
use libp2p::core::multiaddr::{Multiaddr, Protocol};
use libp2p::core::transport::{DialOpts, ListenerId, PortUse, TransportError, TransportEvent};
use nuclei::net::{TcpListener, TcpSocket, TcpStream};
use regolith::sync::HopscotchMap;

use super::interfaces::{Change, Interfaces};

const BACKLOG: u32 = 1024;

/// How long a listener waits after an accept fails before accepting again.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

type Accepting = Pin<Box<dyn Future<Output = io::Result<(TcpStream, SocketAddr)>> + Send>>;

/// The addresses this node listens on, for dials that reuse a listen port.
#[derive(Clone, Default)]
struct PortReuse {
    listening: Arc<HopscotchMap<(IpAddr, u16), (), rapidhash::fast::RandomState>>,
}

impl PortReuse {
    fn register(&self, ip: IpAddr, port: u16) {
        self.listening.insert((ip, port), ());
    }

    fn unregister(&self, ip: IpAddr, port: u16) {
        self.listening.remove(&(ip, port));
    }

    /// The address to bind a dial to `remote` from: the port of a listener
    /// of the same family and the same scope (loopback or not).
    fn local_dial_addr(&self, remote: IpAddr) -> Option<SocketAddr> {
        self.listening.keys().find_map(|(ip, port)| {
            (ip.is_ipv4() == remote.is_ipv4() && ip.is_loopback() == remote.is_loopback()).then(
                || {
                    let any = if remote.is_ipv4() {
                        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
                    } else {
                        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
                    };
                    SocketAddr::new(any, port)
                },
            )
        })
    }
}

/// The TCP transport: dials and listens on `/ip4|ip6/<ip>/tcp/<port>`
/// addresses.
#[derive(Default)]
pub struct Transport {
    port_reuse: PortReuse,
    listeners: SelectAll<Listener>,
    pending: VecDeque<TransportEvent<Ready<io::Result<TcpStream>>, io::Error>>,
}

impl libp2p::core::Transport for Transport {
    type Output = TcpStream;
    type Error = io::Error;
    type ListenerUpgrade = Ready<io::Result<TcpStream>>;
    type Dial = Pin<Box<dyn Future<Output = io::Result<TcpStream>> + Send>>;

    fn listen_on(
        &mut self,
        id: ListenerId,
        addr: Multiaddr,
    ) -> Result<(), TransportError<io::Error>> {
        let socket_addr = socket_addr(&addr)
            .ok_or_else(|| TransportError::MultiaddrNotSupported(addr.clone()))?;
        let listener = self
            .listen(id, socket_addr)
            .map_err(TransportError::Other)?;
        self.listeners.push(listener);
        Ok(())
    }

    fn remove_listener(&mut self, id: ListenerId) -> bool {
        match self.listeners.iter_mut().find(|listener| listener.id == id) {
            Some(listener) => {
                listener.close(Ok(()));
                true
            }
            None => false,
        }
    }

    fn dial(
        &mut self,
        addr: Multiaddr,
        opts: DialOpts,
    ) -> Result<Self::Dial, TransportError<io::Error>> {
        let remote = match socket_addr(&addr) {
            Some(remote) if remote.port() != 0 && !remote.ip().is_unspecified() => remote,
            _ => return Err(TransportError::MultiaddrNotSupported(addr)),
        };
        let reuse = opts.port_use == PortUse::Reuse;
        let bind = reuse
            .then(|| self.port_reuse.local_dial_addr(remote.ip()))
            .flatten();
        Ok(Box::pin(async move {
            match connect(remote, reuse, bind).await {
                // The listen port is already connected to this remote: dial
                // from a new port.
                Err(error) if bind.is_some() && error.kind() == io::ErrorKind::AddrNotAvailable => {
                    connect(remote, false, None).await
                }
                connected => connected,
            }
        }))
    }

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<TransportEvent<Self::ListenerUpgrade, io::Error>> {
        if let Some(event) = self.pending.pop_front() {
            return Poll::Ready(event);
        }
        match self.listeners.poll_next_unpin(cx) {
            Poll::Ready(Some(event)) => Poll::Ready(event),
            _ => Poll::Pending,
        }
    }
}

impl Transport {
    fn listen(&mut self, id: ListenerId, addr: SocketAddr) -> io::Result<Listener> {
        let socket = socket(addr, true)?;
        socket.bind(addr)?;
        let listener = socket.listen(BACKLOG)?;
        let local = listener.get_ref().local_addr()?;
        let interfaces = if local.ip().is_unspecified() {
            Some(Interfaces::new(local.ip()))
        } else {
            self.port_reuse.register(local.ip(), local.port());
            self.pending.push_back(TransportEvent::NewAddress {
                listener_id: id,
                listen_addr: multiaddr(local),
            });
            None
        };
        Ok(Listener {
            id,
            local,
            socket: Arc::new(listener),
            accepting: None,
            interfaces,
            port_reuse: self.port_reuse.clone(),
            pause: None,
            closed: None,
            waker: None,
        })
    }
}

/// A socket for `addr`'s family set up as libp2p-tcp sets one up.
fn socket(addr: SocketAddr, reuse_port: bool) -> io::Result<TcpSocket> {
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(addr),
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )?;
    if addr.is_ipv6() {
        socket.set_only_v6(true)?;
    }
    socket.set_tcp_nodelay(true)?;
    socket.set_reuse_address(true)?;
    #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
    if reuse_port {
        socket.set_reuse_port(true)?;
    }
    #[cfg(not(all(unix, not(any(target_os = "solaris", target_os = "illumos")))))]
    let _ = reuse_port;
    Ok(TcpSocket::from_std_stream(socket.into()))
}

async fn connect(
    remote: SocketAddr,
    reuse_port: bool,
    bind: Option<SocketAddr>,
) -> io::Result<TcpStream> {
    let socket = socket(remote, reuse_port)?;
    if let Some(bind) = bind {
        socket.bind(bind)?;
    }
    socket.connect(remote).await
}

/// One listener's events: its addresses, then each accepted connection.
struct Listener {
    id: ListenerId,
    local: SocketAddr,
    socket: Arc<TcpListener>,
    accepting: Option<Accepting>,
    interfaces: Option<Interfaces>,
    port_reuse: PortReuse,
    pause: Option<Pin<Box<nuclei::time::Sleep>>>,
    /// Why the listener closed, until that is reported.
    closed: Option<Option<io::Result<()>>>,
    waker: Option<Waker>,
}

impl Listener {
    fn close(&mut self, reason: io::Result<()>) {
        if self.closed.is_some() {
            return;
        }
        self.closed = Some(Some(reason));
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    fn release_ports(&mut self) {
        match &self.interfaces {
            Some(interfaces) => {
                for ip in interfaces.addresses() {
                    self.port_reuse.unregister(ip, self.local.port());
                }
            }
            None => self
                .port_reuse
                .unregister(self.local.ip(), self.local.port()),
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.release_ports();
    }
}

impl Stream for Listener {
    type Item = TransportEvent<Ready<io::Result<TcpStream>>, io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if !this
            .waker
            .as_ref()
            .is_some_and(|waker| waker.will_wake(cx.waker()))
        {
            this.waker = Some(cx.waker().clone());
        }
        if let Some(reason) = &mut this.closed {
            return Poll::Ready(reason.take().map(|reason| {
                this.release_ports();
                TransportEvent::ListenerClosed {
                    listener_id: this.id,
                    reason,
                }
            }));
        }
        if let Some(interfaces) = &mut this.interfaces {
            if let Poll::Ready(change) = interfaces.poll_change(cx) {
                let port = this.local.port();
                return Poll::Ready(Some(match change {
                    Change::Up(ip) => {
                        this.port_reuse.register(ip, port);
                        TransportEvent::NewAddress {
                            listener_id: this.id,
                            listen_addr: multiaddr(SocketAddr::new(ip, port)),
                        }
                    }
                    Change::Down(ip) => {
                        this.port_reuse.unregister(ip, port);
                        TransportEvent::AddressExpired {
                            listener_id: this.id,
                            listen_addr: multiaddr(SocketAddr::new(ip, port)),
                        }
                    }
                    Change::Failed(error) => TransportEvent::ListenerError {
                        listener_id: this.id,
                        error,
                    },
                }));
            }
        }
        if let Some(pause) = &mut this.pause {
            if pause.as_mut().poll(cx).is_pending() {
                return Poll::Pending;
            }
            this.pause = None;
        }
        let accepting = this.accepting.get_or_insert_with(|| {
            let socket = Arc::clone(&this.socket);
            Box::pin(async move { socket.accept().await })
        });
        match accepting.as_mut().poll(cx) {
            Poll::Ready(Ok((stream, remote))) => {
                this.accepting = None;
                if let Err(error) = stream.get_ref().set_nodelay(true) {
                    tracing::debug!(%error, "could not disable Nagle's algorithm on an accepted connection");
                }
                let local = stream.get_ref().local_addr().unwrap_or(this.local);
                Poll::Ready(Some(TransportEvent::Incoming {
                    listener_id: this.id,
                    upgrade: ready(Ok(stream)),
                    local_addr: multiaddr(local),
                    send_back_addr: multiaddr(remote),
                }))
            }
            Poll::Ready(Err(error)) => {
                this.accepting = None;
                this.pause = Some(Box::pin(nuclei::time::sleep(ACCEPT_BACKOFF)));
                Poll::Ready(Some(TransportEvent::ListenerError {
                    listener_id: this.id,
                    error,
                }))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// The socket address of `/ip4|ip6/<ip>/tcp/<port>`, with an optional
/// trailing `/p2p/<peer>`.
fn socket_addr(addr: &Multiaddr) -> Option<SocketAddr> {
    let mut protocols = addr.iter().collect::<Vec<_>>();
    if matches!(protocols.last(), Some(Protocol::P2p(_))) {
        protocols.pop();
    }
    match protocols.as_slice() {
        [Protocol::Ip4(ip), Protocol::Tcp(port)] => Some(SocketAddr::new(IpAddr::V4(*ip), *port)),
        [Protocol::Ip6(ip), Protocol::Tcp(port)] => Some(SocketAddr::new(IpAddr::V6(*ip), *port)),
        _ => None,
    }
}

fn multiaddr(addr: SocketAddr) -> Multiaddr {
    Multiaddr::empty()
        .with(addr.ip().into())
        .with(Protocol::Tcp(addr.port()))
}
