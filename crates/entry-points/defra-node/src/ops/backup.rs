//! Backups as the caller `context` names: a JSON export of the documents and
//! its import, as Go's `BasicExport` and `BasicImport` do.

use acp::nac::NodePermission;
use db::backup::{DbJob, ImportHost};
use defra_core::thread_bounds::MaybeBoxFuture;
use defra_core::RequestContext;
use identity::Did;
use query::{QueryRequest, QueryResponse};

use super::handle::NodeOps;
use super::request::acting_identity;
use crate::{Error, Node, Result};

impl Node {
    /// The documents of `collections` (every collection when empty) as the
    /// JSON a backup file holds, read as `context`'s identity.
    pub async fn export_backup_as(
        &self,
        context: &RequestContext,
        collections: &[String],
        pretty: bool,
    ) -> Result<String> {
        self.ops()
            .export_backup(self.caller(context), collections.to_vec(), pretty)
            .await
    }

    /// Import the documents of backup `data` as `context`'s identity. They
    /// get new document IDs, as Go's import gives them. The import is one
    /// admitted request of this node: refused once shutdown stops admitting,
    /// and drained by shutdown once begun, so a saved batch always gets its
    /// aliases. Each create and relation patch continues that request and is
    /// signed and broadcast as any other write. The import runs outside any
    /// transaction `context` names.
    pub async fn import_backup_as(&self, context: &RequestContext, data: &str) -> Result<()> {
        let context = self.caller(context).with_txn(None);
        let host = NodeHost {
            node: self,
            identity: acting_identity(&context)?,
            context: context.clone(),
        };
        self.import_admitted(context, &host, data).await.map(drop)
    }

    /// The import of [`Self::import_backup_as`] as one admitted request, every
    /// write sent through `host`.
    async fn import_admitted(
        &self,
        context: RequestContext,
        host: &dyn ImportHost,
        data: &str,
    ) -> Result<db::backup::ImportStats> {
        let _admitted = self.admit_request()?;
        self.ops().import_backup_with(context, host, data).await
    }
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;

/// This node's request path for an import's writes, each continuing the
/// import's own admitted request ([`Node::admit_request`]).
struct NodeHost<'a> {
    node: &'a Node,
    context: RequestContext,
    identity: Option<Did>,
}

impl ImportHost for NodeHost<'_> {
    fn run(&self, request: QueryRequest) -> MaybeBoxFuture<'_, QueryResponse> {
        Box::pin(self.node.run_continuing_as_identity(
            &self.context,
            self.identity.clone(),
            request,
        ))
    }

    fn on_db(&self, job: DbJob) -> MaybeBoxFuture<'_, std::result::Result<(), String>> {
        let ops = self.node.ops();
        Box::pin(async move {
            ops.on_pool(move || async move { Ok(job().await) })
                .await
                .unwrap_or_else(|error| Err(error.to_string()))
        })
    }
}

impl NodeOps {
    /// [`Node::export_backup_as`] as `context` names, with no default caller.
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

    /// [`Node::import_backup_as`] as `context` names, with no default caller,
    /// every write sent through `host`. The driver runs on the caller; only
    /// `host` places work on the pool.
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
