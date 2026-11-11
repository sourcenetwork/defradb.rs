use crate::database::DB;
use crate::error::{Error, Result};
use acp::nac::NodePermission;
use defra_core::RequestContext;
use identity::Did;
use storage::corekv::Store;

/// The identity acting in `context`, the wildcard for an anonymous request or
/// an identity that is not a DID.
pub(crate) fn acting_did(context: &RequestContext) -> Option<Did> {
    context.identity().and_then(|value| Did::new(value).ok())
}

/// The context a base-shaped call runs in: [`RequestContext::ambient`], with
/// `identity` acting in it when the call names one.
pub(crate) fn ambient_as(identity: Option<&Did>) -> RequestContext {
    let context = RequestContext::ambient();
    match identity {
        Some(identity) => context.with_identity(Some(identity.to_string())),
        None => context,
    }
}

impl<S: Store> DB<S> {
    /// [`Self::check_node_access_as`] read in the transaction `view` belongs
    /// to, each of [`NodePermission::go_order`] in turn.
    pub(crate) async fn check_node_access_in_txn(
        &self,
        context: &RequestContext,
        view: datastore::NamespaceView,
        permission: NodePermission,
    ) -> Result<()> {
        let Some(nac) = self.nac_manager() else {
            return Ok(());
        };
        let identity = acting_did(context);
        if self
            .node_did()
            .is_some_and(|owner| identity.as_ref() == Some(&owner))
        {
            return Ok(());
        }
        let identity = identity.unwrap_or_else(Did::wildcard);
        for permission in permission.go_order() {
            if !nac
                .check_permission_in_txn(view.clone(), &identity, permission)
                .await
                .map_err(|error| Error::Acp(error.to_string()))?
            {
                return Err(Error::NotAuthorized {
                    permission: permission.as_str().to_string(),
                });
            }
        }
        Ok(())
    }

    /// [`Self::check_node_access_as`] for `identity`, or for the identity
    /// acting in [`RequestContext::ambient`] when there is none.
    pub async fn check_node_access(
        &self,
        identity: Option<&Did>,
        permission: NodePermission,
    ) -> Result<()> {
        self.check_node_access_as(&ambient_as(identity), permission)
            .await
    }

    /// Check NAC permission for the identity acting in `context` before
    /// proceeding with a node-level operation. Matches Go's
    /// `db.checkNodeAccess()` pattern.
    ///
    /// Returns `Ok(())` when NAC is not configured or not enabled (all
    /// operations allowed), or when the identity holds the permission.
    /// Returns `Error::NotAuthorized` naming the first of
    /// [`NodePermission::go_order`] the identity lacks.
    pub async fn check_node_access_as(
        &self,
        context: &RequestContext,
        permission: NodePermission,
    ) -> Result<()> {
        let nac = match self.nac_manager() {
            Some(nac) => nac,
            None => return Ok(()),
        };
        if !nac.is_enabled().await {
            return Ok(());
        }
        let resolved = acting_did(context);
        // Node's own identity always has access (mirrors Go db_nac.go).
        if let Some(node) = self.node_did() {
            if resolved.as_ref() == Some(&node) {
                return Ok(());
            }
        }
        let did = resolved.unwrap_or_else(Did::wildcard);
        for permission in permission.go_order() {
            if !nac
                .check_permission(&did, permission)
                .await
                .map_err(|e| Error::Acp(e.to_string()))?
            {
                return Err(Error::NotAuthorized {
                    permission: permission.as_str().to_string(),
                });
            }
        }
        Ok(())
    }
}
