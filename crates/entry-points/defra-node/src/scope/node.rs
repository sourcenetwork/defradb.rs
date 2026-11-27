//! The node's own scope: its principal acting, outside any transaction.

use defra_core::signing::SigningConfig;
use defra_core::{RequestContext, TxnId};
use identity::Did;

use super::{Scope, ScopedNode, TxnNode};
use crate::node_core::NodeCore;
use crate::{Node, Result};

impl Node {
    /// This node acting as `identity`.
    pub fn with_identity(&self, identity: Did) -> ScopedNode<'_> {
        ScopedNode::new(self, Scope::default().with_identity(identity))
    }

    /// This node acting as no one: node and document access control check
    /// its calls as an anonymous caller's, as Go checks a call that names no
    /// identity. Unscoped, [`Node`] acts as its own principal instead.
    pub fn anonymous(&self) -> ScopedNode<'_> {
        ScopedNode::new(self, Scope::default().anonymous())
    }

    /// This node presenting the bearer `token` to a remote document ACP
    /// provider, for the identity the scope acts as; none for `None`.
    pub fn with_bearer<T>(&self, token: T) -> ScopedNode<'_>
    where
        T: Into<Option<String>>,
    {
        ScopedNode::new(self, Scope::default().with_bearer(token.into()))
    }

    /// This node presenting `token` as a delegation bound to the one command
    /// each request sends a remote ACP provider, for the identity the scope
    /// acts as.
    pub fn with_request_delegation(&self, token: String) -> ScopedNode<'_> {
        ScopedNode::new(self, Scope::default().with_request_delegation(token))
    }

    /// This node signing its writes with the caller's own `key`, for the
    /// identity the scope acts as. A key of another identity, or one paired
    /// with a DAC bypass, is refused before anything is written.
    pub fn with_signing(&self, key: SigningConfig) -> ScopedNode<'_> {
        ScopedNode::new(self, Scope::default().with_signing(Some(key)))
    }

    /// This node with document access control bypassed, or not, for its
    /// calls: granted only where node access control lets the scope's
    /// identity bypass it.
    pub fn with_dac_bypass(&self, bypass: bool) -> ScopedNode<'_> {
        ScopedNode::new(self, Scope::default().with_dac_bypass(bypass))
    }

    /// This node running in the explicit transaction `id`, begun elsewhere.
    /// The transaction stays the caller's: dropping the scope leaves it open.
    pub fn with_transaction(&self, id: TxnId) -> TxnNode<'_> {
        TxnNode::external(self, Scope::default(), id)
    }

    /// Begin an explicit transaction owned by the node's principal and
    /// scope this node to it. Dropping the scope unfinalized abandons it.
    /// Begun as a request: a shutting-down node refuses it.
    pub async fn new_txn(&self, read_only: bool) -> Result<TxnNode<'_>> {
        TxnNode::begin(self, Scope::default(), read_only).await
    }

    fn in_scope(&self) -> (NodeCore<'_>, RequestContext) {
        (self.core(), Scope::default().context(self))
    }

    transactional_ops!(forward);
    standalone_ops!(forward);
}
