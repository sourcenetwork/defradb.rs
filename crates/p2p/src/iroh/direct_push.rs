//! An update too large for gossip, pushed to the topic's neighbours on the
//! PushLog stream instead.
//!
//! iroh-gossip drops a broadcast over its budget, so a block bigger than that
//! would reach no subscriber. The neighbours gossip would have sent it to get
//! it as a direct PushLog request, which a node accepts for any collection it
//! subscribes to. Modeled in `proofs/tla/GossipDelivery.tla`.

use futures::future::join_all;

use crate::error::{Error, Result};
use crate::message::{PushLogBroadcast, PushLogReply, PushLogRequest};
use crate::topics::DefraTopic;
use crate::transport::{MessageId, PeerId};

use super::endpoint::{peer_direct_addr, EndpointResources};
use super::endpoint_rpc::handle_request_response;
use super::gossip::GossipHandle;
use super::protocols::STREAM_PUSHLOG;

/// The neighbours on `topic` to push `msg` to. A document topic leaves out
/// the collection topic's neighbours, which its publish reaches.
pub(super) fn peers(gossip: &GossipHandle, topic: &str, msg: &PushLogBroadcast) -> Vec<PeerId> {
    let peers = gossip.topic_peers(topic);
    let collection = DefraTopic::collection(msg.collection_id.as_str()).to_string();
    if topic == collection {
        return peers;
    }
    let reached = gossip.topic_peers(&collection);
    peers
        .into_iter()
        .filter(|peer| !reached.contains(peer))
        .collect()
}

/// Pushes `msg` to every peer in `peers`, and answers once each one replied:
/// an error names every peer that did not take it.
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
    let replies = join_all(peers.iter().map(|peer| push_one(resources, peer, &request))).await;
    let failed: Vec<String> = peers
        .iter()
        .zip(replies)
        .filter_map(|(peer, reply)| reply.err().map(|error| format!("{peer}: {error}")))
        .collect();
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
        Some(error) => Err(Error::GossipSubPublish(error)),
        None => Ok(()),
    }
}

#[cfg(test)]
#[path = "../../tests/iroh/direct_push.rs"]
mod tests;
