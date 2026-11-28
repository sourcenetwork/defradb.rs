//! HTTP error types.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use db::Error as DbError;
use query::error::{QueryError, TransactionError};
use query::executor::TXN_CONFLICT_MESSAGE;
use query::rest::RestError;
use serde::Serialize;
use thiserror::Error;

/// HTTP-layer errors.
#[derive(Debug, Clone, Error)]
#[non_exhaustive]
pub enum HttpError {
    #[error("invalid request: {0}")]
    BadRequest(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("{TXN_CONFLICT_MESSAGE}")]
    TransactionConflict,

    /// A unique index refused the write: a 409 carrying the unique code.
    #[error("conflict: {0}")]
    UniqueViolation(String),

    #[error("unprocessable entity: {0}")]
    UnprocessableEntity(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("storage full: {0}")]
    StorageFull(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("not acceptable: {0}")]
    NotAcceptable(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),

    #[error("query execution failed: {0}")]
    QueryExecution(String),

    /// A bad request whose refusal has a stable code a client classifies it by.
    #[error("{message}")]
    Refused { message: String, code: &'static str },

    /// A create whose document ID is stored: Go's 409, with its code.
    #[error("{0}")]
    DocumentAlreadyExists(String),

    /// A create or upsert of a deleted document: Go's 400, with its code.
    #[error("{0}")]
    DocumentDeleted(String),
}

/// Error response body matching Go DefraDB format.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<&'static str>,
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            HttpError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            HttpError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            HttpError::Conflict(msg) | HttpError::UniqueViolation(msg) => {
                (StatusCode::CONFLICT, msg.clone())
            }
            HttpError::TransactionConflict => {
                (StatusCode::CONFLICT, TXN_CONFLICT_MESSAGE.to_owned())
            }
            HttpError::UnprocessableEntity(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            HttpError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            HttpError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            HttpError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg.clone()),
            HttpError::StorageFull(msg) => (StatusCode::INSUFFICIENT_STORAGE, msg.clone()),
            HttpError::NotAcceptable(msg) => (StatusCode::NOT_ACCEPTABLE, msg.clone()),
            HttpError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            HttpError::NotImplemented(msg) => (StatusCode::NOT_IMPLEMENTED, msg.clone()),
            HttpError::QueryExecution(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            HttpError::Refused { message, .. } => (StatusCode::BAD_REQUEST, message.clone()),
            HttpError::DocumentAlreadyExists(msg) => (StatusCode::CONFLICT, msg.clone()),
            HttpError::DocumentDeleted(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
        };

        let code = match self {
            Self::StorageFull(_) => Some(query::executor::STORAGE_FULL_ERROR_CODE),
            Self::Refused { code, .. } => Some(code),
            Self::UniqueViolation(_) => {
                Some(query::executor::UNIQUE_CONSTRAINT_VIOLATION_ERROR_CODE)
            }
            Self::DocumentAlreadyExists(_) => {
                Some(query::executor::DOCUMENT_ALREADY_EXISTS_ERROR_CODE)
            }
            Self::DocumentDeleted(_) => Some(query::executor::DOCUMENT_DELETED_ERROR_CODE),
            _ => None,
        };
        (
            status,
            Json(ErrorResponse {
                error: message,
                code,
            }),
        )
            .into_response()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorStatus {
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    TransactionConflict,
    UnprocessableEntity,
    ServiceUnavailable,
}

fn http_error_from_status(status: ErrorStatus, message: String) -> HttpError {
    match status {
        ErrorStatus::Unauthorized => HttpError::Unauthorized(message),
        ErrorStatus::Forbidden => HttpError::Forbidden(message),
        ErrorStatus::NotFound => HttpError::NotFound(message),
        ErrorStatus::Conflict => HttpError::Conflict(message),
        ErrorStatus::TransactionConflict => HttpError::TransactionConflict,
        ErrorStatus::UnprocessableEntity => HttpError::UnprocessableEntity(message),
        ErrorStatus::ServiceUnavailable => HttpError::ServiceUnavailable(message),
    }
}

fn message_contains_any(message: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| message.contains(needle))
}

fn classify_backend_message(message: &str) -> Option<ErrorStatus> {
    let message = message.to_ascii_lowercase();
    if message == "transaction conflict. please retry"
        || message.ends_with(": transaction conflict. please retry")
    {
        return Some(ErrorStatus::TransactionConflict);
    }

    // Fallback precedence is intentional: typed errors should be preferred,
    // and ambiguous auth/not-found messages stay privacy-preserving 404s.
    if message_contains_any(
        &message,
        &[
            "p2p disabled",
            "p2p is disabled",
            "database is closed",
            // a remote ACP provider that did not answer: the same call may be
            // made again
            "acp provider unavailable",
            "while the acp provider is unavailable",
        ],
    ) {
        return Some(ErrorStatus::ServiceUnavailable);
    }

    if message_contains_any(
        &message,
        &[
            "not found",
            "does not exist",
            "does not exists",
            "not registered",
        ],
    ) {
        return Some(ErrorStatus::NotFound);
    }

    if message_contains_any(
        &message,
        &[
            "already exists",
            "already registered",
            // enabling NAC that is temporarily disabled: the node's state, not
            // the request, is what stands in the way
            "re-enable it instead",
            "multiple active collection versions",
            "transaction conflict",
            "unique constraint",
            "violates unique index",
        ],
    ) {
        return Some(ErrorStatus::Conflict);
    }

    if message_contains_any(
        &message,
        &[
            "developer mode",
            "nac is disabled",
            "missing permission",
            "missing required permission",
            "permission denied",
        ],
    ) {
        return Some(ErrorStatus::Forbidden);
    }

    if message_contains_any(
        &message,
        &[
            "not authorized to perform operation",
            "unauthorized",
            "not authorized",
            // the request carries no credential the node may act under later
            "no authority to submit acp changes",
        ],
    ) {
        return Some(ErrorStatus::Unauthorized);
    }

    if message_contains_any(
        &message,
        &[
            "acp not available",
            "already disabled",
            "already enabled",
            "can not have policy without acp",
            "cannot delete",
            "can not delete",
            "has a policy",
            "invalid collection name",
            "invalid document",
            "invalid entityset",
            "filtered truncate is not supported",
            "no sets or deletes are allowed in a read-only transaction",
            "invalid lens configuration",
            "invalid patch",
            "invalid policy",
            "invalid relation",
            "materialized view and acp",
            "migration between non-adjacent",
            "not materialized",
            "subject restriction",
            "unsafe policy transition",
        ],
    ) {
        return Some(ErrorStatus::UnprocessableEntity);
    }

    None
}

fn http_error_from_query_error(err: &QueryError) -> HttpError {
    let message = err.to_string();

    match err {
        QueryError::CollectionNotFound(_) | QueryError::DocumentNotFound(_) => {
            HttpError::NotFound(message)
        }
        QueryError::Storage(source) if source.is_not_found() => HttpError::NotFound(message),
        QueryError::Storage(source) if source.is_storage_full() => HttpError::StorageFull(message),
        QueryError::Storage(source) if source.is_txn_conflict() => HttpError::TransactionConflict,
        QueryError::Storage(source) if source.is_unique_constraint_violation() => {
            HttpError::UniqueViolation(message)
        }
        QueryError::TransactionConflict(_) => HttpError::TransactionConflict,
        QueryError::UniqueViolation(_) => HttpError::UniqueViolation(message),
        QueryError::PermissionDenied(_)
        | QueryError::AcpRegistrationFailed { .. }
        | QueryError::AcpCheckFailed { .. }
        | QueryError::AcpRegistrationCheckFailed { .. } => HttpError::Unauthorized(message),
        QueryError::Execution(message) => http_error_from_backend_message(message.clone()),
        QueryError::DocumentAlreadyExists(_) => HttpError::DocumentAlreadyExists(message),
        QueryError::DocumentDeleted(_) => HttpError::DocumentDeleted(message),
        _ => HttpError::BadRequest(message),
    }
}

fn http_status_from_db_error(err: &DbError) -> HttpError {
    let message = err.to_string();

    match err {
        DbError::CollectionNotFound(_)
        | DbError::CollectionVersionNotFound(_)
        | DbError::DocumentNotFound(_)
        | DbError::TransactionNotFound(_) => HttpError::NotFound(message),
        DbError::Storage(source) if source.is_not_found() => HttpError::NotFound(message),
        error if error.is_storage_full() => HttpError::StorageFull(message),
        DbError::CollectionAlreadyExists(_) => HttpError::Conflict(message),
        DbError::Storage(source) if source.is_txn_conflict() => HttpError::TransactionConflict,
        error if error.is_unique_constraint_violation() => HttpError::UniqueViolation(message),
        DbError::InvalidPatch(_)
        | DbError::InvalidDocument(_)
        | DbError::InvalidCollectionName(_)
        | DbError::CollectionVersionIDEmpty
        | DbError::ExplicitTxnMustUseForce
        | DbError::UnsafePolicyTransition(_) => HttpError::UnprocessableEntity(message),
        DbError::DatabaseClosed | DbError::NetworkNotSet => HttpError::ServiceUnavailable(message),
        DbError::Query(source) => http_error_from_query_error(source),
        DbError::Other(message) | DbError::Acp(message) | DbError::Lens(message) => {
            http_error_from_backend_message(message.clone())
        }
        _ => HttpError::BadRequest(message),
    }
}

/// A peer operation's refusal, with Go's status for it and its text unchanged.
pub(crate) fn peer_refused(error: defra_p2p_adapter::peer::Error) -> HttpError {
    http_error_from_backend_message(error.to_string())
}

/// A refused add-collection request: its code when it has one, otherwise the
/// status its message names.
pub(crate) fn collection_refused(refusal: crate::router::CollectionRefusal) -> HttpError {
    match refusal.code {
        Some(code) => HttpError::Refused {
            message: refusal.message,
            code,
        },
        None => http_error_from_backend_message(refusal.message),
    }
}

pub(crate) fn http_error_from_backend_message(message: String) -> HttpError {
    match classify_backend_message(&message) {
        Some(status) => http_error_from_status(status, message),
        None => HttpError::BadRequest(message),
    }
}

/// The answer to a request whose work on the worker pool did not finish: a
/// retry while the pool shuts down, an internal error otherwise (a panic).
pub(crate) fn pool_failure(route: &str, error: workers::Error) -> HttpError {
    let message = error.report(route).into();
    match error {
        workers::Error::ShutDown => HttpError::ServiceUnavailable(message),
        _ => HttpError::Internal(message),
    }
}

impl From<DbError> for HttpError {
    fn from(err: DbError) -> Self {
        http_status_from_db_error(&err)
    }
}

impl From<TransactionError> for HttpError {
    fn from(err: TransactionError) -> Self {
        let message = err.to_string();

        match err {
            TransactionError::NotFound(_) => HttpError::NotFound(message),
            TransactionError::AlreadyFinalized(_) => HttpError::Conflict(message),
            TransactionError::Execution(message) | TransactionError::Operation(message) => {
                http_error_from_backend_message(message)
            }
            TransactionError::Storage(cause) if cause.is_storage_full() => {
                HttpError::StorageFull(message)
            }
            TransactionError::Storage(_) => HttpError::Internal(message),
            TransactionError::Conflict(_) => HttpError::TransactionConflict,
            TransactionError::LockPoisoned(_) | TransactionError::NeedsInput(_) => {
                HttpError::Internal(message)
            }
            TransactionError::NotSupported(_) => HttpError::BadRequest(message),
            TransactionError::UnsupportedFieldType(refusals) => collection_refused(refusals.into()),
        }
    }
}

impl From<RestError> for HttpError {
    fn from(err: RestError) -> Self {
        match err {
            RestError::TransactionConflict => HttpError::TransactionConflict,
            RestError::CollectionNotFound(name) => {
                HttpError::NotFound(format!("Collection '{}' not found", name))
            }
            RestError::DocumentNotFound(id) => {
                HttpError::NotFound(format!("document not found or not authorized: {}", id))
            }
            RestError::InvalidDocId(id) => {
                HttpError::BadRequest(format!("Invalid document ID: {}", id))
            }
            RestError::InvalidInput(msg) => HttpError::BadRequest(msg),
            RestError::PermissionDenied(msg) => HttpError::Unauthorized(msg),
            RestError::Internal(msg) => match classify_backend_message(&msg) {
                Some(status) => http_error_from_status(status, msg),
                None => HttpError::Internal(msg),
            },
        }
    }
}

pub type Result<T> = std::result::Result<T, HttpError>;

#[cfg(test)]
mod tests {
    use super::*;

