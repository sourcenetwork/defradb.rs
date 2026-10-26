//! The replay decision and configuration share one validated transaction.

use std::sync::Arc;
use std::time::Duration;

use events::Bus;
use rapidhash::{RapidHashMap, RapidHashSet};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use storage::corekv::{
    self, Key, OnCommit, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue, RetryOptions,
    RetryTxn, Store, TxnCallbacks,
};
use storage::keys::peerstore::{ReplicatorIncarnationKey, ReplicatorKey};
use storage::stores::peerstore::{decode_retry_marker_generation, encode_retry_marker_generation};

use crate::replicator_installs::ReplicatorInstalls;
use crate::{P2PError, P2PErrorExt as _, P2PResult};

const PREFIX: &[u8] = b"/rep/install/v1/";
const PENDING_PREFIX: &[u8] = b"/rep/install/pending/";
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const INTERRUPTED: &str =
    "initial replay interrupted; add the replicator again to retry";

#[cfg(test)]
#[path = "../tests/unit/replicator_install_store.rs"]
pub(crate) mod tests;

#[derive(Clone)]
pub struct ReplicatorInstallRequest {
    pub(crate) info: p2p::ReplicatorInfo,
    pub(crate) collections: Vec<String>,
    capabilities: RapidHashMap<String, [u8; 32]>,
}

impl ReplicatorInstallRequest {
    pub(crate) fn new(
        info: p2p::ReplicatorInfo,
        collections: Vec<String>,
        capabilities: &[(String, String)],
    ) -> Self {
        Self {
            info,
            collections,
            capabilities: capabilities
                .iter()
                .map(|(collection, token)| (collection.clone(), Sha256::digest(token).into()))
                .collect(),
        }
    }
}

/// The durable plan contains no encryption key or bearer capability.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplicatorInstall {
    pub(crate) id: u64,
    pub(crate) peer_id: String,
    pub(crate) collections: Vec<String>,
    pub(crate) replay: Vec<String>,
    capabilities: RapidHashMap<String, [u8; 32]>,
    pending: bool,
    error: Option<String>,
    #[serde(skip)]
    _owner: Option<Arc<db::database::replication::InstallOwner>>,
}

fn key(peer: &str) -> Vec<u8> {
    [PREFIX, peer.as_bytes()].concat()
}

fn pending_key(peer: &str) -> Vec<u8> {
    [PENDING_PREFIX, peer.as_bytes()].concat()
}

fn storage_error(error: impl std::fmt::Display) -> corekv::Error {
    corekv::Error::Other(error.to_string())
}

fn persistence_error(error: impl std::fmt::Display) -> P2PError {
    P2PError::persistence(format!("replicator install: {error}"))
}

enum Decision {
    Reallocate,
    Busy(u64),
    Installed(ReplicatorInstall),
}

struct Prepare {
    request: ReplicatorInstallRequest,
    id: u64,
    owner: Arc<db::database::replication::InstallOwner>,
}

impl RetryAttemptFn for Prepare {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let peer = self.request.info.peer_id_str();
            let previous: Option<ReplicatorInstall> = txn
                .get(&key(peer))
                .await?
                .map(|bytes| serde_json::from_slice(&bytes).map_err(storage_error))
                .transpose()?;
            if let Some(previous) = &previous {
                if previous.pending {
                    return Ok(Box::new(Decision::Busy(previous.id)) as RetryAttemptValue);
                }
            }
            let row_key = ReplicatorKey::new(peer).bytes();
            let existing = txn
                .get(&row_key)
                .await?
                .map(|bytes| p2p::ReplicatorInfo::from_bytes(&bytes).map_err(storage_error))
                .transpose()?;
            let existing_ids: RapidHashSet<String> = existing
                .as_ref()
                .map(|info| info.collections.iter().cloned().collect())
                .unwrap_or_default();
            let existing_filters = existing.map(|info| info.filters).unwrap_or_default();
            let changed = self
                .request
                .info
                .collections
                .iter()
                .filter(|collection| {
                    previous.as_ref().is_none_or(|previous| {
                        previous.error.is_some()
                            || previous.capabilities.get(*collection)
                                != self.request.capabilities.get(*collection)
                    })
                })
                .cloned()
                .collect();
            let replay = crate::collections_requiring_replay(
                &self.request.collections,
                &self.request.info.collections,
                &existing_ids,
                &existing_filters,
                &self.request.info.filters,
                &changed,
            );
            let incarnation_key = ReplicatorIncarnationKey::new(peer).bytes();
            let incarnation = txn
                .get(&incarnation_key)
                .await?
                .map(|bytes| {
                    decode_retry_marker_generation(&bytes)
                        .ok_or_else(|| storage_error("invalid peer incarnation"))
                })
                .transpose()?
                .unwrap_or(0);
            if incarnation > self.id {
                return Ok(Box::new(Decision::Reallocate) as RetryAttemptValue);
            }
            let plan = ReplicatorInstall {
                id: self.id,
                peer_id: peer.to_owned(),
                collections: self.request.collections.clone(),
                replay,
                capabilities: self.request.capabilities.clone(),
                pending: true,
                error: None,
                _owner: Some(Arc::clone(&self.owner)),
            };
            txn.set(
                &row_key,
                &self.request.info.to_bytes().map_err(storage_error)?,
            )
            .await?;
            txn.set(&incarnation_key, &encode_retry_marker_generation(self.id))
                .await?;
            txn.set(
                &key(peer),
                &serde_json::to_vec(&plan).map_err(storage_error)?,
            )
            .await?;
            txn.set(&pending_key(peer), &encode_retry_marker_generation(self.id))
                .await?;
            Ok(Box::new(Decision::Installed(plan)) as RetryAttemptValue)
        })
    }
}

