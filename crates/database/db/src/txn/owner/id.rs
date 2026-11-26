//! A transaction ID names the process start that opened it and the worker that
//! owns it.
//!
//! The numeric wire ID carries the start token in its high bits and the owner
//! route in its low bits. The owner is `route % workers`, so the edge routes
//! every request for the transaction with arithmetic and no lookup, and each
//! worker draws its own sequence numbers without touching another's. The token
//! keeps an ID from one node start from naming a transaction or retained
//! outcome of another.

use std::fmt;

/// An explicit transaction's ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TxnId {
    start: u64,
    route: u64,
}

const START_MASK: u64 = u64::MAX << 32;
const ROUTE_MASK: u64 = u32::MAX as u64;
/// The start token's width. With the 32-bit route every new ID stays below
/// 2^53, so a JavaScript client reading it as a JSON number gets it back exact.
const START_BITS: u32 = 21;

impl TxnId {
    /// The start token a registry stamps on its IDs, from a random `draw`:
    /// never 0, which legacy IDs use, and within `START_BITS`.
    pub(crate) fn start_token(draw: u32) -> u64 {
        u64::from((draw & ((1 << START_BITS) - 1)).max(1)) << 32
    }

    /// The ID `start` gives worker `worker` of `workers` for its
    /// `sequence`th transaction; start 0 is reserved for legacy IDs, so a new
    /// ID refuses it. A worker's routes run out after 2^32 / `workers`
    /// sequences, so each further run of that many takes the next start token
    /// instead of failing: the sequence keeps counting and no two IDs of a
    /// start collide until 2^21 - 1 tokens have gone by. `None` when `worker`
    /// is not one of `workers`.
    pub(crate) fn new(sequence: u64, worker: usize, workers: usize, start: u64) -> Option<TxnId> {
        if start == 0 {
            return None;
        }
        let workers = u64::try_from(workers).ok()?;
        let worker = u64::try_from(worker).ok()?;
        if worker >= workers {
            return None;
        }
        let per_token = (ROUTE_MASK + 1) / workers;
        if per_token == 0 {
            return None;
        }
        let route = (sequence % per_token) * workers + worker;
        Some(TxnId {
            start: Self::advance(start, sequence / per_token),
            route,
        })
    }

    /// The start token `rollovers` tokens after `start`, cycling through every
    /// nonzero token below 2^21, so it is never the legacy 0.
    fn advance(start: u64, rollovers: u64) -> u64 {
        if rollovers == 0 {
            return start;
        }
        let tokens = (1u64 << START_BITS) - 1;
        let token = (start >> 32).saturating_sub(1) % tokens;
        ((token + rollovers % tokens) % tokens + 1) << 32
    }

