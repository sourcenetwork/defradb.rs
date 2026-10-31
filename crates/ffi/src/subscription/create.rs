use std::ffi::c_char;

use base64::Engine;

use crate::ffi_entry;
use crate::state::{SubscriptionState, NODES, SUBSCRIPTIONS};
use crate::types::c_str_to_string;
use crate::ERR_INVALID_NODE_HANDLE;

use super::CreateSubscriptionResult;

/// Create a subscription to database events.
///
/// # Arguments
///
/// * `node_ptr` - Handle to the node
/// * `collection_filter` - Optional collection name to filter events (null for all)
///
/// # Returns
///
/// A handle that can be used with `poll_subscription` and `close_subscription`.
///
/// # Safety
///
/// The collection_filter must be either null or a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn create_subscription(
    node_ptr: usize,
    collection_filter: *const c_char,
) -> CreateSubscriptionResult {
    ffi_entry! {
        let collection = c_str_to_string(collection_filter);

        // Get the event bus from the node
        let subscription = match NODES.get(node_ptr, |state| {
            // Subscribe to Update events (document changes)
            state.event_bus.subscribe(&[events::EventName::Update])
        }) {
            Some(sub) => sub,
            None => return CreateSubscriptionResult::error(ERR_INVALID_NODE_HANDLE),
        };

        // Create subscription state with optional collection filter
        let state = SubscriptionState {
            subscription,
            node_handle: node_ptr,
            collection_filter: collection,
        };

        // Register and return handle
        let handle = SUBSCRIPTIONS.insert(state);
        CreateSubscriptionResult::success(handle)
    }
}

/// Create a subscription to P2P merge complete events.
///
/// # Arguments
///
/// * `node_ptr` - Handle to the node
///
/// # Returns
///
/// A handle that can be used with `poll_subscription` and `close_subscription`.
/// Events will contain merge complete data (doc_id, cid, collection_id, by_peer).
#[no_mangle]
pub extern "C" fn create_merge_complete_subscription(node_ptr: usize) -> CreateSubscriptionResult {
    ffi_entry! {
        // Get the event bus from the node
        let subscription = match NODES.get(node_ptr, |state| {
            state.event_bus.subscribe(&[
                events::EventName::Update,
                events::EventName::MergeComplete,
                events::EventName::ReplicatorCompleted,
                events::EventName::TopicPeerEvent,
                events::EventName::SEArtifactReceived,
                events::EventName::ActionExecution,
            ])
        }) {
            Some(sub) => sub,
            None => return CreateSubscriptionResult::error(ERR_INVALID_NODE_HANDLE),
        };

        let state = SubscriptionState {
            subscription,
            node_handle: node_ptr,
            collection_filter: None,
        };

        let handle = SUBSCRIPTIONS.insert(state);
        CreateSubscriptionResult::success(handle)
    }
}

/// Create a subscription to the events `names_json` names, a JSON array of
/// Go DefraDB's event names (`"update"`, `"p2p-no-peers"`, `"merge"`,
/// `"merge-complete"`, …; `"*"` for every event).
///
/// # Returns
///
/// A handle for `poll_subscription` and `close_subscription`; an unknown
/// name is an error.
///
/// # Safety
///
/// `names_json` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn create_event_subscription(
    node_ptr: usize,
    names_json: *const c_char,
) -> CreateSubscriptionResult {
    ffi_entry! {
        let Some(names) = c_str_to_string(names_json) else {
            return CreateSubscriptionResult::error("names_json is null");
        };
        let names: Vec<String> = match serde_json::from_str(&names) {
            Ok(names) => names,
            Err(error) => return CreateSubscriptionResult::error(format!("invalid names_json: {error}")),
        };
        let mut events = Vec::with_capacity(names.len());
        for name in &names {
            match event_name(name) {
                Some(event) => events.push(event),
                None => return CreateSubscriptionResult::error(format!("unknown event name: {name}")),
            }
        }
        let subscription = match NODES.get(node_ptr, |state| state.event_bus.subscribe(&events)) {
            Some(sub) => sub,
            None => return CreateSubscriptionResult::error(ERR_INVALID_NODE_HANDLE),
        };
        let state = SubscriptionState {
            subscription,
            node_handle: node_ptr,
            collection_filter: None,
        };
        CreateSubscriptionResult::success(SUBSCRIPTIONS.insert(state))
    }
}

