//! Requests and their GraphQL errors.

use serde_json::Value;

use super::DefraClient;
use crate::error::{Result, WrapErr};

impl DefraClient {
    // ---- Schema operations ----

    /// Deploy a schema via `client collection add '<sdl>'`.
    pub fn schema_add(&self, sdl: &str) -> Result<Value> {
        let out = self.exec(&["client", "collection", "add", sdl])?;
        let json_start = out.find(['{', '[']).unwrap_or(0);
        serde_json::from_str(&out[json_start..]).wrap_err("failed to parse schema_add output")
    }

    /// Execute a GraphQL query/mutation via `client query '<gql>'`.
    pub fn query(&self, gql: &str) -> Result<Value> {
        let out = self.exec(&["client", "query", gql])?;
        self.parse_query_output(&out)
    }

    /// Run a query expected to fail and return the server error message(s).
    ///
    /// The runtimes surface GraphQL errors differently: Rust exits non-zero with
    /// the message on stderr, so `exec` already carries it in the `Err`; Go exits
    /// zero and prints `{"data": null, "errors": [...]}`, which `query` would
    /// discard along with the whole `errors` array. Normalizing both into
    /// `Ok(message)` lets one assertion cover both runtimes.
    pub fn query_expect_error(&self, gql: &str) -> Result<String> {
        match self.exec(&["client", "query", gql]) {
            Err(e) => Ok(e.to_string()),
            Ok(out) => {
                let messages = Self::graphql_error_messages(&out)?;
                if messages.is_empty() {
                    Err(crate::error::err!(
                        "expected query to fail but it succeeded: {}",
                        out.trim()
                    ))
                } else {
                    Ok(messages.join("; "))
                }
            }
        }
    }

    /// Collect `errors[].message` from a query response body.
    pub(super) fn graphql_error_messages(out: &str) -> Result<Vec<String>> {
        // Go CLI prefixes output with "------ Request Results ------\n"
        let json_str = out.find('{').map(|i| &out[i..]).unwrap_or(out);
        let val: Value =
            serde_json::from_str(json_str).wrap_err("failed to parse query error output")?;
        Ok(val
            .get("errors")
            .and_then(Value::as_array)
            .map(|errs| {
                errs.iter()
                    .filter_map(|e| e.get("message").and_then(Value::as_str))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default())
    }

    // ---- Identity-aware query operations ----

    /// Execute a GraphQL query with an identity.
    pub fn query_with_identity(&self, gql: &str, hex_key: &str) -> Result<Value> {
        let out = self.exec(&["client", "-i", hex_key, "query", gql])?;
        self.parse_query_output(&out)
    }

    /// Deploy a schema with identity.
    pub fn schema_add_with_identity(&self, sdl: &str, hex_key: &str) -> Result<Value> {
        let out = self.exec(&["client", "-i", hex_key, "collection", "add", sdl])?;
        let json_start = out.find(['{', '[']).unwrap_or(0);
        serde_json::from_str(&out[json_start..]).wrap_err("failed to parse schema_add output")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphql_error_messages_reads_go_body_behind_header() {
        let out = "------ Request Results ------\n{\"data\":null,\"errors\":[{\"message\":\"a document with the given ID has been deleted. DocID: bae-x\"}]}";
        assert_eq!(
            DefraClient::graphql_error_messages(out).expect("parse"),
            vec!["a document with the given ID has been deleted. DocID: bae-x".to_string()]
        );
    }

    #[test]
    fn graphql_error_messages_empty_on_success_body() {
        let messages =
            DefraClient::graphql_error_messages("{\"data\":{\"foo\":[]}}").expect("parse");
        assert!(messages.is_empty(), "unexpected messages: {:?}", messages);
    }
}
