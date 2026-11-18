//! Shared helper for registering what a committed write publishes.
//!
//! Both `DbDocMutator` (explicit-tx path) and `BatchMutator` (auto-commit path)
//! use this to register it at mutation time. It runs only for a commit, in
//! commit-receipt order with every other ordered event; a discard skips it.

use bytes::Bytes;
use cid::Cid;
use events::{Bus, Message, Update};
use std::sync::Arc;
use storage::corekv::{OnCommit, Store};

use crate::error::Result;
use crate::merge::governance::LocalCommitRelease;
use crate::txn::DbTxn;

/// Per-mutation broadcast payload, mirroring Go's sendUpdate call.
#[derive(Debug, Clone)]
pub struct TxnBroadcastEvent {
    pub collection_name: String,
    pub collection_id: String,
    pub doc_id: String,
    pub doc_cid: Cid,
    pub doc_block: Bytes,
    pub document_json: Option<serde_json::Value>,
    pub collection_block: Option<(Cid, Bytes)>,
    pub creator_did: Option<String>,
}

/// Hook for forwarding committed transactional writes to the P2P stack.
///
/// `DbDocMutator` (the explicit-tx path) and `BatchMutator` (the auto-commit
/// path) hand every committed write here, so a transaction gets the same
/// push-to-replicators and gossipsub broadcast a single mutation does.
///
/// Without a `TxnBroadcaster`, transactional writes commit locally
/// and publish to the local bus only — P2P peers never see them.
/// Mirrors Go's `db.sendUpdate` at `internal/db/p2p.go:23-25`.
pub trait TxnBroadcaster: defra_core::thread_bounds::MaybeSendSync {
    /// Takes one committed write. Called in commit-receipt order, from the
    /// ordered event dispatch, so it must return without waiting; the work
    /// runs elsewhere, in the order of these calls. Drop `done` once the
    /// write's durable delivery markers are written, or once it is given up:
    /// the committing transaction returns only then.
    fn submit_update(&self, event: TxnBroadcastEvent, done: BroadcastDone);
}

/// Tells a committing transaction that its broadcast was handled; set on drop.
pub struct BroadcastDone(Arc<regolith::sync::Event>);

impl Drop for BroadcastDone {
    fn drop(&mut self) {
        self.0.set();
    }
}

/// Register the publication of a committed write: an Update event with the
/// document block bytes (and, when `collection_block` is `Some`, a second
/// collection-level Update with the collection block's own cid and bytes),
/// the release of composites waiting on it, and its broadcast to peers. All
/// three run in commit-receipt order, like every ordered event, and the
/// transaction's commit returns once the broadcast's markers are durable.
///
/// Mirrors Go's `db.sendUpdate` callback registration at
/// `internal/db/collection.go:755` and the branchable collection event at
/// `internal/db/collection.go:789`, both of which publish the actual block bytes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn register_update_event_callback<S>(
    txn: &DbTxn<S>,
    bus: Option<&Arc<dyn Bus>>,
    broadcaster: Option<&Arc<dyn TxnBroadcaster>>,
    release: Option<&Arc<dyn LocalCommitRelease>>,
    collection_name: String,
    collection_id: String,
    doc_id: String,
    doc_cid: Cid,
    doc_block: Bytes,
    document_json: Option<serde_json::Value>,
    collection_block: Option<(Cid, Bytes)>,
    creator_did: Option<String>,
) -> Result<()>
where
    S: Store + 'static,
{
    if bus.is_none() && broadcaster.is_none() && release.is_none() {
        return Ok(());
    }
    let bus = bus.map(Arc::clone);
    let broadcaster = broadcaster.map(Arc::clone);
    let release = release.map(Arc::clone);
    let delivered = broadcaster
        .is_some()
        .then(|| Arc::new(regolith::sync::Event::new()));
    let handed = delivered.clone();
    let event = TxnBroadcastEvent {
        collection_name,
        collection_id,
        doc_id,
        doc_cid,
        doc_block,
        document_json,
        collection_block,
        creator_did,
    };
    txn.on_commit(OnCommit::Ordered(Box::new(move || {
        if let Some(bus) = &bus {
            bus.publish(Message::update(Update::new(
                event.doc_id.clone(),
                event.doc_cid,
                event.collection_id.clone(),
                event.doc_block.clone(),
                false,
                false,
            )));
            if let Some((col_cid, col_block)) = &event.collection_block {
                bus.publish(Message::update(Update::new_with_subject_doc_id(
                    String::new(),
                    event.doc_id.clone(),
                    *col_cid,
                    event.collection_id.clone(),
                    col_block.clone(),
                    false,
                    false,
                )));
            }
        }
        if let Some(release) = &release {
            release.committed(event.doc_cid, event.doc_block.clone());
        }
        if let (Some(broadcaster), Some(handed)) = (&broadcaster, &handed) {
            broadcaster.submit_update(event.clone(), BroadcastDone(Arc::clone(handed)));
        }
    })))?;
    if let Some(delivered) = delivered {
        txn.on_commit(OnCommit::Async(Box::new(move || {
            Box::pin(async move { delivered.wait().await })
        })))?;
    }
    Ok(())
}

/// Register a batch of committed events for receipt-ordered publication.
pub(crate) fn register_ordered_events<S: Store>(
    txn: &DbTxn<S>,
    bus: Option<&Arc<dyn Bus>>,
    messages: Vec<Message>,
) -> Result<()> {
    let Some(bus) = bus.map(Arc::clone) else {
        return Ok(());
    };
    if messages.is_empty() {
        return Ok(());
    }
    txn.on_commit(OnCommit::Ordered(Box::new(move || {
        bus.publish_batch(messages)
    })))?;
    Ok(())
}
