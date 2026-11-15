//! Encrypted (searchable-encryption) indexes as Go's `AddEncryptedIndex`,
//! `ListEncryptedIndexes`, `ListAllEncryptedIndexes` and
//! `DeleteEncryptedIndex` keep them: on the
//! collection's definition. Each reads and writes the definition through the
//! transaction it is given, so in an explicit transaction the change is
//! visible to it at once and to others once it commits.

use std::sync::Arc;

use schema::{CollectionVersion, EncryptedIndexDescription};
use storage::corekv::{IterOptions, Key, OnCommit, Store};
use storage::keys::systemstore::{CollectionKey, CollectionNameKey};

use crate::collection::loader::load_collection_from_systemstore;
use crate::collection::Collection;
use crate::error::{Error, Result};
use crate::txn::DbTxn;
use crate::DB;

impl<S> DB<S>
where
    S: Store + 'static,
{
    /// Add an equality encrypted index on `field` of `collection` in a
    /// transaction of its own. A definition that changes before it commits
    /// runs it again against the new one, under the node's retry policy.
    pub async fn add_encrypted_index(
        self: &Arc<Self>,
        collection: &str,
        field: &str,
    ) -> Result<()> {
        let (db, collection, field) = (Arc::clone(self), collection.to_owned(), field.to_owned());
        self.transact_db(move |txn| {
            let (db, collection, field) = (Arc::clone(&db), collection.clone(), field.clone());
            Box::pin(async move { db.add_encrypted_index_in(txn, &collection, &field).await })
        })
        .await
    }

    /// Delete the encrypted index on `field` of `collection` in a
    /// transaction of its own, retried as
    /// [`add_encrypted_index`](Self::add_encrypted_index) is.
    pub async fn delete_encrypted_index(
        self: &Arc<Self>,
        collection: &str,
        field: &str,
    ) -> Result<()> {
        let (db, collection, field) = (Arc::clone(self), collection.to_owned(), field.to_owned());
        self.transact_db(move |txn| {
            let (db, collection, field) = (Arc::clone(&db), collection.clone(), field.clone());
            Box::pin(async move { db.delete_encrypted_index_in(txn, &collection, &field).await })
        })
        .await
    }
}

impl<S> DB<S>
where
    S: Store,
{
    /// Add an equality encrypted index on `field` of `collection`, in `txn`.
    pub async fn add_encrypted_index_in(
        &self,
        txn: &DbTxn<S>,
        collection: &str,
        field: &str,
    ) -> Result<()> {
        self.rewrite_definition_in(txn, collection, |schema| {
            if !schema.fields.iter().any(|f| f.name == field) {
                return Err(Error::EncryptedIndexFieldMissing(field.to_owned()));
            }
            if schema
                .encrypted_indexes
                .iter()
                .any(|i| i.field_name == field)
            {
                return Err(Error::EncryptedIndexExists(field.to_owned()));
            }
            schema
                .encrypted_indexes
                .push(EncryptedIndexDescription::new(field));
            Ok(())
        })
        .await
    }

    /// Delete the encrypted index on `field` of `collection`, in `txn`.
    pub async fn delete_encrypted_index_in(
        &self,
        txn: &DbTxn<S>,
        collection: &str,
        field: &str,
    ) -> Result<()> {
        self.rewrite_definition_in(txn, collection, |schema| {
            if !schema
                .encrypted_indexes
                .iter()
                .any(|i| i.field_name == field)
            {
                return Err(Error::EncryptedIndexMissing(field.to_owned()));
            }
            schema.encrypted_indexes.retain(|i| i.field_name != field);
            Ok(())
        })
        .await
    }

    /// The fields of `collection` that carry an encrypted index, as `txn`
    /// sees the definition.
    pub async fn encrypted_indexes_in(
        &self,
        txn: &DbTxn<S>,
        collection: &str,
    ) -> Result<Vec<String>> {
        let found = definition_in(txn, collection).await?;
        Ok(found
            .schema()
            .encrypted_indexes
            .iter()
            .map(|index| index.field_name.clone())
            .collect())
    }

    /// Every collection's encrypted index fields as `txn` sees the
    /// definitions, its own uncommitted changes included, by collection name
    /// in name order. A collection without one is left out, as Go's
    /// `ListAllEncryptedIndexes` leaves it out.
    pub async fn all_encrypted_indexes_in(
        &self,
        txn: &DbTxn<S>,
    ) -> Result<Vec<(String, Vec<String>)>> {
        let mut all = Vec::new();
        for name in collection_names_in(txn).await? {
            let found = definition_in(txn, &name).await?;
            if self.is_collection_forbidden(found.collection_id())?
                && !txn.was_collection_created(found.collection_id())
            {
                continue;
            }
            let fields: Vec<String> = found
                .schema()
                .encrypted_indexes
                .iter()
                .map(|index| index.field_name.clone())
                .collect();
            if !fields.is_empty() {
                all.push((name, fields));
            }
        }
        Ok(all)
    }

    /// Change `collection`'s active definition with `change` and stage it on
    /// `txn`. The rest of `txn` reads the new definition, and the shared
    /// cache takes it once `txn` commits.
    async fn rewrite_definition_in<F>(
        &self,
        txn: &DbTxn<S>,
        collection: &str,
        change: F,
    ) -> Result<()>
    where
        F: FnOnce(&mut CollectionVersion) -> Result<()>,
    {
        let found = definition_in(txn, collection).await?;
        crate::chunk::ensure_admin_changeable(found.schema())?;
        let mut updated = found.schema().clone();
        change(&mut updated)?;
        let data = serde_json::to_vec(&updated).map_err(|error| {
            Error::collection_schema_json(
                format!("failed to serialize schema for collection '{collection}'"),
                error,
            )
        })?;
        let systemstore = txn.systemstore()?;
        systemstore
            .set(&CollectionKey::new(&updated.version_id).bytes(), &data)
            .await?;
        systemstore
            .set(
                &CollectionNameKey::new(collection).bytes(),
                updated.version_id.as_bytes(),
            )
            .await?;
        let cached = Collection::load_index_actions(updated, &systemstore).await?;
        txn.uncache_collection(collection);
        let cache = Arc::clone(&self.collections);
        txn.on_commit(OnCommit::OrderedAndWait(Box::new(move || {
            cache.rcu(|old| {
                let mut cache = old.clone();
                cache.put(cached.clone());
                cache
            });
        })))?;
        Ok(())
    }
}

/// The names of the collections `txn` stores an active definition for.
async fn collection_names_in<S>(txn: &DbTxn<S>) -> Result<Vec<String>>
where
    S: Store,
{
    let prefix = CollectionNameKey::name_prefix();
    let systemstore = txn.systemstore()?;
    let mut iter = systemstore
        .iterator(IterOptions::new().with_prefix(prefix.clone()))
        .await
        .map_err(Error::Storage)?;
    let mut names = Vec::new();
    while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
        let name = pair
            .key
            .strip_prefix(prefix.as_slice())
            .unwrap_or(pair.key.as_slice());
        names.push(
            String::from_utf8(name.to_vec()).map_err(|error| {
                Error::text_decode("collection key contains invalid UTF-8", error)
            })?,
        );
    }
    iter.close().await.map_err(Error::Storage)?;
    Ok(names)
}

/// `collection`'s active definition as `txn` stores it.
async fn definition_in<S>(txn: &DbTxn<S>, collection: &str) -> Result<Collection>
where
    S: Store,
{
    load_collection_from_systemstore(&txn.systemstore()?, collection)
        .await?
        .ok_or_else(|| Error::CollectionNotFound(collection.to_owned()))
}
