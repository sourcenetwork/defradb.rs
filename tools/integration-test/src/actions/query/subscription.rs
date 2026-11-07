use std::sync::Arc;
use std::time::Duration;

use defra_core::RequestContext;
use events::{EventName, Message};
use query::subscription::{
    response_has_data, subscription_accepts_doc_id, subscription_doc_ids,
    subscription_to_scoped_query, validate_subscription,
};
use rapidhash::{HashMapExt, RapidHashMap};
use regolith::sync::Atom;

use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// How long reading a subscription waits for each event it still expects.
const EVENT_WAIT: Duration = Duration::from_secs(10);
/// How long it waits for one more event when it expects no count.
const IDLE_WAIT: Duration = Duration::from_millis(500);

/// Starts a GraphQL subscription, or finishes one and checks the events it received.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_count: Option<i64>,
    pub subscription: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

/// An open subscription: the node's update events since it opened, and the
/// query each one answers, as the HTTP API's SSE stream runs it.
pub struct OpenSubscription {
    node: String,
    query: String,
    context: RequestContext,
    expected: Option<usize>,
    events: async_channel::Receiver<Message>,
}

/// The test's open subscriptions by name. Shared by every clone of the state.
#[derive(Clone, Default)]
pub struct Subscriptions(Arc<Atom<RapidHashMap<String, Arc<OpenSubscription>>>>);

impl Subscriptions {
    fn open(&self, name: &str, subscription: OpenSubscription) {
        let subscription = Arc::new(subscription);
        self.0.rcu(|open| {
            let mut next = open.clone();
            next.insert(name.to_owned(), subscription.clone());
            next
        });
    }

    fn take(&self, name: &str) -> Option<Arc<OpenSubscription>> {
        let mut taken = None;
        self.0.rcu(|open| {
            let mut next = RapidHashMap::with_capacity(open.len());
            for (key, value) in open {
                match key == name {
                    true => taken = Some(value.clone()),
                    false => drop(next.insert(key.clone(), value.clone())),
                }
            }
            next
        });
        taken
    }
}

impl Op for SubscriptionRequest {
    const GRAPHQL: bool = true;

    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run(&self, state: &State) -> Result<Value, Failure> {
        let identity = self.common.client_identity(Self::name())?;
        match &self.request {
            Some(query) => self.open(state, query.as_str(), identity),
            None => self.read(state),
        }
    }
}

impl SubscriptionRequest {
    fn open(&self, state: &State, query: &str, identity: Option<u64>) -> Result<Value, Failure> {
        validate_subscription(query, None).map_err(|e| Failure::operation(e.to_string()))?;
        let node = state.node_on(self.common.node())?;
        let events = node.subscribe(&[EventName::Update]).into_receiver();
        state.subscriptions.open(
            &self.subscription,
            OpenSubscription {
                node: self.common.node().to_owned(),
                query: query.to_owned(),
                context: state.context_as(None, identity),
                expected: self.expected_count.and_then(|n| usize::try_from(n).ok()),
                events,
            },
        );
        Ok(json!({ "data": null }))
    }

    /// The responses the subscription delivered: one per update to a document
    /// it selects, each the query scoped to that document's new head.
    fn read(&self, state: &State) -> Result<Value, Failure> {
        let open = state
            .subscriptions
            .take(&self.subscription)
            .ok_or_else(|| {
                Failure::operation(format!("no open subscription {}", self.subscription))
            })?;
        let node = state.node_on(&open.node)?;
        let doc_ids = subscription_doc_ids(&open.query, None, None);
        let mut delivered = Vec::new();
        while open
            .expected
            .is_none_or(|expected| delivered.len() < expected)
        {
            let wait = match open.expected {
                Some(_) => EVENT_WAIT,
                None => IDLE_WAIT,
            };
            let Ok(Ok(message)) = state.block_on(workers::timeout(wait, open.events.recv())) else {
                break;
            };
            let Some(update) = message.as_update() else {
                continue;
            };
            if !subscription_accepts_doc_id(doc_ids.as_deref(), &update.doc_id) {
                continue;
            }
            let scoped = subscription_to_scoped_query(
                &open.query,
                &update.doc_id,
                &update.cid.to_string(),
                None,
            )
            .map_err(|e| Failure::operation(e.to_string()))?;
            let response = state
                .block_on(node.execute_request_as(&open.context, query::QueryRequest::new(scoped)))
                .map_err(|e| Failure::operation(e.to_string()))?;
            if response_has_data(&response) {
                delivered.push(json!({ "data": response.data }));
            }
        }
        Ok(json!({ "data": { "events": delivered } }))
    }
}

impl Action for SubscriptionRequest {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
