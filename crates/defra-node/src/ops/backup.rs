//! Backups as the caller `context` names: a JSON export of the documents and
//! its import, as Go's `BasicExport` and `BasicImport` do.

use std::sync::Arc;

use acp::nac::NodePermission;
use defra_core::RequestContext;

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
        let (database, runner) = (Arc::clone(self.database()), Arc::clone(&self.runner));
        let context = self.caller(context);
        let collections = collections.to_vec();
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

    /// Import the documents of backup `data` as `context`'s identity. They
    /// get new document IDs, as Go's import gives them. The import writes, so
    /// it runs as an admitted request: shutdown drains it before P2P stops.
    pub async fn import_backup_as(&self, context: &RequestContext, data: &str) -> Result<()> {
        let (database, runner) = (Arc::clone(self.database()), Arc::clone(&self.runner));
        let context = self.caller(context);
        let data = data.to_owned();
        let pool = self.pool.clone();
        self.as_request(async move {
            workers::run_on(
                pool.as_ref(),
                workers::Target::LeastLoaded,
                move || async move {
                    database
                        .check_node_access_as(&context, NodePermission::DocumentUpdate)
                        .await?;
                    db::backup::import_database_as(&context, &database, &runner, &data)
                        .await
                        .map(drop)
                        .map_err(Error::Backup)
                },
            )
            .await
            .map_err(Error::WorkerPoolCall)?
        })
        .await?
    }
}
