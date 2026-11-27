//! A node running in an explicit transaction.

use defra_core::signing::SigningConfig;
use defra_core::{RequestContext, TxnId};
use identity::Did;
use query::{QueryRequest, QueryResponse};

use super::{ActingFor, Scope};
use crate::node_core::{Begun, NodeCore};
use crate::{Node, Result, SignerCredential};

/// A [`Node`] running in an explicit transaction, from
/// [`Node::with_transaction`] or [`Node::new_txn`]. It offers only the
/// operations that run in a transaction, each acting as the scope's caller,
/// and finalizes the transaction with [`Self::commit`] or [`Self::discard`].
///
/// A scope that began its transaction owns it: dropping the scope
/// unfinalized abandons its writes, including on task cancellation, unless
/// [`Self::detach`] released it first. Cancellation after a durable commit
/// cannot undo it.
#[must_use = "dropping a scope that began its transaction abandons its writes"]
pub struct TxnNode<'n> {
    node: &'n Node,
    scope: Scope,
    id: TxnId,
    /// The transaction, when this scope began it.
    begun: Option<Begun>,
}

impl<'n> TxnNode<'n> {
    pub(super) fn external(node: &'n Node, scope: Scope, id: TxnId) -> Self {
        Self {
            node,
            scope,
            id,
            begun: None,
        }
    }

    pub(super) async fn begin(node: &'n Node, scope: Scope, read_only: bool) -> Result<Self> {
        let begun = node.core().begin(&scope.context(node), read_only).await?;
        Ok(Self {
            node,
            scope,
            id: begun.id,
            begun: Some(begun),
        })
    }

    fn in_scope(&self) -> (NodeCore<'n>, RequestContext) {
        let context = self.scope.context(self.node).with_txn(Some(self.id));
        (self.node.core(), context)
    }

    /// The transaction's ID, which [`Node::with_transaction`] scopes to again.
    pub fn id(&self) -> TxnId {
        self.id
    }

    /// Stop owning the transaction this scope began, leaving it open, and
    /// return its ID: it is finalized later through
    /// [`Node::with_transaction`].
    pub fn detach(self) -> TxnId {
        match self.begun {
            Some(begun) => begun.detach(),
            None => self.id,
        }
    }

    /// This scope acting as `identity` instead, in the same transaction. A
    /// transaction this scope began is still finalized as the caller that
    /// began it.
    pub fn with_identity(self, identity: Did) -> Self {
        Self {
            scope: self.scope.with_identity(identity),
            ..self
        }
    }

    /// This scope acting as no one instead, in the same transaction. See
    /// [`Node::anonymous`] and [`Self::with_identity`].
    pub fn anonymous(self) -> Self {
        Self {
            scope: self.scope.anonymous(),
            ..self
        }
    }

    /// This scope presenting the bearer `token` instead; none for `None`.
    pub fn with_bearer<T>(self, token: T) -> Self
    where
        T: Into<Option<String>>,
    {
        Self {
            scope: self.scope.with_bearer(token.into()),
            ..self
        }
    }

    /// This scope presenting `token` as the caller's request-bound
    /// delegation instead. See [`crate::ScopedNode::with_request_delegation`].
    pub fn with_request_delegation(self, token: String) -> Self {
        Self {
            scope: self.scope.with_request_delegation(token),
            ..self
        }
    }

    /// This scope signing with the caller's own `key` instead, or with none
    /// for `None`. See [`Node::with_signing`].
    pub fn with_signing<K>(self, key: K) -> Self
    where
        K: Into<Option<SigningConfig>>,
    {
        Self {
            scope: self.scope.with_signing(key.into()),
            ..self
        }
    }

    /// This scope with document access control bypassed, or not. See
    /// [`Node::with_dac_bypass`].
    pub fn with_dac_bypass(self, bypass: bool) -> Self {
        Self {
            scope: self.scope.with_dac_bypass(bypass),
            ..self
        }
    }

