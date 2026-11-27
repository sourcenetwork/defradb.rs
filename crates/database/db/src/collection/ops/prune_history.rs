//! Pruning a collection's history to its `@retention(maxDepth: N)`: each
//! document keeps the composite commits of its top N heights and its live
//! heads, and loses the older composites and every field block no kept
//! composite links and no field still has as its head. Nothing is replicated.
//!
//! A snapshot scan lists the documents; each one is then pruned in its own
//! engine-retried attempt that reads its heads, its commits and the owners of
//! every block it deletes. A block another document comes to own while the
//! attempt runs makes it run again. The priority index entries of pruned
//! commits stay: they record that the commits were published, so a pruned
//! commit a peer sends again is recognised as applied and never becomes a
//! head again, and one a peer fetches back is taken again by the next prune.
//! Model: `proofs/tla/HistoryRetention.tla`.

use std::sync::Arc;

use cid::Cid;
use defra_core::CrdtDelta;
use rapidhash::{HashSetExt, RapidHashSet};
use storage::keys::doc_id_index::decode_doc_short_id;
use storage::keys::{DELETED_KEY_PREFIX, DOC_KEY_PREFIX};

use super::*;

impl<S> crate::database::DB<S>
where
    S: Store,
{
    /// Prune the history of every document of the collection `name` to the
    /// collection's `@retention(maxDepth: N)`: the composite commits more than
    /// N heights below a document's highest commit go, with the field blocks
    /// no kept commit links, unless they are live heads or another document
    /// owns them. A collection without `@retention` keeps everything. Returns
    /// how many composite commits were pruned. Checked against the node
    /// permission of a collection truncate, before the collection is looked
    /// up. Refused first inside an explicit transaction, since each document
    /// is pruned in a transaction of its own.
    #[instrument(skip(self, context), fields(collection = %name), name = "db.prune_history")]
    pub async fn prune_history_as(
        self: &Arc<Self>,
        context: &defra_core::RequestContext,
        name: &str,
    ) -> Result<usize>
    where
        S: 'static,
    {
        if context.txn().is_some() {
            return Err(Error::HistoryPruneInTransaction);
        }
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionTruncate)
            .await?;
        let collection = self
            .get_collection(name)?
            .ok_or_else(|| Error::CollectionNotFound(name.to_string()))?;
        if collection.schema().retention_max_depth.is_none() {
            return Ok(0);
        }
        let mut pruned = 0;
        for doc_short_id in self.history_doc_short_ids(&collection).await? {
            pruned += self
                .prune_doc_history(context, &collection, doc_short_id)
                .await?;
        }
        tracing::info!(collection = %name, pruned, "Pruned collection history");
        Ok(pruned)
    }

    /// The short IDs of the collection's documents, deleted ones included,
    /// from a snapshot: the scan lists, it decides nothing.
    async fn history_doc_short_ids(&self, collection: &Collection) -> Result<Vec<u64>> {
        let txn = self.new_txn(true).await?;
        let mut ids = Vec::new();
        {
            let datastore = txn.datastore()?;
            for prefix in [DOC_KEY_PREFIX, DELETED_KEY_PREFIX] {
                let mut prefix = prefix.to_vec();
                prefix.extend_from_slice(collection.schema().collection_id.as_bytes());
                prefix.push(b'/');
                let prefix_len = prefix.len();
                let mut iter = datastore
                    .iterator(IterOptions::new().with_prefix(prefix))
                    .await
                    .map_err(Error::Storage)?;
                while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
                    if let Ok(doc_short_id) = decode_doc_short_id(&pair.key[prefix_len..]) {
                        ids.push(doc_short_id);
                    }
                }
                iter.close().await.map_err(Error::Storage)?;
            }
        }
        let _ = txn.discard();
        ids.sort_unstable();
        ids.dedup();
        Ok(ids)
    }

    /// Prune one document in one engine-retried attempt, against the
    /// collection definition that attempt reads.
    async fn prune_doc_history(
        self: &Arc<Self>,
        context: &defra_core::RequestContext,
        collection: &Collection,
        doc_short_id: u64,
    ) -> Result<usize>
    where
        S: 'static,
    {
        let db = Arc::clone(self);
        let (context, collection) = (context.clone(), collection.clone());
        self.transact_db(move |txn| {
            let (db, context, collection) = (Arc::clone(&db), context.clone(), collection.clone());
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
                let Some(max_depth) = collection.schema().retention_max_depth else {
                    return Ok(0);
                };
                prune_doc_in(txn, doc_short_id, max_depth).await
            })
        })
        .await
    }
}

/// Delete the document's composites more than `max_depth` heights below its
/// highest one, except its live heads, and every field block of the document
/// that neither a kept composite links nor a field has as its live head; the
/// number of composites deleted.
async fn prune_doc_in<S>(txn: &DbTxn<S>, doc_short_id: u64, max_depth: u64) -> Result<usize>
where
    S: Store,
{
    let headstore = txn.headstore()?;
    let blockstore = txn.blockstore()?;
    let systemstore = txn.systemstore()?;
    let Some(doc_id) = crate::docid::map::get_doc_id(&systemstore, doc_short_id).await? else {
        return Ok(0);
    };
    let heads: RapidHashSet<Cid> = crate::block::heads::document::live_cids(
        &crate::block::heads::document::load(&headstore, doc_short_id)
            .await
            .map_err(Error::Storage)?,
    )
    .into_iter()
    .collect();

    let mut composites = Vec::new();
    let mut fields = Vec::new();
    let mut seen = RapidHashSet::new();
    for (priority, cid) in
        crate::block::priority::document_entries(&headstore, doc_short_id).await?
    {
        if !seen.insert(cid) {
            continue;
        }
        let Some(block) = crate::block::cleanup::load_block(&blockstore, &cid).await? else {
            continue;
        };
        if matches!(block.delta, CrdtDelta::Composite(_)) {
            composites.push((priority, cid));
        } else {
            fields.push(cid);
        }
    }
    let Some(top) = composites.iter().map(|(priority, _)| *priority).max() else {
        return Ok(0);
    };
    let cutoff = top.saturating_sub(max_depth);
    let (pruned, kept): (Vec<_>, Vec<_>) = composites
        .into_iter()
        .partition(|(priority, cid)| *priority <= cutoff && !heads.contains(cid));
    // A field's live head stays even when only a pruned composite links it:
    // the field's next write and merge build on it.
    let mut retained =
        crate::block::cleanup::referenced_blocks(&blockstore, kept.into_iter().map(|(_, cid)| cid))
            .await?;
    for cid in &heads {
        retained.insert(*cid);
        if let Some(block) = crate::block::cleanup::load_block(&blockstore, cid).await? {
            retained.extend(block.encryption);
        }
    }
    for (_, cid) in &pruned {
        crate::block::cleanup::delete_owned_commit(
            &blockstore,
            &systemstore,
            cid,
            &doc_id,
            &retained,
        )
        .await?;
    }
    // A field block an earlier prune kept as its field's head is linked only
    // by a composite that prune deleted, so no pruned composite reaches it
    // once a newer write supersedes it: the priority index still names it.
    for cid in fields.iter().filter(|cid| !retained.contains(cid)) {
        crate::block::cleanup::delete_owned_block(
            &blockstore,
            &systemstore,
            cid,
            &doc_id,
            &retained,
        )
        .await?;
    }
    Ok(pruned.len())
}
