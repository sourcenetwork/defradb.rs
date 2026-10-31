//! Purging documents by DocID: a hard delete on this node only, with no
//! CRDT operation and nothing replicated (Go `shinzo-develop`'s
//! `PurgeByDocIDs`). One `transact_retry` attempt removes every named
//! document; a conflict re-runs it on fresh reads. `proofs/tla/Purge.tla`
//! models it.

use std::sync::Arc;

use rapidhash::{HashSetExt, RapidHashSet};
use storage::corekv::{
    RetryAttemptError, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue, RetryTxn,
};
use storage::keys::{
    DataStoreKey, DatastoreSE, HeadstorePriorityKey, InstanceType, PrimaryDataStoreKey,
};

use super::*;

/// One attempt of a purge.
struct PurgeAttempt<S>
where
    S: Store,
{
    db: Arc<crate::database::DB<S>>,
    context: defra_core::RequestContext,
    collection: Collection,
    doc_ids: Vec<document::DocID>,
    prune_history: bool,
}

impl<S> RetryAttemptFn for PurgeAttempt<S>
where
    S: Store + 'static,
{
    fn run<'a>(&'a mut self, retry: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let io = retry.begin_attempt_io()?;
            let txn = self
                .db
                .new_db_txn(datastore::BasicTxn::from_retry_attempt_io(
                    io,
                    0,
                    self.db.crdts().clone(),
                ));
            let purged = self
                .run_in(&txn)
                .await
                .map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
            let (io, callbacks) = txn.into_retry_attempt()?;
            retry.finish_attempt_io(io, callbacks)?;
            Ok(Box::new(purged) as RetryAttemptValue)
        })
    }
}

impl<S> PurgeAttempt<S>
where
    S: Store + 'static,
{
    async fn run_in(&self, txn: &DbTxn<S>) -> Result<usize> {
        self.db
            .check_node_access_in_txn(
                &self.context,
                txn.acpstore()?,
                acp::nac::NodePermission::CollectionTruncate,
            )
            .await?;
        let collection = txn
            .load_collection_for_write_attempt(self.collection.name(), &self.collection)
            .await?;
        self.db
            .purge_documents(txn, &collection, &self.doc_ids, self.prune_history)
            .await
    }
}

impl<S> crate::database::DB<S>
where
    S: Store,
{
    /// [`Self::purge_by_doc_ids_as`] for `identity`, or for the identity acting in [`defra_core::RequestContext::ambient`] when there is none.
    pub async fn purge_by_doc_ids(
        self: &Arc<Self>,
        name: &str,
        doc_ids: &[document::DocID],
        prune_history: bool,
        identity: Option<&identity::Did>,
    ) -> Result<usize>
    where
        S: 'static,
    {
        self.purge_by_doc_ids_as(
            &crate::access::nac::ambient_as(identity),
            name,
            doc_ids,
            prune_history,
        )
        .await
    }

    /// Permanently remove the documents `doc_ids` name from this node: their
    /// values, index entries, primary keys, heads and every DocID alias. With
    /// `prune_history` every block of their histories that no other document
    /// owns goes too; without it the blocks stay, but no longer name the
    /// purged documents as owners. Nothing is replicated. A DocID that names
    /// no document is skipped. Returns how many documents were purged, once
    /// the purge has committed. Checked against the node permission of a
    /// collection truncate.
    #[instrument(skip(self, context, doc_ids), fields(collection = %name, docs = doc_ids.len()), name = "db.purge_by_doc_ids")]
    pub async fn purge_by_doc_ids_as(
        self: &Arc<Self>,
        context: &defra_core::RequestContext,
        name: &str,
        doc_ids: &[document::DocID],
        prune_history: bool,
    ) -> Result<usize>
    where
        S: 'static,
    {
        let collection = self
            .get_collection(name)?
            .ok_or_else(|| Error::CollectionNotFound(name.to_string()))?;
        let attempt = PurgeAttempt {
            db: Arc::clone(self),
            context: context.clone(),
            collection,
            doc_ids: doc_ids.to_vec(),
            prune_history,
        };
        let purged = self
            .store()
            .transact_retry(self.options().retry_options(), Box::new(attempt))
            .await
            .map_err(|error| match error.downcast_abort::<Error>() {
                Ok(error) => error,
                Err(error) => Error::Storage(error.into_storage()),
            })?;
        let purged = *purged
            .downcast::<usize>()
            .map_err(|_| Error::Other("invalid purge result".into()))?;
        tracing::info!(collection = %name, purged, prune_history, "Purged documents");
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
        prune_history: bool,
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

            if prune_history {
                crate::block::cleanup::delete_owned_dag_for_owners(
                    &blockstore,
                    &systemstore,
                    &block_cids,
                    aliases,
                    collection.schema().is_branchable,
                )
                .await?;
            } else {
                crate::block::cleanup::drop_dag_owners(
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
