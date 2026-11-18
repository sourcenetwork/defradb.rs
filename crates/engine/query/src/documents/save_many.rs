//! Save-many as typed mutations: one per document, built as the parser would
//! build it, so no caller value passes through GraphQL text.

use rapidhash::RapidHashMap;
use serde_json::Value;

use super::{AddOptions, Error, Result};
use crate::document::DocumentMapping;
use crate::mapper::{Field, Mutation, Requestable};
use crate::query_parse::ParsedOperation;

/// The response key of document `index`'s mutation.
pub(super) fn alias(index: usize) -> String {
    format!("s{index}")
}

/// The operation saving every document of `docs` in order, as one request:
/// a document carrying a `_docID` is an upsert of that document, one without
/// is an add. An upsert updates a stored document with the other fields and
/// otherwise adds the document under that ID, which a deleted document's ID
/// refuses (`a document with the given ID has been deleted`).
pub(super) fn operation(
    collection: &str,
    docs: &[Value],
    options: &AddOptions,
) -> Result<ParsedOperation> {
    let mutations = docs
        .iter()
        .enumerate()
        .map(|(index, doc)| mutation(collection, index, doc, options))
        .collect::<Result<Vec<_>>>()?;
    Ok(ParsedOperation::Mutation {
        mutations,
        explain: None,
    })
}

fn mutation(collection: &str, index: usize, doc: &Value, options: &AddOptions) -> Result<Mutation> {
    let mut fields: RapidHashMap<String, Value> = doc
        .as_object()
        .ok_or_else(|| Error::Request("a saved document must be an object".to_owned()))?
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    let mutation = match fields.remove("_docID") {
        None => Mutation::create(collection).with_create_input(vec![fields]),
        Some(Value::String(doc_id)) => Mutation::upsert(collection)
            .with_doc_ids(vec![doc_id])
            .with_create_input(vec![fields.clone()])
            .with_update_input(fields),
        Some(_) => return Err(Error::Request("_docID must be a string".to_owned())),
    };
    let mut mapping = DocumentMapping::new();
    mapping.add(0, "_docID");
    mapping.add_render_key(0, "_docID");
    let mut mutation = mutation
        .with_alias(alias(index))
        .with_fields(vec![Requestable::Field(Field::new("_docID"))])
        .with_document_mapping(mapping);
    mutation.encrypt_doc = options.encrypt_doc;
    mutation.encrypt_fields = options.encrypted_fields.clone();
    Ok(mutation)
}
