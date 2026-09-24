use std::sync::Arc;

use async_lock::{Mutex, RwLock, RwLockReadGuardArc, RwLockWriteGuardArc};
use storage::corekv::{Key, Store};
use storage::keys::systemstore::CollectionKey;

use crate::collection::Collection;
use crate::database::DB;
use crate::error::{Error, Result};
use crate::txn::DbTxn;

impl<S: Store> DB<S> {
    fn collection_lock(&self, collection_id: &str) -> Result<Arc<RwLock<()>>> {
        if let Some(lock) = self.collection_locks.get(collection_id) {
            return Ok(lock);
        }
        Ok(self
            .collection_locks
            .get_or_insert(collection_id.to_string(), Arc::new(RwLock::new(()))))
    }

    pub(crate) async fn collection_read_guard(
        &self,
        collection_id: &str,
    ) -> Result<RwLockReadGuardArc<()>> {
        let lock = self.collection_lock(collection_id)?;
        tracing::trace!(%collection_id, "waiting for the collection guard");
        let guard = lock.read_arc().await;
        tracing::trace!(%collection_id, "holding the collection guard");
        Ok(guard)
    }

    /// The read guard of the collection `name` maps to, or `None` when no
    /// collection has that name. A writer takes it before resolving the
    /// definition it writes with, so a patch or an index committed under
    /// the write guard is the definition it sees.
    pub(crate) async fn collection_read_guard_by_name(
        &self,
        name: &str,
    ) -> Result<Option<RwLockReadGuardArc<()>>> {
        let collection_id = match self
            .collections
            .peek(|cache| cache.get(name).map(|c| c.collection_id().to_string()))
        {
            Some(collection_id) => collection_id,
            None => return Ok(None),
        };
        Ok(Some(self.collection_read_guard(&collection_id).await?))
    }

    /// Hold the collection's read guard for the rest of the transaction.
    ///
    /// The transaction resolved `collection` from its own snapshot, possibly
    /// before the guard was free; its definition key is read again here so
    /// a patch or an index committed since fails this commit rather than
    /// letting it write under a definition that is gone.
    pub(crate) async fn acquire_collection_read_lock(
        &self,
        shared_txn: &Arc<Mutex<Option<DbTxn<S>>>>,
        collection: &Collection,
    ) -> Result<()> {
        let collection_id = collection.collection_id();
        let mut txn = shared_txn.lock().await;
        let txn = txn.as_mut().ok_or(Error::TxnNotActive)?;
        if txn.has_collection_guard(collection_id) {
            return Ok(());
        }

        let guard = self.collection_lock(collection_id)?.read_arc().await;
        txn.insert_collection_read_guard(collection_id.to_string(), guard);
        let defined = txn
            .systemstore()?
            .has(&CollectionKey::new(collection.version_id()).bytes())
            .await
            .map_err(Error::Storage)?;
        if !defined {
            return Err(Error::CollectionNotFound(collection.name().to_string()));
        }
        Ok(())
    }

    pub(crate) async fn acquire_collection_write_locks_for_txn(
        &self,
        txn: &mut DbTxn<S>,
        collection_ids: impl IntoIterator<Item = String>,
    ) -> Result<()> {
        let mut collection_ids: Vec<_> = collection_ids.into_iter().collect();
        collection_ids.sort();
        collection_ids.dedup();

        for collection_id in &collection_ids {
            if !txn.has_collection_write_guard(collection_id) {
                txn.remove_collection_guard(collection_id);
            }
        }

        for collection_id in collection_ids {
            if txn.has_collection_write_guard(&collection_id) {
                continue;
            }
            let guard = self.collection_lock(&collection_id)?.write_arc().await;
            txn.insert_collection_write_guard(collection_id, guard);
        }
        Ok(())
    }

    /// The lock a truncate, delete, or patch holds for the collections it writes.
    pub async fn collection_write_guards(
        &self,
        collection_ids: impl IntoIterator<Item = String>,
    ) -> Result<Vec<RwLockWriteGuardArc<()>>> {
        let mut collection_ids: Vec<_> = collection_ids.into_iter().collect();
        collection_ids.sort();
        collection_ids.dedup();

        let mut guards = Vec::with_capacity(collection_ids.len());
        for collection_id in collection_ids {
            let lock = self.collection_lock(&collection_id)?;
            tracing::trace!(%collection_id, "waiting for the collection write guard");
            guards.push(lock.write_arc().await);
            tracing::trace!(%collection_id, "holding the collection write guard");
        }
        Ok(guards)
    }
}
