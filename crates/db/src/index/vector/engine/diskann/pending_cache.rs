//! An in-memory copy of a DiskANN index's pending log, shared by every search
//! on the node, so a search does not scan the log from storage.
//!
//! Unlike resident codes, which only order a walk, a pending vector missing
//! from the copy is one the search can never return. So the copy is used only
//! where it provably holds every vector the search's snapshot has pending
//! (`proofs/tla/PendingCache.tla`, `proofs/lean/PendingCache`): it is tagged
//! with the graph generation it belongs to and the head it has caught up to.
//! Within a generation the log only grows and catching up only ever adds, so
//! a copy holds everything an older snapshot of its generation has. Each
//! entry keeps its log position, so a search older than the copy visits only
//! the entries its own head reaches and never scores a vector by a code from
//! its future.
//!
//! An update logs a vector again, so each entry also carries the position of
//! the vector's next entry: its superseded marker. A search at head `h`
//! skips an entry whose marker is at or before `h`, and so visits exactly
//! one entry per vector, the latest it can see, without deduplicating. The
//! marker is a fact about the log, the same whichever copy sets it, so two
//! searches catching up from one copy may both set it.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use rapidhash::{HashSetExt, RapidHashMap, RapidHashSet};
use regolith::sync::Atom;

use crate::index::vector::store::NodeId;

/// The most a copy holds. A pending list bigger than this is scanned, as it
/// would be without a copy.
pub const MAX_PENDING_CACHE_BYTES: usize = 64 << 20;

/// Past this many catch-ups the segments are folded into one, so a search
/// walks a few long runs rather than many short ones.
const MAX_SEGMENTS: usize = 32;

/// Which snapshots a copy is valid for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    /// The graph generation it belongs to: a merge starts a new one.
    pub generation: u64,
    /// The newest log position it has caught up to.
    pub position: u64,
}

const NONE: u32 = u32::MAX;

/// Pending log entries in log order, from position `base` with no gaps: the
/// entry at index `i` was logged at `base + i`. Each has its vector, its code
/// (`code_len` bytes each, in one run), and its superseded marker: the
/// distance to the vector's next entry, [`NONE`] while none is known.
#[derive(Debug, Default)]
pub struct Segment {
    base: u64,
    ids: Vec<NodeId>,
    next: Vec<AtomicU32>,
    codes: Vec<u8>,
    /// Each vector's last entry in this segment, so a later catch-up finds
    /// the entry it supersedes.
    last: RapidHashMap<NodeId, u32>,
    /// Whether any marker is set, so a segment of vectors logged once each
    /// is walked without reading them.
    marked: AtomicBool,
    /// An entry broke the run (a gap, or past `u32` entries): the segment
    /// cannot be kept.
    broken: bool,
}

/// What an entry costs besides its code: its id and marker, and a slot in
/// `last` (key, index, control byte, at hashbrown's 7/8 load).
pub const ENTRY_OVERHEAD: usize =
    size_of::<NodeId>() + size_of::<AtomicU32>() + (size_of::<(NodeId, u32)>() + 1) * 8 / 7;

impl Segment {
    pub fn push(&mut self, position: u64, id: NodeId, code: &[u8]) {
        if self.ids.is_empty() {
            self.base = position;
        }
        let index = self.ids.len() as u64;
        if self.broken || position != self.base + index || index >= u64::from(NONE) {
            self.broken = true;
            return;
        }
        let index = index as u32;
        if let Some(previous) = self.last.insert(id, index) {
            *self.next[previous as usize].get_mut() = index - previous;
            *self.marked.get_mut() = true;
        }
        self.ids.push(id);
        self.next.push(AtomicU32::new(NONE));
        self.codes.extend_from_slice(code);
    }

    /// The position just past the last entry.
    fn end(&self) -> u64 {
        self.base + self.ids.len() as u64
    }

    /// Marks `id`'s last entry here as superseded at `position`, if this
    /// segment has one.
    fn supersede(&self, id: NodeId, position: u64) -> bool {
        let Some(&index) = self.last.get(&id) else {
            return false;
        };
        let distance = position - (self.base + u64::from(index));
        if let Ok(distance) = u32::try_from(distance) {
            self.next[index as usize].fetch_min(distance, Ordering::Relaxed);
            self.marked.store(true, Ordering::Relaxed);
        }
        true
    }

