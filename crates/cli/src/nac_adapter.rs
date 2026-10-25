//! Adapter to bridge NacManager to HTTP's NodeAcpOperations trait.

use std::future::Future;
use std::sync::Arc;

use async_trait::async_trait;

use acp::nac::{NacStatus, NodePermission};
use db::NacManagerApi;
use defra_http::router::{NacStatusInfo, NodeAcpOperations};
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

    /// Grants or revokes `relation` of `target`, `requestor` asking.
    async fn relationship(
        &self,
        requestor: &Did,
        target: &Did,
        relation: &str,
        grant: bool,
    ) -> Result<bool, String> {
        let (requestor, target) = (requestor.clone(), target.clone());
        if relation == "admin" {
            self.on_pool(move |nac| async move {
                match grant {
                    true => nac.add_admin(&requestor, &target).await,
                    false => nac.remove_admin(&requestor, &target).await,
                }
            })
            .await
        } else if let Some(perm) = NodePermission::parse(relation) {
            self.on_pool(move |nac| async move {
                match grant {
                    true => nac.add_permission_grant(&requestor, &target, perm).await,
                    false => nac.remove_permission_grant(&requestor, &target, perm).await,
                }
            })
            .await
        } else {
            Err("relation not in resource".to_string())
        }
    }
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
        let identity = identity.clone();
        self.on_pool(move |nac| async move { nac.check_permission(&identity, permission).await })
            .await
    }

    async fn get_status(&self) -> NacStatus {
        self.nac.status().await
    }

    async fn owner(&self) -> Option<Did> {
        self.nac.owner().await
    }

    async fn is_admin(&self, identity: &Did) -> Result<bool, String> {
        let identity = identity.clone();
        self.on_pool(move |nac| async move { nac.is_admin(&identity).await })
            .await
    }

    async fn add_admin(&self, requestor: &Did, target: &Did) -> Result<bool, String> {
        self.relationship(requestor, target, "admin", true).await
    }

    async fn remove_admin(&self, requestor: &Did, target: &Did) -> Result<bool, String> {
        self.relationship(requestor, target, "admin", false).await
    }

    async fn disable(&self, requestor: &Did) -> Result<(), String> {
        let requestor = requestor.clone();
        self.on_pool(move |nac| async move { nac.disable(&requestor).await })
            .await
    }

    async fn re_enable(&self, requestor: &Did) -> Result<(), String> {
        let requestor = requestor.clone();
        self.on_pool(move |nac| async move { nac.re_enable(&requestor).await })
            .await
    }

    async fn enable(&self, owner: &Did) -> Result<(), String> {
        let owner = owner.clone();
        self.on_pool(move |nac| async move { nac.enable(&owner).await })
            .await
    }

    async fn add_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        relation: &str,
    ) -> Result<bool, String> {
        self.relationship(requestor, target, relation, true).await
    }

    async fn remove_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        relation: &str,
    ) -> Result<bool, String> {
        self.relationship(requestor, target, relation, false).await
    }

    async fn info(&self) -> NacStatusInfo {
        let info = self.nac.info().await;
        NacStatusInfo {
            status: info.status,
            configured_enabled: info.configured_enabled,
            dev_mode: info.dev_mode,
            owner: info.owner,
        }
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

#[cfg(test)]
#[path = "nac_adapter_tests.rs"]
mod tests;
