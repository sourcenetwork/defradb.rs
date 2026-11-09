//! Index definitions read and changed inside a transaction.

use super::*;

impl<S: Store + 'static> DbTransactionRegistry<S> {
    /// Go's `Collection.CreateIndex` in transaction `txn_id`: the definition
    /// is visible in the transaction at once and to others once it commits,
    /// and the backfill starts only then.
    pub async fn create_index_in_txn(
        &self,
        context: &RequestContext,
        txn_id: &str,
        collection: &str,
        name: Option<&str>,
        fields: Vec<schema::IndexedFieldDescription>,
        kind: schema::IndexKind,
    ) -> Result<schema::IndexDescription> {
        let db = self.db.clone();
        let (collection, name) = (collection.to_owned(), name.map(str::to_owned));
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                db.create_index_in(txn, &collection, name.as_deref(), fields, kind)
                    .await
            })
        })
        .await
    }

    /// Go's `Collection.DropIndex` in transaction `txn_id`: the index leaves
    /// the definition the transaction sees, and its entries go once it
    /// commits.
    pub async fn drop_index_in_txn(
        &self,
        context: &RequestContext,
        txn_id: &str,
        collection: &str,
        name: &str,
    ) -> Result<()> {
        let db = self.db.clone();
        let (collection, name) = (collection.to_owned(), name.to_owned());
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                db.drop_index_in(txn, &collection, &name).await
            })
        })
        .await
    }

    /// Collection `collection` and its index actions by index ID, as
    /// transaction `txn_id` sees them, its own uncommitted changes included.
    pub async fn indexes_in_txn(
        &self,
        context: &RequestContext,
        txn_id: &str,
        collection: &str,
    ) -> Result<(
        schema::CollectionVersion,
        rapidhash::RapidHashMap<u32, defra_core::ActionExecution>,
    )> {
        let collection = collection.to_owned();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                let found = txn
                    .get_collection(&collection)
                    .await?
                    .ok_or_else(|| Error::CollectionNotFound(collection.clone()))?;
                let actions = crate::database::action::index_action_executions(
                    &txn.systemstore()?,
                    found.collection_id(),
                )
                .await?;
                Ok((found.schema().clone(), actions))
            })
        })
        .await
    }
}
