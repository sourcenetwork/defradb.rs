//! Opening options for the regolith backend.

use std::sync::Arc;
use std::time::Duration;

use regolith::{DurabilityMode as EngineDurability, IsolationLevel, Options};

use super::encryption::AtRestKeys;
use crate::backends::shared::DurabilityMode;
use crate::corekv::Result;

/// How a [`super::RegolithStore`] opens and commits.
#[derive(Clone)]
pub struct RegolithStoreOptions {
    /// Engine options. Built from a regolith profile, so a target that
    /// needs a different memory budget picks it up by construction rather
    /// than by a caller remembering to tune it. The at-rest keys are not in
    /// here: the open installs them ([`Self::engine_with_keys`]).
    pub engine: Options,
    /// Commit-time validation. DefraLevel uses the installed key classifier
    /// to allow identical content-addressed block writes. Other key families
    /// retain ordinary validation until their write protocols are adopted.
    pub isolation: IsolationLevel,
    /// How long `close` waits for in-flight transactions to finish.
    pub close_timeout: Duration,
    /// The at-rest keys. Kept apart from [`Self::engine`] because a store the
    /// previous release encrypted value by value is first looked at without
    /// them, and decrypted once with them (`super::rewrite`).
    at_rest: Option<Arc<AtRestKeys>>,
    /// The worker pool the store was placed on ([`Self::placed_on`]): callers
    /// off its workers do their I/O through one of its queues.
    pub(super) pool: Option<workers::Pool>,
    /// The environment the store's files live in, beneath the pool's
    /// placement wrapper ([`Self::files_env`]).
    files_env: Option<Arc<dyn regolith::Env>>,
}

impl RegolithStoreOptions {
    /// Seal engine files with a key resolved before opening the store.
    ///
    /// Values remain plaintext inside the engine, so equality checks,
    /// allocations, appends and merge operands retain their meaning. A store
    /// the previous release encrypted value by value with this key is
    /// rewritten once, at its first open, into one the engine seals.
    pub fn with_encryption_key(self, key: [u8; 32]) -> Self {
        self.with_keys(AtRestKeys::single(key))
    }

    /// Seal new files with `current` while files sealed under any of `older`
    /// stay readable: a key rotation. Compaction re-seals files under the
    /// current key as it rewrites them, so keep an older key for as long as
    /// a file sealed under it may remain. Two different keys that share a
    /// derived key id are refused.
    pub fn with_encryption_keys<I>(self, current: [u8; 32], older: I) -> Result<Self>
    where
        I: IntoIterator<Item = [u8; 32]>,
    {
        Ok(self.with_keys(AtRestKeys::rotated(current, older)?))
    }

