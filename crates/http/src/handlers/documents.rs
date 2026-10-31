//! Document REST endpoint handlers.
//!
//! Reads go through the REST operations layer. Writes are
//! [`query::documents`], the one implementation every edge calls, run on this
//! server's request path: in the transaction the `x-defradb-tx` header names,
//! else on the request's worker. NAC and ACP are checked in the core.
//!
//! Creation returns document IDs; update and delete return empty success bodies.

use axum::extract::Query;
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use query::documents::{self, AddOptions, DocumentsResult, Filter};
use query::executor::{QueryRequest, QueryResponse};
use query::txn::TransactionHandle;
use serde::Deserialize;
use serde_json::Value as JsonValue;

use crate::error::{http_error_from_backend_message, pool_failure, HttpError};
use crate::handlers::txn_header::{rest_for_request, txn_id_from_headers};
use crate::identity_extractor::ExtractIdentity;
use crate::nac_guard::require_permission;
use crate::query_context::{execute_in_txn_with_context, execute_with_context};
use crate::router::{AppState, NodePermission};

/// Get a single document by ID.
///
/// GET /api/v0/collections/{name}/document/{docID}
///
/// Identity is extracted from the Authorization header and used for ACP checks.
/// Protected documents require read permission.
///
/// Requires `DocumentRead` permission when NAC is enabled.
pub async fn get_document(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Path((collection, doc_id)): Path<(String, String)>,
) -> Result<Json<JsonValue>, HttpError> {
    require_permission(&state, &identity, NodePermission::DocumentRead).await?;

    let rest = rest_for_request(&state, &identity, &headers)?;

    match rest
        .get_document(&collection, &doc_id, identity.did())
        .await
    {
        Ok(Some(doc)) => Ok(Json(doc)),
        Ok(None) => Err(HttpError::NotFound(format!(
            "document not found or not authorized: {}",
            doc_id
        ))),
        Err(e) => {
            tracing::warn!(
                collection = %collection,
                doc_id = %doc_id,
                error = %e,
                "Failed to get document"
            );
            Err(e.into())
        }
    }
}

/// This server's request path for a document write: the request runs as
/// `identity`, in the transaction `headers` names or on its own worker, as
/// the GraphQL route runs it.
struct Server {
    state: AppState,
    identity: ExtractIdentity,
    txn: Option<TransactionHandle>,
}

impl Server {
    fn new(
        state: AppState,
        identity: ExtractIdentity,
        headers: &HeaderMap,
    ) -> Result<Self, HttpError> {
        let txn = txn_id_from_headers(headers)?
            .map(|id| {
                id.parse()
                    .map_err(|_| HttpError::BadRequest("invalid transaction id".into()))
            })
            .transpose()?;
        Ok(Self {
            state,
            identity,
            txn,
        })
    }

    async fn run(self, mutation: String) -> QueryResponse {
        let Self {
            state,
            identity,
            txn,
        } = self;
        let request = QueryRequest::new(mutation).with_identity(identity.did().cloned());
        if let Some(handle) = txn {
            return execute_in_txn_with_context(&state, &identity, request, handle).await;
        }
        let target = query::pooled::request_target(&request);
        let pool = state.pool.clone();
        workers::run_on(pool.as_ref(), target, move || async move {
            execute_with_context(&state, &identity, request).await
        })
        .await
        .unwrap_or_else(|error| QueryResponse::error(pool_failure("documents", error).to_string()))
    }
}

/// A document write's refusal, with Go's status for it and its text unchanged.
fn refused(error: documents::Error) -> HttpError {
    let message = error.to_string();
    match error {
        documents::Error::DocumentNotFound => HttpError::NotFound(message),
        documents::Error::Conflict => HttpError::TransactionConflict,
        documents::Error::Request(_) | documents::Error::SecondaryRelation(_) => {
            http_error_from_backend_message(message)
        }
    }
}

fn json_body(body: &Bytes) -> Result<JsonValue, HttpError> {
    serde_json::from_slice(body).map_err(|e| HttpError::BadRequest(e.to_string()))
}

/// Go's `encrypt` and `encryptFields` query parameters.
#[derive(Debug, Default, Deserialize)]
pub struct AddParams {
    #[serde(default)]
    encrypt: Option<String>,
    #[serde(default, rename = "encryptFields")]
    encrypt_fields: Option<String>,
}

