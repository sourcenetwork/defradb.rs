//! Refuse secondary indexes whose on-disk key encoding changed.
//!
//! DateTime index keys were one varint of nanoseconds and are now a marker plus
//! separate seconds and nanos varints, so entries written by an older build are
//! unreadable by this one. Nothing rebuilds them on its own: the existing
//! reindex path is driven by schema migration, not by a storage format change.
//!
//! Left alone, an upgraded store answers a range query from bounds in the new
//! format against entries in the old one, so rows go missing, deletes orphan
//! their entries, and a unique index stops rejecting duplicates. Before 1.0,
//! refusing an older store keeps those incompatible entries invisible.

use storage::corekv::Store;
use tracing::{info, warn};

use crate::error::{Error, Result};
use crate::DB;

/// Bumped whenever an index key encoding changes. Version 1 is the original
/// single-varint DateTime encoding; version 2 splits seconds from nanos;
/// version 3 gives each deferred index entry its position record, which the
/// check at every open does for the deferred indexes alone; version 4 tags the
/// index keyspace so collection short IDs cannot collide with another family.
pub const CURRENT_INDEX_FORMAT: u32 = 4;

/// Systemstore key holding the format the indexes on disk were written with.
pub use storage::keys::systemstore::INDEX_FORMAT_KEY;

impl<S: Store> DB<S> {
    /// Refuse secondary indexes written by an older encoding, and, at every
    /// open, rebuild the indexes kept the other way on an earlier start.
    ///
    /// A store with no stamp is either brand new, where it has no collections,
    /// or written before the stamp existed. The latter is refused, rather than
    /// guessed at, because its key layout is incompatible.
    pub(crate) async fn migrate_index_format(&self) -> Result<()>
    where
        S: 'static,
    {
        let stored = self.stored_index_format().await?;
        if stored.is_some_and(|format| format != CURRENT_INDEX_FORMAT) {
            return Err(Error::UnsupportedIndexFormat {
                stored,
                current: CURRENT_INDEX_FORMAT,
            });
        }
        if stored.is_none() && !self.get_all_active_collections_internal()?.is_empty() {
            return Err(Error::UnsupportedIndexFormat {
                stored,
                current: CURRENT_INDEX_FORMAT,
            });
        }
        if stored.is_some_and(|format| format >= 2) {
            self.rebuild_indexes_kept_the_other_way().await?;
        } else {
            self.rebuild_every_index(stored).await?;
        }
        if stored == Some(CURRENT_INDEX_FORMAT) {
            return Ok(());
        }
        self.stamp_index_format().await
    }

    /// Rebuilds each index whose stored state belongs to the other way of
    /// keeping it than this start's types give it, and only those.
    async fn rebuild_indexes_kept_the_other_way(&self) -> Result<()>
    where
        S: 'static,
    {
        for schema in self.get_all_active_collections_internal()? {
            if schema.indexes.is_empty() {
                continue;
            }
            let Some(collection) = self.get_collection(&schema.name)? else {
                continue;
            };
            let short_id = collection.resolved_root_id();
            let txn = self.new_txn(true).await?;
            let mut changed = Vec::new();
            {
                let datastore = txn.datastore()?;
                for index in collection.write_indexes() {
                    let deferred =
                        crdt::deferred_index(index, collection.schema(), collection.crdts());
                    if crate::index::deferred::kept_in_other_mode(
                        &datastore, short_id, index.id, deferred,
                    )
                    .await?
                    {
                        changed.push(index.id);
                    }
                }
            }
            txn.discard()?;
            if changed.is_empty() {
                continue;
            }
            info!(
                collection = %schema.name,
                indexes = ?changed,
                "rebuilding indexes kept the other way on an earlier start"
            );
            if let Err(error) = self.reindex_indexes(&schema.name, changed).await {
                warn!(collection = %schema.name, error = %error, "index rebuild failed");
                return Err(error);
            }
        }
        Ok(())
    }

    async fn rebuild_every_index(&self, stored: Option<u32>) -> Result<()>
    where
        S: 'static,
    {
        let stale: Vec<String> = self
            .get_all_active_collections_internal()?
            .into_iter()
            .filter(|collection| !collection.indexes.is_empty())
            .map(|collection| collection.name)
            .collect();

        if !stale.is_empty() {
            info!(
                from = ?stored,
                to = CURRENT_INDEX_FORMAT,
                collections = stale.len(),
                "rebuilding secondary indexes for the new key encoding"
            );
        }

        for name in stale {
            if let Err(error) = self.reindex_collection_with_migrations(&name).await {
                // Leaving the stamp unwritten means the next open retries, which
                // is the honest outcome: a half-rebuilt store must not report
                // itself as current.
                warn!(collection = %name, error = %error, "index rebuild failed");
                return Err(error);
            }
        }
        Ok(())
    }

    /// The format stamped on disk, or `None` on a store that predates it.
    pub async fn stored_index_format(&self) -> Result<Option<u32>> {
        let txn = self.new_txn(true).await?;
        let systemstore = txn.systemstore()?;
        let raw = systemstore
            .get(INDEX_FORMAT_KEY)
            .await
            .map_err(Error::Storage)?;
        let _ = txn.discard();

        Ok(raw
            .and_then(|bytes| <[u8; 4]>::try_from(bytes.as_ref()).ok())
            .map(u32::from_be_bytes))
    }

    async fn stamp_index_format(&self) -> Result<()> {
        let txn = self.new_txn(false).await?;
        txn.systemstore()?
            .set(INDEX_FORMAT_KEY, &CURRENT_INDEX_FORMAT.to_be_bytes())
            .await
            .map_err(Error::Storage)?;
        txn.commit().await
    }
}