    fn status(error: HttpError) -> StatusCode {
        error.into_response().status()
    }

    /// What staging a relationship change for a remote provider refuses
    /// with: no credential to send it under later is an authentication
    /// failure, and a provider that cannot be asked is one to retry.
    #[test]
    fn staging_refusals_map_to_authentication_and_retry() {
        assert!(matches!(
            http_error_from_backend_message(
                "no authority to submit ACP changes as did:key:z6Mk: no delegation".into()
            ),
            HttpError::Unauthorized(_)
        ));
        assert!(matches!(
            http_error_from_backend_message(
                "cannot check did:key:z6Mk's authority to revoke on bae-1 while the ACP provider is unavailable; try again".into()
            ),
            HttpError::ServiceUnavailable(_)
        ));
        assert!(matches!(
            http_error_from_backend_message("ACP provider unavailable: timed out".into()),
            HttpError::ServiceUnavailable(_)
        ));
    }

    #[test]
    fn http_error_variants_map_to_status_codes() {
        let cases = [
            (HttpError::BadRequest("bad".into()), StatusCode::BAD_REQUEST),
            (
                HttpError::Unauthorized("auth".into()),
                StatusCode::UNAUTHORIZED,
            ),
            (HttpError::Forbidden("forbid".into()), StatusCode::FORBIDDEN),
            (HttpError::NotFound("missing".into()), StatusCode::NOT_FOUND),
            (HttpError::Conflict("conflict".into()), StatusCode::CONFLICT),
            (
                HttpError::UnprocessableEntity("semantic".into()),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                HttpError::ServiceUnavailable("down".into()),
                StatusCode::SERVICE_UNAVAILABLE,
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(status(error), expected);
        }
    }

    #[test]
    fn db_errors_map_to_status_buckets() {
        let cases = [
            (
                DbError::Acp("not authorized to perform operation".into()),
                StatusCode::UNAUTHORIZED,
            ),
            (
                DbError::Acp("resource is missing required permission".into()),
                StatusCode::FORBIDDEN,
            ),
            (
                DbError::CollectionNotFound("Users".into()),
                StatusCode::NOT_FOUND,
            ),
            (
                DbError::CollectionAlreadyExists("Users".into()),
                StatusCode::CONFLICT,
            ),
            (
                DbError::Acp(format!(
                    "failed to enable NAC: {}",
                    acp::error::Error::NacTemporarilyDisabled
                )),
                StatusCode::CONFLICT,
            ),
            (
                DbError::Storage(storage::Error::TxnConflict),
                StatusCode::CONFLICT,
            ),
            (
                DbError::UnsafePolicyTransition("collection policy changed".into()),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (DbError::DatabaseClosed, StatusCode::SERVICE_UNAVAILABLE),
            (DbError::Other("unknown".into()), StatusCode::BAD_REQUEST),
        ];

        for (error, expected) in cases {
            assert_eq!(status(HttpError::from(error)), expected);
        }
    }

    #[test]
    fn rest_document_not_found_maps_to_404() {
        assert_eq!(
            status(HttpError::from(RestError::DocumentNotFound(
                "bae-123".into()
            ))),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn backend_message_precedence_keeps_ambiguous_auth_not_found_as_404() {
        assert_eq!(
            status(http_error_from_backend_message(
                "document not found or not authorized".into()
            )),
            StatusCode::NOT_FOUND
        );
    }

    #[nuclei::test]
    async fn transaction_conflicts_preserve_the_client_retry_contract() {
        let errors = [
            HttpError::from(DbError::Storage(storage::Error::TxnConflict)),
            http_error_from_query_error(&QueryError::transaction_conflict("diagnostic context")),
            HttpError::from(RestError::from(QueryError::transaction_conflict(
                "write aborted",
            ))),
            HttpError::from(TransactionError::execution(
                "commit error: datastore error: storage error: transaction conflict. Please retry",
            )),
        ];
        for error in errors {
            let response = error.into_response();
            assert_eq!(response.status(), StatusCode::CONFLICT);
            let body = axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap();
            let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(value, serde_json::json!({"error": TXN_CONFLICT_MESSAGE}));
        }
        assert!(matches!(
            http_error_from_backend_message("unique constraint violation".into()),
            HttpError::Conflict(_)
        ));
        assert!(matches!(
            http_error_from_backend_message("transaction conflict has another cause".into()),
            HttpError::Conflict(_)
        ));
    }

    #[test]
    fn transaction_errors_map_to_status_buckets() {
        let cases = [
            (TransactionError::not_found("1"), StatusCode::NOT_FOUND),
            (
                TransactionError::execution(
                    "No sets or deletes are allowed in a read-only transaction",
                ),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                TransactionError::already_finalized("1"),
                StatusCode::CONFLICT,
            ),
            (
                TransactionError::execution("transaction conflict. Please retry"),
                StatusCode::CONFLICT,
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(status(HttpError::from(error)), expected);
        }
    }

    #[test]
    fn transaction_schema_errors_keep_their_source_context() {
        workers::block_on(async {
            let error = HttpError::from(TransactionError::operation(
                "schema error: duplicate field. Name: _docID",
            ));
            let response = error.into_response();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);

            let body = axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap();
            let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(
                value,
                serde_json::json!({
                    "error": "schema error: duplicate field. Name: _docID"
                })
            );
        })
        .unwrap()
    }
}

#[cfg(test)]
mod storage_failure_tests {
    use super::*;

    #[test]
    fn transaction_storage_full_is_structured_and_not_a_conflict() {
        workers::block_on(async {
            let cause = storage::Error::from(std::io::Error::new(
                std::io::ErrorKind::StorageFull,
                "localized failure",
            ));
            let response = HttpError::from(TransactionError::Storage(cause)).into_response();
            assert_eq!(response.status(), StatusCode::INSUFFICIENT_STORAGE);
            let bytes = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(body["code"], query::executor::STORAGE_FULL_ERROR_CODE);
        })
        .unwrap();
    }

    #[test]
    fn a_coded_collection_refusal_is_a_bad_request_with_its_code() {
        workers::block_on(async {
            let refusal = crate::router::CollectionRefusal {
                message: "NonNull fields are not currently supported".into(),
                code: Some(query::sdl_parse::UNSUPPORTED_FIELD_TYPE_ERROR_CODE),
            };
            let response = collection_refused(refusal).into_response();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                body,
                serde_json::json!({
                    "error": "NonNull fields are not currently supported",
                    "code": "UNSUPPORTED_FIELD_TYPE",
                })
            );
        })
        .unwrap();
    }

    #[test]
    fn other_http_errors_keep_the_existing_wire_shape() {
        let response = ErrorResponse {
            error: "failed".into(),
            code: None,
        };
        assert_eq!(
            serde_json::to_value(response).unwrap(),
            serde_json::json!({"error": "failed"})
        );
        assert!(matches!(
            HttpError::from(TransactionError::Storage(storage::Error::Backend(
                "No space left on device".into()
            ))),
            HttpError::Internal(_)
        ));
    }

    /// The status, `error` and `code` an error answers with.
    fn wire(error: HttpError) -> (StatusCode, serde_json::Value) {
        workers::block_on(async {
            let response = error.into_response();
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            (status, serde_json::from_slice(&bytes).unwrap())
        })
        .unwrap()
    }

    const VIOLATION: &str = "can not index a doc's field(s) that violates unique index";

    #[test]
    fn a_unique_violation_naming_its_writer_is_a_conflict_with_its_code() {
        let (status, body) = wire(HttpError::from(DbError::UniqueViolation(VIOLATION.into())));
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], VIOLATION);
        assert_eq!(
            body["code"],
            query::executor::UNIQUE_CONSTRAINT_VIOLATION_ERROR_CODE
        );
    }

    #[test]
    fn a_query_unique_violation_is_a_conflict_with_its_code() {
        let error = DbError::Query(QueryError::UniqueViolation(VIOLATION.into()));
        let (status, body) = wire(HttpError::from(error));
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], VIOLATION);
        assert_eq!(
            body["code"],
            query::executor::UNIQUE_CONSTRAINT_VIOLATION_ERROR_CODE
        );
    }

