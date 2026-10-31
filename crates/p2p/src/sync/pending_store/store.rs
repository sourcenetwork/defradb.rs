//! The durable registrations in the systemstore, each transition one engine
//! transaction.
//!
//! A live record sits at `/p2p/pending_dag/{root}` with the slot it holds;
//! the durable current head of each sender scope at
//! `/p2p/pending_dag_scope/{scope}`; a quarantined root at
//! `/p2p/quarantined_dag/{root}`; the slots under `/p2p/pending_dag_slot/`
//! ([`super::slots`]); and the sweep's stamp, which gives a sweep a commit of
//! its own and so a place in the receipt order.

use std::sync::Arc;

use async_trait::async_trait;
use cid::Cid;
use datastore::{BasicTxn, NamespaceView};
use serde::{Deserialize, Serialize};
use storage::corekv::{
    IterOptions, Key, Reader, RetryAttemptError, RetryAttemptValue, RetryOptions, Store,
};
use storage::keys::systemstore::{P2PPendingDagKey, P2PQuarantinedDagKey};
use storage::stores::Systemstore;

use super::view::{scope_head, PersistedView};
use super::{
    slots, Admission, PendingDagStorage, PersistedPendingDag, PersistedQuarantinedDag, Registration,
};
use crate::error::{Error, Result};
use crate::sync::head_scope::{HeadVersion, ScopeDecision};

const SCOPE_PREFIX: &[u8] = b"/p2p/pending_dag_scope/";
const SWEEP_KEY: &[u8] = b"/p2p/pending_dag_sweep";

type AttemptResult = std::result::Result<RetryAttemptValue, RetryAttemptError>;

/// `PendingDagStorage` over the systemstore keyspace.
pub struct PendingDagStore<S: Store> {
    store: Arc<S>,
    systemstore: Systemstore<S>,
    retry: RetryOptions,
}

/// A live record as stored: the registration and the slot it holds. A record
/// written before slots holds none.
#[derive(Deserialize)]
struct Stored {
    #[serde(flatten)]
    record: PersistedPendingDag,
    #[serde(default)]
    slot: Option<u64>,
}

#[derive(Serialize)]
struct StoredRef<'a> {
    #[serde(flatten)]
    record: &'a PersistedPendingDag,
    #[serde(skip_serializing_if = "Option::is_none")]
    slot: Option<u64>,
}

