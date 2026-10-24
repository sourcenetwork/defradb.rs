//! The regolith-backed store.
//!
//! regolith validates its own transactions, so there is no conflict
//! tracker or read-set bookkeeping here: the engine decides whether a
//! transaction may land. Snapshot capture and native commit use the engine
//! directly, so independent commits can join its commit groups.

#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use regolith::OptimisticTransactionDb;

use super::config::RegolithStoreOptions;
use super::event_order::EventOrder;
use super::hooks::StoreHooks;
use super::listener::StoreListener;
use super::slots::TxnSlots;
use super::transaction::RegolithTxn;
use crate::backends::shared::TransactionStatsHandle;
use crate::corekv::{
    Dropable, Error, Result, RetryAttempt, RetryAttemptError, RetryAttemptValue, RetryOptions,
    Store, Txn,
};

/// Key-value store backed by regolith.
///
/// Cloning hands back another handle on the same database, not another
/// database: the in-flight slots are shared, so closing through one handle
/// closes it for all of them.
#[derive(Clone)]
pub struct RegolithStore {
    inner: Arc<StoreInner>,
}

struct StoreInner {
    db: Arc<OptimisticTransactionDb>,
    options: RegolithStoreOptions,
    slots: Arc<TxnSlots>,
    stats: TransactionStatsHandle,
    /// Where calls do their I/O: the calling worker's queue, a borrowed one
    /// off the pool, or blocking without a pool.
    io: super::worker_io::StoreIo,
    event_order: Arc<EventOrder>,
    path: PathBuf,
    /// The mounted OPFS environment, kept so `persist` can reach it.
    ///
    /// Where the browser refuses synchronous access handles the database is
    /// resident in linear memory, and only `OpfsEnv::persist` writes it back.
    /// Dropping the handle at mount would leave no way to ask.
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    opfs: Option<Arc<regolith::env::opfs::OpfsEnv>>,
}

