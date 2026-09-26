//! A node judges its own writes by the merge validator before committing
//! them.
//!
//! A local write never meets the merge validator: the query path builds the
//! blocks and commits them, and the validator sees only what arrives from a
//! peer. So a node whose application's write validator was weaker than its
//! merge validator, or absent, could commit a write every peer then
//! rejected, and keep a document state nobody else shared, silently. The
//! contract's rule (P-6) that the write validator refuse whatever the merge
//! validator would was two implementations kept in step by hand.
//!
//! Here the composite a write builds is judged, inside the writing
//! transaction and before it commits, by the same validator with the same
//! candidate every peer will see. Accept commits. Reject refuses the write
//! with the validator's reason. Defer refuses it too, naming what the
//! verdict awaited: a write the node cannot justify from what it holds would
//! be stranded on every peer, so the client is told now rather than never.
//!
//! The blocks are not in the blockstore until the transaction commits, so
//! the validator reads them through a view that answers from the
//! transaction's own blockstore first and falls through to the ordinary
//! merge view for everything else.

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use cid::Cid;
use datastore::NamespaceView;
use defra_core::block::{Block, CrdtDelta};
use defra_core::thread_bounds::MaybeSendSync;
use document::NormalValue;
use schema::CollectionVersion;
use storage::corekv::Store;

use super::signature::SignatureStatus;
use super::validator::MergeCandidate;
use super::verdict::MergeVerdict;
use super::view::{decode, DbMergeView, FieldValue, MergeView};
use crate::database::DB;
use crate::merge::merge_handler::signature::verify_signature_data;
use crate::merge::merge_handler::DbMergeHandler;

/// The writing transaction's stores, as a judgement reads them: the blocks
/// a write built (uncommitted), and the documents this transaction has
/// written so far, so a write may rely on one made earlier in the same
/// batch, as every peer will once both have replicated.
pub struct PendingStores {
    pub blockstore: NamespaceView,
    pub datastore: NamespaceView,
    pub systemstore: NamespaceView,
}

impl PendingStores {
    /// Taken before any await, so no borrow of the transaction is held
    /// across one.
    pub fn of<S: Store>(txn: &crate::txn::DbTxn<S>) -> crate::Result<Self> {
        Ok(Self {
            blockstore: txn.blockstore()?,
            datastore: txn.datastore()?,
            systemstore: txn.systemstore()?,
        })
    }
}

/// Judges a composite a local write built, before its transaction commits.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait LocalWriteJudge: MaybeSendSync {
    /// `collection` is the version the write path resolved, in the writing
    /// transaction, so a collection defined in that same transaction is
    /// judged too. `Ok(None)` lets the write proceed; `Ok(Some(reason))`
    /// refuses it; `Err` refuses it as well, since a write nobody could
    /// judge is a write every peer would defer.
    async fn judge(
        &self,
        pending: &PendingStores,
        collection: &CollectionVersion,
        doc_id: &str,
        cid: &Cid,
        block: &[u8],
    ) -> Result<Option<String>, String>;
}

/// Judge a composite a local write built, if a judge is installed.
/// `pending` is the writing transaction's stores, taken by the caller so no
/// transaction borrow is held across the judgement.
pub(crate) async fn judge_local_write<S: Store>(
    db: &crate::database::DB<S>,
    pending: PendingStores,
    collection: &CollectionVersion,
    doc_id: &str,
    cid: &Cid,
    block: &[u8],
) -> crate::Result<()> {
    // Only a claimed collection is judged, so a write to any other never
    // needs the judge, alive or not.
    if !db
        .merge_governance()
        .is_some_and(|governance| governance.governs(collection))
    {
        return Ok(());
    }
    let Some(judge) = db.local_write_judge() else {
        return Ok(());
    };
    match judge.judge(&pending, collection, doc_id, cid, block).await {
        Ok(None) => Ok(()),
        Ok(Some(reason)) => Err(crate::Error::WriteRefused(reason)),
        Err(error) => Err(crate::Error::WriteRefused(format!(
            "the merge validator could not judge the write: {error}"
        ))),
    }
}

