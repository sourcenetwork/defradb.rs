//! Subscriptions to a node's event bus, read by polling.
//!
//! `defra_event_subscribe` answers with an ID (status 2) that
//! `defra_subscription_poll` and `defra_subscription_close` serve, as they
//! serve a GraphQL subscription's. Each poll yields one event as JSON.

use std::ffi::c_char;

use acp::nac::NodePermission;
use base64::Engine;
use events::{EventName, Message};
use serde_json::{json, Value};

use crate::ffi_entry;
use crate::helpers::get_rt;
use crate::nac_check::check;
use crate::state::{GraphQLSubscriptionState, GRAPHQL_SUBSCRIPTIONS, NODES};
use crate::target::{go_string, resolve_identity, resolve_node};
use crate::types::DefraResult;
use crate::{try_ffi, ERR_INVALID_NODE_HANDLE};

/// Events held for a caller that has not polled them yet. A full buffer holds
/// the forwarding back until the caller polls again; meanwhile the event bus
/// drops what its own subscriber queue cannot hold, and the next event
/// delivered is preceded by an `events-dropped` event counting them, so a
/// slow poller learns it lost events rather than losing them silently.
const EVENT_BUFFER: usize = 4096;

/// The name of the event that reports events the bus dropped before this
/// subscription could take them.
const EVENTS_DROPPED: &str = "events-dropped";

/// Every event a caller can name, under the names Go's event package uses.
const NAMEABLE: [EventName; 13] = [
    EventName::WildCard,
    EventName::Update,
    EventName::Merge,
    EventName::MergeComplete,
    EventName::ReplicatorCompleted,
    EventName::TopicPeerEvent,
    EventName::PeerConnection,
    EventName::SEArtifactReceived,
    EventName::AcpHeightAdvanced,
    EventName::AcpCacheInvalidated,
    EventName::PendingDagQuarantined,
    EventName::ActionExecution,
    EventName::UniqueIndexConflict,
];

/// Subscribes to node `node_ptr`'s events named in `names`, comma-separated
/// (`"update,merge-complete"`; `"*"` names them all), as the identity
/// `identity_ptr` names, and answers with status 2 and the subscription's
/// ID.
///
/// Each `defra_subscription_poll` of the ID yields the next event as a JSON
/// object whose `name` is the event's name, with that event's fields
/// beside it. A caller that polls too slowly loses events; the first event
/// after a loss is `{"name": "events-dropped", "count": N}`. Events carry documents without document access control, so
/// when node access control is on, only an identity allowed to bypass
/// document access control may subscribe.
///
/// # Safety
///
/// `names` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_event_subscribe(
    node_ptr: usize,
    names: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let node = try_ffi!(resolve_node(node_ptr));
        let identity = try_ffi!(resolve_identity(identity_ptr));
        let names = try_ffi!(parse_names(&go_string(names)).map_err(DefraResult::error));
        try_ffi!(check(node, identity.as_deref(), NodePermission::DacBypass));
        let runtime = try_ffi!(get_rt());

        let Some(mut subscription) = NODES.get(node, |state| state.event_bus.subscribe(&names))
        else {
            return DefraResult::error(ERR_INVALID_NODE_HANDLE);
        };
        let event_sub_id = subscription.id();
        let (sender, receiver) = kovan_channel::bounded::<String>(EVENT_BUFFER);
        let service = runtime.spawn_service(forward(subscription, sender));

        let id = GRAPHQL_SUBSCRIPTIONS.insert(GraphQLSubscriptionState {
            result_receiver: receiver,
            node_handle: node,
            event_sub_id,
            service,
        });
        DefraResult::subscription(id.to_string())
    }
}

/// Hands each event `subscription` receives to the caller's buffer as JSON,
/// first reporting any events the bus dropped since the last one delivered.
async fn forward(
    mut subscription: events::Subscription,
    sender: kovan_channel::bounded::Sender<String>,
) {
    while let Some(message) = subscription.recv().await {
        let dropped = subscription.check_and_reset_dropped();
        if dropped > 0 {
            sender
                .send_async(json!({ "name": EVENTS_DROPPED, "count": dropped }).to_string())
                .await;
        }
        sender.send_async(event_json(&message).to_string()).await;
    }
}

/// The events `names` lists, comma-separated, by their Go names.
pub(crate) fn parse_names(names: &str) -> Result<Vec<EventName>, String> {
    let mut parsed = Vec::new();
    for name in names
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        let event = NAMEABLE
            .iter()
            .find(|event| event.to_string() == name)
            .ok_or_else(|| format!("unknown event name: {name}"))?;
        parsed.push(*event);
    }
    if parsed.is_empty() {
        return Err("no event names given".to_string());
    }
    Ok(parsed)
}

/// `message` as the JSON object a poll yields.
pub(crate) fn event_json(message: &Message) -> Value {
    let mut event = fields(message);
    event["name"] = Value::String(message.name.to_string());
    event
}

/// The fields of `message`'s payload; an empty object for a bare signal.
fn fields(message: &Message) -> Value {
    if let Some(update) = message.as_update() {
        return json!({
            "doc_id": update.doc_id,
            "cid": update.cid.to_string(),
            "collection_id": update.collection_id,
            "block": base64::engine::general_purpose::STANDARD.encode(&update.block),
            "is_retry": update.is_retry,
            "is_relay": update.is_relay,
        });
    }
    if let Some(merge) = message.as_merge_complete() {
        return json!({
            "doc_id": merge.doc_id,
            "cid": merge.cid.to_string(),
            "collection_id": merge.collection_id,
            "by_peer": merge.by_peer,
        });
    }
    if let Some(completed) = message.as_replicator_completed() {
        return json!({
            "peer_id": completed.peer_id,
            "collections": completed.collections,
            "skipped": completed.skipped,
            "error": completed.error,
        });
    }
    if let Some(topic) = message.as_topic_peer_event() {
        return json!({
            "peer_id": topic.peer_id,
            "topic": topic.topic,
            "event_type": topic.event_type,
        });
    }
    if let Some(connection) = message.as_peer_connection() {
        return json!({
            "peer_id": connection.peer_id,
            "connected": connection.connected,
        });
    }
    if let Some(artifact) = message.as_se_artifact_received() {
        return json!({ "doc_id": artifact.doc_id });
    }
    if let Some(execution) = message.as_action_execution() {
        return json!({ "action_execution": execution });
    }
    if let Some(quarantined) = message.as_pending_dag_quarantined() {
        return json!({
            "cid": quarantined.cid.to_string(),
            "doc_id": quarantined.doc_id,
            "collection_id": quarantined.collection_id,
            "reason": quarantined.reason,
        });
    }
    if let Some(conflict) = message.as_unique_index_conflict() {
        return json!({
            "collection_id": conflict.collection_id,
            "index": conflict.index,
            "winner_doc_id": conflict.winner_doc_id,
            "unindexed_doc_id": conflict.unindexed_doc_id,
        });
    }
    if let Some(height) = message.as_acp_height_advanced() {
        return json!({
            "height": height.height,
            "module_state_root": height.module_state_root,
        });
    }
    if let Some(invalidated) = message.as_acp_cache_invalidated() {
        return json!({
            "height": invalidated.height,
            "module_state_root": invalidated.module_state_root,
            "previous_root": invalidated.previous_root,
            "entries_invalidated": invalidated.entries_invalidated,
        });
    }
    json!({})
}

#[cfg(test)]
mod tests;