impl RegolithStore {
    /// Open at `path` with the profile for this target.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::open_with_options(path, RegolithStoreOptions::default())
    }

    /// Open at `path` with explicit options.
    pub fn open_with_options<P: AsRef<Path>>(
        path: P,
        options: RegolithStoreOptions,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    Error::Backend(format!(
                        "failed to create directory '{}': {error}",
                        parent.display()
                    ))
                })?;
            }
        }
        // The store reads tickers only, so statistics of its own keep no
        // histogram: a timer reads the clock twice per iterator step, read
        // and log write, and a sample costs atomics on every read and commit.
        let engine_stats = options.engine.get_statistics().cloned().unwrap_or_else(|| {
            Arc::new(regolith::Statistics::with_level(
                regolith::StatsLevel::ExceptHistogramOrTimers,
            ))
        });
        let stats = TransactionStatsHandle::from_engine(engine_stats.clone());
        let engine = options
            .engine_for_open()
            .statistics(Some(engine_stats))
            .add_listener(Arc::new(StoreListener(stats.clone())))
            .transaction_hooks(Arc::new(StoreHooks));
        #[cfg(not(target_arch = "wasm32"))]
        let db = super::rewrite::open(&path, engine, &options)?;
        #[cfg(target_arch = "wasm32")]
        let db = OptimisticTransactionDb::open(&path, engine)
            .map_err(|error| Error::Backend(format!("failed to open regolith: {error}")))?;
        let db = db
            .with_isolation(options.isolation)
            .with_policy(Arc::new(super::classifier::DefraKeyClassifier));
        Ok(Self {
            inner: Arc::new(StoreInner {
                db: Arc::new(db),
                io: super::worker_io::StoreIo::new(options.pool.clone(), stats.clone()),
                options,
                slots: TxnSlots::new(),
                stats,
                event_order: Arc::default(),
                path,
                #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
                opfs: None,
            }),
        })
    }

    /// An in-memory database. Nothing reaches a filesystem, so this is
    /// the store for a test or a deliberately ephemeral node.
    pub fn in_memory() -> Result<Self> {
        Self::open_with_options("regolith-memory", RegolithStoreOptions::memory())
    }

    /// Open a store persisted in the browser's origin-private filesystem.
    ///
    /// `wasm32-unknown-unknown` has no filesystem of its own, so the engine
    /// takes one: this mounts OPFS on `db_name` and installs it before
    /// opening. The mount is asynchronous because acquiring the OPFS root is,
    /// which is why this cannot be a plain `open`.
    ///
    /// The mount probes for synchronous access handles and falls back to a
    /// resident mirror when the browser refuses them, which it does outside a
    /// Worker. The fallback keeps the database in linear memory, so it is
    /// bounded rather than unlimited.
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    pub async fn open_opfs(db_name: &str) -> Result<Self> {
        Self::open_opfs_with(db_name, false).await
    }

    /// [`open_opfs`](Self::open_opfs), optionally refusing the mirror.
    ///
    /// With `require_sync_handles`, a mount that cannot take synchronous
    /// access handles fails instead of falling back to the resident mirror.
    /// The probe cannot tell "not in a Worker" from "another Worker still
    /// holds these files", so without this a Worker that takes over a
    /// database from one that has not yet released it silently mounts an
    /// empty mirror beside the real data. Failing lets the caller retry.
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    pub async fn open_opfs_with(db_name: &str, require_sync_handles: bool) -> Result<Self> {
        use std::sync::Arc;

        let mut mount_options = regolith::env::opfs::OpfsOptions::default();
        if require_sync_handles {
            mount_options.force_mode = Some(regolith::env::opfs::OpfsMode::Sah);
        }
        let env = Arc::new(
            regolith::env::opfs::OpfsEnv::mount(db_name, mount_options)
                .await
                .map_err(|error| Error::Backend(format!("failed to mount OPFS: {error}")))?,
        );

        let mut options = RegolithStoreOptions::wasm();
        // OPFS reports no durable sync, and the engine refuses `Immediate`
        // against an environment that cannot honour it rather than letting a
        // caller believe a synced write survived. That refusal is right: on
        // this target durability comes from `persist`, which writes the
        // database back to OPFS, not from an fsync that does nothing.
        if !env.as_env().capabilities().durable_sync {
            options.engine = options
                .engine
                .durability(regolith::DurabilityMode::Eventual);
        }
        options.engine = options.engine.env(env.as_env());
        let store = Self::open_with_options(db_name, options)?;
        // Safe to reach in: nothing else holds this store yet.
        let mut inner = Arc::try_unwrap(store.inner)
            .map_err(|_| Error::Backend("store handle escaped during open".to_string()))?;
        inner.opfs = Some(env);
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    /// Write everything buffered in linear memory back to OPFS.
    ///
    /// A no-op when the browser granted synchronous access handles, because
    /// those write through. In the mirror fallback this is what makes the
    /// database survive the tab.
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    pub async fn persist(&self) -> Result<()> {
        let Some(env) = self.inner.opfs.as_ref() else {
            return Ok(());
        };
        self.inner
            .db
            .db()
            .flush()
            .map_err(|error| Error::Backend(format!("flush before persist failed: {error}")))?;
        env.persist()
            .await
            .map_err(|error| Error::Backend(format!("OPFS persist failed: {error}")))
    }

    /// Where the database lives.
    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    /// The engine, for tests that reach under the adapter.
    #[cfg(test)]
    pub(super) fn engine(&self) -> &OptimisticTransactionDb {
        &self.inner.db
    }

    /// The engine as the store holds it, for tests of what is keyed by it.
    #[cfg(test)]
    pub(super) fn engine_arc(&self) -> &Arc<OptimisticTransactionDb> {
        &self.inner.db
    }

    fn ensure_open(&self) -> Result<()> {
        if self.inner.slots.is_closing() {
            return Err(Error::DBClosed);
        }
        Ok(())
    }
}

