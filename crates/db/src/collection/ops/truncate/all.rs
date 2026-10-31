use super::*;

const CHUNK_SIZE: usize = 1000;

impl<S: Store> crate::database::DB<S> {
    /// Delete every document of `collection`, with its heads, blocks and
    /// index entries, keeping the collection; the number of documents.
    ///
    /// Deletes run in chunks, so no one transaction holds the whole write set.
    pub(super) async fn truncate_all(&self, collection: &Collection) -> Result<usize> {
        let doc_ids = self.collect_doc_short_ids(collection).await?;
        for chunk in doc_ids.chunks(CHUNK_SIZE) {
            self.truncate_chunk(collection, chunk).await?;
        }
        self.truncate_collection_metadata(
            collection.collection_id(),
            collection.resolved_root_id(),
        )
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

    /// Delete data for a chunk of doc short IDs within a single write transaction.
    async fn truncate_chunk(&self, collection: &Collection, doc_short_ids: &[u64]) -> Result<()> {
        use storage::keys::HeadstorePriorityKey;

        let txn = self.new_txn(false).await?;
        let datastore = txn.datastore()?;
        let headstore = txn.headstore()?;
        let blockstore = txn.blockstore()?;
        let systemstore = txn.systemstore()?;

        let result: Result<()> = async {
            for &doc_short_id in doc_short_ids {
                datastore
                    .delete(&collection.doc_key(doc_short_id))
                    .await
                    .map_err(Error::Storage)?;
                datastore
                    .delete(&collection.deleted_key(doc_short_id))
                    .await
                    .map_err(Error::Storage)?;
                datastore
                    .delete(&collection.version_key(doc_short_id))
                    .await
                    .map_err(Error::Storage)?;

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
            }
            Ok(())
        }
        .await;

        // Drop namespace views before commit/discard (they hold Arc refs to txn internals)
        drop(datastore);
        drop(headstore);
        drop(blockstore);
        drop(systemstore);

        match result {
            Ok(()) => {
                txn.commit().await?;
                Ok(())
            }
            Err(e) => {
                if let Err(discard_err) = txn.discard() {
                    tracing::warn!(
                        error = %discard_err,
                        original_error = %e,
                        "Transaction discard failed during truncate chunk"
                    );
                }
                Err(e)
            }
        }
    }

    /// Delete collection-level metadata: index entries and collection heads.
    async fn truncate_collection_metadata(&self, collection_id: &str, short_id: u32) -> Result<()> {
        use storage::keys::{HeadstoreColPublishedKey, HeadstoreColSetKey, IndexDataStoreKey};

        let txn = self.new_txn(false).await?;
        let datastore = txn.datastore()?;
        let headstore = txn.headstore()?;
        let blockstore = txn.blockstore()?;
        let systemstore = txn.systemstore()?;

        let result: Result<()> = async {
            // Delete index entries
            let idx_prefix = IndexDataStoreKey::collection_prefix(short_id);
            delete_prefix(&datastore, idx_prefix).await?;

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

            crate::block::cleanup::delete_owned_dag(&blockstore, &systemstore, &block_cids, "")
                .await?;

            // Delete the top-level doc/del/version prefixes
            let doc_prefix = format!("/d/{}/", collection_id).into_bytes();
            delete_prefix(&datastore, doc_prefix).await?;
            let del_prefix = format!("/del/{}/", collection_id).into_bytes();
            delete_prefix(&datastore, del_prefix).await?;
            let version_prefix = format!("/v/{}/", collection_id).into_bytes();
            delete_prefix(&datastore, version_prefix).await?;

            Ok(())
        }
        .await;

        drop(datastore);
        drop(headstore);
        drop(blockstore);
        drop(systemstore);

        match result {
            Ok(()) => {
                txn.commit().await?;
                Ok(())
            }
            Err(e) => {
                if let Err(discard_err) = txn.discard() {
                    tracing::warn!(
                        error = %discard_err,
                        original_error = %e,
                        "Transaction discard failed during truncate metadata"
                    );
                }
                Err(e)
            }
        }
    }
}
