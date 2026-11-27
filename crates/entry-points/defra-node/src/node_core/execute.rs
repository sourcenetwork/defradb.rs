//! Queries as the caller `context` names, in the transaction it carries if
//! it carries one, and verified client writes staged in an explicit
//! transaction.

use std::sync::Arc;

use defra_core::{RequestContext, TxnId};
use query::{QueryRequest, QueryResponse, TransactionHandle};

use super::NodeCore;
use crate::ops::{acting_identity, request_in};
#[cfg(not(target_arch = "wasm32"))]
use crate::signed_query::{execute_with_signing_context, unavailable_node_signer_response};
use crate::{Result, SignerCredential, VerifiedClientGraph};

impl NodeCore<'_> {
    /// [`Self::execute`] as `context` serves, rather than as the node.
    /// Admitted as a request (see [`Self::shutdown`]).
    pub(crate) async fn execute_as(
        &self,
        context: &RequestContext,
        query_str: &str,
    ) -> QueryResponse {
        // A caller's key signs only once admitted. Without one the node signs
        // with its own key, looked up by the node's DID alone: a key the
        // process holds for the request's DID is not the caller's to use.
        let caller_signer = match self.node.admit(context) {
            Ok(signer) => signer,
            Err(error) => return QueryResponse::error(error.to_string()),
        };
        let runner = Arc::clone(&self.node.runner);
        let mut request = QueryRequest::new(query_str).with_context(context.clone());
        let override_flag = request.context.signing_override();
        let node_signs = self.node.node_identity_did.is_some()
            && override_flag != Some(false)
            && (self.node.node_signs || override_flag == Some(true));
        let signing_config = match caller_signer {
            Some(config) => Some(config),
            None if node_signs => {
                let node_key = defra_core::signing::resolve_signing_config_with_flag(
                    None,
                    self.node.node_identity_did.as_deref(),
                    true,
                );
                if node_key.is_none() {
                    return unavailable_node_signer_response(
                        self.node.node_identity_did.as_deref().unwrap_or("node"),
                    );
                }
                node_key
            }
            None => None,
        };
        match signing_config {
            Some(signing_config) => {
                request.context = request
                    .context
                    .with_signing_session(Some(signing_config.clone()));
                match self.node.network.as_ref() {
                    Some(network) => {
                        crate::signed_query::execute_with_signing_context(
                            network.requests(),
                            runner,
                            request,
                            None,
                            signing_config,
                            None,
                        )
                        .await
                    }
                    None => self
                        .node
                        .as_request(async move { runner.execute(request).await })
                        .await
                        .unwrap_or_else(crate::signed_query::refused_response),
                }
            }
            None => self
                .node
                .as_request(async move { runner.execute(request).await })
                .await
                .unwrap_or_else(crate::signed_query::refused_response),
        }
    }

    /// Run `request` in all of `context` (its identity, bearer, signing, DAC
    /// bypass), in the transaction it carries if it carries one. Errors in
    /// the request are in the response.
    pub(crate) async fn execute_request(
        &self,
        context: &RequestContext,
        request: QueryRequest,
    ) -> Result<QueryResponse> {
        let identity = acting_identity(context)?;
        let request = request_in(context, identity, request);
        Ok(self.execute_request_in(request, context.txn()).await)
    }

    /// Run `request` as its own context names, in transaction `txn` when
    /// there is one.
    pub(crate) async fn execute_request_in(
        &self,
        request: QueryRequest,
        txn: Option<TxnId>,
    ) -> QueryResponse {
        self.node
            .execute_admitted_request(request, txn.map(handle), crate::Admit::Request)
            .await
    }

    /// Stage the client-signed create `graph` in transaction `id` through
    /// `request`, which must name the graph's signer. A refused or
    /// unconfirmed stage discards the transaction.
    pub(crate) async fn execute_verified_create(
        &self,
        request: QueryRequest,
        id: TxnId,
        graph: &db::block::builder::VerifiedClientCreate,
        credential: SignerCredential,
    ) -> QueryResponse {
        self.execute_verified_graph(request, id, graph, credential)
            .await
    }

    /// [`Self::execute_verified_create`] for a client-signed update or
    /// delete.
    pub(crate) async fn execute_verified_mutation(
        &self,
        request: QueryRequest,
        id: TxnId,
        graph: &db::block::builder::VerifiedClientMutation,
        credential: SignerCredential,
    ) -> QueryResponse {
        self.execute_verified_graph(request, id, graph, credential)
            .await
    }

    async fn execute_verified_graph<G>(
        &self,
        mut request: QueryRequest,
        id: TxnId,
        graph: &G,
        credential: SignerCredential,
    ) -> QueryResponse
    where
        G: VerifiedClientGraph,
    {
        let handle = &handle(id);
        if request.parsed.is_some()
            || request.context.dac_bypass()
            || request
                .identity
                .as_ref()
                .map(ToString::to_string)
                .as_deref()
                != Some(graph.signer_did())
        {
            self.discard_quietly(handle).await;
            return QueryResponse::error("verified create actor or DAC context does not match");
        }
        let variables = match &request.variables {
            Some(serde_json::Value::Object(values)) => Some(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            ),
            None => None,
            _ => {
                self.discard_quietly(handle).await;
                return QueryResponse::error("verified create variables must be an object");
            }
        };
        let Ok(query::query_parse::ParsedOperation::Mutation {
            mutations,
            explain: None,
            ..
        }) = query::query_parse::parse_request_with_limits(
            &request.query,
            variables.as_ref(),
            request.operation_name.as_deref(),
            query::QueryLimits::default(),
        )
        else {
            self.discard_quietly(handle).await;
            return QueryResponse::error("verified create requires one matching mutation");
        };
        if mutations.len() != 1 || !graph.matches_mutation(&mutations[0]) {
            self.discard_quietly(handle).await;
            return QueryResponse::error("verified create requires one matching mutation");
        }
        let key = mutations[0].output_name();
        // The graph supplies signatures only; no caller context may carry a
        // separate actor, encryption, bearer, or transaction authority. The
        // one credential is the signer's own, for the document the graph
        // names (proofs/tla/VerifiedRemoteWrite.tla).
        request.context = credential.carried_by(
            defra_core::RequestContext::default()
                .with_transactional_authority_required()
                .with_verified_document(Some(graph.doc_id().to_owned())),
            graph.signer_did(),
        );
        let Some(node_did) = self.node.node_identity_did.as_deref() else {
            self.discard_quietly(handle).await;
            return QueryResponse::error("verified create requires a configured node identity");
        };
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (node_did, key);
            self.discard_quietly(handle).await;
            QueryResponse::error("verified create is unavailable on wasm32")
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(network) = self.node.network.as_ref() else {
                self.discard_quietly(handle).await;
                return unavailable_node_signer_response(node_did);
            };
            let (signing_config, receipt) = graph.signing_config_for_execution();
            let response = execute_with_signing_context(
                network.requests(),
                self.node.runner.clone(),
                request,
                Some(handle.clone()),
                signing_config,
                self.node.node_principal(),
            )
            .await;
            if response.has_errors() {
                self.discard_quietly(handle).await;
                return response;
            }
            let matches = response
                .data
                .as_ref()
                .and_then(|data| data.get(&key))
                .and_then(serde_json::Value::as_array)
                .is_some_and(|rows| {
                    rows.len() == 1
                        && rows[0].get("_docID").and_then(serde_json::Value::as_str)
                            == Some(graph.doc_id())
                });
            if !matches || !receipt.is_complete() {
                // A failed or unconfirmed stage must not leave partial writes available to commit.
                self.discard_quietly(handle).await;
                return QueryResponse::error("verified create was not confirmed");
            }
            response
        }
    }

    /// Discard transaction `handle` as the node, ignoring the outcome: a
    /// refused verified write must leave nothing to commit.
    async fn discard_quietly(&self, handle: &TransactionHandle) {
        let _ = self
            .node
            .runner()
            .discard_txn_as(&self.node.node_context(), handle)
            .await;
    }
}

fn handle(id: TxnId) -> TransactionHandle {
    TransactionHandle::new(id.to_string())
}