pub(crate) async fn prepare<S>(
    store: &S,
    retry: RetryOptions,
    waiters: &ReplicatorInstalls,
    request: &ReplicatorInstallRequest,
) -> P2PResult<ReplicatorInstall>
where
    S: Store,
{
    prepare_with_timeout(store, retry, waiters, request, WAIT_TIMEOUT).await
}

pub(crate) async fn prepare_with_timeout<S>(
    store: &S,
    retry: RetryOptions,
    waiters: &ReplicatorInstalls,
    request: &ReplicatorInstallRequest,
    timeout: Duration,
) -> P2PResult<ReplicatorInstall>
where
    S: Store,
{
    let deadline = n0_future::time::Instant::now() + timeout;
    loop {
        if n0_future::time::Instant::now() >= deadline {
            return Err(P2PError::transport(
                "replicator replay is still active; try again",
            ));
        }
        let id = store
            .allocate(b"/rep/incarnation/sequence", 1)
            .await
            .map_err(persistence_error)?
            .end;
        let owner = waiters
            .runtime
            .own_install(request.info.peer_id_str(), id)
            .ok_or_else(|| persistence_error("duplicate install identity"))?;
        let decision: Decision = corekv::transact(
            store,
            retry,
            None,
            Box::new(Prepare {
                request: request.clone(),
                id,
                owner,
            }),
        )
        .await
        .map_err(|error| persistence_error(error.into_storage()))?;
        let id = match decision {
            Decision::Reallocate => continue,
            Decision::Installed(plan) => return Ok(plan),
            Decision::Busy(id) => id,
        };
        let watch = waiters.watch(request.info.peer_id_str(), id);
        let snapshot = store.new_txn(true).await.map_err(persistence_error)?;
        let current: Option<ReplicatorInstall> = snapshot
            .get(&key(request.info.peer_id_str()))
            .await
            .map_err(persistence_error)?
            .map(|bytes| serde_json::from_slice(&bytes).map_err(persistence_error))
            .transpose()?;
        drop(snapshot);
        if current.is_some_and(|current| current.id == id && current.pending) {
            let remaining = deadline.saturating_duration_since(n0_future::time::Instant::now());
            n0_future::time::timeout(remaining, watch.wait())
                .await
                .map_err(|_| P2PError::transport("replicator replay is still active; try again"))?;
        }
        // A notification only ends the wait. The next transaction decides again.
    }
}

struct Finish {
    plan: ReplicatorInstall,
    error: Option<String>,
    bus: Option<Arc<dyn Bus>>,
    waiters: Arc<ReplicatorInstalls>,
}

impl RetryAttemptFn for Finish {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let mut io = txn.begin_attempt_io()?;
            let Some(bytes) = io.get(&key(&self.plan.peer_id)).await? else {
                txn.finish_attempt_io(io, TxnCallbacks::default())?;
                return Ok(Box::new(()) as RetryAttemptValue);
            };
            let mut current: ReplicatorInstall =
                serde_json::from_slice(&bytes).map_err(storage_error)?;
            let mut callbacks = TxnCallbacks::default();
            if current.id == self.plan.id && current.pending {
                let present = io
                    .has(&ReplicatorKey::new(&current.peer_id).bytes())
                    .await?;
                let incarnation = io
                    .get(&ReplicatorIncarnationKey::new(&current.peer_id).bytes())
                    .await?
                    .map(|bytes| {
                        decode_retry_marker_generation(&bytes)
                            .ok_or_else(|| storage_error("invalid peer incarnation"))
                    })
                    .transpose()?;
                current.pending = false;
                current.error = self.error.clone().or_else(|| {
                    if !present {
                        Some("replicator removed during initial replay".to_owned())
                    } else if incarnation != Some(current.id) {
                        Some("replicator configuration changed during initial replay".to_owned())
                    } else {
                        None
                    }
                });
                io.set(
                    &key(&current.peer_id),
                    &serde_json::to_vec(&current).map_err(storage_error)?,
                )
                .await?;
                let bus = self.bus.clone();
                io.delete(&pending_key(&current.peer_id)).await?;
                let waiters = Arc::clone(&self.waiters);
                callbacks.on_commit(OnCommit::Ordered(Box::new(move || {
                    if let Some(bus) = bus {
                        bus.publish(events::Message::replicator_completed_with_data(
                            events::ReplicatorCompletedData {
                                peer_id: current.peer_id.clone(),
                                collections: current.collections,
                                skipped: current.replay.is_empty(),
                                error: current.error,
                            },
                        ));
                    }
                    waiters.finish(&current.peer_id, current.id);
                })))?;
            }
            txn.finish_attempt_io(io, callbacks)?;
            Ok(Box::new(()) as RetryAttemptValue)
        })
    }
}

