use crate::corekv::{
    IterOptions, Key, Reader, Result, RetryAttemptFn, RetryAttemptFuture, RetryOptions, RetryTxn,
    Store, Txn, Writer,
};
use crate::keys::peerstore::{
    ReplicatorAckedHeadsKey, ReplicatorIncarnationKey, ReplicatorKey, ReplicatorRetryCollectionKey,
    ReplicatorRetryDocIDKey, ReplicatorRetryIDKey,
};
use crate::namespace::{Namespace, NamespacedStore};
use async_trait::async_trait;
/// Peerstore - Peer and replication metadata
///
/// The Peerstore handles storage of replicator configuration, replication
/// retry tracking, and search engine retry tracking for P2P operations.
use bytes::Bytes;
use std::sync::Arc;
use tracing;

const RETRY_MARKER_GENERATION_MAGIC: &[u8] = b"DFRGMARKER1";

fn encode_retry_marker_generation(generation: u64) -> Vec<u8> {
    let mut bytes = RETRY_MARKER_GENERATION_MAGIC.to_vec();
    bytes.extend_from_slice(&generation.to_be_bytes());
    bytes
}

fn decode_retry_marker_generation(bytes: &[u8]) -> Option<u64> {
    let generation_bytes = bytes.strip_prefix(RETRY_MARKER_GENERATION_MAGIC)?;
    let generation_bytes: [u8; 8] = generation_bytes.try_into().ok()?;
    Some(u64::from_be_bytes(generation_bytes))
}

/// Peerstore provides storage for peer and replication metadata
pub struct Peerstore<S: Store> {
    store: NamespacedStore<S>,
    retry_schedule: super::RetrySchedule,
    retry: RetryOptions,
    /// The pool [`run`](Self::run) runs calls on (`peerstore_placed.rs`).
    pool: Option<workers::Pool>,
}

struct RegisterScopeMarkerAttempt {
    peer_id: String,
    expected_incarnation: u64,
    doc_id: String,
    collection_id: String,
    retry_info_bytes: Vec<u8>,
    retry_schedule: super::RetrySchedule,
    generation: u64,
}

struct RefreshPushFailureAttempt {
    peer_id: String,
    doc_id: String,
    collection_id: String,
    retry_info_bytes: Vec<u8>,
    expected_generation: u64,
    new_generation: u64,
}

struct SetReplicatorIfMarkerGenerationAttempt {
    peer_id: String,
    doc_id: String,
    collection_id: String,
    expected_generation: u64,
    expected_replicator: Vec<u8>,
    updated_replicator: Vec<u8>,
}

struct SetReplicatorIfIncarnationAttempt {
    peer_id: String,
    expected_incarnation: u64,
    expected_replicator: Vec<u8>,
    updated_replicator: Vec<u8>,
    require_no_markers: bool,
}

enum PeerstoreRetryAttempt {
    DeleteReplicator {
        peer_id: String,
    },
    Reschedule {
        peer_id: String,
        expected_incarnation: Option<u64>,
        defer_for: Option<std::time::Duration>,
        advance_cursor_by: u64,
        schedule: super::RetrySchedule,
    },
    Restart {
        peer_id: String,
        expected_incarnation: Option<u64>,
        advance_cursor_by: u64,
        schedule: super::RetrySchedule,
    },
    RecordHint {
        peer_id: String,
        delay: std::time::Duration,
    },
    Activate {
        peer_id: String,
    },
    CompleteMarker {
        peer_id: String,
        key: Vec<u8>,
        /// The marker generation this completion is for; `None` when no
        /// marker tracks the scope (a live push that registered none).
        expected_generation: Option<u64>,
        acked_key: Vec<u8>,
        acked: Vec<u8>,
    },
    ClearPeer {
        peer_id: String,
        expected_incarnation: Option<u64>,
    },
}

