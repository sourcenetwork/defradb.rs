//! An update too large for gossip, pushed on the PushLog stream instead.
//!
//! iroh-gossip drops a broadcast over its budget, so a block bigger than that
//! would reach no subscriber. A node takes a relayed gossip head only from an
//! origin it is connected to, so the peers a small update reaches are the
//! connected peers subscribed to its collection or following its document.
//! The large one goes to every connected peer as a direct PushLog request,
//! which a node accepts for any collection it subscribes to or document it
//! follows, and refuses as access denied otherwise.
//! Modeled in `proofs/tla/GossipDelivery.tla`.

use futures::future::join_all;

use crate::error::{Error, Result};
use crate::message::{PushLogBroadcast, PushLogReply, PushLogRequest};
use crate::topics::DefraTopic;
use crate::transport::{MessageId, PeerId};

use super::endpoint::{peer_direct_addr, EndpointResources};
use super::endpoint_rpc::handle_request_response;
use super::protocols::STREAM_PUSHLOG;

/// Direct pushes of one update in flight at once.
const FANOUT: usize = 8;

/// How a peer that neither subscribes to the collection nor follows the
/// document answers a push (`handle_pushlog_request`).
const NOT_SUBSCRIBED: &str = "access denied:";

/// The peers to push `msg` to. Its collection topic's publish goes to every
/// connected peer; its document topic's goes to none, since that one push
/// already reaches every connected peer, and a peer that follows only the
/// document accepts it too.
pub(super) fn peers(
    resources: &EndpointResources,
    topic: &str,
    msg: &PushLogBroadcast,
) -> Vec<PeerId> {
    if topic != DefraTopic::collection(msg.collection_id.as_str()).to_string() {
        return Vec::new();
    }
    resources.peer_map.connected_peers()
}

/// Pushes `msg` to every peer in `peers`, at most `FANOUT` at once, and answers
/// once each one replied: an error names every peer that subscribes to the
/// collection and did not take it.
pub(super) async fn push(
    resources: &EndpointResources,
    peers: Vec<PeerId>,
    msg: &PushLogBroadcast,
) -> Result<MessageId> {
    let mut request = PushLogRequest::new(
        msg.doc_id.clone(),
        msg.cid.clone(),
        msg.collection_id.clone(),
        msg.creator.clone(),
        msg.block.clone(),
    );
    request.message_id = uuid::Uuid::new_v4().to_string();
    request.supports_same_stream_reply = true;
    let mut failed = Vec::new();
    // ponytail: waves of FANOUT, so one slow peer holds its wave; a rolling window if that shows.
    for wave in peers.chunks(FANOUT) {
        let replies = join_all(wave.iter().map(|peer| push_one(resources, peer, &request))).await;
        failed.extend(
            wave.iter()
                .zip(replies)
                .filter_map(|(peer, reply)| reply.err().map(|error| format!("{peer}: {error}"))),
        );
    }
    if failed.is_empty() {
        return Ok(MessageId::new(request.message_id));
    }
    Err(Error::GossipSubPublish(format!(
        "direct push of a {} byte block failed for {} of {} peers: {}",
        msg.block.len(),
        failed.len(),
        peers.len(),
        failed.join("; ")
    )))
}

/// Pushes to one peer. A peer that does not subscribe to the collection is
/// not owed the update, so its refusal is no failure.
async fn push_one(
    resources: &EndpointResources,
    peer: &PeerId,
    request: &PushLogRequest,
) -> Result<()> {
    let reply: PushLogReply = handle_request_response(
        &resources.endpoint,
        peer,
        STREAM_PUSHLOG,
        request,
        peer_direct_addr(&resources.peer_map, peer),
        &resources.connection_cache,
        &resources.admission,
    )
    .await?;
    match reply.err_message {
        Some(error) if error.starts_with(NOT_SUBSCRIBED) => Ok(()),
        Some(error) => Err(Error::GossipSubPublish(error)),
        None => Ok(()),
    }
}

#[cfg(test)]
#[path = "../../tests/iroh/direct_push.rs"]
mod tests;
