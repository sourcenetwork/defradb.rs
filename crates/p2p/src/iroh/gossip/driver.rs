//! The task that runs iroh-gossip's protocol state.
//!
//! Gossip rides the connections the endpoint already admitted and never
//! opens or closes one itself. A message for a peer with no connection asks
//! the endpoint to dial it, through the same admission checks as any other
//! dial. When the protocol forgets a peer, only our send queue to it is
//! dropped. Revoking a peer closes its connections, and that is what ends its
//! gossip. Modeled in `proofs/tla/GossipOwnership.tla`.

use std::collections::VecDeque;
use std::sync::Arc;

use bytes::Bytes;
use iroh::endpoint::{Connection, RecvStream};
use iroh::EndpointId;
use iroh_gossip::proto::util::TimerMap;
use iroh_gossip::proto::{self, Command, Event, InEvent, OutEvent, PeerData, Scope, TopicId};
use n0_future::time::{Duration, Instant};
use rand::rngs::StdRng;
use rand::seq::IteratorRandom;
use rand::SeedableRng;
use rapidhash::{HashMapExt, RapidHashMap};
use tracing::debug;

use crate::error::{Error, Result};
use crate::transport::PeerAddr;

use super::delivery::{Closed, Delivery};
use super::frame::{read_gossip_frame, write_gossip_frame, ProtoMessage};
use crate::iroh::mailbox::{Mailbox, Wake};
use super::{protocols, spawn_task, topic_id, AddrInfo, Input, Network, SpawnedTasks, Subscribed};

/// Messages queued for one peer's stream before more are dropped.
const SEND_QUEUE: usize = 64;

/// Messages held for a peer while it is being dialled.
const PENDING_PER_PEER: usize = 64;

/// Peers a new topic is joined through; every peer that connects later is
/// joined as well.
const JOIN_SAMPLE: usize = 8;

/// Peers whose gossiped address we keep for dialling.
const MAX_LEARNED: usize = 1024;

/// How long a waiting publish on a topic we just joined holds for a
/// neighbour.
/// Gossip has no store-and-forward, so a broadcast with no neighbour
/// reaches nobody.
const NEIGHBOR_WAIT: Duration = Duration::from_secs(10);

/// Inputs handled before the driver lets other tasks on its thread run.
const INPUTS_PER_SLICE: usize = 64;

/// QUIC stream error code for a uni stream with a tag we do not serve.
const RESET_UNKNOWN_SERVICE: u32 = 2;

pub(super) struct Driver<N> {
    state: proto::State<EndpointId, StdRng>,
    timers: TimerMap<proto::Timer<EndpointId>>,
    queued: VecDeque<InEvent<EndpointId>>,
    topics: RapidHashMap<TopicId, Joined>,
    peers: RapidHashMap<EndpointId, Peer>,
    learned: RapidHashMap<EndpointId, Vec<PeerAddr>>,
    deferred: Vec<Deferred>,
    own_addr: AddrInfo,
    rng: StdRng,
    io: Io,
    subscribed: Subscribed,
    delivery: Delivery,
    network: N,
}

/// What spawned stream tasks need.
struct Io {
    inbox: Arc<Mailbox<Input>>,
    tasks: SpawnedTasks,
    limit: usize,
    next_writer: u64,
}

struct Joined {
    name: String,
    /// Joined only to publish; left once the publish is sent.
    ephemeral: bool,
    neighbors: usize,
}

struct Deferred {
    topic: TopicId,
    payload: Bytes,
    deadline: Instant,
}

#[derive(Default)]
struct Peer {
    connections: Vec<Connection>,
    writer: Option<Writer>,
    pending: Vec<ProtoMessage>,
    dialing: bool,
}

impl Peer {
    fn live_connection(&self) -> Option<&Connection> {
        self.connections
            .iter()
            .find(|connection| connection.close_reason().is_none())
    }
}

/// Our send queue to one peer. Dropping it ends the stream once the queue
/// drains.
struct Writer {
    id: u64,
    queue: Arc<Mailbox<ProtoMessage>>,
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.queue.close();
    }
}