impl AddParams {
    fn options(&self) -> AddOptions {
        AddOptions {
            encrypt_doc: self.encrypt.as_deref() == Some("true"),
            encrypted_fields: self
                .encrypt_fields
                .as_deref()
                .filter(|fields| !fields.is_empty())
                .map(|fields| fields.split(',').map(str::to_owned).collect())
                .unwrap_or_default(),
        }
    }
}

/// Add one document, or an array of them, to a collection.
///
/// POST /api/v0/collections/{name}
///
/// Returns the added document IDs in input order.
pub async fn add_document(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Path(collection): Path<String>,
    Query(params): Query<AddParams>,
    body: Bytes,
) -> Result<Json<Vec<String>>, HttpError> {
    let docs = match json_body(&body)? {
        JsonValue::Array(docs) => docs,
        doc => vec![doc],
    };
    let server = Server::new(state, identity, &headers)?;
    documents::add(|m| server.run(m), &collection, &docs, &params.options())
        .await
        .map(Json)
        .map_err(refused)
}

/// Update a single document.
///
/// PATCH /api/v0/collections/{name}/document/{docID}
///
/// Returns HTTP 200 with an empty body, as Go does.
pub async fn update_document(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Path((collection, doc_id)): Path<(String, String)>,
    body: Bytes,
) -> Result<StatusCode, HttpError> {
    let patch = json_body(&body)?;
    let server = Server::new(state, identity, &headers)?;
    documents::update(|m| server.run(m), &collection, &doc_id, &patch)
        .await
        .map(|()| StatusCode::OK)
        .map_err(refused)
}

/// Delete a single document.
///
/// DELETE /api/v0/collections/{name}/document/{docID}
///
/// Returns HTTP 200 with an empty body, as Go does.
pub async fn delete_document(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Path((collection, doc_id)): Path<(String, String)>,
) -> Result<StatusCode, HttpError> {
    let server = Server::new(state, identity, &headers)?;
    documents::delete(|m| server.run(m), &collection, &doc_id)
        .await
        .map(|()| StatusCode::OK)
        .map_err(refused)
}

/// Go's `DeleteCollectionRequest` and `UpdateCollectionRequest`: the filter
/// is an object or its GraphQL source, and the updater is JSON text.
#[derive(Debug, Deserialize)]
pub struct FilteredRequest {
    #[serde(default)]
    filter: JsonValue,
    #[serde(default)]
    updater: Option<JsonValue>,
}

impl FilteredRequest {
    fn filter(&self) -> Filter {
        match &self.filter {
            JsonValue::Null => Filter::Conditions(serde_json::Map::new()),
            filter => filter.clone().into(),
        }
    }

    /// Go types the updater as a string of JSON; an object is taken as is.
    fn updater(&self) -> String {
        match &self.updater {
            Some(JsonValue::String(updater)) => updater.clone(),
            Some(updater) => updater.to_string(),
            None => String::new(),
        }
    }
}

fn filtered_request(body: &Bytes) -> Result<FilteredRequest, HttpError> {
    serde_json::from_slice(body).map_err(|e| HttpError::BadRequest(e.to_string()))
}

/// Delete every document matching a filter: Go's `DeleteDocumentsWithFilter`.
///
/// DELETE /api/v0/collections/{name}
///
/// Dropping a collection lives on `DELETE /api/v0/collections?name=...`.
pub async fn delete_documents_with_filter(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Path(collection): Path<String>,
    body: Bytes,
) -> Result<Json<DocumentsResult>, HttpError> {
    let request = filtered_request(&body)?;
    let server = Server::new(state, identity, &headers)?;
    documents::delete_with_filter(|m| server.run(m), &collection, &request.filter())
        .await
        .map(Json)
        .map_err(refused)
}

/// Apply an update to every document matching a filter: Go's
/// `UpdateDocumentsWithFilter`.
///
/// PATCH /api/v0/collections/{name}
pub async fn update_documents_with_filter(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Path(collection): Path<String>,
    body: Bytes,
) -> Result<Json<DocumentsResult>, HttpError> {
    let request = filtered_request(&body)?;
    let server = Server::new(state, identity, &headers)?;
    documents::update_with_filter(
        |m| server.run(m),
        &collection,
        &request.filter(),
        &request.updater(),
    )
    .await
    .map(Json)
    .map_err(refused)
}
