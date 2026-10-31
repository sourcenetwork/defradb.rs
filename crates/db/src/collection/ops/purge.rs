//! Purging documents by DocID: a hard delete on this node only, with no
//! CRDT operation and nothing replicated. The document's block history goes
//! with it, unless its collection is branchable, whose collection DAG links
//! that history. One engine-retried attempt removes every named document; a
//! conflict re-runs it on fresh reads. `proofs/tla/Purge.tla` models it.

use std::sync::Arc;

use rapidhash::{HashSetExt, RapidHashSet};
use storage::keys::{
    DataStoreKey, DatastoreSE, HeadstorePriorityKey, InstanceType, PrimaryDataStoreKey,
};

use super::*;

impl<S> crate::database::DB<S>
where
    S: Store,
{
    /// [`Self::purge_by_doc_ids_as`] for `identity`, or for the identity
    /// acting in [`defra_core::RequestContext::ambient`] when there is none.
    pub async fn purge_by_doc_ids(
        self: &Arc<Self>,
        name: &str,
        doc_ids: &[document::DocID],
        identity: Option<&identity::Did>,
    ) -> Result<usize>
    where
        S: 'static,
    {
        self.purge_by_doc_ids_as(&crate::access::nac::ambient_as(identity), name, doc_ids)
            .await
    }

    /// Permanently remove the documents `doc_ids` name from this node: their
    /// values, index entries, primary keys, heads and every DocID alias, and
    /// every block of their histories that no other document owns. In a
    /// branchable collection the blocks stay, since the collection DAG links
    /// them, but no longer name the purged documents as owners. Nothing is
    /// replicated. A DocID that names no document is skipped. Returns how
    /// many documents were purged, once the purge has committed. Checked
    /// against the node permission of a collection truncate.
    #[instrument(skip(self, context, doc_ids), fields(collection = %name, docs = doc_ids.len()), name = "db.purge_by_doc_ids")]
    pub async fn purge_by_doc_ids_as(
        self: &Arc<Self>,
        context: &defra_core::RequestContext,
        name: &str,
        doc_ids: &[document::DocID],
    ) -> Result<usize>
    where
        S: 'static,
    {
        let collection = self
            .get_collection(name)?
            .ok_or_else(|| Error::CollectionNotFound(name.to_string()))?;
        let db = Arc::clone(self);
        let context = context.clone();
        let doc_ids = doc_ids.to_vec();
        let purged = self
            .transact_db(move |txn| {
                let (db, context, collection, doc_ids) = (
                    Arc::clone(&db),
                    context.clone(),
                    collection.clone(),
                    doc_ids.clone(),
                );
                Box::pin(async move {
                    db.check_node_access_in_txn(
                        &context,
                        txn.acpstore()?,
                        acp::nac::NodePermission::CollectionTruncate,
                    )
                    .await?;
                    let collection = txn
                        .load_collection_for_write_attempt(collection.name(), &collection)
                        .await?;
                    db.purge_documents(txn, &collection, &doc_ids).await
                })
            })
            .await?;
        tracing::info!(collection = %name, purged, "Purged documents");
        Ok(purged)
    }

    /// Remove the documents `doc_ids` name from `collection` in `txn`, and
    /// return how many there were. Each document is counted once, however
    /// many of its aliases `doc_ids` names; a DocID with no document is
    /// skipped.
    pub(crate) async fn purge_documents(
        &self,
        txn: &DbTxn<S>,
        collection: &Collection,
        doc_ids: &[document::DocID],
    ) -> Result<usize> {
        let datastore = txn.datastore()?;
        let headstore = txn.headstore()?;
        let blockstore = txn.blockstore()?;
        let systemstore = txn.systemstore()?;

        let index_manager = crate::IndexManager::from_indexes(
            collection.resolved_root_id(),
            collection.schema(),
            collection.write_indexes(),
        )?
        .with_vector_caches(self.vector_codes());
        let mut seen = RapidHashSet::new();
        let mut targets = Vec::with_capacity(doc_ids.len());
        let mut aliases_to_delete = RapidHashSet::new();

        for doc_id in doc_ids {
            let Some((doc_short_id, canonical_doc_id)) = collection
                .resolve_doc_identity(&systemstore, doc_id)
                .await?
            else {
                continue;
            };
            if !seen.insert(doc_short_id) {
                continue;
            }
            let mut aliases =
                crate::docid::map::get_doc_id_aliases(&systemstore, doc_short_id).await?;
            let canonical = canonical_doc_id.to_string();
            if !aliases.contains(&canonical) {
                aliases.push(canonical);
            }
            aliases_to_delete.extend(aliases.iter().cloned());
            targets.push((doc_short_id, canonical_doc_id, aliases));
        }
        if targets.is_empty() {
            return Ok(0);
        }

        let se_prefix = DatastoreSE::collection_prefix(collection.collection_id());
        let mut se_iter = datastore
            .iterator(IterOptions::new().with_prefix(se_prefix))
            .await
            .map_err(Error::Storage)?;
        let mut se_keys = Vec::new();
        while let Some(pair) = se_iter.next().await.map_err(Error::Storage)? {
            if extract_last_path_segment_str(&pair.key)
                .is_some_and(|doc_id| aliases_to_delete.contains(&doc_id))
            {
                se_keys.push(pair.key.to_vec());
            }
        }
        se_iter.close().await.map_err(Error::Storage)?;
        for key in se_keys {
            datastore.delete(&key).await.map_err(Error::Storage)?;
        }

        for (doc_short_id, canonical_doc_id, aliases) in &targets {
            let doc_short_id = *doc_short_id;
            // Index entries are keyed by the field values, so they go before
            // the values do.
            if let Some((doc, _)) = collection
                .get_with_datastore_include_deleted(
                    &datastore,
                    doc_short_id,
                    canonical_doc_id,
                    true,
                )
                .await?
            {
                index_manager
                    .on_document_delete(&datastore, &doc, doc_short_id, collection.schema())
                    .await?;
            }
            for key in [
                collection.doc_key(doc_short_id),
                collection.deleted_key(doc_short_id),
                collection.version_key(doc_short_id),
                PrimaryDataStoreKey::new(collection.resolved_root_id(), doc_short_id).bytes(),
            ] {
                datastore.delete(&key).await.map_err(Error::Storage)?;
            }
            for instance_type in [
                InstanceType::Value,
                InstanceType::Priority,
                InstanceType::Deleted,
            ] {
                delete_prefix(
                    &datastore,
                    DataStoreKey::document_prefix(
                        collection.resolved_root_id(),
                        instance_type,
                        doc_short_id,
                    ),
                )
                .await?;
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

            if collection.schema().is_branchable {
                crate::block::cleanup::drop_dag_owners(
                    &blockstore,
                    &systemstore,
                    &block_cids,
                    aliases,
                )
                .await?;
            } else {
                crate::block::cleanup::delete_owned_dag_for_owners(
                    &blockstore,
                    &systemstore,
                    &block_cids,
                    aliases,
                )
                .await?;
            }
            crate::docid::map::delete_doc_id_mappings(&systemstore, doc_short_id).await?;
        }
        Ok(targets.len())
    }
}
