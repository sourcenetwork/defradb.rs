//! Iteration over a regolith transaction or snapshot.
//!
//! Nothing here ever holds the range. A read-only scan is a cursor that
//! holds one entry, and a scan over a writing transaction is one
//! [`regolith::TxnCursor`] read a bounded page at a time. Values arrive as
//! [`regolith::DbSlice`] and are handed on as [`Bytes`] without a copy.
//!
//! A `TxnCursor` holds no borrow of its transaction, so one cursor serves
//! the whole scan: it merges the transaction's buffered writes over the
//! snapshot and folds them again only when the transaction wrote since the
//! last page. An unvalidated scan reads pages bounded by bytes; a validated
//! one reads one entry per call, so the range it validates at commit is
//! exactly the range the caller consumed.

use std::collections::VecDeque;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;

use super::handle::Handle;
use super::worker_io::wait_out;
use crate::corekv::{Error, IterOptions, Iterator, KvPair, Result};

/// Bytes of keys and values a page may hold before it stops filling.
///
/// Bounding by bytes, not entries: an entry count says nothing about
/// memory when values vary from a few bytes to megabytes, and this runs
/// on devices where the difference is the whole budget.
const PAGE_BYTES: usize = 256 * 1024;

/// About how many entries a page holds, so a range of tiny values does not
/// turn one page into a very long list: each page asks for this many
/// entries at the average size seen so far, never more than [`PAGE_BYTES`].
///
/// Visible to the backend conformance suite, which sizes its ranges around a
/// page boundary so a scan that spans one is actually exercised.
pub(crate) const PAGE_ENTRIES: usize = 1024;

/// The size the first page assumes an entry has, before any was seen.
const FIRST_ENTRY_BYTES: usize = 16;

/// Where a walk begins.
#[derive(Clone)]
enum Anchor {
    /// The natural start of the range for the direction being walked: the
    /// lower bound going forward, the upper bound going back.
    RangeStart,
    /// An explicit key, from `seek`, included when it exists.
    At(Vec<u8>),
}

impl Anchor {
    /// The key to seek to.
    fn seek_target(&self) -> Option<&[u8]> {
        match self {
            Anchor::RangeStart => None,
            Anchor::At(key) => Some(key),
        }
    }
}

/// The highest key a backward walk may stand on.
enum Ceiling {
    /// An explicit seek target, included when it exists.
    At(Vec<u8>),
    /// Strictly below this key, which is the range's exclusive end.
    Below(Vec<u8>),
    /// The last key there is.
    Last,
}

/// The source being drained.
///
/// The cursor variant is much the larger, and it is not boxed: there is one
/// `Source` per iterator and the iterator is already behind a `Box<dyn
/// Iterator>`, so boxing again would add an allocation to every scan to shrink
/// something already on the heap exactly once.
#[allow(clippy::large_enum_variant)]
enum Source {
    /// A positioned cursor, owned and `'static`, holding one entry rather
    /// than the range.
    Cursor(regolith::Entries<regolith::OwnedSnapshotIter>),
    /// One cursor over the transaction the `Arc` keeps alive, for the whole
    /// scan, and the page it last read.
    Txn {
        cursor: regolith::TxnCursor,
        page: VecDeque<(Vec<u8>, regolith::DbSlice)>,
        /// The cursor reported the end of its range.
        done: bool,
        /// Entries and bytes read so far, which size the next page.
        entries: usize,
        bytes: usize,
    },
    /// The anchor fell outside the range, so there is nothing to walk.
    Empty,
}

/// A streaming scan over one key range.
pub(crate) struct RegolithIterator {
    source: Source,
    handle: Arc<Handle>,
    start: Option<Vec<u8>>,
    end: Option<Vec<u8>>,
    /// Only keys under this prefix are returned. The range already
    /// excludes the rest, except for a prefix with no expressible
    /// successor, which is a prefix of all `0xff` bytes.
    prefix: Option<Vec<u8>>,
    /// The range is exactly the prefix's: it starts at the prefix and ends
    /// where the prefix does.
    prefix_range: bool,
    reverse: bool,
    keys_only: bool,
    validated: bool,
    /// The parts a scan's decision used; see [`IterOptions::with_parts`].
    parts: Option<Box<[u32]>>,
    closed: bool,
    /// An entry `seek` pulled off to answer whether one exists, handed
    /// back by the next `next` rather than skipped.
    peeked: Option<KvPair>,
}

