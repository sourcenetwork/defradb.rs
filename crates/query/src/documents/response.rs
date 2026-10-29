use super::{Error, Result};
use serde_json::Value;

use crate::executor::{QueryResponse, TXN_CONFLICT_ERROR_CODE};

/// The `_docID` of each document mutation `op` on `collection` returned, or
/// the request's first error with its text unchanged.
pub(super) fn doc_ids(response: QueryResponse, op: &str, collection: &str) -> Result<Vec<String>> {
    if let Some(error) = response.errors.first() {
        return Err(match error.code() == Some(TXN_CONFLICT_ERROR_CODE) {
            true => Error::Conflict,
            false => Error::Request(error.message.clone()),
        });
    }
    let key = format!("{op}_{collection}");
    let docs = response
        .data
        .as_ref()
        .and_then(|data| data.get(&key))
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Request(format!("{key} returned no documents")))?;
    Ok(docs
        .iter()
        .filter_map(|doc| doc.get("_docID").and_then(Value::as_str))
        .map(str::to_owned)
        .collect())
}

/// Success when the mutation touched its one document; Go's not-found error
/// when the document does not exist or the caller may not see it.
pub(super) fn one(response: QueryResponse, op: &str, collection: &str) -> Result<()> {
    match doc_ids(response, op, collection)?.is_empty() {
        true => Err(Error::DocumentNotFound),
        false => Ok(()),
    }
}

pub(super) fn single(ids: Vec<String>) -> Result<String> {
    ids.into_iter().next().ok_or(Error::DocumentNotFound)
}
