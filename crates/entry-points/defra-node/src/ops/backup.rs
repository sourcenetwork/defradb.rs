//! Backups as the caller `context` names: a JSON export of the documents and
//! its import, as Go's `BasicExport` and `BasicImport` do.

use acp::nac::NodePermission;
use defra_core::RequestContext;

use super::handle::NodeOps;
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
    /// get new document IDs, as Go's import gives them. The import writes, so
    /// it runs as an admitted request: shutdown drains it before P2P stops.
    pub async fn import_backup_as(&self, context: &RequestContext, data: &str) -> Result<()> {
        let (ops, context, data) = (self.ops(), self.caller(context), data.to_owned());
        self.as_request(async move { ops.import_backup(context, data).await.map(drop) })
            .await?
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
    /// in the request it is called in.
    pub(crate) async fn import_backup(
        &self,
        context: RequestContext,
        data: String,
    ) -> Result<db::backup::ImportStats> {
        let (database, runner) = (self.database.clone(), self.runner.clone());
        self.on_pool(move || async move {
            database
                .check_node_access_as(&context, NodePermission::DocumentUpdate)
                .await?;
            db::backup::import_database_as(&context, &database, &runner, &data)
                .await
                .map_err(Error::Backup)
        })
        .await
    }
}
