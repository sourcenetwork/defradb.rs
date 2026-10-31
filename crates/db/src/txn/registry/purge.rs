//! Purging documents by DocID, in the caller's transaction when the request
//! carries one.

use super::*;

impl<S> DbTransactionRegistry<S>
where
    S: Store + 'static,
{
    /// [`DB::purge_by_doc_ids_as`], run in the transaction `context` carries
    /// when it carries one: nothing is removed until that transaction
    /// commits, and nothing at all if it is discarded. The node permission is
    /// read in that transaction, and a definition change that commits first
    /// conflicts with it, as with any other write there.
    pub async fn purge_by_doc_ids_as(
        &self,
        context: &RequestContext,
        name: &str,
        doc_ids: &[document::DocID],
    ) -> Result<usize> {
        let Some(id) = context.txn() else {
            return self.db.purge_by_doc_ids_as(context, name, doc_ids).await;
        };
        let db = self.db.clone();
        let acting = context.clone();
        let name = name.to_owned();
        let doc_ids = doc_ids.to_vec();
        self.in_txn(context, &id.to_string(), move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                db.check_node_access_in_txn(
                    &acting,
                    txn.acpstore()?,
                    acp::nac::NodePermission::CollectionTruncate,
                )
                .await?;
                let collection = txn
                    .get_collection(&name)
                    .await?
                    .ok_or_else(|| Error::CollectionNotFound(name.clone()))?;
                txn.validate_collection_for_write(&collection).await?;
                db.purge_documents(txn, &collection, &doc_ids).await
            })
        })
        .await
    }
}
