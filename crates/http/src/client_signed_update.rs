use async_tungstenite::tungstenite::Message;
use db::block::builder::ClientUpdateSignatures;
use futures::{Sink, Stream};
use query::executor::{QueryRequest, QueryResponse};
use serde_json::Value;

use crate::client_signed_create;
pub use crate::client_signed_create::ClientSignedCreateError as ClientSignedUpdateError;

/// Query the complete ACP-filtered history needed for a client-verified snapshot.
pub fn history_request(doc_id: &str) -> QueryRequest {
    QueryRequest::new("query($docID: [String!]!) { _commits(docID: $docID) { cid docID fieldName height delta collectionVersionId links { cid fieldName } heads { cid } signature { type identity value } } }")
        .with_variables(serde_json::json!({"docID": [doc_id]}))
}

/// Validate the complete history in a GraphQL response.
pub fn snapshot_from_response(
    doc_id: &str,
    response: &QueryResponse,
) -> Result<db::block::builder::ClientUpdateSnapshot, db::block::builder::ClientUpdateError> {
    if !response.errors.is_empty() {
        return Err(db::block::builder::ClientUpdateError::HistoryRead);
    }
    let rows = response
        .data
        .as_ref()
        .and_then(|data| data.get("_commits"))
        .and_then(Value::as_array)
        .ok_or(db::block::builder::ClientUpdateError::InvalidSnapshot)?;
    db::block::builder::ClientUpdateSnapshot::from_commits(doc_id, rows)
}

/// Identify an unconditional DocID update or a conditional update whose
/// top-level `_docID` equality restricts its complete filter to one document.
pub fn single_document_target(mutation: &query::mapper::Mutation) -> Option<&str> {
    match (&mutation.doc_ids, &mutation.filter) {
        (Some(ids), None) if ids.len() == 1 => Some(ids[0].as_str()),
        (None, Some(filter)) => {
            let condition = filter.conditions().get("_docID")?.as_object()?;
            if condition.len() != 1 {
                return None;
            }
            condition.get("_eq")?.as_str()
        }
        _ => None,
    }
}

fn response_key(request: &QueryRequest, graph: &ClientUpdateSignatures) -> Option<String> {
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
        || mutations[0].mutation_type != query::MutationType::Update
        || mutations[0].encryption().is_some()
        || single_document_target(&mutations[0]) != Some(graph.doc_id())
        || !client_signed_create::valid_doc_id_selection(&mutations[0].fields)
    {
        return None;
    }
    let input = serde_json::to_value(&mutations[0].update_input).ok()?;
    graph
        .matches_input(&mutations[0].collection_name, graph.doc_id(), &input)
        .then(|| mutations[0].output_name())
}

/// Run one update against the independently approved graph. A lost response
/// after dispatch leaves the commit outcome unknown.
pub async fn execute(
    url: &str,
    authorization: &str,
    request: QueryRequest,
    txn_id: Option<&str>,
    graph: &mut ClientUpdateSignatures,
) -> Result<QueryResponse, ClientSignedUpdateError> {
    let key = response_key(&request, graph).ok_or(ClientSignedUpdateError::InputMismatch)?;
    client_signed_create::execute_graph(url, authorization, request, txn_id, graph, &key).await
}

/// Run the same conversation over the caller's authenticated, bounded socket.
pub async fn execute_with_socket<S, E>(
    mut socket: S,
    request: QueryRequest,
    txn_id: Option<&str>,
    graph: &mut ClientUpdateSignatures,
) -> Result<QueryResponse, ClientSignedUpdateError>
where
    S: Sink<Message, Error = E> + Stream<Item = Result<Message, E>> + Unpin,
    E: std::fmt::Display,
{
    let key = response_key(&request, graph).ok_or(ClientSignedUpdateError::InputMismatch)?;
    client_signed_create::conversation_on_socket(&mut socket, request, txn_id, graph, &key).await
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::process::{Command, Stdio};

    use serde_json::{json, Value};

    #[test]
    #[ignore = "Requires Lean; run in the Lean Conformance job"]
    fn single_document_filter_admission_matches_model() {
        let requests = [
            r#"mutation { update_Item(docID: "doc", input: {status: "done"}) { _docID } }"#,
            r#"mutation { update_Item(docID: "other", input: {status: "done"}) { _docID } }"#,
            r#"mutation { update_Item(filter: {_docID: {_eq: "doc"}, status: {_eq: "open"}}, input: {status: "done"}) { _docID } }"#,
            r#"mutation { update_Item(filter: {_docID: {_eq: "other"}}, input: {status: "done"}) { _docID } }"#,
            r#"mutation { update_Item(filter: {_docID: {_in: ["doc"]}}, input: {status: "done"}) { _docID } }"#,
            r#"mutation { update_Item(filter: {_or: [{_docID: {_eq: "doc"}}]}, input: {status: "done"}) { _docID } }"#,
            r#"mutation { update_Item(docID: "doc", filter: {_docID: {_eq: "doc"}}, input: {status: "done"}) { _docID } }"#,
        ];
        let mutations = requests
            .iter()
            .map(|source| {
                let query::query_parse::ParsedOperation::Mutation { mut mutations, .. } =
                    query::query_parse::parse_request_with_limits(
                        source,
                        None,
                        None,
                        query::QueryLimits::default(),
                    )
                    .unwrap()
                else {
                    panic!("expected mutation")
                };
                mutations.remove(0)
            })
            .collect::<Vec<_>>();
        let inputs = mutations
            .iter()
            .map(|mutation| {
                let direct = mutation.doc_ids.as_ref().map(|ids| {
                    if ids.len() == 1 {
                        ids[0].as_str()
                    } else {
                        "multiple"
                    }
                });
                json!({"expected": "doc", "direct": direct,
                "filter": mutation.filter.as_ref().map(|filter| filter.conditions())})
            })
            .collect::<Vec<_>>();
        let mut child = Command::new("lake")
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
            .args(["env", "lean", "--run", "ClientSignedUpdateFilter.lean"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(), "{}", json!(inputs)).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        for (mutation, modeled) in mutations.iter().zip(expected) {
            assert_eq!(
                super::single_document_target(mutation) == Some("doc"),
                modeled["accepted"].as_bool().unwrap()
            );
        }
    }
}
