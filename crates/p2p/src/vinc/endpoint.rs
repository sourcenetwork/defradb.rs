//! The endpoint task: one vinc endpoint on one edge thread.
//!
//! [`spawn_endpoint`] binds a nuclei UDP socket and builds the endpoint on a
//! thread of the edge ([`edge::Edge::spawn_pinned`]). Everything vinc hands
//! out (`Endpoint`, `Connection`, streams) is `!Send` and stays there: the
//! command loop, the accept loop, one task per connection and per stream,
//! all spawned local to that thread. Nothing enters `edge::compat`.

use std::cell::{Cell, RefCell};
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::FutureExt;
use nuclei::task::JoinSet;
use rapidhash::{HashMapExt, RapidHashMap};
use tracing::{debug, warn};
use vinc::{Connection, Hosted, NodeAddr, NodeId};

use crate::bitswap::ReplicatorRegistry;
use crate::error::{Error, Result};
use crate::rt::{mpsc, oneshot};
use crate::sync::DispatchSender;
use crate::transport::{PeerAddr, PeerConnectionInfo, PeerId, TransportEvent};

use super::addr::{node_id, node_to_peer_id, socket_addrs};
use super::admission::Allowlist;
use super::command::{Command, Reply};
use super::config::VincEndpointConfig;
use super::directory::PeerDirectory;
use super::inbound_budget::InboundBudget;
use super::reply_table::ReplyTable;
use super::services::{self, ALPN_MUX};
use super::token::VincResponseToken;
use super::transport::VincTransport;
use super::wire::vinc_error;

/// Events each dispatch class may queue before its senders wait.
const EVENT_LANE_CAPACITY: usize = 64;
/// Commands the facade may queue before its callers wait.
const COMMAND_CAPACITY: usize = 256;
/// Replies owed at once; a request past this is refused busy.
const MAX_PENDING_REPLIES: usize = 4096;
/// Tasks local to the endpoint at once: streams, dials and requests.
const MAX_LOCAL_TASKS: usize = 8192;
/// Callers waiting for a peer to connect.
const MAX_CONNECT_WAITERS: usize = 1024;
/// How long a reply stream is held for the engine's answer.
pub(super) const REPLY_DEADLINE: Duration = Duration::from_secs(60);
/// How often owed replies past their deadline are reset.
const SWEEP_INTERVAL: Duration = Duration::from_secs(5);
pub(super) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub type EventSender = DispatchSender<TransportEvent<VincResponseToken>>;
pub type EventReceiver = crate::sync::DispatchReceiver<TransportEvent<VincResponseToken>>;

/// The endpoint's discovery: records seen on this node's connections.
pub(super) type Discovery = vinc::Records<PeerDirectory>;
pub(super) type Ep<H> = vinc::Endpoint<H, Discovery>;

/// Spawn the endpoint on a thread of `edge` and return its facade, the
/// transport events, the replicator registry the coordinator shares, and the
/// task, which ends after `shutdown` or once the edge closes.
pub async fn spawn_endpoint(
    config: VincEndpointConfig,
    edge: edge::Edge,
) -> Result<(
    VincTransport,
    EventReceiver,
    Arc<ReplicatorRegistry>,
    edge::Task<()>,
)> {
    let (command_tx, command_rx) = mpsc::channel::<Command>(COMMAND_CAPACITY);
    let (running, stopped) = oneshot::channel::<()>();
    let (event_tx, event_rx) =
        crate::sync::dispatch_queue::<TransportEvent<VincResponseToken>>(EVENT_LANE_CAPACITY);
    let (ready_tx, ready_rx) = oneshot::channel::<Result<SocketAddr>>();
    let replicators = Arc::new(ReplicatorRegistry::new());
    let seed = *config.seed;
    let task_edge = edge.clone();
    let commands = command_tx.clone();
    let task = edge
        .spawn_pinned(move || {
            run(
                config, task_edge, command_rx, commands, event_tx, running, ready_tx,
            )
        })
        .within(Duration::MAX);
    match ready_rx.await {
        Ok(Ok(local)) => debug!(%local, "vinc endpoint bound"),
        Ok(Err(error)) => return Err(error),
        Err(_) => {
            return Err(Error::Transport(
                "the vinc endpoint stopped before it was bound".to_owned(),
            ))
        }
    }
    let transport = VincTransport::new(
        (command_tx, stopped.shared()),
        &seed,
        Arc::clone(&replicators),
    );
    Ok((transport, event_rx, replicators, task))
}

