//! A node acting as a caller, outside any transaction.

use defra_core::signing::SigningConfig;
use defra_core::{RequestContext, TxnId};
use identity::Did;
use query::{QueryRequest, QueryResponse};

use super::{Scope, TxnNode};
use crate::node_core::NodeCore;
use crate::{Node, Result};

/// A [`Node`] acting as the caller [`Node::with_identity`],
/// [`Node::anonymous`], [`Node::with_bearer`], [`Node::with_signing`] or
/// [`Node::with_dac_bypass`] named. Each operation is [`Node`]'s, acting as
/// that caller.
#[derive(Clone)]
pub struct ScopedNode<'n> {
    node: &'n Node,
    scope: Scope,
}

impl<'n> ScopedNode<'n> {
    pub(super) fn new(node: &'n Node, scope: Scope) -> Self {
        Self { node, scope }
    }

    fn in_scope(&self) -> (NodeCore<'n>, RequestContext) {
        (self.node.core(), self.scope.context(self.node))
    }

    /// This scope acting as `identity` instead.
    #[must_use]
    pub fn with_identity(self, identity: Did) -> Self {
        Self::new(self.node, self.scope.with_identity(identity))
    }

    /// This scope acting as no one instead. See [`Node::anonymous`].
    #[must_use]
    pub fn anonymous(self) -> Self {
        Self::new(self.node, self.scope.anonymous())
    }

    /// This scope presenting the bearer `token` instead; none for `None`.
    #[must_use]
    pub fn with_bearer<T>(self, token: T) -> Self
    where
        T: Into<Option<String>>,
    {
        Self::new(self.node, self.scope.with_bearer(token.into()))
    }

    /// This scope presenting `token` as the caller's delegation bound to the
    /// one command each request sends a remote ACP provider: a provider that
    /// knows such delegations takes it for that command and for nothing
    /// else.
    #[must_use]
    pub fn with_request_delegation(self, token: String) -> Self {
        Self::new(self.node, self.scope.with_request_delegation(token))
    }

    /// This scope signing with the caller's own `key` instead, or with none
    /// for `None`. See [`Node::with_signing`].
    #[must_use]
    pub fn with_signing<K>(self, key: K) -> Self
    where
        K: Into<Option<SigningConfig>>,
    {
        Self::new(self.node, self.scope.with_signing(key.into()))
    }

    /// This scope with document access control bypassed, or not. See
    /// [`Node::with_dac_bypass`].
    #[must_use]
    pub fn with_dac_bypass(self, bypass: bool) -> Self {
        Self::new(self.node, self.scope.with_dac_bypass(bypass))
    }

    /// This scope running in the explicit transaction `id`, begun elsewhere.
    /// See [`Node::with_transaction`].
    pub fn with_transaction(self, id: TxnId) -> TxnNode<'n> {
        TxnNode::external(self.node, self.scope, id)
    }

    /// Begin an explicit transaction owned by this scope's caller and scope
    /// to it. See [`Node::new_txn`].
    pub async fn new_txn(self, read_only: bool) -> Result<TxnNode<'n>> {
        TxnNode::begin(self.node, self.scope, read_only).await
    }

    /// Run the GraphQL `query` as this scope's caller. A caller's key signs
    /// once admitted; without one the node signs with its own key. Errors
    /// are in the response.
    pub async fn execute(&self, query: &str) -> QueryResponse {
        let (core, context) = self.in_scope();
        core.execute_as(&context, query).await
    }

    /// Run `request` in this scope: as its caller, with its bearer, key and
    /// DAC bypass. Errors are in the response.
    pub async fn execute_request(&self, request: QueryRequest) -> QueryResponse {
        let (core, context) = self.in_scope();
        core.execute_request(&context, request)
            .await
            .unwrap_or_else(crate::Error::into_response)
    }

    transactional_ops!(forward);
    standalone_ops!(forward);
}
