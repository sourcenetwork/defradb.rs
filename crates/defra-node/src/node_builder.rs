//! [`NodeBuilder`]: opens the store, then hands the node to the one assembly.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "p2p")]
use crate::assemble::check_access_hooks;
use crate::assemble::{assemble, Extras};
use crate::config::{NodeConfig, NodeSigning, Persistence, SigningKey, TransportConfig};
use crate::Node;
#[cfg(feature = "otel")]
use crate::TelemetryHandle;
#[cfg(feature = "p2p")]
use crate::{AccessHooks, P2PConfig};
#[cfg(feature = "vera")]
use crate::{DocumentAcpConfig, VeraConfig};
use crate::{Error, QueryLimits, Result};
#[cfg(feature = "http")]
use crate::{HttpConfig, TransactionCleanupConfig};

/// Selects which persistent storage backend to use when `data_path` is set.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub enum StorageBackend {
    /// regolith, the one store. Kept as an enum so `with_storage_backend`
    /// stays on the builder and a later variant does not churn it.
    #[default]
    Regolith,
}

/// The sizing profile regolith opens a store with, when no explicit options
/// are given.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum StoreProfile {
    /// `RegolithStoreOptions::default()`, sized for a server.
    #[default]
    Server,
    /// `RegolithStoreOptions::embedded()`, sized for a 1-4 MiB working set, as a
    /// phone or other embedded host has to live inside.
    Embedded,
}

/// Builder for constructing an `Node`. Its setters fill in a
/// [`NodeConfig`], which is the one place the node's settings live.
#[derive(Default)]
pub struct NodeBuilder {
    config: NodeConfig,
    workers: Option<workers::Config>,
    data_path: Option<PathBuf>,
    storage_backend: StorageBackend,
    storage_profile: StoreProfile,
    storage_durability: storage::backends::DurabilityMode,
    regolith_options: Option<storage::RegolithStoreOptions>,
    embedding_url: Option<String>,
    embedding_model: Option<String>,
    embedding_api_key: Option<String>,
    at_rest_encryption_key: Option<[u8; 32]>,
    previous_at_rest_encryption_keys: Vec<[u8; 32]>,
    #[cfg(feature = "http")]
    http_config: Option<HttpConfig>,
    #[cfg(feature = "p2p")]
    access_hooks: Option<AccessHooks>,
    #[cfg(feature = "otel")]
    telemetry_handle: Option<TelemetryHandle>,
    crdts: Vec<crdt::Registration>,
}

/// Durability is a separate builder setting, so it is applied over the
/// caller's options rather than being one of the values they replace.
fn resolve_regolith_options(
    explicit: Option<storage::RegolithStoreOptions>,
    profile: StoreProfile,
    durability: storage::backends::DurabilityMode,
) -> storage::RegolithStoreOptions {
    explicit
        .unwrap_or_else(|| match profile {
            StoreProfile::Server => storage::RegolithStoreOptions::default(),
            StoreProfile::Embedded => storage::RegolithStoreOptions::embedded(),
        })
        .with_durability(durability)
}

/// The builder's at-rest keys, current and previous, installed on `options`.
fn with_at_rest_keys(
    options: storage::RegolithStoreOptions,
    current: Option<[u8; 32]>,
    previous: &[[u8; 32]],
) -> Result<storage::RegolithStoreOptions> {
    match current {
        Some(key) => options
            .with_encryption_keys(key, previous.iter().copied())
            .map_err(Error::InvalidAtRestKeys),
        None => Ok(options),
    }
}

impl NodeBuilder {
    /// The worker pool the node's data path runs on. Unset, the pool's own
    /// defaults (`workers::Config::default()`).
    pub fn with_workers(mut self, config: workers::Config) -> Self {
        self.workers = Some(config);
        self
    }

    /// Run the node's network work (signed queries, the HTTP server, P2P)
    /// on the application's network runtime. Without one, the node starts
    /// its own when it has network work to run, and stops it at shutdown.
    pub fn with_edge(mut self, edge: edge::Edge) -> Self {
        self.config.edge = Some(edge);
        self
    }

