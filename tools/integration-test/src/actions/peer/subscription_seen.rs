//! After a node subscribes to collections, wait until every node that can
//! deliver to it has seen the subscription.
//!
//! A write is announced once, to the peers subscribed when it commits (Go's
//! `SendUpdate`), so a write made before its node sees a subscription never
//! reaches the subscriber. Go's harness covers this with a 100 ms sleep after
//! `AddP2PCollections`; testo waits for the subscription itself, so a slow
//! machine is not a flake and a fast one does not wait.

use std::sync::Arc;
use std::time::{Duration, Instant};

use defra_core::RequestContext;
use defra_node::Node;
use events::{EventName, Message};
use rapidhash::{HashSetExt, RapidHashSet};
use schema::CollectionSelector;
use serde_json::Value;

use crate::nodes::peer_id;
use crate::scenario::Failure;
use crate::test_case::State;

/// How long every node in reach gets to see a subscription.
const SUBSCRIPTION_SEEN: Duration = Duration::from_secs(10);

/// One node in reach of the subscriber, and its topic events since before
/// the subscription.
struct Watcher {
    name: String,
    events: async_channel::Receiver<Message>,
}

/// Run `subscribe` on node `name`, then wait until every node connected to it,
/// directly or through other nodes, reports it subscribed to each topic it
/// newly subscribed to.
///
/// Nodes in reach are those in the subscriber's connected component when it
/// subscribes, from each node's own connected peers: discovery only connects
/// nodes within a component, so a node outside it can never deliver to the
/// subscriber and is not waited for. Only Rust nodes report what they see;
/// a Go node (the go-http client) is neither waited for nor walked through.
pub fn subscribed_everywhere<F>(state: &State, name: &str, subscribe: F) -> Result<Value, Failure>
where
    F: FnOnce() -> Result<Value, Failure>,
{
    let Some(subscriber) = state.node(name)?.core() else {
        return subscribe();
    };
    let context = RequestContext::ambient();
    // ponytail: a node whose subscriptions the harness cannot list (NAC
    // refusing the ambient caller) is not waited on.
    let Ok(before) = subscribed(state, &subscriber, &context) else {
        return subscribe();
    };
    let watchers = in_reach(state, name, &context)?
        .into_iter()
        .map(|(name, node)| Watcher {
            name,
            events: node
                .event_bus()
                .subscribe(&[EventName::TopicPeerEvent])
                .into_receiver(),
        })
        .collect::<Vec<_>>();
    let done = subscribe()?;
    let names: Vec<String> = subscribed(state, &subscriber, &context)?
        .difference(&before)
        .cloned()
        .collect();
    if names.is_empty() {
        return Ok(done);
    }
    let added = topics(state, &subscriber, &context, names)?;
    let subscriber_id = own_peer_id(state, name)?;
    let end = Instant::now() + SUBSCRIPTION_SEEN;
    let mut unseen = Vec::new();
    for watcher in watchers {
        let mut wanted: RapidHashSet<&str> = added.iter().map(String::as_str).collect();
        while !wanted.is_empty() {
            let left = end.saturating_duration_since(Instant::now());
            let Ok(Ok(message)) = state.block_on(workers::timeout(left, watcher.events.recv()))
            else {
                break;
            };
            if let Some(event) = message.as_topic_peer_event() {
                if event.event_type == "JOINED" && event.peer_id == subscriber_id {
                    wanted.remove(event.topic.as_str());
                }
            }
        }
        if !wanted.is_empty() {
            unseen.push(watcher.name);
        }
    }
    if unseen.is_empty() {
        return Ok(done);
    }
    unseen.sort();
    Err(Failure::operation(format!(
        "{name}'s subscription to {added:?} never reached {unseen:?} within {SUBSCRIPTION_SEEN:?}"
    )))
}

/// The names of the collections node `node` subscribes to.
fn subscribed(
    state: &State,
    node: &Arc<Node>,
    context: &RequestContext,
) -> Result<RapidHashSet<String>, Failure> {
    let names = state
        .block_on(node.list_p2p_collections_as(context))
        .map_err(|error| Failure::operation(error.to_string()))?;
    Ok(names.into_iter().collect())
}

/// The gossip topics of collections `names`: their collection IDs.
fn topics(
    state: &State,
    node: &Arc<Node>,
    context: &RequestContext,
    names: Vec<String>,
) -> Result<Vec<String>, Failure> {
    let selector = CollectionSelector {
        names: Some(names),
        ..Default::default()
    };
    let versions = state
        .block_on(node.get_collections_as(context, &selector))
        .map_err(|error| Failure::operation(error.to_string()))?;
    let ids: RapidHashSet<String> = versions.into_iter().map(|v| v.collection_id).collect();
    Ok(ids.into_iter().collect())
}

/// Node `name`'s peer ID.
fn own_peer_id(state: &State, name: &str) -> Result<String, Failure> {
    let addresses = state.node(name)?.peer_info().map_err(Failure::operation)?;
    let address = addresses
        .first()
        .ok_or_else(|| Failure::operation(format!("node {name} has no P2P address")))?;
    Ok(peer_id(address).to_owned())
}

/// The other Rust nodes in node `name`'s connected component, walked
/// through each node's connected peers.
fn in_reach(
    state: &State,
    name: &str,
    context: &RequestContext,
) -> Result<Vec<(String, Arc<Node>)>, Failure> {
    let mut nodes = Vec::new();
    for (other, node) in state.running() {
        let Some(core) = node.core() else { continue };
        let Ok(addresses) = node.peer_info() else {
            continue;
        };
        let Some(id) = addresses.first().map(|address| peer_id(address).to_owned()) else {
            continue;
        };
        let peers: RapidHashSet<String> = state
            .block_on(core.active_peers_as(context))
            .unwrap_or_default()
            .iter()
            .map(|peer| peer_id(peer).to_owned())
            .collect();
        nodes.push((other, id, core, peers));
    }
    let start = own_peer_id(state, name)?;
    let mut reached: RapidHashSet<String> = RapidHashSet::new();
    let mut frontier = vec![start.clone()];
    // Connections are two-way: a node is in reach if it lists a reached peer
    // or a reached node lists it.
    while let Some(id) = frontier.pop() {
        if !reached.insert(id.clone()) {
            continue;
        }
        for (_, node_id, _, peers) in &nodes {
            if *node_id == id {
                frontier.extend(peers.iter().cloned());
            } else if peers.contains(&id) {
                frontier.push(node_id.clone());
            }
        }
    }
    Ok(nodes
        .into_iter()
        .filter(|(_, id, _, _)| *id != start && reached.contains(id))
        .map(|(other, _, core, _)| (other, core))
        .collect())
}