/// [`LocalWriteJudge`] over the database itself, building the merge handler
/// a judgement reads through when a write needs one.
///
/// It holds the database weakly, since the database holds it, and holds no
/// merge handler: a judge borrowing a replication stack's handler judged
/// nothing once that stack stopped, and the first-wins slot it sits in
/// kept the dead one, so every write after a P2P restart, and every write
/// on a node that never started one, committed unjudged.
pub(crate) struct DbJudge<S: Store, B: blockstore::Blockstore> {
    db: Weak<DB<S>>,
    blockstore: Arc<B>,
    max_merge_depth: usize,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, B> LocalWriteJudge for DbJudge<S, B>
where
    S: Store + 'static,
    B: blockstore::Blockstore + MaybeSendSync + 'static,
{
    async fn judge(
        &self,
        pending: &PendingStores,
        collection: &CollectionVersion,
        doc_id: &str,
        cid: &Cid,
        block: &[u8],
    ) -> Result<Option<String>, String> {
        // The judge outlives the database only while the node shuts down;
        // a write it cannot judge is refused, never let through.
        let Some(db) = self.db.upgrade() else {
            return Err("the database is no longer available".to_string());
        };
        let Some(governance) = db.merge_governance() else {
            return Ok(None);
        };
        if !governance.governs(collection) {
            return Ok(None);
        }
        let collection_name = &collection.name;
        let Some(validator) = governance.validator() else {
            return Ok(Some(format!(
                "collection {collection_name} is governed but no merge validator is installed, \
                 so every peer would defer this write"
            )));
        };
        let handler = DbMergeHandler::new_with_max_merge_depth(
            db.clone(),
            self.blockstore.clone(),
            self.max_merge_depth,
        );
        let block = Block::from_dag_cbor(block).map_err(|error| error.to_string())?;
        let CrdtDelta::Composite(payload) = &block.delta else {
            return Ok(None);
        };
        let signature = match block.signature {
            None => SignatureStatus::Unsigned,
            Some(signature_cid) => match pending
                .blockstore
                .get(&signature_cid.to_bytes())
                .await
                .map_err(|error| error.to_string())?
            {
                None => SignatureStatus::NotHeld(signature_cid),
                Some(data) => match verify_signature_data(cid, &block, &data) {
                    Ok(did) => SignatureStatus::Verified(did),
                    Err(error) => SignatureStatus::Invalid(error.to_string()),
                },
            },
        };
        let is_genesis = block.heads.as_deref().is_none_or(<[Cid]>::is_empty);
        let candidate = MergeCandidate {
            cid,
            block: &block,
            payload,
            doc_id,
            collection,
            is_genesis,
            signature,
        };
        let view = PendingView {
            pending: &pending.blockstore,
            inner: DbMergeView::with_stores(&handler, &pending.datastore, &pending.systemstore),
            handler: &handler,
            doc_id,
            is_genesis,
        };
        let verdict = validator.validate(&candidate, &view).await;
        view.inner.finish().await;
        Ok(match verdict? {
            MergeVerdict::Accept => None,
            MergeVerdict::Reject { reason } => Some(reason),
            MergeVerdict::Defer { reason, awaiting } => {
                let keys: Vec<String> = awaiting.iter().map(|key| format!("{key:?}")).collect();
                Some(format!(
                    "every peer would defer it: {reason} (awaiting {})",
                    if keys.is_empty() {
                        "nothing nameable".to_string()
                    } else {
                        keys.join(", ")
                    }
                ))
            }
        })
    }
}

/// The merge view a local write is judged through: the transaction's
/// uncommitted blocks first, and its documents as it has written them so
/// far, so a batch that creates a grant and then a note under it is judged
/// as every peer will judge it once both have merged. The one document it
/// hides is the candidate's own when the write creates it: no peer holds
/// that document while judging its genesis, and a rule that counts matches
/// would otherwise count the write against itself.
struct PendingView<'a, S: Store, B: blockstore::Blockstore> {
    pending: &'a NamespaceView,
    inner: DbMergeView<'a, S, B>,
    handler: &'a DbMergeHandler<S, B>,
    doc_id: &'a str,
    is_genesis: bool,
}

