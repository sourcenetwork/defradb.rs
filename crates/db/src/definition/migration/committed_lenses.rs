//! The database's lens store: the lenses it has durably committed.
//!
//! A lens config commits under its `LensConfigKey` in the transaction that
//! registers it, with its IPLD blocks when it is added as a lens of its own
//! ([`PreparedLens`]). Registering the lens in the in-memory transform store is the
//! follow-up: it runs once the commit is durable, and at open for every
//! persisted config. It can fail (a module path that cannot be read now, a
//! compile that runs out of resources), and the commit stands regardless: the
//! failure is reported once and the lens is owed. An owed lens is known
//! (`has_transform`, `list`), so nothing treats a committed migration as
//! absent; each use of it registers it first, and fails loud with the reason
//! while registration still fails. A crash loses nothing: the next open
//! registers every persisted config again.

use std::sync::Arc;

use async_trait::async_trait;
use datastore::NamespaceView;
use futures::TryStreamExt;
use kovan_map::HopscotchMap;
use lens::{LensConfig, LensDoc, LensDocResultStream, LensDocStream, TransformId, TransformStore};
use rapidhash::fast::RandomState;
use rapidhash::RapidHashMap;
use storage::corekv::{IterOptions, Key, Store};
use storage::keys::systemstore::LensConfigKey;

use crate::error::{Error, Result};
use crate::DB;

/// Committed lenses whose registration is owed, with their configs.
type Owed = HopscotchMap<TransformId, Arc<LensConfig>, RandomState>;

/// The database's transform store, holding the registrations its commits owe.
pub(crate) struct CommittedLensStore {
    base: Arc<dyn TransformStore>,
    owed: Arc<Owed>,
}

impl CommittedLensStore {
    pub(crate) fn wrap(base: Arc<dyn TransformStore>) -> Arc<dyn TransformStore> {
        Arc::new(Self {
            base,
            owed: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
        })
    }

    fn run(
        &self,
        id: &TransformId,
        docs: LensDocStream,
        inverse: bool,
    ) -> lens::Result<LensDocResultStream> {
        match self.owed.get(id) {
            Some(config) => Ok(self.run_after_registration(id.clone(), config, docs, inverse)),
            None if inverse => self.base.inverse(id, docs),
            None => self.base.transform(id, docs),
        }
    }

    /// Registers the owed lens `id`, then runs it over `docs`.
    fn run_after_registration(
        &self,
        id: TransformId,
        config: Arc<LensConfig>,
        docs: LensDocStream,
        inverse: bool,
    ) -> LensDocResultStream {
        let base = Arc::clone(&self.base);
        let owed = Arc::clone(&self.owed);
        Box::pin(
            futures::stream::once(async move {
                register_owed(base.as_ref(), &owed, &id, &config).await?;
                if inverse {
                    base.inverse(&id, docs)
                } else {
                    base.transform(&id, docs)
                }
            })
            .try_flatten(),
        )
    }
}

/// Registers the owed lens `id` before a use of it.
async fn register_owed(
    base: &dyn TransformStore,
    owed: &Owed,
    id: &TransformId,
    config: &LensConfig,
) -> lens::Result<()> {
    register(base, owed, id, config)
        .await
        .inspect_err(|error| {
            tracing::debug!(
                transform_id = %id,
                %error,
                "An owed lens registration failed again"
            );
        })?;
    tracing::info!(transform_id = %id, "Registered an owed committed lens");
    Ok(())
}

/// Registers `id` in `base`. A registered lens is owed no longer.
async fn register(
    base: &dyn TransformStore,
    owed: &Owed,
    id: &TransformId,
    config: &LensConfig,
) -> lens::Result<()> {
    base.add_with_id(id.clone(), config.clone()).await?;
    owed.remove(id);
    Ok(())
}

#[async_trait]
impl TransformStore for CommittedLensStore {
    fn new_isolated_store(&self) -> Arc<dyn TransformStore> {
        self.base.new_isolated_store()
    }

    async fn add(&self, config: LensConfig) -> lens::Result<TransformId> {
        self.base.add(config).await
    }

    /// Registers a committed lens. One that cannot be registered now is owed,
    /// and the error is returned for the caller to report.
    async fn add_with_id(&self, id: TransformId, config: LensConfig) -> lens::Result<()> {
        let config = Arc::new(config);
        register(self.base.as_ref(), &self.owed, &id, &config)
            .await
            .inspect_err(|_| {
                self.owed.insert(id.clone(), Arc::clone(&config));
            })
    }

    async fn list(&self) -> lens::Result<RapidHashMap<String, Vec<lens::LensModule>>> {
        let mut modules = self.base.list().await?;
        for (id, config) in self.owed.iter() {
            modules
                .entry(id.to_string())
                .or_insert_with(|| config.lenses.clone());
        }
        Ok(modules)
    }

