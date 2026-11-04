//! Stream protocols an embedder serves on the node's connections beside
//! Defra's own.

use std::sync::Arc;

use async_trait::async_trait;
use defra_core::thread_bounds::MaybeSendSync;
use iroh::endpoint::{RecvStream, SendStream};
use iroh::EndpointId;

use super::inbound_budget::peer_share_bytes;
use super::services::{Priority, Service};
use crate::error::{Error, Result};

/// Every Defra stream tag starts with this, so a tag Defra adds later cannot
/// take an embedder's.
const RESERVED_TAG_PREFIX: &str = "/defra-iroh/";

/// A protocol an embedder serves on the node's connections.
///
/// The opener writes the tag and one length-prefixed request. The serving
/// node admits the request as it admits Defra's own messages, then hands the
/// stream to the protocol's [`StreamHandler`]. What follows on the stream is
/// up to the protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamProtocol {
    /// Names the protocol in a stream's first frame: 1 to 64 bytes, not
    /// starting with `/defra-iroh/`.
    pub tag: &'static [u8],
    /// The inbound budget the request is admitted under, and the priority
    /// the stream is sent at both ways.
    pub priority: Priority,
    /// The largest request accepted, in bytes: at most half of the
    /// priority's inbound budget, the most one peer may hold.
    pub max_message: usize,
}

impl StreamProtocol {
    /// The service admission reads this protocol's requests under. Refuses a
    /// tag Defra's own would shadow, and a request limit above one peer's
    /// share of the priority's budget, which so large a request never gets.
    fn service(self) -> Result<Service> {
        let tag = String::from_utf8_lossy(self.tag);
        if self.tag.starts_with(RESERVED_TAG_PREFIX.as_bytes()) {
            return Err(Error::InvalidConfig(format!(
                "stream tag {tag:?} is in Defra's reserved {RESERVED_TAG_PREFIX} namespace"
            )));
        }
        let share = peer_share_bytes(self.priority);
        if self.max_message > share {
            return Err(Error::InvalidConfig(format!(
                "stream protocol {tag:?} accepts requests of up to {} bytes, but one peer may \
                 hold at most {share} bytes at {} priority",
                self.max_message,
                self.priority.name()
            )));
        }
        Ok(Service {
            tag: self.tag,
            priority: self.priority,
            max_message: self.max_message,
            solicited: false,
        })
    }
}

/// A stream a peer opened for a registered [`StreamProtocol`], its request
/// already read.
#[non_exhaustive]
pub struct InboundStream {
    /// The peer that opened the stream, authenticated by its connection.
    pub peer: EndpointId,
    /// The stream's first message.
    pub request: Vec<u8>,
    /// The send half, for the reply. Dropping it finishes the stream with
    /// what was written, so reset it to refuse the request.
    pub send: SendStream,
    /// The receive half, past the request.
    pub recv: RecvStream,
}

/// Serves the streams peers open for one [`StreamProtocol`].
///
/// Each stream is handled on its own task, which holds the request's room in
/// the inbound budget until [`Self::handle`] returns and is cancelled when
/// the endpoint shuts down.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait StreamHandler: MaybeSendSync {
    /// Serves one stream.
    async fn handle(&self, stream: InboundStream);
}

struct Registered {
    service: Service,
    handler: Arc<dyn StreamHandler>,
}

/// The protocols an endpoint serves besides Defra's own.
#[derive(Clone, Default)]
pub(super) struct StreamProtocols(Arc<[Registered]>);

impl StreamProtocols {
    pub(super) fn new(protocols: Vec<(StreamProtocol, Arc<dyn StreamHandler>)>) -> Result<Self> {
        protocols
            .into_iter()
            .map(|(protocol, handler)| {
                Ok(Registered {
                    service: protocol.service()?,
                    handler,
                })
            })
            .collect::<Result<_>>()
            .map(Self)
    }

    pub(super) fn service(&self, tag: &[u8]) -> Option<&Service> {
        self.get(tag).map(|registered| &registered.service)
    }

    pub(super) fn handler(&self, tag: &[u8]) -> Option<&Arc<dyn StreamHandler>> {
        self.get(tag).map(|registered| &registered.handler)
    }

    fn get(&self, tag: &[u8]) -> Option<&Registered> {
        self.0
            .iter()
            .find(|registered| registered.service.tag == tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iroh::services::SERVICES;

    fn protocol(tag: &'static [u8], max_message: usize) -> StreamProtocol {
        StreamProtocol {
            tag,
            priority: Priority::Sync,
            max_message,
        }
    }

    #[test]
    fn defra_tags_and_limits_above_a_peers_share_are_refused() {
        for service in SERVICES {
            assert!(protocol(service.tag, 1).service().is_err());
        }
        let share = peer_share_bytes(Priority::Sync);
        assert!(protocol(b"/test/large/0.1", share).service().is_ok());
        assert!(protocol(b"/test/large/0.1", share + 1).service().is_err());
    }
}