impl RetryAttemptFn for PeerstoreRetryAttempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            use PeerstoreRetryAttempt as Attempt;
            match self {
                Attempt::DeleteReplicator { peer_id } => {
                    let mut io = txn.begin_attempt_io()?;
                    let key = ReplicatorKey::new(peer_id.as_str()).bytes();
                    // This point read conflicts with every state transition that
                    // validates the peer incarnation before writing.
                    let _ = io.get(&key).await?;
                    io.delete(&key).await?;
                    let mut keys = Vec::new();
                    for prefix in [
                        ReplicatorRetryDocIDKey::peer_prefix(peer_id.as_str()),
                        ReplicatorRetryCollectionKey::peer_prefix(peer_id.as_str()),
                        ReplicatorAckedHeadsKey::peer_prefix(peer_id.as_str(), false),
                        ReplicatorAckedHeadsKey::peer_prefix(peer_id.as_str(), true),
                    ] {
                        let mut iter = io.iterator(IterOptions::new().with_prefix(prefix)).await?;
                        while let Some(pair) = iter.next().await? {
                            keys.push(pair.key);
                        }
                        iter.close().await?;
                    }
                    for key in keys {
                        io.delete(&key).await?;
                    }
                    io.delete(&ReplicatorRetryIDKey::new(peer_id.as_str()).bytes())
                        .await?;
                    txn.finish_attempt_io(io, crate::corekv::TxnCallbacks::default())?;
                    Ok(Box::new(()) as crate::corekv::RetryAttemptValue)
                }
                Attempt::Reschedule {
                    peer_id,
                    expected_incarnation,
                    defer_for,
                    advance_cursor_by,
                    schedule,
                } => {
                    if txn
                        .get(&ReplicatorKey::new(peer_id.as_str()).bytes())
                        .await?
                        .is_none()
                    {
                        return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
                    }
                    if let Some(expected) = expected_incarnation {
                        let incarnation_key = ReplicatorIncarnationKey::new(peer_id.as_str());
                        let current = txn
                            .get(&incarnation_key.bytes())
                            .await?
                            .map(|bytes| decode_retry_marker_generation(&bytes))
                            .unwrap_or(Some(0));
                        if current != Some(*expected) {
                            return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
                        }
                    }
                    let key = ReplicatorRetryIDKey::new(peer_id.as_str()).bytes();
                    let Some(bytes) = txn.get(&key).await? else {
                        return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
                    };
                    let mut info = super::RetryInfo::from_bytes(&bytes)
                        .map_err(crate::corekv::Error::Other)?;
                    info.advance_dispatch_cursor(*advance_cursor_by);
                    if let Some(delay) = defer_for {
                        info.defer_for(*delay);
                    } else {
                        info.bump_with_schedule(peer_id, schedule);
                    }
                    txn.set(&key, &info.to_bytes().map_err(crate::corekv::Error::Other)?)
                        .await?;
                    Ok(Box::new(true) as crate::corekv::RetryAttemptValue)
                }
                Attempt::Restart {
                    peer_id,
                    expected_incarnation,
                    advance_cursor_by,
                    schedule,
                } => {
                    if txn
                        .get(&ReplicatorKey::new(peer_id.as_str()).bytes())
                        .await?
                        .is_none()
                    {
                        return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
                    }
                    if let Some(expected) = expected_incarnation {
                        let incarnation_key = ReplicatorIncarnationKey::new(peer_id.as_str());
                        let current = txn
                            .get(&incarnation_key.bytes())
                            .await?
                            .map(|bytes| decode_retry_marker_generation(&bytes))
                            .unwrap_or(Some(0));
                        if current != Some(*expected) {
                            return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
                        }
                    }
                    let key = ReplicatorRetryIDKey::new(peer_id.as_str()).bytes();
                    let Some(bytes) = txn.get(&key).await? else {
                        return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
                    };
                    let mut info = super::RetryInfo::from_bytes(&bytes)
                        .map_err(crate::corekv::Error::Other)?;
                    info.advance_dispatch_cursor(*advance_cursor_by);
                    info.num_retries = 0;
                    info.bump_with_schedule(peer_id, schedule);
                    txn.set(&key, &info.to_bytes().map_err(crate::corekv::Error::Other)?)
                        .await?;
                    Ok(Box::new(true) as crate::corekv::RetryAttemptValue)
                }
                Attempt::RecordHint { peer_id, delay } => {
                    if txn
                        .get(&ReplicatorKey::new(peer_id.as_str()).bytes())
                        .await?
                        .is_none()
                    {
                        return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
                    }
                    let key = ReplicatorRetryIDKey::new(peer_id.as_str()).bytes();
                    let mut info = match txn.get(&key).await? {
                        Some(bytes) => super::RetryInfo::from_bytes(&bytes)
                            .map_err(crate::corekv::Error::Other)?,
                        None => super::RetryInfo::new_initial(),
                    };
                    info.defer_for_hint(*delay);
                    txn.set(&key, &info.to_bytes().map_err(crate::corekv::Error::Other)?)
                        .await?;
                    Ok(Box::new(true) as crate::corekv::RetryAttemptValue)
                }
                Attempt::Activate { peer_id } => {
                    if txn
                        .get(&ReplicatorKey::new(peer_id.as_str()).bytes())
                        .await?
                        .is_none()
                    {
                        return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
                    }
                    let key = ReplicatorRetryIDKey::new(peer_id.as_str()).bytes();
                    let Some(bytes) = txn.get(&key).await? else {
                        return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
                    };
                    let mut info = super::RetryInfo::from_bytes(&bytes)
                        .map_err(crate::corekv::Error::Other)?;
                    info.defer_for(std::time::Duration::ZERO);
                    txn.set(&key, &info.to_bytes().map_err(crate::corekv::Error::Other)?)
                        .await?;
                    Ok(Box::new(true) as crate::corekv::RetryAttemptValue)
                }
                Attempt::CompleteMarker {
                    peer_id,
                    key,
                    expected_generation,
                    acked_key,
                    acked,
                } => {
                    let mut io = txn.begin_attempt_io()?;
                    if io
                        .get(&ReplicatorKey::new(peer_id.as_str()).bytes())
                        .await?
                        .is_none()
                    {
                        txn.finish_attempt_io(io, crate::corekv::TxnCallbacks::default())?;
                        return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
                    }
                    if let Some(expected_generation) = *expected_generation {
                        let Some(bytes) = io.get(key).await? else {
                            txn.finish_attempt_io(io, crate::corekv::TxnCallbacks::default())?;
                            return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
                        };
                        let current = decode_retry_marker_generation(&bytes).unwrap_or(0);
                        if current != expected_generation {
                            txn.finish_attempt_io(io, crate::corekv::TxnCallbacks::default())?;
                            return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
                        }
                        io.delete(key).await?;
                    }
                    if acked.is_empty() {
                        io.delete(acked_key).await?;
                    } else {
                        io.set(acked_key, acked).await?;
                    }

                    // Completion and empty-schedule cleanup share this same
                    // transaction. A separate clear after ACK could otherwise
                    // run against a re-added peer after its new marker lands.
                    let mut has_markers = false;
                    let mut empty_legacy_keys = Vec::new();
                    for (prefix, expected_prefix) in [
                        (
                            ReplicatorRetryDocIDKey::peer_prefix(peer_id.as_str()),
                            format!("/rep/retry/doc/{peer_id}/"),
                        ),
                        (
                            ReplicatorRetryCollectionKey::peer_prefix(peer_id.as_str()),
                            format!("/rep/retry/col/{peer_id}/"),
                        ),
                    ] {
                        let mut iter = io.iterator(IterOptions::new().with_prefix(prefix)).await?;
                        while let Some(pair) = iter.next().await? {
                            let key = String::from_utf8_lossy(&pair.key);
                            if key
                                .strip_prefix(&expected_prefix)
                                .is_some_and(str::is_empty)
                            {
                                empty_legacy_keys.push(pair.key);
                            } else {
                                has_markers = true;
                                break;
                            }
                        }
                        iter.close().await?;
                        if has_markers {
                            break;
                        }
                    }
                    if !has_markers {
                        for empty_key in empty_legacy_keys {
                            io.delete(&empty_key).await?;
                        }
                        let retry_info_key = ReplicatorRetryIDKey::new(peer_id.as_str());
                        if let Some(bytes) = io.get(&retry_info_key.bytes()).await? {
                            let retry_info = super::RetryInfo::from_bytes(&bytes)
                                .map_err(crate::corekv::Error::Other)?;
                            if retry_info.is_backpressure_elapsed() {
                                io.delete(&retry_info_key.bytes()).await?;
                            }
                        }
                    }
                    txn.finish_attempt_io(io, crate::corekv::TxnCallbacks::default())?;
                    Ok(Box::new(true) as crate::corekv::RetryAttemptValue)
                }
                Attempt::ClearPeer {
                    peer_id,
                    expected_incarnation,
                } => {
                    let mut io = txn.begin_attempt_io()?;
                    let exists = io
                        .get(&ReplicatorKey::new(peer_id.as_str()).bytes())
                        .await?
                        .is_some();
                    if let Some(expected) = expected_incarnation {
                        let incarnation_key = ReplicatorIncarnationKey::new(peer_id.as_str());
                        let current = io
                            .get(&incarnation_key.bytes())
                            .await?
                            .map(|bytes| decode_retry_marker_generation(&bytes))
                            .unwrap_or(Some(0));
                        if !exists || current != Some(*expected) {
                            txn.finish_attempt_io(io, crate::corekv::TxnCallbacks::default())?;
                            return Ok(Box::new(()) as crate::corekv::RetryAttemptValue);
                        }
                    }
                    let mut has_markers = false;
                    let mut empty_legacy_keys = Vec::new();
                    if exists {
                        for (prefix, expected_prefix) in [
                            (
                                ReplicatorRetryDocIDKey::peer_prefix(peer_id.as_str()),
                                format!("/rep/retry/doc/{peer_id}/"),
                            ),
                            (
                                ReplicatorRetryCollectionKey::peer_prefix(peer_id.as_str()),
                                format!("/rep/retry/col/{peer_id}/"),
                            ),
                        ] {
                            let mut iter =
                                io.iterator(IterOptions::new().with_prefix(prefix)).await?;
                            while let Some(pair) = iter.next().await? {
                                let key = String::from_utf8_lossy(&pair.key);
                                if key
                                    .strip_prefix(&expected_prefix)
                                    .is_some_and(str::is_empty)
                                {
                                    empty_legacy_keys.push(pair.key);
                                } else {
                                    has_markers = true;
                                    break;
                                }
                            }
                            iter.close().await?;
                            if has_markers {
                                break;
                            }
                        }
                        if !has_markers {
                            for key in empty_legacy_keys {
                                io.delete(&key).await?;
                            }
                            let key = ReplicatorRetryIDKey::new(peer_id.as_str()).bytes();
                            let keep_deadline = match io.get(&key).await? {
                                Some(bytes) => !super::RetryInfo::from_bytes(&bytes)
                                    .map_err(crate::corekv::Error::Other)?
                                    .is_backpressure_elapsed(),
                                None => false,
                            };
                            if !keep_deadline {
                                io.delete(&key).await?;
                            }
                        }
                    }
                    txn.finish_attempt_io(io, crate::corekv::TxnCallbacks::default())?;
                    Ok(Box::new(()) as crate::corekv::RetryAttemptValue)
                }
            }
        })
    }
}

