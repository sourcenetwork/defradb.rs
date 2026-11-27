//! Backups as the caller `context` names: a JSON export of the documents and
//! its import, as Go's `BasicExport` and `BasicImport` do.

use acp::nac::NodePermission;
use db::backup::{DbJob, ImportHost};
use defra_core::thread_bounds::MaybeBoxFuture;
use defra_core::RequestContext;
use identity::Did;
use query::{QueryRequest, QueryResponse};

use super::handle::NodeOps;
use super::request::{acting_identity, request_in};
use crate::request_path::OwnedRequestPath;
use crate::{Admit, Error, Node, Result};

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;

/// This node's request path for an import's writes, each continuing the
/// import's own admitted request. It owns what it needs, so the import's
/// task outlives its caller's borrow of the node.
pub(crate) struct NodeHost {
    path: OwnedRequestPath,
    ops: NodeOps,
    context: RequestContext,
    identity: Option<Did>,
}

impl NodeHost {
    pub(crate) fn new(node: &Node, context: RequestContext) -> Result<Self> {
        Ok(Self {
            path: node.owned_request_path(),
            ops: node.ops(),
            identity: acting_identity(&context)?,
            context,
        })
    }
}

impl ImportHost for NodeHost {
    fn run(&self, request: QueryRequest) -> MaybeBoxFuture<'_, QueryResponse> {
        let request = request_in(&self.context, self.identity.clone(), request);
        Box::pin(self.path.path().execute(request, None, Admit::Continuation))
    }

    fn on_db(&self, job: DbJob) -> MaybeBoxFuture<'_, std::result::Result<(), String>> {
        let ops = self.ops.clone();
        Box::pin(async move {
            ops.on_pool(move || async move { Ok(job().await) })
                .await
                .unwrap_or_else(|error| Err(error.to_string()))
        })
    }
}

impl NodeOps {
    /// [`crate::Node::export_backup`] as `context` names, with no default caller.
    pub(crate) async fn export_backup(
        &self,
        context: RequestContext,
        collections: Vec<String>,
        pretty: bool,
    ) -> Result<String> {
        let (database, runner) = (self.database.clone(), self.runner.clone());
        self.on_pool(move || async move {
            database
                .check_node_access_as(&context, NodePermission::DocumentRead)
                .await?;
            db::backup::export_database_as(&context, &database, &runner, &collections, pretty)
                .await
                .map_err(Error::Backup)
        })
        .await
    }

    /// [`crate::Node::import_backup`] as `context` names, with no default caller,
    /// every write sent through `host`. The driver runs where it is awaited;
    /// only `host` places work on the pool.
    pub(crate) async fn import_backup_with(
        &self,
        context: RequestContext,
        host: &dyn ImportHost,
        data: &str,
    ) -> Result<db::backup::ImportStats> {
        db::backup::import_database_as(&context, &self.database, host, data)
            .await
            .map_err(Error::Backup)
    }
}
