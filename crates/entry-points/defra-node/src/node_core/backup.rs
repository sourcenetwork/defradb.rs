//! Backups as the caller `context` names, outside any transaction.

use db::backup::ImportHost;
use defra_core::RequestContext;

use super::NodeCore;
use crate::ops::NodeHost;
use crate::Result;

impl NodeCore<'_> {
    /// The documents of `collections` (every collection when empty) as the
    /// JSON a backup file holds, read as `context`'s identity.
    pub(crate) async fn export_backup(
        &self,
        context: &RequestContext,
        collections: &[String],
        pretty: bool,
    ) -> Result<String> {
        self.node
            .ops()
            .export_backup(
                self.node.admitted_caller(context)?,
                collections.to_vec(),
                pretty,
            )
            .await
    }

    /// Import the documents of backup `data` as `context`'s identity. They
    /// get new document IDs, as Go's import gives them. The import is one
    /// admitted request of this node, run as a task of its own: refused once
    /// shutdown stops admitting, drained by shutdown once begun, and run to
    /// its end if its caller stops waiting, so a saved batch always gets its
    /// aliases. Each create and relation patch continues that request and is
    /// signed and broadcast as any other write. The import runs outside any
    /// transaction `context` names.
    pub(crate) async fn import_backup(&self, context: &RequestContext, data: &str) -> Result<()> {
        let context = self.node.admitted_caller(context)?.with_txn(None);
        let host = NodeHost::new(self.node, context.clone())?;
        self.import_admitted(context, host, data).await.map(drop)
    }

    /// The import of [`Self::import_backup`] as one admitted request, every
    /// write sent through `host`.
    pub(crate) async fn import_admitted<H>(
        &self,
        context: RequestContext,
        host: H,
        data: &str,
    ) -> Result<db::backup::ImportStats>
    where
        H: ImportHost + 'static,
    {
        let (ops, data) = (self.node.ops(), data.to_owned());
        self.node
            .as_request(async move { ops.import_backup_with(context, &host, &data).await })
            .await?
    }
}
