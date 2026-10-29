//! Adapters bridging db types into the KMS trait surface.
//!
//! Kept in `crates/db` (not `crates/kms`) so the KMS crate doesn't depend on
//! `db`. Shared by both the embedded node and the CLI node.

use async_trait::async_trait;
use bytes::Bytes;
use defra_core::thread_bounds::MaybeSend;
use std::future::Future;
use std::sync::{Arc, Weak};

use kms::{
    BlockDocIDResolver, DocCollectionInfo, DocCollectionLookup, EncBlockStore, EncryptionCid,
    NodeAcpRead,
};

/// Runs `read` on `db`'s worker pool when it has one, so a KMS request the
/// network serves reads storage on a worker, never on a network thread.
async fn on_pool<S, T, F, Fut>(db: Arc<crate::DB<S>>, read: F) -> kms::Result<T>
where
    S: storage::corekv::Store + Send + Sync + 'static,
    F: FnOnce(Arc<crate::DB<S>>) -> Fut + MaybeSend + 'static,
    Fut: Future<Output = kms::Result<T>> + 'static,
    T: MaybeSend + 'static,
{
    #[cfg(not(target_arch = "wasm32"))]
    {
        let pool = db.pool().cloned();
        workers::run_on(pool.as_ref(), workers::Target::LeastLoaded, move || {
            read(db)
        })
        .await
        .map_err(|error| kms::Error::Storage(error.report("KMS storage read").to_string()))?
    }
    // A browser's one worker is the page's thread: the read runs where it
    // is asked for.
    #[cfg(target_arch = "wasm32")]
    read(db).await
}

/// Resolves doc_id → collection via the DB headstore (Go's
/// `RetrieveCollectionFromDocID` port).
///
/// Holds a [`Weak`] back-reference to the DB: the DB owns the KMS (and the KMS
/// owns this adapter), so a strong `Arc<DB>` here would form a reference cycle
/// that keeps the DB — and its storage lock — alive forever (#976). Each call
/// upgrades the weak handle; if the DB is gone the node is shutting down, so we
/// report no collection.
pub struct DbDocCollectionLookup<S: storage::corekv::Store + Send + Sync + 'static> {
    db: Weak<crate::DB<S>>,
}

