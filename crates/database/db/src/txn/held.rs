//! A transaction held by one handle that may also give it back (a fetcher or
//! a mutator its caller commits through), or only reached by one that was
//! lent it ([`Held::borrowed`]).
//!
//! A holding handle keeps the transaction's strong reference in a one-place
//! slot; every handle reaches it, for each operation, through a weak one. Giving it back
//! takes the strong reference out of the slot and unwraps it: that succeeds
//! only once no other handle and no operation in flight holds the
//! transaction, and otherwise puts the reference back and says so. Nothing
//! waits and nothing locks; an operation that starts after the transaction
//! was given back finds it gone.

use std::sync::{Arc, Weak};

use regolith::sync::ArrayQueue;
use storage::corekv::Store;

use super::{DbTxn, LentTxn};

pub(crate) struct Held<S>
where
    S: Store + 'static,
{
    /// Holds at most the one strong reference.
    strong: ArrayQueue<LentTxn<S>>,
    weak: Weak<DbTxn<S>>,
}

impl<S> Held<S>
where
    S: Store + 'static,
{
    /// Holds `txn`, which other handles may share too.
    pub(crate) fn new(txn: LentTxn<S>) -> Self {
        let weak = Arc::downgrade(&txn);
        let strong = ArrayQueue::new(1);
        // The slot is new and holds one: the push cannot find it full.
        let _ = strong.push(txn);
        Self { strong, weak }
    }

    /// Reaches `txn` without holding it: whoever lent it keeps it, and takes
    /// it back whether this handle is still around or not, once no
    /// operation of it is in flight. Afterwards every operation finds none.
    pub(crate) fn borrowed(txn: &LentTxn<S>) -> Self {
        Self {
            strong: ArrayQueue::new(1),
            weak: Arc::downgrade(txn),
        }
    }

    /// A handle that holds no transaction: every operation finds none.
    pub(crate) fn none() -> Self {
        Self {
            strong: ArrayQueue::new(1),
            weak: Weak::new(),
        }
    }

    /// The transaction, for one operation or for another handle to share.
    /// `None` once it was given back.
    pub(crate) fn get(&self) -> Option<LentTxn<S>> {
        self.weak.upgrade()
    }

    /// Gives the transaction back, by value. `None` when it was given back
    /// already, or while another handle or an operation still holds it, in
    /// which case it stays held here.
    pub(crate) fn take(&self) -> Option<DbTxn<S>> {
        let txn = self.strong.pop()?;
        match Arc::try_unwrap(txn) {
            Ok(txn) => Some(txn),
            Err(txn) => {
                // Put back where it was just taken from: the slot is empty.
                let _ = self.strong.push(txn);
                None
            }
        }
    }

    /// Whether the transaction was given back.
    pub(crate) fn is_taken(&self) -> bool {
        self.weak.strong_count() == 0
    }
}
