//! Adding, listing and deleting encrypted indexes as the caller `context`
//! names: the one implementation HTTP, the CLI, FFI and the embedded node
//! call. Each runs in the transaction `context` carries, where the registry
//! checks node access, or else checks it here and runs in one of its own.

mod error;

pub use error::EncryptedIndexError;

use acp::nac::NodePermission;
use defra_core::RequestContext;
use storage::corekv::Store;

use crate::DbTransactionRegistry;

type Result<T> = std::result::Result<T, EncryptedIndexError>;

impl<S> DbTransactionRegistry<S>
where
    S: Store + 'static,
{
    /// Go's `NewEncryptedIndex`: an equality encrypted index on `field` of
    /// `collection`.
    pub async fn add_encrypted_index_as(
        &self,
        context: &RequestContext,
        collection: &str,
        field: &str,
    ) -> Result<()> {
        if let Some(txn) = context.txn() {
            return Ok(self
                .add_encrypted_index_in_txn(context, &txn.to_string(), collection, field)
                .await?);
        }
        self.check_encrypted_index_access(context, NodePermission::EncryptedIndexAdd)
            .await?;
        Ok(self.db().add_encrypted_index(collection, field).await?)
    }

    /// Go's `DeleteEncryptedIndex`: drop the encrypted index on `field` of
    /// `collection`.
    pub async fn delete_encrypted_index_as(
        &self,
        context: &RequestContext,
        collection: &str,
        field: &str,
    ) -> Result<()> {
        if let Some(txn) = context.txn() {
            return Ok(self
                .delete_encrypted_index_in_txn(context, &txn.to_string(), collection, field)
                .await?);
        }
        self.check_encrypted_index_access(context, NodePermission::EncryptedIndexDelete)
            .await?;
        Ok(self.db().delete_encrypted_index(collection, field).await?)
    }

    /// Go's `ListEncryptedIndexes`: the fields of `collection` that carry an
    /// encrypted index.
    pub async fn encrypted_indexes_as(
        &self,
        context: &RequestContext,
        collection: &str,
    ) -> Result<Vec<String>> {
        if let Some(txn) = context.txn() {
            return Ok(self
                .encrypted_indexes_in_txn(context, &txn.to_string(), collection)
                .await?);
        }
        self.check_encrypted_index_access(context, NodePermission::EncryptedIndexList)
            .await?;
        let db = self.db();
        let txn = db
            .new_txn(true)
            .await
            .map_err(EncryptedIndexError::TxnCreate)?;
        let fields = db.encrypted_indexes_in(&txn, collection).await;
        let _ = txn.discard();
        Ok(fields?)
    }

    /// Go's `ListAllEncryptedIndexes`: every collection's encrypted index
    /// fields, by collection name in name order; a collection without one
    /// is left out. Node access is checked for `list-all-encrypted-index`
    /// alone, as Go checks it.
    pub async fn all_encrypted_indexes_as(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<(String, Vec<String>)>> {
        if let Some(txn) = context.txn() {
            return Ok(self
                .all_encrypted_indexes_in_txn(context, &txn.to_string())
                .await?);
        }
        let db = self.db();
        db.check_node_access_as(context, NodePermission::EncryptedIndexListAll)
            .await?;
        let txn = db
            .new_txn(true)
            .await
            .map_err(EncryptedIndexError::TxnCreate)?;
        let all = db.all_encrypted_indexes_in(&txn).await;
        let _ = txn.discard();
        Ok(all?)
    }

    /// Node access for an operation on a collection outside a transaction:
    /// `get-collection`, as Go's collection lookup checks first, then
    /// `permission`.
    async fn check_encrypted_index_access(
        &self,
        context: &RequestContext,
        permission: NodePermission,
    ) -> Result<()> {
        let db = self.db();
        for needed in [NodePermission::CollectionGet, permission] {
            db.check_node_access_as(context, needed).await?;
        }
        Ok(())
    }
}
