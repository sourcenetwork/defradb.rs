//! Shared transaction header handling for HTTP endpoints.

use axum::http::HeaderMap;

use crate::error::HttpError;
use crate::handlers::graphql::TX_HEADER_NAME;

/// Extract the Go-compatible transaction header.
///
/// This is how Go carries a transaction on every route.
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

/// The context of the request `identity` makes, running in the transaction
/// the header names, if it names one.
pub(crate) fn txn_context(
    identity: &crate::identity_extractor::ExtractIdentity,
    headers: &HeaderMap,
) -> Result<Option<defra_core::RequestContext>, HttpError> {
    let Some(id) = txn_id_from_headers(headers)? else {
        return Ok(None);
    };
    let id = id
        .parse()
        .map_err(|_| HttpError::BadRequest("invalid transaction id".into()))?;
    Ok(Some(identity.context().with_txn(Some(id))))
}

/// Resolve REST operations serving the request `identity` acts in, in the
/// transaction it selects.
pub(crate) fn rest_for_request(
    state: &crate::router::AppState,
    identity: &crate::identity_extractor::ExtractIdentity,
    headers: &HeaderMap,
) -> Result<std::sync::Arc<dyn query::rest::RestOperations>, HttpError> {
    let rest = crate::request_scope::bound(
        state
            .rest
            .as_ref()
            .ok_or_else(|| HttpError::Internal("REST operations not configured".into()))?,
        &identity.context(),
    );
    match txn_id_from_headers(headers)? {
        Some(id) => rest
            .with_transaction(
                id.parse()
                    .map_err(|_| HttpError::BadRequest("invalid transaction id".into()))?,
            )
            .map_err(Into::into),
        None => Ok(rest),
    }
}
