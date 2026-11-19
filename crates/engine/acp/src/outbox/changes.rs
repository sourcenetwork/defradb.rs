//! Owed changes as one transaction sees and writes them.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use datastore::{NamespaceView, WeakNamespaceView};
use storage::corekv::Store;

use super::keys::OwedTupleState;
use super::ops;
use super::record::{NewOwedChange, OwedAuthority, OwedChange, OwedState, OwedTuple};
use crate::error::{Error, Result};

/// The most changes one transaction may stage.
pub const MAX_OWED_CHANGES_PER_TRANSACTION: u32 = 256;

/// The most changes a node holds unsettled, unless
/// [`OwedChanges::with_node_limit`] sets another bound.
pub const MAX_UNSETTLED_OWED_CHANGES: u64 = 10_000;

/// The most changes one acting DID holds unsettled on a node. A change that
/// is parked stays unsettled, so without this bound one actor's parked
/// changes could take the whole of the node's.
pub const MAX_UNSETTLED_OWED_CHANGES_PER_ACTOR: u64 = 1_000;

/// How close to the node's bound its count is read validated. Below that the
/// count is read without validation, as every staging and settling
/// transaction merges into it; from there on it is read validated on what was
/// staged only, so transactions that stage there commit one after another,
/// each having read the count the one before it left, while a settle aborts
/// none of them. The bound then holds unless transactions staging together,
/// each below this margin when it read, add more than the margin between
/// them.
pub const VALIDATED_WITHIN_OF_NODE_LIMIT: u64 = 4 * MAX_OWED_CHANGES_PER_TRANSACTION as u64;

/// Records read into memory per page of the queue, bounded by their encoded
/// size.
const PAGE_BYTES: usize = 256 * 1024;

/// How many changes one transaction has staged, against
/// [`MAX_OWED_CHANGES_PER_TRANSACTION`]. The transaction's owner keeps one for
/// the transaction and hands it to every handle it opens on it. A rollback to
/// a savepoint drops the changes staged since, so the owner that rolls back
/// puts the count back to what it read at the savepoint.
#[derive(Clone, Debug, Default)]
pub struct StagedCount(Arc<AtomicU32>);

impl StagedCount {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }

    /// Put the count back to `count`, read at a savepoint now rolled back to.
    pub fn restore(&self, count: u32) {
        self.0.store(count, Ordering::Relaxed);
    }
}

/// Owed relationship changes through one transaction: its own writes over
/// its snapshot, committed and rolled back with its rows. The handle does not
/// keep the transaction in use, but a savepoint needs it dropped, as it needs
/// every view of the transaction gone.
#[derive(Clone)]
pub struct OwedChanges {
    view: WeakNamespaceView,
    sequences: Arc<dyn Store>,
    staged: StagedCount,
    node_limit: u64,
}

impl OwedChanges {
    /// `view` is the transaction's ACP namespace, `sequences` the store's,
    /// and `staged` the transaction's count. Sequences are allocated outside
    /// the transaction, so one that rolls back leaves a gap and nothing else.
    pub fn new(view: &NamespaceView, sequences: Arc<dyn Store>, staged: StagedCount) -> Self {
        Self {
            view: view.downgrade(),
            sequences,
            staged,
            node_limit: MAX_UNSETTLED_OWED_CHANGES,
        }
    }

    /// Hold at most `limit` changes unsettled on the node.
    pub fn with_node_limit(mut self, limit: u64) -> Self {
        self.node_limit = limit;
        self
    }

    /// How many changes the transaction has staged and kept so far.
    pub fn staged(&self) -> u32 {
        self.staged.get()
    }

    fn active_view(&self) -> Result<NamespaceView> {
        self.view
            .upgrade()
            .ok_or_else(|| Error::Storage("the transaction is no longer active".into()))
    }