    fn with_keys(mut self, keys: AtRestKeys) -> Self {
        self.at_rest = Some(Arc::new(keys));
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn at_rest_keys(&self) -> Option<&AtRestKeys> {
        self.at_rest.as_deref()
    }

    /// [`Self::engine`] with the at-rest keys installed: what every open of
    /// the store uses.
    /// The environment the store's files live in: the engine's, or, once
    /// the store is placed on a pool, the one the pool's wrapper places.
    pub(super) fn files_env(&self) -> Arc<dyn regolith::Env> {
        self.files_env
            .clone()
            .unwrap_or_else(|| self.engine.get_env().clone())
    }

    pub(super) fn engine_with_keys(&self) -> Options {
        match &self.at_rest {
            Some(keys) => self.engine.clone().key_provider(keys.clone()),
            None => self.engine.clone(),
        }
    }

    /// [`Self::engine_with_keys`] with what reading the store's data takes:
    /// the merge operator its merged values fold with, and the prefix
    /// extractor its tables' prefix filters are built and read with. The
    /// writable and the read-only open both take their options from here, so
    /// a read-only open reads a merged value, and seeks a prefix, as a
    /// writable one does.
    pub(super) fn engine_for_open(&self) -> Options {
        self.engine_with_keys()
            .merge_operator(Some(Arc::new(super::merge::DefraMergeOperator)))
            .prefix_extractor(Some(Arc::new(super::prefix::DefraPrefixExtractor)))
    }

    /// Server defaults: DefraLevel, fsync on every commit.
    pub fn new() -> Self {
        Self {
            engine: Options::default().durability(EngineDurability::Immediate),
            isolation: IsolationLevel::DefraLevel,
            close_timeout: Duration::from_secs(30),
            at_rest: None,
            pool: None,
            files_env: None,
        }
    }

    /// A 1-4 MiB working set, for a phone or an edge device.
    pub fn embedded() -> Self {
        Self {
            engine: Options::embedded().durability(EngineDurability::Immediate),
            ..Self::new()
        }
    }

    /// A browser or wasi module. The caller supplies the filesystem: on
    /// `wasm32-unknown-unknown` there is none, so mount OPFS and set
    /// [`RegolithStoreOptions::engine`]'s `env` before opening.
    pub fn wasm() -> Self {
        Self {
            engine: Options::wasm().durability(EngineDurability::Immediate),
            ..Self::new()
        }
    }

    /// Keep the database in memory. For tests and for a node that is
    /// explicitly ephemeral.
    pub fn memory() -> Self {
        // Take the environment constraints from the engine's own preset,
        // which is where the knowledge lives: `MemEnv` starts no threads, so
        // a compaction worker cannot exist, and nothing is being made durable,
        // so an fsync per commit would buy nothing.
        //
        // The size limits stay the server ones. `Options::memory()` is sized
        // from the embedded profile, whose 256 KiB value ceiling is a limit a
        // caller can hit rather than a memory-tuning knob: a document larger
        // than that is ordinary here, and an in-memory store that refuses what
        // the on-disk one accepts is a different database, not a faster one.
        //
        // With no worker, a write that a stall stops has to run the
        // compaction itself, or nothing ever clears the stall.
        let preset = Options::memory();
        let mut opts = Self::new();
        opts.engine = opts
            .engine
            .env(preset.get_env().clone())
            .max_background_compactions(preset.get_max_background_compactions())
            .inline_compaction(true)
            .durability(preset.get_durability());
        opts
    }

    /// Start the threads regolith starts (compaction, log writeback) where
    /// `pool` places its background threads, at their priority. Pair it with
    /// a pool started with [`with_worker_io`](super::with_worker_io).
    pub fn placed_on(mut self, pool: &workers::Pool) -> Self {
        let files = self.files_env();
        self.files_env = Some(Arc::clone(&files));
        let env = pool.env(files);
        self.engine = self.engine.env(env);
        self.pool = Some(pool.clone());
        self
    }

    /// Override commit validation. The Defra key policy only applies at
    /// `DefraLevel`; other levels retain Regolith's native semantics.
    pub fn with_isolation(mut self, isolation: IsolationLevel) -> Self {
        self.isolation = isolation;
        self
    }

    /// Trade durability for throughput: `Eventual` survives a process
    /// crash but not a power cut.
    pub fn with_durability(mut self, durability: DurabilityMode) -> Self {
        self.engine = self.engine.durability(match durability {
            DurabilityMode::Immediate => EngineDurability::Immediate,
            DurabilityMode::Eventual => EngineDurability::Eventual,
        });
        self
    }
}

impl Default for RegolithStoreOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for RegolithStoreOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegolithStoreOptions")
            .field("isolation", &self.isolation)
            .field("durability", &self.engine.get_durability())
            .field("close_timeout", &self.close_timeout)
            .field("pool", &self.pool.is_some())
            .finish_non_exhaustive()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use crate::backends::RegolithTxn;
    use crate::corekv::{Reader, Store};
    use crate::merge::counter_i64;
    use crate::{RegolithStore, RegolithStoreOptions};

    /// A read-only open folds merged values as the writable open does. It
    /// opened without the merge operator before, and the read failed.
    #[test]
    fn a_read_only_open_reads_merged_values() {
        workers::block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let options = RegolithStoreOptions::default();
            let store = RegolithStore::open_with_options(dir.path(), options.clone()).unwrap();
            for delta in [2, 3] {
                let mut txn = store.new_txn(false).await.unwrap();
                txn.merge(b"counter", &counter_i64(delta)).await.unwrap();
                txn.commit().await.unwrap();
            }
            store.close().await.unwrap();
            drop(store);
            let txn = RegolithTxn::open_read_only(dir.path(), options).unwrap();
            assert_eq!(
                txn.get(b"counter").await.unwrap().as_deref(),
                Some(5i64.to_be_bytes().as_slice())
            );
        })
        .unwrap()
    }
}