impl crate::corekv::private::Sealed for RegolithStore {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Store for RegolithStore {
    async fn allocate(&self, key: &[u8], count: u64) -> Result<std::ops::Range<u64>> {
        self.ensure_open()?;
        let db = self.inner.db.db();
        // On a pool the reservation never reads the device on the thread:
        // what it waits for is recorded on a worker's queue.
        match self.inner.io.queue(&self.inner.db, "allocate").await {
            Some(queue) => super::worker_io::settle(|| db.allocate_nowait(queue, key, count)).await,
            None => db.allocate(key, count),
        }
        .map_err(Error::from)
    }

    fn transaction_stats_handle(&self) -> Option<TransactionStatsHandle> {
        Some(self.inner.stats.clone())
    }

    async fn new_txn(&self, readonly: bool) -> Result<Box<dyn Txn>> {
        let slot = self.inner.slots.enter()?;
        let mode = self.inner.io.read_mode(&self.inner.db, "transaction").await;
        let txn = RegolithTxn::new(
            &self.inner.db,
            readonly,
            self.inner.options.isolation,
            slot,
            Arc::clone(&self.inner.event_order),
            mode,
        )?;
        Ok(Box::new(txn))
    }

    async fn transact_retry(
        &self,
        options: RetryOptions,
        attempt: RetryAttempt,
    ) -> std::result::Result<RetryAttemptValue, RetryAttemptError> {
        let _slot = self.inner.slots.enter()?;
        let queue = self.inner.io.queue(&self.inner.db, "transact").await;
        super::retry::transact(
            &self.inner.db,
            &self.inner.io,
            queue,
            &self.inner.event_order,
            options,
            attempt,
        )
        .await
    }

    async fn close(&self) -> Result<()> {
        if !self.inner.slots.begin_close() {
            return Ok(());
        }
        // A transaction still open, a commit still running and its
        // post-commit work each hold a slot; the last to end opens the latch.
        self.inner
            .slots
            .quiesce(self.inner.options.close_timeout)
            .await?;
        // On OPFS the database is resident in linear memory and only `persist`
        // writes it back, so closing without it discards everything since the
        // last explicit call. A no-op on every other environment.
        #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
        self.persist().await?;
        self.inner
            .db
            .db()
            .close()
            .map_err(|error| Error::Backend(format!("failed to close regolith: {error}")))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Dropable for RegolithStore {
    async fn drop_all(&self) -> Result<()> {
        self.ensure_open()?;
        self.inner
            .db
            .db()
            .drop_all()
            .map_err(|error| Error::Backend(format!("drop_all failed: {error}")))
    }
}

#[cfg(test)]
mod statistics_tests {
    use super::*;
    use crate::corekv::{IterOptions, Iterator, Reader, Writer};
    use regolith::{Histogram, StatsLevel, Ticker};

    /// Statistics the store opens on its own count tickers and keep no
    /// histogram: the store reports only tickers, a timer reads the clock
    /// twice per iterator step, read and log write, and a sample costs
    /// atomics on every read and commit.
    #[test]
    fn the_stores_own_statistics_count_tickers_only() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let mut txn = store.new_txn(false).await.unwrap();
            txn.set(b"key", b"value").await.unwrap();
            txn.commit().await.unwrap();
            let reader = store.new_txn(true).await.unwrap();
            assert!(reader.get(b"key").await.unwrap().is_some());
            let mut iter = reader.iterator(IterOptions::new()).await.unwrap();
            while iter.next().await.unwrap().is_some() {}
            iter.close().await.unwrap();
            reader.discard();

            let stats = store.engine().db().statistics().unwrap();
            assert_eq!(stats.level(), StatsLevel::ExceptHistogramOrTimers);
            assert!(stats.get_ticker(Ticker::CommitCount) > 0);
            assert!(stats.get_ticker(Ticker::IterSeekCount) > 0);
            for histogram in [
                Histogram::DbIterSeek,
                Histogram::DbIterNext,
                Histogram::WalWriteTime,
                Histogram::BytesPerWrite,
            ] {
                assert_eq!(
                    stats.get_histogram_snapshot(histogram).count,
                    0,
                    "{histogram:?}"
                );
            }
        })
        .unwrap()
    }
}