    fn code_len(&self) -> usize {
        self.codes.len() / self.ids.len().max(1)
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn bytes(&self) -> usize {
        if self.broken {
            return usize::MAX;
        }
        self.ids.len() * ENTRY_OVERHEAD + self.codes.len()
    }
}

/// One published copy: never changed once published.
#[derive(Debug, Default)]
pub struct Copy {
    tag: Option<Tag>,
    segments: Vec<Arc<Segment>>,
    bytes: usize,
    /// The pending list at `tag` was past [`MAX_PENDING_CACHE_BYTES`]: nothing
    /// is held, and searches at that generation scan without copying.
    oversized: bool,
}

impl Copy {
    /// A copy loaded from a scan of the pending list at `tag`.
    pub fn loaded(tag: Tag, segment: Segment) -> Self {
        let bytes = segment.bytes();
        Self {
            tag: Some(tag),
            segments: vec![Arc::new(segment)],
            bytes,
            oversized: false,
        }
    }

    pub fn tag(&self) -> Option<Tag> {
        self.tag
    }

    /// This copy plus `added`, caught up to `position`. Each vector `added`
    /// logs again has its last entry here marked superseded by its first in
    /// `added`, before the result can be published.
    pub fn caught_up(&self, position: u64, added: Segment) -> Self {
        let mut marked = RapidHashSet::with_capacity(added.last.len());
        for (at, id) in (added.base..).zip(&added.ids) {
            if marked.insert(*id) {
                self.segments
                    .iter()
                    .rev()
                    .any(|segment| segment.supersede(*id, at));
            }
        }
        let mut segments = self.segments.clone();
        let bytes = self.bytes.saturating_add(added.bytes());
        if !added.is_empty() {
            segments.push(Arc::new(added));
        }
        if segments.len() > MAX_SEGMENTS {
            let mut folded = Segment::default();
            for segment in &segments {
                let codes = segment.codes.chunks_exact(segment.code_len().max(1));
                for ((at, id), code) in (segment.base..).zip(&segment.ids).zip(codes) {
                    folded.push(at, *id, code);
                }
            }
            segments = vec![Arc::new(folded)];
        }
        Self {
            tag: self.tag.map(|tag| Tag { position, ..tag }),
            segments,
            bytes,
            oversized: false,
        }
    }

    pub fn is_oversized(&self) -> bool {
        self.oversized
    }

    /// Whether `entries` more inserts of `code_len`-byte codes still fit
    /// under [`MAX_PENDING_CACHE_BYTES`].
    pub fn can_hold(&self, entries: u64, code_len: usize) -> bool {
        let per_entry = (ENTRY_OVERHEAD + code_len) as u64;
        entries
            .checked_mul(per_entry)
            .and_then(|more| more.checked_add(self.bytes as u64))
            .is_some_and(|total| total <= MAX_PENDING_CACHE_BYTES as u64)
    }

    pub fn fits(&self) -> bool {
        self.bytes <= MAX_PENDING_CACHE_BYTES
    }

    /// Each vector's latest entry logged at or before `head`, with its code:
    /// an entry whose next one is also at or before `head` is skipped, so
    /// every vector is visited once.
    pub fn for_each_latest_through<F>(&self, head: u64, code_len: usize, mut visit: F)
    where
        F: FnMut(NodeId, &[u8]),
    {
        for segment in &self.segments {
            if head < segment.base {
                return;
            }
            let reach = head - segment.base;
            let visible = (segment.end().min(head + 1) - segment.base) as usize;
            // Relaxed: a marker this search needs was set before the copy
            // holding the superseding entry was published or built.
            if !segment.marked.load(Ordering::Relaxed) {
                let codes = segment.codes.chunks_exact(code_len);
                for (id, code) in segment.ids.iter().zip(codes).take(visible) {
                    visit(*id, code);
                }
                continue;
            }
            let entries = segment
                .ids
                .iter()
                .zip(&segment.next)
                .zip(segment.codes.chunks_exact(code_len))
                .take(visible);
            for (index, ((id, next), code)) in (0u64..).zip(entries) {
                let next = next.load(Ordering::Relaxed);
                if next == NONE || index + u64::from(next) > reach {
                    visit(*id, code);
                }
            }
        }
    }
}

/// The copy one index's searches share.
#[derive(Debug, Default)]
pub struct PendingCache {
    current: Atom<Arc<Copy>>,
}

impl PendingCache {
    pub fn load(&self) -> Arc<Copy> {
        self.current.load().clone()
    }

    /// Publishes `next` if it is newer than the current copy: a later
    /// generation, or the same one caught up further. A copy past
    /// [`MAX_PENDING_CACHE_BYTES`] is published as an oversized marker instead,
    /// so searches at its generation scan without building a copy to discard.
    pub fn publish(&self, next: Arc<Copy>) {
        let Some(tag) = next.tag else {
            return;
        };
        let next = if next.fits() {
            next
        } else {
            Arc::new(Copy {
                tag: Some(tag),
                oversized: true,
                ..Copy::default()
            })
        };
        self.current.rcu(|current| match current.tag {
            Some(held)
                if held.generation > tag.generation
                    || (held.generation == tag.generation && held.position >= tag.position) =>
            {
                current.clone()
            }
            _ => next.clone(),
        });
    }
}
