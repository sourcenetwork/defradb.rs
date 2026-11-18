use async_tungstenite::tungstenite::Message;
use db::block::builder::ClientDeleteSignatures;
use futures::{Sink, Stream};
use query::executor::{QueryRequest, QueryResponse};
use serde_json::Value;

use crate::client_signed_create::{self, ClientSignedCreateError};

/// DeleteNode does not retain extra filter predicates after resolving IDs.
pub fn single_document_target(mutation: &query::mapper::Mutation) -> Option<&str> {
    query::mapper::single_document_mutation_target(mutation, true)
}

fn response_key(request: &QueryRequest, graph: &ClientDeleteSignatures) -> Option<String> {
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
        || mutations[0].mutation_type != query::MutationType::Delete
        || mutations[0].encryption().is_some()
        || single_document_target(&mutations[0]) != Some(graph.doc_id())
        || !client_signed_create::valid_doc_id_selection(&mutations[0].fields)
        || !graph.matches_target(&mutations[0].collection_name, graph.doc_id())
    {
        return None;
    }
    Some(mutations[0].output_name())
}

/// Execute one approved deletion. A lost response after dispatch leaves the outcome unknown.
pub async fn execute(
    url: &str,
    authorization: &str,
    request: QueryRequest,
    txn_id: Option<&str>,
    graph: &mut ClientDeleteSignatures,
) -> Result<QueryResponse, ClientSignedCreateError> {
    let key = response_key(&request, graph).ok_or(ClientSignedCreateError::InputMismatch)?;
    client_signed_create::execute_graph(url, authorization, request, txn_id, graph, &key).await
}

/// Run the same conversation over the caller's authenticated, bounded socket.
pub async fn execute_with_socket<S, E>(
    mut socket: S,
    request: QueryRequest,
    txn_id: Option<&str>,
    graph: &mut ClientDeleteSignatures,
) -> Result<QueryResponse, ClientSignedCreateError>
where
    S: Sink<Message, Error = E> + Stream<Item = Result<Message, E>> + Unpin,
    E: std::fmt::Display,
{
    let key = response_key(&request, graph).ok_or(ClientSignedCreateError::InputMismatch)?;
    client_signed_create::conversation_on_socket(&mut socket, request, txn_id, graph, &key).await
}

#[cfg(test)]
#[path = "../tests/unit/client_signed_delete.rs"]
mod tests;