pub(crate) async fn finish<S>(
    store: &S,
    retry: RetryOptions,
    waiters: Arc<ReplicatorInstalls>,
    bus: Option<Arc<dyn Bus>>,
    plan: &ReplicatorInstall,
    error: Option<String>,
) -> P2PResult<()>
where
    S: Store,
{
    corekv::transact(
        store,
        retry,
        None,
        Box::new(Finish {
            plan: plan.clone(),
            error,
            bus,
            waiters,
        }),
    )
    .await
    .map_err(|error| persistence_error(error.into_storage()))
}

/// Startup and periodic sweeps settle only installs whose replay owner is gone.
pub(crate) async fn recover<S>(
    store: &S,
    retry: RetryOptions,
    waiters: Arc<ReplicatorInstalls>,
    bus: Option<Arc<dyn Bus>>,
) -> P2PResult<()>
where
    S: Store,
{
    let snapshot = store.new_txn(true).await.map_err(persistence_error)?;
    let mut cursor = snapshot
        .iterator(corekv::IterOptions::new().with_prefix(PENDING_PREFIX.to_vec()))
        .await
        .map_err(persistence_error)?;
    while let Some(entry) = cursor.next().await.map_err(persistence_error)? {
        let recovered: P2PResult<()> = async {
            let peer = std::str::from_utf8(&entry.key[PENDING_PREFIX.len()..])
                .map_err(persistence_error)?;
            let plan = read_pending_install(snapshot.as_ref(), peer, &entry.value).await?;
            recover_plan(store, retry, &waiters, bus.clone(), &plan).await?;
            Ok(())
        }
        .await;
        if let Err(error) = recovered {
            tracing::warn!(%error, "failed to recover a replicator install");
        }
    }
    Ok(())
}

async fn read_pending_install(
    snapshot: &dyn corekv::Txn,
    peer: &str,
    pending: &[u8],
) -> P2PResult<ReplicatorInstall> {
    let Some(bytes) = snapshot.get(&key(peer)).await.map_err(persistence_error)? else {
        return Err(persistence_error("pending install has no durable record"));
    };
    let plan: ReplicatorInstall = serde_json::from_slice(&bytes).map_err(persistence_error)?;
    if decode_retry_marker_generation(pending) != Some(plan.id) || !plan.pending {
        return Err(persistence_error(
            "pending install index does not match its record",
        ));
    }
    Ok(plan)
}

async fn recover_plan<S: Store>(
    store: &S,
    retry: RetryOptions,
    waiters: &Arc<ReplicatorInstalls>,
    bus: Option<Arc<dyn Bus>>,
    plan: &ReplicatorInstall,
) -> P2PResult<()> {
    if !waiters.runtime.install_is_running(&plan.peer_id, plan.id) {
        finish(
            store,
            retry,
            Arc::clone(waiters),
            bus,
            plan,
            Some(INTERRUPTED.to_owned()),
        )
        .await?;
    }
    Ok(())
}

/// An install request recovers only its peer; periodic sweeps own global recovery.
pub(crate) async fn recover_peer<S: Store>(
    store: &S,
    retry: RetryOptions,
    waiters: Arc<ReplicatorInstalls>,
    bus: Option<Arc<dyn Bus>>,
    peer: &str,
) -> P2PResult<()> {
    let snapshot = store.new_txn(true).await.map_err(persistence_error)?;
    let Some(pending) = snapshot
        .get(&pending_key(peer))
        .await
        .map_err(persistence_error)?
    else {
        return Ok(());
    };
    let plan = read_pending_install(snapshot.as_ref(), peer, &pending).await?;
    drop(snapshot);
    recover_plan(store, retry, &waiters, bus, &plan).await
}