impl<S: Store> PendingDagStore<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self {
            systemstore: Systemstore::new(Arc::clone(&store)),
            store,
            retry: storage::corekv::engine_retry_options(storage::corekv::DEFAULT_MAX_TXN_RETRIES),
        }
    }

    /// Re-run a conflicting transition up to `max_txn_retries` times, the
    /// node's `datastore.max_txn_retries`.
    pub fn with_max_txn_retries(mut self, max_txn_retries: u32) -> Self {
        self.retry = storage::corekv::engine_retry_options(max_txn_retries);
        self
    }

    /// Runs one transition as an engine transaction.
    async fn transition<T, F>(&self, body: F) -> Result<T>
    where
        T: 'static,
        F: for<'t> FnMut(&'t mut BasicTxn) -> storage::corekv::RetryAttemptFuture<'t>
            + storage::corekv::MaybeSend
            + 'static,
    {
        datastore::transact(&*self.store, self.retry, body)
            .await
            .map_err(|error| Error::Storage(error.into_storage().to_string()))
    }

    async fn snapshot_txn(&self) -> Result<Box<dyn storage::corekv::Txn>> {
        self.systemstore
            .new_txn(true)
            .await
            .map_err(|e| Error::Storage(e.to_string()))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static> PendingDagStorage for PendingDagStore<S> {
    async fn register(
        &self,
        registration: Registration<'_>,
        view: &Arc<PersistedView>,
    ) -> Result<Admission> {
        // The ticket is allocated outside the transaction: an allocation is
        // not undone by an abort, and a re-run reuses it.
        let ticket = match registration.capacity {
            Some(_) => slots::ticket(&self.systemstore)
                .await
                .map_err(|e| Error::Storage(e.to_string()))?,
            None => 0,
        };
        let root = registration.root;
        let record = registration.record.clone();
        let capacity = registration.capacity;
        let view = Arc::clone(view);
        self.transition(move |txn| {
            Box::pin(register(
                txn,
                root,
                record.clone(),
                capacity,
                ticket,
                Arc::clone(&view),
            ))
        })
        .await
    }

    async fn remove(&self, root_cid: &Cid, view: &Arc<PersistedView>) -> Result<bool> {
        let root = *root_cid;
        let view = Arc::clone(view);
        self.transition(move |txn| Box::pin(remove(txn, root, Arc::clone(&view))))
            .await
    }

    async fn quarantine(
        &self,
        root_cid: &Cid,
        entry: &PersistedQuarantinedDag,
        view: &Arc<PersistedView>,
    ) -> Result<bool> {
        let root = *root_cid;
        let entry = entry.to_bytes()?;
        let view = Arc::clone(view);
        self.transition(move |txn| {
            Box::pin(quarantine(txn, root, entry.clone(), Arc::clone(&view)))
        })
        .await
    }

    async fn sweep(&self, view: &Arc<PersistedView>) -> Result<Vec<(Cid, PersistedPendingDag)>> {
        let view = Arc::clone(view);
        self.transition(move |txn| Box::pin(sweep(txn, Arc::clone(&view))))
            .await
    }

    async fn put(&self, root_cid: &Cid, record: &PersistedPendingDag) -> Result<()> {
        let key = record_key(root_cid);
        let value = encode(record, None).map_err(|e| Error::Storage(e.to_string()))?;
        self.transition(move |txn| {
            let (key, value) = (key.clone(), value.clone());
            Box::pin(async move {
                txn.systemstore().set(&key, &value).await?;
                Ok(Box::new(()) as RetryAttemptValue)
            })
        })
        .await
    }

    async fn load_all(&self) -> Result<Vec<(Cid, PersistedPendingDag)>> {
        let txn = self.snapshot_txn().await?;
        let opts = IterOptions::new().with_prefix(P2PPendingDagKey::p2p_pending_dag_prefix());
        let mut iter = txn
            .iterator(opts)
            .await
            .map_err(|e| Error::Storage(e.to_string()))?;
        let mut records = Vec::new();
        while let Some(pair) = iter
            .next()
            .await
            .map_err(|e| Error::Storage(e.to_string()))?
        {
            if let Some((root, stored)) = decode_pair(&pair.key, &pair.value) {
                records.push((root, stored.record));
            }
        }
        iter.close()
            .await
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(records)
    }

    async fn is_quarantined(&self, root_cid: &Cid) -> Result<bool> {
        let key = P2PQuarantinedDagKey::new(root_cid.to_string());
        let txn = self.snapshot_txn().await?;
        let value = txn
            .get(&key.bytes())
            .await
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(value.is_some())
    }

    async fn load_quarantined(&self) -> Result<Vec<(Cid, PersistedQuarantinedDag)>> {
        let txn = self.snapshot_txn().await?;
        let opts =
            IterOptions::new().with_prefix(P2PQuarantinedDagKey::p2p_quarantined_dag_prefix());
        let mut iter = txn
            .iterator(opts)
            .await
            .map_err(|e| Error::Storage(e.to_string()))?;

        let mut records = Vec::new();
        while let Some(pair) = iter
            .next()
            .await
            .map_err(|e| Error::Storage(e.to_string()))?
        {
            let key_str = String::from_utf8_lossy(&pair.key);
            let Some(cid_str) = key_str.strip_prefix("/p2p/quarantined_dag/") else {
                continue;
            };
            let Ok(root_cid) = cid_str.parse::<Cid>() else {
                tracing::warn!(key = %key_str, "Skipping quarantined DAG with invalid CID key");
                continue;
            };
            match PersistedQuarantinedDag::from_bytes(&pair.value) {
                Ok(record) => records.push((root_cid, record)),
                Err(error) => {
                    tracing::warn!(
                        root_cid = %root_cid,
                        error = %error,
                        "Skipping undecodable quarantined DAG record"
                    );
                }
            }
        }
        iter.close()
            .await
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(records)
    }

    async fn remove_quarantined(&self, root_cid: &Cid) -> Result<()> {
        let key = P2PQuarantinedDagKey::new(root_cid.to_string()).bytes();
        self.transition(move |txn| {
            let key = key.clone();
            Box::pin(async move {
                txn.systemstore().delete(&key).await?;
                Ok(Box::new(()) as RetryAttemptValue)
            })
        })
        .await
    }
}

/// Registers `root`: decides against the durable current head of its scope,
/// retires an older one, takes a slot, and writes the record and the scope's
/// new head.
async fn register(
    txn: &mut BasicTxn,
    root: Cid,
    record: PersistedPendingDag,
    capacity: Option<usize>,
    ticket: u64,
    view: Arc<PersistedView>,
) -> AttemptResult {
    let systemstore = txn.systemstore();
    let head = scope_head(root, &record);
    let retired = match &head {
        None => None,
        Some((scope, version)) => {
            let current = read_head(&systemstore, &scope.key(SCOPE_PREFIX)).await?;
            match ScopeDecision::of(*version, current) {
                ScopeDecision::CoveredByCurrent => return Ok(Box::new(Admission::Covered)),
                ScopeDecision::Supersedes(older) => Some(older),
                ScopeDecision::Current | ScopeDecision::Independent => None,
            }
        }
    };
    let slot = match read_record(&systemstore, &root).await? {
        Some(own) => {
            if let Some(older) = retired {
                discharge(&systemstore, &older, true).await?;
            }
            own.slot
        }
        None => match retired {
            Some(older) => match discharge(&systemstore, &older, false).await? {
                Some(Stored {
                    slot: Some(slot), ..
                }) => {
                    slots::assign(&systemstore, slot, &root).await?;
                    Some(slot)
                }
                Some(Stored { slot: None, .. }) => None,
                None => match capacity {
                    None => None,
                    Some(capacity) => {
                        match slots::claim(&systemstore, &root, ticket, capacity).await? {
                            Some(slot) => Some(slot),
                            None => return Ok(Box::new(Admission::AtCapacity)),
                        }
                    }
                },
            },
            None => match capacity {
                None => None,
                Some(capacity) => {
                    match slots::claim(&systemstore, &root, ticket, capacity).await? {
                        Some(slot) => Some(slot),
                        None => return Ok(Box::new(Admission::AtCapacity)),
                    }
                }
            },
        },
    };
    systemstore
        .set(&record_key(&root), &encode(&record, slot)?)
        .await?;
    if let Some((scope, version)) = &head {
        systemstore
            .set(&scope.key(SCOPE_PREFIX), &version.to_bytes())
            .await?;
    }
    drop(systemstore);
    on_commit(txn, view.registered(root, &record, retired))?;
    Ok(Box::new(Admission::Registered))
}

/// Deletes `root`'s record and frees its slot; the value is whether it had a
/// record.
async fn remove(txn: &mut BasicTxn, root: Cid, view: Arc<PersistedView>) -> AttemptResult {
    let systemstore = txn.systemstore();
    let discharged = discharge(&systemstore, &root, true).await?;
    drop(systemstore);
    let had_record = discharged.is_some();
    if let Some(stored) = discharged {
        on_commit(txn, view.removed(root, Some(&stored.record), false))?;
    }
    Ok(Box::new(had_record))
}

/// Writes `root`'s quarantine record and deletes its live record and slot;
/// the value is whether it was quarantined before.
async fn quarantine(
    txn: &mut BasicTxn,
    root: Cid,
    entry: Vec<u8>,
    view: Arc<PersistedView>,
) -> AttemptResult {
    let systemstore = txn.systemstore();
    let key = P2PQuarantinedDagKey::new(root.to_string()).bytes();
    let already = systemstore.get(&key).await?.is_some();
    systemstore.set(&key, &entry).await?;
    let discharged = discharge(&systemstore, &root, true).await?;
    drop(systemstore);
    on_commit(
        txn,
        view.removed(
            root,
            discharged.as_ref().map(|stored| &stored.record),
            !already,
        ),
    )?;
    Ok(Box::new(already))
}

/// Reads every live record with a validated scan, so a record that comes or
/// goes before the sweep commits makes it run again, and stamps the sweep so
/// it commits with a receipt of its own.
async fn sweep(txn: &mut BasicTxn, view: Arc<PersistedView>) -> AttemptResult {
    let systemstore = txn.systemstore();
    let opts = IterOptions::new()
        .with_prefix(P2PPendingDagKey::p2p_pending_dag_prefix())
        .with_validation(true);
    let mut iter = systemstore.iterator(opts).await?;
    let mut records = Vec::new();
    while let Some(pair) = iter.next().await? {
        if let Some((root, stored)) = decode_pair(&pair.key, &pair.value) {
            records.push((root, stored.record));
        }
    }
    iter.close().await?;
    let stamp = systemstore
        .get(SWEEP_KEY)
        .await?
        .and_then(|bytes| <[u8; 8]>::try_from(bytes.as_ref()).ok())
        .map_or(0, u64::from_be_bytes);
    systemstore
        .set(SWEEP_KEY, &stamp.wrapping_add(1).to_be_bytes())
        .await?;
    drop(systemstore);
    on_commit(txn, view.swept(&records))?;
    Ok(Box::new(records))
}

/// Deletes `root`'s record, its scope's head when that is `root`, and, with
/// `free_slot`, its slot. Returns what was stored.
async fn discharge(
    systemstore: &NamespaceView,
    root: &Cid,
    free_slot: bool,
) -> storage::corekv::Result<Option<Stored>> {
    let Some(stored) = read_record(systemstore, root).await? else {
        return Ok(None);
    };
    systemstore.delete(&record_key(root)).await?;
    if let (true, Some(slot)) = (free_slot, stored.slot) {
        slots::free(systemstore, slot).await?;
    }
    if let Some((scope, _)) = scope_head(*root, &stored.record) {
        let key = scope.key(SCOPE_PREFIX);
        if read_head(systemstore, &key)
            .await?
            .is_some_and(|head| head.cid == *root)
        {
            systemstore.delete(&key).await?;
        }
    }
    Ok(Some(stored))
}

async fn read_record(
    systemstore: &NamespaceView,
    root: &Cid,
) -> storage::corekv::Result<Option<Stored>> {
    let Some(bytes) = systemstore.get(&record_key(root)).await? else {
        return Ok(None);
    };
    defra_core::cbor::from_slice(&bytes)
        .map(Some)
        .map_err(|error| {
            storage::corekv::Error::Other(format!(
                "the pending DAG record of {root} cannot be decoded: {error}"
            ))
        })
}

async fn read_head(
    systemstore: &NamespaceView,
    key: &[u8],
) -> storage::corekv::Result<Option<HeadVersion>> {
    Ok(systemstore
        .get(key)
        .await?
        .and_then(|bytes| HeadVersion::from_bytes(&bytes)))
}

fn on_commit(
    txn: &mut BasicTxn,
    callback: storage::corekv::TxnCallback,
) -> std::result::Result<(), RetryAttemptError> {
    txn.on_commit(storage::corekv::OnCommit::Ordered(callback))
        .map_err(|error| storage::corekv::Error::Other(error.to_string()).into())
}

fn record_key(root: &Cid) -> Vec<u8> {
    P2PPendingDagKey::new(root.to_string()).bytes()
}

/// The bytes `root`'s record writes: its key and value, and its sender
/// scope's head key and value when it has a scope.
pub(crate) fn footprint(root: &Cid, record: &PersistedPendingDag) -> u64 {
    let value = encode(record, None).map_or(0, |value| value.len());
    let scope = scope_head(*root, record).map_or(0, |(scope, version)| {
        scope.key(SCOPE_PREFIX).len() + version.to_bytes().len()
    });
    (record_key(root).len() + value + scope) as u64
}

fn encode(record: &PersistedPendingDag, slot: Option<u64>) -> storage::corekv::Result<Vec<u8>> {
    defra_core::cbor::to_vec(&StoredRef { record, slot })
        .map_err(|error| storage::corekv::Error::Other(error.to_string()))
}

/// A scanned record, or `None`, reported, for a key or value it cannot read.
fn decode_pair(key: &[u8], value: &[u8]) -> Option<(Cid, Stored)> {
    let key_str = String::from_utf8_lossy(key);
    let cid_str = key_str.strip_prefix("/p2p/pending_dag/")?;
    let Ok(root) = cid_str.parse::<Cid>() else {
        tracing::warn!(key = %key_str, "Skipping persisted pending DAG with invalid CID key");
        return None;
    };
    match defra_core::cbor::from_slice::<Stored>(value) {
        Ok(stored) => Some((root, stored)),
        Err(error) => {
            tracing::warn!(
                root_cid = %root,
                error = %error,
                "Skipping undecodable persisted pending DAG record"
            );
            None
        }
    }
}
