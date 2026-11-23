//! A transaction ID names the process start that opened it and the worker that
//! owns it.
//!
//! The wire ID is the opaque text `<start>-<route>`: `start` is a 64-bit token
//! drawn once by the owner registry, and `route` is `sequence * workers +
//! worker`. The owner is `route % workers`, so the edge routes every request
//! for the transaction with arithmetic and no lookup, and each worker draws its
//! own sequence numbers without touching another's. The token keeps an ID from
//! one node start from naming a transaction or retained outcome of another.

use std::fmt;

/// An explicit transaction's ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TxnId {
    start: u64,
    route: u64,
}

impl TxnId {
    /// The ID `start` gives worker `worker` of `workers` for its
    /// `sequence`th transaction; `None` when the worker or route cannot be
    /// represented.
    pub(crate) fn new(sequence: u64, worker: usize, workers: usize, start: u64) -> Option<TxnId> {
        let workers = u64::try_from(workers).ok()?;
        let worker = u64::try_from(worker).ok()?;
        let route = sequence
            .checked_mul(workers)?
            .checked_add(worker)
            .map(|route| (start, route))
            .map(|(_, route)| route)?;
        Some(TxnId { start, route })
    }

    /// The ID a client sent back, as text; `None` when it is not
    /// `<start>-<route>`.
    pub(crate) fn parse(text: &str) -> Option<TxnId> {
        let (start, route) = text.split_once('-')?;
        Some(TxnId {
            start: start.parse().ok()?,
            route: route.parse().ok()?,
        })
    }

    /// The index of the worker that owns this transaction among `workers`.
    pub(crate) fn owner(self, workers: usize) -> usize {
        let workers = u64::try_from(workers.max(1)).unwrap_or(u64::MAX);
        usize::try_from(self.route % workers).unwrap_or(0)
    }

    pub(crate) fn get(self) -> u64 {
        self.route
    }

    /// The full key a registry-wide table stores: the start token and route.
    pub(crate) fn key(self) -> (u64, u64) {
        (self.start, self.route)
    }

    /// The ID of a transaction no table holds and no request can name (a
    /// standalone query's snapshot), for its log lines.
    pub(crate) fn unnamed() -> TxnId {
        TxnId {
            start: u64::MAX,
            route: u64::MAX,
        }
    }

    /// The ID a table stored.
    pub(super) fn from_raw(id: u64) -> TxnId {
        TxnId {
            start: 0,
            route: id,
        }
    }
}

impl fmt::Display for TxnId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.start, self.route)
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
                    let start = (worker as u64) * 7 + sequence + 1;
                    let id = TxnId::new(sequence, worker, workers, start).unwrap();
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
                assert!(seen.insert(TxnId::new(sequence, worker, workers, 1).unwrap()));
            }
        }
    }

    #[test]
    fn one_worker_counts_from_zero_and_ids_past_u64_are_refused() {
        assert_eq!(TxnId::new(0, 0, 1, 9).map(TxnId::get), Some(0));
        assert_eq!(TxnId::new(41, 0, 1, 9).map(TxnId::get), Some(41));
        assert_eq!(TxnId::new(41, 0, 1, 9).map(TxnId::key), Some((9, 41)));
        assert_eq!(TxnId::new(1, 1, 2, 9).unwrap().start, 9);
        assert_eq!(TxnId::parse("9-41"), TxnId::new(41, 0, 1, 9));
        assert_eq!(TxnId::parse("9"), None);
        assert_eq!(TxnId::parse("x-1"), None);
        assert_eq!(TxnId::parse("not a number"), None);
        assert_eq!(TxnId::parse("-1"), None);
    }
}