    fn transform(
        &self,
        id: &TransformId,
        docs: LensDocStream,
    ) -> lens::Result<LensDocResultStream> {
        self.run(id, docs, false)
    }

    fn inverse(&self, id: &TransformId, docs: LensDocStream) -> lens::Result<LensDocResultStream> {
        self.run(id, docs, true)
    }

    async fn transform_doc(
        &self,
        id: &TransformId,
        doc: LensDoc,
        inverse: bool,
    ) -> lens::Result<LensDoc> {
        if let Some(config) = self.owed.get(id) {
            register_owed(self.base.as_ref(), &self.owed, id, &config).await?;
        }
        self.base.transform_doc(id, doc, inverse).await
    }

    fn has_transform(&self, id: &TransformId) -> bool {
        self.base.has_transform(id) || self.owed.contains_key(id)
    }

    async fn remove(&self, id: &TransformId) -> lens::Result<()> {
        let owed = self.owed.remove(id).is_some();
        match self.base.remove(id).await {
            Err(lens::Error::TransformNotFound(_)) if owed => Ok(()),
            outcome => outcome,
        }
    }
}

/// A lens ready to commit. Its WASM bytes are serialized into IPLD blocks
/// (LensConfigBlock, LensModuleBlock, LensWasmBlock), as Go's lens store does,
/// and the root CID of that DAG is its transform ID, a CID peers can fetch
/// during version sync.
pub(crate) struct PreparedLens {
    pub(crate) id: TransformId,
    blocks: Vec<defra_core::CidBlock>,
    pub(crate) config: LensConfig,
}

impl PreparedLens {
    /// Builds the blocks of every module of `config`, each read from its bytes
    /// or its path, so the ID is Go's: the CID of the whole lens. A path is
    /// read here, once, on a blocking thread of `pool` (the process's default
    /// pool's without one): committing the lens later never reads it again.
    pub(crate) async fn new(config: LensConfig, pool: Option<&workers::Pool>) -> Result<Self> {
        let mut modules = Vec::with_capacity(config.lenses.len());
        for module in &config.lenses {
            modules.push((
                module_wasm_bytes(module, pool).await?,
                module_arguments(module)?,
            ));
        }
        let parts: Vec<_> = config
            .lenses
            .iter()
            .zip(&modules)
            .map(
                |(module, (wasm_bytes, arguments))| defra_core::LensModuleParts {
                    wasm_bytes,
                    inverse: module.inverse,
                    arguments,
                },
            )
            .collect();
        let (config_cid, blocks) = defra_core::build_lens_ipld_blocks(&parts)
            .map_err(|e| Error::Lens(format!("failed to build lens IPLD blocks: {}", e)))?;

        Ok(Self {
            id: TransformId::new(config_cid.to_string()),
            blocks,
            config,
        })
    }

    /// Writes the lens into a transaction, its blocks and its config, so it is
    /// durable with that transaction's commit.
    pub(crate) async fn write(
        &self,
        blockstore: &NamespaceView,
        systemstore: &NamespaceView,
    ) -> Result<()> {
        for (cid, data) in &self.blocks {
            blockstore
                .set(&cid.to_bytes(), data)
                .await
                .map_err(Error::Storage)?;
        }
        write_lens_config(systemstore, &self.id, &self.config).await
    }
}

/// A module's WASM bytes, read from its path, else its bytes: the source the
/// lens runtime (`WasmTransformStore::load_module`) loads, so the ID names the
/// code that runs.
async fn module_wasm_bytes(
    module: &lens::LensModule,
    pool: Option<&workers::Pool>,
) -> Result<Vec<u8>> {
    let Some(ref path) = module.path else {
        return module
            .module
            .clone()
            .ok_or_else(|| Error::Lens("lens module has neither path nor bytes".into()));
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        let clean_path = path.strip_prefix("file://").unwrap_or(path).to_owned();
        let unread = |e: &dyn std::fmt::Display| {
            Error::Lens(format!("failed to read WASM from {}: {}", path, e))
        };
        let pool = match pool {
            Some(pool) => pool.clone(),
            None => workers::default_pool().map_err(|e| unread(&e))?,
        };
        pool.spawn_blocking(move || std::fs::read(clean_path))
            .await
            .map_err(|e| unread(&e))?
            .await
            .map_err(|e| unread(&e))?
            .map_err(|e| unread(&e))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = pool;
        Err(Error::Lens(format!(
            "path-based lens modules not supported on wasm32 (path: {}); pass bytes instead",
            path
        )))
    }
}

