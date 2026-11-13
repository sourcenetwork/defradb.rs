//! Adapter to bridge NacManager to HTTP's NodeAcpOperations trait.

use std::convert::Infallible;
use std::future::Future;
use std::sync::Arc;

use async_trait::async_trait;

use crate::router::{NodeAcpOperations, PermissionCheck};
use acp::nac::{NacStatus, NodePermission};
use db::NacManagerApi;
use identity::Did;

/// Adapter that implements NodeAcpOperations and NacChecker using NacManagerApi.
///
/// Every call that reads or writes the NAC's store runs on the worker pool
/// the adapter is placed on: the HTTP routes that check permissions on the
/// network edge reach the store through here.
pub struct NacAdapter {
    nac: Arc<dyn NacManagerApi>,
    pool: Option<workers::Pool>,
}

impl NacAdapter {
    /// Create a new adapter wrapping the given NAC manager.
    pub fn new(nac: Arc<dyn NacManagerApi>) -> Self {
        Self { nac, pool: None }
    }

    /// Create an Arc-wrapped adapter.
    pub fn new_arc(nac: Arc<dyn NacManagerApi>) -> Arc<dyn NodeAcpOperations> {
        Arc::new(Self::new(nac))
    }

    /// Runs the calls that reach the NAC's store on `pool`.
    pub fn placed_on(mut self, pool: Option<&workers::Pool>) -> Self {
        self.pool = pool.cloned();
        self
    }

    /// The underlying NAC manager. Used to build the KMS `NodeAcpRead` bridge.
    pub fn nac_manager(&self) -> Arc<dyn NacManagerApi> {
        self.nac.clone()
    }

    /// Runs `call` on the NAC manager on the adapter's pool: in place on a
    /// worker, or without a pool.
    async fn on_pool<T, E, F, Fut>(&self, call: F) -> Result<T, String>
    where
        F: FnOnce(Arc<dyn NacManagerApi>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, E>> + 'static,
        T: Send + 'static,
        E: std::fmt::Display + Send + 'static,
    {
        let nac = Arc::clone(&self.nac);
        workers::run_on(
            self.pool.as_ref(),
            workers::Target::LeastLoaded,
            move || call(nac),
        )
        .await
        .map_err(|error| error.report("node access call").to_string())?
        .map_err(|e| e.to_string())
    }
}

/// The first of `permissions` `identity` does not hold, checked in order in
/// one [`NacManagerApi::check_permissions`] call.
async fn first_missing(
    nac: &dyn NacManagerApi,
    identity: &Did,
    permissions: &[NodePermission],
) -> PermissionCheck {
    first_outcome(
        permissions,
        nac.check_permissions(identity, permissions).await,
    )
}

/// The first refusal or failure among `outcomes`, one per permission checked.
fn first_outcome(
    permissions: &[NodePermission],
    outcomes: Vec<db::nac::Result<bool>>,
) -> PermissionCheck {
    for (&permission, outcome) in permissions.iter().zip(outcomes) {
        match outcome {
            Ok(true) => {}
            Ok(false) => return PermissionCheck::Denied(permission),
            Err(error) => return PermissionCheck::Failed(permission, error.to_string()),
        }
    }
    PermissionCheck::Granted
}

#[async_trait]
impl NodeAcpOperations for NacAdapter {
    async fn check_permission(
        &self,
        identity: &Did,
        permission: NodePermission,
    ) -> Result<bool, String> {
        // A check on a worker (a route already on the pool) reads in place.
        if self.pool.is_none() || workers::on_worker() {
            return self
                .nac
                .check_permission(identity, permission)
                .await
                .map_err(|e| e.to_string());
        }
        if let Some(mut outcomes) = self
            .nac
            .check_permissions_in_memory(identity, &[permission])
            .await
        {
            return outcomes.remove(0).map_err(|e| e.to_string());
        }
        let identity = identity.clone();
        self.on_pool(move |nac| async move { nac.check_permission(&identity, permission).await })
            .await
    }

    async fn check_permissions(
        &self,
        identity: &Did,
        permissions: &[NodePermission],
    ) -> PermissionCheck {
        let Some(&first) = permissions.first() else {
            return PermissionCheck::Granted;
        };
        if self.pool.is_none() || workers::on_worker() {
            return first_missing(self.nac.as_ref(), identity, permissions).await;
        }
        // Answered from the manager's snapshot of the grants, the check reads
        // no storage, so it needs no worker: it runs here, on the edge.
        if let Some(outcomes) = self
            .nac
            .check_permissions_in_memory(identity, permissions)
            .await
        {
            return first_outcome(permissions, outcomes);
        }
        // One placed call decides them all: the edge crosses onto the pool
        // once per request, not once per permission.
        let identity = identity.clone();
        let permissions = permissions.to_vec();
        self.on_pool(move |nac| async move {
            Ok::<_, Infallible>(first_missing(nac.as_ref(), &identity, &permissions).await)
        })
        .await
        .unwrap_or_else(|error| PermissionCheck::Failed(first, error))
    }

    async fn get_status(&self) -> NacStatus {
        self.nac.status().await
    }

    async fn enable(&self, owner: &Did) -> Result<(), String> {
        let owner = owner.clone();
        self.on_pool(move |nac| async move { nac.enable(&owner).await })
            .await
    }
}

#[async_trait]
impl query::NacChecker for NacAdapter {
    async fn check_permission(&self, identity: &Did, permission: NodePermission) -> bool {
        NodeAcpOperations::check_permission(self, identity, permission)
            .await
            .unwrap_or(false)
    }
}
