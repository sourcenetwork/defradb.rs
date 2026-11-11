//! Encrypted (searchable-encryption) indexes as the caller `context` names,
//! as Go's `AddEncryptedIndex`, `ListEncryptedIndexes` and
//! `DeleteEncryptedIndex` keep them: on the collection's definition.

use acp::nac::NodePermission;
use defra_core::RequestContext;
use serde::Serialize;

use super::handle::{Database, NodeOps};
use crate::ops::request::transaction_error;
use crate::{Error, Node, Result};

/// An encrypted index as Go lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EncryptedIndexListing {
    #[serde(rename = "FieldName")]
    pub field_name: String,
    #[serde(rename = "Type")]
    pub index_type: String,
}

impl Node {
    /// Add an equality encrypted index on `field_name` of `collection`.
    pub async fn add_encrypted_index_as(
        &self,
        context: &RequestContext,
        collection: &str,
        field_name: &str,
    ) -> Result<EncryptedIndexListing> {
        self.ops()
            .add_encrypted_index(&self.caller(context), collection, field_name)
            .await
    }

    /// The encrypted indexes of `collection`.
    pub async fn list_encrypted_indexes_as(
        &self,
        context: &RequestContext,
        collection: &str,
    ) -> Result<Vec<EncryptedIndexListing>> {
        self.ops()
            .list_encrypted_indexes(&self.caller(context), collection)
            .await
    }

    /// Every collection's encrypted indexes, by collection name; a
    /// collection without one is left out.
    pub async fn list_all_encrypted_indexes_as(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<(String, Vec<EncryptedIndexListing>)>> {
        self.ops()
            .list_all_encrypted_indexes(&self.caller(context))
            .await
    }

    /// Delete the encrypted index on `field_name` of `collection`.
    pub async fn delete_encrypted_index_as(
        &self,
        context: &RequestContext,
        collection: &str,
        field_name: &str,
    ) -> Result<()> {
        self.ops()
            .delete_encrypted_index(&self.caller(context), collection, field_name)
            .await
    }
}

/// The operations of [`Node`] above, as `context` names: no default caller.
/// Each runs in the transaction `context` carries, or in one of its own.
impl NodeOps {
    /// Add an equality encrypted index on `field_name` of `collection`.
    pub(crate) async fn add_encrypted_index(
        &self,
        context: &RequestContext,
        collection: &str,
        field_name: &str,
    ) -> Result<EncryptedIndexListing> {
        if context.txn().is_some() {
            self.runner
                .add_encrypted_index_in_txn_as(context, collection, field_name)
                .await
                .map_err(transaction_error)?;
        } else {
            self.allowed(context, NodePermission::EncryptedIndexAdd)
                .await?;
            let database = self.database.clone();
            let (collection, field) = (collection.to_owned(), field_name.to_owned());
            self.on_pool(move || async move {
                database
                    .add_encrypted_index(&collection, &field)
                    .await
                    .map_err(definition_error)
            })
            .await?;
        }
        Ok(listing(field_name))
    }

    /// The encrypted indexes of `collection`.
    pub(crate) async fn list_encrypted_indexes(
        &self,
        context: &RequestContext,
        collection: &str,
    ) -> Result<Vec<EncryptedIndexListing>> {
        let fields = if context.txn().is_some() {
            self.runner
                .encrypted_indexes_in_txn_as(context, collection)
                .await
                .map_err(transaction_error)?
        } else {
            self.allowed(context, NodePermission::EncryptedIndexList)
                .await?;
            let database = self.database.clone();
            let collection = collection.to_owned();
            self.on_pool(move || async move {
                let txn = database.new_txn(true).await.map_err(Error::TxnCreate)?;
                let fields = database
                    .encrypted_indexes_in(&txn, &collection)
                    .await
                    .map_err(definition_error);
                let _ = txn.discard();
                fields
            })
            .await?
        };
        Ok(fields.iter().map(|field| listing(field)).collect())
    }

    /// Every collection's encrypted indexes, by collection name; a
    /// collection without one is left out.
    pub(crate) async fn list_all_encrypted_indexes(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<(String, Vec<EncryptedIndexListing>)>> {
        let (database, context) = (self.database.clone(), context.clone());
        self.on_pool(move || async move {
            database
                .check_node_access_as(&context, NodePermission::EncryptedIndexListAll)
                .await?;
            let mut all = Vec::new();
            for name in database.list_collections()? {
                let indexes = indexes_of(&database, &name)?;
                if !indexes.is_empty() {
                    all.push((name, indexes));
                }
            }
            Ok(all)
        })
        .await
    }

    /// Delete the encrypted index on `field_name` of `collection`.
    pub(crate) async fn delete_encrypted_index(
        &self,
        context: &RequestContext,
        collection: &str,
        field_name: &str,
    ) -> Result<()> {
        if context.txn().is_some() {
            return self
                .runner
                .delete_encrypted_index_in_txn_as(context, collection, field_name)
                .await
                .map_err(transaction_error);
        }
        self.allowed(context, NodePermission::EncryptedIndexDelete)
            .await?;
        let database = self.database.clone();
        let (collection, field) = (collection.to_owned(), field_name.to_owned());
        self.on_pool(move || async move {
            database
                .delete_encrypted_index(&collection, &field)
                .await
                .map_err(definition_error)
        })
        .await
    }

    /// The NAC checks every encrypted index operation makes outside a
    /// transaction: the caller may read collections and may do
    /// `permission`. In a transaction the registry checks both there.
    async fn allowed(&self, context: &RequestContext, permission: NodePermission) -> Result<()> {
        let (database, context) = (self.database.clone(), context.clone());
        self.on_pool(move || async move {
            database
                .check_node_access_as(&context, NodePermission::CollectionGet)
                .await?;
            Ok(database.check_node_access_as(&context, permission).await?)
        })
        .await
    }
}

/// A definition change's error in the node's words: a missing collection as
/// Go reports it, the rest as the core does.
fn definition_error(error: db::Error) -> Error {
    match error {
        db::Error::CollectionNotFound(name) => Error::CollectionMissing(name),
        other => Error::from(other),
    }
}

fn listing(field: &str) -> EncryptedIndexListing {
    EncryptedIndexListing {
        field_name: field.to_owned(),
        index_type: "equality".to_owned(),
    }
}

fn indexes_of(database: &Database, collection: &str) -> Result<Vec<EncryptedIndexListing>> {
    let col = database
        .get_collection(collection)
        .map_err(Error::CollectionLookupFailed)?
        .ok_or_else(|| Error::CollectionMissing(collection.to_owned()))?;
    Ok(col
        .schema()
        .encrypted_indexes
        .iter()
        .map(|index| listing(&index.field_name))
        .collect())
}
