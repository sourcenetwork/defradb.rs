use std::sync::Arc;

use futures::StreamExt;

use super::*;

/// Documents deleted at once, each in its own attempt.
const IN_FLIGHT: usize = 64;

impl<S: Store> crate::database::DB<S> {
    /// Delete every document of `collection` committed before the truncate
    /// began, with its heads, blocks and index entries, keeping the
    /// collection; the number of documents.
    ///
    /// Each document goes in its own attempt, so a write to one document
    /// re-runs only that document's delete.
    pub(super) async fn truncate_all(self: &Arc<Self>, collection: &Collection) -> Result<usize>
    where
        S: 'static,
    {
        let doc_ids = self.collect_doc_short_ids(collection).await?;
        let mut deletes = futures::stream::iter(doc_ids.iter().copied())
            .map(|doc_short_id| self.truncate_doc(collection, doc_short_id))
            .buffer_unordered(IN_FLIGHT);
        while let Some(deleted) = deletes.next().await {
            deleted?;
        }
        drop(deletes);
        self.truncate_collection_metadata(collection.resolved_root_id())
            .await?;
        Ok(doc_ids.len())
    }

    /// Collect all doc short IDs for a collection using a read-only transaction.
    async fn collect_doc_short_ids(&self, collection: &Collection) -> Result<Vec<u64>> {
        use storage::keys::doc_id_index::decode_doc_short_id;

        let txn = self.new_txn(true).await?;
        let doc_ids;
        {
            let datastore = txn.datastore()?;
            let doc_prefix = collection.collection_key_prefix();
            let prefix_len = doc_prefix.len();
            let opts = IterOptions::new().with_prefix(doc_prefix);
            let mut iter = datastore.iterator(opts).await.map_err(Error::Storage)?;
            let mut ids = Vec::new();
            while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
                if let Ok(doc_short_id) = decode_doc_short_id(&pair.key[prefix_len..]) {
                    ids.push(doc_short_id);
                }
            }
            iter.close().await.map_err(Error::Storage)?;
            doc_ids = ids;
        }
        let _ = txn.discard();
        Ok(doc_ids)
    }

    /// Delete one document in an engine-retried write transaction: a
    /// conflict with a write to it runs the delete again on fresh reads.
    async fn truncate_doc(
        self: &Arc<Self>,
        collection: &Collection,
        doc_short_id: u64,
    ) -> Result<()>
    where
        S: 'static,
    {
        let db = Arc::clone(self);
        let collection = collection.clone();
        self.transact_db(move |txn| {
            let db = Arc::clone(&db);
            let collection = collection.clone();
            Box::pin(async move { db.truncate_doc_in(txn, &collection, doc_short_id).await })
        })
        .await
    }

    async fn truncate_doc_in(
        &self,
        txn: &DbTxn<S>,
        collection: &Collection,
        doc_short_id: u64,
    ) -> Result<()> {
        use storage::keys::HeadstorePriorityKey;

        let datastore = txn.datastore()?;
        let headstore = txn.headstore()?;
        let blockstore = txn.blockstore()?;
        let systemstore = txn.systemstore()?;

        let Some(doc_id) = crate::docid::map::get_doc_id(&systemstore, doc_short_id).await? else {
            // A delete that won a race with this one already removed it.
            return Ok(());
        };
        let canonical = document::DocID::from_string(&doc_id)?;
        // Index entries are keyed by the field values, so they go before
        // the values do.
        if let Some((doc, _)) = collection
            .get_with_datastore_include_deleted(&datastore, doc_short_id, &canonical, true)
            .await?
        {
            crate::IndexManager::from_indexes(
                collection.resolved_root_id(),
                collection.schema(),
                collection.write_indexes(),
            )?
            .with_vector_caches(self.vector_codes())
            .on_document_delete(&datastore, &doc, doc_short_id, collection.schema())
            .await?;
        }
        for key in [
            collection.doc_key(doc_short_id),
            collection.deleted_key(doc_short_id),
            collection.version_key(doc_short_id),
        ] {
            datastore.delete(&key).await.map_err(Error::Storage)?;
        }

        let heads = crate::block::heads::document::load(&headstore, doc_short_id)
            .await
            .map_err(Error::Storage)?;
        let block_cids = crate::block::heads::document::live_cids(&heads);
        headstore
            .delete(&crate::block::heads::document::set_key(doc_short_id))
            .await
            .map_err(Error::Storage)?;
        delete_prefix(
            &headstore,
            HeadstorePriorityKey::document_prefix(doc_short_id),
        )
        .await?;

        datastore
            .project_document(collection.name(), &doc_id)
            .await
            .map_err(Error::Storage)?;
        crate::block::cleanup::delete_owned_dag(&blockstore, &systemstore, &block_cids, &doc_id)
            .await?;

        // Clear the identity mappings so recreating identical content
        // does not trip the create duplicate check.
        crate::docid::map::delete_doc_id_mappings(&systemstore, doc_short_id).await
    }

    /// Delete collection-level metadata (a view's cache, collection heads and
    /// the collection DAG) in one engine-retried write transaction. Document
    /// keys and index entries are left to the per-document deletes, since a
    /// prefix wipe would also take documents created after the truncate began.
    async fn truncate_collection_metadata(self: &Arc<Self>, short_id: u32) -> Result<()>
    where
        S: 'static,
    {
        self.transact_db(move |txn| Box::pin(Self::truncate_metadata_in(txn, short_id)))
            .await
    }

    async fn truncate_metadata_in(txn: &DbTxn<S>, short_id: u32) -> Result<()> {
        use storage::keys::datastore::ViewCacheKey;
        use storage::keys::{HeadstoreColPublishedKey, HeadstoreColSetKey};

        let datastore = txn.datastore()?;
        let headstore = txn.headstore()?;
        let blockstore = txn.blockstore()?;
        let systemstore = txn.systemstore()?;

        // A materialized view's rows live in its cache, not as documents.
        delete_prefix(&datastore, ViewCacheKey::collection_prefix(short_id)).await?;

        // Collect block CIDs from the collection's heads, then delete them
        // with the guards that record their publication.
        let block_cids = crate::block::heads::live_collection_heads(&headstore, short_id)
            .await
            .map_err(Error::Storage)?
            .live;
        headstore
            .delete(&HeadstoreColSetKey::new(short_id).bytes())
            .await
            .map_err(Error::Storage)?;
        delete_prefix(
            &headstore,
            HeadstoreColPublishedKey::collection_prefix(short_id),
        )
        .await?;

        crate::block::cleanup::delete_owned_dag(&blockstore, &systemstore, &block_cids, "").await?;
        Ok(())
    }
}