/// The event `name` (its display name, as Go DefraDB names it) names.
fn event_name(name: &str) -> Option<events::EventName> {
    use events::EventName::*;
    [
        WildCard,
        Update,
        Merge,
        MergeComplete,
        ReplicatorCompleted,
        TopicPeerEvent,
        PeerConnection,
        SEArtifactReceived,
        AcpHeightAdvanced,
        AcpCacheInvalidated,
        PendingDagQuarantined,
        ActionExecution,
        UniqueIndexConflict,
        P2PNoPeers,
        P2PGossipHeadDropped,
    ]
    .into_iter()
    .find(|event| event.to_string() == name)
}

/// Convert an event message to JSON.
///
/// Update-event `block` bytes use standard padded base64. This is the encoding
/// consumed by the Go rust-FFI bridge; it intentionally differs from the HTTP
/// event surface's hex representation.
pub(crate) fn message_to_json(message: &events::Message) -> String {
    // Check if this is an Update event with data
    if let Some(update) = message.as_update() {
        return serde_json::json!({
            "type": "update",
            "doc_id": update.doc_id,
            "cid": update.cid.to_string(),
            "collection_id": update.collection_id,
            "block": base64::engine::general_purpose::STANDARD.encode(&update.block),
            "is_retry": update.is_retry,
            "is_relay": update.is_relay
        })
        .to_string();
    }

    // Check if this is a MergeComplete event with data
    if let Some(mc) = message.as_merge_complete() {
        return serde_json::json!({
            "type": "merge_complete",
            "doc_id": mc.doc_id,
            "cid": mc.cid.to_string(),
            "collection_id": mc.collection_id,
            "by_peer": mc.by_peer
        })
        .to_string();
    }

    // Check if this is a ReplicatorCompleted event
    if message.name == events::EventName::ReplicatorCompleted {
        if let Some(data) = message.as_replicator_completed() {
            return serde_json::json!({
                "type": "replicator_completed",
                "peer_id": data.peer_id,
                "collections": data.collections,
                "skipped": data.skipped,
                "error": data.error,
            })
            .to_string();
        }
        return serde_json::json!({
            "type": "replicator_completed"
        })
        .to_string();
    }

    // Check if this is a TopicPeerEvent
    if let Some(tpe) = message.as_topic_peer_event() {
        return serde_json::json!({
            "type": "topic_peer_event",
            "peer_id": tpe.peer_id,
            "topic": tpe.topic,
            "event_type": tpe.event_type
        })
        .to_string();
    }

    // Check if this is an SEArtifactReceived event
    if let Some(se) = message.as_se_artifact_received() {
        return serde_json::json!({
            "type": "se_artifact_received",
            "doc_id": se.doc_id
        })
        .to_string();
    }

    if let Some(no_peers) = message.as_p2p_no_peers() {
        return serde_json::json!({
            "type": "p2p_no_peers",
            "doc_id": no_peers.doc_id,
            "collection_id": no_peers.collection_id,
            "topic": no_peers.topic,
        })
        .to_string();
    }

    if let Some(execution) = message.as_action_execution() {
        return serde_json::json!({
            "type": "action_execution",
            "action_execution": execution
        })
        .to_string();
    }

    // Signal event without data
    let event_type = match message.name {
        events::EventName::Merge => "merge",
        events::EventName::MergeComplete => "merge_complete",
        events::EventName::Update => "update",
        events::EventName::ReplicatorCompleted => "replicator_completed",
        events::EventName::TopicPeerEvent => "topic_peer_event",
        events::EventName::SEArtifactReceived => "se_artifact_received",
        events::EventName::AcpHeightAdvanced => "acp_height_advanced",
        events::EventName::AcpCacheInvalidated => "acp_cache_invalidated",
        events::EventName::ActionExecution => "action_execution",
        events::EventName::WildCard => "wildcard",
        _ => "unknown",
    };
    serde_json::json!({
        "type": event_type
    })
    .to_string()
}

pub(crate) use query::subscription::response_has_data;
