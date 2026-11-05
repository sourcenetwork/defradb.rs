use std::time::Duration;

use async_tungstenite::tungstenite::client::IntoClientRequest;
use async_tungstenite::tungstenite::Message;
use futures::{Sink, SinkExt, Stream, StreamExt};
use query::executor::{QueryRequest, QueryResponse};
use serde_json::{json, Value};

use db::block::builder::{
    ClientCreateError, ClientCreateSignatures, ClientDeleteError, ClientDeleteSignatures,
    ClientUpdateError, ClientUpdateSignatures,
};

#[derive(Debug, thiserror::Error)]
pub enum ClientSignedCreateError {
    #[error(
        "the GraphQL mutation differs from the approved document or omits the unaliased _docID"
    )]
    InputMismatch,
    #[error("invalid signing endpoint or authorization header: {0}")]
    Request(String),
    #[error("signing connection failed: {0}")]
    Connection(String),
    #[error("invalid signing conversation: {0}")]
    Protocol(String),
    #[error("server requested a block outside the approved graph: {0}")]
    Challenge(#[from] ClientCreateError),
    #[error("server requested a block outside the approved update graph: {0}")]
    UpdateChallenge(#[from] ClientUpdateError),
    #[error("server requested a block outside the approved delete graph: {0}")]
    DeleteChallenge(#[from] ClientDeleteError),
    #[error("server reported success before the approved graph was signed")]
    Incomplete,
    #[error("server returned a different document ID")]
    DocumentId,
    #[error("client signing conversation timed out")]
    Timeout,
}

pub(crate) trait ClientSigningGraph {
    fn public_key_hex(&self) -> &str;
    fn doc_id(&self) -> &str;
    fn is_complete(&self) -> bool;
    fn respond(&mut self, blocks: &[Vec<u8>]) -> Result<Vec<Vec<u8>>, ClientSignedCreateError>;
}

impl ClientSigningGraph for ClientCreateSignatures {
    fn public_key_hex(&self) -> &str {
        self.public_key_hex()
    }
    fn doc_id(&self) -> &str {
        self.doc_id()
    }
    fn is_complete(&self) -> bool {
        self.is_complete()
    }
    fn respond(&mut self, blocks: &[Vec<u8>]) -> Result<Vec<Vec<u8>>, ClientSignedCreateError> {
        Ok(self.respond(blocks)?)
    }
}

impl ClientSigningGraph for ClientUpdateSignatures {
    fn public_key_hex(&self) -> &str {
        self.public_key_hex()
    }
    fn doc_id(&self) -> &str {
        self.doc_id()
    }
    fn is_complete(&self) -> bool {
        self.is_complete()
    }
    fn respond(&mut self, blocks: &[Vec<u8>]) -> Result<Vec<Vec<u8>>, ClientSignedCreateError> {
        Ok(self.respond(blocks)?)
    }
}

impl ClientSigningGraph for ClientDeleteSignatures {
    fn public_key_hex(&self) -> &str {
        self.public_key_hex()
    }
    fn doc_id(&self) -> &str {
        self.doc_id()
    }
    fn is_complete(&self) -> bool {
        self.is_complete()
    }
    fn respond(&mut self, blocks: &[Vec<u8>]) -> Result<Vec<Vec<u8>>, ClientSignedCreateError> {
        Ok(self.respond(blocks)?)
    }
}

pub(crate) fn valid_doc_id_selection(fields: &[query::mapper::Requestable]) -> bool {
    query::mapper::valid_unaliased_doc_id_selection(fields)
}

fn response_key(request: &QueryRequest, graph: &ClientCreateSignatures) -> Option<String> {
    let variables = match &request.variables {
        Some(Value::Object(map)) => Some(
            map.iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        ),
        None => None,
        _ => return None,
    };
    let Ok(query::query_parse::ParsedOperation::Mutation {
        mutations,
        explain: None,
    }) = query::query_parse::parse_request_with_limits(
        &request.query,
        variables.as_ref(),
        request.operation_name.as_deref(),
        query::QueryLimits::default(),
    )
    else {
        return None;
    };
    if mutations.len() != 1
        || mutations[0].mutation_type != query::MutationType::Add
        || mutations[0].create_input.len() != 1
        || mutations[0].encryption().is_some()
        || !valid_doc_id_selection(&mutations[0].fields)
    {
        return None;
    }
    let Ok(input) = serde_json::to_value(&mutations[0].create_input[0]) else {
        return None;
    };
    graph
        .matches_input(&mutations[0].collection_name, &input)
        .then(|| mutations[0].output_name())
}

/// Execute one create with a client-owned precomputed graph. The caller must
/// build `graph` from the exact intended input and a locally trusted schema.
/// A connection failure after dispatch may leave the commit outcome unknown.
pub async fn execute(
    url: &str,
    authorization: &str,
    request: QueryRequest,
    txn_id: Option<&str>,
    graph: &mut ClientCreateSignatures,
) -> Result<QueryResponse, ClientSignedCreateError> {
    let response_key =
        response_key(&request, graph).ok_or(ClientSignedCreateError::InputMismatch)?;
    execute_graph(url, authorization, request, txn_id, graph, &response_key).await
}

pub(crate) async fn execute_graph<G>(
    url: &str,
    authorization: &str,
    request: QueryRequest,
    txn_id: Option<&str>,
    graph: &mut G,
    response_key: &str,
) -> Result<QueryResponse, ClientSignedCreateError>
where
    G: ClientSigningGraph,
{
    nuclei::time::timeout(
        Duration::from_secs(90),
        conversation(url, authorization, request, txn_id, graph, response_key),
    )
    .await
    .map_err(|_| ClientSignedCreateError::Timeout)?
}

/// Build the authenticated WebSocket handshake for a caller-owned transport.
pub fn websocket_request(
    url: &str,
    authorization: &str,
) -> Result<axum::http::Request<()>, ClientSignedCreateError> {
    let mut handshake = url
        .into_client_request()
        .map_err(|error| ClientSignedCreateError::Request(error.to_string()))?;
    let mut auth = authorization
        .parse::<axum::http::HeaderValue>()
        .map_err(|error| ClientSignedCreateError::Request(error.to_string()))?;
    auth.set_sensitive(true);
    handshake
        .headers_mut()
        .insert(axum::http::header::AUTHORIZATION, auth);
    Ok(handshake)
}

/// Bound incoming frames before the transport allocates their payloads.
pub fn websocket_config() -> async_tungstenite::tungstenite::protocol::WebSocketConfig {
    async_tungstenite::tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(1024 * 1024))
        .max_frame_size(Some(1024 * 1024))
}

/// Run the native signing conversation on a caller-connected WebSocket.
/// The caller supplies the authenticated handshake, `websocket_config` limits,
/// and an overall deadline using its own runtime. A lost response after
/// dispatch leaves the commit outcome unknown.
pub async fn execute_with_socket<S, E>(
    mut socket: S,
    request: QueryRequest,
    txn_id: Option<&str>,
    graph: &mut ClientCreateSignatures,
) -> Result<QueryResponse, ClientSignedCreateError>
where
    S: Sink<Message, Error = E> + Stream<Item = Result<Message, E>> + Unpin,
    E: std::fmt::Display,
{
    let response_key =
        response_key(&request, graph).ok_or(ClientSignedCreateError::InputMismatch)?;
    conversation_on_socket(&mut socket, request, txn_id, graph, &response_key).await
}

async fn conversation<G>(
    url: &str,
    authorization: &str,
    request: QueryRequest,
    txn_id: Option<&str>,
    graph: &mut G,
    response_key: &str,
) -> Result<QueryResponse, ClientSignedCreateError>
where
    G: ClientSigningGraph,
{
    let handshake = websocket_request(url, authorization)?;
    let stream = net::connect(handshake.uri())
        .await
        .map_err(|error| ClientSignedCreateError::Connection(error.to_string()))?;
    let config = websocket_config();
    let (mut socket, _) =
        async_tungstenite::client_async_with_config(handshake, stream, Some(config))
            .await
            .map_err(|error| ClientSignedCreateError::Connection(error.to_string()))?;
    conversation_on_socket(&mut socket, request, txn_id, graph, response_key).await
}

pub(crate) async fn conversation_on_socket<S, E, G>(
    socket: &mut S,
    request: QueryRequest,
    txn_id: Option<&str>,
    graph: &mut G,
    response_key: &str,
) -> Result<QueryResponse, ClientSignedCreateError>
where
    S: Sink<Message, Error = E> + Stream<Item = Result<Message, E>> + Unpin,
    E: std::fmt::Display,
    G: ClientSigningGraph,
{
    let start = json!({"query":request.query, "operationName":request.operation_name, "variables":request.variables, "txn_id":txn_id, "public_key_hex":graph.public_key_hex()});
    socket
        .send(Message::Text(start.to_string().into()))
        .await
        .map_err(|error| ClientSignedCreateError::Connection(error.to_string()))?;
    while let Some(message) = socket.next().await {
        let message =
            message.map_err(|error| ClientSignedCreateError::Connection(error.to_string()))?;
        let Message::Text(text) = message else {
            return Err(ClientSignedCreateError::Protocol(
                "expected a text message".into(),
            ));
        };
        if text.len() > 1024 * 1024 {
            return Err(ClientSignedCreateError::Protocol(
                "message too large".into(),
            ));
        }
        let value: Value = serde_json::from_str(&text)
            .map_err(|error| ClientSignedCreateError::Protocol(error.to_string()))?;
        if let Some(blocks) = value.get("blocks").and_then(Value::as_array) {
            let blocks = blocks
                .iter()
                .map(|block| {
                    block
                        .as_str()
                        .ok_or_else(|| ClientSignedCreateError::Protocol("invalid block".into()))
                        .and_then(|block| {
                            hex::decode(block).map_err(|error| {
                                ClientSignedCreateError::Protocol(error.to_string())
                            })
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let signatures = graph.respond(&blocks)?;
            let signatures = signatures.iter().map(hex::encode).collect::<Vec<_>>();
            socket
                .send(Message::Text(
                    json!({"signatures":signatures}).to_string().into(),
                ))
                .await
                .map_err(|error| ClientSignedCreateError::Connection(error.to_string()))?;
        } else if let Some(response) = value.get("response") {
            let response: QueryResponse = serde_json::from_value(response.clone())
                .map_err(|error| ClientSignedCreateError::Protocol(error.to_string()))?;
            if response.errors.is_empty() {
                if !graph.is_complete() {
                    return Err(ClientSignedCreateError::Incomplete);
                }
                if !response
                    .data
                    .as_ref()
                    .and_then(|data| data.get(response_key))
                    .and_then(Value::as_array)
                    .is_some_and(|docs| {
                        docs.len() == 1
                            && docs[0].get("_docID").and_then(Value::as_str) == Some(graph.doc_id())
                    })
                {
                    return Err(ClientSignedCreateError::DocumentId);
                }
            }
            return Ok(response);
        } else {
            return Err(ClientSignedCreateError::Protocol(
                "unexpected message".into(),
            ));
        }
    }
    Err(ClientSignedCreateError::Connection(
        "connection closed before a response".into(),
    ))
}