impl RegolithIterator {
    pub(crate) fn open(handle: Arc<Handle>, opts: &IterOptions) -> Result<Self> {
        let (start, end) = range_bounds(opts);
        let prefix_range = opts.prefix().is_some_and(|prefix| {
            start.as_deref() == Some(prefix) && end == prefix_successor(prefix)
        });
        let mut iter = Self {
            source: Source::Empty,
            handle,
            start,
            end,
            prefix: opts.prefix().map(<[u8]>::to_vec),
            prefix_range,
            reverse: opts.reverse(),
            keys_only: opts.keys_only(),
            validated: opts.validated(),
            parts: opts.parts().map(Box::from),
            closed: false,
            peeked: None,
        };
        iter.position(Anchor::RangeStart);
        Ok(iter)
    }

    /// Point the scan at `anchor` and start again from there.
    fn position(&mut self, anchor: Anchor) {
        self.source = match self.handle.as_ref() {
            Handle::ReadOnly(snapshot) => {
                let mut cursor = snapshot.owned_iter();
                if self.reverse {
                    match self.ceiling(&anchor) {
                        Ceiling::At(key) => cursor.seek_for_prev(&key),
                        Ceiling::Below(end) => {
                            cursor.seek_for_prev(&end);
                            if cursor.valid() && cursor.key() == Some(end.as_slice()) {
                                cursor.prev();
                            }
                        }
                        Ceiling::Last => cursor.seek_to_last(),
                    }
                } else {
                    // The cursor skips entries this snapshot cannot see
                    // before it reports a key; only a bound it holds stops
                    // that skip at the end of the range. A walk of exactly
                    // the prefix seeks it as one, which holds the same bound
                    // and lets regolith pass over the tables whose prefix
                    // filter lacks it (`prefix`).
                    match (self.floor(&anchor), self.end.as_deref()) {
                        (Some(floor), _) if self.walks_prefix_from(&floor) => {
                            cursor.seek_prefix(&floor)
                        }
                        (floor, Some(end)) => cursor.seek_bounded(&floor.unwrap_or_default(), end),
                        (Some(floor), None) => cursor.seek(&floor),
                        (None, None) => cursor.seek_to_first(),
                    }
                }
                // A cursor seeked past the range is invalid, and `Entries`
                // reads an invalid cursor as one that was never positioned:
                // it would restart at the far end of the store and hand back
                // keys the caller seeked away from. So it must not see one.
                // A seek that stopped on a block the worker's cache does not
                // hold, or on an error, is no end: `Entries` hands either out
                // and resumes the seek after a `WouldBlock`.
                if !cursor.valid() && cursor.status().is_ok() {
                    Source::Empty
                } else if self.reverse {
                    Source::Cursor(cursor.entries_rev())
                } else {
                    Source::Cursor(cursor.entries())
                }
            }
            Handle::Writable(txn) => {
                let (start, end, direction) = if self.reverse {
                    let end = match self.ceiling(&anchor) {
                        Ceiling::At(key) => Some(above(&key)),
                        Ceiling::Below(end) => Some(end),
                        Ceiling::Last => None,
                    };
                    (self.start.clone(), end, regolith::ScanDirection::Reverse)
                } else {
                    (
                        self.floor(&anchor),
                        self.end.clone(),
                        regolith::ScanDirection::Forward,
                    )
                };
                let check = match (&self.parts, self.validated) {
                    (_, true) => regolith::ScanCheck::Range,
                    (Some(parts), false) => regolith::ScanCheck::Parts(parts.clone()),
                    (None, false) => regolith::ScanCheck::Stretch,
                };
                Source::Txn {
                    cursor: txn.cursor(start.as_deref(), end.as_deref(), direction, check),
                    page: VecDeque::new(),
                    done: false,
                    entries: 0,
                    bytes: 0,
                }
            }
        };
    }

    /// The lowest key a forward walk may stand on: the range's start, raised
    /// to an explicit seek target. Seeking below the start lands on the start.
    fn floor(&self, anchor: &Anchor) -> Option<Vec<u8>> {
        match anchor.seek_target() {
            Some(key) => Some(match &self.start {
                Some(start) if start.as_slice() > key => start.clone(),
                _ => key.to_vec(),
            }),
            None => self.start.clone(),
        }
    }

    /// Whether a forward walk from `floor` covers exactly the prefix: the
    /// range is the prefix's, and the walk starts at its first key.
    fn walks_prefix_from(&self, floor: &[u8]) -> bool {
        self.prefix_range && self.prefix.as_deref() == Some(floor)
    }

