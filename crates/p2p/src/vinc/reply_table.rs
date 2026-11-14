//! The endpoint's side of the reply bridge: each request stream still owed a
//! reply, under the id its token carries.
//!
//! An entry leaves the table exactly once: taken by the reply, taken by the
//! token's drop (which resets the stream), purged when its connection ends,
//! or swept once its deadline passes (also a reset). Ids are never reused, so
//! a late reply or drop finds nothing and touches no other stream. The table
//! holds at most `capacity` entries; a request beyond that is refused busy.
//!
//! Modeled in `proofs/tla/VincReplyBridge.tla`; the table's laws are proved in
//! `proofs/lean/VincReplyTable/Table.lean`.

use std::cell::{Cell, RefCell};
use std::time::Instant;

use rapidhash::{HashMapExt, RapidHashMap};

struct Entry<S> {
    stream: S,
    connection: u64,
    deadline: Instant,
}

pub struct ReplyTable<S> {
    entries: RefCell<RapidHashMap<u64, Entry<S>>>,
    next: Cell<u64>,
    capacity: usize,
}

impl<S> ReplyTable<S> {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: RefCell::new(RapidHashMap::new()),
            next: Cell::new(1),
            capacity,
        }
    }

    /// Hold `stream`, owed a reply on `connection` until `deadline`, under a
    /// fresh id. `Err(stream)` when the table is full.
    pub fn issue(&self, stream: S, connection: u64, deadline: Instant) -> Result<u64, S> {
        let mut entries = self.entries.borrow_mut();
        if entries.len() >= self.capacity {
            return Err(stream);
        }
        let id = self.next.get();
        self.next.set(id + 1);
        entries.insert(
            id,
            Entry {
                stream,
                connection,
                deadline,
            },
        );
        Ok(id)
    }

    /// The stream under `id`, removed: `None` once anything took it.
    pub fn take(&self, id: u64) -> Option<S> {
        self.entries
            .borrow_mut()
            .remove(&id)
            .map(|entry| entry.stream)
    }

    /// Every stream of `connection`, removed: the connection ended, so they
    /// can carry nothing.
    pub fn purge(&self, connection: u64) -> Vec<S> {
        self.drain_where(|entry| entry.connection == connection)
    }

    /// Every stream whose deadline is at or before `now`, removed.
    pub fn sweep(&self, now: Instant) -> Vec<S> {
        self.drain_where(|entry| entry.deadline <= now)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    fn drain_where<F>(&self, mut matches: F) -> Vec<S>
    where
        F: FnMut(&Entry<S>) -> bool,
    {
        let mut entries = self.entries.borrow_mut();
        let ids: Vec<u64> = entries
            .iter()
            .filter(|(_, entry)| matches(entry))
            .map(|(id, _)| *id)
            .collect();
        ids.into_iter()
            .filter_map(|id| entries.remove(&id).map(|entry| entry.stream))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    thread_local! {
        static BASE: Instant = Instant::now();
    }

    fn at(secs: u64) -> Instant {
        BASE.with(|base| *base + Duration::from_secs(secs))
    }

    #[test]
    fn a_stream_is_taken_at_most_once() {
        let table = ReplyTable::new(4);
        let id = table.issue("a", 1, at(30)).unwrap();
        assert_eq!(table.take(id), Some("a"));
        assert_eq!(table.take(id), None);
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn ids_are_never_reused() {
        let table = ReplyTable::new(4);
        let first = table.issue("a", 1, at(30)).unwrap();
        table.take(first);
        let second = table.issue("b", 1, at(30)).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            table.take(first),
            None,
            "a late take of the old id finds nothing"
        );
        assert_eq!(table.take(second), Some("b"));
    }

    #[test]
    fn a_full_table_hands_the_stream_back() {
        let table = ReplyTable::new(1);
        table.issue("a", 1, at(30)).unwrap();
        assert_eq!(table.issue("b", 1, at(30)), Err("b"));
    }

    #[test]
    fn purge_takes_only_the_ended_connection() {
        let table = ReplyTable::new(4);
        let kept = table.issue("kept", 1, at(30)).unwrap();
        table.issue("gone", 2, at(30)).unwrap();
        assert_eq!(table.purge(2), vec!["gone"]);
        assert_eq!(table.take(kept), Some("kept"));
    }

    #[test]
    fn sweep_takes_only_what_is_due() {
        let table = ReplyTable::new(4);
        let late = table.issue("late", 1, at(60)).unwrap();
        table.issue("due", 1, at(10)).unwrap();
        assert_eq!(table.sweep(at(10)), vec!["due"]);
        assert_eq!(table.take(late), Some("late"));
    }
}
