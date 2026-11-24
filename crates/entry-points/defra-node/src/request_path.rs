//! The node's request path: what a query needs to run as
//! [`Node::execute_request`](crate::Node::execute_request) runs it, borrowed
//! from the node for each request, or owned by work that outlives its
//! caller's borrow of the node, such as a backup import's driver.

use std::sync::Arc;

use query::{QueryExecutor, QueryRequest, QueryResponse, TransactionHandle};

use crate::{embedded_signer, execute_autocommit_request, signed_query, Admit};

/// The node's request path, borrowed from the node.
#[derive(Clone, Copy)]
pub(crate) struct RequestPath<'a> {
    pub(crate) runner: &'a Arc<dyn QueryExecutor>,
    pub(crate) node_signs: bool,
    pub(crate) node_identity_did: Option<&'a str>,
    pub(crate) node_query_identity: Option<&'a identity::Did>,
    /// The node's request scope; `None` on a node with no network.
    pub(crate) requests: Option<&'a edge::Edge>,
}

/// [`RequestPath`], cloned out of the node once, for work that outlives a
/// borrow of it.
#[derive(Clone)]
pub(crate) struct OwnedRequestPath {
    pub(crate) runner: Arc<dyn QueryExecutor>,
    pub(crate) node_signs: bool,
    pub(crate) node_identity_did: Option<String>,
    pub(crate) node_query_identity: Option<identity::Did>,
    pub(crate) requests: Option<edge::Edge>,
}

impl OwnedRequestPath {
    /// This path, borrowed.
    pub(crate) fn path(&self) -> RequestPath<'_> {
        RequestPath {
            runner: &self.runner,
            node_signs: self.node_signs,
            node_identity_did: self.node_identity_did.as_deref(),
            node_query_identity: self.node_query_identity.as_ref(),
            requests: self.requests.as_ref(),
        }
    }
}

impl RequestPath<'_> {
    /// Runs `request`, admitted as `admit` says, in `txn_handle` if given.
    pub(crate) async fn execute(
        self,
        request: QueryRequest,
        txn_handle: Option<TransactionHandle>,
        admit: Admit,
    ) -> QueryResponse {
        // Read before the node's default fills in the actor: a caller's key
        // signs only for an actor the caller named.
        let actor = request.identity.as_ref().map(ToString::to_string);
        let caller_signer =
            match embedded_signer::admitted_signer(actor.as_deref(), &request.context) {
                Ok(signer) => signer,
                Err(error) => return QueryResponse::error(error.to_string()),
            };
        let mut request = request;
        if request.identity.is_none() {
            request.identity = self.node_query_identity.cloned();
        }
        // A request is signed when the node signs with its identity, when the
        // caller brings its own key, or when the write asks for signing with
        // `enableSigning: true`, as Go's `EnableSigning` does on a node that
        // does not sign. Otherwise an identity-only node runs it unsigned, as
        // its caller, as Go's `NodeIdentity` without `EnableSigning` does.
        // Either way it runs under the node's principal (`node_query_identity`),
        // the owner of the transactions the node begins.
        let signs = self.node_signs
            || caller_signer.is_some()
            || request.context.signing_override() == Some(true);
        let signing_identity = self.node_identity_did.filter(|_| signs);
        let Some(node_identity_did) = signing_identity else {
            if caller_signer.is_some() {
                return QueryResponse::error(
                    "explicit embedded signing requires a configured node identity",
                );
            }
            let runner = Arc::clone(self.runner);
            return self
                .admitted(admit, async move {
                    match txn_handle {
                        Some(handle) => runner.execute_in_txn(request, &handle).await,
                        None => execute_autocommit_request(runner.as_ref(), request).await,
                    }
                })
                .await
                .unwrap_or_else(signed_query::refused_response);
        };

        #[cfg(target_arch = "wasm32")]
        return QueryResponse::error(format!(
            "configured node signing identity {node_identity_did} cannot execute queries on wasm32"
        ));

        #[cfg(not(target_arch = "wasm32"))]
        let signing_config = caller_signer.or_else(|| {
            defra_core::signing::resolve_signing_config_with_flag(
                None,
                Some(node_identity_did),
                true,
            )
        });
        #[cfg(not(target_arch = "wasm32"))]
        let Some(signing_config) = signing_config
        else {
            return signed_query::unavailable_node_signer_response(node_identity_did);
        };
        #[cfg(not(target_arch = "wasm32"))]
        let Some(requests) = self.requests
        else {
            return signed_query::unavailable_node_signer_response(node_identity_did);
        };

        #[cfg(not(target_arch = "wasm32"))]
        signed_query::execute_signed(
            admit,
            requests,
            Arc::clone(self.runner),
            request,
            txn_handle,
            signing_config,
            self.node_query_identity.map(ToString::to_string),
        )
        .await
    }

    /// Runs `work` as a task of the node's request scope, admitted as `admit`
    /// says, with no deadline of its own, so it runs to its end if its caller
    /// stops waiting. A node with no network runs it in place.
    pub(crate) async fn admitted<F>(
        self,
        admit: Admit,
        work: F,
    ) -> Result<F::Output, edge::EdgeError>
    where
        F: std::future::Future + defra_core::thread_bounds::MaybeSend + 'static,
        F::Output: defra_core::thread_bounds::MaybeSend + 'static,
    {
        match self.requests {
            Some(requests) => {
                admit
                    .spawn(requests, work)
                    .within(std::time::Duration::MAX)
                    .await
            }
            None => Ok(work.await),
        }
    }
}