impl RetryAttemptFn for RegisterScopeMarkerAttempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            if txn
                .get(&ReplicatorKey::new(&self.peer_id).bytes())
                .await?
                .is_none()
            {
                return Ok(Box::new(None::<u64>) as crate::corekv::RetryAttemptValue);
            }
            let incarnation_key = ReplicatorIncarnationKey::new(&self.peer_id);
            let incarnation = txn.get(&incarnation_key.bytes()).await?;
            let incarnation = match incarnation {
                Some(bytes) => decode_retry_marker_generation(&bytes),
                None => Some(0),
            };
            if incarnation != Some(self.expected_incarnation) {
                return Ok(Box::new(None::<u64>) as crate::corekv::RetryAttemptValue);
            }
            let marker = if self.doc_id.is_empty() && self.collection_id.is_empty() {
                None
            } else if self.doc_id.is_empty() {
                Some(ReplicatorRetryCollectionKey::new(&self.peer_id, &self.collection_id).bytes())
            } else {
                Some(ReplicatorRetryDocIDKey::new(&self.peer_id, &self.doc_id).bytes())
            };
            if let Some(marker) = &marker {
                if txn
                    .get(marker)
                    .await?
                    .as_deref()
                    .and_then(decode_retry_marker_generation)
                    .is_some_and(|current| current > self.generation)
                {
                    // Allocations can be committed out of order when concurrent
                    // registrations conflict and retry. Never let an older
                    // announcement replace a newer durable scope generation.
                    return Ok(Box::new(None::<u64>) as crate::corekv::RetryAttemptValue);
                }
            }
            let id_key = ReplicatorRetryIDKey::new(&self.peer_id);
            let requested = super::RetryInfo::from_bytes(&self.retry_info_bytes)
                .unwrap_or_else(|_| super::RetryInfo::new_initial());
            let existing = txn.get(&id_key.bytes()).await?;
            if existing.is_none() || requested.not_before_unix > 0 {
                let mut info = match existing {
                    Some(bytes) => {
                        super::RetryInfo::from_bytes(&bytes).map_err(crate::corekv::Error::Other)?
                    }
                    None => requested.clone(),
                };
                if requested.not_before_unix > 0 {
                    if requested.not_before_unix > info.not_before_unix {
                        info.not_before_unix = requested.not_before_unix;
                        info.deferred_at_unix = requested.deferred_at_unix;
                    }
                    info.next_retry_unix = info.not_before_unix;
                } else {
                    info.bump_with_schedule(&self.peer_id, &self.retry_schedule);
                }
                txn.set(
                    &id_key.bytes(),
                    &info.to_bytes().map_err(crate::corekv::Error::Other)?,
                )
                .await?;
            }

            let Some(marker) = marker else {
                return Ok(Box::new(None::<u64>) as crate::corekv::RetryAttemptValue);
            };
            txn.set(&marker, &encode_retry_marker_generation(self.generation))
                .await?;
            Ok(Box::new(Some(self.generation)) as crate::corekv::RetryAttemptValue)
        })
    }
}

impl RetryAttemptFn for RefreshPushFailureAttempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            if txn
                .get(&ReplicatorKey::new(&self.peer_id).bytes())
                .await?
                .is_none()
            {
                return Ok(Box::new(None::<u64>) as crate::corekv::RetryAttemptValue);
            }

            let marker = if self.doc_id.is_empty() {
                ReplicatorRetryCollectionKey::new(&self.peer_id, &self.collection_id).bytes()
            } else {
                ReplicatorRetryDocIDKey::new(&self.peer_id, &self.doc_id).bytes()
            };
            let Some(marker_bytes) = txn.get(&marker).await? else {
                return Ok(Box::new(None::<u64>) as crate::corekv::RetryAttemptValue);
            };
            if decode_retry_marker_generation(&marker_bytes) != Some(self.expected_generation) {
                return Ok(Box::new(None::<u64>) as crate::corekv::RetryAttemptValue);
            }

            let retry_info_key = ReplicatorRetryIDKey::new(&self.peer_id);
            let Some(existing_bytes) = txn.get(&retry_info_key.bytes()).await? else {
                return Ok(Box::new(None::<u64>) as crate::corekv::RetryAttemptValue);
            };
            let mut info = super::RetryInfo::from_bytes(&existing_bytes)
                .map_err(crate::corekv::Error::Other)?;
            let requested = super::RetryInfo::from_bytes(&self.retry_info_bytes)
                .map_err(crate::corekv::Error::Other)?;
            if requested.not_before_unix > info.not_before_unix {
                info.not_before_unix = requested.not_before_unix;
                info.deferred_at_unix = requested.deferred_at_unix;
                info.next_retry_unix = info.not_before_unix;
                txn.set(
                    &retry_info_key.bytes(),
                    &info.to_bytes().map_err(crate::corekv::Error::Other)?,
                )
                .await?;
            }

            txn.set(
                &marker,
                &encode_retry_marker_generation(self.new_generation),
            )
            .await?;
            Ok(Box::new(Some(self.new_generation)) as crate::corekv::RetryAttemptValue)
        })
    }
}

impl RetryAttemptFn for SetReplicatorIfMarkerGenerationAttempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let replicator_key = ReplicatorKey::new(&self.peer_id).bytes();
            let Some(current_replicator) = txn.get(&replicator_key).await? else {
                return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
            };
            if current_replicator.as_ref() != self.expected_replicator {
                return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
            }

            let marker = if self.doc_id.is_empty() {
                ReplicatorRetryCollectionKey::new(&self.peer_id, &self.collection_id).bytes()
            } else {
                ReplicatorRetryDocIDKey::new(&self.peer_id, &self.doc_id).bytes()
            };
            let Some(marker_bytes) = txn.get(&marker).await? else {
                return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
            };
            if decode_retry_marker_generation(&marker_bytes) != Some(self.expected_generation) {
                return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
            }

            txn.set(&replicator_key, &self.updated_replicator).await?;
            Ok(Box::new(true) as crate::corekv::RetryAttemptValue)
        })
    }
}