/// The host's I/O for vinc: one nuclei UDP socket and nuclei's clock.
pub(super) struct NucleiIo {
    pub(super) socket: Rc<nuclei::Handle<std::net::UdpSocket>>,
}

impl vinc::Io for NucleiIo {
    async fn recv(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr, usize)> {
        let (n, from) = self.socket.recv_from(buf).await?;
        Ok((n, from, n))
    }

    async fn send(&self, buf: &[u8], to: SocketAddr, segment: usize) -> std::io::Result<usize> {
        // One datagram per call: vinc is built with no segmentation offload,
        // so a transmit is one datagram of `segment` bytes.
        let mut sent = 0;
        for datagram in buf.chunks(segment.max(1)) {
            self.socket.send_to(datagram, to).await?;
            sent += 1;
        }
        Ok(sent)
    }

    async fn sleep(&self, at: Instant) {
        nuclei::time::sleep_until_precise(at).await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn run(
    config: VincEndpointConfig,
    edge: edge::Edge,
    command_rx: mpsc::Receiver<Command>,
    commands: mpsc::Sender<Command>,
    events: EventSender,
    running: oneshot::Sender<()>,
    ready: oneshot::Sender<Result<SocketAddr>>,
) {
    let built = async {
        let socket = nuclei::Handle::<std::net::UdpSocket>::bind(config.bind)
            .await
            .map_err(|error| {
                Error::Transport(format!(
                    "failed to bind vinc socket {}: {error}",
                    config.bind
                ))
            })?;
        let local = socket
            .local_addr()
            .map_err(|error| Error::Transport(format!("vinc socket has no address: {error}")))?;
        let allowlist = Allowlist::from_config(&config.allowlist)?;
        let directory = PeerDirectory::default();
        let limits = vinc::Limits {
            connections: config.max_connections,
            ..vinc::Limits::default()
        };
        let ep = vinc::Vinc::builder()
            .seed(&config.seed)
            .map_err(|error| vinc_error("vinc identity", error))?
            .alpn(ALPN_MUX)
            .local_addr(local)
            .limits(limits)
            .discovery(vinc::Records::new(directory.clone()))
            .acp(allowlist.clone())
            .build_io(NucleiIo {
                socket: Rc::new(socket),
            })
            .map_err(|error| vinc_error("vinc endpoint", error))?;
        Ok::<_, Error>((ep, allowlist, directory))
    }
    .await;
    let (ep, allowlist, directory) = match built {
        Ok(built) => built,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let node = Rc::new(Node {
        ep,
        edge,
        events,
        commands,
        allowlist,
        directory,
        identity: config.node_identity.clone(),
        peers: RefCell::new(RapidHashMap::new()),
        replies: ReplyTable::new(MAX_PENDING_REPLIES),
        budget: Arc::new(InboundBudget::default()),
        tasks: RefCell::new(JoinSet::new()),
        waiters: RefCell::new(Vec::new()),
        syncs: RefCell::new(RapidHashMap::new()),
        next_query: Cell::new(1),
        next_connection: Cell::new(1),
    });
    let _ = ready.send(Ok(node.ep.local_addr()));
    node.serve(command_rx, running).await;
}

/// One live connection to a peer.
pub(super) struct Link<H: Hosted> {
    pub(super) serial: u64,
    pub(super) connection: Connection<H>,
    pub(super) since: Instant,
}

/// The endpoint and everything it owns, on its one thread.
pub(super) struct Node<H: Hosted + 'static> {
    pub(super) ep: Ep<H>,
    pub(super) edge: edge::Edge,
    pub(super) events: EventSender,
    pub(super) commands: mpsc::Sender<Command>,
    pub(super) allowlist: Allowlist,
    pub(super) directory: PeerDirectory,
    pub(super) identity: Option<Arc<identity::RawIdentity>>,
    pub(super) peers: RefCell<RapidHashMap<NodeId, Vec<Link<H>>>>,
    pub(super) replies: ReplyTable<vinc::SendStream<H>>,
    pub(super) budget: Arc<InboundBudget>,
    tasks: RefCell<JoinSet<()>>,
    waiters: RefCell<Vec<(NodeId, Reply<()>)>>,
    pub(super) syncs: RefCell<RapidHashMap<u64, nuclei::task::AbortHandle>>,
    pub(super) next_query: Cell<u64>,
    next_connection: Cell<u64>,
}

impl<H: Hosted + 'static> Node<H> {
    /// Run `future` on this thread, owned by the endpoint: it is cancelled
    /// when the endpoint stops. `None` when too many tasks run already.
    pub(super) fn spawn<F>(&self, future: F) -> Option<nuclei::task::AbortHandle>
    where
        F: std::future::Future<Output = ()> + 'static,
    {
        let mut tasks = self.tasks.borrow_mut();
        while let Some(done) = tasks.try_join_next() {
            if let Err(error) = done {
                debug!(%error, "a vinc endpoint task ended without finishing");
            }
        }
        if tasks.len() >= MAX_LOCAL_TASKS {
            warn!(
                tasks = tasks.len(),
                "vinc endpoint has no room for another task"
            );
            return None;
        }
        Some(tasks.spawn_local(future))
    }

    async fn serve(
        self: Rc<Self>,
        mut command_rx: mpsc::Receiver<Command>,
        running: oneshot::Sender<()>,
    ) {
        let runner = Rc::clone(&self);
        self.spawn(async move {
            if let Err(error) = runner.ep.run().await {
                warn!(%error, "vinc endpoint failed");
            }
        });
        let acceptor = Rc::clone(&self);
        self.spawn(async move { acceptor.accept_loop().await });
        let sweeper = Rc::clone(&self);
        self.spawn(async move { sweeper.sweep_loop().await });

        let local = node_to_peer_id(&self.ep.id());
        let listening = PeerAddr::new(self.ep.addr().ticket());
        if self
            .events
            .send(TransportEvent::Listening(listening))
            .await
            .is_err()
        {
            warn!("Event channel closed, cannot emit Listening event");
        }
        debug!(peer_id = %local, "vinc endpoint serving");

        let closing = self.edge.closing().fuse();
        futures::pin_mut!(closing);
        let mut shutdown_reply = None;
        loop {
            if running.is_closed() {
                break;
            }
            futures::select! {
                command = command_rx.recv().fuse() => {
                    let Some(command) = command else { break };
                    if let Command::Shutdown { reply } = command {
                        shutdown_reply = Some(reply);
                        break;
                    }
                    Rc::clone(&self).handle(command).await;
                }
                () = closing => break,
            }
        }
        drop(command_rx);
        drop(running);
        for (_, sync) in self.syncs.borrow_mut().drain() {
            sync.abort();
        }
        self.tasks.borrow_mut().abort_all();
        self.ep.close().await;
        debug!(peer_id = %local, "vinc endpoint stopped");
        if let Some(reply) = shutdown_reply {
            let _ = reply.send(Ok(()));
        }
    }

    async fn accept_loop(self: Rc<Self>) {
        while let Some(incoming) = self.ep.accept().await {
            let connection = incoming.into_connection();
            // vinc asked the allowlist and the revocation bar before the
            // session came up; nothing refused reaches here.
            self.connected(connection).await;
        }
    }

    async fn sweep_loop(self: Rc<Self>) {
        loop {
            crate::rt::time::sleep(SWEEP_INTERVAL).await;
            for mut stream in self.replies.sweep(Instant::now()) {
                stream.reset(super::serve::RESET_STALLED).await;
            }
        }
    }

    /// Track `connection`, serve the streams the peer opens on it, give the
    /// peer this node's record, and announce the peer if it is new.
    pub(super) async fn connected(self: &Rc<Self>, connection: Connection<H>) {
        let peer = connection.remote();
        let serial = self.next_connection.get();
        self.next_connection.set(serial + 1);
        let is_new = {
            let mut peers = self.peers.borrow_mut();
            let links = peers.entry(peer).or_default();
            links.push(Link {
                serial,
                connection: connection.clone(),
                since: Instant::now(),
            });
            links.len() == 1
        };
        // The address the peer was seen at is one it can be dialed at again,
        // since vinc sends from the socket it listens on.
        if let Ok(stats) = connection.stats().await {
            if let Some((_, remote)) = stats.route {
                self.ep.add_address(peer, remote);
            }
        }
        let server = Rc::clone(self);
        let conn = connection.clone();
        if self
            .spawn(async move { server.serve_connection(conn, serial).await })
            .is_none()
        {
            connection.close(0).await;
            self.disconnected(&peer, serial).await;
            return;
        }
        let announcer = Rc::clone(self);
        let conn = connection.clone();
        self.spawn(async move { announcer.send_record(&conn).await });
        if is_new {
            self.wake_waiters(&peer);
            if self
                .events
                .send(TransportEvent::PeerConnected(node_to_peer_id(&peer)))
                .await
                .is_err()
            {
                warn!("Event channel closed, cannot emit PeerConnected");
            }
        }
    }

    /// Forget connection `serial` of `peer`; announce the peer gone when it
    /// was its last.
    pub(super) async fn disconnected(&self, peer: &NodeId, serial: u64) {
        drop(self.replies.purge(serial));
        let gone = {
            let mut peers = self.peers.borrow_mut();
            let Some(links) = peers.get_mut(peer) else {
                return;
            };
            let before = links.len();
            links.retain(|link| link.serial != serial);
            if before == links.len() {
                return;
            }
            let gone = links.is_empty();
            if gone {
                peers.remove(peer);
            }
            gone
        };
        if gone
            && self
                .events
                .send(TransportEvent::PeerDisconnected(node_to_peer_id(peer)))
                .await
                .is_err()
        {
            debug!("Event channel closed, cannot emit PeerDisconnected");
        }
    }

    /// A live connection to `peer`, if one is held.
    pub(super) fn link(&self, peer: &NodeId) -> Option<Connection<H>> {
        self.peers.borrow().get(peer).and_then(|links| {
            links
                .iter()
                .rev()
                .find(|link| link.connection.close_reason().is_none())
                .map(|link| link.connection.clone())
        })
    }

    /// A connection to `peer`: one held, or a new dial to `direct`, else to
    /// what the address book and the records know.
    pub(super) async fn connection(
        self: &Rc<Self>,
        peer: &NodeId,
        direct: Vec<SocketAddr>,
    ) -> Result<Connection<H>> {
        if let Some(connection) = self.link(peer) {
            return Ok(connection);
        }
        for addr in &direct {
            self.ep.add_address(*peer, *addr);
        }
        let mut to = NodeAddr::new(*peer);
        to.direct = direct;
        let connection = crate::rt::time::timeout(CONNECT_TIMEOUT, self.ep.connect(to, ALPN_MUX))
            .await
            .map_err(|_| Error::ConnectionTimeout(node_to_peer_id(peer).to_string()))?
            .map_err(|error| vinc_error("dial", error))?;
        self.connected(connection.clone()).await;
        Ok(connection)
    }

    fn wake_waiters(&self, peer: &NodeId) {
        let mut waiters = self.waiters.borrow_mut();
        let mut kept = Vec::with_capacity(waiters.len());
        for (waiting_for, reply) in waiters.drain(..) {
            if waiting_for == *peer {
                let _ = reply.send(Ok(()));
            } else if !reply.is_closed() {
                kept.push((waiting_for, reply));
            }
        }
        *waiters = kept;
    }

    async fn handle(self: Rc<Self>, command: Command) {
        match command {
            Command::Dial {
                peer_id,
                addrs,
                reply,
            } => {
                let dialer = Rc::clone(&self);
                self.run_or_busy(reply, async move {
                    let peer = node_id(&peer_id)?;
                    dialer
                        .connection(&peer, socket_addrs(&addrs))
                        .await
                        .map(drop)
                });
            }
            Command::Disconnect { peer_id, reply } => {
                let result = match node_id(&peer_id) {
                    Ok(peer) => {
                        self.close_links(&peer).await;
                        Ok(())
                    }
                    Err(error) => Err(error),
                };
                let _ = reply.send(result);
            }
            Command::AllowPeer {
                peer_id,
                authority,
                reply,
            } => {
                let _ = reply.send(self.allow(&peer_id, authority));
            }
            Command::DenyPeer { peer_id, reply } => {
                let result = match node_id(&peer_id) {
                    Ok(peer) => self.deny(&peer).await,
                    Err(error) => Err(error),
                };
                let _ = reply.send(result);
            }
            Command::IsPeerRevoked { peer_id, reply } => {
                let _ = reply.send(node_id(&peer_id).map(|peer| self.ep.is_revoked(&peer)));
            }
            Command::ConnectedPeers { reply } => {
                let peers = self.peers.borrow().keys().map(node_to_peer_id).collect();
                let _ = reply.send(Ok(peers));
            }
            Command::WaitConnected { peer_id, reply } => match node_id(&peer_id) {
                Ok(peer) if self.link(&peer).is_some() => {
                    let _ = reply.send(Ok(()));
                }
                Ok(peer) => {
                    let mut waiters = self.waiters.borrow_mut();
                    waiters.retain(|(_, waiting)| !waiting.is_closed());
                    if waiters.len() >= MAX_CONNECT_WAITERS {
                        drop(waiters);
                        let _ = reply.send(Err(Error::Transport(
                            "too many callers are waiting for a vinc peer".to_owned(),
                        )));
                    } else {
                        waiters.push((peer, reply));
                    }
                }
                Err(error) => {
                    let _ = reply.send(Err(error));
                }
            },
            Command::PeerConnection { peer_id, reply } => {
                let result = match node_id(&peer_id) {
                    Ok(peer) => Ok(self.connection_info(&peer).await),
                    Err(error) => Err(error),
                };
                let _ = reply.send(result);
            }
            Command::InboundBudget { reply } => {
                let _ = reply.send(Ok(self.budget.snapshot().to_vec()));
            }
            Command::ListenAddresses { reply } => {
                let addrs = self
                    .ep
                    .addr()
                    .direct
                    .iter()
                    .map(|socket| PeerAddr::new(socket.to_string()))
                    .collect();
                let _ = reply.send(Ok(addrs));
            }
            Command::PeerAddresses { reply } => {
                let mut addrs = Vec::new();
                for (peer, links) in self.peers.borrow().iter() {
                    if let Some(link) = links.first() {
                        addrs.push(format!(
                            "{}{}",
                            vinc::node_to_string(peer),
                            if link.connection.is_dialer() {
                                " (dialed)"
                            } else {
                                ""
                            }
                        ));
                    }
                }
                let _ = reply.send(Ok(addrs));
            }
            Command::NetworkChange { reply } => {
                self.ep.network_change().await;
                let _ = reply.send(Ok(()));
            }
            Command::SetServicePointer { pointer, reply } => {
                let _ = reply.send(self.set_service_pointer(&pointer).await);
            }
            Command::ServicePointer { peer_id, reply } => {
                let result = match node_id(&peer_id) {
                    Ok(peer) => self
                        .ep
                        .lookup(&peer)
                        .await
                        .map(|record| {
                            record
                                .map(|record| record.hints)
                                .filter(|hints| hints.len() == services::SERVICE_POINTER_LEN)
                        })
                        .map_err(|error| vinc_error("record lookup", error)),
                    Err(error) => Err(error),
                };
                let _ = reply.send(result);
            }
            Command::Request {
                peer_id,
                tag,
                body,
                outbound,
                reply,
            } => {
                let requester = Rc::clone(&self);
                let spawned = self.spawn(async move {
                    requester.request(peer_id, tag, body, outbound, reply).await;
                });
                if spawned.is_none() {
                    // The reply sender went with the refused future; the caller
                    // sees the channel close.
                    debug!("vinc request refused: no room for its task");
                }
            }
            Command::Reply { id, body, reply } => {
                let Some(stream) = self.replies.take(id) else {
                    let _ = reply.send(Err(Error::ResponseSend(
                        "the request's stream is gone: its connection ended or its deadline passed"
                            .to_owned(),
                    )));
                    return;
                };
                self.spawn(async move {
                    let _ = reply.send(super::serve::write_reply(stream, body).await);
                });
            }
            Command::DropReply { id } => {
                if let Some(mut stream) = self.replies.take(id) {
                    self.spawn(async move { stream.reset(super::serve::RESET_DROPPED).await });
                }
            }
            Command::SendCarRequest {
                peer_id,
                root_cid,
                reply,
            } => {
                let requester = Rc::clone(&self);
                self.run_or_busy(reply, async move {
                    requester.car_request(&peer_id, root_cid).await
                });
            }
            Command::SyncBlocks {
                request,
                providers,
                reply,
            } => {
                let query_id = crate::QueryId(self.next_query.get());
                self.next_query.set(query_id.0 + 1);
                let syncer = Rc::clone(&self);
                let handle = self.spawn(async move {
                    syncer.block_sync(query_id, request, providers).await;
                });
                match handle {
                    Some(handle) => {
                        let mut syncs = self.syncs.borrow_mut();
                        syncs.retain(|_, sync| !sync.is_finished());
                        syncs.insert(query_id.0, handle);
                        let _ = reply.send(Ok(query_id));
                    }
                    None => {
                        let _ = reply.send(Err(Error::Transport(
                            "no room for another vinc block sync".to_owned(),
                        )));
                    }
                }
            }
            Command::CancelSync { query_id, reply } => {
                let cancelled = self
                    .syncs
                    .borrow_mut()
                    .remove(&query_id.0)
                    .inspect(|sync| sync.abort());
                let _ = reply.send(Ok(cancelled.is_some()));
            }
            Command::Shutdown { reply } => {
                let _ = reply.send(Ok(()));
            }
        }
    }

    /// Run `work` as a task and answer `reply` with its result, or answer
    /// busy when there is no room for the task.
    fn run_or_busy<T, F>(&self, reply: Reply<T>, work: F)
    where
        T: 'static,
        F: std::future::Future<Output = Result<T>> + 'static,
    {
        let reply = Rc::new(Cell::new(Some(reply)));
        let answer = Rc::clone(&reply);
        let spawned = self.spawn(async move {
            let result = work.await;
            if let Some(reply) = answer.take() {
                let _ = reply.send(result);
            }
        });
        if spawned.is_none() {
            if let Some(reply) = reply.take() {
                let _ = reply.send(Err(Error::Transport(
                    "the vinc endpoint is busy".to_owned(),
                )));
            }
        }
    }

    fn allow(&self, peer_id: &PeerId, authority: super::AdmissionAuthority) -> Result<()> {
        let peer = node_id(peer_id)?;
        if !authority.may_admit() {
            return Err(Error::Transport(format!(
                "cannot admit peer {peer_id}: caller is not authorized to admit peers"
            )));
        }
        if self.ep.is_revoked(&peer) && !authority.may_revoke() {
            return Err(Error::Transport(format!(
                "cannot admit peer {peer_id}: the peer is revoked, and lifting a revocation \
                 needs the same authority that can revoke one"
            )));
        }
        // One thread runs both, so nothing lands between these two.
        self.allowlist.allow(peer);
        self.ep.unrevoke(&peer);
        Ok(())
    }

    async fn deny(&self, peer: &NodeId) -> Result<()> {
        // The bar first, so a dial racing this is refused; then every session
        // ends within the call.
        self.ep
            .revoke(peer)
            .await
            .map_err(|error| vinc_error("revoke", error))?;
        self.allowlist.remove(peer);
        self.close_links(peer).await;
        Ok(())
    }

    async fn close_links(&self, peer: &NodeId) {
        self.ep.close_peer(peer).await;
        let serials: Vec<u64> = self
            .peers
            .borrow()
            .get(peer)
            .map(|links| links.iter().map(|link| link.serial).collect())
            .unwrap_or_default();
        for serial in serials {
            self.disconnected(peer, serial).await;
        }
    }

    async fn connection_info(&self, peer: &NodeId) -> Option<PeerConnectionInfo> {
        let (connection, open, since) = {
            let peers = self.peers.borrow();
            let links = peers.get(peer)?;
            let link = links.first()?;
            (link.connection.clone(), links.len(), link.since)
        };
        let stats = connection.stats().await.ok();
        Some(PeerConnectionInfo {
            path: "direct",
            remote_address: stats
                .and_then(|stats| stats.route)
                .map(|(_, remote)| remote.to_string()),
            open_connections: u32::try_from(open).unwrap_or(u32::MAX),
            rtt_micros: stats
                .map(|stats| stats.rtt)
                .filter(|rtt| !rtt.is_zero())
                .map(|rtt| u64::try_from(rtt.as_micros()).unwrap_or(u64::MAX)),
            bytes_sent: stats.map(|stats| stats.bytes_sent),
            bytes_received: stats.map(|stats| stats.bytes_received),
            dialed_by_us: stats.map(|stats| stats.dialed_by_us),
            connected_for_secs: Some(since.elapsed().as_secs()),
        })
    }

    async fn set_service_pointer(&self, pointer: &[u8]) -> Result<()> {
        self.ep
            .set_hints(pointer)
            .map_err(|error| vinc_error("record hints", error))?;
        self.ep
            .publish()
            .await
            .map_err(|error| vinc_error("record publish", error))?;
        let connections: Vec<Connection<H>> = self
            .peers
            .borrow()
            .values()
            .filter_map(|links| links.first().map(|link| link.connection.clone()))
            .collect();
        for connection in connections {
            self.send_record(&connection).await;
        }
        Ok(())
    }
}
