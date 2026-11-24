//! Index definitions read and changed inside a transaction. Each operation
//! checks node access in the transaction, `get-collection` first as Go's
//! collection lookup does, and a change that fails leaves nothing behind.

use super::*;
use acp::nac::NodePermission;

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
        let acting = context.clone();
        let (collection, name) = (collection.to_owned(), name.map(str::to_owned));
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                check_access_in(&db, &acting, txn, NodePermission::IndexCreate).await?;
                txn.set_statement_savepoint()?;
                let created = db
                    .create_index_in(txn, &collection, name.as_deref(), fields, kind)
                    .await;
                finish_statement(txn, created)
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
        let acting = context.clone();
        let (collection, name) = (collection.to_owned(), name.to_owned());
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                check_access_in(&db, &acting, txn, NodePermission::IndexDelete).await?;
                txn.set_statement_savepoint()?;
                let dropped = db.drop_index_in(txn, &collection, &name).await;
                finish_statement(txn, dropped)
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
        let db = self.db.clone();
        let acting = context.clone();
        let collection = collection.to_owned();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                check_access_in(&db, &acting, txn, NodePermission::IndexList).await?;
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

    /// Go's `AddEncryptedIndex` in transaction `txn_id`.
    pub async fn add_encrypted_index_in_txn(
        &self,
        context: &RequestContext,
        txn_id: &str,
        collection: &str,
        field: &str,
    ) -> Result<()> {
        let db = self.db.clone();
        let acting = context.clone();
        let (collection, field) = (collection.to_owned(), field.to_owned());
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                check_access_in(&db, &acting, txn, NodePermission::EncryptedIndexAdd).await?;
                txn.set_statement_savepoint()?;
                let added = db.add_encrypted_index_in(txn, &collection, &field).await;
                finish_statement(txn, added)
            })
        })
        .await
    }

    /// Go's `DeleteEncryptedIndex` in transaction `txn_id`.
    pub async fn delete_encrypted_index_in_txn(
        &self,
        context: &RequestContext,
        txn_id: &str,
        collection: &str,
        field: &str,
    ) -> Result<()> {
        let db = self.db.clone();
        let acting = context.clone();
        let (collection, field) = (collection.to_owned(), field.to_owned());
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                check_access_in(&db, &acting, txn, NodePermission::EncryptedIndexDelete).await?;
                txn.set_statement_savepoint()?;
                let deleted = db.delete_encrypted_index_in(txn, &collection, &field).await;
                finish_statement(txn, deleted)
            })
        })
        .await
    }

    /// Go's `ListEncryptedIndexes` in transaction `txn_id`: the fields with
    /// an encrypted index, its own uncommitted changes included.
    pub async fn encrypted_indexes_in_txn(
        &self,
        context: &RequestContext,
        txn_id: &str,
        collection: &str,
    ) -> Result<Vec<String>> {
        let db = self.db.clone();
        let acting = context.clone();
        let collection = collection.to_owned();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                check_access_in(&db, &acting, txn, NodePermission::EncryptedIndexList).await?;
                db.encrypted_indexes_in(txn, &collection).await
            })
        })
        .await
    }

    /// Go's `ListAllEncryptedIndexes` in transaction `txn_id`: every
    /// collection's encrypted index fields, by collection name, its own
    /// uncommitted changes included. Node access is checked there, for
    /// `list-all-encrypted-index` alone, as Go checks it.
    pub async fn all_encrypted_indexes_in_txn(
        &self,
        context: &RequestContext,
        txn_id: &str,
    ) -> Result<Vec<(String, Vec<String>)>> {
        let db = self.db.clone();
        let acting = context.clone();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                db.check_node_access_in_txn(
                    &acting,
                    txn.acpstore()?,
                    NodePermission::EncryptedIndexListAll,
                )
                .await?;
                db.all_encrypted_indexes_in(txn).await
            })
        })
        .await
    }
}

/// Node access for an operation on a collection, read in `txn`:
/// `get-collection`, as Go's collection lookup checks first, then
/// `permission`. A revocation that commits first aborts the decision.
pub(super) async fn check_access_in<S>(
    db: &DB<S>,
    context: &RequestContext,
    txn: &crate::txn::DbTxn<S>,
    permission: NodePermission,
) -> Result<()>
where
    S: Store,
{
    for needed in [NodePermission::CollectionGet, permission] {
        db.check_node_access_in_txn(context, txn.acpstore()?, needed)
            .await?;
    }
    Ok(())
}