impl RetryAttemptFn for SetReplicatorIfIncarnationAttempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let mut io = txn.begin_attempt_io()?;
            let replicator_key = ReplicatorKey::new(&self.peer_id).bytes();
            let Some(current_replicator) = io.get(&replicator_key).await? else {
                txn.finish_attempt_io(io, crate::corekv::TxnCallbacks::default())?;
                return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
            };
            if current_replicator.as_ref() != self.expected_replicator {
                txn.finish_attempt_io(io, crate::corekv::TxnCallbacks::default())?;
                return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
            }

            let incarnation_key = ReplicatorIncarnationKey::new(&self.peer_id);
            let current_incarnation = io
                .get(&incarnation_key.bytes())
                .await?
                .map(|bytes| decode_retry_marker_generation(&bytes))
                .unwrap_or(Some(0));
            if current_incarnation != Some(self.expected_incarnation) {
                txn.finish_attempt_io(io, crate::corekv::TxnCallbacks::default())?;
                return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
            }

            if self.require_no_markers {
                for prefix in [
                    ReplicatorRetryDocIDKey::peer_prefix(&self.peer_id),
                    ReplicatorRetryCollectionKey::peer_prefix(&self.peer_id),
                ] {
                    let mut iter = io.iterator(IterOptions::new().with_prefix(prefix)).await?;
                    let has_marker = iter.next().await?.is_some();
                    iter.close().await?;
                    if has_marker {
                        txn.finish_attempt_io(io, crate::corekv::TxnCallbacks::default())?;
                        return Ok(Box::new(false) as crate::corekv::RetryAttemptValue);
                    }
                }
            }

            io.set(&replicator_key, &self.updated_replicator).await?;
            txn.finish_attempt_io(io, crate::corekv::TxnCallbacks::default())?;
            Ok(Box::new(true) as crate::corekv::RetryAttemptValue)
        })
    }
}

impl<S: Store> Peerstore<S> {
    /// Every peerstore transaction runs here: one engine transaction, its
    /// re-runs counted in the push-marker retry layer.
    async fn transact_conflicts<T>(&self, attempt: crate::corekv::RetryAttempt) -> Result<T>
    where
        T: Send + 'static,
    {
        crate::corekv::transact(
            &self.store,
            self.retry,
            Some(telemetry::RetryLayer::PushMarker),
            attempt,
        )
        .await
        .map_err(crate::corekv::RetryAttemptError::into_storage)
    }

