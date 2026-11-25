use std::sync::Arc;

use super::*;

const CHUNK_SIZE: usize = 1000;

impl<S: Store> crate::database::DB<S> {
    /// Delete every document of `collection`, with its heads, blocks, index
    /// entries and the access-control objects this node holds for it, keeping
    /// the collection; the number of documents.
    ///
    /// Deletes run in chunks, so no one transaction holds the whole write set.
    pub(super) async fn truncate_all(self: &Arc<Self>, collection: &Collection) -> Result<usize>
    where
        S: 'static,
    {
        // `@chunk` collections go first. If a parent is written meanwhile, the worst
        // case is stray chunks no parent shows.
        for chunked in &collection.schema().chunked_fields {
            let name = chunked.collection(collection.name());
            let chunks_col = self
                .get_collection(&name)?
                .ok_or(Error::CollectionNotFound(name))?;
            self.truncate_documents(&chunks_col).await?;
        }
        self.truncate_documents(collection).await
    }

    async fn truncate_documents(self: &Arc<Self>, collection: &Collection) -> Result<usize>
    where
        S: 'static,
    {
        let doc_ids = self.collect_doc_short_ids(collection).await?;
        for chunk in doc_ids.chunks(CHUNK_SIZE) {
            self.truncate_chunk(collection, chunk).await?;
        }
        self.truncate_deferred_indexes(collection).await?;
        self.truncate_collection_metadata(collection).await?;
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

    /// Delete data for a chunk of doc short IDs in one engine-retried write
    /// transaction: a conflict on an owner scan runs the chunk again.
    async fn truncate_chunk(
        self: &Arc<Self>,
        collection: &Collection,
        doc_short_ids: &[u64],
    ) -> Result<()>
    where
        S: 'static,
    {
        let collection = collection.clone();
        let doc_short_ids = doc_short_ids.to_vec();
        let acp = self.document_acp().cloned();
        self.transact_db(move |txn| {
            let collection = collection.clone();
            let doc_short_ids = doc_short_ids.clone();
            let acp = acp.clone();
            Box::pin(async move {
                Self::truncate_chunk_in(txn, acp.as_ref(), &collection, &doc_short_ids).await
            })
        })
        .await
    }

    async fn truncate_chunk_in(
        txn: &DbTxn<S>,
        acp: Option<&Arc<dyn acp::DocumentACP>>,
        collection: &Collection,
        doc_short_ids: &[u64],
    ) -> Result<()> {
        use storage::keys::HeadstorePriorityKey;

        let datastore = txn.datastore()?;
        let headstore = txn.headstore()?;
        let blockstore = txn.blockstore()?;
        let systemstore = txn.systemstore()?;
        for &doc_short_id in doc_short_ids {
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

            let doc_id = crate::docid::map::get_doc_id(&systemstore, doc_short_id)
                .await?
                .ok_or_else(|| {
                    Error::InvalidDocument(format!(
                        "document short ID {doc_short_id} has no canonical DocID"
                    ))
                })?;
            datastore
                .project_document(collection.name(), &doc_id)
                .await
                .map_err(Error::Storage)?;
            crate::block::cleanup::delete_owned_dag(
                &blockstore,
                &systemstore,
                &block_cids,
                &doc_id,
            )
            .await?;

            // Clear the identity mappings so recreating identical content
            // does not trip the create duplicate check.
            crate::docid::map::delete_doc_id_mappings(&systemstore, doc_short_id).await?;
            crate::collection::acp::release_purged_doc_objects(
                acp,
                txn.acpstore()?,
                collection.schema(),
                [doc_id.as_str()],
            )
            .await?;
        }
        Ok(())
    }

    /// Delete collection-level metadata (index entries, collection heads and
    /// the collection DAG) in one engine-retried write transaction.
    async fn truncate_collection_metadata(self: &Arc<Self>, collection: &Collection) -> Result<()>
    where
        S: 'static,
    {
        let collection = collection.clone();
        self.transact_db(move |txn| {
            let collection = collection.clone();
            Box::pin(async move { Self::truncate_metadata_in(txn, &collection).await })
        })
        .await
    }

    async fn truncate_metadata_in(txn: &DbTxn<S>, collection: &Collection) -> Result<()> {
        use storage::keys::datastore::ViewCacheKey;
        use storage::keys::{
            DatastoreSE, HeadstoreColPublishedKey, HeadstoreColSetKey, IndexDataStoreKey,
        };

        let collection_id = collection.collection_id();
        let short_id = collection.resolved_root_id();

        let datastore = txn.datastore()?;
        let headstore = txn.headstore()?;
        let blockstore = txn.blockstore()?;
        let systemstore = txn.systemstore()?;

        // Every version's definition is read in this transaction, after the
        // chunks, so it lists any index that can hold an entry for a deleted
        // document (proofs/tla/TruncateIndexCleanup.tla). Versions, not the
        // one the truncate began with: a patch can activate another meanwhile,
        // and an index can be added through it.
        let versions = crate::definition::loader::get_collections_by_collection_id(
            &systemstore,
            collection_id,
        )
        .await?;
        let mut index_ids: Vec<u32> = versions
            .iter()
            .flat_map(|version| version.indexes.iter())
            .filter(|index| !index.is_vector())
            .map(|index| index.id)
            .collect();
        index_ids.sort_unstable();
        index_ids.dedup();

        // Index entries go one declared index at a time, failed builds
        // included: a short ID below 240 is one raw byte, so the collection's
        // own prefix can be the start of another family (`/d/` for short ID
        // 100) and cover every collection's keys (proofs/lean/IndexPrefix.lean).
        for index_id in index_ids {
            delete_prefix(
                &datastore,
                IndexDataStoreKey::index_prefix(short_id, index_id),
            )
            .await?;
        }
        // Searchable-encryption artifacts, including those a replicator
        // received for documents it may not hold (Go collection_truncate.go).
        delete_prefix(&datastore, DatastoreSE::collection_prefix(collection_id)).await?;
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

        for prefix in ["/d/", "/del/", "/v/"] {
            delete_prefix(&datastore, format!("{prefix}{collection_id}/").into_bytes()).await?;
        }
        Ok(())
    }
}
