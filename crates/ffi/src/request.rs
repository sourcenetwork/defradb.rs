//! What every store call starts with: its handles resolved, the caller
//! admitted by node access control, and the context the call acts in.

use std::sync::Arc;

use acp::nac::NodePermission;

use crate::state::{FfiDatabase, NODES};
use crate::target::{resolve_caller, resolve_target, Caller, Target, ERR_INVALID_HANDLE};
use crate::types::DefraResult;

/// A store call's resolved handles.
pub(crate) struct Request {
    pub(crate) target: Target,
    /// The caller's DID; `None` for no identity.
    pub(crate) identity: Option<String>,
    /// The caller, with the bearer token it presents, if any.
    caller: Caller,
}

impl Request {
    /// Resolves `node_ptr` (a node or a transaction) and `identity_ptr`, and
    /// admits the caller for `permission`.
    pub(crate) fn begin(
        node_ptr: usize,
        identity_ptr: usize,
        permission: NodePermission,
    ) -> Result<Self, DefraResult> {
        let request = Self::resolve(node_ptr, identity_ptr)?;
        crate::nac_check::check(request.node(), request.identity.as_deref(), permission)?;
        Ok(request)
    }

    /// Resolves `node_ptr` and `identity_ptr`, for a call that decides node
    /// access itself (the node access control calls).
    pub(crate) fn resolve(node_ptr: usize, identity_ptr: usize) -> Result<Self, DefraResult> {
        let target = resolve_target(node_ptr)?;
        let caller = resolve_caller(identity_ptr)?;
        let request = Self {
            target,
            identity: caller.did.clone(),
            caller,
        };
        // A synchronous call waits here, on the caller's thread, for its turn
        // on a transaction: every call made on it before runs first. The
        // request holds the turn until it is dropped, when the call ends.
        if let Target::Txn(call) = &request.target {
            crate::call::wait(async {
                call.turn().await;
                Ok(())
            })
            .map_err(DefraResult::error)?;
        }
        Ok(request)
    }

    /// `context` presenting the caller's bearer token, if it has one.
    pub(crate) fn present(
        &self,
        context: defra_core::RequestContext,
    ) -> defra_core::RequestContext {
        self.caller.present(context)
    }

    /// The node the call runs on.
    pub(crate) fn node(&self) -> usize {
        self.target.node()
    }

    /// The node's database.
    pub(crate) fn database(&self) -> Result<Arc<FfiDatabase>, DefraResult> {
        crate::helpers::get_node_database(self.node())
    }

    /// The node's executor, which runs a call in the transaction its context
    /// names, or in one of its own.
    pub(crate) fn runner(&self) -> Result<Arc<dyn query::QueryExecutor>, DefraResult> {
        NODES
            .get(self.node(), |state| state.query_runner.clone())
            .ok_or_else(|| DefraResult::error(ERR_INVALID_HANDLE))
    }

    /// The context the call acts in: the caller's identity, so DB-layer node
    /// access checks resolve the caller instead of the wildcard, and the
    /// transaction the call runs in, if any, and the caller's bearer token.
    pub(crate) fn context(&self) -> defra_core::RequestContext {
        self.present(
            defra_core::RequestContext::default()
                .with_identity(self.identity.clone())
                .with_txn(self.target.txn_id()),
        )
    }

    /// The caller's DID, parsed.
    pub(crate) fn did(&self) -> Result<Option<identity::Did>, DefraResult> {
        self.identity
            .as_deref()
            .map(|did| {
                identity::Did::new(did)
                    .map_err(|e| DefraResult::error(format!("invalid identity DID: {}", e)))
            })
            .transpose()
    }

    /// The node, for an operation that does not run in an explicit
    /// transaction: a transaction handle fails the call.
    pub(crate) fn require_node(&self, operation: &str) -> Result<usize, DefraResult> {
        match self.target {
            Target::Node(node) => Ok(node),
            Target::Txn(_) => Err(DefraResult::error(format!(
                "not supported: {} in a transaction",
                operation
            ))),
        }
    }
}