    /// Set the data directory for persistent storage.
    /// If not set, uses in-memory storage.
    pub fn data_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.data_path = Some(path.into());
        self
    }

    /// Select the persistent storage backend (default: `Regolith`, the only
    /// one).
    ///
    /// Has no effect when `data_path` is not set (in-memory mode).
    pub fn with_storage_backend(mut self, backend: StorageBackend) -> Self {
        self.storage_backend = backend;
        self
    }

    /// Set the persistent storage durability mode (default: `Immediate`).
    ///
    /// `Immediate` fsyncs acknowledged commits. `Eventual` can improve write
    /// throughput, but OS crashes may lose acknowledged writes.
    pub fn with_storage_durability(
        mut self,
        durability: storage::backends::DurabilityMode,
    ) -> Self {
        self.storage_durability = durability;
        self
    }

    /// Set the regolith store options, including its memory budget.
    ///
    /// Defaults to [`storage::RegolithStoreOptions::default`], whose sizing
    /// targets a server. An embedded host that opens the node in-process
    /// pays that budget out of its own address space, so it picks a profile
    /// here rather than inheriting one. Durability stays with
    /// [`NodeBuilder::with_storage_durability`] and is applied over whatever
    /// is set here.
    pub fn with_regolith_options(mut self, options: storage::RegolithStoreOptions) -> Self {
        self.regolith_options = Some(options);
        self
    }

    /// Enable engine file encryption at rest for the persistent storage backend.
    pub fn with_at_rest_encryption_key(mut self, key: [u8; 32]) -> Self {
        self.at_rest_encryption_key = Some(key);
        self
    }

    /// Keys the store's files may still be sealed under after a rotation to
    /// the key given to [`NodeBuilder::with_at_rest_encryption_key`]. They
    /// only open what they sealed; new files use the current key.
    pub fn with_previous_at_rest_encryption_keys<I>(mut self, keys: I) -> Self
    where
        I: IntoIterator<Item = [u8; 32]>,
    {
        self.previous_at_rest_encryption_keys = keys.into_iter().collect();
        self
    }

    /// Set the maximum number of auto-commit transaction conflict retries.
    pub fn with_max_txn_retries(mut self, retries: u32) -> Self {
        self.config.max_txn_retries = Some(retries);
        self
    }

    /// Set the fallback OpenAI-compatible embedding base URL.
    pub fn with_embedding_url(mut self, url: impl Into<String>) -> Self {
        self.embedding_url = Some(url.into());
        self
    }

    /// Set the fallback embedding model name used when the schema leaves it empty.
    pub fn with_embedding_model(mut self, model: impl Into<String>) -> Self {
        self.embedding_model = Some(model.into());
        self
    }

    /// Set the resolved embedding API key value used for Authorization headers.
    pub fn with_embedding_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.embedding_api_key = Some(api_key.into());
        self
    }

    /// Configure the node to use Vera-backed document ACP.
    ///
    /// Requires the `vera` feature (on by default).
    #[cfg(feature = "vera")]
    pub fn with_vera(mut self, config: VeraConfig) -> Self {
        self.config.document_acp = DocumentAcpConfig::Vera(config);
        self
    }

    /// Configure Vera ACP when LCD and gRPC use distinct endpoints.
    #[cfg(feature = "vera")]
    pub fn with_vera_lcd(mut self, config: VeraConfig, lcd_address: impl Into<String>) -> Self {
        self.config.document_acp = DocumentAcpConfig::VeraWithLcd {
            config,
            lcd_address: lcd_address.into(),
        };
        self
    }

    /// Use an identity already registered in DefraDB's process-local signing registry.
    ///
    /// The caller must register the signer before calling [`NodeBuilder::build`].
    /// Registered identities may be backed by exportable private key bytes or by a
    /// remote signer such as a host Secure Enclave adapter.
    pub fn with_node_identity_did(mut self, did: impl Into<String>) -> Self {
        self.config.signing = NodeSigning::RegisteredIdentity { did: did.into() };
        self
    }

    /// Enable Node Access Control (NAC) with the configured node identity as owner.
    ///
    /// NAC is disabled by default. When enabled, DB-layer node operations are
    /// gated by the NAC policy; the node's own identity always retains full
    /// access (it is the owner). Requires [`NodeBuilder::with_node_identity_did`]
    /// to be set with an identity backed by local key bytes, so the node DID can
    /// own the policy — [`NodeBuilder::build`] fails otherwise.
    pub fn with_node_acp_enabled(mut self) -> Self {
        self.config.node_acp_enabled = true;
        self
    }

    /// Set the query execution timeout.
    ///
    /// `Duration::ZERO` disables query execution timeouts.
    pub fn with_query_timeout(mut self, timeout: Duration) -> Self {
        self.config.query_timeout = Some(timeout);
        self
    }

    /// Set GraphQL parsing and filter evaluation limits.
    pub fn with_query_limits(mut self, limits: QueryLimits) -> Self {
        self.config.query_limits = limits;
        self
    }

    /// Enable or disable the introspection schema cache. Enabled by default.
    pub fn with_introspection_cache(mut self, enabled: bool) -> Self {
        self.config.introspection_cache = Some(enabled);
        self
    }

    /// Enable the HTTP GraphQL server.
    #[cfg(feature = "http")]
    pub fn with_http(mut self, config: HttpConfig) -> Self {
        self.http_config = Some(config);
        self
    }

    /// Enable Iroh/QUIC P2P replication.
    ///
    /// Requires the `p2p` feature, which implies `native`.
    #[cfg(feature = "p2p")]
    pub fn with_p2p(mut self, config: P2PConfig) -> Self {
        self.config.transport = TransportConfig::Iroh(config);
        self
    }

    /// Hand a pre-built telemetry handle to the node so it owns shutdown.
    ///
    /// The caller is responsible for calling [`telemetry::init`] and
    /// composing the bridge layer onto their `tracing` subscriber — this
    /// method just transfers ownership of the lifecycle handle so the node
    /// flushes providers when it shuts down. Because the handle is moved,
    /// calling this twice is a compile error rather than a silent overwrite.
    ///
    /// For a one-call ergonomic version, callers can write
    /// `.with_telemetry(telemetry::init(cfg)?.0)` — `init` requires a Tokio
    /// runtime and returns `(TelemetryHandle, SdkTracer)`.
    #[cfg(feature = "otel")]
    pub fn with_telemetry(mut self, handle: TelemetryHandle) -> Self {
        self.telemetry_handle = Some(handle);
        self
    }

    /// A custom CRDT type, which a field takes with `@crdt(custom: "<name>")`;
    /// `crdt.ctype()` is `CType::Custom` of that name. The type implements
    /// [`crdt::FieldCrdt`] and passes `crdt::field::testing::check_field_crdt`.
    ///
    /// The type belongs to this node alone: another node in the process has
    /// only the types its own builder registers. A node without the type
    /// keeps and replicates definitions naming it, leaves the field out of
    /// its queries, and parks the field's replicated blocks; a later build
    /// with the type merges them. Registering two types under one name fails
    /// [`build`](Self::build).
    pub fn with_crdt<T>(mut self, crdt: T) -> Self
    where
        T: crdt::FieldCrdt + 'static,
    {
        self.crdts.push(crdt::Registration::new(crdt));
        self
    }

    /// Start from `config`, replacing every setting a setter filled in before.
    pub fn with_config(mut self, config: NodeConfig) -> Self {
        self.config = config;
        self
    }

    /// The settings the setters have filled in so far.
    pub fn config(&self) -> &NodeConfig {
        &self.config
    }

    /// Size the store for an embedded host ([`StoreProfile::Embedded`]) rather
    /// than a server. Explicit [`NodeBuilder::with_regolith_options`] win.
    pub fn with_store_profile(mut self, profile: StoreProfile) -> Self {
        self.storage_profile = profile;
        self
    }

    /// Run the transport `transport` names.
    pub fn with_transport(mut self, transport: TransportConfig) -> Self {
        self.config.transport = transport;
        self
    }

    /// Run libp2p replication, listening on `listen_addr`.
    #[cfg(feature = "libp2p")]
    pub fn with_libp2p(mut self, listen_addr: impl Into<String>) -> Self {
        self.config.transport = TransportConfig::Libp2p(crate::Libp2pConfig {
            listen_addrs: vec![listen_addr.into()],
            ..Default::default()
        });
        self
    }

    /// Keep the transport identity in an application-owned keyring.
    ///
    /// The backend must provide durable storage and its own namespace per node.
    /// Startup rejects existing plaintext peer keys; this does not erase or
    /// migrate historical key material from a database or its backups.
    pub fn with_peer_keyring(mut self, keyring: Arc<dyn keyring::Keyring>) -> Self {
        self.config.peer_keyring = Some(crate::PeerKeyring::new(keyring));
        self
    }

    /// Sign as a node identity the node creates from `key`.
    pub fn with_signing_key(mut self, key: SigningKey) -> Self {
        self.config.signing = NodeSigning::Enabled { key: Some(key) };
        self
    }

    /// Sign as a node identity the node generates.
    pub fn enable_signing(mut self) -> Self {
        self.config.signing = NodeSigning::Enabled { key: None };
        self
    }

    /// Replicas' bytes per block chunk; the database default when unset.
    pub fn with_chunk_size(mut self, bytes: usize) -> Self {
        self.config.chunk_size = Some(bytes);
        self
    }

    /// Install the app's own access control. Only an iroh node accepts it:
    /// `build` fails for any other transport.
    #[cfg(feature = "p2p")]
    pub fn with_access_hooks(mut self, hooks: AccessHooks) -> Self {
        self.access_hooks = Some(hooks);
        self
    }

    /// Build and start the embedded DefraDB node.
    pub async fn build(mut self) -> Result<Node> {
        let crdts = crdt::Crdts::new(std::mem::take(&mut self.crdts))?;
        #[cfg(feature = "p2p")]
        check_access_hooks(&self.config, self.access_hooks.as_ref())?;

        #[cfg(feature = "http")]
        let (serves_http, transaction_cleanup) = match self.http_config.as_ref() {
            Some(config) if config.transaction_idle_timeout.is_zero() => (true, None),
            Some(config) => {
                if config.transaction_cleanup_interval.is_zero() {
                    return Err(Error::ZeroCleanupInterval);
                }
                (
                    true,
                    Some(TransactionCleanupConfig {
                        max_idle_age: config.transaction_idle_timeout,
                        sweep_interval: config.transaction_cleanup_interval,
                    }),
                )
            }
            None => (false, None),
        };

        let extras = Extras {
            embedding_url: self.embedding_url.take(),
            embedding_model: self.embedding_model.take(),
            embedding_api_key: self.embedding_api_key.take(),
            #[cfg(feature = "http")]
            transaction_cleanup,
            #[cfg(feature = "http")]
            serves_http,
            #[cfg(feature = "p2p")]
            access_hooks: self.access_hooks.take(),
            #[cfg(feature = "otel")]
            telemetry: self.telemetry_handle.take(),
        };

        self.config.persistence = if self.data_path.is_some() {
            Persistence::Persistent
        } else {
            Persistence::Memory
        };
        let config = self.config.clone();
        let query_limits = config.query_limits;
        let workers = self.workers.take();
        let node = assemble(|pool| self.open_store(pool, crdts), config, workers, extras).await?;

        // Spawn the HTTP server if configured.
        #[cfg(feature = "http")]
        if let Some(http_cfg) = self.http_config.take() {
            let network = node.network.as_ref().ok_or(Error::HttpNeedsNetwork)?;
            let server_config = defra_http::ServerConfig {
                address: http_cfg.address,
                request_timeout: crate::timeout_secs(http_cfg.request_timeout),
                query_limits,
                ..Default::default()
            };
            let server = defra_http::Server::from_arc_with_config(
                node.runner.clone(),
                server_config,
                network.edge().clone(),
            )
            .with_event_bus_arc(node.event_bus.clone())
            .with_pool(node.pool.clone())
            .with_dev_mode(http_cfg.dev_mode)
            .with_schema_arc(Arc::new(crate::HttpSchemaOps {
                schema_ops: Arc::clone(&node.schema_ops),
                context: None,
            }));

            // The node's own DID, however it was configured: a registered
            // identity, or one signing generated or was given a key for.
            let server = if let Some(did) = node.node_identity_did() {
                server.with_node_identity_did(did.to_owned())
            } else {
                server
            };

            #[cfg(any(feature = "p2p", feature = "libp2p"))]
            let server = if let Some(p2p) = node.p2p_arc() {
                server.with_p2p_arc(p2p)
            } else {
                server
            };

            let server = match http_cfg.extra_routes {
                Some(extra) => server.with_extra_routes(extra),
                None => server,
            };
            let server = match http_cfg.router_transform {
                Some(transform) => server.with_router_transform(transform),
                None => server,
            };
            let addr = http_cfg.address;
            let bound = http_cfg.listener;
            // The accept loop is a service of the node's scope: shutting the
            // node down stops it accepting.
            let service = network.edge().spawn_service(async move {
                let served = match bound {
                    Some(listener) => server.serve_on(listener).await,
                    None => server.run().await,
                };
                if let Err(e) = served {
                    tracing::error!(error = %e, address = %addr, "HTTP server exited with error");
                }
            });
            service.detach();
            tracing::info!(address = %addr, "HTTP server started");
        }
        #[cfg(not(feature = "http"))]
        let _ = query_limits;

        Ok(node)
    }

    /// Opens the node's regolith store on `pool`, in memory or at `data_path`.
    fn open_store(
        &self,
        pool: &workers::Pool,
        crdts: crdt::Crdts,
    ) -> Result<Arc<storage::RegolithStore>> {
        let store = if let Some(path) = self.data_path.as_ref() {
            std::fs::create_dir_all(path)?;
            tracing::info!(
                storage_backend = "regolith",
                data_path = %path.display(),
                "node starting"
            );
            let opts = with_at_rest_keys(
                resolve_regolith_options(
                    self.regolith_options.clone(),
                    self.storage_profile,
                    self.storage_durability,
                )
                .placed_on(pool)
                .with_crdts(crdts),
                self.at_rest_encryption_key,
                &self.previous_at_rest_encryption_keys,
            )?;
            storage::RegolithStore::open_with_options(path, opts).map_err(Error::StoreOpen)?
        } else {
            tracing::info!(
                storage_backend = "memory",
                "node starting (ephemeral, no data_path)"
            );
            storage::RegolithStore::open_with_options(
                "memory",
                with_at_rest_keys(
                    storage::RegolithStoreOptions::memory()
                        .placed_on(pool)
                        .with_crdts(crdts),
                    self.at_rest_encryption_key,
                    &self.previous_at_rest_encryption_keys,
                )?,
            )
            .map_err(Error::MemoryStoreOpen)?
        };
        Ok(Arc::new(store))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn node_builder_accepts_regolith_options() {
        let builder =
            crate::Node::builder().with_regolith_options(storage::RegolithStoreOptions::embedded());

        assert_eq!(
            builder
                .regolith_options
                .as_ref()
                .map(|options| options.engine.get_write_buffer_size()),
            Some(256 * 1024)
        );
    }

    #[test]
    fn the_embedded_profile_sizes_the_store_for_an_embedded_host() {
        let resolved = super::resolve_regolith_options(
            None,
            super::StoreProfile::Embedded,
            storage::backends::DurabilityMode::Immediate,
        );

        assert_eq!(resolved.engine.get_write_buffer_size(), 256 * 1024);
        assert_eq!(resolved.engine.get_block_cache_size(), 0);
    }

    #[test]
    fn regolith_options_default_to_server_sizing() {
        let resolved = super::resolve_regolith_options(
            None,
            super::StoreProfile::Server,
            storage::backends::DurabilityMode::Immediate,
        );
        let server = storage::RegolithStoreOptions::default();

        assert_eq!(
            resolved.engine.get_write_buffer_size(),
            server.engine.get_write_buffer_size()
        );
        assert_eq!(
            resolved.engine.get_block_cache_size(),
            server.engine.get_block_cache_size()
        );
    }

    #[test]
    fn explicit_regolith_options_survive_and_durability_stays_with_the_builder() {
        let resolved = super::resolve_regolith_options(
            Some(storage::RegolithStoreOptions::embedded()),
            super::StoreProfile::Server,
            storage::backends::DurabilityMode::Eventual,
        );
        let expected = storage::RegolithStoreOptions::embedded()
            .with_durability(storage::backends::DurabilityMode::Eventual);

        assert_eq!(resolved.engine.get_write_buffer_size(), 256 * 1024);
        assert_eq!(resolved.engine.get_block_cache_size(), 0);
        assert_eq!(format!("{resolved:?}"), format!("{expected:?}"));
    }

    #[test]
    fn node_builder_accepts_query_timeout() {
        let timeout = std::time::Duration::from_secs(45);
        let builder = crate::Node::builder().with_query_timeout(timeout);

        assert_eq!(builder.config.query_timeout, Some(timeout));
    }
}
