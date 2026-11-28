//! Encrypted (searchable-encryption) indexes as the caller `context` names,
//! as Go's `AddEncryptedIndex`, `ListEncryptedIndexes`,
//! `ListAllEncryptedIndexes` and `DeleteEncryptedIndex` keep them: on the
//! collection's definition.

use defra_core::RequestContext;
use serde::Serialize;

use super::handle::NodeOps;
use crate::Result;

/// An encrypted index as Go lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EncryptedIndexListing {
    #[serde(rename = "FieldName")]
    pub field_name: String,
    #[serde(rename = "Type")]
    pub index_type: String,
}

/// The operations of [`Node`] above, as `context` names: no default caller.
/// Each forwards to the core: in the transaction `context` carries where its
/// owner runs it, or else on the database's worker pool.
impl NodeOps {
    /// Add an equality encrypted index on `field_name` of `collection`.
    pub(crate) async fn add_encrypted_index(
        &self,
        owner: &RequestContext,
        caller: &RequestContext,
        collection: &str,
        field_name: &str,
    ) -> Result<EncryptedIndexListing> {
        let (registry, owner, caller) = (self.txn_registry.clone(), owner.clone(), caller.clone());
        let (collection, field) = (collection.to_owned(), field_name.to_owned());
        self.encrypted(owner.txn().is_some(), move || async move {
            match owner.txn() {
                Some(txn) => {
                    let txn = txn.to_string();
                    registry
                        .add_encrypted_index_in_txn_for(&owner, &caller, &txn, &collection, &field)
                        .await
                        .map_err(db::EncryptedIndexError::from)?;
                }
                None => {
                    registry
                        .add_encrypted_index_as(&caller, &collection, &field)
                        .await?;
                }
            }
            Ok(())
        })
        .await?;
        Ok(listing(field_name))
    }

    /// The encrypted indexes of `collection`.
    pub(crate) async fn list_encrypted_indexes(
        &self,
        owner: &RequestContext,
        caller: &RequestContext,
        collection: &str,
    ) -> Result<Vec<EncryptedIndexListing>> {
        let (registry, owner, caller) = (self.txn_registry.clone(), owner.clone(), caller.clone());
        let collection = collection.to_owned();
        let fields = self
            .encrypted(owner.txn().is_some(), move || async move {
                Ok(match owner.txn() {
                    Some(txn) => {
                        let txn = txn.to_string();
                        registry
                            .encrypted_indexes_in_txn_for(&owner, &caller, &txn, &collection)
                            .await
                            .map_err(db::EncryptedIndexError::from)?
                    }
                    None => registry.encrypted_indexes_as(&caller, &collection).await?,
                })
            })
            .await?;
        Ok(fields.iter().map(|field| listing(field)).collect())
    }

    /// Every collection's encrypted indexes, by collection name; a
    /// collection without one is left out.
    pub(crate) async fn list_all_encrypted_indexes(
        &self,
        owner: &RequestContext,
        caller: &RequestContext,
    ) -> Result<Vec<(String, Vec<EncryptedIndexListing>)>> {
        let (registry, owner, caller) = (self.txn_registry.clone(), owner.clone(), caller.clone());
        let all = self
            .encrypted(owner.txn().is_some(), move || async move {
                Ok(match owner.txn() {
                    Some(txn) => {
                        let txn = txn.to_string();
                        registry
                            .all_encrypted_indexes_in_txn_for(&owner, &caller, &txn)
                            .await
                            .map_err(db::EncryptedIndexError::from)?
                    }
                    None => registry.all_encrypted_indexes_as(&caller).await?,
                })
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
        owner: &RequestContext,
        caller: &RequestContext,
        collection: &str,
        field_name: &str,
    ) -> Result<()> {
        let (registry, owner, caller) = (self.txn_registry.clone(), owner.clone(), caller.clone());
        let (collection, field) = (collection.to_owned(), field_name.to_owned());
        self.encrypted(owner.txn().is_some(), move || async move {
            match owner.txn() {
                Some(txn) => {
                    let txn = txn.to_string();
                    registry
                        .delete_encrypted_index_in_txn_for(
                            &owner,
                            &caller,
                            &txn,
                            &collection,
                            &field,
                        )
                        .await
                        .map_err(db::EncryptedIndexError::from)?;
                }
                None => {
                    registry
                        .delete_encrypted_index_as(&caller, &collection, &field)
                        .await?;
                }
            }
            Ok(())
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