    /// The highest key a backward walk may stand on. Seeking at or past the
    /// exclusive end lands on the highest key the range admits.
    fn ceiling(&self, anchor: &Anchor) -> Ceiling {
        match (anchor.seek_target(), self.end.as_deref()) {
            (Some(key), Some(end)) if key >= end => Ceiling::Below(end.to_vec()),
            (Some(key), _) => Ceiling::At(key.to_vec()),
            (None, Some(end)) => Ceiling::Below(end.to_vec()),
            (None, None) => Ceiling::Last,
        }
    }

    async fn step(&mut self) -> Result<Option<KvPair>> {
        let (key, value) = match &mut self.source {
            Source::Empty => return Ok(None),
            Source::Cursor(entries) => loop {
                match entries.next() {
                    Some(Ok(entry)) => break entry,
                    // The worker's queue reads the block; the next item
                    // resumes the walk where it stopped.
                    Some(Err(regolith::Error::WouldBlock(would_block))) => {
                        wait_out(would_block)
                            .await
                            .map_err(|error| Error::Backend(error.to_string()))?;
                    }
                    Some(Err(error)) => return Err(Error::Backend(error.to_string())),
                    None => return Ok(None),
                }
            },
            Source::Txn {
                cursor,
                page,
                done,
                entries,
                bytes,
            } => {
                if page.is_empty() && !*done {
                    let Handle::Writable(txn) = self.handle.as_ref() else {
                        return Err(Error::ReadOnlyTxn);
                    };
                    // A validated scan reads one entry per call, so the range
                    // it validates is exactly what the caller consumed.
                    let request = if self.validated {
                        0
                    } else {
                        page_request(*entries, *bytes)
                    };
                    let next = regolith::ready(|| cursor.next_page(txn, request))
                        .await
                        .map_err(super::transaction::map_txn_error)?;
                    *done = next.done;
                    *entries += next.entries.len();
                    *bytes += next
                        .entries
                        .iter()
                        .map(|(key, value)| key.len() + value.len())
                        .sum::<usize>();
                    page.extend(next.entries);
                }
                match page.pop_front() {
                    Some(entry) => entry,
                    None => return Ok(None),
                }
            }
        };
        let value = if self.keys_only {
            Bytes::new()
        } else {
            Bytes::from_owner(value)
        };
        Ok(Some(KvPair { key, value }))
    }

    /// The next entry inside the range, or `None` at its end.
    async fn next_in_range(&mut self) -> Result<Option<KvPair>> {
        while let Some(pair) = self.step().await? {
            // A reverse walk runs down to `start`, which is inclusive, so it
            // stops at the first key below it; a forward walk runs up to
            // `end`, which is not.
            if self.reverse {
                if let Some(start) = &self.start {
                    if pair.key.as_slice() < start.as_slice() {
                        return Ok(None);
                    }
                }
            } else if let Some(end) = &self.end {
                if pair.key.as_slice() >= end.as_slice() {
                    return Ok(None);
                }
            }
            if let Some(prefix) = &self.prefix {
                if !pair.key.starts_with(prefix) {
                    continue;
                }
            }
            return Ok(Some(pair));
        }
        Ok(None)
    }
}

/// The bytes the next page of a scan asks for: [`PAGE_ENTRIES`] entries at
/// the average size of the `entries` read so far, which held `bytes` of keys
/// and values, never more than [`PAGE_BYTES`].
fn page_request(entries: usize, bytes: usize) -> usize {
    let average = bytes
        .checked_div(entries)
        .unwrap_or(FIRST_ENTRY_BYTES)
        .max(1);
    average.saturating_mul(PAGE_ENTRIES).min(PAGE_BYTES)
}

/// Lower and upper bound for the scan, folding a prefix into the range so
/// the engine skips what it can instead of the iterator filtering it.
fn range_bounds(opts: &IterOptions) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let mut start = opts.start().map(<[u8]>::to_vec);
    let mut end = opts.end().map(<[u8]>::to_vec);
    if let Some(prefix) = opts.prefix() {
        if start.as_deref().is_none_or(|s| s < prefix) {
            start = Some(prefix.to_vec());
        }
        // The successor of a prefix is it with the last non-`0xff` byte
        // raised. An all-`0xff` prefix has none, and the scan then runs to
        // the end of the keyspace with the prefix check doing the work.
        if let Some(successor) = prefix_successor(prefix) {
            if end.as_deref().is_none_or(|e| e > successor.as_slice()) {
                end = Some(successor);
            }
        }
    }
    (start, end)
}

