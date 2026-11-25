//! Commands from the `PincTransport` facade to the endpoint's thread.

use cid::Cid;

use p2p::error::Result;
use p2p::message::CarFetchRequest;
use p2p::rt::{mpsc, oneshot};
use p2p::transport::{PeerAddr, PeerConnectionInfo, PeerId};
use p2p::QueryId;

use super::admission::AdmissionAuthority;
use super::protocol_stream::{ProtocolRecvStream, ProtocolSendStream};
use super::stream_protocols::StreamProtocol;

pub type Reply<T> = oneshot::Sender<Result<T>>;

/// Resolves, as an error, once the endpoint task has stopped.
pub type EndpointStopped = futures::future::Shared<oneshot::Receiver<()>>;

/// How a facade reaches its endpoint.
pub type CommandSender = (mpsc::Sender<Command>, EndpointStopped);

/// What a reply on a request's stream carries.
#[derive(Debug)]
pub enum ReplyBody {
    /// A message: length-prefixed, as every request is.
    Framed(Vec<u8>),
    /// Raw CAR bytes up to the stream's end.
    Raw(Vec<u8>),
}

/// An outbound request and what is done with its reply.
#[derive(Debug)]
pub enum Outbound {
    /// The decoded reply is the answer.
    Ask,
    /// The request is all; the peer only finishes its side.
    Tell,
    /// The reply becomes a transport event, made from the reply's bytes. The
    /// caller is answered once the request is written when
    /// `answer_when_sent`, else once the event is queued.
    Event {
        to_event: ToEvent,
        answer_when_sent: bool,
    },
}

/// Builds the event a reply's bytes become, checking what it must.
pub type ToEvent = fn(PeerId, Vec<u8>) -> Result<EventOut>;

/// An event built from a reply, ready for the dispatch queue.
pub type EventOut = p2p::transport::TransportEvent<super::PincResponseToken>;

#[non_exhaustive]
pub enum Command {
    /// Open a stream for an embedder protocol to a peer and write its request.
    OpenStream {
        peer_id: PeerId,
        addrs: Vec<PeerAddr>,
        protocol: StreamProtocol,
        request: Vec<u8>,
        reply: Reply<(ProtocolSendStream, ProtocolRecvStream)>,
    },
    Dial {
        peer_id: PeerId,
        addrs: Vec<PeerAddr>,
        reply: Reply<()>,
    },
    Disconnect {
        peer_id: PeerId,
        reply: Reply<()>,
    },
    AllowPeer {
        peer_id: PeerId,
        authority: AdmissionAuthority,
        reply: Reply<()>,
    },
    DenyPeer {
        peer_id: PeerId,
        reply: Reply<()>,
    },
    IsPeerRevoked {
        peer_id: PeerId,
        reply: Reply<bool>,
    },
    ConnectedPeers {
        reply: Reply<Vec<PeerId>>,
    },
    /// Resolves once `peer_id` holds a connection; dropped when the reply's
    /// receiver goes.
    WaitConnected {
        peer_id: PeerId,
        reply: Reply<()>,
    },
    PeerConnection {
        peer_id: PeerId,
        reply: Reply<Option<PeerConnectionInfo>>,
    },
    InboundBudget {
        reply: Reply<Vec<p2p::transport::InboundClassSnapshot>>,
    },
    ListenAddresses {
        reply: Reply<Vec<PeerAddr>>,
    },
    PeerAddresses {
        reply: Reply<Vec<String>>,
    },
    NetworkChange {
        reply: Reply<()>,
    },
    SetServicePointer {
        pointer: [u8; super::SERVICE_POINTER_LEN],
        reply: Reply<()>,
    },
    ServicePointer {
        peer_id: PeerId,
        reply: Reply<Option<Vec<u8>>>,
    },
    /// Open a `tag` stream to `peer_id`, write `body`, and handle the reply
    /// as `outbound` says.
    Request {
        peer_id: PeerId,
        tag: &'static [u8],
        body: Vec<u8>,
        outbound: Outbound,
        reply: Reply<Vec<u8>>,
    },
    /// Write `body` on the stream `id` names, and end it.
    Reply {
        id: u64,
        body: ReplyBody,
        reply: Reply<()>,
    },
    /// The token for `id` was dropped unspent: reset its stream.
    DropReply {
        id: u64,
    },
    SendCarRequest {
        peer_id: PeerId,
        root_cid: Cid,
        haves: Vec<Cid>,
        reply: Reply<()>,
    },
    SyncBlocks {
        request: CarFetchRequest,
        providers: Vec<PeerId>,
        reply: Reply<QueryId>,
    },
    CancelSync {
        query_id: QueryId,
        reply: Reply<bool>,
    },
    /// Join `topic`. A `raw` topic is marked raw before the join, so none of
    /// its messages is ever decoded as a PushLog.
    Subscribe {
        topic: String,
        raw: bool,
        reply: Reply<bool>,
    },
    Unsubscribe {
        topic: String,
        reply: Reply<bool>,
    },
    /// Route `topic`'s messages as raw, without joining it.
    RegisterRawTopic {
        topic: String,
        reply: Reply<()>,
    },
    /// Publish `payload` on `topic`. With `wait`, first wait briefly for a
    /// member of the topic: a KMS reply must not reach nobody.
    Publish {
        topic: String,
        payload: Vec<u8>,
        wait: bool,
        reply: Reply<p2p::transport::MessageId>,
    },
    TopicPeers {
        topic: String,
        reply: Reply<Vec<PeerId>>,
    },
    Shutdown {
        reply: Reply<()>,
    },
}
