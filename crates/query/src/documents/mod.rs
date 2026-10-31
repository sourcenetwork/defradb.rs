//! Document writes, implemented once for every edge.
//!
//! Each write is a GraphQL mutation, so it takes the same preparation, NAC
//! and ACP checks, signing, encryption and write pipeline as any mutation.
//! The caller hands over only its request path (`run`): the in-process node,
//! the HTTP server and FFI each run the mutation with their own context, and
//! what the write means and answers is decided here.

mod error;
mod request;
mod response;
mod types;

use std::future::Future;

pub use error::{Error, Result};
use serde_json::Value;
pub use types::{AddOptions, DocumentsResult, Filter};

use crate::executor::QueryResponse;

/// Add `docs` to `collection` and return their IDs in order.
pub async fn add<R, F>(
    run: R,
    collection: &str,
    docs: &[Value],
    options: &AddOptions,
) -> Result<Vec<String>>
where
    R: FnOnce(String) -> F,
    F: Future<Output = QueryResponse>,
{
    let mutation = request::add(collection, docs, options)?;
    response::doc_ids(run(mutation).await, "add", collection)
}

/// Set the fields `patch` names on document `doc_id`.
pub async fn update<R, F>(run: R, collection: &str, doc_id: &str, patch: &Value) -> Result<()>
where
    R: FnOnce(String) -> F,
    F: Future<Output = QueryResponse>,
{
    let mutation = request::update(collection, doc_id, patch)?;
    response::one(run(mutation).await, "update", collection)
}

/// Go's `SaveDocument`: refuse document `doc_id` when it has been deleted,
/// else write `doc` over it when it exists, else add it; the document's ID.
/// `run` must run every request in one transaction, so the refusal and the
/// write decide on the same state (proofs/tla/DocumentSave.tla).
pub async fn save<R, F>(
    run: R,
    collection: &str,
    doc_id: Option<&str>,
    doc: &Value,
    options: &AddOptions,
) -> Result<String>
where
    R: Fn(String) -> F,
    F: Future<Output = QueryResponse>,
{
    let Some(doc_id) = doc_id else {
        let ids = add(run, collection, std::slice::from_ref(doc), options).await?;
        return response::single(ids);
    };
    if response::deleted(run(request::lookup(collection, doc_id)?).await, collection)? {
        return Err(Error::Deleted(doc_id.to_owned()));
    }
    let mutation = request::save(collection, doc_id, doc, options)?;
    response::single(response::doc_ids(
        run(mutation).await,
        "upsert",
        collection,
    )?)
}

/// Delete document `doc_id`.
pub async fn delete<R, F>(run: R, collection: &str, doc_id: &str) -> Result<()>
where
    R: FnOnce(String) -> F,
    F: Future<Output = QueryResponse>,
{
    let mutation = request::delete(collection, doc_id)?;
    response::one(run(mutation).await, "delete", collection)
}

/// Apply `updater`, a JSON object of fields, to every document `filter`
/// matches.
pub async fn update_with_filter<R, F>(
    run: R,
    collection: &str,
    filter: &Filter,
    updater: &str,
) -> Result<DocumentsResult>
where
    R: FnOnce(String) -> F,
    F: Future<Output = QueryResponse>,
{
    let mutation = request::update_with_filter(collection, filter, updater)?;
    response::doc_ids(run(mutation).await, "update", collection).map(Into::into)
}

/// Delete every document `filter` matches.
pub async fn delete_with_filter<R, F>(
    run: R,
    collection: &str,
    filter: &Filter,
) -> Result<DocumentsResult>
where
    R: FnOnce(String) -> F,
    F: Future<Output = QueryResponse>,
{
    let mutation = request::delete_with_filter(collection, filter)?;
    response::doc_ids(run(mutation).await, "delete", collection).map(Into::into)
}

#[cfg(test)]
mod tests;
