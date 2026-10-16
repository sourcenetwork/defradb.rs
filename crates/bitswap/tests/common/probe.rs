use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::future::{ready, Future, Ready};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use bitswap::{Bitswap, Block, Config};
use bytes::Bytes;
use cid::Cid;
use futures::io::{AsyncReadExt, AsyncWriteExt};
use futures::StreamExt;
use libp2p::core::transport::PortUse;
use libp2p::core::upgrade::{DeniedUpgrade, InboundUpgrade, UpgradeInfo};
use libp2p::core::Endpoint;
use libp2p::swarm::handler::{ConnectionEvent, FullyNegotiatedInbound};
use libp2p::swarm::{
    ConnectionDenied, ConnectionHandler, ConnectionHandlerEvent, ConnectionId, FromSwarm,
    NetworkBehaviour, SubstreamProtocol, SwarmEvent, THandler, THandlerInEvent, THandlerOutEvent,
    ToSwarm,
};
use libp2p::Multiaddr;
use libp2p::{noise, tcp, yamux, PeerId, StreamProtocol, Swarm, SwarmBuilder};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::fixtures::{cid, s};
use super::mem_store::MemStore;

const IDLE_WINDOW: Duration = Duration::from_millis(1500);
const WRITE_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_MAX_WINDOW: Duration = Duration::from_millis(3000);
const SLOW_MAX_WINDOW: Duration = Duration::from_secs(12);
const SLOW_SCENARIO: &str = "14_want_block_H1_H2_H3";

pub struct Rx {
    pub protocol: String,
    pub seq: usize,
    pub frame: Vec<u8>,
}

pub struct StepRun {
    pub responses: Vec<Rx>,
    pub error: Option<String>,
}

type Verdict = Pin<Box<dyn Future<Output = bool> + Send + 'static>>;

fn deny_filter(deny: HashSet<Cid>) -> impl Fn(&PeerId, &Cid) -> Verdict + Send + Sync + 'static {
    move |_peer, cid| {
        let allowed = !deny.contains(cid);
        Box::pin(async move { allowed })
    }
}

fn server_swarm(store: MemStore, deny: HashSet<Cid>) -> Swarm<Bitswap<MemStore>> {
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let peer_id = keypair.public().to_peer_id();
    let mut config = Config::default();
    config.server.as_mut().unwrap().peer_block_request_filter = Some(Box::new(deny_filter(deny)));
    let bitswap = Bitswap::new(peer_id, store, config);
    SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .unwrap()
        .with_behaviour(|_| bitswap)
        .unwrap()
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(120)))
        .build()
}

struct OfferedProtocols {
    protocols: Vec<StreamProtocol>,
}

impl UpgradeInfo for OfferedProtocols {
    type Info = StreamProtocol;
    type InfoIter = std::vec::IntoIter<StreamProtocol>;

    fn protocol_info(&self) -> Self::InfoIter {
        self.protocols.clone().into_iter()
    }
}

impl InboundUpgrade<libp2p::Stream> for OfferedProtocols {
    type Output = (libp2p::Stream, StreamProtocol);
    type Error = Infallible;
    type Future = Ready<Result<Self::Output, Self::Error>>;

    fn upgrade_inbound(self, socket: libp2p::Stream, info: Self::Info) -> Self::Future {
        ready(Ok((socket, info)))
    }
}

type Inbound = mpsc::UnboundedSender<(libp2p::Stream, StreamProtocol)>;

struct AcceptAll {
    protocols: Vec<StreamProtocol>,
    sink: Inbound,
}

struct AcceptAllHandler {
    protocols: Vec<StreamProtocol>,
    sink: Inbound,
}

impl NetworkBehaviour for AcceptAll {
    type ConnectionHandler = AcceptAllHandler;
    type ToSwarm = Infallible;

    fn handle_established_inbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(self.handler())
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        _: Endpoint,
        _: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(self.handler())
    }

    fn on_swarm_event(&mut self, _: FromSwarm) {}

    fn on_connection_handler_event(
        &mut self,
        _: PeerId,
        _: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {}
    }

    fn poll(&mut self, _: &mut Context<'_>) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        Poll::Pending
    }
}

impl AcceptAll {
    fn handler(&self) -> AcceptAllHandler {
        AcceptAllHandler {
            protocols: self.protocols.clone(),
            sink: self.sink.clone(),
        }
    }
}

impl ConnectionHandler for AcceptAllHandler {
    type FromBehaviour = Infallible;
    type ToBehaviour = Infallible;
    type InboundProtocol = OfferedProtocols;
    type OutboundProtocol = DeniedUpgrade;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = Infallible;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol> {
        SubstreamProtocol::new(
            OfferedProtocols {
                protocols: self.protocols.clone(),
            },
            (),
        )
    }

    fn poll(
        &mut self,
        _: &mut Context<'_>,
    ) -> Poll<ConnectionHandlerEvent<Self::OutboundProtocol, Infallible, Infallible>> {
        Poll::Pending
    }

    fn on_behaviour_event(&mut self, event: Infallible) {
        match event {}
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<Self::InboundProtocol, Self::OutboundProtocol, (), Infallible>,
    ) {
        if let ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
            protocol, ..
        }) = event
        {
            let _ = self.sink.send(protocol);
        }
    }
}

#[derive(NetworkBehaviour)]
struct Probe {
    stream: libp2p_stream::Behaviour,
    accept: AcceptAll,
}

