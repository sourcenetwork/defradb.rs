use async_tungstenite::tungstenite::Message;
use db::block::builder::ClientDeleteSignatures;
use futures::{Sink, Stream};
use query::executor::{QueryRequest, QueryResponse};
use serde_json::Value;

use crate::client_signed_create::{self, ClientSignedCreateError};

/// DeleteNode does not retain extra filter predicates after resolving IDs.
pub fn single_document_target(mutation: &query::mapper::Mutation) -> Option<&str> {
    if mutation
        .filter
        .as_ref()
        .is_some_and(|filter| filter.conditions().len() != 1)
    {
        return None;
    }
    crate::client_signed_update::single_document_target(mutation)
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
mod tests {
    use serde_json::{json, Value};
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[test]
    #[ignore = "Requires Lean; run in the Lean Conformance job"]
    fn physical_delete_filter_admission_matches_model() {
        let requests = [
            r#"mutation { delete_Item(docIDs: ["doc"]) { _docID } }"#,
            r#"mutation { delete_Item(docIDs: ["doc", "other"]) { _docID } }"#,
            r#"mutation { delete_Item(filter: {_docID: {_eq: "doc"}}) { _docID } }"#,
            r#"mutation { delete_Item(filter: {_docID: {_eq: "doc"}, status: {_eq: "open"}}) { _docID } }"#,
            r#"mutation { delete_Item(filter: {_docID: {_eq: "other"}}) { _docID } }"#,
            r#"mutation { delete_Item(filter: {_docID: {_in: ["doc"]}}) { _docID } }"#,
            r#"mutation { delete_Item(filter: {_or: [{_docID: {_eq: "doc"}}]}) { _docID } }"#,
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
        let inputs = mutations.iter().map(|mutation| {
            let direct = mutation.doc_ids.as_ref().map(|ids| if ids.len() == 1 {ids[0].as_str()} else {"multiple"});
            json!({"expected":"doc", "direct":direct, "filter":mutation.filter.as_ref().map(|filter| filter.conditions())})
        }).collect::<Vec<_>>();
        let mut child = Command::new("lake")
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
            .args(["env", "lean", "--run", "ClientSignedDeleteFilter.lean"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(), "{}", json!(inputs)).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let modeled: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        for (mutation, expected) in mutations.iter().zip(modeled) {
            assert_eq!(
                super::single_document_target(mutation) == Some("doc"),
                expected["accepted"].as_bool().unwrap()
            );
        }
    }
}