    #[test]
    fn a_collection_conflict_carries_no_code() {
        let (status, body) = wire(HttpError::Conflict("collection already exists".into()));
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body.get("code").is_none(), "{body}");
    }
}

#[cfg(test)]
mod document_collision_tests {
    use super::*;

    const ID: &str = "bae-e761da93-66da-577d-a503-54047c4cf3ea";

    fn answer(error: HttpError) -> (StatusCode, serde_json::Value) {
        let response = error.into_response();
        let status = response.status();
        let body = workers::block_on(async {
            axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap()
        })
        .unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[test]
    fn an_existing_document_is_a_409_with_its_code_and_go_text() {
        let error = DbError::Query(QueryError::DocumentAlreadyExists(ID.into()));
        let (status, body) = answer(HttpError::from(error));
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(
            body["error"],
            format!("a document with the given ID already exists. DocID: {ID}")
        );
        assert_eq!(
            body["code"],
            query::executor::DOCUMENT_ALREADY_EXISTS_ERROR_CODE
        );
    }

    #[test]
    fn a_deleted_document_is_a_400_with_its_code_and_go_text() {
        let error = DbError::Query(QueryError::DocumentDeleted(ID.into()));
        let (status, body) = answer(HttpError::from(error));
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body["error"],
            format!("a document with the given ID has been deleted. DocID: {ID}")
        );
        assert_eq!(body["code"], query::executor::DOCUMENT_DELETED_ERROR_CODE);
    }

    /// The statuses the uncoded text got before the typed refusals, which the
    /// typed ones keep.
    #[test]
    fn the_untyped_texts_kept_the_same_statuses() {
        let existing = format!("a document with the given ID already exists. DocID: {ID}");
        let deleted = format!("a document with the given ID has been deleted. DocID: {ID}");
        assert_eq!(
            answer(http_error_from_backend_message(existing)).0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            answer(http_error_from_backend_message(deleted)).0,
            StatusCode::BAD_REQUEST
        );
    }
}