impl<S: storage::corekv::Store + Send + Sync + 'static> DbDocCollectionLookup<S> {
    pub fn new(db: Arc<crate::DB<S>>) -> Self {
        Self {
            db: Arc::downgrade(&db),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: storage::corekv::Store + Send + Sync + 'static> DocCollectionLookup
    for DbDocCollectionLookup<S>
{
    async fn collection_for_doc(&self, doc_id: &str) -> kms::Result<Option<DocCollectionInfo>> {
        let Some(db) = self.db.upgrade() else {
            // DB dropped → node shutting down → treat as "no collection".
            return Ok(None);
        };
        let doc_id = doc_id.to_owned();
        on_pool(db, move |db| async move {
            match crate::resolve_collection_from_doc_id(&db, &doc_id).await {
                Ok(Some(info)) => Ok(Some(DocCollectionInfo {
                    collection_id: info.collection_id,
                    policy_id: info.policy_id,
                    resource_name: info.resource_name,
                    is_branchable: info.is_branchable,
                })),
                Ok(None) => Ok(None),
                Err(e) => Err(kms::Error::Storage(e.to_string())),
            }
        })
        .await
    }
}

/// Resolves which documents own a block via the systemstore ownership
/// index (Go's `ResolveBlockDocIDs` port). Weak back-reference for the
/// same cycle reason as [`DbDocCollectionLookup`].
pub struct DbBlockDocIDResolver<S: storage::corekv::Store + Send + Sync + 'static> {
    db: Weak<crate::DB<S>>,
}

impl<S: storage::corekv::Store + Send + Sync + 'static> DbBlockDocIDResolver<S> {
    pub fn new(db: Arc<crate::DB<S>>) -> Self {
        Self {
            db: Arc::downgrade(&db),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: storage::corekv::Store + Send + Sync + 'static> BlockDocIDResolver
    for DbBlockDocIDResolver<S>
{
    async fn doc_ids_for_block(&self, cid: &EncryptionCid) -> kms::Result<Vec<String>> {
        let Some(db) = self.db.upgrade() else {
            return Ok(Vec::new());
        };
        let cid = cid.to_string();
        on_pool(db, move |db| async move {
            let txn = db
                .new_txn(true)
                .await
                .map_err(|e| kms::Error::Storage(e.to_string()))?;
            let systemstore = txn
                .systemstore()
                .map_err(|e| kms::Error::Storage(e.to_string()))?;
            crate::docid::map::get_doc_ids_for_block(&systemstore, &cid)
                .await
                .map_err(|e| kms::Error::Storage(e.to_string()))
        })
        .await
    }
}

/// Durable `Encryption`-block store backing the KMS `BlockstoreKeyStore`.
///
/// Mirrors Go's `internal/kms/enc_store.go`: reads the encstore→blockstore
/// (the same order as the merge decrypt path) and writes new blocks to the
/// durable blockstore. This lets the KMS serve a DEK for ANY encrypted write,
/// including blocks written by the legacy auto-commit path.
///
/// Holds only [`Weak`] back-references (no `Arc<DB>`, no `Arc<blockstore>`):
/// both `Arc<DB>` and the blockstore's `Arc<Store>` keep the underlying store —
/// and thus its exclusive storage lock — alive forever via the
/// DB→KMS→`KeyStore`→adapter cycle, so the lock never releases on node close
/// and reopening the same data dir fails (#976). The owning `Arc`s live on the
/// node (the DB itself, and the shared KMS `DefraBlockstore` parked alongside
/// it), so they drop on node teardown and the lock is freed.
///
/// The blockstore is held as a `Weak` to the SAME shared instance the rest of
/// the node uses, preserving its in-process block cache. That cache matters for
/// correctness, not just speed: a fresh per-call blockstore misses a DEK that
/// was just written, forcing a redundant cross-peer fetch + `put` whose
/// independent write transaction conflicts with the in-flight merge txn
/// ("transaction conflict. Please retry") and aborts the merge.
pub struct DbEncBlockStore<S, B>
where
    S: storage::corekv::Store + Send + Sync + 'static,
    B: blockstore::Blockstore,
{
    db: Weak<crate::DB<S>>,
    blockstore: Weak<B>,
}

impl<S, B> DbEncBlockStore<S, B>
where
    S: storage::corekv::Store + Send + Sync + 'static,
    B: blockstore::Blockstore,
{
    /// `blockstore` must be the node's shared durable blockstore; the caller
    /// retains the owning `Arc` for the node's lifetime (see struct docs).
    pub fn new(db: Arc<crate::DB<S>>, blockstore: Arc<B>) -> Self {
        Self {
            db: Arc::downgrade(&db),
            blockstore: Arc::downgrade(&blockstore),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, B> EncBlockStore for DbEncBlockStore<S, B>
where
    S: storage::corekv::Store + Send + Sync + 'static,
    B: blockstore::Blockstore + 'static,
{
    async fn get_block(&self, cid: &EncryptionCid) -> kms::Result<Option<Bytes>> {
        let Some(db) = self.db.upgrade() else {
            // DB dropped → node shutting down → nothing to serve.
            return Ok(None);
        };
        // Mirror the merge decrypt path: encstore first, then blockstore.
        let key = cid.to_bytes();
        let from_encstore = on_pool(db, move |db| async move {
            let txn = db
                .new_txn(true)
                .await
                .map_err(|e| kms::Error::Storage(e.to_string()))?;
            let encstore = txn
                .encstore()
                .map_err(|e| kms::Error::Storage(e.to_string()))?;
            encstore
                .get(&key)
                .await
                .map_err(|e| kms::Error::Storage(e.to_string()))
        })
        .await?;
        if let Some(bytes) = from_encstore {
            return Ok(Some(bytes));
        }
        let Some(blockstore) = self.blockstore.upgrade() else {
            return Ok(None);
        };
        match blockstore
            .get(cid)
            .await
            .map_err(|e| kms::Error::Storage(e.to_string()))?
        {
            Some(bytes) => Ok(Some(bytes)),
            None => Ok(None),
        }
    }

    async fn put_block(&self, cid: EncryptionCid, bytes: Bytes) -> kms::Result<()> {
        let Some(blockstore) = self.blockstore.upgrade() else {
            return Err(kms::Error::Storage("blockstore gone".into()));
        };
        blockstore
            .put(&cid, &bytes)
            .await
            .map_err(|e| kms::Error::Storage(e.to_string()))
    }
}

/// Bridges the node NAC manager into the KMS `NodeAcpRead` trait.
pub struct DbNodeAcpRead {
    nac: Arc<dyn crate::NacManagerApi>,
}

impl DbNodeAcpRead {
    pub fn new(nac: Arc<dyn crate::NacManagerApi>) -> Self {
        Self { nac }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl NodeAcpRead for DbNodeAcpRead {
    async fn check_node_permission(
        &self,
        identity: &identity::Did,
        permission: &str,
    ) -> acp::Result<bool> {
        let perm = match permission {
            "read-document" => acp::nac::NodePermission::DocumentRead,
            other => {
                return Err(acp::Error::PermissionDenied(format!(
                    "unknown node permission {other}"
                )))
            }
        };
        self.nac
            .check_permission(identity, perm)
            .await
            .map_err(|e| acp::Error::Storage(e.to_string()))
    }
}
