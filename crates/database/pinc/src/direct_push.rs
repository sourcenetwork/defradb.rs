//! An update too large for one topic message, pushed on the PushLog stream
//! instead.
//!
//! A hub refuses a topic message over [`MAX_TOPIC_MESSAGE`], so a head block
//! bigger than that would reach no subscriber. The peers a small update
//! reaches are the connected peers subscribed to its collection or following
//! its document. The large one goes to every connected peer as a signed
//! direct PushLog request, which a node accepts for any collection it
//! subscribes to or document it follows, and refuses as access denied
//! otherwise (`handle_pushlog_request`).
//!
//! [`MAX_TOPIC_MESSAGE`]: super::hub::MAX_TOPIC_MESSAGE

use futures::stream::{self, StreamExt};
use tracing::debug;

use p2p::error::{Error, Result};
use p2p::message::{PushLogBroadcast, PushLogReply, PushLogRequest};
use p2p::signing::sign_with_transport;
use p2p::topics::DefraTopic;
use p2p::transport::{MessageId, P2PTransport, PeerId};

use super::command::Outbound;
use super::services;
use super::transport::PincTransport;
use super::wire;

/// Direct pushes of one update in flight at once.
const FANOUT: usize = 8;

/// How a peer that neither subscribes to the collection nor follows the
/// document answers a push.
const NOT_SUBSCRIBED: &str = "access denied:";

impl PincTransport {
    /// Push `msg`, whose broadcast is `size` bytes, to every connected peer
    /// rather than publishing it on `topic`. Its collection topic's publish
    /// pushes; its document topic's sends nothing, since that one push
    /// already reaches every connected peer, and a peer that follows only the
    /// document takes it too. Answers once each peer replied: an error names
    /// every peer that took the push as its own and failed it.
    pub(super) async fn push_too_large(
        &self,
        topic: &DefraTopic,
        msg: &PushLogBroadcast,
        size: usize,
    ) -> Result<MessageId> {
        let mut request = PushLogRequest::new(
            msg.doc_id.clone(),
            msg.cid.clone(),
            msg.collection_id.clone(),
            msg.creator.clone(),
            msg.block.clone(),
        );
        request.message_id = uuid::Uuid::new_v4().to_string();
        if *topic != DefraTopic::collection(&msg.collection_id) {
            return Ok(MessageId::new(request.message_id));
        }
        request.supports_same_stream_reply = true;
        sign_with_transport(self, &mut request)?;
        let peers = self.connected_peers().await?;
        debug!(
            doc_id = %msg.doc_id,
            size,
            peers = peers.len(),
            "pinc broadcast larger than one topic message: pushing it directly"
        );
        let body = wire::encode(&request)?;
        let body = &body;
        let pushes: Vec<_> = peers
            .iter()
            .map(|peer| async move { (peer, self.push_one(peer, body.clone()).await) })
            .collect();
        let replies: Vec<_> = stream::iter(pushes)
            .buffer_unordered(FANOUT)
            .collect()
            .await;
        let failed: Vec<String> = replies
            .into_iter()
            .filter_map(|(peer, reply)| reply.err().map(|error| format!("{peer}: {error}")))
            .collect();
        if failed.is_empty() {
            return Ok(MessageId::new(request.message_id));
        }
        Err(Error::GossipSubPublish(format!(
            "direct push of a {size} byte broadcast failed for {} of {} peers: {}",
            failed.len(),
            peers.len(),
            failed.join("; ")
        )))
    }

    /// Push to one peer. A peer that does not subscribe to the collection is
    /// not owed the update, so its refusal is no failure.
    async fn push_one(&self, peer: &PeerId, body: Vec<u8>) -> Result<()> {
        let bytes = self
            .request(peer, services::STREAM_PUSHLOG, body, Outbound::Ask)
            .await?;
        let reply: PushLogReply = wire::decode(bytes)?;
        match reply.err_message {
            Some(error) if error.starts_with(NOT_SUBSCRIBED) => Ok(()),
            Some(error) => Err(Error::GossipSubPublish(error)),
            None => Ok(()),
        }
    }
}
