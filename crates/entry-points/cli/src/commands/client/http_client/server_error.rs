//! A failed response's body as the client's error.

use net::http::StatusCode;

use super::types::ErrorResponse;
use crate::error::Error;

/// The server's `{"error": …}` text unchanged, as Go's client returns it,
/// with the `code` a defradb.rs server adds beside it; or the status and raw
/// body when the body is not that shape.
pub(super) fn server_error(status: StatusCode, body: &str) -> Error {
    match serde_json::from_str::<ErrorResponse>(body) {
        Ok(ErrorResponse {
            error,
            code: Some(code),
        }) => Error::ServerRefused {
            message: error,
            code,
        },
        Ok(ErrorResponse { error, code: None }) => Error::Server(error),
        Err(_) => Error::Server(format!("HTTP {}: {}", status, body.trim())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXISTS: &str = "a document with the given ID already exists. DocID: bae-1";

    #[test]
    fn a_coded_body_keeps_its_code_and_prints_as_go_does() {
        let body = serde_json::json!({"error": EXISTS, "code": "DOCUMENT_ALREADY_EXISTS"});
        let error = server_error(StatusCode::CONFLICT, &body.to_string());
        assert_eq!(error.code(), Some("DOCUMENT_ALREADY_EXISTS"));
        assert_eq!(
            error.to_string(),
            format!("server returned error: {EXISTS}")
        );
    }

    #[test]
    fn a_go_body_has_no_code() {
        let body = serde_json::json!({"error": EXISTS});
        let error = server_error(StatusCode::CONFLICT, &body.to_string());
        assert!(matches!(&error, Error::Server(message) if message == EXISTS));
        assert_eq!(error.code(), None);
    }

    #[test]
    fn another_body_keeps_the_status_and_text() {
        let error = server_error(StatusCode::BAD_GATEWAY, " upstream down ");
        assert!(
            matches!(&error, Error::Server(message) if message == "HTTP 502 Bad Gateway: upstream down")
        );
    }
}
