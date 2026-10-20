//! An in-memory copy of a DiskANN index's pending list, shared by every
//! search on the node, so a search does not scan the list from storage.
//!
//! Unlike resident codes, which only order a walk, a pending vector missing
//! from the copy is one the search can never return. So the copy is used only
//! where it provably holds every vector the search's snapshot has pending
//! (`proofs/tla/PendingCache.tla`, `proofs/lean/PendingCache`): it is tagged
//! with the graph generation it belongs to, the log head it was loaded at, and
//! the head it has caught up to, and catching up only ever adds. Extra vectors
//! cost a search nothing but a dropped candidate, since ranking reads each
//! candidate's vector in the search's own snapshot.

use std::sync::Arc;

use kovan::Atom;

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
    /// The log head of the scan it was loaded from. A snapshot older than
    /// that may hold a vector the scan saw deleted.
    pub loaded_at: u64,
    /// The newest log position it has caught up to.
    pub position: u64,
}

/// Pending vectors and their codes, `code_len` bytes each, in one run.
#[derive(Debug, Default)]
pub struct Segment {
    ids: Vec<NodeId>,
    codes: Vec<u8>,
}

impl Segment {
    pub fn push(&mut self, id: NodeId, code: &[u8]) {
        self.ids.push(id);
        self.codes.extend_from_slice(code);
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    fn bytes(&self) -> usize {
        self.ids.len() * size_of::<NodeId>() + self.codes.len()
    }
}

/// One published copy: never changed once published.
#[derive(Debug, Default)]
pub struct Copy {
    tag: Option<Tag>,
    segments: Vec<Arc<Segment>>,
    bytes: usize,
}

impl Copy {
    /// A copy loaded from a scan of the pending list at `tag`.
    pub fn loaded(tag: Tag, segment: Segment) -> Self {
        let bytes = segment.bytes();
        Self {
            tag: Some(tag),
            segments: vec![Arc::new(segment)],
            bytes,
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
                folded.ids.extend_from_slice(&segment.ids);
                folded.codes.extend_from_slice(&segment.codes);
            }
            segments = vec![Arc::new(folded)];
        }
        Self {
            tag: self.tag.map(|tag| Tag { position, ..tag }),
            segments,
            bytes,
        }
    }

    pub fn fits(&self) -> bool {
        self.bytes <= MAX_PENDING_CACHE_BYTES
    }

    /// Every vector held, with its code. A vector inserted more than once
    /// appears once per insert.
    pub fn for_each<F>(&self, code_len: usize, mut visit: F)
    where
        F: FnMut(NodeId, &[u8]),
    {
        for segment in &self.segments {
            for (id, code) in segment.ids.iter().zip(segment.codes.chunks_exact(code_len)) {
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
    /// [`MAX_PENDING_CACHE_BYTES`] drops the cache instead, so searches scan.
    pub fn publish(&self, next: Arc<Copy>) {
        let Some(tag) = next.tag else {
            return;
        };
        let next = if next.fits() { next } else { Arc::default() };
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