    /// Record, in this transaction, a change it owes the provider, and give
    /// it its sequence.
    ///
    /// Two transactions that stage the same tuple conflict, and the one that
    /// commits later holds the higher sequence. Changes of different tuples
    /// do not conflict unless they are on one object, whose count both write;
    /// their sequences follow the order they were staged in, which is not
    /// always the order their transactions commit in.
    ///
    /// The node's bound and the acting DID's are checked against what this
    /// transaction sees. Near the node's bound its count is read validated
    /// on what was staged ([`VALIDATED_WITHIN_OF_NODE_LIMIT`]), so callers
    /// acting as many identities at once cannot together pass it there; the
    /// acting DID's bound is read without validation, and transactions
    /// staging at once may pass it by what they stage together.
    pub async fn stage(&self, new: NewOwedChange) -> Result<OwedChange> {
        let view = self.active_view()?;
        if self.staged.get() >= MAX_OWED_CHANGES_PER_TRANSACTION {
            return Err(Error::OwedChangesPerTransaction {
                limit: MAX_OWED_CHANGES_PER_TRANSACTION,
            });
        }
        let mut unsettled = ops::unsettled(&view).await?;
        if unsettled.saturating_add(VALIDATED_WITHIN_OF_NODE_LIMIT) >= self.node_limit {
            unsettled = ops::unsettled_validated(&view).await?;
        }
        if unsettled >= self.node_limit {
            return Err(Error::OwedChangesPerNode {
                unsettled,
                limit: self.node_limit,
            });
        }
        let by_actor = ops::unsettled_by(&view, &new.actor).await?;
        if by_actor >= MAX_UNSETTLED_OWED_CHANGES_PER_ACTOR {
            return Err(Error::OwedChangesPerActor {
                actor: new.actor.to_string(),
                unsettled: by_actor,
                limit: MAX_UNSETTLED_OWED_CHANGES_PER_ACTOR,
            });
        }
        let change = ops::stage(&view, self.sequences.as_ref(), new).await?;
        self.staged.0.fetch_add(1, Ordering::Relaxed);
        Ok(change)
    }

    /// The owed change at `seq`, if it is unsettled.
    pub async fn get(&self, seq: u64) -> Result<Option<OwedChange>> {
        ops::change(&self.active_view()?, seq).await
    }

    /// The next page of the queue in sequence order: the changes after
    /// `after`, or from its start. Empty at the end of the queue.
    pub async fn page(&self, after: Option<u64>) -> Result<Vec<OwedChange>> {
        ops::page(&self.active_view()?, after, PAGE_BYTES).await
    }

    /// Delete the record of the change at `seq` in this transaction: the
    /// provider holds it, or its owner gave it up. `false` when no such
    /// change is owed.
    pub async fn settle(&self, seq: u64) -> Result<bool> {
        ops::settle(&self.active_view()?, seq).await
    }

    /// Record, in this transaction, what a submission of the change at `seq`
    /// came to: its state, how many submissions in a row the provider
    /// answered this way, and its words. The change stays unsettled, so its
    /// tuple's state and the counts do not move. `false` when no such change
    /// is owed.
    pub async fn mark(
        &self,
        seq: u64,
        state: OwedState,
        attempts: u32,
        error: Option<String>,
    ) -> Result<bool> {
        ops::mark(&self.active_view()?, seq, state, attempts, error).await
    }

    /// Record, in this transaction, a fresh authority with the change at
    /// `seq` and make it owed again: staged, with nothing counted against
    /// it. `false` when no such change is owed.
    pub async fn reauthorize(&self, seq: u64, authority: OwedAuthority) -> Result<bool> {
        ops::reauthorize(&self.active_view()?, seq, authority).await
    }

    /// Delete the record of the change at `seq` in this transaction without
    /// the provider holding it, and keep a note of it and of `reason` for
    /// its actor. `false` when no such change is owed.
    pub async fn discard(&self, seq: u64, reason: &str) -> Result<bool> {
        ops::discard(&self.active_view()?, seq, reason).await
    }

    /// The note kept of the discarded change at `seq`, if any.
    pub async fn discarded(&self, seq: u64) -> Result<Option<OwedChange>> {
        ops::discarded(&self.active_view()?, seq).await
    }

    /// Every note of a discarded change, in sequence order.
    pub async fn discards(&self) -> Result<Vec<OwedChange>> {
        ops::discards(&self.active_view()?).await
    }

    /// Delete, in this transaction, the note of the discarded change at
    /// `seq`: its actor was told.
    pub async fn acknowledge(&self, seq: u64) -> Result<()> {
        ops::acknowledge(&self.active_view()?, seq).await
    }

    /// How many changes `actor` has unsettled on the node.
    pub async fn unsettled_by(&self, actor: &identity::Did) -> Result<u64> {
        ops::unsettled_by(&self.active_view()?, actor).await
    }

    /// What is unsettled on `tuple`, if anything is.
    pub async fn tuple(&self, tuple: &OwedTuple) -> Result<Option<OwedTupleState>> {
        ops::tuple_state(&self.active_view()?, tuple).await
    }

    /// How many changes are unsettled on one object.
    pub async fn unsettled_on(&self, policy_id: &str, resource: &str, object: &str) -> Result<u64> {
        ops::unsettled_on(&self.active_view()?, policy_id, resource, object).await
    }

    /// How many changes are unsettled on the node.
    pub async fn unsettled(&self) -> Result<u64> {
        ops::unsettled(&self.active_view()?).await
    }
}
