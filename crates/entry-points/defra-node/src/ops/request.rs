//! Requests that carry their own context: the caller acting, and the
//! explicit transaction they run in.

use defra_core::RequestContext;
use identity::Did;
use query::{QueryRequest, QueryResponse, TransactionError};

use defra_core::signing::SigningConfig;

use crate::embedded_signer::{self, EmbeddedSignerError};
use crate::{Error, Node, Result};

impl Node {
    /// Run `request` in all of `context` as `identity`, in the transaction
    /// `context` carries if it carries one.
    pub(crate) async fn run_as_identity(
        &self,
        context: &RequestContext,
        identity: Option<Did>,
        request: QueryRequest,
    ) -> QueryResponse {
        let request = request_in(context, identity, request);
        self.core().execute_request_in(request, context.txn()).await
    }

    /// Node access control for `permission`, for the operations that check
    /// one before the work they hand on, as Go's do.
    #[cfg(feature = "managed-p2p")]
    pub(crate) async fn check_access(
        &self,
        context: &RequestContext,
        permission: acp::nac::NodePermission,
    ) -> Result<()> {
        let (database, context) = (
            std::sync::Arc::clone(self.database()),
            self.admitted_caller(context)?,
        );
        self.on_pool(move || async move {
            Ok(database.check_node_access_as(&context, permission).await?)
        })
        .await
    }

    /// The key `context` carries, once it may sign for the identity `context`
    /// names: the one admission boundary of the `_as` entries. A refused key
    /// fails the call before it writes anything
    /// (`proofs/tla/EmbeddedSigner.tla`).
    pub(crate) fn admit(
        &self,
        context: &RequestContext,
    ) -> std::result::Result<Option<SigningConfig>, EmbeddedSignerError> {
        embedded_signer::admitted_signer(context.identity(), context)
    }

    /// The caller `context` names, once [`Self::admit`] admits its key.
    /// Outside a transaction a call that names no identity acts as no one,
    /// as Go's in-process client does; inside one it acts as the
    /// transaction's owner, [`Self::txn_owner`].
    pub(crate) fn admitted_caller(&self, context: &RequestContext) -> Result<RequestContext> {
        self.admit(context)?;
        Ok(match context.txn() {
            Some(_) => self.txn_owner(context),
            None => context.clone(),
        })
    }

    /// Who owns the transaction `context` begins or ends: the identity it
    /// names, or else the node's principal, which a request that names none
    /// runs under inside a transaction ([`crate::TxnNode::execute_request`]): the
    /// node on a node that acts as its identity, no one on an identity-only
    /// node.
    pub(crate) fn txn_owner(&self, context: &RequestContext) -> RequestContext {
        match context.identity() {
            Some(_) => context.clone(),
            None => self.node_context().with_txn(context.txn()),
        }
    }
}

/// `request` running in all of `context` (its DAC bypass, encryption, signing,
/// batch session, transaction) as `identity`.
pub(crate) fn request_in(
    context: &RequestContext,
    identity: Option<Did>,
    request: QueryRequest,
) -> QueryRequest {
    let signing_override = request.context.signing_override();
    request
        .with_context(context.clone().with_signing_override(signing_override))
        .with_identity(identity)
}

/// The identity acting in `context`; a malformed one fails.
pub(crate) fn acting_identity(context: &RequestContext) -> Result<Option<Did>> {
    context
        .identity()
        .map(Did::new)
        .transpose()
        .map_err(Error::InvalidActingIdentity)
}

/// A transaction failure with Go's words: only a missing transaction has its
/// own text, and a refused field type keeps its code.
pub(crate) fn transaction_error(error: TransactionError) -> Error {
    match error {
        TransactionError::NotFound(_) => Error::TransactionNotFound,
        conflict @ TransactionError::Conflict(_) => Error::TransactionConflict(conflict),
        TransactionError::UnsupportedFieldType(refusals) => {
            Error::AddCollection(db::AddCollectionError::UnsupportedFieldType(refusals))
        }
        other => Error::Transaction(other),
    }
}
#[cfg(test)]
mod tests {
    use defra_core::{RequestContext, TxnId};
    use query::QueryRequest;

    use super::request_in;

    #[test]
    fn a_request_runs_in_the_whole_context() {
        let context = RequestContext::ambient()
            .with_dac_bypass(true)
            .with_batch_session(Some("session".to_owned()))
            .with_txn(Some(TxnId { start: 0, route: 3 }));

        let request = request_in(&context, None, QueryRequest::new("{ User { name } }"));

        assert!(request.context.dac_bypass());
        assert_eq!(request.context.batch_session(), Some("session"));
        assert_eq!(request.context.txn(), Some(TxnId { start: 0, route: 3 }));
    }
}
