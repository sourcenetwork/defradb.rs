use crate::database::DB;
use crate::error::Result;
use acp::nac::NodePermission;
use async_trait::async_trait;
use defra_core::RequestContext;
use storage::corekv::Store;

/// Type-erased handle to DB-layer NAC enforcement, for components that are
/// generic-erased over the store and cannot hold an `Arc<DB<S>>`.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait NodeAccessChecker: defra_core::thread_bounds::MaybeSendSync {
    /// Enforce `permission` for the identity acting in
    /// [`RequestContext::ambient`]. Mirrors `DB::check_node_access`: a no-op
    /// when NAC is unset or disabled, with node-identity bypass.
    async fn check_node_access(&self, permission: NodePermission) -> Result<()>;

    /// Enforce `permission` for the identity acting in `context`. The default
    /// runs [`Self::check_node_access`] inside [`RequestContext::mirror`].
    async fn check_node_access_as(
        &self,
        context: &RequestContext,
        permission: NodePermission,
    ) -> Result<()> {
        context.mirror(self.check_node_access(permission)).await
    }
}

struct DbNodeAccessChecker<S: Store> {
    db: std::sync::Arc<DB<S>>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store> NodeAccessChecker for DbNodeAccessChecker<S> {
    async fn check_node_access(&self, permission: NodePermission) -> Result<()> {
        self.db.check_node_access(None, permission).await
    }

    async fn check_node_access_as(
        &self,
        context: &RequestContext,
        permission: NodePermission,
    ) -> Result<()> {
        self.db.check_node_access_as(context, permission).await
    }
}

/// `checker` serving the request `context` serves: its context-free
/// [`NodeAccessChecker::check_node_access`] checks the identity acting in
/// `context`.
#[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
pub fn bound_node_access_checker(
    checker: std::sync::Arc<dyn NodeAccessChecker>,
    context: RequestContext,
) -> std::sync::Arc<dyn NodeAccessChecker> {
    std::sync::Arc::new(RequestNodeAccessChecker { checker, context })
}

struct RequestNodeAccessChecker {
    checker: std::sync::Arc<dyn NodeAccessChecker>,
    context: RequestContext,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl NodeAccessChecker for RequestNodeAccessChecker {
    async fn check_node_access(&self, permission: NodePermission) -> Result<()> {
        self.checker
            .check_node_access_as(&self.context, permission)
            .await
    }

    async fn check_node_access_as(
        &self,
        context: &RequestContext,
        permission: NodePermission,
    ) -> Result<()> {
        self.checker.check_node_access_as(context, permission).await
    }
}

/// Build a type-erased NAC checker from a database handle.
pub fn node_access_checker<S: Store + 'static>(
    db: std::sync::Arc<DB<S>>,
) -> std::sync::Arc<dyn NodeAccessChecker> {
    std::sync::Arc::new(DbNodeAccessChecker { db })
}
