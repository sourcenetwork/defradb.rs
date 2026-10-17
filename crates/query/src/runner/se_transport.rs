//! Searchable-encryption remote query transport.
//!
//! Injection seam mirroring the `DocumentACP` pattern: the query crate defines
//! a thin trait and the P2P-aware implementation lives in `db-merge` (which can
//! see `p2p`, `crypto`, and `se`). This keeps `query` free of crypto/SE/P2P
//! dependencies while still allowing `execute_encrypted_select` to fan a search
//! out to replicators.
//!
//! Tag generation (HMAC over the SE key + identity + value) happens entirely on
//! the implementation side, so the trait passes raw `(field, json value)` pairs.

use async_trait::async_trait;
use serde_json::Value as JsonValue;

/// Resolves `encrypted_<Collection>` equality queries by fanning search tags
/// out to replicators and returning the matching document IDs.
///
/// Matches Go's `Coordinator.QueryDocIDsByValues`: the querying node is the
/// document owner, which queries its replicators; it never serves from local
/// state. Zero replicators yields an empty result.
#[async_trait]
pub trait SeQueryTransport: Send + Sync {
    /// Query replicators for document IDs whose encrypted index tags match the
    /// given `_eq` conditions on `collection_id` (the collection version ID).
    ///
    /// `eq_conditions` is a list of `(field_name, equality_value)` pairs. The
    /// implementation generates a search tag per condition and intersects the
    /// results. Returns an error string on transport failure.
    async fn query_doc_ids(
        &self,
        collection_id: &str,
        eq_conditions: Vec<(String, JsonValue)>,
    ) -> std::result::Result<Vec<String>, String>;
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct PreparedEncryptedSearch {
    pub collection_id: String,
    pub eq_conditions: Vec<(String, JsonValue)>,
    pub caller_identity: Option<String>,
}

pub(super) async fn query_with_prepared_inputs(
    transport: &dyn SeQueryTransport,
    inputs: Option<&crate::prepared::PreparedInputMemo>,
    input: PreparedEncryptedSearch,
) -> crate::error::Result<Vec<String>> {
    if let Some(inputs) = inputs {
        let request = crate::prepared::PreparedInputRequest::new(
            crate::prepared::PreparedInputKind::EncryptedSearch,
            serde_json::to_vec(
                &serde_json::to_value(&input)
                    .map_err(|error| crate::error::QueryError::execution(error.to_string()))?,
            )
            .map_err(|error| crate::error::QueryError::execution(error.to_string()))?,
        );
        let Some(response) = inputs.get(&request) else {
            return Err(crate::error::QueryError::NeedsInput(request));
        };
        return serde_json::from_slice(&response)
            .map_err(|error| crate::error::QueryError::execution(error.to_string()));
    }
    transport
        .query_doc_ids(&input.collection_id, input.eq_conditions)
        .await
        .map_err(crate::error::QueryError::execution)
}

impl<F: super::DocFetcher + 'static, R: crate::txn::TransactionRegistry> super::QueryRunner<F, R> {
    pub(crate) async fn resolve_encrypted_search_input(
        &self,
        input: &crate::prepared::PreparedInputRequest,
    ) -> crate::error::Result<std::sync::Arc<[u8]>> {
        let transport = self.se_transport.as_ref().ok_or_else(|| {
            crate::error::QueryError::execution("prepared encrypted search has no transport")
        })?;
        let input: PreparedEncryptedSearch = serde_json::from_slice(input.exact_request())
            .map_err(|error| crate::error::QueryError::execution(error.to_string()))?;
        let ids = query_with_prepared_inputs(transport.as_ref(), None, input).await?;
        Ok(serde_json::to_vec(&ids)
            .map_err(|error| crate::error::QueryError::execution(error.to_string()))?
            .into())
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Transport(AtomicUsize);
    #[async_trait]
    impl SeQueryTransport for Transport {
        async fn query_doc_ids(
            &self,
            _: &str,
            _: Vec<(String, JsonValue)>,
        ) -> Result<Vec<String>, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(vec!["doc1".into()])
        }
    }

    #[test]
    fn prepared_encrypted_search_defers_transport_and_keys_query_inputs() {
        workers::block_on(async {
            let transport = Transport(AtomicUsize::new(0));
            let memo = crate::prepared::PreparedInputMemo::default();
            let input = |value: &str| PreparedEncryptedSearch {
                collection_id: "users".into(),
                eq_conditions: vec![("name".into(), JsonValue::String(value.into()))],
                caller_identity: Some("did:key:reader".into()),
            };
            let crate::error::QueryError::NeedsInput(request) =
                query_with_prepared_inputs(&transport, Some(&memo), input("alice"))
                    .await
                    .unwrap_err()
            else {
                panic!("expected a typed input miss");
            };
            assert_eq!(transport.0.load(Ordering::SeqCst), 0);
            let decoded = serde_json::from_slice(request.exact_request()).unwrap();
            let response = query_with_prepared_inputs(&transport, None, decoded)
                .await
                .unwrap();
            memo.insert(request, serde_json::to_vec(&response).unwrap());
            assert_eq!(
                query_with_prepared_inputs(&transport, Some(&memo), input("alice"))
                    .await
                    .unwrap(),
                vec!["doc1"]
            );
            assert!(matches!(
                query_with_prepared_inputs(&transport, Some(&memo), input("bob")).await,
                Err(crate::error::QueryError::NeedsInput(_))
            ));
            assert_eq!(transport.0.load(Ordering::SeqCst), 1);
        })
        .unwrap()
    }
}
