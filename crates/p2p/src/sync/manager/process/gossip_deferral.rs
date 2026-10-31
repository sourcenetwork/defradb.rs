//! Durable deferral of gossip heads refused at the edge (#2489).
//!
//! Gossip has no reply and no retry, and a write-once document is never
//! announced again, so a head refused for rate, worker saturation or pending
//! capacity is kept as a durable pending record with no in-memory slot. The
//! resync sweep promotes it once a slot frees; a root whose block never
//! arrived is fetched from the origin like a record restored after a restart.
//! Model: proofs/tla/GossipAdmission.tla.

use cid::Cid;

use blockstore::{verify_block_cid, Blockstore};

use super::pushlog::{announced_block_kind, AnnouncedBlockKind};
use super::SyncManager;
use crate::error::{Error, Result};
use crate::message::PushLogBroadcast;
use crate::sync::manager::events::SyncEvent;
use crate::sync::pending_store::{record_footprint, Admission, PersistedPendingDag, Registration};

impl<B> SyncManager<B>
where
    B: Blockstore + 'static,
{
    /// Keep a refused gossip head as a durable-only pending record, owned by
    /// `origin` for recovery. Past a byte budget, or with no durable store,
    /// the head is dropped: counted, logged and reported as a node event.
    pub(crate) async fn defer_gossip_head(
        &self,
        msg: &PushLogBroadcast,
        origin: &str,
    ) -> Result<()> {
        let cid = Cid::try_from(msg.cid.as_ref())
            .map_err(|e| Error::InvalidCid(format!("Failed to parse CID: {}", e)))?;
        let AnnouncedBlockKind::Head(head_priority) = announced_block_kind(&msg.block) else {
            // A descendant is not an obligation of its own; its head is.
            return Ok(());
        };
        if self.persisted.contains(&cid) || self.is_merged(&cid).await? {
            return Ok(());
        }
        let record = PersistedPendingDag {
            doc_id: msg.doc_id.clone(),
            collection_id: msg.collection_id.clone(),
            head_priority: Some(head_priority),
            creator: msg.creator.clone(),
            source_peer: Some(origin.to_owned()),
            alternate_providers: Vec::new(),
            is_explicit_replicator: false,
            explicit_replay_authorization: None,
        };
        let Some(store) = self.pending_store() else {
            return self
                .drop_gossip_head(cid, msg, origin, "no durable store")
                .await;
        };
        // ponytail: the budgets are read from the commit-ordered view before
        // the registration, as edge admission, so concurrent deferrals of one
        // peer may overshoot by one record each; a check inside the
        // transaction would make every deferral of a peer conflict.
        let bytes = record_footprint(&cid, &record);
        if self.persisted.source_bytes(origin) + bytes > self.gossip_deferral_peer_bytes {
            return self
                .drop_gossip_head(cid, msg, origin, "peer byte budget full")
                .await;
        }
        if self.persisted.total_bytes() + bytes > self.gossip_deferral_total_bytes {
            return self
                .drop_gossip_head(cid, msg, origin, "total byte budget full")
                .await;
        }

        // Keeping the bytes saves the promotion a fetch round trip.
        verify_block_cid(&cid, &msg.block)
            .map_err(|e| crate::error::blockstore_verify_to_p2p(e, &cid))?;
        self.blockstore
            .put(&cid, &msg.block)
            .await
            .map_err(Error::from_blockstore)?;

        // No slot: the byte budgets above bound these records, not the count
        // cap that bounds pushed registrations.
        let registration = Registration {
            root: cid,
            record: &record,
            capacity: None,
        };
        match store.register(registration, &self.persisted).await {
            Ok(Admission::Registered) => {
                self.diagnostics.record_gossip_head_deferred();
                self.durable_only_ready.notify_one();
                tracing::debug!(
                    cid = %cid,
                    doc_id = %msg.doc_id,
                    origin,
                    "Kept refused gossip head as a durable-only pending record"
                );
                Ok(())
            }
            Ok(Admission::Covered) => Ok(()),
            Ok(Admission::AtCapacity) => {
                self.drop_gossip_head(cid, msg, origin, "store full").await
            }
            Err(error) => Err(Error::Storage(format!(
                "failed to persist deferred gossip head: {error}"
            ))),
        }
    }

    /// Drop a refused gossip head that cannot be kept, and say so: a counter,
    /// a WARN, and a node event naming the peer and the root.
    async fn drop_gossip_head(
        &self,
        root_cid: Cid,
        msg: &PushLogBroadcast,
        origin: &str,
        reason: &'static str,
    ) -> Result<()> {
        self.diagnostics.record_gossip_head_shed();
        tracing::warn!(
            cid = %root_cid,
            doc_id = %msg.doc_id,
            collection_id = %msg.collection_id,
            origin,
            reason,
            "Refused gossip head dropped; the document will not arrive unless resynced"
        );
        let event = SyncEvent::GossipHeadDropped {
            root_cid,
            doc_id: msg.doc_id.clone(),
            collection_id: msg.collection_id.clone(),
            peer_id: origin.to_owned(),
        };
        if self.event_tx.send(event).await.is_err() {
            tracing::warn!(cid = %root_cid, "Failed to report a dropped gossip head");
        }
        Err(Error::GossipHeadDropped {
            cid: root_cid.to_string(),
            reason,
        })
    }

    /// Completes when a durable-only record may now fit a pending slot.
    pub async fn durable_only_ready(&self) {
        self.durable_only_ready.notified().await;
    }

    /// Whether a record from `source_peer` would fit the pending map now,
    /// checked before the sweep pays for its block read and link walk.
    pub(super) fn pending_has_room_for(&self, source_peer: Option<&str>) -> bool {
        let max_per_peer = self.max_pending_dags_per_peer();
        self.pending_dags.read(|pending| {
            pending.len() < self.max_pending_dags
                && source_peer.is_none_or(|peer| pending.source_count(peer) < max_per_peer)
        })
    }
}

#[cfg(test)]
#[path = "gossip_deferral_tests.rs"]
mod tests;
