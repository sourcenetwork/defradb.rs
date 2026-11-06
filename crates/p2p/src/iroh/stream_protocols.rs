//! Stream protocols an embedder serves on the node's connections beside
//! Defra's own.
//!
//! Registration checks are proved in `proofs/lean/StreamProtocols`; the
//! lifecycle of a served stream is modeled in `proofs/tla/EmbedderStream.tla`.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use defra_core::thread_bounds::MaybeSendSync;

use super::inbound_budget::{peer_share_bytes, InboundLease};
use super::protocol_stream::{ProtocolRecvStream, ProtocolSendStream};
use super::protocols::MAX_STREAM_TAG_LEN;
use super::services::{self, Priority, Service};
use crate::error::{Error, Result};
use crate::transport::PeerId;

/// Every Defra stream tag starts with this, so a tag Defra adds later cannot
/// take an embedder's.
pub const RESERVED_TAG_PREFIX: &[u8] = b"/defra-iroh/";

/// The longest [`StreamProtocol::deadline`] accepted. A stream served for
/// longer than a day has, for every purpose, no deadline.
pub const MAX_STREAM_DEADLINE: Duration = Duration::from_secs(24 * 60 * 60);

/// A protocol an embedder serves on the node's iroh connections.
///
/// The opener writes the tag and one length-prefixed request. The serving
/// node admits the request as it admits Defra's own messages (the inbound
/// allowlist, revocations, the priority's budget and the peer's share of it,
/// the size limit and the body deadlines), and hands the stream to the
/// protocol's [`StreamHandler`]. The request's room in the budget goes with it
/// ([`ProtocolRequest`]) and comes back once the handler drops the request.
/// What follows on the stream is up to the protocol.
///
/// Put a version in the tag (`/myapp/pull/1`): a peer that does not serve a
/// tag resets the stream with code 2, so a new version is a new tag served
/// beside the old one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamProtocol {
    /// Names the protocol in a stream's first frame: 1 to 64 bytes, not
    /// starting with [`RESERVED_TAG_PREFIX`].
    pub tag: &'static [u8],
    /// The inbound budget the request is admitted under, and the priority
    /// the stream is sent at both ways.
    pub priority: Priority,
    /// The largest request accepted, in bytes: at most one peer's share of
    /// the priority's inbound budget.
    pub max_message: usize,
    /// How long the serving node gives its handler for one stream, counted
    /// from the request's admission: more than zero, at most
    /// [`MAX_STREAM_DEADLINE`]. A handler still running then is dropped and
    /// both halves of its stream are reset.
    pub deadline: Duration,
}

impl StreamProtocol {
    /// Refuses a protocol no node could serve as declared.
    pub fn validate(&self) -> Result<()> {
        let tag = String::from_utf8_lossy(self.tag);
        let refuse = |reason: String| {
            Err(Error::InvalidConfig(format!(
                "stream protocol {tag:?} {reason}"
            )))
        };
        if self.tag.is_empty() || self.tag.len() > MAX_STREAM_TAG_LEN {
            return refuse(format!(
                "has a tag of {} bytes; a tag is 1 to {MAX_STREAM_TAG_LEN} bytes",
                self.tag.len()
            ));
        }
        if self.tag.starts_with(RESERVED_TAG_PREFIX) || services::lookup(self.tag).is_some() {
            return refuse(format!(
                "is in Defra's reserved {} namespace",
                String::from_utf8_lossy(RESERVED_TAG_PREFIX)
            ));
        }
        let share = peer_share_bytes(self.priority);
        if self.max_message > share {
            return refuse(format!(
                "accepts requests of up to {} bytes, but one peer may hold at most {share} bytes at \
                 {} priority",
                self.max_message,
                self.priority.name()
            ));
        }
        if self.deadline.is_zero() || self.deadline > MAX_STREAM_DEADLINE {
            return refuse(format!(
                "has a deadline of {:?}; a deadline is more than zero and at most {:?}",
                self.deadline, MAX_STREAM_DEADLINE
            ));
        }
        Ok(())
    }

    fn service(&self) -> Service {
        Service {
            tag: self.tag,
            priority: self.priority,
            max_message: self.max_message,
            solicited: false,
        }
    }
}

/// A protocol stream's first message, read and admitted. It holds the
/// request's room in the peer's inbound budget until it is dropped, so the
/// budget counts these bytes for exactly as long as the handler keeps them:
/// parse what you need and drop it before a long wait.
pub struct ProtocolRequest {
    bytes: Vec<u8>,
    _room: InboundLease,
}

impl ProtocolRequest {
    pub(super) fn new(bytes: Vec<u8>, room: InboundLease) -> Self {
        Self { bytes, _room: room }
    }
}

impl std::ops::Deref for ProtocolRequest {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}

/// A stream a peer opened for a registered [`StreamProtocol`], its request
/// already read and admitted.
#[non_exhaustive]
pub struct InboundStream {
    /// The peer that opened the stream, authenticated by its connection.
    pub peer: PeerId,
    /// The stream's first message, holding its room in the budget.
    pub request: ProtocolRequest,
    /// When the node drops the handler and resets the stream.
    pub deadline: web_time::Instant,
    /// The send half, for the reply.
    pub send: ProtocolSendStream,
    /// The receive half, past the request.
    pub recv: ProtocolRecvStream,
}