impl<N> Driver<N>
where
    N: Network,
{
    pub(super) fn new(
        me: EndpointId,
        network: N,
        delivery: Delivery,
        inbox: Arc<Mailbox<Input>>,
        subscribed: Subscribed,
        tasks: SpawnedTasks,
    ) -> Self {
        let own_addr = network.local_addr();
        let config = proto::Config::default();
        let limit = config.max_message_size;
        let state = proto::State::new(
            me,
            peer_data(&own_addr),
            config,
            StdRng::from_rng(&mut rand::rng()),
        );
        Self {
            state,
            timers: TimerMap::new(),
            queued: VecDeque::new(),
            topics: RapidHashMap::new(),
            peers: RapidHashMap::new(),
            learned: RapidHashMap::new(),
            deferred: Vec::new(),
            own_addr,
            rng: StdRng::from_rng(&mut rand::rng()),
            io: Io {
                inbox,
                tasks,
                limit,
                next_writer: 0,
            },
            subscribed,
            delivery,
            network,
        }
    }

    pub(super) async fn run(mut self) {
        let inbox = Arc::clone(&self.io.inbox);
        let mut handled = 0;
        loop {
            handled = (handled + 1) % INPUTS_PER_SLICE;
            if handled == 0 {
                tokio::task::yield_now().await;
            }
            let wake_at = self.next_deadline();
            let input = tokio::select! {
                input = inbox.next() => match input {
                    Some(input) => Some(input),
                    None => break,
                },
                () = sleep_until(wake_at) => None,
            };
            let outcome = match input {
                Some(input) => self.on_input(input).await,
                None => self.on_deadlines().await,
            };
            if outcome.is_err() {
                break;
            }
        }
        debug!("Gossip driver stopped");
    }

    fn next_deadline(&self) -> Option<Instant> {
        let deferred = self.deferred.iter().map(|d| d.deadline).min();
        match (self.timers.first().copied(), deferred) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    async fn on_input(&mut self, input: Input) -> std::result::Result<(), Closed> {
        match input {
            Input::Join(name) => self.join(name, false),
            Input::Quit(name) => self.quit(topic_id(&name)),
            Input::Publish {
                topic,
                payload,
                wait_for_neighbor,
            } => self.publish(topic, payload, wait_for_neighbor),
            Input::Connected { peer, connection } => self.connected(peer, connection),
            Input::Closed { peer, connection } => self.closed(peer, connection),
            Input::WriterDone { peer, writer } => {
                if let Some(entry) = self.peers.get_mut(&peer) {
                    if entry.writer.as_ref().is_some_and(|w| w.id == writer) {
                        entry.writer = None;
                    }
                }
            }
            Input::DialFailed(peer) => {
                if let Some(entry) = self.peers.get_mut(&peer) {
                    entry.dialing = false;
                    entry.pending.clear();
                }
                self.drop_if_unreachable(peer);
            }
            Input::Received { peer, message } => {
                self.queued.push_back(InEvent::RecvMessage(peer, message));
            }
        }
        self.drain().await
    }

    async fn on_deadlines(&mut self) -> std::result::Result<(), Closed> {
        let now = Instant::now();
        let expired: Vec<_> = self.timers.drain_until(&now).map(|(_, t)| t).collect();
        self.queued
            .extend(expired.into_iter().map(InEvent::TimerExpired));
        let mut due = Vec::new();
        self.deferred.retain(|deferred| {
            let keep = deferred.deadline > now;
            if !keep {
                due.push(deferred.topic);
            }
            keep
        });
        for topic in due {
            debug!(
                topic = %topic.fmt_short(),
                "No gossip neighbour joined in time; broadcasting anyway"
            );
            self.flush_deferred(topic, Some(now));
        }
        self.drain().await
    }

    /// Feeds queued events to the protocol, and acts on what it asks for,
    /// until nothing is left.
    async fn drain(&mut self) -> std::result::Result<(), Closed> {
        while let Some(event) = self.queued.pop_front() {
            let now = Instant::now();
            let out: Vec<_> = self.state.handle(event, now, None).collect();
            for event in out {
                match event {
                    OutEvent::SendMessage(peer, message) => self.send(peer, message),
                    OutEvent::EmitEvent(topic, event) => self.emit(topic, event).await?,
                    OutEvent::ScheduleTimer(delay, timer) => self.timers.insert(now + delay, timer),
                    OutEvent::DisconnectPeer(peer) => self.forget(peer),
                    OutEvent::PeerData(peer, data) => self.learn(peer, &data),
                }
            }
        }
        Ok(())
    }

    fn join(&mut self, name: String, ephemeral: bool) {
        let id = topic_id(&name);
        if let Some(joined) = self.topics.get_mut(&id) {
            joined.ephemeral &= ephemeral;
            return;
        }
        self.topics.insert(
            id,
            Joined {
                name,
                ephemeral,
                neighbors: 0,
            },
        );
        let bootstrap = self
            .peers
            .iter()
            .filter(|(_, peer)| peer.live_connection().is_some())
            .map(|(id, _)| *id)
            .sample(&mut self.rng, JOIN_SAMPLE);
        self.queued
            .push_back(InEvent::Command(id, Command::Join(bootstrap)));
    }

    fn quit(&mut self, id: TopicId) {
        if self.topics.remove(&id).is_some() {
            self.deferred.retain(|deferred| deferred.topic != id);
            self.queued.push_back(InEvent::Command(id, Command::Quit));
        }
    }

    fn publish(&mut self, name: String, payload: Bytes, wait_for_neighbor: bool) {
        let id = topic_id(&name);
        let ready = self
            .topics
            .get(&id)
            .is_some_and(|joined| !joined.ephemeral || joined.neighbors > 0);
        if ready {
            self.broadcast(id, payload);
            return;
        }
        self.join(name, true);
        self.deferred.push(Deferred {
            topic: id,
            payload,
            deadline: Instant::now() + NEIGHBOR_WAIT,
        });
        if !wait_for_neighbor {
            self.flush_deferred(id, None);
        }
    }

    fn broadcast(&mut self, id: TopicId, payload: Bytes) {
        self.queued.push_back(InEvent::Command(
            id,
            Command::Broadcast(payload, Scope::Swarm),
        ));
    }

    /// Sends the publishes waiting on `id`: those past `due`, or all of them
    /// when a neighbour arrived. An ephemeral topic is left once none wait.
    fn flush_deferred(&mut self, id: TopicId, due: Option<Instant>) {
        let mut ready = Vec::new();
        self.deferred.retain(|deferred| {
            let send = deferred.topic == id && due.is_none_or(|now| deferred.deadline <= now);
            if send {
                ready.push(deferred.payload.clone());
            }
            !send
        });
        for payload in ready {
            self.broadcast(id, payload);
        }
        let still_waiting = self.deferred.iter().any(|deferred| deferred.topic == id);
        let ephemeral = self.topics.get(&id).is_some_and(|joined| joined.ephemeral);
        if ephemeral && !still_waiting {
            self.quit(id);
        }
    }

    async fn emit(
        &mut self,
        id: TopicId,
        event: Event<EndpointId>,
    ) -> std::result::Result<(), Closed> {
        let Some(joined) = self.topics.get_mut(&id) else {
            return Ok(());
        };
        let name = joined.name.clone();
        match event {
            Event::NeighborUp(_) => joined.neighbors += 1,
            Event::NeighborDown(_) => joined.neighbors = joined.neighbors.saturating_sub(1),
            Event::Received(_) => {}
        }
        let neighbors = self.subscribed.get(&name);
        match event {
            Event::NeighborUp(peer) => {
                self.flush_deferred(id, None);
                if let Some(neighbors) = neighbors {
                    neighbors.insert(peer, ());
                    self.delivery.neighbor(&name, peer, true).await?;
                }
            }
            Event::NeighborDown(peer) => {
                if let Some(neighbors) = neighbors {
                    neighbors.remove(&peer);
                    self.delivery.neighbor(&name, peer, false).await?;
                }
            }
            event @ Event::Received(_) => {
                if neighbors.is_some() {
                    self.delivery.received(&name, event).await?;
                }
            }
        }
        Ok(())
    }

    fn send(&mut self, peer: EndpointId, message: ProtoMessage) {
        let entry = self.peers.entry(peer).or_default();
        if entry.writer.is_none() {
            if let Some(connection) = entry.live_connection() {
                entry.writer = Some(self.io.spawn_writer(peer, connection.clone()));
            }
        }
        if let Some(writer) = &entry.writer {
            if writer.queue.try_push(message).is_err() {
                debug!(peer = %peer.fmt_short(), "Gossip send queue full; dropping a message");
            }
            return;
        }
        if entry.pending.len() < PENDING_PER_PEER {
            entry.pending.push(message);
        }
        if !entry.dialing {
            entry.dialing = true;
            let addrs = self.learned.get(&peer).cloned().unwrap_or_default();
            self.network.dial(peer, addrs);
        }
    }

    fn connected(&mut self, peer: EndpointId, connection: Connection) {
        let id = connection.stable_id();
        let entry = self.peers.entry(peer).or_default();
        entry.dialing = false;
        // A revoke that raced the admission check has already closed it.
        if connection.close_reason().is_some() {
            self.drop_if_unreachable(peer);
            return;
        }
        entry
            .connections
            .retain(|known| known.close_reason().is_none());
        if entry
            .connections
            .iter()
            .any(|known| known.stable_id() == id)
        {
            return;
        }
        let first = entry.connections.is_empty();
        entry.connections.push(connection.clone());
        self.io.serve(peer, connection.clone());

        if !entry.pending.is_empty() && entry.writer.is_none() {
            entry.writer = Some(self.io.spawn_writer(peer, connection));
        }
        if let Some(writer) = &entry.writer {
            for message in entry.pending.drain(..) {
                let _ = writer.queue.try_push(message);
            }
        }
        if !first {
            return;
        }

        let own_addr = self.network.local_addr();
        if own_addr != self.own_addr {
            self.queued
                .push_back(InEvent::UpdatePeerData(peer_data(&own_addr)));
            self.own_addr = own_addr;
        }
        for id in self.topics.keys() {
            self.queued
                .push_back(InEvent::Command(*id, Command::Join(vec![peer])));
        }
    }

    fn closed(&mut self, peer: EndpointId, connection: usize) {
        if let Some(entry) = self.peers.get_mut(&peer) {
            entry
                .connections
                .retain(|known| known.stable_id() != connection && known.close_reason().is_none());
        }
        self.drop_if_unreachable(peer);
    }

    /// A peer with no live connection and no dial under way is gone as far
    /// as the protocol is concerned.
    fn drop_if_unreachable(&mut self, peer: EndpointId) {
        let unreachable = self
            .peers
            .get(&peer)
            .is_some_and(|entry| !entry.dialing && entry.live_connection().is_none());
        if unreachable {
            self.peers.remove(&peer);
            self.queued.push_back(InEvent::PeerDisconnected(peer));
        }
    }

    /// The protocol is done with `peer`. Its connection is shared with every
    /// other protocol, so only our stream to it ends.
    fn forget(&mut self, peer: EndpointId) {
        if let Some(entry) = self.peers.get_mut(&peer) {
            entry.writer = None;
            entry.pending.clear();
        }
    }

    fn learn(&mut self, peer: EndpointId, data: &PeerData) {
        if self.learned.len() >= MAX_LEARNED && !self.learned.contains_key(&peer) {
            return;
        }
        match postcard::from_bytes::<AddrInfo>(data.as_bytes()) {
            Ok(info) => {
                self.learned.insert(peer, info.dial_hints());
            }
            Err(error) => {
                debug!(peer = %peer.fmt_short(), %error, "Ignoring undecodable gossip peer data");
            }
        }
    }
}

impl Io {
    fn spawn_writer(&mut self, peer: EndpointId, connection: Connection) -> Writer {
        let id = self.next_writer;
        self.next_writer += 1;
        let queue = Arc::new(Mailbox::new(SEND_QUEUE, Wake::default()));
        let inbox = Arc::clone(&self.inbox);
        let limit = self.limit;
        let stream_queue = Arc::clone(&queue);
        let _ = spawn_task(&self.tasks, async move {
            if let Err(error) = write_stream(&connection, &stream_queue, limit).await {
                debug!(peer = %peer.fmt_short(), %error, "Gossip stream to peer ended");
            }
            inbox.push(Input::WriterDone { peer, writer: id }).await;
        });
        Writer { id, queue }
    }

    /// Reads the gossip streams `peer` opens on `connection`, and reports the
    /// connection closed once it is.
    fn serve(&self, peer: EndpointId, connection: Connection) {
        let inbox = Arc::clone(&self.inbox);
        let tasks = Arc::clone(&self.tasks);
        let limit = self.limit;
        let _ = spawn_task(&self.tasks, async move {
            while let Ok(mut recv) = connection.accept_uni().await {
                let inbox = Arc::clone(&inbox);
                let _ = spawn_task(&tasks, async move {
                    if let Err(error) = read_stream(peer, &mut recv, &inbox, limit).await {
                        debug!(peer = %peer.fmt_short(), %error, "Gossip stream from peer ended");
                    }
                });
            }
            let connection = connection.stable_id();
            inbox.push(Input::Closed { peer, connection }).await;
        });
    }
}

async fn write_stream(
    connection: &Connection,
    queue: &Mailbox<ProtoMessage>,
    limit: usize,
) -> Result<()> {
    let mut send = connection
        .open_uni()
        .await
        .map_err(|e| Error::Transport(e.to_string()))?;
    protocols::write_stream_tag(&mut send, protocols::STREAM_GOSSIP).await?;
    while let Some(message) = queue.next().await {
        write_gossip_frame(&mut send, &message, limit).await?;
    }
    let _ = send.finish();
    Ok(())
}

async fn read_stream(
    peer: EndpointId,
    recv: &mut RecvStream,
    inbox: &Mailbox<Input>,
    limit: usize,
) -> Result<()> {
    let tag = protocols::read_stream_tag(recv).await?;
    if tag != protocols::STREAM_GOSSIP {
        let _ = recv.stop(RESET_UNKNOWN_SERVICE.into());
        return Err(Error::Codec("unknown uni stream tag".to_string()));
    }
    while let Some(message) = read_gossip_frame(recv, limit).await? {
        inbox.push(Input::Received { peer, message }).await;
    }
    Ok(())
}

fn peer_data(addr: &AddrInfo) -> PeerData {
    PeerData::new(postcard::to_stdvec(addr).unwrap_or_default())
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => n0_future::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
#[path = "../../../tests/iroh/gossip_driver.rs"]
mod tests;
