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

use std::sync::Arc;

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

/// Pending log entries in log order: each one's position, vector and code,
/// `code_len` bytes each, in one run.
#[derive(Debug, Default)]
pub struct Segment {
    positions: Vec<u64>,
    ids: Vec<NodeId>,
    codes: Vec<u8>,
}

const ENTRY_OVERHEAD: usize = size_of::<u64>() + size_of::<NodeId>();

impl Segment {
    pub fn push(&mut self, position: u64, id: NodeId, code: &[u8]) {
        self.positions.push(position);
        self.ids.push(id);
        self.codes.extend_from_slice(code);
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn bytes(&self) -> usize {
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

    /// This copy plus `added`, caught up to `position`.
    pub fn caught_up(&self, position: u64, added: Segment) -> Self {
        let mut segments = self.segments.clone();
        let bytes = self.bytes + added.bytes();
        if !added.is_empty() {
            segments.push(Arc::new(added));
        }
        if segments.len() > MAX_SEGMENTS {
            let mut folded = Segment::default();
            for segment in &segments {
                folded.positions.extend_from_slice(&segment.positions);
                folded.ids.extend_from_slice(&segment.ids);
                folded.codes.extend_from_slice(&segment.codes);
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

    /// How many entries it holds.
    pub fn len(&self) -> usize {
        self.segments.iter().map(|segment| segment.ids.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
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

    /// Every entry logged at or before `head`, oldest first, with its code. A
    /// vector inserted more than once appears once per insert.
    pub fn for_each_through<F>(&self, head: u64, code_len: usize, mut visit: F)
    where
        F: FnMut(NodeId, &[u8]),
    {
        for segment in &self.segments {
            let entries = segment
                .positions
                .iter()
                .zip(&segment.ids)
                .zip(segment.codes.chunks_exact(code_len));
            for ((position, id), code) in entries {
                if *position > head {
                    return;
                }
                visit(*id, code);
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
