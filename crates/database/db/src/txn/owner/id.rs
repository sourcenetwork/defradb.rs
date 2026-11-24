//! A transaction ID names the worker that owns the transaction.
//!
//! The ID is `sequence * workers + worker`: the owner is `id % workers`, so the
//! edge routes every request for the transaction with arithmetic and no
//! lookup, and each worker draws its own sequence numbers without touching
//! another's. With one worker the IDs are 0, 1, 2, ... The ID stays a plain
//! `u64` on the wire, as Go DefraDB's transaction IDs are.

use std::fmt;

/// An explicit transaction's ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TxnId(u64);

impl TxnId {
    /// The ID worker `worker` of `workers` gives its `sequence`th transaction;
    /// `None` once the IDs would pass `u64::MAX`.
    pub(crate) fn new(sequence: u64, worker: usize, workers: usize) -> Option<TxnId> {
        let workers = u64::try_from(workers).ok()?;
        let worker = u64::try_from(worker).ok()?;
        sequence
            .checked_mul(workers)?
            .checked_add(worker)
            .map(TxnId)
    }

    /// The ID a client sent back, as text; `None` when it is not a `u64`.
    pub(crate) fn parse(text: &str) -> Option<TxnId> {
        text.parse().ok().map(TxnId)
    }

    /// The index of the worker that owns this transaction among `workers`.
    pub(crate) fn owner(self, workers: usize) -> usize {
        let workers = u64::try_from(workers.max(1)).unwrap_or(u64::MAX);
        usize::try_from(self.0 % workers).unwrap_or(0)
    }

    pub(crate) fn get(self) -> u64 {
        self.0
    }

    /// The ID of a transaction no table holds and no request can name (a
    /// standalone query's snapshot), for its log lines.
    pub(crate) fn unnamed() -> TxnId {
        TxnId(u64::MAX)
    }

    /// The ID a table stored.
    pub(super) fn from_raw(id: u64) -> TxnId {
        TxnId(id)
    }
}

impl fmt::Display for TxnId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_names_the_worker_that_drew_it() {
        for workers in [1, 2, 3, 7, 64] {
            for worker in 0..workers {
                for sequence in [0, 1, 2, 1_000, u64::MAX / 128] {
                    let id = TxnId::new(sequence, worker, workers).unwrap();
                    assert_eq!(id.owner(workers), worker);
                    assert_eq!(TxnId::parse(&id.to_string()), Some(id));
                }
            }
        }
    }

    #[test]
    fn ids_from_different_workers_never_collide() {
        let workers = 4;
        let mut seen = rapidhash::RapidHashSet::default();
        for worker in 0..workers {
            for sequence in 0..100 {
                assert!(seen.insert(TxnId::new(sequence, worker, workers).unwrap()));
            }
        }
    }

    #[test]
    fn one_worker_counts_from_zero_and_ids_past_u64_are_refused() {
        assert_eq!(TxnId::new(0, 0, 1).map(TxnId::get), Some(0));
        assert_eq!(TxnId::new(41, 0, 1).map(TxnId::get), Some(41));
        assert_eq!(TxnId::new(u64::MAX, 1, 2), None);
        assert_eq!(TxnId::parse("not a number"), None);
        assert_eq!(TxnId::parse("-1"), None);
    }
}
