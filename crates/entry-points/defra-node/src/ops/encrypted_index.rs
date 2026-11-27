//! Encrypted (searchable-encryption) indexes as the caller `context` names,
//! as Go's `AddEncryptedIndex`, `ListEncryptedIndexes`,
//! `ListAllEncryptedIndexes` and `DeleteEncryptedIndex` keep them: on the
//! collection's definition.

use defra_core::RequestContext;
use serde::Serialize;

use super::handle::NodeOps;
use crate::{Node, Result};

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
            .add_encrypted_index(&self.admitted_caller(context)?, collection, field_name)
            .await
    }

    /// The encrypted indexes of `collection`.
    pub async fn list_encrypted_indexes_as(
        &self,
        context: &RequestContext,
        collection: &str,
    ) -> Result<Vec<EncryptedIndexListing>> {
        self.ops()
            .list_encrypted_indexes(&self.admitted_caller(context)?, collection)
            .await
    }

    /// Every collection's encrypted indexes, by collection name; a
    /// collection without one is left out.
    pub async fn list_all_encrypted_indexes_as(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<(String, Vec<EncryptedIndexListing>)>> {
        self.ops()
            .list_all_encrypted_indexes(&self.admitted_caller(context)?)
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
            .delete_encrypted_index(&self.admitted_caller(context)?, collection, field_name)
            .await
    }
}

/// The operations of [`Node`] above, as `context` names: no default caller.
/// Each forwards to the core: in the transaction `context` carries where its
/// owner runs it, or else on the database's worker pool.
impl NodeOps {
    /// Add an equality encrypted index on `field_name` of `collection`.
    pub(crate) async fn add_encrypted_index(
        &self,
        context: &RequestContext,
        collection: &str,
        field_name: &str,
    ) -> Result<EncryptedIndexListing> {
        let (registry, context) = (self.txn_registry.clone(), context.clone());
        let (collection, field) = (collection.to_owned(), field_name.to_owned());
        self.encrypted(context.txn().is_some(), move || async move {
            Ok(registry
                .add_encrypted_index_as(&context, &collection, &field)
                .await?)
        })
        .await?;
        Ok(listing(field_name))
    }

    /// The encrypted indexes of `collection`.
    pub(crate) async fn list_encrypted_indexes(
        &self,
        context: &RequestContext,
        collection: &str,
    ) -> Result<Vec<EncryptedIndexListing>> {
        let (registry, context) = (self.txn_registry.clone(), context.clone());
        let collection = collection.to_owned();
        let fields = self
            .encrypted(context.txn().is_some(), move || async move {
                Ok(registry.encrypted_indexes_as(&context, &collection).await?)
            })
            .await?;
        Ok(fields.iter().map(|field| listing(field)).collect())
    }

    /// Every collection's encrypted indexes, by collection name; a
    /// collection without one is left out.
    pub(crate) async fn list_all_encrypted_indexes(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<(String, Vec<EncryptedIndexListing>)>> {
        let (registry, context) = (self.txn_registry.clone(), context.clone());
        let all = self
            .encrypted(context.txn().is_some(), move || async move {
                Ok(registry.all_encrypted_indexes_as(&context).await?)
            })
            .await?;
        Ok(all
            .into_iter()
            .map(|(name, fields)| (name, fields.iter().map(|field| listing(field)).collect()))
            .collect())
    }

    /// Delete the encrypted index on `field_name` of `collection`.
    pub(crate) async fn delete_encrypted_index(
        &self,
        context: &RequestContext,
        collection: &str,
        field_name: &str,
    ) -> Result<()> {
        let (registry, context) = (self.txn_registry.clone(), context.clone());
        let (collection, field) = (collection.to_owned(), field_name.to_owned());
        self.encrypted(context.txn().is_some(), move || async move {
            Ok(registry
                .delete_encrypted_index_as(&context, &collection, &field)
                .await?)
        })
        .await
    }

    /// Runs `make` where an encrypted index operation belongs: at once when
    /// it is `in_txn`, whose owner the registry routes it to, and on the
    /// worker pool otherwise.
    async fn encrypted<F, Fut, T>(&self, in_txn: bool, make: F) -> Result<T>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T>> + 'static,
        T: Send + 'static,
    {
        if in_txn {
            make().await
        } else {
            self.on_pool(make).await
        }
    }
}

fn listing(field: &str) -> EncryptedIndexListing {
    EncryptedIndexListing {
        field_name: field.to_owned(),
        index_type: "equality".to_owned(),
    }
}
