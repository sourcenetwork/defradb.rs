//! Tests for the HTTP client

use cli::commands::client::http_client::{
    GraphQLError, GraphQLRequest, GraphQLResponse, HttpClient,
};
use core::sync::atomic::{AtomicUsize, Ordering};
use net::http::StatusCode;
use std::io::{Read, Write};
use std::net::TcpListener;

#[test]
fn test_http_client_new() {
    let client = HttpClient::new("http://localhost:9181/", &edge::testing::edge()).unwrap();
    assert_eq!(client.base_url(), "http://localhost:9181");
}

#[test]
fn test_http_client_new_invalid_url() {
    let result = HttpClient::new("not-a-valid-url", &edge::testing::edge());
    assert!(result.is_err());
}

#[test]
fn test_graphql_request_serialization() {
    let request = GraphQLRequest {
        query: "{ Users { name } }".to_string(),
        variables: None,
        operation_name: None,
        txn_id: None,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("query"));
    assert!(!json.contains("variables"));
}

#[test]
fn test_graphql_request_with_txn_id() {
    let request = GraphQLRequest {
        query: "{ Users { name } }".to_string(),
        variables: None,
        operation_name: None,
        txn_id: Some("12345".to_string()),
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"txn_id\":\"12345\""));
}

#[test]
fn test_graphql_response_has_errors() {
    let response = GraphQLResponse {
        data: None,
        errors: vec![GraphQLError {
            message: "error".to_string(),
            extensions: None,
        }],
        extensions: None,
    };
    assert!(response.has_errors());
    assert_eq!(response.error_message(), "error");
}

#[test]
fn test_graphql_response_no_errors() {
    let response = GraphQLResponse {
        data: Some(serde_json::json!({})),
        errors: vec![],
        extensions: None,
    };
    assert!(!response.has_errors());
}

#[test]
fn test_is_retryable_status_service_unavailable() {
    assert!(HttpClient::is_retryable_status(
        StatusCode::SERVICE_UNAVAILABLE
    ));
}

#[test]
fn test_is_retryable_status_too_many_requests() {
    assert!(HttpClient::is_retryable_status(
        StatusCode::TOO_MANY_REQUESTS
    ));
}

#[test]
fn test_is_retryable_status_internal_server_error() {
    assert!(HttpClient::is_retryable_status(
        StatusCode::INTERNAL_SERVER_ERROR
    ));
}

#[test]
fn test_is_retryable_status_bad_gateway() {
    assert!(HttpClient::is_retryable_status(StatusCode::BAD_GATEWAY));
}

#[test]
fn test_is_retryable_status_gateway_timeout() {
    assert!(HttpClient::is_retryable_status(StatusCode::GATEWAY_TIMEOUT));
}

#[test]
fn test_is_retryable_status_request_timeout() {
    assert!(HttpClient::is_retryable_status(StatusCode::REQUEST_TIMEOUT));
}

#[test]
fn test_is_not_retryable_bad_request() {
    assert!(!HttpClient::is_retryable_status(StatusCode::BAD_REQUEST));
}

#[test]
fn test_is_not_retryable_not_found() {
    assert!(!HttpClient::is_retryable_status(StatusCode::NOT_FOUND));
}

#[test]
fn test_is_not_retryable_unauthorized() {
    assert!(!HttpClient::is_retryable_status(StatusCode::UNAUTHORIZED));
}

#[test]
fn test_is_not_retryable_forbidden() {
    assert!(!HttpClient::is_retryable_status(StatusCode::FORBIDDEN));
}

#[test]
fn test_is_not_retryable_ok() {
    assert!(!HttpClient::is_retryable_status(StatusCode::OK));
}

#[test]
fn test_retry_constants() {
    // Verify retry configuration values are sensible
    assert_eq!(cli::commands::client::http_client::MAX_RETRIES, 3);
    assert_eq!(cli::commands::client::http_client::INITIAL_BACKOFF_MS, 100);
    assert!(cli::commands::client::http_client::RETRYABLE_STATUS_CODES.contains(&503));
    assert!(cli::commands::client::http_client::RETRYABLE_STATUS_CODES.contains(&429));
    assert!(cli::commands::client::http_client::RETRYABLE_STATUS_CODES.contains(&500));
}

/// A write is sent again only on a status that refused it before it ran: a
/// 500, 502 or 504 may follow a commit, and a 408 leaves the request running
/// on the server (`defra.txn.commit_outcome.no_resend`).
#[test]
fn a_write_is_not_resent_after_a_status_that_may_follow_its_commit() {
    for status in [
        StatusCode::REQUEST_TIMEOUT,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::BAD_GATEWAY,
        StatusCode::GATEWAY_TIMEOUT,
    ] {
        assert!(HttpClient::may_resend("GET", status), "{status}");
        for method in ["POST", "PATCH", "DELETE"] {
            assert!(!HttpClient::may_resend(method, status), "{method} {status}");
        }
    }
    for status in [
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::SERVICE_UNAVAILABLE,
    ] {
        assert!(HttpClient::may_resend("POST", status), "{status}");
    }
}

