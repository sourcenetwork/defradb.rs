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
mod save_many;
mod types;

use std::future::Future;

pub use error::{Error, Result};
use serde_json::Value;
pub use types::{AddOptions, DocumentsResult, Filter};

use crate::executor::{QueryRequest, QueryResponse};

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

/// Write `doc` over document `doc_id` when it exists, else add it, in one
/// mutation; the document's ID.
pub async fn save<R, F>(
    run: R,
    collection: &str,
    doc_id: Option<&str>,
    doc: &Value,
    options: &AddOptions,
) -> Result<String>
where
    R: FnOnce(String) -> F,
    F: Future<Output = QueryResponse>,
{
    let Some(doc_id) = doc_id else {
        let ids = add(run, collection, std::slice::from_ref(doc), options).await?;
        return response::single(ids);
    };
    let mutation = request::save(collection, doc_id, doc, options)?;
    response::single(response::doc_ids(
        run(mutation).await,
        "upsert",
        collection,
    )?)
}

/// Save every document of `docs` as one request, so in one transaction: one
/// that carries a `_docID` naming a stored document is updated with its other
/// fields, any other is added, and one naming a deleted document fails the
/// batch. Either all are saved or none is; their IDs, in order. The request
/// carries typed mutations, not GraphQL text, and `run` runs it on the
/// caller's request path, which gives it a mutation's preparation, NAC and
/// document ACP checks, signing and encryption.
pub async fn save_many<R, F>(
    run: R,
    collection: &str,
    docs: &[Value],
    options: &AddOptions,
) -> Result<Vec<String>>
where
    R: FnOnce(QueryRequest) -> F,
    F: Future<Output = QueryResponse>,
{
    if docs.is_empty() {
        return Ok(Vec::new());
    }
    let request = QueryRequest::parsed(save_many::operation(collection, docs, options)?);
    response::saved_doc_ids(run(request).await, docs.len())
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