fn probe_swarm(
    accept: Vec<StreamProtocol>,
) -> (
    Swarm<Probe>,
    libp2p_stream::Control,
    mpsc::UnboundedReceiver<(libp2p::Stream, StreamProtocol)>,
) {
    let stream = libp2p_stream::Behaviour::new();
    let control = stream.new_control();
    let (sink, inbound) = mpsc::unbounded_channel();
    let swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .unwrap()
        .with_behaviour(|_| Probe {
            stream,
            accept: AcceptAll {
                protocols: accept,
                sink,
            },
        })
        .unwrap()
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(120)))
        .build();
    (swarm, control, inbound)
}

async fn read_frames(
    mut stream: libp2p::Stream,
    protocol: String,
    seq: usize,
    tx: mpsc::UnboundedSender<Rx>,
) {
    loop {
        let mut prefix = Vec::new();
        loop {
            let mut b = [0u8; 1];
            if stream.read_exact(&mut b).await.is_err() {
                return;
            }
            prefix.push(b[0]);
            if b[0] & 0x80 == 0 {
                break;
            }
            if prefix.len() > 10 {
                return;
            }
        }
        let Ok((len, _)) = unsigned_varint::decode::u64(&prefix) else {
            return;
        };
        if len > 64 * 1024 * 1024 {
            return;
        }
        let mut body = vec![0u8; len as usize];
        if stream.read_exact(&mut body).await.is_err() {
            return;
        }
        let frame = [prefix, body].concat();
        let rx = Rx {
            protocol: protocol.clone(),
            seq,
            frame,
        };
        if tx.send(rx).is_err() {
            return;
        }
    }
}

async fn collect(rx: &mut mpsc::UnboundedReceiver<Rx>, max_window: Duration) -> Vec<Rx> {
    let start = Instant::now();
    let mut last = start;
    let mut out = Vec::new();
    loop {
        let deadline = (last + IDLE_WINDOW).min(start + max_window);
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(r)) => {
                out.push(r);
                last = Instant::now();
            }
            _ => return out,
        }
    }
}

fn step_frame(step: &Value) -> Vec<u8> {
    if let Some(hex) = step["frame_hex"].as_str() {
        return hex::decode(hex).unwrap();
    }
    let over = &step["frame_oversize"];
    let length = over["length"].as_u64().unwrap();
    let zeros = over["zero_body"].as_u64().unwrap() as usize;
    let mut buf = unsigned_varint::encode::u64_buffer();
    let mut frame = unsigned_varint::encode::u64(length, &mut buf).to_vec();
    frame.resize(frame.len() + zeros, 0);
    frame
}

pub async fn run(scenario: &Value) -> Vec<StepRun> {
    let name = s(scenario, "name");
    let max_window = if name == SLOW_SCENARIO {
        SLOW_MAX_WINDOW
    } else {
        DEFAULT_MAX_WINDOW
    };
    let blocks: Vec<Block> = scenario["store"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| Block::new(Bytes::from(hex::decode(s(b, "data_hex")).unwrap()), cid(b)))
        .collect();
    let deny: HashSet<Cid> = scenario["deny"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| Cid::try_from(c.as_str().unwrap()).unwrap())
        .collect();

    let mut server = server_swarm(MemStore::new(&blocks), deny);
    let server_id = *server.local_peer_id();
    server
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let addr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
            break address;
        }
    };
    let server_task = tokio::spawn(async move {
        loop {
            server.select_next_some().await;
        }
    });

    let accept: Vec<StreamProtocol> = scenario["accept_protocols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .map(|p| StreamProtocol::try_from_owned(p.to_string()).unwrap())
        .collect();
    let (mut probe, mut control, mut inbound) = probe_swarm(accept);
    let (tx, mut rx) = mpsc::unbounded_channel::<Rx>();
    let accept_task = tokio::spawn(async move {
        let mut seq = 0usize;
        while let Some((stream, protocol)) = inbound.recv().await {
            tokio::spawn(read_frames(stream, protocol.to_string(), seq, tx.clone()));
            seq += 1;
        }
    });
    probe.dial(addr).unwrap();
    loop {
        if let SwarmEvent::ConnectionEstablished { .. } = probe.select_next_some().await {
            break;
        }
    }
    let probe_task = tokio::spawn(async move {
        loop {
            probe.select_next_some().await;
        }
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut streams: HashMap<u64, libp2p::Stream> = HashMap::new();
    let mut runs: Vec<StepRun> = Vec::new();
    let steps = scenario["steps"].as_array().unwrap();
    for st in steps {
        let frame = step_frame(st);
        let stream_id = st["stream"].as_u64().unwrap();
        let protocol = s(st, "protocol");
        let mut error = None;
        let opened = if streams.contains_key(&stream_id) {
            None
        } else {
            let protocol = StreamProtocol::try_from_owned(protocol.to_string()).unwrap();
            Some(control.open_stream(server_id, protocol).await)
        };
        match opened {
            Some(Ok(stream)) => {
                streams.insert(stream_id, stream);
            }
            Some(Err(e)) => error = Some(format!("open_stream: {e}")),
            None => {}
        }
        if let Some(stream) = streams.get_mut(&stream_id) {
            let write = async {
                stream.write_all(&frame).await?;
                stream.flush().await
            };
            match tokio::time::timeout(WRITE_TIMEOUT, write).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => error = Some(format!("write: {e}")),
                Err(_) => error = Some("write: timed out".to_string()),
            }
            if st["close_after"].as_bool().unwrap() {
                let _ = stream.close().await;
                streams.remove(&stream_id);
            }
        }
        let responses = if st["wait"].as_bool().unwrap() {
            collect(&mut rx, max_window).await
        } else {
            Vec::new()
        };
        runs.push(StepRun { responses, error });
    }
    if !steps.last().unwrap()["wait"].as_bool().unwrap() {
        runs.last_mut().unwrap().responses = collect(&mut rx, max_window).await;
    }

    drop(streams);
    server_task.abort();
    probe_task.abort();
    accept_task.abort();
    runs
}