    /// Create a new Peerstore
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store: NamespacedStore::new(store, Namespace::Peerstore),
            retry_schedule: super::RetrySchedule::default(),
            retry: crate::corekv::engine_retry_options(crate::corekv::DEFAULT_MAX_TXN_RETRIES),
            pool: None,
        }
    }

    /// Re-run a conflicting transaction up to `max_txn_retries` times, the
    /// node's `datastore.max_txn_retries`.
    pub fn with_max_txn_retries(mut self, max_txn_retries: u32) -> Self {
        self.retry = crate::corekv::engine_retry_options(max_txn_retries);
        self
    }

    /// Set the runtime retry policy without rewriting persisted deadlines.
    pub fn with_retry_schedule(mut self, schedule: super::RetrySchedule) -> Self {
        self.retry_schedule = schedule;
        self
    }

    /// Store a replicator's configuration.
    ///
    /// The peer_id is used as the key, and the data is stored as raw bytes.
    /// The caller is responsible for serialization (typically CBOR).
    pub async fn create_replicator(&self, peer_id: &str, data: &[u8]) -> Result<()> {
        let key = ReplicatorKey::new(peer_id);
        let incarnation = self
            .store
            .allocate(b"/rep/incarnation/sequence", 1)
            .await?
            .end;
        let incarnation_key = ReplicatorIncarnationKey::new(peer_id);
        let mut txn = self.store.new_txn(false).await?;
        let previous_incarnation = txn.get(&incarnation_key.bytes()).await?;
        let previous_incarnation = match previous_incarnation {
            Some(bytes) => decode_retry_marker_generation(&bytes)
                .ok_or_else(|| crate::corekv::Error::Other("invalid peer incarnation".into()))?,
            None => 0,
        };
        if previous_incarnation > incarnation {
            return Err(crate::corekv::Error::Other(
                "stale peer incarnation allocation".into(),
            ));
        }
        txn.set(&key.bytes(), data).await?;
        txn.set(
            &incarnation_key.bytes(),
            &encode_retry_marker_generation(incarnation),
        )
        .await?;
        txn.commit().await
    }

    /// Get a replicator's configuration by peer ID.
    ///
    /// Returns None if the replicator doesn't exist.
    pub async fn get_replicator(&self, peer_id: &str) -> Result<Option<Bytes>> {
        let key = ReplicatorKey::new(peer_id);
        let txn = self.store.new_txn(true).await?;
        txn.get(&key.bytes()).await
    }

    /// Delete a replicator and all of its persisted push-retry state.
    pub async fn delete_replicator(&self, peer_id: &str) -> Result<()> {
        self.transact_conflicts(Box::new(PeerstoreRetryAttempt::DeleteReplicator {
            peer_id: peer_id.to_owned(),
        }))
        .await
    }

    /// Get all stored replicator configurations.
    ///
    /// Returns a list of (peer_id, data) pairs. Keys that don't match the
    /// expected format are logged and skipped.
    pub async fn list_replicators(&self) -> Result<Vec<(String, Bytes)>> {
        let prefix = ReplicatorKey::replicator_prefix();
        let txn = self.store.new_txn(true).await?;
        let opts = IterOptions::new().with_prefix(prefix);
        let mut iter = txn.iterator(opts).await?;

        let mut results = Vec::new();
        while let Some(pair) = iter.next().await? {
            // Parse the key using ReplicatorKey::from_bytes for safe extraction
            if let Some(key) = ReplicatorKey::from_bytes(&pair.key) {
                results.push((key.replicator_id().to_string(), pair.value));
            } else {
                let key_str = String::from_utf8_lossy(&pair.key);
                tracing::warn!(
                    key = %key_str,
                    "Skipping replicator entry with unexpected key format"
                );
            }
        }

        Ok(results)
    }

    /// Check if a replicator exists.
    pub async fn has_replicator(&self, peer_id: &str) -> Result<bool> {
        let key = ReplicatorKey::new(peer_id);
        let txn = self.store.new_txn(true).await?;
        txn.has(&key.bytes()).await
    }

    /// Store the node's Ed25519 peer key seed, shared by every transport.
    pub async fn set_local_peer_key(&self, seed: &[u8]) -> Result<()> {
        let mut txn = self.store.new_txn(false).await?;
        txn.set(b"/p2p/local-peer-key", seed).await?;
        txn.commit().await
    }

    /// Load the node's Ed25519 peer key seed.
    pub async fn get_local_peer_key(&self) -> Result<Option<Bytes>> {
        let txn = self.store.new_txn(true).await?;
        txn.get(b"/p2p/local-peer-key").await
    }

    /// Public identity bound to an application-owned peer keyring.
    pub async fn get_local_peer_keyring_identity(&self) -> Result<Option<Bytes>> {
        let txn = self.store.new_txn(true).await?;
        txn.get(b"/p2p/local-peer-keyring-identity").await
    }

    /// Record keyring mode without storing any private key material.
    pub async fn set_local_peer_keyring_identity(&self, public_key: &[u8]) -> Result<()> {
        let mut txn = self.store.new_txn(false).await?;
        txn.set(b"/p2p/local-peer-keyring-identity", public_key)
            .await?;
        txn.commit().await
    }

    /// Store P2P collection subscriptions (persists across restarts).
    pub async fn set_p2p_collections(&self, data: &[u8]) -> Result<()> {
        let mut txn = self.store.new_txn(false).await?;
        txn.set(b"/p2p/collections", data).await?;
        txn.commit().await
    }

    /// Load stored P2P collection subscriptions.
    pub async fn get_p2p_collections(&self) -> Result<Option<Bytes>> {
        let txn = self.store.new_txn(true).await?;
        txn.get(b"/p2p/collections").await
    }

    /// Store P2P document subscriptions (persists across restarts).
    pub async fn set_p2p_documents(&self, data: &[u8]) -> Result<()> {
        let mut txn = self.store.new_txn(false).await?;
        txn.set(b"/p2p/documents", data).await?;
        txn.commit().await
    }

    /// Load stored P2P document subscriptions.
    pub async fn get_p2p_documents(&self) -> Result<Option<Bytes>> {
        let txn = self.store.new_txn(true).await?;
        txn.get(b"/p2p/documents").await
    }

    /// Persist a list of P2P collection subscriptions as JSON.
    pub async fn persist_collections(&self, collections: &[String]) -> Result<()> {
        let data = serde_json::to_vec(collections).map_err(|e| {
            crate::corekv::Error::Other(format!("failed to serialize P2P collections: {}", e))
        })?;
        self.set_p2p_collections(&data).await
    }

    /// Load the persisted P2P collection subscription list.
    pub async fn load_collections(&self) -> Result<Vec<String>> {
        match self.get_p2p_collections().await? {
            Some(data) => serde_json::from_slice(&data).map_err(|e| {
                crate::corekv::Error::Other(format!("failed to deserialize P2P collections: {}", e))
            }),
            None => Ok(Vec::new()),
        }
    }

    /// Persist a list of P2P document subscriptions as JSON.
    pub async fn persist_documents(&self, documents: &[String]) -> Result<()> {
        let data = serde_json::to_vec(documents).map_err(|e| {
            crate::corekv::Error::Other(format!("failed to serialize P2P documents: {}", e))
        })?;
        self.set_p2p_documents(&data).await
    }

    /// Load the persisted P2P document subscription list.
    pub async fn load_documents(&self) -> Result<Vec<String>> {
        match self.get_p2p_documents().await? {
            Some(data) => serde_json::from_slice(&data).map_err(|e| {
                crate::corekv::Error::Other(format!("failed to deserialize P2P documents: {}", e))
            }),
            None => Ok(Vec::new()),
        }
    }

    async fn register_scope_marker(
        &self,
        peer_id: &str,
        doc_id: &str,
        collection_id: &str,
        retry_info_bytes: &[u8],
    ) -> Result<Option<u64>> {
        let incarnation_key = ReplicatorIncarnationKey::new(peer_id);
        let peer_key = ReplicatorKey::new(peer_id);
        let txn = self.store.new_txn(true).await?;
        if txn.get(&peer_key.bytes()).await?.is_none() {
            return Ok(None);
        }
        let expected_incarnation = match txn.get(&incarnation_key.bytes()).await? {
            Some(bytes) => decode_retry_marker_generation(&bytes)
                .ok_or_else(|| crate::corekv::Error::Other("invalid peer incarnation".into()))?,
            None => 0,
        };
        drop(txn);

        self.register_scope_marker_once(
            peer_id,
            doc_id,
            collection_id,
            retry_info_bytes,
            expected_incarnation,
        )
        .await
    }

    async fn register_scope_marker_once(
        &self,
        peer_id: &str,
        doc_id: &str,
        collection_id: &str,
        retry_info_bytes: &[u8],
        expected_incarnation: u64,
    ) -> Result<Option<u64>> {
        let peer_id = peer_id.to_owned();
        let doc_id = doc_id.to_owned();
        let collection_id = collection_id.to_owned();
        let retry_info_bytes = retry_info_bytes.to_vec();
        let retry_schedule = self.retry_schedule.clone();
        let generation = if doc_id.is_empty() && collection_id.is_empty() {
            0
        } else {
            self.store
                .allocate(b"/rep/retry/marker-generation", 1)
                .await?
                .end
        };
        self.transact_conflicts(Box::new(RegisterScopeMarkerAttempt {
            peer_id,
            expected_incarnation,
            doc_id,
            collection_id,
            retry_info_bytes,
            retry_schedule,
            generation,
        }))
        .await
    }

    /// Record a push failure for a specific peer/doc pair.
    ///
    /// Writes the peer schedule and advances the document or collection marker generation.
    pub async fn record_push_failure(
        &self,
        peer_id: &str,
        doc_id: &str,
        collection_id: &str,
        retry_info_bytes: &[u8],
    ) -> Result<()> {
        if doc_id.is_empty() && collection_id.is_empty() {
            return Ok(());
        }
        self.register_scope_marker(peer_id, doc_id, collection_id, retry_info_bytes)
            .await
            .map(|_| ())
    }

    /// Register a push failure and return the generation written for its scope.
    pub async fn record_push_failure_with_generation(
        &self,
        peer_id: &str,
        doc_id: &str,
        collection_id: &str,
        retry_info_bytes: &[u8],
    ) -> Result<Option<u64>> {
        if doc_id.is_empty() && collection_id.is_empty() {
            return Ok(None);
        }
        self.register_scope_marker(peer_id, doc_id, collection_id, retry_info_bytes)
            .await
    }

    /// Refresh an admitted push failure only while it still owns the exact
    /// marker generation it was admitted with. A late failure cannot recreate
    /// a forgotten marker or overwrite a newer head's retry obligation.
    pub async fn refresh_push_failure_with_generation(
        &self,
        peer_id: &str,
        doc_id: &str,
        collection_id: &str,
        expected_generation: u64,
        retry_info_bytes: &[u8],
    ) -> Result<Option<u64>> {
        if doc_id.is_empty() && collection_id.is_empty() {
            return Ok(None);
        }
        let peer_id = peer_id.to_owned();
        let doc_id = doc_id.to_owned();
        let collection_id = collection_id.to_owned();
        let retry_info_bytes = retry_info_bytes.to_vec();
        let new_generation = self
            .store
            .allocate(b"/rep/retry/marker-generation", 1)
            .await?
            .end;
        self.transact_conflicts(Box::new(RefreshPushFailureAttempt {
            peer_id,
            doc_id,
            collection_id,
            retry_info_bytes,
            expected_generation,
            new_generation,
        }))
        .await
    }

    /// Replace replicator bytes only while both the peer value and its exact
    /// scope marker generation remain unchanged. This fences follow-up status
    /// writes from a completed push against newer heads and peer recreation.
    pub async fn set_replicator_if_marker_generation(
        &self,
        peer_id: &str,
        doc_id: &str,
        collection_id: &str,
        expected_generation: u64,
        expected_replicator: &[u8],
        updated_replicator: &[u8],
    ) -> Result<bool> {
        self.transact_conflicts(Box::new(SetReplicatorIfMarkerGenerationAttempt {
            peer_id: peer_id.to_owned(),
            doc_id: doc_id.to_owned(),
            collection_id: collection_id.to_owned(),
            expected_generation,
            expected_replicator: expected_replicator.to_vec(),
            updated_replicator: updated_replicator.to_vec(),
        }))
        .await
    }

    /// Replace replicator bytes only while the peer incarnation and current
    /// bytes still match the values observed by the caller.
    pub async fn set_replicator_if_incarnation(
        &self,
        peer_id: &str,
        expected_incarnation: u64,
        expected_replicator: &[u8],
        updated_replicator: &[u8],
    ) -> Result<bool> {
        self.set_replicator_if_incarnation_with_marker_state(
            peer_id,
            expected_incarnation,
            expected_replicator,
            updated_replicator,
            false,
        )
        .await
    }

    /// Replace replicator bytes only while the incarnation and expected bytes
    /// match, optionally requiring that no retry markers remain.
    pub async fn set_replicator_if_incarnation_with_marker_state(
        &self,
        peer_id: &str,
        expected_incarnation: u64,
        expected_replicator: &[u8],
        updated_replicator: &[u8],
        require_no_markers: bool,
    ) -> Result<bool> {
        self.transact_conflicts(Box::new(SetReplicatorIfIncarnationAttempt {
            peer_id: peer_id.to_owned(),
            expected_incarnation,
            expected_replicator: expected_replicator.to_vec(),
            updated_replicator: updated_replicator.to_vec(),
            require_no_markers,
        }))
        .await
    }

    /// Register the dirty scope before a live head announcement.
    pub async fn observe_push_head(
        &self,
        peer_id: &str,
        doc_id: &str,
        collection_id: &str,
    ) -> Result<()> {
        if doc_id.is_empty() && collection_id.is_empty() {
            return Ok(());
        }
        let initial = super::RetryInfo::new_initial()
            .to_bytes()
            .map_err(crate::corekv::Error::Other)?;
        self.register_scope_marker(peer_id, doc_id, collection_id, &initial)
            .await
            .map(|_| ())
    }

    /// Register a live head and return the new scope generation.
    pub async fn observe_push_head_with_generation(
        &self,
        peer_id: &str,
        doc_id: &str,
        collection_id: &str,
    ) -> Result<Option<u64>> {
        if doc_id.is_empty() && collection_id.is_empty() {
            return Ok(None);
        }
        let initial = super::RetryInfo::new_initial()
            .to_bytes()
            .map_err(crate::corekv::Error::Other)?;
        self.register_scope_marker(peer_id, doc_id, collection_id, &initial)
            .await
    }

    async fn load_scope_markers(&self, peer_id: &str) -> Result<Vec<super::PushRetryMarker>> {
        let retry_info = self
            .get_retry_info(peer_id)
            .await?
            .and_then(|bytes| super::RetryInfo::from_bytes(&bytes).ok())
            .unwrap_or_else(super::RetryInfo::new_initial);
        let txn = self.store.new_txn(true).await?;
        let mut result = Vec::new();

        let doc_prefix = ReplicatorRetryDocIDKey::peer_prefix(peer_id);
        let expected_doc = format!("/rep/retry/doc/{peer_id}/");
        let mut docs = txn
            .iterator(IterOptions::new().with_prefix(doc_prefix))
            .await?;
        while let Some(pair) = docs.next().await? {
            let key = String::from_utf8_lossy(&pair.key);
            let Some(doc_id) = key.strip_prefix(&expected_doc) else {
                continue;
            };
            if !doc_id.is_empty() {
                result.push(super::PushRetryMarker {
                    doc_id: doc_id.to_string(),
                    collection_id: String::new(),
                    scope: super::RetryScope::Document,
                    retry_info: retry_info.clone(),
                    generation: decode_retry_marker_generation(&pair.value).unwrap_or(0),
                });
            }
        }
        drop(docs);

        let col_prefix = ReplicatorRetryCollectionKey::peer_prefix(peer_id);
        let expected_col = format!("/rep/retry/col/{peer_id}/");
        let mut cols = txn
            .iterator(IterOptions::new().with_prefix(col_prefix))
            .await?;
        while let Some(pair) = cols.next().await? {
            let key = String::from_utf8_lossy(&pair.key);
            let Some(collection_id) = key.strip_prefix(&expected_col) else {
                continue;
            };
            if !collection_id.is_empty() {
                result.push(super::PushRetryMarker {
                    doc_id: String::new(),
                    collection_id: collection_id.to_string(),
                    scope: super::RetryScope::CollectionCommit,
                    retry_info: retry_info.clone(),
                    generation: decode_retry_marker_generation(&pair.value).unwrap_or(0),
                });
            }
        }
        result.sort_by(|a, b| {
            a.scope
                .cmp(&b.scope)
                .then_with(|| a.doc_id.cmp(&b.doc_id))
                .then_with(|| a.collection_id.cmp(&b.collection_id))
        });
        if !result.is_empty() {
            let offset = retry_info.dispatch_cursor as usize % result.len();
            result.rotate_left(offset);
        }
        Ok(result)
    }

    /// Load document and collection scopes on the peer clock.
    pub async fn get_retry_documents(&self, peer_id: &str) -> Result<Vec<super::PushRetryMarker>> {
        self.load_scope_markers(peer_id).await
    }

    /// Count durable scope markers and report the oldest shared peer schedule.
    pub async fn push_retry_marker_stats(&self) -> Result<super::PushRetryMarkerStats> {
        let txn = self.store.new_txn(true).await?;
        let mut stats = super::PushRetryMarkerStats::default();

        let mut docs = txn
            .iterator(IterOptions::new().with_prefix(ReplicatorRetryDocIDKey::retry_doc_prefix()))
            .await?;
        while let Some(pair) = docs.next().await? {
            if !pair.key.ends_with(b"/") {
                stats.document_markers += 1;
            }
        }
        drop(docs);

        let mut collections = txn
            .iterator(
                IterOptions::new()
                    .with_prefix(ReplicatorRetryCollectionKey::retry_collection_prefix()),
            )
            .await?;
        while let Some(pair) = collections.next().await? {
            if !pair.key.ends_with(b"/") {
                stats.collection_markers += 1;
            }
        }
        drop(collections);

        let mut schedules = txn
            .iterator(IterOptions::new().with_prefix(ReplicatorRetryIDKey::retry_prefix()))
            .await?;
        while let Some(pair) = schedules.next().await? {
            let Ok(info) = super::RetryInfo::from_bytes(&pair.value) else {
                continue;
            };
            stats.scheduled_peers += 1;
            stats.oldest_scheduled_retry_unix = Some(
                stats
                    .oldest_scheduled_retry_unix
                    .map_or(info.next_retry_unix, |oldest| {
                        oldest.min(info.next_retry_unix)
                    }),
            );
        }
        Ok(stats)
    }

    /// Advance the current peer schedule after an attempted replay.
    ///
    /// The transaction validates the replicator key and reads the current
    /// schedule, so forget and concurrent schedule updates serialize through
    /// ordinary transaction conflicts.
    ///
    /// `advance_cursor_by` is the number of markers the finished pass consumed:
    /// a bounded pass must resume past them, or the tail of a large marker set
    /// is never reached.
    pub async fn reschedule_retry_peer(
        &self,
        peer_id: &str,
        defer_for: Option<std::time::Duration>,
        advance_cursor_by: u64,
    ) -> Result<bool> {
        self.transact_conflicts(Box::new(PeerstoreRetryAttempt::Reschedule {
            peer_id: peer_id.to_owned(),
            expected_incarnation: None,
            defer_for,
            advance_cursor_by,
            schedule: self.retry_schedule.clone(),
        }))
        .await
    }

    /// Reschedule a peer only if it is still the incarnation observed when the
    /// retry pass began.
    pub async fn reschedule_retry_peer_for_incarnation(
        &self,
        peer_id: &str,
        expected_incarnation: u64,
        defer_for: Option<std::time::Duration>,
        advance_cursor_by: u64,
    ) -> Result<bool> {
        self.transact_conflicts(Box::new(PeerstoreRetryAttempt::Reschedule {
            peer_id: peer_id.to_owned(),
            expected_incarnation: Some(expected_incarnation),
            defer_for,
            advance_cursor_by,
            schedule: self.retry_schedule.clone(),
        }))
        .await
    }

    /// Return a peer to the first ladder rung after a pass that delivered
    /// documents. Escalation is evidence of an unreachable peer; a peer that
    /// is taking documents has refuted it.
    pub async fn restart_retry_peer(&self, peer_id: &str, advance_cursor_by: u64) -> Result<bool> {
        self.transact_conflicts(Box::new(PeerstoreRetryAttempt::Restart {
            peer_id: peer_id.to_owned(),
            expected_incarnation: None,
            advance_cursor_by,
            schedule: self.retry_schedule.clone(),
        }))
        .await
    }

    /// Restart the retry ladder only if it is still the incarnation observed
    /// when the retry pass began.
    pub async fn restart_retry_peer_for_incarnation(
        &self,
        peer_id: &str,
        expected_incarnation: u64,
        advance_cursor_by: u64,
    ) -> Result<bool> {
        self.transact_conflicts(Box::new(PeerstoreRetryAttempt::Restart {
            peer_id: peer_id.to_owned(),
            expected_incarnation: Some(expected_incarnation),
            advance_cursor_by,
            schedule: self.retry_schedule.clone(),
        }))
        .await
    }

    /// Apply a receiver hint atomically, creating the schedule when necessary.
    /// Returns false if the replicator was removed.
    pub async fn record_retry_after(
        &self,
        peer_id: &str,
        delay: std::time::Duration,
    ) -> Result<bool> {
        self.transact_conflicts(Box::new(PeerstoreRetryAttempt::RecordHint {
            peer_id: peer_id.to_owned(),
            delay,
        }))
        .await
    }

    /// Make an existing peer retry schedule immediately due without changing
    /// its failure-ladder rung.  A connection-established event is new
    /// delivery evidence: retaining an old connection-failure deadline after
    /// that event can leave otherwise actionable scope markers dormant.
    pub async fn activate_retry_peer(&self, peer_id: &str) -> Result<bool> {
        self.transact_conflicts(Box::new(PeerstoreRetryAttempt::Activate {
            peer_id: peer_id.to_owned(),
        }))
        .await
    }

    /// Remove a scope marker after the caller has verified its rederived heads
    /// are still current. Runtime ack fences serialize this with live updates.
    pub async fn complete_retry_document(
        &self,
        peer_id: &str,
        retry: &super::PushRetryMarker,
    ) -> Result<bool> {
        self.complete_retry_scope(
            peer_id,
            &retry.doc_id,
            &retry.collection_id,
            retry.is_collection_commit(),
            retry.generation,
            &[],
        )
        .await
    }

    /// Remove one retry marker only if no newer registration replaced the
    /// generation observed before rederiving the scope.
    ///
    /// `acked_heads` are the verified-current heads the peer acknowledged; in
    /// the same transaction they replace the scope's acknowledged record.
    /// Empty means nothing was delivered (the document is gone or filtered
    /// out) and clears it.
    pub async fn complete_retry_scope(
        &self,
        peer_id: &str,
        doc_id: &str,
        collection_id: &str,
        is_collection: bool,
        expected_generation: u64,
        acked_heads: &[String],
    ) -> Result<bool> {
        self.complete_scope(
            peer_id,
            doc_id,
            collection_id,
            is_collection,
            Some(expected_generation),
            acked_heads,
        )
        .await
    }

    /// Record the heads `peer_id` acknowledged for a scope no retry marker
    /// tracks (a live push that registered none), in the same transaction
    /// [`Self::complete_retry_scope`] uses. `false` once the peer is gone.
    pub async fn record_acknowledged_heads(
        &self,
        peer_id: &str,
        doc_id: &str,
        collection_id: &str,
        is_collection: bool,
        acked_heads: &[String],
    ) -> Result<bool> {
        self.complete_scope(
            peer_id,
            doc_id,
            collection_id,
            is_collection,
            None,
            acked_heads,
        )
        .await
    }

    async fn complete_scope(
        &self,
        peer_id: &str,
        doc_id: &str,
        collection_id: &str,
        is_collection: bool,
        expected_generation: Option<u64>,
        acked_heads: &[String],
    ) -> Result<bool> {
        let scope_id = if is_collection { collection_id } else { doc_id };
        let key = if is_collection {
            ReplicatorRetryCollectionKey::new(peer_id, collection_id).bytes()
        } else {
            ReplicatorRetryDocIDKey::new(peer_id, doc_id).bytes()
        };
        let acked_key = ReplicatorAckedHeadsKey::new(peer_id, scope_id, is_collection).bytes();
        let mut acked = acked_heads.to_vec();
        acked.sort_unstable();
        self.transact_conflicts(Box::new(PeerstoreRetryAttempt::CompleteMarker {
            peer_id: peer_id.to_owned(),
            key,
            expected_generation,
            acked_key,
            acked: acked.join("\n").into_bytes(),
        }))
        .await
    }

    /// Return the current replicator incarnation, defaulting to zero for a
    /// pre-incarnation record. The peer existence check and token read share a
    /// snapshot.
    pub async fn get_replicator_incarnation(&self, peer_id: &str) -> Result<Option<u64>> {
        let txn = self.store.new_txn(true).await?;
        if !txn.has(&ReplicatorKey::new(peer_id).bytes()).await? {
            return Ok(None);
        }
        let key = ReplicatorIncarnationKey::new(peer_id);
        let Some(bytes) = txn.get(&key.bytes()).await? else {
            return Ok(Some(0));
        };
        decode_retry_marker_generation(&bytes)
            .map(Some)
            .ok_or_else(|| crate::corekv::Error::Other("invalid peer incarnation".into()))
    }

    /// The sorted heads `peer_id` last acknowledged for a scope, or `None`
    /// while a marker for that scope is outstanding or nothing was recorded.
    pub async fn acknowledged_heads(
        &self,
        peer_id: &str,
        scope_id: &str,
        is_collection: bool,
    ) -> Result<Option<Vec<String>>> {
        let marker = if is_collection {
            ReplicatorRetryCollectionKey::new(peer_id, scope_id).bytes()
        } else {
            ReplicatorRetryDocIDKey::new(peer_id, scope_id).bytes()
        };
        let txn = self.store.new_txn(true).await?;
        if txn.get(&marker).await?.is_some() {
            return Ok(None);
        }
        let acked = txn
            .get(&ReplicatorAckedHeadsKey::new(peer_id, scope_id, is_collection).bytes())
            .await?;
        Ok(acked.map(|bytes| {
            String::from_utf8_lossy(&bytes)
                .split('\n')
                .map(str::to_string)
                .collect()
        }))
    }

    /// Get retry info bytes for a peer.
    pub async fn get_retry_info(&self, peer_id: &str) -> Result<Option<Bytes>> {
        let txn = self.store.new_txn(true).await?;
        let key = ReplicatorRetryIDKey::new(peer_id);
        txn.get(&key.bytes()).await
    }

    /// Check whether a live push still owns the exact durable marker it was
    /// admitted with, and whether the peer's backpressure deadline has elapsed.
    ///
    /// All three facts are read from one transaction snapshot so forgetting a
    /// peer or replacing a scope marker fences queued jobs before transport I/O.
    pub async fn admit_push_scope(
        &self,
        peer_id: &str,
        doc_id: &str,
        collection_id: &str,
        expected_generation: u64,
    ) -> Result<bool> {
        let txn = self.store.new_txn(true).await?;
        if !txn.has(&ReplicatorKey::new(peer_id).bytes()).await? {
            return Ok(false);
        }

        let retry_info_key = ReplicatorRetryIDKey::new(peer_id);
        let Some(retry_info_bytes) = txn.get(&retry_info_key.bytes()).await? else {
            return Ok(false);
        };
        let Ok(retry_info) = super::RetryInfo::from_bytes(&retry_info_bytes) else {
            return Ok(false);
        };
        if !retry_info.is_backpressure_elapsed() {
            return Ok(false);
        }

        let marker_key = if doc_id.is_empty() {
            ReplicatorRetryCollectionKey::new(peer_id, collection_id).bytes()
        } else {
            ReplicatorRetryDocIDKey::new(peer_id, doc_id).bytes()
        };
        let Some(marker) = txn.get(&marker_key).await? else {
            return Ok(false);
        };
        Ok(decode_retry_marker_generation(&marker) == Some(expected_generation))
    }

    /// Get all peers that have pending retries.
    ///
    /// Returns `(peer_id, retry_info_bytes)` pairs.
    pub async fn get_all_retry_peers(&self) -> Result<Vec<(String, Bytes)>> {
        self.get_retry_peers(false).await
    }

    /// Get retry peers that still have a persisted replicator.
    pub async fn get_replicator_retry_peers(&self) -> Result<Vec<(String, Bytes)>> {
        self.get_retry_peers(true).await
    }

    async fn get_retry_peers(&self, require_replicator: bool) -> Result<Vec<(String, Bytes)>> {
        let prefix = ReplicatorRetryIDKey::retry_prefix();
        let txn = self.store.new_txn(true).await?;
        let opts = IterOptions::new().with_prefix(prefix);
        let mut iter = txn.iterator(opts).await?;

        let mut results = Vec::new();
        while let Some(pair) = iter.next().await? {
            let key_str = String::from_utf8_lossy(&pair.key);
            if let Some(peer_id) = key_str.strip_prefix("/rep/retry/id/") {
                if !peer_id.is_empty()
                    && (!require_replicator
                        || txn.has(&ReplicatorKey::new(peer_id).bytes()).await?)
                {
                    results.push((peer_id.to_string(), pair.value));
                }
            }
        }
        Ok(results)
    }

    /// Get all doc IDs pending retry for a specific peer.
    ///
    /// Returns `(doc_id, collection_id)` pairs.
    pub async fn get_retry_doc_ids(&self, peer_id: &str) -> Result<Vec<(String, String)>> {
        Ok(self
            .get_retry_documents(peer_id)
            .await?
            .into_iter()
            .map(|retry| (retry.doc_id, retry.collection_id))
            .collect())
    }

    /// Stop sweeping an empty peer after its receiver backpressure has expired.
    pub async fn clear_retry_peer(&self, peer_id: &str) -> Result<()> {
        self.transact_conflicts(Box::new(PeerstoreRetryAttempt::ClearPeer {
            peer_id: peer_id.to_owned(),
            expected_incarnation: None,
        }))
        .await
    }

    /// Clear the empty peer schedule only if the peer still has the incarnation
    /// that was observed when its retry pass began.
    pub async fn clear_retry_peer_for_incarnation(
        &self,
        peer_id: &str,
        expected_incarnation: u64,
    ) -> Result<()> {
        self.transact_conflicts(Box::new(PeerstoreRetryAttempt::ClearPeer {
            peer_id: peer_id.to_owned(),
            expected_incarnation: Some(expected_incarnation),
        }))
        .await
    }
}

impl<S: Store> crate::corekv::private::Sealed for Peerstore<S> {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store> Store for Peerstore<S> {
    fn crdts(&self) -> &crdt::Crdts {
        self.store.crdts()
    }

    async fn allocate(&self, key: &[u8], count: u64) -> Result<std::ops::Range<u64>> {
        self.store.allocate(key, count).await
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn transaction_stats_handle(&self) -> Option<crate::backends::TransactionStatsHandle> {
        self.store.transaction_stats_handle()
    }

    async fn new_txn(&self, readonly: bool) -> Result<Box<dyn Txn>> {
        self.store.new_txn(readonly).await
    }

    async fn transact_retry(
        &self,
        options: crate::corekv::RetryOptions,
        attempt: crate::corekv::RetryAttempt,
    ) -> std::result::Result<crate::corekv::RetryAttemptValue, crate::corekv::RetryAttemptError>
    {
        self.store.transact_retry(options, attempt).await
    }

    async fn close(&self) -> Result<()> {
        self.store.close().await
    }
}

#[path = "peerstore_placed.rs"]
mod placed;

#[cfg(test)]
#[path = "peerstore_tests.rs"]
mod tests;