    /// The ID a client sent back, as text. A number whose high bits are zero
    /// is a legacy route; otherwise it names a start and a low-bit route.
    pub(crate) fn parse(text: &str) -> Option<TxnId> {
        let encoded = text.parse::<u64>().ok()?;
        let start = encoded & START_MASK;
        if start == 0 {
            return Some(TxnId {
                start: 0,
                route: encoded,
            });
        }
        Some(TxnId {
            start,
            route: encoded & ROUTE_MASK,
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

    /// Rebuilds an ID from the two halves a table stores.
    pub(crate) fn from_key(key: (u64, u64)) -> TxnId {
        TxnId {
            start: key.0,
            route: key.1,
        }
    }

    /// The ID of a transaction no table holds and no request can name (a
    /// standalone query's snapshot), for its log lines.
    pub(crate) fn unnamed() -> TxnId {
        TxnId {
            start: u64::MAX,
            route: u64::MAX,
        }
    }
}

impl fmt::Display for TxnId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.start | self.route)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_names_the_worker_that_drew_it() {
        for workers in [1, 2, 3, 7, 64] {
            for worker in 0..workers {
                for sequence in [
                    0,
                    1,
                    2,
                    1_000,
                    ROUTE_MASK / u64::from(u32::try_from(workers).unwrap()) - 1,
                ] {
                    let start = 9u64 << 32;
                    let id = TxnId::new(sequence, worker, workers, start).unwrap();
                    assert_eq!(id.owner(workers), worker);
                    assert_eq!(TxnId::parse(&id.to_string()), Some(id));
                }
            }
        }
    }

    #[test]
    fn every_new_id_survives_a_json_number() {
        assert_eq!(TxnId::start_token(0), 1u64 << 32);
        for draw in [0, 1, 0x1F_FFFF, 0x20_0000, u32::MAX] {
            let start = TxnId::start_token(draw);
            assert_ne!(start, 0);
            let id = TxnId::new(ROUTE_MASK, 0, 1, start).unwrap();
            let encoded = id.start | id.route;
            assert!(encoded < 1u64 << 53);
            assert_eq!(encoded as f64 as u64, encoded);
            assert_eq!(TxnId::parse(&id.to_string()), Some(id));
        }
    }

    #[test]
    fn a_worker_past_its_routes_takes_the_next_start_token() {
        for workers in [1usize, 3, 64] {
            let per_token = (ROUTE_MASK + 1) / workers as u64;
            let start = TxnId::start_token(41);
            for worker in [0, workers - 1] {
                let last = TxnId::new(per_token - 1, worker, workers, start).unwrap();
                let next = TxnId::new(per_token, worker, workers, start).unwrap();
                assert_eq!(last.start, start);
                assert_eq!(next.start, start + (1u64 << 32));
                assert_eq!(next.route, worker as u64);
                assert_eq!(next.owner(workers), worker);
                assert_ne!(next, TxnId::new(0, worker, workers, start).unwrap());
                assert!(next.start | next.route < 1u64 << 53);
                assert_eq!(TxnId::parse(&next.to_string()), Some(next));
            }
        }
        let tokens = (1u64 << START_BITS) - 1;
        let highest = TxnId::start_token(u32::MAX);
        assert_eq!(highest, tokens << 32);
        let wrapped = TxnId::new(ROUTE_MASK + 1, 0, 1, highest).unwrap();
        assert_eq!(wrapped.start, 1u64 << 32, "the token cycles and skips 0");
        assert_eq!(TxnId::new(0, 2, 2, highest), None);
    }

    #[test]
    fn ids_from_different_workers_never_collide() {
        let workers = 4;
        let mut seen = rapidhash::RapidHashSet::default();
        for worker in 0..workers {
            for sequence in 0..100 {
                assert!(seen.insert(TxnId::new(sequence, worker, workers, 1u64 << 32).unwrap()));
            }
        }
    }

    #[test]
    fn one_worker_counts_from_zero_and_legacy_start_is_refused() {
        assert_eq!(TxnId::new(0, 0, 1, 9).map(TxnId::get), Some(0));
        assert_eq!(TxnId::new(41, 0, 1, 9u64 << 32).map(TxnId::get), Some(41));
        assert_eq!(
            TxnId::new(41, 0, 1, 9u64 << 32).map(TxnId::key),
            Some((9u64 << 32, 41))
        );
        assert_eq!(TxnId::new(0, 0, 1, 0), None);
        assert_eq!(TxnId::new(1, 1, 2, 9u64 << 32).unwrap().start, 9u64 << 32);
        assert_eq!(
            TxnId::new(0, 1, 2, 9u64 << 32),
            Some(TxnId {
                start: 9u64 << 32,
                route: 1
            })
        );
        assert_eq!(
            TxnId::parse(
                &TxnId {
                    start: 9u64 << 32,
                    route: 41
                }
                .to_string()
            ),
            Some(TxnId {
                start: 9u64 << 32,
                route: 41
            })
        );
        assert_eq!(
            TxnId::parse("41"),
            Some(TxnId {
                start: 0,
                route: 41
            })
        );
        assert_eq!(TxnId::parse("x"), None);
        assert_eq!(TxnId::parse("not a number"), None);
        assert_eq!(TxnId::parse("-1"), None);
    }
}
