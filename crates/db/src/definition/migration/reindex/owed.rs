//! The reindex a schema change owes its collection.
//!
//! Registering a migration, or switching a collection's active version, commits
//! first; rebuilding the collection's secondary indexes over the documents the
//! change migrates follows. That rebuild can fail (a lens that cannot run yet, a
//! storage error), and the change stands regardless. So the committing
//! transaction records the reindex it owes the collection, and only the rebuild
//! page that finishes clears the record, in its own transaction. The follow-up
//! after the commit settles it; one that fails is reported and stays owed,
//! settled again after the next schema change commits and at every open, from
//! the last page that committed. A crash between the commit and the rebuild
//! loses nothing.
//!
//! Each record carries a random token, and a settle clears only the record it
//! read: a reindex owed by a later change is never cleared by a rebuild that
//! ran before it.
//!
//! While a record stands, none of the collection's indexes answers queries
//! ([`reindex_owed`], read wherever index statuses are), so a query plans a
//! scan instead of reading entries that may describe documents at an older
//! version or be half rebuilt. Writing or clearing a record republishes the
//! cached collection when the transaction commits, in commit order.

use std::sync::Arc;

use datastore::NamespaceView;
use serde::{Deserialize, Serialize};
use storage::corekv::{IterOptions, OnCommit, Store};

use crate::collection::epoch::EpochedCollections;
use crate::error::{Error, Result};
use crate::DB;

/// Start of every owed-reindex key in the system store; the collection ID follows.
const OWED_PREFIX: &[u8] = b"/reindex/owed/";

#[derive(Serialize, Deserialize)]
pub(super) struct OwedReindex {
    pub(super) token: u64,
    /// The destination version of the migration that owes the reindex: the
    /// rebuild is needed only while the active version's history contains it.
    /// `None` after a version switch, which always needs it.
    migration_destination: Option<String>,
}

fn owed_key(collection_id: &str) -> Vec<u8> {
    [OWED_PREFIX, collection_id.as_bytes()].concat()
}

pub(super) async fn read_owed(
    systemstore: &NamespaceView,
    collection_id: &str,
) -> Result<Option<OwedReindex>> {
    systemstore
        .get(&owed_key(collection_id))
        .await
        .map_err(Error::Storage)?
        .map(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|error| Error::Other(format!("invalid owed reindex record: {error}")))
        })
        .transpose()
}

/// Whether a schema change owes `collection_id` a reindex that has not
/// settled, in which case none of its indexes answers queries.
pub(crate) async fn reindex_owed(systemstore: &NamespaceView, collection_id: &str) -> Result<bool> {
    systemstore
        .has(&owed_key(collection_id))
        .await
        .map_err(Error::Storage)
}

/// Records, in the transaction of `systemstore`, the reindex a schema change
/// owes `collection_id`.
pub(crate) async fn owe_reindex(
    systemstore: &NamespaceView,
    collections: &Arc<EpochedCollections>,
    collection_id: &str,
    migration_destination: Option<&str>,
) -> Result<()> {
    write_owed(
        systemstore,
        collections,
        collection_id,
        &OwedReindex {
            token: rand::random(),
            migration_destination: migration_destination.map(str::to_owned),
        },
    )
    .await
}

/// Records, in the transaction of `systemstore`, a reindex of every index
/// `collection_id` has, owed under `token` by the rebuild that writes it.
pub(super) async fn owe_full_reindex(
    systemstore: &NamespaceView,
    collections: &Arc<EpochedCollections>,
    collection_id: &str,
    token: u64,
) -> Result<()> {
    write_owed(
        systemstore,
        collections,
        collection_id,
        &OwedReindex {
            token,
            migration_destination: None,
        },
    )
    .await
}

async fn write_owed(
    systemstore: &NamespaceView,
    collections: &Arc<EpochedCollections>,
    collection_id: &str,
    owed: &OwedReindex,
) -> Result<()> {
    let bytes = serde_json::to_vec(owed)
        .map_err(|error| Error::Other(format!("failed to encode an owed reindex: {error}")))?;
    systemstore
        .set(&owed_key(collection_id), &bytes)
        .await
        .map_err(Error::Storage)?;
    publish_reindex_owed(systemstore, collections, collection_id, true).await
}

/// Whether the reindex owed to `collection_id` is still the one with `token`.
/// The read is validated, so a later change that owes the collection again
/// aborts a transaction that relied on it.
pub(super) async fn owes(
    systemstore: &NamespaceView,
    collection_id: &str,
    token: u64,
) -> Result<bool> {
    Ok(read_owed(systemstore, collection_id)
        .await?
        .is_some_and(|owed| owed.token == token))
}

