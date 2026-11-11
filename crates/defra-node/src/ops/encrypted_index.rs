//! Encrypted (searchable-encryption) indexes as the caller `context` names,
//! as Go's `AddEncryptedIndex`, `ListEncryptedIndexes` and
//! `DeleteEncryptedIndex` keep them: on the collection's definition.

use acp::nac::NodePermission;
use defra_core::RequestContext;
use schema::EncryptedIndexDescription;
use serde::Serialize;
use storage::corekv::Key;

use super::handle::{Database, NodeOps};
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
impl NodeOps {
    /// Add an equality encrypted index on `field_name` of `collection`.
    pub(crate) async fn add_encrypted_index(
        &self,
        context: &RequestContext,
        collection: &str,
        field_name: &str,
    ) -> Result<EncryptedIndexListing> {
        let (database, context) = (self.database.clone(), context.clone());
        let (collection, field) = (collection.to_owned(), field_name.to_owned());
        self.on_pool(move || async move {
            database
                .check_node_access_as(&context, NodePermission::CollectionGet)
                .await?;
            database
                .check_node_access_as(&context, NodePermission::EncryptedIndexAdd)
                .await?;
            rewrite(&database, &collection, |schema| {
                if !schema.fields.iter().any(|f| f.name == field) {
                    return Err(Error::EncryptedIndexFieldMissing(field.clone()));
                }
                if schema
                    .encrypted_indexes
                    .iter()
                    .any(|i| i.field_name == field)
                {
                    return Err(Error::EncryptedIndexExists(field.clone()));
                }
                schema
                    .encrypted_indexes
                    .push(EncryptedIndexDescription::new(&field));
                Ok(())
            })
            .await?;
            Ok(listing(&field))
        })
        .await
    }

    /// The encrypted indexes of `collection`.
    pub(crate) async fn list_encrypted_indexes(
        &self,
        context: &RequestContext,
        collection: &str,
    ) -> Result<Vec<EncryptedIndexListing>> {
        let (database, context) = (self.database.clone(), context.clone());
        let collection = collection.to_owned();
        self.on_pool(move || async move {
            database
                .check_node_access_as(&context, NodePermission::CollectionGet)
                .await?;
            database
                .check_node_access_as(&context, NodePermission::EncryptedIndexList)
                .await?;
            indexes_of(&database, &collection)
        })
        .await
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
        let (database, context) = (self.database.clone(), context.clone());
        let (collection, field) = (collection.to_owned(), field_name.to_owned());
        self.on_pool(move || async move {
            database
                .check_node_access_as(&context, NodePermission::CollectionGet)
                .await?;
            database
                .check_node_access_as(&context, NodePermission::EncryptedIndexDelete)
                .await?;
            rewrite(&database, &collection, |schema| {
                if !schema
                    .encrypted_indexes
                    .iter()
                    .any(|i| i.field_name == field)
                {
                    return Err(Error::EncryptedIndexMissing(field.clone()));
                }
                schema.encrypted_indexes.retain(|i| i.field_name != field);
                Ok(())
            })
            .await
        })
        .await
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

/// Change `collection`'s definition with `change` and save it.
async fn rewrite<F>(database: &Database, collection: &str, change: F) -> Result<()>
where
    F: FnOnce(&mut schema::CollectionVersion) -> Result<()>,
{
    let col = database
        .get_collection(collection)
        .map_err(Error::CollectionLookupFailed)?
        .ok_or_else(|| Error::CollectionMissing(collection.to_owned()))?;
    db::chunk::ensure_admin_changeable(col.schema())?;
    let mut updated = col.schema().clone();
    change(&mut updated)?;

    let txn = database.new_txn(false).await.map_err(Error::TxnCreate)?;
    {
        let collection_key = storage::keys::systemstore::CollectionKey::new(&updated.version_id);
        let schema_data = serde_json::to_vec(&updated).map_err(Error::SchemaSerialize)?;
        let systemstore = txn.systemstore().map_err(Error::SystemstoreOpen)?;
        systemstore
            .set(&collection_key.bytes(), &schema_data)
            .await
            .map_err(Error::SchemaSave)?;
        let name_key = storage::keys::systemstore::CollectionNameKey::new(collection);
        systemstore
            .set(&name_key.bytes(), updated.version_id.as_bytes())
            .await
            .map_err(Error::NameMappingSave)?;
    }
    txn.commit().await.map_err(Error::Commit)?;
    database.reload_cache().await.map_err(Error::CacheReload)?;
    Ok(())
}