/// A module's arguments as Go's lens store records them: sorted by key, each
/// value JSON-encoded as Go's `json.Marshal` writes it. Arguments are an
/// object, as Go's `map[string]any`: any other value would reach the module's
/// `set_param` while leaving no trace in the ID, so it is refused.
fn module_arguments(module: &lens::LensModule) -> Result<Vec<(String, String)>> {
    let arguments = match &module.arguments {
        None => return Ok(Vec::new()),
        Some(serde_json::Value::Object(arguments)) => arguments,
        Some(other) => {
            return Err(Error::Lens(format!(
                "lens module arguments must be a JSON object of named arguments, got {other}"
            )));
        }
    };
    let mut encoded: Vec<_> = arguments
        .iter()
        .map(|(key, value)| (key.clone(), defra_core::encode_lens_argument(value)))
        .collect();
    encoded.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(encoded)
}

/// Persists a lens config under its transform ID in the caller's transaction:
/// the record every open registers the lens from.
pub(crate) async fn write_lens_config(
    systemstore: &NamespaceView,
    id: &TransformId,
    config: &LensConfig,
) -> Result<()> {
    let data = serde_json::to_vec(config)
        .map_err(|e| Error::lens_config_json("failed to serialize lens config", e))?;
    systemstore
        .set(&LensConfigKey::new(id.to_string()).bytes(), &data)
        .await
        .map_err(Error::Storage)
}

impl<S> DB<S>
where
    S: Store,
{
    /// Add a lens, building its IPLD blocks and storing them in the blockstore
    /// for P2P sync, with its config, in one transaction.
    pub async fn add_lens(&self, config: LensConfig) -> Result<TransformId> {
        let lens = PreparedLens::new(config, self.pool()).await?;
        let txn = self.new_txn(false).await?;
        lens.write(&txn.blockstore()?, &txn.systemstore()?).await?;
        txn.commit().await?;

        tracing::info!(
            transform_id = %lens.id,
            blocks_stored = lens.blocks.len(),
            "Lens added with IPLD blocks in blockstore"
        );
        self.register_committed_lens(lens.id.clone(), lens.config)
            .await;
        Ok(lens.id)
    }

    /// Keep a lens that P2P sync fetched under its block CID `id`: its config
    /// commits as a local lens's does, then it is registered. Its blocks are
    /// already in the blockstore. Like Go's lens node, which keeps a synced
    /// lens in the store `getLensStore` reads, so a restart reloads it and a
    /// view's lens check finds it in its transaction.
    pub async fn commit_synced_lens(&self, id: TransformId, config: LensConfig) -> Result<()> {
        let txn = self.new_txn(false).await?;
        write_lens_config(&txn.systemstore()?, &id, &config).await?;
        txn.commit().await?;
        self.register_committed_lens(id, config).await;
        Ok(())
    }

    /// Reload persisted lens configs from the systemstore into the lens store.
    ///
    /// Called during database open to restore transforms that were registered
    /// before the last shutdown. Matches Go's `getLensStore().Reload()` call
    /// in `db.initialize()`.
    pub async fn reload_lens_configs(&self) -> Result<()> {
        let txn = self.new_txn(true).await?;

        {
            let systemstore = txn.systemstore()?;
            let prefix = LensConfigKey::prefix();
            let opts = IterOptions::new().with_prefix(prefix);
            let mut iter = systemstore.iterator(opts).await.map_err(Error::Storage)?;

            let prefix_str = "/lens/config/";

            while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
                let config: LensConfig = match serde_json::from_slice(&pair.value) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            key = ?String::from_utf8_lossy(&pair.key),
                            "Skipping malformed lens config during reload"
                        );
                        continue;
                    }
                };

                let key_str = String::from_utf8_lossy(&pair.key);
                let transform_id = key_str.strip_prefix(prefix_str).map(TransformId::new);

                let Some(id) = transform_id else {
                    if let Err(e) = self.lens_store.add(config).await {
                        tracing::warn!(
                            error = %e,
                            key = ?String::from_utf8_lossy(&pair.key),
                            "Failed to reload lens config"
                        );
                    }
                    continue;
                };
                self.register_committed_lens(id, config).await;
            }

            iter.close().await.map_err(Error::Storage)?;
        }

        let _ = txn.discard();
        Ok(())
    }

    /// Registers a lens whose config this database has durably committed. The
    /// commit stands whatever happens here: a registration that fails is
    /// reported and owed, and runs again when the lens is next used.
    pub(crate) async fn register_committed_lens(&self, id: TransformId, config: LensConfig) {
        if let Err(error) = self.lens_store.add_with_id(id.clone(), config).await {
            tracing::warn!(
                transform_id = %id,
                %error,
                "A committed lens could not be registered yet; it is registered when next used"
            );
        }
    }
}
