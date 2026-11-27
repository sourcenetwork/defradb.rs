//! Who a node call acts as and which transaction it runs in, scoped once
//! instead of passed to every call.
//!
//! [`Node`] acts as the node's own principal outside any transaction.
//! [`Node::with_identity`], [`Node::anonymous`], [`Node::with_bearer`],
//! [`Node::with_signing`] and [`Node::with_dac_bypass`] give a [`ScopedNode`]
//! acting as a caller; [`Node::with_transaction`] and [`Node::new_txn`] give a
//! [`TxnNode`] running in an explicit transaction, which offers only the
//! operations that run in one. [`TxnNode::acting_for`] gives an [`ActingFor`]
//! for a trusted embedder's actor in a node-owned transaction. Each scope
//! builds the request context its calls run in and hands it to the node's
//! core.

#[macro_use]
mod ops;

mod acting_for;
mod node;
mod scoped_node;
mod txn_node;

pub use acting_for::ActingFor;
pub use scoped_node::ScopedNode;
pub use txn_node::TxnNode;

use defra_core::signing::SigningConfig;
use defra_core::RequestContext;
use identity::Did;

use crate::Node;

/// What a scope curries into every call: the caller, the bearer token it
/// presents, the key it signs with and whether it reads past document access
/// control.
#[derive(Clone, Default)]
pub(crate) struct Scope {
    caller: Caller,
    bearer: Option<String>,
    /// The bearer is a delegation bound to the one command a request sends
    /// a remote ACP provider.
    bearer_names_request: bool,
    signing: Option<SigningConfig>,
    dac_bypass: Option<bool>,
}

/// Who a call acts as.
#[derive(Clone, Default)]
enum Caller {
    /// The node's own principal ([`Node::node_context`]).
    #[default]
    Node,
    /// No one: an anonymous caller.
    Anonymous,
    /// The given identity.
    Identity(Did),
}

impl Scope {
    fn with_caller(self, caller: Caller) -> Self {
        Self { caller, ..self }
    }

    fn with_identity(self, identity: Did) -> Self {
        self.with_caller(Caller::Identity(identity))
    }

    fn anonymous(self) -> Self {
        self.with_caller(Caller::Anonymous)
    }

    fn with_bearer(self, token: Option<String>) -> Self {
        Self {
            bearer: token,
            bearer_names_request: false,
            ..self
        }
    }

    fn with_request_delegation(self, token: String) -> Self {
        Self {
            bearer: Some(token),
            bearer_names_request: true,
            ..self
        }
    }

    fn with_signing(self, signing: Option<SigningConfig>) -> Self {
        Self { signing, ..self }
    }

    fn with_dac_bypass(self, bypass: bool) -> Self {
        Self {
            dac_bypass: Some(bypass),
            ..self
        }
    }

    /// Whether this is the node's own scope with nothing added: a request
    /// run in it keeps the context the request carries.
    fn is_node_default(&self) -> bool {
        matches!(self.caller, Caller::Node)
            && self.bearer.is_none()
            && self.signing.is_none()
            && self.dac_bypass.is_none()
    }

    /// The context a call in this scope runs in, outside any transaction.
    fn context(&self, node: &Node) -> RequestContext {
        let mut context = match &self.caller {
            Caller::Node => node.node_context(),
            Caller::Identity(identity) => {
                RequestContext::ambient().with_identity(Some(identity.to_string()))
            }
            Caller::Anonymous => RequestContext::ambient(),
        };
        if let Some(token) = &self.bearer {
            let presenter = context.identity().map(str::to_owned);
            context = match (self.bearer_names_request, presenter) {
                (true, Some(presenter)) => {
                    context.with_request_delegation(presenter, token.clone())
                }
                (_, presenter) => context.with_bearer(presenter, Some(token.clone())),
            };
        }
        if self.signing.is_some() {
            context = context.with_signing(self.signing.clone());
        }
        match self.dac_bypass {
            Some(bypass) => context.with_dac_bypass(bypass),
            None => context,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/scope_actor.rs"]
mod tests;