/// Clears, in the transaction of `systemstore`, the reindex owed to
/// `collection_id` if its record is still the one with `token`. The read is
/// validated, so a later change that owes the collection again aborts the
/// clearing transaction.
pub(super) async fn clear_owed_reindex(
    systemstore: &NamespaceView,
    collections: &Arc<EpochedCollections>,
    collection_id: &str,
    token: u64,
) -> Result<()> {
    if owes(systemstore, collection_id, token).await? {
        systemstore
            .delete(&owed_key(collection_id))
            .await
            .map_err(Error::Storage)?;
        super::progress::clear(systemstore, collection_id).await?;
        publish_reindex_owed(systemstore, collections, collection_id, false).await?;
    }
    Ok(())
}

/// Once the transaction of `systemstore` commits, the cached collection
/// `collection_id` answers queries from its indexes again, or stops.
async fn publish_reindex_owed(
    systemstore: &NamespaceView,
    collections: &Arc<EpochedCollections>,
    collection_id: &str,
    owed: bool,
) -> Result<()> {
    let collections = Arc::clone(collections);
    let collection_id = collection_id.to_owned();
    systemstore
        .on_commit(OnCommit::OrderedAndWait(Box::new(move || {
            collections.rcu(|cache| {
                let mut cache = cache.clone();
                cache.update(&collection_id, |collection| {
                    collection.with_reindex_owed(owed)
                });
                cache
            });
        })))
        .await
        .map_err(Error::Storage)
}

/// What one settle pass did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReindexReport {
    /// Owed reindexes that ran, or turned out not to be needed, and were cleared.
    pub settled: usize,
    /// Owed reindexes whose rebuild failed; they stay owed.
    pub failed: usize,
}

impl<S> DB<S>
where
    S: Store,
{
    /// The IDs of the collections owed a reindex.
    pub async fn owed_reindexes(&self) -> Result<Vec<String>> {
        let txn = self.new_txn(true).await?;
        let mut collection_ids = Vec::new();
        {
            let systemstore = txn.systemstore()?;
            let mut iter = systemstore
                .iterator(
                    IterOptions::new()
                        .with_prefix(OWED_PREFIX.to_vec())
                        .with_keys_only(true),
                )
                .await
                .map_err(Error::Storage)?;
            while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
                if let Some(id) = pair.key.strip_prefix(OWED_PREFIX) {
                    collection_ids.push(String::from_utf8_lossy(id).into_owned());
                }
            }
            iter.close().await.map_err(Error::Storage)?;
        }
        txn.discard()?;
        Ok(collection_ids)
    }

    /// Settles every owed reindex once. A rebuild that fails is reported and
    /// stays owed.
    pub async fn settle_owed_reindexes(&self) -> Result<ReindexReport> {
        let mut report = ReindexReport::default();
        for collection_id in self.owed_reindexes().await? {
            match self.settle_owed_reindex(&collection_id).await {
                Ok(()) => report.settled += 1,
                Err(error) => {
                    report.failed += 1;
                    tracing::warn!(
                        %collection_id,
                        %error,
                        "A reindex owed after a schema change failed; it runs again after the next schema change and at open"
                    );
                }
            }
        }
        Ok(report)
    }

    /// The follow-up of a committed schema change: settle what is owed. The
    /// change stands whatever happens here; a failure is reported and stays owed.
    pub(crate) async fn settle_owed_reindexes_after_commit(&self) {
        if let Err(error) = self.settle_owed_reindexes().await {
            tracing::warn!(
                %error,
                "Owed reindexes could not be read; they are settled at the next schema change and at open"
            );
        }
    }

    async fn settle_owed_reindex(&self, collection_id: &str) -> Result<()> {
        let txn = self.new_txn(true).await?;
        let owed = read_owed(&txn.systemstore()?, collection_id).await?;
        txn.discard()?;
        let Some(owed) = owed else {
            return Ok(());
        };
        let rebuild = match self.find_collection_by_id(collection_id)? {
            Some(collection) => self
                .reindex_needed(&collection, owed.migration_destination.as_deref())
                .await?
                .then(|| collection.name().to_string()),
            None => None,
        };
        match rebuild {
            Some(name) => self.rebuild_settling(&name, owed.token).await,
            None => {
                let txn = self.new_txn(false).await?;
                clear_owed_reindex(
                    &txn.systemstore()?,
                    &self.collections,
                    collection_id,
                    owed.token,
                )
                .await?;
                txn.commit().await
            }
        }
    }
}
