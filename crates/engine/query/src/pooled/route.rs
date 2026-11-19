//! Where a request runs, decided once at the edge: a write that names one
//! document goes to that document's worker, which runs one such task per
//! document at a time; everything else goes to the least-loaded worker.
//!
//! The decision reads the request's text only as far as it must. A request
//! with no `mutation` in it is a read; a mutation is parsed, and it names one
//! document when its operation has a single `update_`, `delete_` or
//! `upsert_` field with a `docID` that is one string, written in place or
//! passed as a variable. Anything else (a create, whose ID its content
//! derives, a list of IDs, a filter, several fields) is routed as unbound
//! work: correctness never depends on placement, since the storage engine
//! validates every commit wherever it runs.

use std::hash::Hasher;

use graphql_parser::query::{Definition, OperationDefinition, Selection, Value};
use serde_json::Value as JsonValue;

use crate::executor::QueryRequest;

/// The mutation fields that act on documents already written, by prefix.
const ONE_DOCUMENT_FIELDS: [&str; 3] = ["update_", "delete_", "upsert_"];

/// The worker a request runs on: its document's when it writes one it
/// names, the least-loaded otherwise.
pub fn request_target(request: &QueryRequest) -> workers::Target {
    one_document(request).map_or(workers::Target::LeastLoaded, |(collection, doc_id)| {
        workers::Target::Key(document_key(&collection, &doc_id))
    })
}

/// The routing key of document `doc_id` of `collection`: the same for every
/// write that names the collection the same way. GraphQL and REST name it by
/// its name, a P2P merge by its ID, so a merge may run on another worker than
/// a local write to the same document; the storage engine validates each
/// commit wherever it runs.
pub fn document_key(collection: &str, doc_id: &str) -> u64 {
    hash_parts(&[collection.as_bytes(), doc_id.as_bytes()])
}

fn hash_parts(parts: &[&[u8]]) -> u64 {
    let mut hasher = rapidhash::fast::RapidHasher::default();
    for part in parts {
        hasher.write(part);
        // A separator no UTF-8 text contains, so ("ab", "c") and ("a", "bc")
        // differ.
        hasher.write_u8(0xff);
    }
    hasher.finish()
}

/// The collection and document a request writes, when it writes exactly one
/// it names.
fn one_document(request: &QueryRequest) -> Option<(String, String)> {
    if !request.query.contains("mutation") {
        return None;
    }
    let document = graphql_parser::parse_query::<String>(&request.query).ok()?;
    let mut operations = document
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::Operation(OperationDefinition::Mutation(mutation)) => Some(mutation),
            _ => None,
        });
    let mutation = match request.operation_name.as_deref() {
        Some(name) => operations.find(|mutation| mutation.name.as_deref() == Some(name))?,
        None => {
            let only = operations.next()?;
            if operations.next().is_some() {
                return None;
            }
            only
        }
    };
    let [Selection::Field(field)] = mutation.selection_set.items.as_slice() else {
        return None;
    };
    let collection = ONE_DOCUMENT_FIELDS
        .iter()
        .find_map(|prefix| field.name.strip_prefix(prefix))?;
    let doc_id = field
        .arguments
        .iter()
        .find(|(name, _)| name == "docID")
        .and_then(|(_, value)| single_id(value, request.variables.as_ref()))?;
    Some((collection.to_owned(), doc_id))
}

/// A `docID` argument that names one document.
fn single_id(value: &Value<'_, String>, variables: Option<&JsonValue>) -> Option<String> {
    match value {
        Value::String(id) => Some(id.clone()),
        Value::Variable(name) => variables?.get(name)?.as_str().map(str::to_owned),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn target(query: &str) -> workers::Target {
        request_target(&QueryRequest::new(query))
    }

    #[test]
    fn a_write_naming_one_document_goes_to_that_documents_worker() {
        let key = workers::Target::Key(document_key("User", "bae-1"));
        assert_eq!(
            target(r#"mutation { update_User(docID: "bae-1", input: {age: 3}) { _docID } }"#),
            key
        );
        assert_eq!(
            target(r#"mutation { delete_User(docID: "bae-1") { _docID } }"#),
            key
        );
        let by_variable = QueryRequest::new(
            r#"mutation ($id: ID) { update_User(docID: $id, input: {age: 3}) { _docID } }"#,
        )
        .with_variables(json!({ "id": "bae-1" }));
        assert_eq!(request_target(&by_variable), key);
        let named = QueryRequest::new(
            r#"mutation A { delete_User(docID: "bae-2") { _docID } }
               mutation B { delete_User(docID: "bae-1") { _docID } }"#,
        )
        .with_operation_name("B");
        assert_eq!(request_target(&named), key);
    }

    #[test]
    fn reads_creates_and_writes_to_many_documents_are_unbound() {
        let unbound = workers::Target::LeastLoaded;
        assert_eq!(target(r#"{ User(docID: "bae-1") { name } }"#), unbound);
        assert_eq!(
            target(r#"mutation { create_User(input: {name: "a"}) { _docID } }"#),
            unbound
        );
        assert_eq!(
            target(r#"mutation { delete_User(docID: ["bae-1", "bae-2"]) { _docID } }"#),
            unbound
        );
        assert_eq!(
            target(r#"mutation { delete_User(filter: {age: {_gt: 3}}) { _docID } }"#),
            unbound
        );
        assert_eq!(
            target(
                r#"mutation { a: delete_User(docID: "bae-1") { _docID } b: delete_User(docID: "bae-2") { _docID } }"#
            ),
            unbound
        );
        assert_eq!(target("mutation { not graphql"), unbound);
    }

    #[test]
    fn the_key_separates_its_parts() {
        assert_ne!(document_key("ab", "c"), document_key("a", "bc"));
        assert_eq!(document_key("User", "bae-1"), document_key("User", "bae-1"));
    }
}