    /// The node acting for `actor`, an identity the embedding process has
    /// authenticated, in this transaction: the transaction stays the node's,
    /// checked as the node's principal, while native NAC and DAC check
    /// `actor`. A trusted embedder boundary that must not be exposed as HTTP
    /// actor impersonation.
    ///
    /// The actor inherits nothing else from this scope: its bearer, signing
    /// key and DAC bypass belong to this scope's caller, not to `actor`. The
    /// actor presents only what [`ActingFor::with_bearer`] or
    /// [`ActingFor::with_request_delegation`] gives it.
    pub fn acting_for(&self, actor: Did) -> ActingFor<'n> {
        ActingFor::new(self.node, self.id, Scope::default().with_identity(actor))
    }

    /// Make the transaction's writes durable and end it. Admitted as a
    /// request, so a shutting-down node refuses it uncommitted. A begun
    /// transaction commits as the caller that began it, and its commit task
    /// owns it until it completes; one begun elsewhere commits as this
    /// scope's caller.
    pub async fn commit(self) -> Result<()> {
        let core = self.node.core();
        match self.begun {
            Some(begun) => core.commit_begun(begun).await,
            None => core.commit(&self.scope.context(self.node), self.id).await,
        }
    }

    /// Drop the transaction's writes and end it, as [`Self::commit`] names
    /// the caller.
    pub async fn discard(self) -> Result<()> {
        let core = self.node.core();
        match self.begun {
            Some(begun) => core.discard_begun(begun).await,
            None => core.discard(&self.scope.context(self.node), self.id).await,
        }
    }

    /// Run the GraphQL `query` in this transaction, as [`Self::execute_request`]
    /// runs it.
    pub async fn execute(&self, query: &str) -> QueryResponse {
        self.execute_request(QueryRequest::new(query)).await
    }

    /// Run `request` in this transaction. In the node's own scope the
    /// request keeps the identity and context it carries, acting as the node
    /// when it names no one; in any other it runs as the scope's caller, with
    /// its bearer, key and DAC bypass. Errors are in the response.
    pub async fn execute_request(&self, request: QueryRequest) -> QueryResponse {
        let (core, context) = self.in_scope();
        if self.scope.is_node_default() {
            return core.execute_request_in(request, Some(self.id)).await;
        }
        core.execute_request(&context, request)
            .await
            .unwrap_or_else(crate::Error::into_response)
    }

    /// Stage the client-signed create `graph` in this transaction through
    /// `request`, which names the graph's signer; the scope's caller does not
    /// apply. A refused or unconfirmed stage discards the transaction.
    pub async fn execute_verified_create(
        &self,
        request: QueryRequest,
        graph: &db::block::builder::VerifiedClientCreate,
        credential: SignerCredential,
    ) -> QueryResponse {
        self.node
            .core()
            .execute_verified_create(request, self.id, graph, credential)
            .await
    }

    /// [`Self::execute_verified_create`] for a client-signed update or
    /// delete.
    pub async fn execute_verified_mutation(
        &self,
        request: QueryRequest,
        graph: &db::block::builder::VerifiedClientMutation,
        credential: SignerCredential,
    ) -> QueryResponse {
        self.node
            .core()
            .execute_verified_mutation(request, self.id, graph, credential)
            .await
    }

    /// Stage one source-specific node permission in this transaction, as
    /// the scope's caller.
    pub async fn change_nac_source_permission(
        &self,
        edge: &query::txn::SourcePermission,
    ) -> Result<bool> {
        let (core, context) = self.in_scope();
        core.change_nac_source_permission(&context, edge).await
    }

    /// Stage a source-only change as the transaction's authenticated
    /// caller, the scope's. A trusted embedded-host capability, not an HTTP
    /// or GraphQL operation; it needs a configured node identity. Native NAC
    /// checks the caller on the bound view.
    pub async fn change_host_nac_source_permission(
        &self,
        edge: &query::txn::SourcePermission,
    ) -> Result<bool> {
        let (core, context) = self.in_scope();
        core.change_host_nac_source_permission(&context, edge).await
    }

    /// Observe access for the authenticated `actor` on known document IDs,
    /// in this transaction; the scope's caller does not apply. This
    /// privileged in-process API returns no document metadata; its decisions
    /// must not be exposed to untrusted requesters. Local ACP and row reads
    /// share the transaction; remote ACP evaluates its current authority and
    /// cannot share it.
    pub async fn observe_document_access(
        &self,
        collection: &str,
        doc_ids: &[String],
        actor: Did,
        mode: query::owners::DocumentAccessMode,
    ) -> query::error::Result<Vec<query::owners::DocumentAccessDecision>> {
        self.node
            .core()
            .observe_document_access(self.id, collection, doc_ids, actor, mode)
            .await
    }

    /// Read authenticated owners in this transaction, with its rows. The
    /// node remains the transaction actor; a supplied document-read
    /// `identity` follows the ordinary GraphQL permission path. This grants
    /// no relationship or detached ACP read.
    pub async fn observe_document_owners(
        &self,
        collection: &str,
        doc_ids: &[String],
        identity: Option<Did>,
    ) -> query::error::Result<Vec<query::owners::DocumentOwnerObservation>> {
        self.node
            .core()
            .observe_document_owners(self.id, collection, doc_ids, identity)
            .await
    }

    transactional_ops!(forward);
}
