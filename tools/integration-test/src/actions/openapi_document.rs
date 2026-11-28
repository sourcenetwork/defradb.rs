use axum::body::Bytes;
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

use crate::client_type::{network_edge, ClientType};
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;

/// The most of a response body the in-process client reads.
const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

/// The `http.openapi` op: `GET /openapi.json` from the node, or
/// `GET /openapi.rs.json` with `alias`. It answers
/// `{status, contentType, document}`.
///
/// Only a Rust node serves this document; a Go node answers with Go's own, so
/// `go-http` reports `unsupported`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiDocument {
    /// Fetch the short-lived alias, `/openapi.rs.json`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub alias: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl OpenApiDocument {
    fn path(&self) -> &'static str {
        match self.alias {
            true => "/openapi.rs.json",
            false => "/openapi.json",
        }
    }

    /// Fetches the document over the node's HTTP API. The CLI has no command
    /// for it, Go's neither, so the CLI client reads it from the API too.
    fn fetch(&self, state: &State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        if state.client_type() == ClientType::GoHttp {
            return Err(Failure::unsupported(format!(
                "{} under GoHttp: a Go node answers Go's own document",
                Self::name()
            )));
        }
        let url = format!("{}{}", state.url_on(self.common.node())?, self.path());
        let edge = network_edge();
        let fetched = state
            .block_on(edge.clone().spawn(async move {
                let response = net::http::Client::new(edge)
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                let (status, headers) = (response.status(), response.headers().clone());
                let body = response.bytes().await.map_err(|e| e.to_string())?;
                Ok::<_, String>((status, headers, body))
            }))
            .expect("run on the network edge")
            .map_err(Failure::operation)?;
        answer(fetched)
    }
}

/// What the op answers for one response.
fn answer((status, headers, body): (StatusCode, HeaderMap, Bytes)) -> Result<Value, Failure> {
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let document = serde_json::from_slice::<Value>(&body).map_err(|e| {
        Failure::operation(format!(
            "the document is not JSON ({e}): {}",
            String::from_utf8_lossy(&body)
        ))
    })?;
    Ok(json!({
        "status": status.as_u16(),
        "contentType": content_type,
        "document": document,
    }))
}

impl Op for OpenApiDocument {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    /// The in-process node serves no socket, so the client asks the handler
    /// the node's HTTP API routes the path to.
    fn run(&self, state: &State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        state.node_on(self.common.node())?;
        let response = state.block_on(async {
            match self.alias {
                true => defra_http::handlers::openapi::generated()
                    .await
                    .into_response(),
                false => defra_http::handlers::openapi::get().await.into_response(),
            }
        });
        let (parts, body) = response.into_parts();
        let body = state
            .block_on(axum::body::to_bytes(body, MAX_DOCUMENT_BYTES))
            .map_err(|e| Failure::operation(e.to_string()))?;
        answer((parts.status, parts.headers, body))
    }
}

impl Action for OpenApiDocument {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for OpenApiDocument {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, Self::fetch);
    }
}

impl CliRun for OpenApiDocument {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, Self::fetch);
    }
}