/// A server on `127.0.0.1` that answers every request with `reply`, or closes
/// the connection unanswered when there is none, and counts the requests. The
/// server lives as long as the test process, so its counter does too.
fn canned_server(reply: Option<String>) -> (String, &'static AtomicUsize) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let hits: &'static AtomicUsize = Box::leak(Box::new(AtomicUsize::new(0)));
    let counted = hits;
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            while !request.windows(4).any(|end| end == b"\r\n\r\n") {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => request.extend_from_slice(&buf[..read]),
                }
            }
            counted.fetch_add(1, Ordering::SeqCst);
            if let Some(reply) = &reply {
                let _ = stream.write_all(reply.as_bytes());
            }
        }
    });
    (url, hits)
}

fn reply(status: &str, body: &str) -> Option<String> {
    Some(format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ))
}

/// The error a commit answered with `answer` ends in, and how many times the
/// commit was sent.
fn commit_against(answer: Option<String>) -> (cli::error::Error, usize) {
    let (url, hits) = canned_server(answer);
    let edge = edge::testing::edge();
    let client = HttpClient::new(&url, &edge).unwrap();
    let error = edge
        .block_on(client.tx_commit("123"))
        .unwrap()
        .expect_err("the commit failed");
    (error, hits.load(Ordering::SeqCst))
}

/// A commit is sent once unless the server refused it before it ran, and a
/// failure with no outcome the server vouched for is `indeterminate`: a 408
/// (the server keeps running a request whose wait timed out), a 5xx with no
/// outcome, or a connection that closed after the request was sent
/// (`defra.txn.commit_outcome.no_resend`, `defra.txn.commit_outcome.indeterminate`).
#[test]
fn a_commit_with_no_readable_answer_is_sent_once_and_indeterminate() {
    let indeterminate = Some(defra_core::CommitOutcome::Indeterminate);
    for (answer, outcome, sent, text) in [
        (
            reply("408 Request Timeout", ""),
            indeterminate,
            1,
            "server returned error: HTTP 408 Request Timeout:",
        ),
        (
            reply("500 Internal Server Error", ""),
            indeterminate,
            1,
            "server returned error: HTTP 500 Internal Server Error:",
        ),
        (
            reply("500 Internal Server Error", r#"{"error":"boom"}"#),
            indeterminate,
            1,
            "server returned error: boom",
        ),
        (
            reply("502 Bad Gateway", r#"{"error":"upstream"}"#),
            indeterminate,
            1,
            "server returned error: upstream",
        ),
        (
            reply(
                "500 Internal Server Error",
                r#"{"error":"refused","outcome":"aborted"}"#,
            ),
            Some(defra_core::CommitOutcome::NotCommitted),
            1,
            "server returned error: refused",
        ),
        (
            reply("400 Bad Request", r#"{"error":"bad"}"#),
            None,
            1,
            "server returned error: bad",
        ),
        (
            reply("503 Service Unavailable", r#"{"error":"busy"}"#),
            None,
            1,
            "server returned error: busy",
        ),
        (
            reply(
                "503 Service Unavailable\r\nx-defradb-outcome: indeterminate",
                r#"{"error":"node is shutting down","outcome":"indeterminate"}"#,
            ),
            indeterminate,
            1,
            "server returned error: node is shutting down",
        ),
        (None, indeterminate, 1, "HTTP request failed:"),
    ] {
        let label = format!("{answer:?}");
        let (error, hits) = commit_against(answer);
        let label = format!("{label}: {error}");
        assert_eq!(hits, sent, "{label}");
        assert_eq!(error.commit_outcome(), outcome, "{label}");
        assert!(error.to_string().starts_with(text), "{label}");
    }
}

/// A read keeps its retries and says nothing about a commit.
#[test]
fn a_read_is_resent_after_a_server_error_and_has_no_outcome() {
    let (url, hits) = canned_server(reply("500 Internal Server Error", r#"{"error":"boom"}"#));
    let edge = edge::testing::edge();
    let client = HttpClient::new(&url, &edge).unwrap();
    let error = edge
        .block_on(client.schema())
        .unwrap()
        .expect_err("the read failed");
    assert_eq!(hits.load(Ordering::SeqCst), 4);
    assert_eq!(error.commit_outcome(), None);
}
