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

use crate::client_type::{network_edge, ClientType};
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// How long reading a subscription waits for each event it still expects.
const EVENT_WAIT: Duration = Duration::from_secs(10);
/// How long it waits for one more event when it expects no count.
const IDLE_WAIT: Duration = Duration::from_millis(500);
/// How long an SSE subscription stream may stay open.
const STREAM_LIFETIME: Duration = Duration::from_secs(600);

/// Starts a GraphQL subscription, or finishes one and checks the events it received.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "query")]
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
    source: Source,
}

/// Where an open subscription's responses come from.
enum Source {
    /// The in-process node's update events, each answered here.
    Node(async_channel::Receiver<Message>),
    /// The `next` events of the node's SSE stream, read on the network edge
    /// until the stream ends or `reader` is aborted.
    Http {
        responses: async_channel::Receiver<Result<Value, String>>,
        reader: edge::Task<()>,
    },
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
        match (&self.request, state.client_type()) {
            (Some(_), ClientType::Cli) => Err(Failure::unsupported(format!(
                "{} over the CLI, which has no subscription stream",
                Self::name()
            ))),
            (Some(query), ClientType::InProcess) => self.open(state, query.as_str(), identity),
            (Some(query), _) => self.open_http(state, query.as_str(), identity),
            (None, _) => self.read(state),
        }
    }
}

impl SubscriptionRequest {
    fn open(&self, state: &State, query: &str, identity: Option<u64>) -> Result<Value, Failure> {
        validate_subscription(query, None).map_err(|e| Failure::operation(e.to_string()))?;
        let node = state.node_on(self.common.node())?;
        let events = node.subscribe(&[EventName::Update]).into_receiver();
        self.keep(state, query, identity, Source::Node(events));
        Ok(json!({ "data": null }))
    }

    /// Open the subscription as the HTTP API serves it: a GraphQL request
    /// answered with a server-sent event stream, whose `next` events a task
    /// on the network edge reads until the stream ends.
    fn open_http(
        &self,
        state: &State,
        query: &str,
        identity: Option<u64>,
    ) -> Result<Value, Failure> {
        let base = state.url_on(self.common.node())?;
        let url = format!("{base}/api/v0/graphql");
        let token = identity.map(|seed| {
            cli::commands::client::generate_auth_token(
                &crate::scenario::identity::private_key_hex(seed),
                &base,
            )
            .expect("a token for a test identity")
        });
        let body = json!({ "query": query });
        let edge = network_edge();
        let response = state
            .block_on(edge.spawn({
                let edge = edge.clone();
                async move {
                    let mut request = net::http::Client::new(edge)
                        .post(&url)
                        .header("Accept", "text/event-stream")
                        .timeout(STREAM_LIFETIME)
                        .json(&body);
                    if let Some(token) = token {
                        request = request.bearer_auth(token);
                    }
                    request.send().await
                }
            }))
            .expect("run on the network edge")
            .map_err(|e| Failure::operation(e.to_string()))?;
        if !response.status().is_success() {
            let text = state
                .block_on(edge.spawn(response.text()))
                .expect("run on the network edge")
                .unwrap_or_default();
            return Err(Failure::operation(text));
        }
        let (sender, responses) = async_channel::unbounded();
        let reader = edge.spawn(read_events(response, sender));
        self.keep(state, query, identity, Source::Http { responses, reader });
        Ok(json!({ "data": null }))
    }

    fn keep(&self, state: &State, query: &str, identity: Option<u64>, source: Source) {
        state.subscriptions.open(
            &self.subscription,
            OpenSubscription {
                node: self.common.node().to_owned(),
                query: query.to_owned(),
                context: state.context_as(None, identity),
                expected: self.expected_count.and_then(|n| usize::try_from(n).ok()),
                source,
            },
        );
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
        let delivered = match &open.source {
            Source::Node(events) => self.read_node(state, &open, events)?,
            Source::Http { responses, reader } => {
                let delivered = read_http(state, &open, responses);
                reader.abort();
                delivered?
            }
        };
        Ok(json!({ "data": { "events": delivered } }))
    }

    fn read_node(
        &self,
        state: &State,
        open: &OpenSubscription,
        events: &async_channel::Receiver<Message>,
    ) -> Result<Vec<Value>, Failure> {
        let node = state.node_on(&open.node)?;
        let doc_ids = subscription_doc_ids(&open.query, None, None);
        let mut delivered = Vec::new();
        while open
            .expected
            .is_none_or(|expected| delivered.len() < expected)
        {
            let Ok(Ok(message)) = state.block_on(workers::timeout(open.wait(), events.recv()))
            else {
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
        Ok(delivered)
    }
}

impl OpenSubscription {
    /// How long a read waits for the next response.
    fn wait(&self) -> Duration {
        match self.expected {
            Some(_) => EVENT_WAIT,
            None => IDLE_WAIT,
        }
    }
}

/// The `next` responses an SSE subscription delivered, as the in-process
/// read returns them; a response with errors fails the read with them.
fn read_http(
    state: &State,
    open: &OpenSubscription,
    responses: &async_channel::Receiver<Result<Value, String>>,
) -> Result<Vec<Value>, Failure> {
    let mut delivered = Vec::new();
    while open
        .expected
        .is_none_or(|expected| delivered.len() < expected)
    {
        let Ok(Ok(response)) = state.block_on(workers::timeout(open.wait(), responses.recv()))
        else {
            break;
        };
        let response = response.map_err(Failure::operation)?;
        if let Some(errors) = response.get("errors").filter(|errors| !errors.is_null()) {
            return Err(Failure::operation(errors.to_string()));
        }
        delivered.push(json!({ "data": response.get("data").cloned().unwrap_or(Value::Null) }));
    }
    Ok(delivered)
}

/// Read the SSE stream `response`, sending the data of each `next` event,
/// until the stream completes or closes; a failed read is sent as its error.
async fn read_events(
    mut response: net::http::Response,
    sender: async_channel::Sender<Result<Value, String>>,
) {
    let mut buffer = Vec::new();
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => return,
            Err(error) => {
                let _ = sender.send(Err(error.to_string())).await;
                return;
            }
        };
        buffer.extend(chunk.iter().copied().filter(|byte| *byte != b'\r'));
        while let Some(end) = buffer.windows(2).position(|pair| pair == b"\n\n") {
            let frame: Vec<u8> = buffer.drain(..end + 2).collect();
            let event = String::from_utf8_lossy(&frame);
            let (mut name, mut data) = ("message", Vec::new());
            for line in event.lines() {
                if let Some(value) = line.strip_prefix("event:") {
                    name = if value.trim() == "next" {
                        "next"
                    } else {
                        "other"
                    };
                } else if let Some(value) = line.strip_prefix("data:") {
                    data.push(value.trim_start());
                }
            }
            if name != "next" {
                continue;
            }
            let Ok(value) = serde_json::from_str(&data.join("\n")) else {
                continue;
            };
            if sender.send(Ok(value)).await.is_err() {
                return;
            }
        }
    }
}

impl Action for SubscriptionRequest {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
