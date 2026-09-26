//! Shared transaction header handling for HTTP endpoints.

use axum::http::HeaderMap;

use crate::error::HttpError;
use crate::handlers::graphql::TX_HEADER_NAME;

/// Extract the Go-compatible transaction header.
///
/// New HTTP handlers should prefer this header form for client parity. The
/// path-scoped `/tx/{id}/...` endpoints remain available where already exposed.
pub(crate) fn txn_id_from_headers(headers: &HeaderMap) -> Result<Option<&str>, HttpError> {
    let Some(txn_id) = headers.get(TX_HEADER_NAME) else {
        return Ok(None);
    };

    let txn_id = txn_id
        .to_str()
        .map_err(|_| HttpError::BadRequest("invalid transaction id header".to_string()))?;

    if txn_id.is_empty() {
        tracing::debug!("ignoring empty x-defradb-tx header");
        return Ok(None);
    }

    Ok(Some(txn_id))
}

/// Resolve REST operations in the transaction selected by the request.
pub(crate) fn rest_for_request(
    state: &crate::router::AppState,
    headers: &HeaderMap,
) -> Result<std::sync::Arc<dyn query::rest::RestOperations>, HttpError> {
    let rest = state
        .rest
        .as_ref()
        .ok_or_else(|| HttpError::Internal("REST operations not configured".into()))?;
    match txn_id_from_headers(headers)? {
        Some(id) => rest
            .with_transaction(
                id.parse()
                    .map_err(|_| HttpError::BadRequest("invalid transaction id".into()))?,
            )
            .map_err(Into::into),
        None => Ok(rest.clone()),
    }
}