/// Serves the streams peers open for one [`StreamProtocol`].
///
/// Each stream is served on a task of its own at the network edge. Only its
/// [`ProtocolRequest`] holds room in the inbound budget, so a handler that
/// drops the request before waiting holds none. The node drops that task, resetting both
/// halves of the stream, at the protocol's deadline, when the peer's
/// connection closes (a revocation with `deny_peer` closes it), or when the
/// endpoint shuts down. A peer has at most 64 streams open per connection,
/// Defra's own included.
///
/// The handler runs on the network runtime: database work goes through the
/// node's request path (`defra_node::Node::execute_request`, which runs it on a
/// database worker), never inline on this task. Anything that blocks here
/// blocks the network.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait StreamHandler: MaybeSendSync {
    /// Serves one stream.
    async fn handle(&self, stream: InboundStream);
}

/// A protocol and the handler serving it.
pub type StreamRegistration = (StreamProtocol, Arc<dyn StreamHandler>);

pub(super) struct Registered {
    pub(super) protocol: StreamProtocol,
    pub(super) service: Service,
    pub(super) handler: Arc<dyn StreamHandler>,
}

/// The protocols an endpoint serves besides Defra's own: fixed when the
/// endpoint starts, one entry per tag.
#[derive(Clone, Default)]
pub(super) struct StreamProtocols(Arc<[Registered]>);

impl StreamProtocols {
    /// Refuses an invalid protocol and a tag registered twice.
    pub(super) fn new(protocols: Vec<StreamRegistration>) -> Result<Self> {
        let mut registered: Vec<Registered> = Vec::with_capacity(protocols.len());
        for (protocol, handler) in protocols {
            protocol.validate()?;
            if registered.iter().any(|r| r.protocol.tag == protocol.tag) {
                return Err(Error::InvalidConfig(format!(
                    "stream protocol {:?} is registered twice",
                    String::from_utf8_lossy(protocol.tag)
                )));
            }
            registered.push(Registered {
                service: protocol.service(),
                protocol,
                handler,
            });
        }
        Ok(Self(registered.into()))
    }

    // ponytail: linear scan, a handful of protocols; a map if embedders register dozens.
    pub(super) fn get(&self, tag: &[u8]) -> Option<&Registered> {
        self.0
            .iter()
            .find(|registered| registered.protocol.tag == tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iroh::services::SERVICES;

    struct Nothing;

    #[async_trait]
    impl StreamHandler for Nothing {
        async fn handle(&self, _stream: InboundStream) {}
    }

    fn protocol(tag: &'static [u8]) -> StreamProtocol {
        StreamProtocol {
            tag,
            priority: Priority::Sync,
            max_message: 1024,
            deadline: Duration::from_secs(1),
        }
    }

    fn register(protocols: &[StreamProtocol]) -> Result<StreamProtocols> {
        StreamProtocols::new(
            protocols
                .iter()
                .map(|p| (*p, Arc::new(Nothing) as Arc<dyn StreamHandler>))
                .collect(),
        )
    }

    #[test]
    #[mantra_macros::req_test("defra.stream_protocols.registration")]
    fn defra_tags_and_the_reserved_prefix_are_refused() {
        for service in SERVICES {
            assert!(protocol(service.tag).validate().is_err());
        }
        assert!(protocol(b"/defra-iroh/new/1").validate().is_err());
        assert!(protocol(b"/defra-iroh").validate().is_ok());
    }

    #[test]
    #[mantra_macros::req_test("defra.stream_protocols.registration")]
    fn a_tag_outside_one_to_64_bytes_is_refused() {
        const LONGEST: &[u8; 64] = &[b'a'; 64];
        const TOO_LONG: &[u8; 65] = &[b'a'; 65];
        assert!(protocol(b"").validate().is_err());
        assert!(protocol(b"a").validate().is_ok());
        assert!(protocol(LONGEST).validate().is_ok());
        assert!(protocol(TOO_LONG).validate().is_err());
    }

    #[test]
    #[mantra_macros::req_test("defra.stream_protocols.registration")]
    fn a_limit_above_a_peers_share_is_refused() {
        let share = peer_share_bytes(Priority::Sync);
        let at = StreamProtocol {
            max_message: share,
            ..protocol(b"/test/large/1")
        };
        assert!(at.validate().is_ok());
        let over = StreamProtocol {
            max_message: share + 1,
            ..at
        };
        assert!(over.validate().is_err());
    }

    #[test]
    #[mantra_macros::req_test("defra.stream_protocols.registration")]
    fn a_zero_or_unbounded_deadline_is_refused() {
        let zero = StreamProtocol {
            deadline: Duration::ZERO,
            ..protocol(b"/test/1")
        };
        assert!(zero.validate().is_err());
        let longest = StreamProtocol {
            deadline: MAX_STREAM_DEADLINE,
            ..zero
        };
        assert!(longest.validate().is_ok());
        let over = StreamProtocol {
            deadline: MAX_STREAM_DEADLINE + Duration::from_secs(1),
            ..zero
        };
        assert!(over.validate().is_err());
    }

    #[test]
    #[mantra_macros::req_test("defra.stream_protocols.registration")]
    fn a_tag_registered_twice_is_refused_and_each_tag_finds_its_own() {
        assert!(register(&[protocol(b"/a/1"), protocol(b"/a/1")]).is_err());
        let table = register(&[protocol(b"/a/1"), protocol(b"/b/1")]).unwrap();
        assert_eq!(table.get(b"/b/1").unwrap().protocol.tag, b"/b/1");
        assert!(table.get(b"/c/1").is_none());
        assert!(register(&[protocol(b"/a/1"), protocol(b"")]).is_err());
    }
}