/// The smallest key strictly greater than `key`.
///
/// Appending a zero byte works for any key, where raising the last byte does
/// not: `key` followed by anything sorts above `key`, and nothing sorts
/// between the two. Used to turn an inclusive seek target into the exclusive
/// upper bound a backward scan takes.
fn above(key: &[u8]) -> Vec<u8> {
    let mut next = Vec::with_capacity(key.len() + 1);
    next.extend_from_slice(key);
    next.push(0);
    next
}

fn prefix_successor(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut successor = prefix.to_vec();
    while let Some(last) = successor.pop() {
        if last != 0xff {
            successor.push(last + 1);
            return Some(successor);
        }
    }
    None
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Iterator for RegolithIterator {
    async fn next(&mut self) -> Result<Option<KvPair>> {
        if self.closed {
            return Err(Error::Iterator("iterator is closed".to_string()));
        }
        if let Some(peeked) = self.peeked.take() {
            return Ok(Some(peeked));
        }
        self.next_in_range().await
    }

    async fn close(&mut self) -> Result<()> {
        self.closed = true;
        self.peeked = None;
        Ok(())
    }

    async fn seek(&mut self, key: &[u8]) -> Result<bool> {
        if self.closed {
            return Err(Error::Iterator("seek on a closed iterator".to_string()));
        }
        self.position(Anchor::At(key.to_vec()));
        // Pull the entry the seek landed on so the caller learns whether
        // one exists; `next` hands back that same entry.
        self.peeked = self.next_in_range().await?;
        Ok(self.peeked.is_some())
    }

    async fn reset(&mut self) -> Result<()> {
        if self.closed {
            return Err(Error::Iterator("reset on a closed iterator".to_string()));
        }
        self.peeked = None;
        self.position(Anchor::RangeStart);
        Ok(())
    }

    fn is_valid(&self) -> bool {
        !self.closed
    }
}

impl crate::corekv::private::Sealed for RegolithIterator {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_asks_for_about_page_entries_and_never_more_than_page_bytes() {
        assert_eq!(page_request(0, 0), FIRST_ENTRY_BYTES * PAGE_ENTRIES);
        assert_eq!(page_request(10, 100), 10 * PAGE_ENTRIES);
        assert_eq!(page_request(1, 1 << 30), PAGE_BYTES);
        assert_eq!(page_request(1000, 0), PAGE_ENTRIES);
    }
}

#[cfg(test)]
mod scan_tests {
    use super::*;
    use crate::backends::RegolithStore;
    use crate::corekv::IterOptions;
    use crate::corekv::{Store, Txn};
    use regolith::{PerfContext, PerfLevel};

    async fn scan_lookups(txn: &dyn Txn, rows: usize) -> u64 {
        PerfContext::reset();
        let mut iter = txn
            .iterator(IterOptions::new().with_prefix(b"row/".to_vec()))
            .await
            .unwrap();
        let mut count = 0;
        while iter.next().await.unwrap().is_some() {
            count += 1;
        }
        iter.close().await.unwrap();
        assert_eq!(count, rows);
        PerfContext::capture().block_cache_lookup_count
    }

    /// A scan of a writing transaction walks the tables once, as a snapshot
    /// scan does, rather than seeking them again for every page.
    #[test]
    fn an_unvalidated_scan_of_a_writing_transaction_reads_each_block_once() {
        workers::block_on(async {
            let directory = tempfile::tempdir().unwrap();
            let store = RegolithStore::open(directory.path()).unwrap();
            let rows = 3 * PAGE_ENTRIES + 7;
            let mut seed = store.new_txn(false).await.unwrap();
            for row in 0..rows {
                seed.set(format!("row/{row:06}").as_bytes(), &[7; 64])
                    .await
                    .unwrap();
            }
            seed.commit().await.unwrap();
            store.engine().db().flush().unwrap();

            let level = PerfContext::set_level(PerfLevel::EnableCount);
            let snapshot = store.new_txn(true).await.unwrap();
            let snapshot_lookups = scan_lookups(snapshot.as_ref(), rows).await;
            snapshot.discard();
            let mut writing = store.new_txn(false).await.unwrap();
            writing.set(b"other", b"own").await.unwrap();
            let writing_lookups = scan_lookups(writing.as_ref(), rows).await;
            writing.discard();
            PerfContext::set_level(level);

            assert!(snapshot_lookups > 0);
            assert_eq!(writing_lookups, snapshot_lookups);
            store.close().await.unwrap();
        })
        .unwrap()
    }
}
