use super::*;
use crate::error::QueryError;

#[test]
fn test_rest_error_display() {
    let err = RestError::collection_not_found("Users");
    assert_eq!(err.to_string(), "collection not found: Users");

    let err = RestError::document_not_found("bae-123");
    assert_eq!(err.to_string(), "document not found: bae-123");

    let err = RestError::invalid_doc_id("invalid");
    assert_eq!(err.to_string(), "invalid document ID: invalid");

    let err = RestError::invalid_input("missing field");
    assert_eq!(err.to_string(), "invalid input: missing field");

    let err = RestError::permission_denied("access denied");
    assert_eq!(err.to_string(), "permission denied: access denied");

    let err = RestError::internal("storage failure");
    assert_eq!(err.to_string(), "internal error: storage failure");
}

#[test]
fn test_rest_error_from_query_error() {
    let err = QueryError::collection_not_found("Users");
    let rest_err: RestError = err.into();
    assert!(matches!(rest_err, RestError::CollectionNotFound(_)));

    let err = QueryError::DocumentNotFound("bae-123".into());
    let rest_err: RestError = err.into();
    assert!(matches!(rest_err, RestError::DocumentNotFound(_)));

    let err = QueryError::parse("unexpected token");
    let rest_err: RestError = err.into();
    assert!(matches!(rest_err, RestError::InvalidInput(_)));
    assert!(rest_err.to_string().contains("parse error"));

    let err = QueryError::invalid_filter("bad condition");
    let rest_err: RestError = err.into();
    assert!(matches!(rest_err, RestError::InvalidInput(_)));
    assert!(rest_err.to_string().contains("invalid filter"));

    let err = QueryError::unknown_field("foo");
    let rest_err: RestError = err.into();
    assert!(matches!(rest_err, RestError::InvalidInput(_)));
    assert!(rest_err.to_string().contains("foo"));

    let err = QueryError::TypeMismatch {
        expected: "String".into(),
        actual: "Int".into(),
    };
    let rest_err: RestError = err.into();
    assert!(matches!(rest_err, RestError::InvalidInput(_)));
    assert!(rest_err.to_string().contains("type mismatch"));

    let err = QueryError::RequiredFieldMissing("name".into());
    let rest_err: RestError = err.into();
    assert!(matches!(rest_err, RestError::InvalidInput(_)));
    assert!(rest_err.to_string().contains("required field missing"));

    let err = QueryError::permission_denied("not authorized");
    let rest_err: RestError = err.into();
    assert!(matches!(rest_err, RestError::PermissionDenied(_)));

    let err = QueryError::acp_registration_failed("bae-123", "policy error");
    let rest_err: RestError = err.into();
    assert!(matches!(rest_err, RestError::PermissionDenied(_)));
    assert!(rest_err.to_string().contains("ACP registration failed"));
}
