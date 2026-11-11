//! Commands from the `VincTransport` facade to the endpoint's thread.

use cid::Cid;

use crate::error::Result;
use crate::message::CarFetchRequest;
use crate::rt::{mpsc, oneshot};
use crate::transport::{PeerAddr, PeerConnectionInfo, PeerId};
use crate::QueryId;

use super::admission::AdmissionAuthority;

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
pub type EventOut = crate::transport::TransportEvent<super::VincResponseToken>;

#[non_exhaustive]
pub enum Command {
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
        reply: Reply<Vec<crate::transport::InboundClassSnapshot>>,
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
    Shutdown {
        reply: Reply<()>,
    },
}