impl<S: Store, B: blockstore::Blockstore> PendingView<'_, S, B> {
    async fn load(&self, cid: &Cid) -> Result<Option<Block>, String> {
        if let Some(data) = self
            .pending
            .get(&cid.to_bytes())
            .await
            .map_err(|error| error.to_string())?
        {
            return Block::from_dag_cbor(&data)
                .map(Some)
                .map_err(|error| error.to_string());
        }
        self.inner.block(cid).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, B> MergeView for PendingView<'_, S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    async fn block(&self, cid: &Cid) -> Result<Option<Block>, String> {
        self.load(cid).await
    }

    async fn composite_fields(
        &self,
        cid: &Cid,
    ) -> Result<Option<Vec<(String, FieldValue)>>, String> {
        let Some(composite) = self.load(cid).await? else {
            return Ok(None);
        };
        let mut fields = Vec::new();
        for link in composite.links.iter().flatten() {
            let value = match self.load(&link.link).await? {
                None => FieldValue::NotHeld(link.link),
                Some(field) if field.encryption.is_some() => FieldValue::Encrypted,
                Some(field) => match &field.delta {
                    CrdtDelta::Lww(payload) => decode(&payload.data),
                    CrdtDelta::Counter(payload) => decode(&payload.data),
                    _ => FieldValue::Undecodable,
                },
            };
            fields.push((link.name.clone(), value));
        }
        Ok(Some(fields))
    }

    async fn genesis(&self, cid: &Cid) -> Result<Option<Cid>, String> {
        let mut current = *cid;
        for depth in 0.. {
            self.handler
                .ensure_merge_depth(&current, depth)
                .map_err(|error| error.to_string())?;
            let Some(block) = self.load(&current).await? else {
                return Ok(None);
            };
            match block.heads.as_deref().and_then(<[Cid]>::first) {
                Some(parent) => current = *parent,
                None => return Ok(Some(current)),
            }
        }
        unreachable!("the depth check ends the walk")
    }

    async fn find_documents(
        &self,
        collection: &str,
        field: &str,
        value: &NormalValue,
    ) -> Result<Vec<String>, String> {
        let mut ids = self.inner.find_documents(collection, field, value).await?;
        if self.is_genesis {
            ids.retain(|id| id != self.doc_id);
        }
        Ok(ids)
    }

    async fn immutable_fields(
        &self,
        collection: &str,
        doc_id: &str,
    ) -> Result<Option<Vec<(String, NormalValue)>>, String> {
        if self.is_genesis && doc_id == self.doc_id {
            return Ok(None);
        }
        self.inner.immutable_fields(collection, doc_id).await
    }
}

impl<S, B> DbMergeHandler<S, B>
where
    S: Store + 'static,
    B: blockstore::Blockstore + MaybeSendSync + 'static,
{
    /// Judge this node's own writes by the merge validator before they
    /// commit, so a write every peer would refuse is refused here first.
    /// The judge outlives this handler: it needs only the database and the
    /// blockstore.
    pub fn install_local_write_judge(self: &Arc<Self>) {
        install_local_write_judge(&self.db, self.blockstore.clone(), self.max_merge_depth);
    }
}

/// Judge `db`'s own writes by its merge validator, through `blockstore`,
/// whether or not a replication stack ever runs. First call wins.
pub(crate) fn install_local_write_judge<S, B>(
    db: &Arc<DB<S>>,
    blockstore: Arc<B>,
    max_merge_depth: usize,
) where
    S: Store + 'static,
    B: blockstore::Blockstore + MaybeSendSync + 'static,
{
    db.set_local_write_judge(Arc::new(DbJudge {
        db: Arc::downgrade(db),
        blockstore,
        max_merge_depth,
    }));
}
